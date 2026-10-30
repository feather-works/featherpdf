//! Ferramenta "Editar texto": linhas de texto e imagens da página viram objetos.
//! Passar o mouse destaca, clique escolhe (Shift soma), arrastar move, clique
//! duplo num texto abre a caixa de edição no lugar, Alt+clique escolhe o parágrafo
//! (tags do PDF ou posição), Delete apaga, setas movem 1 mm (Shift: 10 mm). Cada mudança é um Ctrl+Z.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use featherpdf_core::{ObjetoPdf, TipoObjeto};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::{self, Cursor};
use crate::compat::{event, canvas::{self, Frame, Geometry, Path, Stroke}};
use crate::inicio::VERMELHO;
use crate::{App, Ferramenta, Msg};

const CLIQUE_DUPLO: Duration = Duration::from_millis(400);
const MM: f32 = 72.0 / 25.4;
/// Distância, em pixels da tela, em que o arrasto gruda numa guia.
const ENCAIXE_PX: f32 = 6.0;
/// Margem desenhada e usada pelo encaixe.
const MARGEM_MM: f32 = 15.0;
const ROSA: Color = Color { r: 0.88, g: 0.18, b: 0.54, a: 1.0 };
const CIANO: Color = Color { r: 0.0, g: 0.62, b: 0.85, a: 1.0 };
/// Largura das réguas no topo e à esquerda da página (px da tela).
const REGUA: f32 = 14.0;
/// Passos da grade: rótulo e tamanho em pontos.
pub const PASSOS_GRADE: [(&str, f32); 6] = [("2 mm", 2.0 * MM), ("5 mm", 5.0 * MM), ("10 mm", 10.0 * MM), ("6 pt", 6.0), ("12 pt", 12.0), ("36 pt", 36.0)];

pub fn id_edicao() -> &'static str { "featherpdf.edicao" }

#[derive(Debug, Clone)]
pub enum Obj {
    Escolher { pagina: u32, objeto: usize },
    Limpar,
    /// Retângulo arrastado no vazio: escolhe os objetos que ele toca (`Shift` soma), em pixels da tela.
    Laco { pagina: u32, objetos: Vec<usize> },
    /// Arrasto terminado, em pixels da tela.
    Arrastou { pagina: u32, dx: f32, dy: f32 },
    Editar { pagina: u32, objeto: usize },
    Texto(String),
    Aplicar,
    Cancelar,
    Excluir,
    /// `true` traz para a frente (por cima de tudo), `false` manda para trás.
    Ordem(bool),
    /// Tamanho, cor e espaçamento dos textos escolhidos.
    Tipografia(featherpdf_core::EstiloTexto),
    /// Setas: deslocamento em milímetros na tela (x para a direita, y para baixo).
    Seta(f32, f32),
}

/// Objetos de uma página já lidos, com a caixa visível e o giro.
pub struct ObjetosDaPagina {
    pub lista: Vec<ObjetoPdf>,
    pub caixa: [f32; 4],
    pub giro: i64,
}

#[derive(Debug, Clone)]
pub struct Edicao { pub pagina: u32, pub objeto: usize, pub texto: String, original: String }

/// Retângulo do objeto na tela (pixels lógicos a partir do canto da página).
pub fn na_tela(c: [f32; 4], pag: [f32; 4], giro: i64, zoom: f32) -> Rect {
    let [x0, y0, x1, y1] = pag;
    let ponto = |x: f32, y: f32| match giro {
        90 => (y - y0, x - x0),
        180 => (x1 - x, y - y0),
        270 => (y1 - y, x1 - x),
        _ => (x - x0, y1 - y),
    };
    let (a, b) = (ponto(c[0], c[1]), ponto(c[2], c[3]));
    let (l, t) = (a.0.min(b.0) * zoom, a.1.min(b.1) * zoom);
    Rect { x: l, y: t, width: (a.0 - b.0).abs() * zoom, height: (a.1 - b.1).abs() * zoom }
}

/// Deslocamento da tela (y para baixo) no espaço da página (y para cima).
pub(crate) fn para_pagina(dx: f32, dy: f32, giro: i64, zoom: f32) -> (f32, f32) {
    let (dx, dy) = (dx / zoom, dy / zoom);
    match giro { 90 => (dy, dx), 180 => (-dx, dy), 270 => (-dy, -dx), _ => (dx, -dy) }
}

