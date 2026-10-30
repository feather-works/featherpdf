//! Aba Editar: inserir texto, imagem, forma, ícone e link na página, e alinhar
//! os objetos escolhidos. Formas, ícones e cores são os mesmos do FeatherSheet:
//! o catálogo comum `feather-shapes`, a galeria Tabler com busca e a paleta com
//! "Mais cores…" (`feather_ui::seletor_cor`). Arrastar na página marca a caixa;
//! um clique usa o tamanho padrão. O objeto novo já sai escolhido na ferramenta
//! Editar, para mover ou apagar. Cada inserção é um Ctrl+Z.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use feather_draw::{Alinhar, Caminho, Cena, Comando, Eixo, Estilo as EstiloCena, Figura, Ponto, Retangulo};
use feather_shapes::icones::{self, Icone};
use feather_shapes::TipoForma;
use feather_ui::dialogo::{self as dlg, Dialogo};
use feather_ui::seletor_cor::{self, Evento as EventoCor};
use feather_ui::{componentes, icones as ic, tema};
use featherpdf_core::{DesenhoNovo, Destino, EstiloDesenho, EstiloTexto, ImagemNova, TipoObjeto};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::{self, Cursor};
use crate::compat::{event, canvas::{self, Frame, Geometry, Path, Stroke}};
use crate::edicao::{na_tela, para_pagina};
use crate::inicio::VERMELHO;
use crate::{App, Ferramenta, Msg};

/// O que a ferramenta de inserir põe na página (forma e ícone pelo id do catálogo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insercao { Texto, Imagem, Forma(&'static str), Icone(&'static str), Link }

/// Menus suspensos da faixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu { Formas, Icones, Preenchimento, Contorno, CorTexto }

/// As cores de conteúdo do FeatherSheet (tema do Office).
const PALETA: [&str; 12] = [
    "#4472c4", "#ed7d31", "#a5a5a5", "#ffc000", "#5b9bd5", "#70ad47",
    "#264478", "#9e480e", "#000000", "#ffffff", "#c00000", "#7030a0",
];
const ESPESSURAS: [f32; 7] = [0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0];
const TAMANHOS: [f32; 10] = [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 18.0, 24.0, 36.0, 48.0];
/// Ícones mostrados de uma vez na galeria; a busca acha os outros.
const MAX_ICONES: usize = 168;

/// Posição do mouse na janela (x e y em f32), para abrir o menu perto do botão
/// clicado sem mandar uma mensagem a cada movimento.
static PONTEIRO: AtomicU64 = AtomicU64::new(0);
static JANELA: AtomicU64 = AtomicU64::new(0);

fn guardar(a: &AtomicU64, x: f32, y: f32) { a.store((x.to_bits() as u64) << 32 | y.to_bits() as u64, Ordering::Relaxed); }
/// Posição do cursor e tamanho da janela, para os menus flutuantes.
pub(crate) fn ponteiro() -> (f32, f32) { ler(&PONTEIRO) }
pub(crate) fn janela() -> (f32, f32) { ler(&JANELA) }

fn ler(a: &AtomicU64) -> (f32, f32) {
    let v = a.load(Ordering::Relaxed);
    (f32::from_bits((v >> 32) as u32), f32::from_bits(v as u32))
}

/// Chamado pelo `listen_with`: guarda ponteiro e tamanho da janela; nunca vira mensagem.
pub fn observar(evento: &Event) {
    match evento {
        Event::CursorMoved(p) => guardar(&PONTEIRO, p.x, p.y),
        Event::Resized(t) => guardar(&JANELA, t.width, t.height),
        _ => {}
    }
}

#[derive(Debug, Clone)]
pub enum Ins {
    Ferramenta(Insercao),
    ImagemLida(Result<(Arc<ImagemNova>, u32, u32), String>),
    /// Ctrl+V: a imagem da área de transferência (`None` = não havia imagem).
    Colar,
    Colada(Option<(Arc<ImagemNova>, u32, u32)>),
    /// Arrasto terminado na página, em pixels da tela a partir do canto dela.
    Colocar { pagina: u32, a: Point, b: Point },
    Texto(String),
    GravarTexto,
    Link(String),
    GravarLink,
    Fechar,
    AbrirMenu(Menu),
    FecharMenu,
    BuscaIcones(String),
    /// Cor da paleta (`None` = sem preenchimento / sem contorno).
    Cor(Option<String>),
    MaisCores,
    Seletor(EventoCor),
    Espessura(f32),
    Tamanho(f32),
    Negrito,
    Alinhar(Alinhar),
    Distribuir(Eixo),
    /// Liga/desliga o encaixe: 0 objetos, 1 margem e centro da página, 2 grade.
    Encaixe(usize),
    /// Passo da grade, índice em `edicao::PASSOS_GRADE`.
    PassoGrade(usize),
    Reguas,
    /// Guia puxada da régua: vertical?, posição em pontos da página vista.
    Guia(bool, f32),
    LimparGuias,
    /// Alinhar um ou mais objetos à página (verdadeiro) ou entre si.
    AlinharPagina(bool),
}

pub struct Insercoes {
    pub preenchimento: Option<String>,
    pub contorno: Option<String>,
    pub espessura: f32,
    pub cor_texto: String,
    pub tamanho: f32,
    pub negrito: bool,
    /// Imagem escolhida, à espera do clique na página (com largura e altura em pixels).
    pub imagem: Option<(Arc<ImagemNova>, u32, u32)>,
    /// Largura (pt) do clique para a imagem esperando; `None` = 96 dpi (a assinatura usa ~5 cm).
    pub largura_imagem: Option<f32>,
    /// Caixa de texto novo aberta: página, ponto na tela e texto.
    pub texto: Option<(u32, Point, String)>,
    /// Diálogo do link: página, caixa na página e o que foi digitado.
    pub link: Option<(u32, [f32; 4], String)>,
    /// Menu aberto e onde (canto de cima à esquerda, na janela).
    pub menu: Option<(Menu, (f32, f32))>,
    pub busca_icones: String,
    pub mais_cores: bool,
    /// Encaixe ao arrastar: objetos, margem/centro da página, grade.
    pub encaixe: [bool; 3],
    /// Passo da grade em pontos.
    pub passo_grade: f32,
    pub reguas: bool,
    /// Guias das réguas, valem para todas as páginas: vertical?, posição em pontos.
    pub guias: Vec<(bool, f32)>,
    pub alinhar_pagina: bool,
    hsva: [f32; 4],
    hex: String,
}

impl Default for Insercoes {
    fn default() -> Self {
        Self {
            preenchimento: Some(PALETA[0].into()), contorno: Some(PALETA[6].into()), espessura: 1.0,
            cor_texto: PALETA[8].into(), tamanho: 12.0, negrito: false,
            imagem: None, largura_imagem: None, texto: None, link: None, menu: None, busca_icones: String::new(),
            mais_cores: false, encaixe: [true, true, false], passo_grade: crate::edicao::PASSOS_GRADE[1].1, reguas: false, guias: Vec::new(), alinhar_pagina: false, hsva: [0.0, 0.0, 0.0, 1.0], hex: String::new(),
        }
    }
}

