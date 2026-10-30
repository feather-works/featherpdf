//! Aba Medir: distância, perímetro, área e ângulo sobre a página, como a
//! ferramenta de medição do Acrobat. A escala é uma proporção (1:100 em m) ou
//! calibrada traçando uma medida conhecida. As medidas ficam no painel Medidas
//! enquanto o documento está aberto, e "Gravar no PDF" as desenha na página.
//! Contar põe marcas numeradas; Nuvem desenha a nuvem de revisão na página.

use feather_draw::{Caminho, Comando, Ponto};
use featherpdf_core::{comprimento, medir, numero, DesenhoNovo, Escala, TipoMedida, Unidade};
use feather_ui::dialogo::{self as dlg, Dialogo};
use feather_ui::{icones as ic, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::{self, Cursor};
use crate::compat::{event, canvas::{self, Frame, Geometry, Path, Stroke, Text}};
use crate::edicao::na_tela;
use crate::{App, Ferramenta, Msg, Painel};

/// Ferramenta da aba Medir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Medidor { Medir(TipoMedida), Calibrar, Nuvem }

impl Medidor {
    fn tipo(self) -> TipoMedida {
        match self { Medidor::Medir(t) => t, Medidor::Calibrar => TipoMedida::Distancia, Medidor::Nuvem => TipoMedida::Area }
    }
}

#[derive(Debug, Clone)]
pub struct Medida { pub pagina: u32, pub tipo: TipoMedida, pub pontos: Vec<(f32, f32)> }

/// Diálogo da escala: `calibrar` traz o comprimento (pt) do traço medido.
#[derive(Debug, Clone)]
pub struct DialogoEscala { pub unidade: Unidade, pub texto: String, pub calibrar: Option<f32> }

#[derive(Default)]
pub struct Medidas {
    pub escala: Escala,
    pub feitas: Vec<Medida>,
    /// Medida em andamento: página e pontos já clicados.
    pub atual: Option<(u32, Vec<(f32, f32)>)>,
    pub dialogo: Option<DialogoEscala>,
    /// O imã (grudar nos cantos do desenho) vem ligado; isto o desliga.
    pub sem_ima: bool,
    /// Vértices de cada página na tela, guardados pelo zoom e pela versão do documento.
    ima: std::cell::RefCell<std::collections::HashMap<u32, (f32, u64, std::sync::Arc<Vec<Point>>)>>,
}

#[derive(Debug, Clone)]
pub enum Med {
    Ferramenta(Medidor),
    /// Clique na página, em pixels a partir do canto dela.
    Ponto { pagina: u32, p: Point },
    Terminar,
    /// Clique no primeiro ponto de área ou perímetro: fecha a forma.
    Fechar,
    Ima(bool),
    Cancelar,
    DesfazerPonto,
    Apagar(usize),
    Limpar,
    Copiar,
    /// Desenha as medidas na página (linhas e valores) e tira da lista.
    Gravar,
    AbrirEscala,
    Unidade(Unidade),
    Texto(String),
    AplicarEscala,
    FecharEscala,
}

const AZUL: Color = Color { r: 0.11, g: 0.45, b: 0.84, a: 1.0 };
const LARANJA: Color = Color { r: 0.91, g: 0.35, b: 0.05, a: 1.0 };
/// Distância (px de tela) em que o imã gruda e o clique no primeiro ponto fecha a forma.
const RAIO_IMA: f32 = 10.0;
const VERDE: Color = Color { r: 0.16, g: 0.60, b: 0.28, a: 1.0 };
/// Dois cliques mais perto que isso (ms e px) são um clique duplo.
const DUPLO_MS: u128 = 400;
const DUPLO_PX: f32 = 6.0;

fn id_escala() -> &'static str { "featherpdf.escala" }

/// Camada da página com as medidas desenhadas e, com a ferramenta ligada, os cliques.
struct Regua {
    pagina: u32,
    feitas: Vec<(TipoMedida, Vec<Point>, String)>,
    atual: Vec<Point>,
    tipo: Option<TipoMedida>,
    zoom: f32,
    escala: Escala,
    /// Vértices do desenho na tela (imã), se ligado.
    ima: Option<std::sync::Arc<Vec<Point>>>,
    /// Shift apertado: o traço anda de 45 em 45°.
    travar: bool,
}

impl Regua {
    /// O ponto que vale para o cursor em `p`: com Shift, no ângulo travado a partir do último ponto;
    /// senão, o vértice do desenho mais perto (se houver um a `RAIO_IMA`). O `bool` diz se o imã pegou.
    fn ajustar(&self, p: Point) -> (Point, bool) {
        if let (true, Some(&u)) = (self.travar, self.atual.last()) {
            let (dx, dy) = (p.x - u.x, p.y - u.y);
            let passo = std::f32::consts::FRAC_PI_4;
            let a = (dy.atan2(dx) / passo).round() * passo;
            let r = dx.hypot(dy);
            return (Point::new(u.x + r * a.cos(), u.y + r * a.sin()), false);
        }
        let perto = self.ima.as_ref().and_then(|v| v.iter().copied()
            .map(|q| (q, q.distance(p))).filter(|(_, d)| *d < RAIO_IMA).min_by(|a, b| a.1.total_cmp(&b.1)));
        perto.map_or((p, false), |(q, _)| (q, true))
    }

    /// Área ou perímetro com 3 pontos ou mais, e o cursor no primeiro: clicar fecha.
    fn fecha(&self, p: Point) -> bool {
        matches!(self.tipo, Some(TipoMedida::Area | TipoMedida::Perimetro)) && self.atual.len() >= 3 && p.distance(self.atual[0]) < RAIO_IMA
    }
}

#[derive(Default)]
struct EstadoRegua { cursor: Option<Point>, ultimo: Option<(std::time::Instant, Point)> }

fn rotulo(f: &mut Frame, p: Point, texto: &str, cor: Color) {
    let largura = texto.chars().count() as f32 * 6.6 + 10.0;
    let caixa = Rect::new(Point::new(p.x + 6.0, p.y - 20.0), Size::new(largura, 17.0));
    f.fill(&Path::rounded_rectangle(caixa.position(), caixa.size(), 3.0_f32.into()), Color::WHITE);
    f.stroke(&Path::rounded_rectangle(caixa.position(), caixa.size(), 3.0_f32.into()), Stroke::default().with_color(cor).with_width(1.0));
    f.fill_text(Text { content: texto.into(), position: Point::new(caixa.x + 5.0, caixa.y + 2.5), size: 12.0, color: cor, ..Text::default() });
}

fn desenhar(f: &mut Frame, tipo: TipoMedida, pts: &[Point], cor: Color, fechar: bool) {
    if pts.is_empty() { return; }
    if tipo == TipoMedida::Contagem {
        for (i, &p) in pts.iter().enumerate() {
            f.fill(&Path::circle(p, 8.0), cor);
            f.fill_text(Text { content: (i + 1).to_string(), position: p, size: 11.0, color: Color::WHITE,
                horizontal_alignment: Align::Center, vertical_alignment: Align::Center, ..Text::default() });
        }
        return;
    }
    let caminho = Path::new(|b| {
        b.move_to(pts[0]);
        for &p in &pts[1..] { b.line_to(p); }
        if fechar && tipo == TipoMedida::Area { b.close(); }
    });
    if tipo == TipoMedida::Area && pts.len() > 2 { f.fill(&caminho, Color { a: 0.15, ..cor }); }
    f.stroke(&caminho, Stroke::default().with_color(cor).with_width(1.8));
    for &p in pts { f.fill(&Path::circle(p, 3.0), cor); }
}

/// Onde pôr o texto: o vértice do ângulo, o meio da área ou o último ponto.
fn ponto_do_rotulo(tipo: TipoMedida, pts: &[Point]) -> Point {
    match tipo {
        TipoMedida::Angulo if pts.len() > 1 => pts[1],
        TipoMedida::Area if pts.len() > 2 => {
            let n = pts.len() as f32;
            Point::new(pts.iter().map(|p| p.x).sum::<f32>() / n, pts.iter().map(|p| p.y).sum::<f32>() / n)
        }
        _ => *pts.last().unwrap_or(&Point::ORIGIN),
    }
}

impl canvas::Program<Msg> for Regua {
    type State = EstadoRegua;

    fn update(&self, estado: &mut EstadoRegua, evento: canvas::Event, limites: Rect, cursor: Cursor) -> (event::Status, Option<Msg>) {
        if self.tipo.is_none() { return (event::Status::Ignored, None); }
        match evento {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let novo = cursor.position_in(limites);
                if novo == estado.cursor { return (event::Status::Ignored, None); }
                estado.cursor = novo;
                // Com o imã, o quadradinho segue o mouse mesmo antes do primeiro clique.
                (if self.atual.is_empty() && self.ima.is_none() { event::Status::Ignored } else { event::Status::Captured }, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cru) = cursor.position_in(limites) else { return (event::Status::Ignored, None) };
                let (p, _) = self.ajustar(cru);
                let agora = std::time::Instant::now();
                let duplo = estado.ultimo.is_some_and(|(t, q)| agora.duration_since(t).as_millis() < DUPLO_MS && q.distance(cru) < DUPLO_PX);
                estado.ultimo = Some((agora, cru));
                let msg = if self.fecha(p) { Med::Fechar } else if duplo { Med::Terminar } else { Med::Ponto { pagina: self.pagina, p } };
                (event::Status::Captured, Some(Msg::Medir(msg)))
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if !self.atual.is_empty() =>
                (event::Status::Captured, Some(Msg::Medir(Med::Terminar))),
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(&self, estado: &EstadoRegua, renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        for (tipo, pts, texto) in &self.feitas {
            desenhar(&mut f, *tipo, pts, AZUL, true);
            rotulo(&mut f, ponto_do_rotulo(*tipo, pts), texto, AZUL);
        }
        let ajustado = estado.cursor.filter(|_| self.tipo.is_some()).map(|c| self.ajustar(c));
        if let Some((c, true)) = ajustado {
            f.stroke(&Path::rectangle(Point::new(c.x - 5.0, c.y - 5.0), Size::new(10.0, 10.0)), Stroke::default().with_color(VERDE).with_width(2.0));
        }
        // Anel no primeiro ponto: clicar ali fecha a forma.
        if ajustado.is_some_and(|(c, _)| self.fecha(c)) {
            f.stroke(&Path::circle(self.atual[0], 8.0), Stroke::default().with_color(VERDE).with_width(2.0));
        }
        if let Some(tipo) = self.tipo.filter(|_| !self.atual.is_empty()) {
            let mut pts = self.atual.clone();
            if let Some((c, _)) = ajustado { pts.push(if self.fecha(c) { self.atual[0] } else { c }); }
            desenhar(&mut f, tipo, &pts, LARANJA, false);
            // Comprimento e área não mudam com o giro: a tela dividida pelo zoom basta.
            let pagina: Vec<(f32, f32)> = pts.iter().map(|p| (p.x / self.zoom, p.y / self.zoom)).collect();
            if pagina.len() >= 2 {
                rotulo(&mut f, ponto_do_rotulo(tipo, &pts), &medir(tipo, &pagina, &self.escala).1, LARANJA);
            }
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, _: &EstadoRegua, limites: Rect, cursor: Cursor) -> mouse::Interaction {
        if self.tipo.is_some() && cursor.is_over(limites) { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }
}

impl App {
    pub fn medidor(&self) -> Option<Medidor> {
        match self.ferramenta { Ferramenta::Medir(m) if self.aberto.is_some() => Some(m), _ => None }
    }

    /// Camada de medidas da página `n` (nada quando não há medida nem ferramenta).
    pub fn camada_medir(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        let tipo = self.medidor().map(Medidor::tipo);
        let atual_aqui = self.medidas.atual.as_ref().filter(|(p, _)| *p == n);
        if tipo.is_none() && !self.medidas.feitas.iter().any(|m| m.pagina == n) { return None; }
        let ([x0, y0, x1, y1], giro) = self.aberto.as_ref()?.pdf.geometria(n).ok()?;
        let tela = |pts: &[(f32, f32)]| -> Vec<Point> {
            pts.iter().map(|&(x, y)| na_tela([x, y, x, y], [x0, y0, x1, y1], giro, self.zoom).position()).collect()
        };
        let feitas = self.medidas.feitas.iter().filter(|m| m.pagina == n)
            .map(|m| (m.tipo, tela(&m.pontos), medir(m.tipo, &m.pontos, &self.medidas.escala).1)).collect();
        let regua = Regua {
            pagina: n, feitas, atual: atual_aqui.map(|(_, p)| tela(p)).unwrap_or_default(),
            tipo, zoom: self.zoom, escala: self.medidas.escala,
            ima: tipo.filter(|_| !self.medidas.sem_ima).and_then(|_| self.ima_da_pagina(n, &tela)),
            travar: self.modificadores.shift_key(),
        };
        Some(canvas::Canvas::new(regua).width(l).height(a).into())
    }

    /// Vértices da página `n` na tela, lidos do PDF uma vez por zoom e versão do documento.
    fn ima_da_pagina(&self, n: u32, tela: &dyn Fn(&[(f32, f32)]) -> Vec<Point>) -> Option<std::sync::Arc<Vec<Point>>> {
        let mut cache = self.medidas.ima.borrow_mut();
        if let Some((z, g, v)) = cache.get(&n) { if *z == self.zoom && *g == self.geracao { return Some(v.clone()); } }
        let pontos = self.aberto.as_ref()?.pdf.vertices(n).ok()?;
        let v = std::sync::Arc::new(tela(&pontos));
        cache.insert(n, (self.zoom, self.geracao, v.clone()));
        Some(v)
    }

    pub fn update_medir(&mut self, msg: Med) -> Task<Msg> {
        match msg {
            Med::Ima(v) => self.medidas.sem_ima = !v,
            Med::Fechar => {
                // Perímetro fechado conta o lado de volta ao começo; a área já se fecha sozinha.
                if self.medidor().map(Medidor::tipo) == Some(TipoMedida::Perimetro) {
                    if let Some((_, pts)) = &mut self.medidas.atual { if let Some(&p0) = pts.first() { pts.push(p0); } }
                }
                return self.update_medir(Med::Terminar);
            }
            Med::Ferramenta(f) => {
                let concluir = self.concluir_medida();
                self.ferramenta = Ferramenta::Medir(f);
                self.escolhidos = None;
                self.edicao = None;
                self.painel = Some(Painel::Medidas);
                self.status = match f {
                    Medidor::Calibrar => "Calibrar: clique nas duas pontas de uma medida conhecida (uma cota da planta, por exemplo).".into(),
                    Medidor::Nuvem => "Nuvem: clique em volta do trecho; clique duplo, Enter ou botão direito fecha e grava na página.".into(),
                    Medidor::Medir(TipoMedida::Contagem) => "Contar: clique em cada item; Enter ou clique duplo termina a contagem.".into(),
                    Medidor::Medir(t) if t.pontos_fixos().is_some() => format!("{}: clique nos pontos. Esc cancela.", t.nome()),
                    Medidor::Medir(t @ (TipoMedida::Area | TipoMedida::Perimetro)) => format!("{}: clique nos pontos; clique no primeiro ponto para fechar. Shift trava o ângulo; o quadrado verde é o imã.", t.nome()),
                    Medidor::Medir(t) => format!("{}: clique nos pontos; clique duplo, Enter ou botão direito termina. Shift trava o ângulo. Backspace volta um ponto.", t.nome()),
                };
                return concluir;
            }
            Med::Ponto { pagina, p } => {
                let Some(ponto) = self.ponto_na_pagina(pagina, p) else { return Task::none() };
                let Some(medidor) = self.medidor() else { return Task::none() };
                let m = &mut self.medidas;
                match &mut m.atual {
                    Some((pag, pts)) if *pag == pagina => pts.push(ponto),
                    _ => m.atual = Some((pagina, vec![ponto])),
                }
                let feitos = m.atual.as_ref().map_or(0, |a| a.1.len());
                if medidor.tipo().pontos_fixos() == Some(feitos) { return self.update_medir(Med::Terminar); }
            }
            Med::Terminar => {
                let medidor = self.medidor();
                let m = &mut self.medidas;
                let (Some(medidor), Some((pagina, mut pontos))) = (medidor, m.atual.take()) else { return Task::none() };
                // O clique duplo também chegou como ponto: o último repetido sai.
                if pontos.len() > 1 && pontos[pontos.len() - 1] == pontos[pontos.len() - 2] { pontos.pop(); }
                let tipo = medidor.tipo();
                if pontos.len() < tipo.minimo() { self.status = format!("{} precisa de {} pontos.", tipo.nome(), tipo.minimo()); return Task::none(); }
                if medidor == Medidor::Nuvem { return self.gravar_nuvem(pagina, &pontos); }
                if medidor == Medidor::Calibrar {
                    m.dialogo = Some(DialogoEscala { unidade: m.escala.unidade, texto: String::new(), calibrar: Some(comprimento(&pontos) as f32) });
                    return crate::focar(id_escala());
                }
                let texto = medir(tipo, &pontos, &m.escala).1;
                m.feitas.push(Medida { pagina, tipo, pontos });
                self.status = format!("{}: {texto}", tipo.nome());
            }
            Med::Cancelar => self.medidas.atual = None,
            Med::DesfazerPonto => if let Some((_, pts)) = &mut self.medidas.atual {
                pts.pop();
                if pts.is_empty() { self.medidas.atual = None; }
            },
            Med::Apagar(i) => if i < self.medidas.feitas.len() { self.medidas.feitas.remove(i); },
            Med::Limpar => self.medidas.feitas.clear(),
            Med::Copiar => {
                let m = &self.medidas;
                let linhas: Vec<String> = m.feitas.iter()
                    .map(|f| format!("{}\tpágina {}\t{}", f.tipo.nome(), f.pagina, medir(f.tipo, &f.pontos, &m.escala).1)).collect();
                self.status = format!("{} medida(s) copiada(s).", linhas.len());
                return Task::write_clipboard(linhas.join("\n"));
            }
            Med::Gravar => return self.gravar_medidas(),
            Med::AbrirEscala => {
                let m = &mut self.medidas;
                let texto = m.escala.proporcao.map(|n| numero(n, 0).replace('.', "")).unwrap_or_default();
                m.dialogo = Some(DialogoEscala { unidade: m.escala.unidade, texto, calibrar: None });
                return crate::focar(id_escala());
            }
            Med::Unidade(u) => if let Some(d) = &mut self.medidas.dialogo { d.unidade = u },
            Med::Texto(t) => if let Some(d) = &mut self.medidas.dialogo { d.texto = t },
            Med::AplicarEscala => {
                let m = &mut self.medidas;
                let Some(d) = &m.dialogo else { return Task::none() };
                let Some(escala) = escala_do_dialogo(d) else { return Task::none() };
                m.escala = escala;
                m.dialogo = None;
                self.status = format!("Escala {}.", escala.rotulo());
                if self.medidor() == Some(Medidor::Calibrar) { self.ferramenta = Ferramenta::Medir(Medidor::Medir(TipoMedida::Distancia)); }
            }
            Med::FecharEscala => self.medidas.dialogo = None,
        }
        Task::none()
    }

    /// Diálogo da escala sobre a tela.
    pub fn com_dialogo_escala<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(d) = &self.medidas.dialogo else { return crate::inicio::sem_camada(base) };
        let unidade = suspensa(Unidade::TODAS, Some(d.unidade), |u| Msg::Medir(Med::Unidade(u))).text_size(13).padding([5, 8]);
        let campo = text_input(if d.calibrar.is_some() { "ex.: 3,5" } else { "100" }, &d.texto).id(id_escala())
            .on_input(|t| Msg::Medir(Med::Texto(t))).on_submit(Msg::Medir(Med::AplicarEscala)).padding([5, 8]).size(13).width(110);
        let linha: Element<'_, Msg> = if d.calibrar.is_some() {
            row![text("O traço mede").size(13), campo, unidade].spacing(8).align_y(Align::Center).into()
        } else {
            row![text("1 :").size(13), campo, text("medido em").size(13), unidade].spacing(8).align_y(Align::Center).into()
        };
        let valida = escala_do_dialogo(d);
        let mut corpo = column![linha].spacing(10);
        match valida {
            Some(e) => corpo = corpo.push(text(format!("Uma régua de 1 cm no papel mede {} {}.",
                numero(e.fator * 72.0 / 2.54, 2), e.unidade)).size(12).color(tema::COR_TEXTO_MUTED())),
            None if !d.texto.trim().is_empty() => corpo = corpo.push(dlg::aviso("alert-triangle", "Digite um número maior que zero.")),
            None => {}
        }
        let (titulo, sub) = if d.calibrar.is_some() {
            ("Calibrar escala", "Quanto mede, no mundo real, o traço que você acabou de marcar?")
        } else {
            ("Escala", "Proporção do desenho: 1:100 quer dizer que 1 cm no papel vale 1 m.")
        };
        Dialogo::new(titulo, corpo).icone("arrows-horizontal").subtitulo(sub)
            .primario("Aplicar", valida.map(|_| Msg::Medir(Med::AplicarEscala)))
            .ao_fechar(Msg::Medir(Med::FecharEscala)).topo(200.0).sobre(base)
    }

    /// Painel Medidas: a lista, a escala e os totais.
    pub fn painel_medidas(&self) -> Element<'_, Msg> {
        let m = &self.medidas;
        let muted = tema::COR_TEXTO_MUTED();
        let mut lista = column![].spacing(4);
        for (i, f) in m.feitas.iter().enumerate() {
            lista = lista.push(container(row![
                column![
                    text(medir(f.tipo, &f.pontos, &m.escala).1).size(14),
                    text(format!("{} · página {}", f.tipo.nome(), f.pagina)).size(11).color(muted),
                ].spacing(1),
                Space::with_width(Length::FILL),
                button(ic::icone("trash", 14.0)).padding(4).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Medir(Med::Apagar(i))),
            ].align_y(Align::Center)).padding([6, 8]).style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL())),
                border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [6.0; 4] },
                ..Default::default()
            }));
        }
        if m.feitas.is_empty() {
            lista = lista.push(text("Escolha Distância, Perímetro, Área ou Ângulo e clique na página.").size(13).color(muted));
        }
        // Totais por tipo (comprimentos somam distância e perímetro).
        let soma = |tipos: &[TipoMedida]| m.feitas.iter().filter(|f| tipos.contains(&f.tipo))
            .map(|f| medir(f.tipo, &f.pontos, &m.escala).0).sum::<f64>();
        let mut totais = column![].spacing(2);
        let comprimentos = m.feitas.iter().filter(|f| matches!(f.tipo, TipoMedida::Distancia | TipoMedida::Perimetro)).count();
        if comprimentos > 1 {
            totais = totais.push(text(format!("Comprimento total: {} {}", numero(soma(&[TipoMedida::Distancia, TipoMedida::Perimetro]), 2), m.escala.unidade)).size(12));
        }
        if m.feitas.iter().filter(|f| f.tipo == TipoMedida::Area).count() > 1 {
            totais = totais.push(text(format!("Área total: {} {}²", numero(soma(&[TipoMedida::Area]), 2), m.escala.unidade)).size(12));
        }
        let tem = !m.feitas.is_empty();
        let botao = |r: &'static str, msg: Med| button(text(r).size(12)).padding([5, 10]).style(tema::estilo_botao_pequeno(false)).on_press_maybe(tem.then_some(Msg::Medir(msg)));
        column![
            row![text(format!("Escala {}", m.escala.rotulo())).size(12).color(muted), Space::with_width(Length::FILL),
                button(text("Mudar…").size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Medir(Med::AbrirEscala))]
                .align_y(Align::Center),
            checkbox("Imã: grudar nos cantos do desenho", !m.sem_ima).on_toggle(|v| Msg::Medir(Med::Ima(v))).size(15).text_size(12),
            text("Shift trava o ângulo em 45°. Clique no primeiro ponto fecha área e perímetro.").size(11).color(muted),
            scrollable(lista).height(Length::FILL),
            totais,
            row![botao("Copiar tudo", Med::Copiar), botao("Gravar no PDF", Med::Gravar), botao("Limpar", Med::Limpar)].spacing(6),
        ].spacing(10).height(Length::FILL).into()
    }
}