/// Camada desenhada sobre a página: destaque, seleção e prévia do arrasto.
pub struct Camada {
    pagina: u32,
    caixas: Vec<(Rect, bool)>,
    escolhidos: Vec<usize>,
    /// Tamanho da página na tela e o zoom (a grade de 5 mm acompanha).
    tamanho: (f32, f32),
    zoom: f32,
    /// Encaixar em objetos, na margem e centro da página, na grade.
    encaixe: [bool; 3],
    /// Passo da grade em pontos.
    passo: f32,
    reguas: bool,
    /// Guias (vertical?, posição em pontos da página vista).
    guias: Vec<(bool, f32)>,
}

/// Guia que acendeu: vertical (x) ou horizontal (y), posição e nome.
struct Guia { vertical: bool, pos: f32, nome: &'static str }

#[derive(Default)]
pub struct EstadoCamada {
    sobre: Option<usize>,
    arrasto: Option<(Point, Point, bool)>,
    /// Seleção por retângulo, começada no vazio.
    laco: Option<(Point, Point)>,
    ultimo: Option<(usize, Instant)>,
    /// Guia sendo puxada da régua: vertical?, posição na tela.
    guia: Option<(bool, f32)>,
}

impl Camada {
    /// Deslocamento corrigido para grudar nas guias ligadas, e as guias que acenderam.
    fn encaixar(&self, desvio: Vector) -> (Vector, Vec<Guia>) {
        let mut sel = self.escolhidos.iter().filter_map(|&k| self.caixas.get(k)).map(|(r, _)| *r);
        let Some(primeiro) = sel.next() else { return (desvio, vec![]) };
        let caixa = sel.fold(primeiro, |a, b| a.union(&b));
        let (l, a) = self.tamanho;
        let margem = MARGEM_MM * MM * self.zoom;
        let mut alvos: Vec<(bool, f32, &'static str)> = Vec::new();
        if self.encaixe[1] {
            alvos.extend([(true, l / 2.0, "centro da página"), (false, a / 2.0, "centro da página"),
                (true, margem, "margem"), (true, l - margem, "margem"), (false, margem, "margem"), (false, a - margem, "margem"),
                (true, 0.0, "borda"), (true, l, "borda"), (false, 0.0, "borda"), (false, a, "borda")]);
        }
        alvos.extend(self.guias.iter().map(|&(v, pos)| (v, pos * self.zoom, "guia")));
        if self.encaixe[0] {
            for (_, (r, _)) in self.caixas.iter().enumerate().filter(|(k, _)| !self.escolhidos.contains(k)) {
                alvos.extend([(true, r.x, "objeto"), (true, r.center_x(), "objeto"), (true, r.x + r.width, "objeto"),
                    (false, r.y, "objeto"), (false, r.center_y(), "objeto"), (false, r.y + r.height, "objeto")]);
            }
        }
        let melhor = |vertical: bool, bordas: [f32; 3]| -> Option<(f32, Guia)> {
            let mut achado: Option<(f32, Guia)> = None;
            for b in bordas {
                for &(v, pos, nome) in alvos.iter().filter(|(v, _, _)| *v == vertical) {
                    let d = pos - b;
                    if d.abs() <= ENCAIXE_PX && achado.as_ref().is_none_or(|(m, _)| d.abs() < m.abs()) {
                        achado = Some((d, Guia { vertical: v, pos, nome }));
                    }
                }
            }
            if achado.is_none() && self.encaixe[2] {
                // Grade: o canto de cima à esquerda da seleção cai num múltiplo do passo.
                let passo = self.passo * self.zoom;
                let d = (bordas[0] / passo).round() * passo - bordas[0];
                if d.abs() <= ENCAIXE_PX { achado = Some((d, Guia { vertical, pos: bordas[0] + d, nome: "grade" })); }
            }
            achado
        };
        let movida = Rect { x: caixa.x + desvio.x, y: caixa.y + desvio.y, ..caixa };
        let gx = melhor(true, [movida.x, movida.center_x(), movida.x + movida.width]);
        let gy = melhor(false, [movida.y, movida.center_y(), movida.y + movida.height]);
        let ajuste = Vector::new(gx.as_ref().map_or(0.0, |g| g.0), gy.as_ref().map_or(0.0, |g| g.0));
        (desvio + ajuste, gx.into_iter().chain(gy).map(|(_, g)| g).collect())
    }

    /// O menor objeto sob o ponto: numa linha dentro de uma imagem, a linha ganha.
    fn achar(&self, p: Point) -> Option<usize> {
        self.caixas.iter().enumerate()
            .filter(|(_, (r, _))| r.expand(2.0).contains(p))
            .min_by(|a, b| (a.1.0.width * a.1.0.height).total_cmp(&(b.1.0.width * b.1.0.height)))
            .map(|(i, _)| i)
    }
}

impl canvas::Program<Msg> for Camada {
    type State = EstadoCamada;