pub fn id_texto_novo() -> &'static str { "featherpdf.texto_novo" }
fn id_link() -> &'static str { "featherpdf.link" }
fn id_busca_icones() -> &'static str { "featherpdf.busca_icones" }

fn cor_hex(hex: &str) -> Color {
    let [r, g, b, _] = seletor_cor::rgba_de(hex);
    Color::rgb8(r, g, b)
}

fn rgb(hex: &str) -> [f32; 3] {
    let [r, g, b, _] = seletor_cor::rgba_de(hex);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn para_path(c: &Caminho) -> Path {
    let pt = |p: Ponto| Point::new(p.x, p.y);
    Path::new(|b| {
        for cmd in &c.comandos {
            match *cmd {
                Comando::Mover(p) => b.move_to(pt(p)),
                Comando::Linha(p) => b.line_to(pt(p)),
                Comando::Quadratica { controle, fim } => b.quadratic_curve_to(pt(controle), pt(fim)),
                Comando::Cubica { controle1, controle2, fim } => b.bezier_curve_to(pt(controle1), pt(controle2), pt(fim)),
                Comando::Fechar => b.close(),
            }
        }
    })
}

/// Linhas do ícone (`icones::tracos`) como caminho.
fn caminho_do_icone(linhas: &[Vec<[f32; 2]>]) -> Caminho {
    let mut c = Caminho::novo();
    for l in linhas.iter().filter(|l| l.len() > 1) {
        for (i, p) in l.iter().enumerate() {
            let p = Ponto { x: p[0], y: p[1] };
            c.adicionar(if i == 0 { Comando::Mover(p) } else { Comando::Linha(p) });
        }
    }
    c
}

/// Espelha o caminho na caixa l × a: linha arrastada para cima ou para a esquerda.
fn espelhar(c: Caminho, l: f32, a: f32, ex: bool, ey: bool) -> Caminho {
    if !ex && !ey { return c; }
    let f = |p: Ponto| Ponto { x: if ex { l - p.x } else { p.x }, y: if ey { a - p.y } else { p.y } };
    let mut novo = Caminho::novo();
    for cmd in c.comandos {
        novo.adicionar(match cmd {
            Comando::Mover(p) => Comando::Mover(f(p)),
            Comando::Linha(p) => Comando::Linha(f(p)),
            Comando::Quadratica { controle, fim } => Comando::Quadratica { controle: f(controle), fim: f(fim) },
            Comando::Cubica { controle1, controle2, fim } => Comando::Cubica { controle1: f(controle1), controle2: f(controle2), fim: f(fim) },
            Comando::Fechar => Comando::Fechar,
        });
    }
    novo
}

/// Miniatura de uma forma do catálogo (menu Formas), como no FeatherSheet.
struct Miniatura(TipoForma);

impl canvas::Program<Msg> for Miniatura {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, b: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, b.size());
        let r = Retangulo { x: 3.0, y: 3.0, largura: b.width - 6.0, altura: b.height - 6.0 };
        if let Some(g) = feather_shapes::forma::geometria(self.0, &[], r) {
            let traco = Stroke::default().with_color(tema::COR_TEXTO()).with_width(1.3).with_line_join(canvas::LineJoin::Round);
            f.stroke(&para_path(&g.preenchida), traco.clone());
            f.stroke(&para_path(&g.tracos), traco);
        }
        vec![f.into_geometry()]
    }
}

/// Miniatura de um ícone Tabler (galeria de ícones).
struct MiniIcone(&'static Icone);

impl canvas::Program<Msg> for MiniIcone {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, b: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, b.size());
        let (linhas, escala) = icones::tracos(self.0, 0.0, 0.0, b.width, b.height);
        f.stroke(&para_path(&caminho_do_icone(&linhas)), Stroke::default().with_color(tema::COR_TEXTO())
            .with_width((1.5 * escala).max(0.8)).with_line_join(canvas::LineJoin::Round).with_line_cap(canvas::LineCap::Round));
        vec![f.into_geometry()]
    }
}

/// Camada da página com a ferramenta de inserir: marca a caixa arrastando.
struct Colocar { pagina: u32, linha: bool, msg: fn(u32, Point, Point) -> Msg }

/// Camada que marca uma caixa arrastando sobre a página `n` e manda `msg` ao soltar.
pub fn camada_arrastar<'a>(n: u32, msg: fn(u32, Point, Point) -> Msg, l: f32, a: f32) -> Element<'a, Msg> {
    canvas::Canvas::new(Colocar { pagina: n, linha: false, msg }).width(l).height(a).into()
}

impl canvas::Program<Msg> for Colocar {
    type State = Option<(Point, Point)>;

