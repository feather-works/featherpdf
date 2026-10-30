//! Aba Comentar: destacar, sublinhar, riscar, nota, caixa de texto, desenho
//! à mão e carimbo, mais os painéis Comentários e Camadas.

use feather_ui::dialogo::Dialogo;
use feather_ui::tema;
use featherpdf_core::{AnotacaoNova, Marcacao};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::{self, Cursor};
use crate::compat::{event, canvas::{self, Frame, Geometry, Path, Stroke}};
use crate::{App, Ferramenta, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anotador { Destacar, Sublinhar, Riscar, Ondular, Substituir, Nota, Caixa, Desenhar, Carimbo }

pub const CARIMBOS: [&str; 8] = ["Aprovado", "Revisado", "Recebido", "Pago", "Conferido", "Rascunho", "Confidencial", "Reprovado"];

/// Anotação esperando o texto (nota, caixa de texto) ou o carimbo escolhido.
#[derive(Debug, Clone)]
pub struct Pendente { pagina: u32, tipo: Anotador, caixa: [f32; 4], texto: String, detalhe: String }

#[derive(Debug, Clone, Default)]
pub struct Comentarios { pub pendente: Option<Pendente>, pub carimbo: usize }

#[derive(Debug, Clone)]
pub enum Cm {
    Caixa { pagina: u32, a: Point, b: Point },
    Tinta { pagina: u32, pontos: Vec<Point> },
    Texto(String),
    Detalhe(String),
    Carimbo(usize),
    Gravar,
    Fechar,
    Apagar(u32, usize),
    Camada((u32, u16), bool),
    /// Carimbo com imagem (assinatura escaneada, logotipo): escolher o arquivo.
    EscolherImagem,
    ImagemEscolhida(Option<std::path::PathBuf>),
}

/// Carimbo desenhado como sai na página: borda dupla e letras na cor do sentido.
fn selo<'a>(titulo: &str, detalhe: &str, tamanho: f32, ativo: bool, largura: Length) -> Element<'a, Msg> {
    let [r, g, b] = featherpdf_core::cor_do_carimbo(titulo);
    let cor = Color::rgb(r, g, b);
    let negrito = Font::BOLD;
    let mut dentro = column![text(titulo.to_uppercase()).size(tamanho).font(negrito).color(cor)].align_x(Align::Center).spacing(2);
    if !detalhe.trim().is_empty() { dentro = dentro.push(text(detalhe.to_string()).size(tamanho * 0.45).color(cor)); }
    let borda = move |largura: f32, fundo: f32| move |_: &Theme| Style {
        background: Some(Background::Color(Color { a: fundo, ..cor })),
        border: Border { color: cor, width: largura, radius: [3.0; 4] }, ..Default::default()
    };
    container(container(dentro).padding([tamanho * 0.25, tamanho * 0.6]).width(Length::FILL).center_x(Length::FILL).style(borda(1.0, 0.0)))
        .padding(2).width(largura).style(borda(if ativo { 2.5 } else { 1.5 }, if ativo { 0.12 } else { 0.03 })).into()
}

pub fn id_texto() -> &'static str { "comentario-texto" }

fn autor() -> String {
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default()
}

/// Desenho à mão livre: junta os pontos do arrasto e manda ao soltar.
struct Mao { pagina: u32 }

impl canvas::Program<Msg> for Mao {
    type State = Option<Vec<Point>>;

    fn update(&self, estado: &mut Self::State, evento: canvas::Event, limites: Rect, cursor: Cursor) -> (event::Status, Option<Msg>) {
        match evento {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_in(limites) else { return (event::Status::Ignored, None) };
                *estado = Some(vec![p]);
                (event::Status::Captured, None)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => match (estado.as_mut(), cursor.position_from(limites.position())) {
                (Some(v), Some(p)) => {
                    if v.last().is_none_or(|u| u.distance(p) > 1.5) { v.push(p); }
                    (event::Status::Captured, None)
                }
                _ => (event::Status::Ignored, None),
            },
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => match estado.take() {
                Some(pontos) if pontos.len() > 1 => (event::Status::Captured, Some(Msg::Com(Cm::Tinta { pagina: self.pagina, pontos }))),
                _ => (event::Status::Ignored, None),
            },
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(&self, estado: &Self::State, renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        if let Some(v) = estado.as_ref().filter(|v| v.len() > 1) {
            let caminho = Path::new(|b| { b.move_to(v[0]); for p in &v[1..] { b.line_to(*p); } });
            f.stroke(&caminho, Stroke::default().with_color(Color::rgb(0.1, 0.35, 0.85)).with_width(2.0));
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, _: &Self::State, limites: Rect, cursor: Cursor) -> mouse::Interaction {
        if cursor.is_over(limites) { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }
}

impl App {
    pub fn anotador(&self) -> Option<Anotador> {
        match self.ferramenta { Ferramenta::Comentar(a) if self.aberto.is_some() => Some(a), _ => None }
    }

    /// Camada da ferramenta de comentar sobre a página `n`.
    pub fn camada_comentar(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        Some(match self.anotador()? {
            Anotador::Desenhar => canvas::Canvas::new(Mao { pagina: n }).width(l).height(a).into(),
            _ => crate::inserir::camada_arrastar(n, |pagina, a, b| Msg::Com(Cm::Caixa { pagina, a, b }), l, a),
        })
    }

    pub(crate) fn anotar(&mut self, pagina: u32, nova: AnotacaoNova, aviso: &'static str) -> Task<Msg> {
        let quem = autor();
        self.editar(move |pdf| pdf.anotar(pagina, &nova, &quem).map(|_| aviso.to_string()))
    }

    pub fn update_comentar(&mut self, m: Cm) -> Task<Msg> {
        match m {
            Cm::Caixa { pagina, a, b } => {
                let Some(tipo) = self.anotador() else { return Task::none() };
                let clique = a.distance(b) < 4.0;
                let z = self.zoom;
                // Clique sem arrastar: tamanho padrão a partir do ponto.
                let (l, al) = match tipo { Anotador::Caixa => (180.0, 50.0), Anotador::Carimbo => (150.0, 50.0), _ => (0.0, 0.0) };
                let b = if clique { Point::new(a.x + l * z, a.y + al * z) } else { b };
                let Some(caixa) = self.caixa_na_pagina(pagina, a, b) else { return Task::none() };
                match tipo {
                    Anotador::Destacar | Anotador::Sublinhar | Anotador::Riscar | Anotador::Ondular => {
                        let tipo_m = match tipo { Anotador::Destacar => Marcacao::Destacar, Anotador::Sublinhar => Marcacao::Sublinhar,
                            Anotador::Ondular => Marcacao::Ondulado, _ => Marcacao::Riscar };
                        let Some(aberto) = &self.aberto else { return Task::none() };
                        // Clique numa palavra: a caixa vira um quadradinho em volta do ponto.
                        let r = if clique { [caixa[0] - 2.0, caixa[1] - 4.0, caixa[0] + 2.0, caixa[1] + 4.0] } else { caixa };
                        return match aberto.pdf.caixas_de_texto(pagina, r) {
                            Ok((caixas, _)) if caixas.is_empty() => { self.status = "Arraste por cima do texto que quer marcar.".into(); Task::none() }
                            Ok((caixas, texto)) => self.anotar(pagina, AnotacaoNova::Marcacao { tipo: tipo_m, caixas, texto }, "Marcado. Ctrl+Z desfaz."),
                            Err(e) => { self.status = e; Task::none() }
                        };
                    }
                    Anotador::Substituir => {
                        let Some(aberto) = &self.aberto else { return Task::none() };
                        if aberto.pdf.caixas_de_texto(pagina, caixa).is_ok_and(|(c, _)| c.is_empty()) {
                            self.status = "Arraste por cima do texto que quer substituir.".into();
                            return Task::none();
                        }
                        self.comentarios.pendente = Some(Pendente { pagina, tipo, caixa, texto: String::new(), detalhe: String::new() });
                        return crate::focar(id_texto());
                    }
                    Anotador::Nota | Anotador::Caixa | Anotador::Carimbo => {
                        let detalhe = if tipo == Anotador::Carimbo { format!("{} · {}", autor(), featherpdf_core::hoje()) } else { String::new() };
                        self.comentarios.pendente = Some(Pendente { pagina, tipo, caixa, texto: String::new(), detalhe });
                        return crate::focar(id_texto());
                    }
                    Anotador::Desenhar => {}
                }
            }
            Cm::Tinta { pagina, pontos } => {
                let tracos: Vec<(f32, f32)> = pontos.iter().filter_map(|p| self.ponto_na_pagina(pagina, *p)).collect();
                return self.anotar(pagina, AnotacaoNova::Tinta { tracos: vec![tracos] }, "Desenho anotado. Ctrl+Z desfaz.");
            }
            Cm::Texto(t) => if let Some(p) = &mut self.comentarios.pendente { p.texto = t },
            Cm::Detalhe(t) => if let Some(p) = &mut self.comentarios.pendente { p.detalhe = t },
            Cm::Carimbo(i) => self.comentarios.carimbo = i,
            Cm::Fechar => self.comentarios.pendente = None,
            Cm::Gravar => {
                let Some(p) = self.comentarios.pendente.take() else { return Task::none() };
                if p.tipo == Anotador::Substituir {
                    let quem = autor();
                    return self.editar(move |pdf| {
                        let (caixas, texto) = pdf.caixas_de_texto(p.pagina, p.caixa)?;
                        pdf.substituir_texto(p.pagina, caixas, texto, &p.texto, &quem).map(|_| "Trecho riscado com o texto novo ao lado. Ctrl+Z desfaz.".into())
                    });
                }
                let nova = match p.tipo {
                    Anotador::Nota => AnotacaoNova::Nota { ponto: (p.caixa[0], p.caixa[3]), texto: p.texto },
                    Anotador::Caixa => AnotacaoNova::CaixaTexto { caixa: p.caixa, texto: p.texto },
                    _ => {
                        let titulo = if p.texto.trim().is_empty() { CARIMBOS[self.comentarios.carimbo].to_string() } else { p.texto.trim().to_string() };
                        AnotacaoNova::Carimbo { caixa: p.caixa, titulo, detalhe: p.detalhe }
                    }
                };
                return self.anotar(p.pagina, nova, "Comentário criado. Ctrl+Z desfaz.");
            }
            Cm::Apagar(pagina, i) => return self.editar(|pdf| pdf.apagar_anotacao(pagina, i).map(|_| "Comentário apagado. Ctrl+Z desfaz.".into())),
            Cm::EscolherImagem => return Task::perform(async {
                rfd::AsyncFileDialog::new().add_filter("Imagem", &crate::juntar::EXTENSOES_IMAGEM).pick_file().await.map(|h| h.path().to_path_buf())
            }, |c| Msg::Com(Cm::ImagemEscolhida(c))),
            Cm::ImagemEscolhida(None) => {}
            Cm::ImagemEscolhida(Some(caminho)) => {
                let Some(p) = self.comentarios.pendente.take() else { return Task::none() };
                let lida = featherpdf_io::ler_bytes(&caminho).and_then(crate::inserir::ler_imagem);
                let (imagem, l, a) = match lida { Ok(v) => v, Err(e) => { self.status = e; return Task::none(); } };
                let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let quem = autor();
                return self.editar(move |pdf| pdf.carimbo_imagem(p.pagina, p.caixa, std::sync::Arc::unwrap_or_clone(imagem), l, a, &nome, &quem)
                    .map(|_| "Carimbo com imagem criado. Ctrl+Z desfaz.".into()));
            }
            Cm::Camada(id, v) => return self.editar(|pdf| pdf.mostrar_camada(id, v)
                .map(|_| "Camada trocada; o PDF salvo abre assim em qualquer leitor.".into())),
        }
        Task::none()
    }

    /// Caixa que pede o texto da nota, da caixa de texto ou o carimbo.
    pub fn com_dialogo_comentario<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(p) = &self.comentarios.pendente else { return crate::inicio::sem_camada(base) };
        let campo = |dica: &'static str, v: &str, f: fn(String) -> Cm| text_input(dica, v.to_owned())
            .on_input(move |t| Msg::Com(f(t))).on_submit(Msg::Com(Cm::Gravar)).padding([6, 8]).size(13);
        let (titulo, icone, corpo, pronto): (&str, &str, Element<'a, Msg>, bool) = match p.tipo {
            Anotador::Carimbo => {
                let titulo = if p.texto.trim().is_empty() { CARIMBOS[self.comentarios.carimbo].to_string() } else { p.texto.trim().to_string() };
                let mut grade = column![].spacing(8);
                for (k, linha) in CARIMBOS.chunks(4).enumerate() {
                    let mut r = row![].spacing(8);
                    for (j, nome) in linha.iter().enumerate() {
                        let i = k * 4 + j;
                        let ativo = i == self.comentarios.carimbo && p.texto.trim().is_empty();
                        r = r.push(button(selo(nome, "", 11.0, ativo, Length::FILL)).padding(0).width(Length::FILL)
                            .style(|_, _| Style::default()).on_press(Msg::Com(Cm::Carimbo(i))));
                    }
                    grade = grade.push(r);
                }
                let muted = tema::COR_TEXTO_MUTED();
                let atalho = |rotulo: &'static str, valor: String| button(text(rotulo).size(12)).padding([3, 8])
                    .style(tema::estilo_botao_pequeno(p.detalhe == valor)).on_press(Msg::Com(Cm::Detalhe(valor)));
                let hoje = featherpdf_core::hoje();
                ("Carimbo", "tag", column![
                    grade,
                    campo("Ou escreva o seu (ex.: EM ANÁLISE)", &p.texto, Cm::Texto).id(id_texto()),
                    text("Linha de baixo").size(12).color(muted),
                    row![
                        atalho("Nome e data", format!("{} · {hoje}", autor())),
                        atalho("Só a data", hoje),
                        atalho("Nenhuma", String::new()),
                    ].spacing(6),
                    campo("Texto da linha de baixo", &p.detalhe, Cm::Detalhe),
                    row![button(text("Usar uma imagem (PNG/JPG)…").size(12)).padding([4, 10]).style(tema::estilo_botao_pequeno(false))
                        .on_press(Msg::Com(Cm::EscolherImagem)),
                        text("logotipo, assinatura escaneada").size(12).color(muted)].spacing(8).align_y(Align::Center),
                    text("Prévia").size(12).color(muted),
                    container(selo(&titulo, &p.detalhe, 22.0, false, Length::Shrink)).width(Length::FILL).center_x(Length::FILL),
                ].spacing(10).into(), true)
            }
            Anotador::Nota => ("Nota", "message-circle", campo("Escreva a nota", &p.texto, Cm::Texto).id(id_texto()).into(), !p.texto.trim().is_empty()),
            Anotador::Substituir => ("Substituir texto", "edit", campo("Texto que entra no lugar", &p.texto, Cm::Texto).id(id_texto()).into(), !p.texto.trim().is_empty()),
            _ => ("Caixa de texto", "text-box", campo("Texto da caixa", &p.texto, Cm::Texto).id(id_texto()).into(), !p.texto.trim().is_empty()),
        };
        let largura = if p.tipo == Anotador::Carimbo { 560.0 } else { 460.0 };
        Dialogo::new(titulo, corpo).icone(icone).largura(largura)
            .subtitulo("Fica como comentário do PDF: aparece em qualquer leitor e sai no painel Comentários.")
            .primario("Criar", pronto.then_some(Msg::Com(Cm::Gravar)))
            .ao_fechar(Msg::Com(Cm::Fechar)).topo(200.0).sobre(base)
    }
}