    fn update(&self, estado: &mut EstadoCamada, evento: canvas::Event, limites: Rect, cursor: Cursor)
        -> (event::Status, Option<Msg>)
    {
        let dentro = cursor.position_in(limites);
        match evento {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some((v, pos)) = &mut estado.guia {
                    if let Some(p) = cursor.position_from(limites.position()) { *pos = if *v { p.x } else { p.y }; }
                    return (event::Status::Captured, None);
                }
                if let Some((_, b)) = &mut estado.laco {
                    if let Some(p) = cursor.position_from(limites.position()) { *b = p; }
                    return (event::Status::Captured, None);
                }
                if let Some((inicio, atual, moveu)) = &mut estado.arrasto {
                    if let Some(p) = cursor.position_from(limites.position()) {
                        *atual = p;
                        *moveu |= inicio.distance(p) > 3.0;
                    }
                    return (event::Status::Captured, None);
                }
                estado.sobre = dentro.and_then(|p| self.achar(p));
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = dentro else { return (event::Status::Ignored, None) };
                // Da régua de cima sai uma guia horizontal; da esquerda, uma vertical.
                if self.reguas && (p.y < REGUA || p.x < REGUA) {
                    estado.guia = Some(if p.y < REGUA { (false, p.y) } else { (true, p.x) });
                    return (event::Status::Captured, None);
                }
                let Some(k) = self.achar(p) else {
                    estado.laco = Some((p, p));
                    return (event::Status::Captured, None);
                };
                let duplo = estado.ultimo.is_some_and(|(u, t)| u == k && t.elapsed() < CLIQUE_DUPLO);
                estado.ultimo = Some((k, Instant::now()));
                if duplo && self.caixas[k].1 {
                    estado.arrasto = None;
                    return (event::Status::Captured, Some(Msg::Obj(Obj::Editar { pagina: self.pagina, objeto: k })));
                }
                estado.arrasto = Some((p, p, false));
                return (event::Status::Captured, Some(Msg::Obj(Obj::Escolher { pagina: self.pagina, objeto: k })));
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                // Fora de objeto, o botão direito passa para o menu da página.
                let Some(k) = dentro.and_then(|p| self.achar(p)) else { return (event::Status::Ignored, None) };
                let alvo = crate::contexto::Alvo::Objeto { pagina: self.pagina, objeto: k };
                return (event::Status::Captured, Some(Msg::Ctx(crate::contexto::Ct::Abrir(alvo))));
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((v, pos)) = estado.guia.take() {
                    // Solta de volta na régua: desiste.
                    if pos < REGUA { return (event::Status::Captured, None); }
                    return (event::Status::Captured, Some(Msg::Ins(crate::inserir::Ins::Guia(v, pos / self.zoom))));
                }
                if let Some((a, b)) = estado.laco.take() {
                    // Clique sem arrastar no vazio: só limpa, como antes.
                    if a.distance(b) <= 3.0 { return (event::Status::Captured, Some(Msg::Obj(Obj::Limpar))); }
                    let laco = Rect::new(Point::new(a.x.min(b.x), a.y.min(b.y)), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()));
                    let objetos = self.caixas.iter().enumerate().filter(|(_, (c, _))| c.intersects(&laco)).map(|(i, _)| i).collect();
                    return (event::Status::Captured, Some(Msg::Obj(Obj::Laco { pagina: self.pagina, objetos })));
                }
                if let Some((inicio, fim, true)) = estado.arrasto.take() {
                    let (d, _) = self.encaixar(Vector::new(fim.x - inicio.x, fim.y - inicio.y));
                    return (event::Status::Captured, Some(Msg::Obj(Obj::Arrastou { pagina: self.pagina, dx: d.x, dy: d.y })));
                }
            }
            _ => {}
        }
        (event::Status::Ignored, None)
    }