fn escala_do_dialogo(d: &DialogoEscala) -> Option<Escala> {
    let v: f64 = d.texto.trim().replace('.', "").replace(',', ".").parse().ok().filter(|v: &f64| *v > 0.0 && v.is_finite())?;
    match d.calibrar {
        Some(pontos) => Escala::calibrada(pontos, v, d.unidade),
        None => Some(Escala::proporcao(d.unidade, v)),
    }
}

/// Pontos da página levados para dentro de uma caixa (a caixa na página e os
/// pontos em pt a partir do canto de cima à esquerda dela, como na tela).
fn na_caixa(pontos: &[(f32, f32)], folga: f32, pag: [f32; 4], giro: i64) -> ([f32; 4], Vec<Ponto>) {
    let caixa = pontos.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |c, &(x, y)| [c[0].min(x), c[1].min(y), c[2].max(x), c[3].max(y)]);
    let caixa = [caixa[0] - folga, caixa[1] - folga, caixa[2] + folga, caixa[3] + folga];
    let origem = na_tela(caixa, pag, giro, 1.0).position();
    let locais = pontos.iter().map(|&(x, y)| {
        let p = na_tela([x, y, x, y], pag, giro, 1.0).position();
        Ponto { x: p.x - origem.x, y: p.y - origem.y }
    }).collect();
    (caixa, locais)
}