    fn update(&self, estado: &mut Self::State, evento: canvas::Event, limites: Rect, cursor: Cursor)
        -> (event::Status, Option<Msg>)
    {
        match evento {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_in(limites) else { return (event::Status::Ignored, None) };
                *estado = Some((p, p));
                (event::Status::Captured, None)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => match (estado.as_mut(), cursor.position_from(limites.position())) {
                (Some((_, b)), Some(p)) => {
                    *b = Point::new(p.x.clamp(0.0, limites.width), p.y.clamp(0.0, limites.height));
                    (event::Status::Captured, None)
                }
                _ => (event::Status::Ignored, None),
            },
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => match estado.take() {
                Some((a, b)) => (event::Status::Captured, Some((self.msg)(self.pagina, a, b))),
                None => (event::Status::Ignored, None),
            },
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(&self, estado: &Self::State, renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        if let Some((a, b)) = *estado {
            let traco = Stroke::default().with_color(VERMELHO).with_width(1.5);
            if self.linha {
                f.stroke(&Path::line(a, b), traco);
            } else {
                let r = Rect::new(Point::new(a.x.min(b.x), a.y.min(b.y)), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()));
                f.fill_rectangle(r.position(), r.size(), Color { a: 0.08, ..VERMELHO });
                f.stroke(&Path::rectangle(r.position(), r.size()), traco);
            }
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, _: &Self::State, limites: Rect, cursor: Cursor) -> mouse::Interaction {
        if cursor.is_over(limites) { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }
}

impl App {
    pub fn insercao(&self) -> Option<Insercao> {
        match self.ferramenta { Ferramenta::Inserir(i) if self.aberto.is_some() => Some(i), _ => None }
    }

    /// Camada de inserir e a caixa do texto novo sobre a página `n`.
    pub fn camada_inserir<'a>(&'a self, n: u32, pilha: Stack<'a, Msg>, l: f32, a: f32) -> Stack<'a, Msg> {
        let Some(i) = self.insercao() else { return pilha };
        let linha = matches!(i, Insercao::Forma(id) if TipoForma::pelo_id(id).is_some_and(TipoForma::so_traco));
        let mut pilha = pilha.push(canvas::Canvas::new(Colocar { pagina: n, linha, msg: |pagina, a, b| Msg::Ins(Ins::Colocar { pagina, a, b }) }).width(l).height(a));
        if let Some((p, ponto, texto)) = &self.ins.texto {
            if *p == n {
                let tamanho = (self.ins.tamanho * self.zoom).clamp(8.0, 96.0);
                let cor = cor_hex(&self.ins.cor_texto);
                let negrito = self.ins.negrito;
                let caixa = text_input("Digite e tecle Enter", texto).id(id_texto_novo())
                    .on_input(|t| Msg::Ins(Ins::Texto(t))).on_submit(Msg::Ins(Ins::GravarTexto))
                    .size(tamanho).padding([0, 2]).width((l - ponto.x).max(120.0).min(520.0))
                    .font(if negrito { Font::BOLD } else { Font::DEFAULT })
                    .style(move |tema, status| {
                        let mut s = campo_padrao(tema, status);
                        s.background = Some(Background::Color(Color::rgba(1.0, 1.0, 1.0, 0.9)));
                        s.text_color = Some(cor);
                        s.border = Border { color: VERMELHO, width: 1.0, radius: [2.0; 4] };
                        s
                    });
                pilha = pilha.push(column![
                    Space::with_height(ponto.y.max(0.0)),
                    row![Space::with_width(ponto.x.max(0.0)), container(caixa)],
                ]);
            }
        }
        pilha
    }

    /// Ponto da tela (a partir do canto da página) no espaço da página.
    pub(crate) fn ponto_na_pagina(&self, pagina: u32, p: Point) -> Option<(f32, f32)> {
        let ([x0, y0, x1, y1], giro) = self.aberto.as_ref()?.pdf.geometria(pagina).ok()?;
        let (u, v) = (p.x / self.zoom, p.y / self.zoom);
        Some(match giro {
            90 => (x0 + v, y0 + u),
            180 => (x1 - u, y0 + v),
            270 => (x1 - v, y1 - u),
            _ => (x0 + u, y1 - v),
        })
    }

    pub(crate) fn caixa_na_pagina(&self, pagina: u32, a: Point, b: Point) -> Option<[f32; 4]> {
        let (p, q) = (self.ponto_na_pagina(pagina, a)?, self.ponto_na_pagina(pagina, b)?);
        Some([p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1)])
    }

    /// Grava com um Ctrl+Z e deixa o objeto novo (o último da página) escolhido.
    fn inserir(&mut self, pagina: u32, f: impl FnOnce(&mut featherpdf_core::DocumentoPdf) -> Result<String, String>) -> Task<Msg> {
        let tarefa = self.editar(f);
        if self.ultima_edicao_ok {
            self.ferramenta = Ferramenta::Editar;
            self.ler_objetos_sempre(&[pagina]);
            if let Some(k) = self.objetos.get(&pagina).and_then(|o| o.lista.len().checked_sub(1)) {
                self.escolhidos = Some((pagina, BTreeSet::from([k])));
            }
        }
        tarefa
    }

    /// Cor do menu de cor aberto.
    fn cor_do_menu(&self, m: Menu) -> Option<String> {
        match m {
            Menu::Preenchimento => self.ins.preenchimento.clone(),
            Menu::Contorno => self.ins.contorno.clone(),
            _ => Some(self.ins.cor_texto.clone()),
        }
    }

    /// Com desenhos escolhidos na ferramenta Editar, a cor ou espessura vale para eles (um Ctrl+Z).
    fn estilizar_escolhidos(&mut self, estilo: EstiloDesenho) -> Task<Msg> {
        if self.ferramenta != Ferramenta::Editar { return Task::none(); }
        let Some((pagina, escolhidos)) = self.escolhidos.clone() else { return Task::none() };
        let tem_desenho = self.objetos.get(&pagina)
            .is_some_and(|o| escolhidos.iter().any(|&k| o.lista.get(k).is_some_and(|ob| ob.tipo == TipoObjeto::Desenho)));
        if !tem_desenho { return Task::none(); }
        let indices: Vec<usize> = escolhidos.iter().copied().collect();
        let tarefa = self.editar(|pdf| {
            let n = pdf.estilizar_desenhos(pagina, &indices, &estilo)?;
            Ok(format!("{n} desenho(s) com estilo novo."))
        });
        self.escolhidos = Some((pagina, escolhidos));
        self.ler_objetos_sempre(&[pagina]);
        tarefa
    }

    pub fn tem_texto_escolhido(&self) -> bool {
        self.escolhidos.as_ref().is_some_and(|(pagina, e)| self.objetos.get(pagina)
            .is_some_and(|o| e.iter().any(|&k| o.lista.get(k).is_some_and(|ob| matches!(ob.tipo, TipoObjeto::Texto { .. })))))
    }

    /// Tamanho, cor e espaçamento nos textos escolhidos (um Ctrl+Z).
    pub fn tipografia_escolhidos(&mut self, estilo: EstiloTexto) -> Task<Msg> {
        if !self.tem_texto_escolhido() { return Task::none(); }
        let Some((pagina, escolhidos)) = self.escolhidos.clone() else { return Task::none() };
        let indices: Vec<usize> = escolhidos.iter().copied().collect();
        let tarefa = self.editar(|pdf| {
            let n = pdf.estilizar_texto(pagina, &indices, &estilo)?;
            Ok(format!("{n} texto(s) com estilo novo."))
        });
        self.escolhidos = Some((pagina, escolhidos));
        self.ler_objetos_sempre(&[pagina]);
        tarefa
    }

    /// A cor escolhida no menu aberto, como estilo de desenho.
    fn estilo_da_cor(&self, cor: &Option<String>) -> EstiloDesenho {
        let c = cor.as_deref().map(rgb);
        match self.ins.menu.map(|m| m.0) {
            Some(Menu::Preenchimento) => EstiloDesenho { preenchimento: Some(c), ..Default::default() },
            Some(Menu::Contorno) => EstiloDesenho { contorno: Some(c), ..Default::default() },
            _ => EstiloDesenho::default(),
        }
    }

    fn por_cor(&mut self, cor: Option<String>) {
        match self.ins.menu.map(|m| m.0) {
            Some(Menu::Preenchimento) => self.ins.preenchimento = cor,
            Some(Menu::Contorno) => self.ins.contorno = cor,
            Some(Menu::CorTexto) => if let Some(c) = cor { self.ins.cor_texto = c },
            _ => {}
        }
    }

    pub fn update_ins(&mut self, msg: Ins) -> Task<Msg> {
        match msg {
            Ins::Ferramenta(Insercao::Imagem) => {
                self.ins.menu = None;
                return Task::perform(async {
                    let arquivo = rfd::AsyncFileDialog::new()
                        .add_filter("Imagem", &["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"]).pick_file().await?;
                    let bytes = arquivo.read().await;
                    Some(tokio::task::spawn_blocking(move || ler_imagem(bytes)).await
                        .unwrap_or_else(|e| Err(format!("Falha ao ler a imagem: {e}"))))
                }, |r| match r { Some(r) => Msg::Ins(Ins::ImagemLida(r)), None => Msg::Ins(Ins::Fechar) });
            }
            Ins::Ferramenta(i) => {
                self.ins.texto = None;
                self.ins.menu = None;
                self.escolhidos = None;
                self.edicao = None;
                self.ferramenta = Ferramenta::Inserir(i);
                self.status = match i {
                    Insercao::Texto => "Clique na página onde o texto começa.".into(),
                    Insercao::Link => "Arraste sobre a área que vai ser clicável.".into(),
                    _ => "Arraste para marcar o tamanho, ou clique para o tamanho padrão.".into(),
                };
            }
            Ins::ImagemLida(Ok(imagem)) => {
                self.status = format!("Imagem {} × {} px: clique na página para pôr, ou arraste para escolher a largura.", imagem.1, imagem.2);
                self.ins.imagem = Some(imagem);
                self.ins.largura_imagem = None;
                self.ferramenta = Ferramenta::Inserir(Insercao::Imagem);
            }
            Ins::ImagemLida(Err(e)) => self.status = e,
            Ins::Colar => return Task::perform(tokio::task::spawn_blocking(imagem_da_area_de_transferencia),
                |r| Msg::Ins(Ins::Colada(r.ok().flatten()))),
            Ins::Colada(None) => self.status = "A área de transferência não tem imagem.".into(),
            Ins::Colada(Some((imagem, w, h))) => {
                self.ins.largura_imagem = None;
                // No meio da página atual, a 96 dpi, até metade da largura (como um clique da ferramenta Imagem).
                let pagina = self.pagina;
                let Some(&(pl, pa)) = self.aberto.as_ref().and_then(|a| a.tamanhos.get(pagina as usize - 1)) else { return Task::none() };
                let z = self.zoom;
                let largura = (w as f32 * 0.75).min(pl / 2.0);
                let altura = largura * h as f32 / w as f32;
                let inicio = Point::new((pl - largura) / 2.0 * z, ((pa - altura) / 2.0).max(0.0) * z);
                let fim = Point::new(inicio.x + largura * z, inicio.y + altura * z);
                let Some(caixa) = self.caixa_na_pagina(pagina, inicio, fim) else { return Task::none() };
                let imagem = (*imagem).clone();
                return self.inserir(pagina, |pdf| { pdf.inserir_imagem(pagina, caixa, imagem)?; Ok("Imagem colada. Arraste para mover; Ctrl+Z desfaz.".into()) });
            }
            Ins::Colocar { pagina, a, b } => return self.colocar(pagina, a, b),
            Ins::Texto(t) => if let Some(e) = &mut self.ins.texto { e.2 = t },
            Ins::GravarTexto => {
                let Some((pagina, ponto, texto)) = self.ins.texto.take() else { return Task::none() };
                if texto.trim().is_empty() { return Task::none(); }
                let Some((x, y)) = self.ponto_na_pagina(pagina, ponto) else { return Task::none() };
                let (tamanho, negrito, cor) = (self.ins.tamanho, self.ins.negrito, rgb(&self.ins.cor_texto));
                let tarefa = self.inserir(pagina, |pdf| {
                    pdf.inserir_texto(pagina, x, y, &texto, tamanho, negrito, cor)?;
                    Ok("Texto inserido · clique duplo edita".into())
                });
                // Letra fora da Helvetica: a caixa volta com o texto para corrigir.
                if !self.ultima_edicao_ok {
                    self.ins.texto = Some((pagina, ponto, texto));
                    return Task::batch([tarefa, crate::focar(id_texto_novo())]);
                }
                return tarefa;
            }
            Ins::Link(t) => if let Some(l) = &mut self.ins.link { l.2 = t },
            Ins::GravarLink => {
                let Some((pagina, caixa, alvo)) = self.ins.link.take() else { return Task::none() };
                let destino = match alvo.trim().parse::<u32>() { Ok(n) => Destino::Pagina(n), Err(_) => Destino::Endereco(alvo.clone()) };
                let tarefa = self.editar(|pdf| { pdf.inserir_link(pagina, caixa, destino)?; Ok("Link criado.".into()) });
                if !self.ultima_edicao_ok { self.ins.link = Some((pagina, caixa, alvo)); }
                return tarefa;
            }
            Ins::Fechar => {
                self.ins.texto = None;
                self.ins.link = None;
            }
            Ins::AbrirMenu(m) => {
                if self.ins.menu.is_some_and(|(atual, _)| atual == m) {
                    self.ins.menu = None;
                    return Task::none();
                }
                let (x, y) = ler(&PONTEIRO);
                self.ins.menu = Some((m, ((x - 30.0).max(8.0), y + 26.0)));
                self.ins.mais_cores = false;
                if let Some(c) = self.cor_do_menu(m) {
                    self.ins.hsva = seletor_cor::rgba_para_hsva(seletor_cor::rgba_de(&c), self.ins.hsva[0]);
                    self.ins.hex = c.trim_start_matches('#').into();
                }
                if m == Menu::Icones { return crate::focar(id_busca_icones()); }
            }
            Ins::FecharMenu => { self.ins.menu = None; self.ins.mais_cores = false; }
            Ins::BuscaIcones(t) => self.ins.busca_icones = t,
            Ins::Cor(c) => {
                if let (Some((Menu::CorTexto, _)), Some(h)) = (self.ins.menu, c.as_deref()) {
                    let cor = rgb(h);
                    self.por_cor(c);
                    self.ins.menu = None;
                    return self.tipografia_escolhidos(EstiloTexto { cor: Some(cor), ..Default::default() });
                }
                let estilo = self.estilo_da_cor(&c);
                self.por_cor(c);
                self.ins.menu = None;
                self.ins.mais_cores = false;
                if estilo != EstiloDesenho::default() { return self.estilizar_escolhidos(estilo); }
            }
            Ins::MaisCores => self.ins.mais_cores = !self.ins.mais_cores,
            Ins::Seletor(e) => {
                let escolhida = matches!(e, EventoCor::Escolhida(_));
                let hsva = &mut self.ins.hsva;
                let do_hsva = |h: [f32; 4]| {
                    let [r, g, b] = seletor_cor::hsv_para_rgb(h[0], h[1], h[2]);
                    seletor_cor::hex_de([r, g, b, 255])
                };
                let cor = match e {
                    EventoCor::Fechar => { self.ins.mais_cores = false; return Task::none(); }
                    EventoCor::Escolhida(h) if h.is_empty() => None,
                    EventoCor::Escolhida(h) => { *hsva = seletor_cor::rgba_para_hsva(seletor_cor::rgba_de(&h), hsva[0]); Some(h) }
                    EventoCor::SaturacaoBrilho(s, v) => { hsva[1] = s.clamp(0.0, 1.0); hsva[2] = v.clamp(0.0, 1.0); Some(do_hsva(*hsva)) }
                    EventoCor::Matiz(h) => { hsva[0] = h.rem_euclid(360.0); Some(do_hsva(*hsva)) }
                    // shortcut: o PDF inserido é opaco; a transparência pede um ExtGState, fazer quando pedirem.
                    EventoCor::Alfa(_) => return Task::none(),
                    EventoCor::TextoHex(v) => {
                        self.ins.hex = v.clone();
                        if !seletor_cor::hex_valido(&v) { return Task::none(); }
                        let h = format!("#{}", v.trim().trim_start_matches('#').to_lowercase());
                        *hsva = seletor_cor::rgba_para_hsva(seletor_cor::rgba_de(&h), hsva[0]);
                        Some(h)
                    }
                    EventoCor::Canal(i, v) => {
                        let (Ok(n), Some(m)) = (v.trim().parse::<u8>(), self.ins.menu) else { return Task::none() };
                        let mut c = seletor_cor::rgba_de(&self.cor_do_menu(m.0).unwrap_or_else(|| "#000000".into()));
                        if i > 2 { return Task::none(); }
                        c[i] = n;
                        c[3] = 255;
                        let h = seletor_cor::hex_de(c);
                        self.ins.hsva = seletor_cor::rgba_para_hsva(c, self.ins.hsva[0]);
                        Some(h)
                    }
                };
                if let Some(h) = &cor { self.ins.hex = h.trim_start_matches('#').into(); }
                // O desenho escolhido muda ao confirmar no seletor, não a cada arrasto (cada mudança é um Ctrl+Z).
                if let (true, Some((Menu::CorTexto, _)), Some(h)) = (escolhida, self.ins.menu, cor.as_deref()) {
                    let c = rgb(h);
                    self.por_cor(cor);
                    return self.tipografia_escolhidos(EstiloTexto { cor: Some(c), ..Default::default() });
                }
                let estilo = if escolhida { self.estilo_da_cor(&cor) } else { EstiloDesenho::default() };
                self.por_cor(cor);
                if estilo != EstiloDesenho::default() { return self.estilizar_escolhidos(estilo); }
            }
            Ins::Espessura(e) => {
                self.ins.espessura = e;
                self.ins.menu = None;
                return self.estilizar_escolhidos(EstiloDesenho { espessura: Some(e), ..Default::default() });
            }
            Ins::Tamanho(t) => self.ins.tamanho = t,
            Ins::Negrito => self.ins.negrito = !self.ins.negrito,
            Ins::Alinhar(modo) => return self.alinhar(Some(modo), None),
            Ins::Encaixe(i) => if let Some(e) = self.ins.encaixe.get_mut(i) { *e = !*e },
            Ins::PassoGrade(i) => if let Some(&(_, p)) = crate::edicao::PASSOS_GRADE.get(i) { self.ins.passo_grade = p; self.ins.encaixe[2] = true; },
            Ins::Reguas => self.ins.reguas = !self.ins.reguas,
            Ins::Guia(v, pos) => { self.ins.guias.push((v, pos)); self.status = "Guia criada: os objetos grudam nela ao arrastar.".into(); }
            Ins::LimparGuias => self.ins.guias.clear(),
            Ins::AlinharPagina(v) => self.ins.alinhar_pagina = v,
            Ins::Distribuir(eixo) => return self.alinhar(None, Some(eixo)),
        }
        Task::none()
    }

    fn colocar(&mut self, pagina: u32, a: Point, b: Point) -> Task<Msg> {
        let Some(i) = self.insercao() else { return Task::none() };
        let clique = a.distance(b) < 4.0;
        let z = self.zoom;
        // Caixa na tela: a arrastada, ou o tamanho padrão a partir do clique.
        let caixa_tela = |l: f32, al: f32| if clique { (a, Point::new(a.x + l * z, a.y + al * z)) }
            else { (Point::new(a.x.min(b.x), a.y.min(b.y)), Point::new(a.x.max(b.x), a.y.max(b.y))) };
        match i {
            Insercao::Texto => {
                // Clicar fora com texto digitado grava, como no Acrobat.
                if self.ins.texto.as_ref().is_some_and(|t| !t.2.trim().is_empty()) {
                    return self.update_ins(Ins::GravarTexto);
                }
                self.ins.texto = Some((pagina, a, String::new()));
                crate::focar(id_texto_novo())
            }
            Insercao::Link => {
                if clique { self.status = "Arraste sobre a área que vai ser clicável.".into(); return Task::none(); }
                let Some(caixa) = self.caixa_na_pagina(pagina, a, b) else { return Task::none() };
                self.ins.link = Some((pagina, caixa, String::new()));
                crate::focar(id_link())
            }
            Insercao::Forma(id) => {
                let Some(t) = TipoForma::pelo_id(id) else { return Task::none() };
                let (p0, p1) = caixa_tela(100.0, 60.0);
                let (l, al) = (((p1.x - p0.x) / z).max(1.0), ((p1.y - p0.y) / z).max(1.0));
                let Some(caixa) = self.caixa_na_pagina(pagina, p0, p1) else { return Task::none() };
                let Some(g) = feather_shapes::forma::geometria(t, &[], Retangulo { x: 0.0, y: 0.0, largura: l, altura: al }) else { return Task::none() };
                // Linha arrastada para cima ou para a esquerda sai na direção do arrasto.
                let (ex, ey) = (!clique && b.x < a.x, !clique && b.y < a.y);
                let linha = t.so_traco();
                let desenho = DesenhoNovo {
                    preenchida: espelhar(g.preenchida, l, al, ex, ey),
                    tracos: espelhar(g.tracos, l, al, ex, ey),
                    preenchimento: if linha { None } else { self.ins.preenchimento.as_deref().map(rgb) },
                    contorno: self.ins.contorno.as_deref().map(rgb).or(linha.then_some([0.0; 3])),
                    espessura: self.ins.espessura,
                };
                let nome = t.nome();
                self.inserir(pagina, |pdf| { pdf.inserir_desenho(pagina, caixa, &desenho)?; Ok(format!("{nome} inserida.")) })
            }
            Insercao::Icone(id) => {
                let Some(icone) = icones::buscar_por_id(id) else { return Task::none() };
                let (p0, p1) = caixa_tela(32.0, 32.0);
                let (l, al) = (((p1.x - p0.x) / z).max(1.0), ((p1.y - p0.y) / z).max(1.0));
                let Some(caixa) = self.caixa_na_pagina(pagina, p0, p1) else { return Task::none() };
                let (linhas, escala) = icones::tracos(icone, 0.0, 0.0, l, al);
                // Cor do ícone = contorno (como no FeatherSheet); traço do Tabler (2 em 24) vezes a espessura.
                let cor = self.ins.contorno.as_deref().or(self.ins.preenchimento.as_deref()).map_or([0.0; 3], rgb);
                let desenho = DesenhoNovo {
                    preenchida: Caminho::novo(), tracos: caminho_do_icone(&linhas),
                    preenchimento: None, contorno: Some(cor), espessura: 2.0 * escala * self.ins.espessura,
                };
                self.inserir(pagina, |pdf| { pdf.inserir_desenho(pagina, caixa, &desenho)?; Ok("Ícone inserido.".into()) })
            }
            Insercao::Imagem => {
                let Some((imagem, w, h)) = self.ins.imagem.clone() else { return Task::none() };
                // Um clique: tamanho a 96 dpi, até metade da largura da página.
                let largura = if clique {
                    let pagina_l = self.aberto.as_ref().and_then(|ab| ab.tamanhos.get(pagina as usize - 1)).map_or(595.0, |t| t.0);
                    self.ins.largura_imagem.unwrap_or(w as f32 * 0.75).min(pagina_l / 2.0) * z
                } else { (b.x - a.x).abs() };
                let inicio = if clique { a } else { Point::new(a.x.min(b.x), a.y.min(b.y)) };
                let fim = Point::new(inicio.x + largura, inicio.y + largura * h as f32 / w as f32);
                let Some(caixa) = self.caixa_na_pagina(pagina, inicio, fim) else { return Task::none() };
                let imagem = (*imagem).clone();
                self.inserir(pagina, |pdf| { pdf.inserir_imagem(pagina, caixa, imagem)?; Ok("Imagem inserida.".into()) })
            }
        }
    }

    /// Alinha (um objeto só: na página) ou distribui os escolhidos, pela mesma
    /// regra do FeatherSlide (`feather_draw::Cena`), na orientação da tela.
    fn alinhar(&mut self, modo: Option<Alinhar>, eixo: Option<Eixo>) -> Task<Msg> {
        let Some((pagina, escolhidos)) = &self.escolhidos else { return Task::none() };
        let pagina = *pagina;
        let Some(o) = self.objetos.get(&pagina) else { return Task::none() };
        let indices: Vec<usize> = escolhidos.iter().copied().collect();
        let caixas: Vec<Rect> = indices.iter().filter_map(|&k| o.lista.get(k)).map(|ob| na_tela(ob.caixa, o.caixa, o.giro, 1.0)).collect();
        if caixas.len() != indices.len() { return Task::none(); }
        let figura = |r: Rect| Figura::Retangulo {
            area: Retangulo { x: r.x, y: r.y, largura: r.width, altura: r.height },
            estilo: EstiloCena { contorno: None, espessura: 0.0, ..EstiloCena::default() },
        };
        let mut cena = Cena::nova(1.0e5, 1.0e5);
        for &r in &caixas { cena.adicionar(figura(r)); }
        let mut ids: Vec<usize> = (0..caixas.len()).collect();
        if caixas.len() == 1 || self.ins.alinhar_pagina && modo.is_some() {
            // Um objeto só (ou "à página" escolhido) alinha com a página.
            cena.adicionar(figura(na_tela(o.caixa, o.caixa, o.giro, 1.0)));
            ids.push(caixas.len());
        }
        let feito = match (modo, eixo) {
            (Some(m), _) => cena.alinhar(&ids, m),
            (_, Some(e)) => cena.distribuir(&ids, e),
            _ => return Task::none(),
        };
        if feito.is_err() { self.status = "Escolha mais objetos (distribuir pede três).".into(); return Task::none(); }
        let giro = o.giro;
        let movimentos: Vec<(usize, f32, f32)> = indices.iter().zip(&caixas).enumerate().filter_map(|(i, (&k, antes))| {
            let depois = cena.figuras[i].bounding_box()?;
            let (dx, dy) = (depois.x - antes.x, depois.y - antes.y);
            (dx.abs() > 0.01 || dy.abs() > 0.01).then(|| { let (px, py) = para_pagina(dx, dy, giro, 1.0); (k, px, py) })
        }).collect();
        if movimentos.is_empty() { return Task::none(); }
        let tarefa = self.editar(|pdf| {
            for &(k, px, py) in &movimentos { pdf.mover_objetos(pagina, &[k], px, py)?; }
            Ok(format!("{} objeto(s) alinhado(s).", movimentos.len()))
        });
        // A escolha continua a mesma depois de mover.
        self.escolhidos = Some((pagina, indices.into_iter().collect()));
        self.ler_objetos_sempre(&[pagina]);
        tarefa
    }

    /// Diálogo do link sobre a tela.
    pub fn com_dialogo_link<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some((_, _, alvo)) = &self.ins.link else { return crate::inicio::sem_camada(base) };
        let total = self.total_paginas();
        let destino = match alvo.trim().parse::<u32>() {
            Ok(n) if (1..=total).contains(&n) => Ok(format!("Vai para a página {n}.")),
            Ok(_) => Err(format!("Página entre 1 e {total}.")),
            Err(_) if alvo.trim().is_empty() => Err(String::new()),
            Err(_) => Ok(format!("Abre {} no navegador.", alvo.trim())),
        };
        let mut corpo = column![
            text_input("https://www.gov.br ou um número de página", alvo).id(id_link())
                .on_input(|t| Msg::Ins(Ins::Link(t))).on_submit(Msg::Ins(Ins::GravarLink)).padding([6, 8]).size(13),
        ].spacing(10);
        match &destino {
            Ok(d) => corpo = corpo.push(text(d.clone()).size(12).color(tema::COR_TEXTO_MUTED())),
            Err(e) if !e.is_empty() => corpo = corpo.push(dlg::aviso("alert-triangle", e.clone())),
            Err(_) => {}
        }
        Dialogo::new("Link", corpo).icone("link")
            .subtitulo("A área marcada fica clicável no PDF, sem mudar o desenho da página.")
            .primario("Criar link", destino.is_ok().then_some(Msg::Ins(Ins::GravarLink)))
            .ao_fechar(Msg::Ins(Ins::Fechar)).topo(200.0).sobre(base)
    }

    // ---------- Menus suspensos ----------

    /// O menu aberto (formas, ícones ou cor) por cima da tela; clicar fora fecha.
    pub fn com_menu_ins<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some((menu, (x, y))) = self.ins.menu else { return crate::inicio::sem_camada(base) };
        let (jl, ja) = ler(&JANELA);
        let alto = (if ja > 0.0 { ja } else { 800.0 } - y - 16.0).max(200.0);
        let painel: Element<'a, Msg> = match menu {
            Menu::Formas => self.menu_formas(alto),
            Menu::Icones => self.menu_icones(alto),
            m => self.menu_cor(m),
        };
        let largura = match menu { Menu::Formas | Menu::Icones => 336.0, _ if self.ins.mais_cores => 476.0, _ => 206.0 };
        let x = if jl > 0.0 { x.min(jl - largura - 8.0).max(8.0) } else { x };
        Stack::new().width(Length::FILL).height(Length::FILL)
            .push(base)
            .push(mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::Ins(Ins::FecharMenu)))
            .push(column![Space::with_height(y), row![Space::with_width(x), opaque(painel)]])
            .into()
    }

    fn cabecalho_menu<'a>(titulo: &'a str) -> Element<'a, Msg> {
        row![
            text(titulo).size(14),
            Space::with_width(Length::FILL),
            button(ic::icone("x", 14.0)).padding(4).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Ins(Ins::FecharMenu)),
        ].align_y(Align::Center).into()
    }

    /// Inserir > Formas: o catálogo do `feather-shapes` por categoria, como no FeatherSheet.
    fn menu_formas(&self, alto: f32) -> Element<'_, Msg> {
        let atual = match self.insercao() { Some(Insercao::Forma(id)) => Some(id), _ => None };
        let mut grupos: Vec<(&'static str, Vec<TipoForma>)> = Vec::new();
        for &t in TipoForma::TODOS {
            let nome = t.categoria().nome();
            match grupos.iter_mut().find(|(n, _)| *n == nome) {
                Some((_, v)) => v.push(t),
                None => grupos.push((nome, vec![t])),
            }
        }
        let mut lista = column![].spacing(6);
        for (grupo, tipos) in grupos {
            lista = lista.push(text(grupo.to_uppercase()).size(10.5).color(tema::COR_TEXTO_MUTED()));
            for fileira in tipos.chunks(8) {
                lista = lista.push(row(fileira.iter().map(|&t| {
                    let botao = button(canvas::Canvas::new(Miniatura(t)).width(26).height(26)).padding(4)
                        .style(tema::estilo_botao_pequeno(atual == Some(t.id())))
                        .on_press(Msg::Ins(Ins::Ferramenta(Insercao::Forma(t.id()))));
                    tooltip(botao, t.nome(), TooltipPosition::Bottom).style(tema::estilo_dica).into()
                })).spacing(4));
            }
        }
        let corpo = column![
            Self::cabecalho_menu("Formas"),
            scrollable(lista).height(Length::Fixed((alto - 96.0).max(120.0))),
            text("Escolha e arraste na página. Um clique cria no tamanho padrão.").size(11).color(tema::COR_TEXTO_MUTED()),
        ].spacing(8);
        container(corpo.padding(12).width(336)).style(tema::estilo_popup_menu).into()
    }

    /// Inserir > Ícones: busca (português ou inglês) e a grade dos ícones Tabler do `feather-shapes`.
    fn menu_icones(&self, alto: f32) -> Element<'_, Msg> {
        let busca = &self.ins.busca_icones;
        let achados = if busca.trim().is_empty() {
            // Os mais usados primeiro, depois o resto.
            let mut v = icones::buscar("", usize::MAX);
            v.extend(icones::todos().iter().filter(|i| !v.contains(i)).collect::<Vec<_>>());
            v
        } else { icones::buscar(busca, usize::MAX) };
        let mut grade = column![].spacing(2);
        for fileira in achados[..achados.len().min(MAX_ICONES)].chunks(8) {
            grade = grade.push(row(fileira.iter().map(|&icone| {
                let botao = button(canvas::Canvas::new(MiniIcone(icone)).width(24).height(24)).padding(5)
                    .style(tema::estilo_botao_pequeno(self.insercao() == Some(Insercao::Icone(icone.id))))
                    .on_press(Msg::Ins(Ins::Ferramenta(Insercao::Icone(icone.id))));
                tooltip(botao, icone.rotulo(), TooltipPosition::Bottom).style(tema::estilo_dica).into()
            })).spacing(2));
        }
        let resumo = if achados.is_empty() {
            "Nenhum ícone encontrado. Tente outra palavra (casa, seta, relógio, user).".to_string()
        } else if achados.len() > MAX_ICONES {
            format!("Mostrando {MAX_ICONES} de {} — busque para achar os outros", achados.len())
        } else {
            format!("{} resultado(s)", achados.len())
        };
        let corpo = column![
            Self::cabecalho_menu("Ícones"),
            text_input("Buscar ícone… (casa, seta, relógio, chart)", busca).id(id_busca_icones())
                .on_input(|t| Msg::Ins(Ins::BuscaIcones(t))).size(13).padding([6, 9]),
            text(resumo).size(11).color(tema::COR_TEXTO_MUTED()),
            scrollable(grade).height(Length::Fixed((alto - 150.0).max(120.0))),
            text("Escolha e clique na página. A cor é a do contorno. Ícones Tabler (licença MIT).").size(11).color(tema::COR_TEXTO_MUTED()),
        ].spacing(8);
        container(corpo.padding(12).width(336)).style(tema::estilo_popup_menu).into()
    }

    /// Painel de cor (preenchimento, contorno com espessura, ou cor do texto), como no FeatherSheet.
    fn menu_cor(&self, menu: Menu) -> Element<'_, Msg> {
        let atual = self.cor_do_menu(menu);
        let amostra = |hex: &'static str| -> Element<'_, Msg> {
            let escolhida = atual.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(hex));
            let cor = cor_hex(hex);
            button(Space::new(20, 20)).padding(0).on_press(Msg::Ins(Ins::Cor(Some(hex.into()))))
                .style(move |_: &Theme, status| Style {
                    background: Some(Background::Color(cor)),
                    border: Border {
                        color: if escolhida || matches!(status, Status::Hovered) { VERMELHO } else { tema::COR_BORDA() },
                        width: if escolhida { 2.5 } else { 1.0 },
                        radius: [5.0; 4],
                    },
                    ..Default::default()
                }).into()
        };
        let mut cores = column![].spacing(6);
        for fileira in PALETA.chunks(6) {
            cores = cores.push(row(fileira.iter().map(|&c| amostra(c))).spacing(6));
        }
        let titulo = match menu { Menu::Preenchimento => "Preenchimento", Menu::Contorno => "Contorno", _ => "Cor do texto" };
        let rotulo = |t: &'static str| text(t).size(10.5).color(tema::COR_TEXTO_MUTED());
        let mut corpo = column![text(titulo).size(13), rotulo("COR"), cores].spacing(8);
        if menu != Menu::CorTexto {
            let sem = if menu == Menu::Preenchimento { "Sem preenchimento" } else { "Sem contorno" };
            corpo = corpo.push(button(text(sem).size(12)).width(Length::FILL).padding([5, 8])
                .style(tema::estilo_botao_pequeno(atual.is_none())).on_press(Msg::Ins(Ins::Cor(None))));
        }
        corpo = corpo.push(button(row![ic::icone("palette", 14.0), text("Mais cores…").size(12)].spacing(6).align_y(Align::Center))
            .width(Length::FILL).padding([5, 8]).style(tema::estilo_botao_pequeno(self.ins.mais_cores)).on_press(Msg::Ins(Ins::MaisCores)));
        if menu == Menu::Contorno {
            let cor_linha = atual.as_deref().map_or(tema::COR_TEXTO(), cor_hex);
            let mut grossuras = column![].spacing(2);
            for e in ESPESSURAS {
                let traco = container(Space::with_height((e * 1.2).clamp(1.0, 10.0))).width(60).style(move |_| Style {
                    background: Some(Background::Color(cor_linha)), ..Default::default()
                });
                grossuras = grossuras.push(button(row![container(traco).width(66).center_y(14),
                        text(format!("{} pt", e.to_string().replace('.', ","))).size(12)].spacing(8).align_y(Align::Center))
                    .width(Length::FILL).padding([3, 8])
                    .style(tema::estilo_botao_pequeno((self.ins.espessura - e).abs() < 0.01))
                    .on_press(Msg::Ins(Ins::Espessura(e))));
            }
            corpo = corpo.push(rotulo("ESPESSURA")).push(grossuras);
        }
        let painel = container(corpo.padding(12).width(206)).style(tema::estilo_popup_menu);
        if !self.ins.mais_cores { return painel.into(); }
        let seletor = seletor_cor::painel(titulo, atual.as_deref().unwrap_or(""), self.ins.hsva, &self.ins.hex)
            .map(|e| Msg::Ins(Ins::Seletor(e)));
        row![painel, seletor].spacing(6).into()
    }

    // ---------- Faixa ----------

    /// Botão de cor da faixa: ícone com a amostra embaixo e a seta do menu.
    pub fn botao_cor(&self, icone: &'static str, cor: Option<&str>, menu: Menu, dica: &'static str) -> Element<'_, Msg> {
        let cor = cor.map(cor_hex);
        let amostra = container(Space::new(14, 4)).style(move |_| Style {
            background: cor.map(Background::Color),
            border: Border { color: tema::COR_BORDA(), width: if cor.is_some() { 0.0 } else { 1.0 }, radius: [1.0; 4] },
            ..Default::default()
        });
        let botao = button(row![
            column![ic::icone(icone, 16.0), amostra].spacing(1).align_x(Align::Center),
            ic::icone("chevron-down", 11.0),
        ].spacing(3).align_y(Align::Center))
            .padding([4, 6]).style(tema::estilo_botao_pequeno(self.ins.menu.is_some_and(|m| m.0 == menu)))
            .on_press(Msg::Ins(Ins::AbrirMenu(menu)));
        tooltip(botao, dica, TooltipPosition::Bottom).style(tema::estilo_dica).into()
    }

    pub fn grupo_estilo(&self) -> Element<'_, Msg> {
        let opcoes: Vec<String> = TAMANHOS.iter().map(|v| format!("{v} pt")).collect();
        let escolhida = TAMANHOS.iter().position(|t| (t - self.ins.tamanho).abs() < 0.01).map(|i| opcoes[i].clone());
        let rotulos = opcoes.clone();
        let tamanho = suspensa(opcoes, escolhida, move |o| {
            Msg::Ins(Ins::Tamanho(TAMANHOS[rotulos.iter().position(|r| *r == o).unwrap_or(4)]))
        }).text_size(12).padding([2, 6]).width(76);
        column![
            row![
                self.botao_cor("bucket", self.ins.preenchimento.as_deref(), Menu::Preenchimento, "Preenchimento da forma"),
                self.botao_cor("pencil", self.ins.contorno.as_deref(), Menu::Contorno, "Contorno: cor e espessura (também a cor do ícone)"),
                self.botao_cor("cor_texto", Some(&self.ins.cor_texto), Menu::CorTexto, "Cor do texto"),
            ].spacing(2).align_y(Align::Center),
            row![
                tamanho,
                button(text("Negrito").size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno(self.ins.negrito)).on_press(Msg::Ins(Ins::Negrito)),
            ].spacing(4).align_y(Align::Center),
        ].spacing(4).into()
    }

    /// Botão do grupo Alinhar: precisa de objetos escolhidos (distribuir, de três).
    pub fn botao_alinhar(&self, icone: &str, dica: &str, msg: Ins) -> Element<'_, Msg> {
        let n = self.escolhidos.as_ref().map_or(0, |e| e.1.len());
        let pode = if matches!(msg, Ins::Distribuir(_)) { n >= 3 } else { n >= 1 };
        componentes::botao_icone(ic::icone(icone, 20.), dica, pode.then_some(Msg::Ins(msg)), false)
    }
}