    fn draw(&self, estado: &EstadoCamada, renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        let borda = |f: &mut Frame, r: Rect, cor: Color, w: f32| {
            f.stroke(&Path::rectangle(r.position(), r.size()), Stroke::default().with_color(cor).with_width(w));
        };
        let (l, a) = self.tamanho;
        if self.encaixe[2] {
            let passo = self.passo * self.zoom;
            // Grade fina demais vira borrão: dobra o passo desenhado até ficar legível.
            let mut visto = passo;
            while visto < 6.0 { visto *= 2.0; }
            let cor = Stroke::default().with_color(Color { a: 0.12, ..CIANO }).with_width(1.0);
            let mut x = visto;
            while x < l { f.stroke(&Path::line(Point::new(x, 0.0), Point::new(x, a)), cor); x += visto; }
            let mut y = visto;
            while y < a { f.stroke(&Path::line(Point::new(0.0, y), Point::new(l, y)), cor); y += visto; }
        }
        for &(v, pos) in &self.guias {
            let p = pos * self.zoom;
            let (de, ate) = if v { (Point::new(p, 0.0), Point::new(p, a)) } else { (Point::new(0.0, p), Point::new(l, p)) };
            f.stroke(&Path::line(de, ate), Stroke::default().with_color(CIANO).with_width(1.0));
        }
        if self.reguas {
            let fundo = Color::rgba(0.95, 0.95, 0.95, 0.92);
            f.fill_rectangle(Point::ORIGIN, Size::new(l, REGUA), fundo);
            f.fill_rectangle(Point::ORIGIN, Size::new(REGUA, a), fundo);
            let cinza = Color::rgb(0.45, 0.45, 0.45);
            let traco = Stroke::default().with_color(cinza).with_width(1.0);
            // Marca a cada 5 mm, número a cada 10 mm (em cm).
            for i in 1.. {
                let d = i as f32 * 5.0 * MM * self.zoom;
                if d > l.max(a) { break; }
                let t = if i % 2 == 0 { REGUA * 0.6 } else { REGUA * 0.3 };
                if d < l { f.stroke(&Path::line(Point::new(d, REGUA - t), Point::new(d, REGUA)), traco); }
                if d < a { f.stroke(&Path::line(Point::new(REGUA - t, d), Point::new(REGUA, d)), traco); }
                if i % 2 == 0 && 10.0 * MM * self.zoom >= 18.0 {
                    let n = (i / 2).to_string();
                    if d < l { f.fill_text(canvas::Text { content: n.clone(), position: Point::new(d + 2.0, 0.0), color: cinza, size: 9.0, ..canvas::Text::default() }); }
                    if d < a { f.fill_text(canvas::Text { content: n, position: Point::new(1.0, d + 1.0), color: cinza, size: 9.0, ..canvas::Text::default() }); }
                }
            }
        }
        if let Some((v, p)) = estado.guia {
            let (de, ate) = if v { (Point::new(p, 0.0), Point::new(p, a)) } else { (Point::new(0.0, p), Point::new(l, p)) };
            f.stroke(&Path::line(de, ate), Stroke { line_dash: canvas::LineDash { segments: &[4.0, 3.0], offset: 0 }, ..Stroke::default().with_color(CIANO).with_width(1.0) });
        }
        if let Some(k) = estado.sobre.filter(|k| !self.escolhidos.contains(k)) {
            borda(&mut f, self.caixas[k].0.expand(1.0), Color { a: 0.55, ..VERMELHO }, 1.0);
        }
        let (desvio, guias) = match estado.arrasto {
            Some((a, b, true)) => self.encaixar(Vector::new(b.x - a.x, b.y - a.y)),
            _ => (Vector::new(0.0, 0.0), vec![]),
        };
        if estado.arrasto.is_some_and(|a| a.2) && self.encaixe[1] {
            let m = MARGEM_MM * MM * self.zoom;
            f.stroke(&Path::rectangle(Point::new(m, m), Size::new(l - 2.0 * m, a - 2.0 * m)),
                Stroke { line_dash: canvas::LineDash { segments: &[4.0, 3.0], offset: 0 }, ..Stroke::default().with_color(Color { a: 0.5, ..ROSA }).with_width(1.0) });
        }
        if let Some((a, b)) = estado.laco {
            let (p, t) = (Point::new(a.x.min(b.x), a.y.min(b.y)), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()));
            f.fill_rectangle(p, t, Color { a: 0.08, ..VERMELHO });
            f.stroke(&Path::rectangle(p, t), Stroke { line_dash: canvas::LineDash { segments: &[4.0, 3.0], offset: 0 }, ..Stroke::default().with_color(VERMELHO).with_width(1.0) });
        }
        for g in &guias {
            let (de, ate, rotulo) = if g.vertical { (Point::new(g.pos, 0.0), Point::new(g.pos, a), Point::new(g.pos + 4.0, 4.0)) }
                else { (Point::new(0.0, g.pos), Point::new(l, g.pos), Point::new(4.0, g.pos + 3.0)) };
            f.stroke(&Path::line(de, ate), Stroke::default().with_color(ROSA).with_width(1.0));
            f.fill_text(canvas::Text { content: g.nome.into(), position: rotulo, color: ROSA, size: 11.0, ..canvas::Text::default() });
        }
        for &k in &self.escolhidos {
            let Some((r, _)) = self.caixas.get(k) else { continue };
            let r = r.expand(1.5);
            f.fill_rectangle(r.position(), r.size(), Color { a: 0.08, ..VERMELHO });
            borda(&mut f, r, VERMELHO, 1.5);
            if desvio != Vector::new(0.0, 0.0) {
                let movido = Rect { x: r.x + desvio.x, y: r.y + desvio.y, ..r };
                f.fill_rectangle(movido.position(), movido.size(), Color { a: 0.12, ..VERMELHO });
                borda(&mut f, movido, Color { a: 0.8, ..VERMELHO }, 1.0);
            }
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, estado: &EstadoCamada, limites: Rect, cursor: Cursor) -> mouse::Interaction {
        if estado.guia.is_some_and(|g| g.0) { return mouse::Interaction::ResizingHorizontally; }
        if estado.guia.is_some() { return mouse::Interaction::ResizingVertically; }
        if estado.arrasto.is_some_and(|a| a.2) { return mouse::Interaction::Grabbing; }
        match cursor.position_in(limites).and_then(|p| self.achar(p)) {
            Some(k) if self.escolhidos.contains(&k) => mouse::Interaction::Grab,
            Some(k) if self.caixas[k].1 => mouse::Interaction::Text,
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::default(),
        }
    }
}