fn circulo(c: &mut Caminho, centro: Ponto, r: f32) {
    let k = 0.5523 * r;
    let p = |x: f32, y: f32| Ponto { x: centro.x + x, y: centro.y + y };
    c.adicionar(Comando::Mover(p(r, 0.0)));
    for (c1, c2, fim) in [((r, k), (k, r), (0.0, r)), ((-k, r), (-r, k), (-r, 0.0)), ((-r, -k), (-k, -r), (0.0, -r)), ((k, -r), (r, -k), (r, 0.0))] {
        c.adicionar(Comando::Cubica { controle1: p(c1.0, c1.1), controle2: p(c2.0, c2.1), fim: p(fim.0, fim.1) });
    }
}

/// Contorno de nuvem: cada lado vira arcos de ~2r para fora do polígono.
fn nuvem(pontos: &[Ponto], r: f32) -> Caminho {
    let n = pontos.len() as f32;
    let centro = Ponto { x: pontos.iter().map(|p| p.x).sum::<f32>() / n, y: pontos.iter().map(|p| p.y).sum::<f32>() / n };
    let mut c = Caminho::novo();
    c.adicionar(Comando::Mover(pontos[0]));
    for i in 0..pontos.len() {
        let (a, b) = (pontos[i], pontos[(i + 1) % pontos.len()]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        if len < 0.5 { continue; }
        let arcos = (len / (2.0 * r)).round().max(1.0) as usize;
        let (mut nx, mut ny) = (dy / len, -dx / len);
        let meio = Ponto { x: (a.x + b.x) / 2.0, y: (a.y + b.y) / 2.0 };
        if (meio.x - centro.x) * nx + (meio.y - centro.y) * ny < 0.0 { nx = -nx; ny = -ny; }
        for k in 0..arcos {
            let t0 = k as f32 / arcos as f32;
            let t1 = (k + 1) as f32 / arcos as f32;
            let tm = (t0 + t1) / 2.0;
            let controle = Ponto { x: a.x + dx * tm + nx * r * 1.6, y: a.y + dy * tm + ny * r * 1.6 };
            c.adicionar(Comando::Quadratica { controle, fim: Ponto { x: a.x + dx * t1, y: a.y + dy * t1 } });
        }
    }
    c.adicionar(Comando::Fechar);
    c
}

const AZUL_PDF: [f32; 3] = [0.11, 0.45, 0.84];

impl App {
    /// Antes de trocar de ferramenta: a contagem ou a nuvem em andamento termina como se
    /// tivesse apertado Enter, em vez de sumir. Sem pontos suficientes, é descartada.
    pub fn concluir_medida(&mut self) -> Task<Msg> {
        let (Some(medidor), Some((_, pontos))) = (self.medidor(), &self.medidas.atual) else { return Task::none() };
        if medidor == Medidor::Calibrar || pontos.len() < medidor.tipo().minimo() {
            self.medidas.atual = None;
            return Task::none();
        }
        self.update_medir(Med::Terminar)
    }

    /// Nuvem de revisão vermelha gravada na página (um Ctrl+Z), já escolhida para mover.
    fn gravar_nuvem(&mut self, pagina: u32, pontos: &[(f32, f32)]) -> Task<Msg> {
        let Some(Ok((pag, giro))) = self.aberto.as_ref().map(|a| a.pdf.geometria(pagina)) else { return Task::none() };
        let raio = 7.0;
        let (caixa, locais) = na_caixa(pontos, raio * 1.6, pag, giro);
        let desenho = DesenhoNovo {
            preenchida: Caminho::novo(), tracos: nuvem(&locais, raio),
            preenchimento: None, contorno: Some([0.85, 0.1, 0.1]), espessura: 1.5,
        };
        self.editar(|pdf| { pdf.inserir_desenho(pagina, caixa, &desenho)?; Ok("Nuvem de revisão inserida.".into()) })
    }

    /// Desenha as medidas da lista na página (traços azuis e o valor), um Ctrl+Z para todas.
    fn gravar_medidas(&mut self) -> Task<Msg> {
        let Some(aberto) = &self.aberto else { return Task::none() };
        let mut desenhos = Vec::new();
        let mut textos = Vec::new();
        for m in &self.medidas.feitas {
            let Ok((pag, giro)) = aberto.pdf.geometria(m.pagina) else { continue };
            let (caixa, locais) = na_caixa(&m.pontos, 10.0, pag, giro);
            let mut tracos = Caminho::novo();
            if m.tipo == TipoMedida::Contagem {
                for &p in &locais { circulo(&mut tracos, p, 5.0); }
            } else {
                for (i, &p) in locais.iter().enumerate() { tracos.adicionar(if i == 0 { Comando::Mover(p) } else { Comando::Linha(p) }); }
                if m.tipo == TipoMedida::Area { tracos.adicionar(Comando::Fechar); }
            }
            desenhos.push((m.pagina, caixa, DesenhoNovo { preenchida: Caminho::novo(), tracos, preenchimento: None, contorno: Some(AZUL_PDF), espessura: 1.2 }));
            // O valor perto do mesmo ponto da tela; a caixa do texto começa um pouco acima e à direita.
            let tela: Vec<Point> = m.pontos.iter().map(|&(x, y)| na_tela([x, y, x, y], pag, giro, self.zoom).position()).collect();
            let alvo = ponto_do_rotulo(m.tipo, &tela);
            if let Some((x, y)) = self.ponto_na_pagina(m.pagina, Point::new(alvo.x + 6.0 * self.zoom, alvo.y - 14.0 * self.zoom)) {
                textos.push((m.pagina, x, y, medir(m.tipo, &m.pontos, &self.medidas.escala).1));
            }
        }
        if desenhos.is_empty() { return Task::none(); }
        let n = desenhos.len();
        let tarefa = self.editar(|pdf| {
            for (pagina, caixa, d) in &desenhos { pdf.inserir_desenho(*pagina, *caixa, d)?; }
            for (pagina, x, y, t) in &textos { pdf.inserir_texto(*pagina, *x, *y, t, 9.0, true, AZUL_PDF)?; }
            Ok(format!("{n} medida(s) gravada(s) na página."))
        });
        if self.ultima_edicao_ok { self.medidas.feitas.clear(); }
        tarefa
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn regua(atual: Vec<Point>, ima: Vec<Point>, travar: bool, tipo: TipoMedida) -> Regua {
        Regua { pagina: 1, feitas: vec![], atual, tipo: Some(tipo), zoom: 1.0, escala: Escala::default(),
            ima: Some(std::sync::Arc::new(ima)), travar }
    }

    #[test]
    fn ima_gruda_no_canto_mais_perto() {
        let r = regua(vec![], vec![Point::new(100.0, 100.0), Point::new(104.0, 100.0)], false, TipoMedida::Distancia);
        assert_eq!(r.ajustar(Point::new(103.0, 101.0)), (Point::new(104.0, 100.0), true));
        // Longe de tudo: fica onde o mouse está.
        assert_eq!(r.ajustar(Point::new(300.0, 300.0)), (Point::new(300.0, 300.0), false));
    }

    #[test]
    fn shift_trava_em_45_graus() {
        let r = regua(vec![Point::new(0.0, 0.0)], vec![], true, TipoMedida::Distancia);
        let (p, _) = r.ajustar(Point::new(100.0, 8.0));
        assert!((p.y).abs() < 0.01 && (p.x - 100.32).abs() < 0.1, "quase horizontal vira horizontal: {p:?}");
        let (p, _) = r.ajustar(Point::new(50.0, 46.0));
        assert!((p.x - p.y).abs() < 0.01, "quase diagonal vira 45°: {p:?}");
    }

    #[test]
    fn clicar_no_primeiro_ponto_fecha_area_e_perimetro() {
        let pts = vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0)];
        assert!(regua(pts.clone(), vec![], false, TipoMedida::Area).fecha(Point::new(3.0, 2.0)));
        assert!(regua(pts.clone(), vec![], false, TipoMedida::Perimetro).fecha(Point::new(3.0, 2.0)));
        assert!(!regua(pts.clone(), vec![], false, TipoMedida::Distancia).fecha(Point::new(3.0, 2.0)));
        assert!(!regua(pts[..2].to_vec(), vec![], false, TipoMedida::Area).fecha(Point::new(3.0, 2.0)), "com 2 pontos ainda não fecha");
    }
}