/// Bloqueante: chamar fora da thread da janela.
fn imagem_da_area_de_transferencia() -> Option<(Arc<ImagemNova>, u32, u32)> {
    let dados = arboard::Clipboard::new().ok()?.get_image().ok()?;
    let (w, h) = (u32::try_from(dados.width).ok()?, u32::try_from(dados.height).ok()?);
    if w == 0 || h == 0 || dados.bytes.len() != dados.width * dados.height * 4 { return None; }
    let mut rgb = Vec::with_capacity(dados.width * dados.height * 3);
    let mut alfa = Vec::with_capacity(dados.width * dados.height);
    for p in dados.bytes.chunks_exact(4) { rgb.extend_from_slice(&p[..3]); alfa.push(p[3]); }
    // Print de tela vem todo opaco: sem máscara, o PDF fica menor.
    let alfa = alfa.iter().any(|&a| a != 255).then_some(alfa);
    Some((Arc::new(ImagemNova::Pixels { largura: w, altura: h, rgb, alfa }), w, h))
}

/// Lê PNG ou JPEG. O JPEG em RGB ou cinza vai como está para o PDF.
pub(crate) fn ler_imagem(bytes: Vec<u8>) -> Result<(Arc<ImagemNova>, u32, u32), String> {
    let img = image::load_from_memory(&bytes).map_err(|e| format!("Não consegui ler a imagem: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let jpeg = bytes.starts_with(&[0xFF, 0xD8]);
    let nova = match img.color() {
        image::ColorType::L8 if jpeg => ImagemNova::Jpeg { bytes, largura: w, altura: h, cinza: true },
        image::ColorType::Rgb8 if jpeg => ImagemNova::Jpeg { bytes, largura: w, altura: h, cinza: false },
        _ => {
            let rgba = img.to_rgba8();
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            let mut alfa = Vec::with_capacity((w * h) as usize);
            for p in rgba.pixels() { rgb.extend_from_slice(&p.0[..3]); alfa.push(p.0[3]); }
            ImagemNova::Pixels { largura: w, altura: h, rgb, alfa: Some(alfa) }
        }
    };
    Ok((Arc::new(nova), w, h))
}