impl App {
    pub fn editando_objetos(&self) -> bool { self.ferramenta == Ferramenta::Editar && self.aberto.is_some() }

    /// Lê os objetos das páginas pedidas que ainda não foram lidas (cerca de 1–2 ms por página).
    pub fn ler_objetos(&mut self, paginas: &[u32]) {
        if self.editando_objetos() || self.busca.aberta || self.ferramenta == Ferramenta::Selecionar || self.modo_escuro { self.ler_objetos_sempre(paginas); }
    }

    pub fn ler_objetos_sempre(&mut self, paginas: &[u32]) {
        let Some(a) = &self.aberto else { return };
        for &p in paginas {
            if self.objetos.contains_key(&p) { continue; }
            let lido = a.pdf.objetos(p).and_then(|lista| {
                let (caixa, giro) = a.pdf.geometria(p)?;
                Ok(ObjetosDaPagina { lista, caixa, giro })
            });
            match lido {
                Ok(o) => { self.objetos.insert(p, o); }
                Err(e) => {
                    self.status = format!("Página {p}: {e}");
                    self.objetos.insert(p, ObjetosDaPagina { lista: vec![], caixa: [0.0; 4], giro: 0 });
                }
            }
        }
    }

    /// Camada de objetos e caixa de edição por cima da página `n` (tamanho na tela `l` × `a`).
    pub fn sobre_pagina<'a>(&'a self, n: u32, pagina: Element<'a, Msg>, l: f32, a: f32) -> Element<'a, Msg> {
        let destaques = self.destaques(n, l, a);
        if self.insercao().is_some() {
            let pilha = Stack::new().width(l).height(a).push(pagina).push_maybe(destaques).push_maybe(self.camada_medir(n, l, a));
            return self.camada_inserir(n, pilha, l, a).into();
        }
        let Some(o) = self.objetos.get(&n).filter(|_| self.editando_objetos()) else {
            let medidas = self.camada_medir(n, l, a);
            let tarjas = self.camada_tarja(n, l, a);
            let comentar = self.camada_comentar(n, l, a);
            let campos = self.camada_formulario(n, l, a);
            let assinatura = self.camada_assinatura(n, l, a);
            let selecao = self.camada_selecao(n, l, a);
            let regiao = self.camada_regiao(n, l, a);
            if destaques.is_none() && medidas.is_none() && tarjas.is_none() && comentar.is_none() && campos.is_none() && assinatura.is_none() && selecao.is_none() && regiao.is_none() { return pagina; }
            return Stack::new().width(l).height(a).push(pagina).push_maybe(destaques).push_maybe(selecao).push_maybe(medidas)
                .push_maybe(tarjas).push_maybe(comentar).push_maybe(campos).push_maybe(assinatura).push_maybe(regiao).into();
        };
        let caixas: Vec<(Rect, bool)> = o.lista.iter()
            .map(|ob| (na_tela(ob.caixa, o.caixa, o.giro, self.zoom), matches!(ob.tipo, TipoObjeto::Texto { .. })))
            .collect();
        let escolhidos = match &self.escolhidos { Some((p, s)) if *p == n => s.iter().copied().collect(), _ => vec![] };
        let mut pilha = Stack::new().width(l).height(a)
            .push(pagina)
            .push_maybe(destaques)
            // Contagens e medidas continuam à vista enquanto se edita (só desenho: a régua não recebe clique fora do Medir).
            .push_maybe(self.camada_medir(n, l, a))
            .push(canvas::Canvas::new(Camada { pagina: n, caixas: caixas.clone(), escolhidos, tamanho: (l, a), zoom: self.zoom, encaixe: self.ins.encaixe,
                passo: self.ins.passo_grade, reguas: self.ins.reguas, guias: self.ins.guias.clone() })
                .width(l).height(a));
        if let Some(ed) = self.edicao.as_ref().filter(|e| e.pagina == n) {
            if let Some((r, _)) = caixas.get(ed.objeto) {
                let tamanho = (r.height * 0.95).clamp(10.0, 72.0);
                // Mesma família e peso da fonte da linha, sobre a cor do papel em volta dela.
                let nome = match o.lista.get(ed.objeto).map(|ob| &ob.tipo) { Some(TipoObjeto::Texto { fonte, .. }) => fonte.to_lowercase(), _ => String::new() };
                let fonte = Font {
                    family: if nome.contains("courier") || nome.contains("mono") { Some("monospace") }
                        else if nome.contains("times") || nome.contains("serif") && !nome.contains("sans") || nome.contains("georgia") { Some("serif") }
                        else { None },
                    weight: if nome.contains("semibold") || nome.contains("demi") || nome.contains("medium") { 600 }
                        else if nome.contains("bold") || nome.contains("black") || nome.contains("heavy") { 700 } else { 400 },
                    italic: nome.contains("italic") || nome.contains("oblique"),
                };
                let fundo = self.cor_do_papel(n, *r).unwrap_or(Color::WHITE);
                // A cor da tinta sai do próprio desenho: a caixa fica igual ao texto, só com o cursor.
                let tinta = self.cor_da_tinta(n, *r, fundo).unwrap_or(Color::BLACK);
                let caixa = text_input("", &ed.texto).id(id_edicao()).font(fonte)
                    .on_input(|t| Msg::Obj(Obj::Texto(t))).on_submit(Msg::Obj(Obj::Aplicar))
                    .size(tamanho).padding([1, 4]).width((r.width + 40.0).max(140.0).min(l - r.x + 20.0))
                    .style(move |tema, status| {
                        let mut s = campo_padrao(tema, status);
                        s.background = Some(Background::Color(fundo));
                        s.text_color = Some(tinta);
                        s.border = Border { color: Color { a: 0.45, ..feather_ui::tema::COR_AZUL_ACENTO() }, width: 1.0, radius: [2.0; 4] };
                        s
                    });
                pilha = pilha.push(column![
                    Space::with_height((r.y - 3.0).max(0.0)),
                    row![Space::with_width((r.x - 4.0).max(0.0)), container(caixa)],
                ]);
            }
        }
        pilha.into()
    }

    /// A cor mais distante do papel dentro de `r`: a da letra (preta, vermelha de botão, branca sobre fundo escuro).
    fn cor_da_tinta(&self, pagina: u32, r: Rect, papel: Color) -> Option<Color> {
        let d = self.desenhadas.get(&pagina)?;
        let pixels = &d.pixels;
        let k = d.escala / self.zoom;
        let (ox, oy) = d.trecho.map_or((0.0, 0.0), |t| (t[0] as f32, t[1] as f32));
        let distancia = |c: &[u8]| (c[0] as f32 / 255.0 - papel.r).abs() + (c[1] as f32 / 255.0 - papel.g).abs() + (c[2] as f32 / 255.0 - papel.b).abs();
        let mut melhor: Option<(f32, [u8; 3])> = None;
        for j in 0..12 {
            for i in 0..40 {
                let (px, py) = ((r.x + r.width * (i as f32 + 0.5) / 40.0) * k - ox, (r.y + r.height * (j as f32 + 0.5) / 12.0) * k - oy);
                if px < 0.0 || py < 0.0 || px >= d.largura as f32 || py >= d.altura as f32 { continue; }
                let at = (py as usize * d.largura as usize + px as usize) * 4;
                let Some(c) = pixels.get(at..at + 3) else { continue };
                let dist = distancia(c);
                if melhor.is_none_or(|(m, _)| dist > m) { melhor = Some((dist, [c[0], c[1], c[2]])); }
            }
        }
        melhor.filter(|(m, _)| *m > 0.3).map(|(_, c)| Color::rgb8(c[0], c[1], c[2]))
    }

    /// Cor do desenho da página logo à esquerda do retângulo `r` (na tela, sem o zoom da janela).
    fn cor_do_papel(&self, pagina: u32, r: Rect) -> Option<Color> {
        let d = self.desenhadas.get(&pagina)?;
        let pixels = &d.pixels;
        let k = d.escala / self.zoom;
        let (ox, oy) = d.trecho.map_or((0.0, 0.0), |t| (t[0] as f32, t[1] as f32));
        let ponto = |x: f32, y: f32| -> Option<Color> {
            let (px, py) = (x * k - ox, y * k - oy);
            if px < 0.0 || py < 0.0 || px >= d.largura as f32 || py >= d.altura as f32 { return None; }
            let i = (py as usize * d.largura as usize + px as usize) * 4;
            let p = pixels.get(i..i + 3)?;
            Some(Color::rgb8(p[0], p[1], p[2]))
        };
        let meio = r.y + r.height / 2.0;
        ponto(r.x - 2.0, meio).or_else(|| ponto(r.x + r.width + 2.0, meio)).or_else(|| ponto(r.x + 1.0, r.y - 2.0))
    }

    pub fn update_objetos(&mut self, msg: Obj) -> Task<Msg> {
        match msg {
            Obj::Escolher { pagina, objeto } => {
                let tarefa = self.fechar_edicao(true);
                // Alt+clique: o parágrafo inteiro, pelas tags do PDF ou pela geometria das linhas.
                if let (true, Some(a)) = (self.modificadores.alt_key(), &self.aberto) {
                    match a.pdf.paragrafo(pagina, objeto) {
                        Ok((linhas, _)) if linhas.is_empty() => self.status = "Alt+clique escolhe o parágrafo de uma linha de texto.".into(),
                        Ok((linhas, tags)) => {
                            let fonte = if tags { "tags do PDF" } else { "pela posição das linhas" };
                            self.status = format!("Parágrafo ({fonte}) · {} linha(s) · arraste ou use as setas para mover", linhas.len());
                            self.escolhidos = Some((pagina, linhas.into_iter().collect()));
                        }
                        Err(e) => self.status = e,
                    }
                    self.pagina = pagina;
                    return tarefa;
                }
                let somar = self.modificadores.shift_key();
                match &mut self.escolhidos {
                    Some((p, s)) if *p == pagina && somar => { if !s.remove(&objeto) { s.insert(objeto); } }
                    Some((p, s)) if *p == pagina && s.contains(&objeto) => {}
                    _ => self.escolhidos = Some((pagina, BTreeSet::from([objeto]))),
                }
                self.pagina = pagina;
                self.descrever_escolha();
                return tarefa;
            }
            Obj::Laco { pagina, objetos } => {
                let tarefa = self.fechar_edicao(true);
                let mut escolhidos = match self.escolhidos.take() { Some((p, s)) if p == pagina && self.modificadores.shift_key() => s, _ => BTreeSet::new() };
                escolhidos.extend(objetos);
                self.escolhidos = (!escolhidos.is_empty()).then_some((pagina, escolhidos));
                return tarefa;
            }
            Obj::Limpar => {
                let tarefa = self.fechar_edicao(true);
                self.escolhidos = None;
                return tarefa;
            }
            Obj::Arrastou { pagina, dx, dy } => return self.mover_escolhidos(pagina, dx, dy),
            Obj::Seta(x, y) => {
                let Some((pagina, _)) = &self.escolhidos else { return Task::none() };
                let pagina = *pagina;
                return self.mover_escolhidos(pagina, x * MM * self.zoom, y * MM * self.zoom);
            }
            Obj::Editar { pagina, objeto } => {
                let Some(TipoObjeto::Texto { texto, .. }) = self.objetos.get(&pagina)
                    .and_then(|o| o.lista.get(objeto)).map(|o| &o.tipo) else { return Task::none() };
                let texto = texto.trim_end().to_string();
                self.edicao = Some(Edicao { pagina, objeto, original: texto.clone(), texto });
                self.escolhidos = Some((pagina, BTreeSet::from([objeto])));
                self.status = "Enter grava · Esc desiste · a fonte do PDF precisa ter as letras".into();
                return crate::focar(id_edicao());
            }
            Obj::Texto(t) => if let Some(e) = &mut self.edicao { e.texto = t },
            Obj::Aplicar => return self.fechar_edicao(true),
            Obj::Cancelar => {
                if self.edicao.take().is_none() { self.escolhidos = None; }
                self.status.clear();
            }
            Obj::Tipografia(e) => return self.tipografia_escolhidos(e),
            Obj::Ordem(frente) => {
                let Some((pagina, s)) = self.escolhidos.take() else { return Task::none() };
                let lista: Vec<usize> = s.into_iter().collect();
                return self.editar(move |pdf| {
                    pdf.reordenar_objetos(pagina, &lista, frente)?;
                    Ok(format!("{} objeto(s) {}.", lista.len(), if frente { "trazido(s) para a frente" } else { "mandado(s) para trás" }))
                });
            }
            Obj::Excluir => {
                let Some((pagina, s)) = self.escolhidos.take() else { return Task::none() };
                let lista: Vec<usize> = s.into_iter().collect();
                return self.editar(|pdf| {
                    pdf.excluir_objetos(pagina, &lista)?;
                    Ok(format!("{} objeto(s) apagado(s) da página {pagina}.", lista.len()))
                });
            }
        }
        Task::none()
    }

    /// Grava a caixa de edição se o texto mudou (`gravar`) e a fecha.
    fn fechar_edicao(&mut self, gravar: bool) -> Task<Msg> {
        let Some(ed) = self.edicao.take() else { return Task::none() };
        if !gravar || ed.texto == ed.original { return Task::none(); }
        let tarefa = self.editar(|pdf| {
            pdf.editar_texto(ed.pagina, ed.objeto, &ed.texto)?;
            Ok("Texto trocado.".into())
        });
        if !self.ultima_edicao_ok {
            // Letra que a fonte não tem: a caixa volta com o texto, para corrigir.
            self.edicao = Some(ed);
        }
        tarefa
    }

    fn mover_escolhidos(&mut self, pagina: u32, dx: f32, dy: f32) -> Task<Msg> {
        let Some((p, s)) = &self.escolhidos else { return Task::none() };
        if *p != pagina { return Task::none(); }
        let Some(o) = self.objetos.get(&pagina) else { return Task::none() };
        let (px, py) = para_pagina(dx, dy, o.giro, self.zoom);
        let lista: Vec<usize> = s.iter().copied().collect();
        self.editar(|pdf| {
            pdf.mover_objetos(pagina, &lista, px, py)?;
            Ok(format!("Movido {:.1} × {:.1} mm.", px / MM, py / MM))
        })
    }

    fn descrever_escolha(&mut self) {
        let Some((p, s)) = &self.escolhidos else { return };
        let Some(o) = self.objetos.get(p) else { return };
        self.status = if s.len() > 1 {
            format!("{} objetos · arraste ou use as setas para mover · Delete apaga", s.len())
        } else {
            match s.first().and_then(|&k| o.lista.get(k)).map(|o| &o.tipo) {
                Some(TipoObjeto::Texto { fonte, tamanho, .. }) =>
                    format!("Texto · {fonte} {tamanho:.0} pt · clique duplo edita · setas movem 1 mm"),
                Some(TipoObjeto::Imagem { largura, altura }) =>
                    format!("Imagem · {largura} × {altura} px · arraste para mover · Delete apaga"),
                Some(TipoObjeto::Desenho) => "Desenho vetorial (borda, linha, forma) · arraste para mover · Estilo troca cor e espessura · Delete apaga".into(),
                Some(TipoObjeto::Grupo) => "Desenho agrupado · arraste para mover · Delete apaga".into(),
                None => String::new(),
            }
        };
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn arrasto_gruda_na_guia_e_na_grade() {
        let mut c = Camada { pagina: 1, caixas: vec![(Rect::new(Point::new(100.0, 100.0), Size::new(50.0, 20.0)), false)],
            escolhidos: vec![0], tamanho: (600.0, 800.0), zoom: 2.0, encaixe: [false, false, false], passo: 12.0, reguas: true, guias: vec![(true, 80.0)] };
        // Guia em x = 80 pt = 160 px: a borda esquerda (100 + 57) gruda nela.
        let (d, guias) = c.encaixar(Vector::new(57.0, 0.0));
        assert_eq!(d.x, 60.0);
        assert!(guias.iter().any(|g| g.vertical && g.nome == "guia"));
        // Grade de 12 pt (24 px) sem guia: y = 100 + 21 vai para 120.
        c.guias.clear();
        c.encaixe[2] = true;
        assert_eq!(c.encaixar(Vector::new(0.0, 21.0)).0.y, 20.0);
    }
}
