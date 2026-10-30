//! A assinatura à mão do painel Preencher e assinar: desenhada, digitada numa fonte
//! cursiva, tirada de uma imagem ou uma das guardadas. Vira uma imagem de fundo
//! transparente e entra na página pela ferramenta Imagem (clique ou arraste onde vai).

use std::path::PathBuf;
use std::sync::Arc;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::ImagemNova;
use ::image::RgbaImage;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::{self, Cursor};
use crate::compat::{event, canvas::{self, Frame, Geometry, Path, Stroke}};
use crate::inserir::Insercao;
use crate::{App, Ferramenta, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modo { #[default] Desenhar, Digitar, Imagem, Salvas }

const FONTES: [&[u8]; 3] = [
    include_bytes!("../fontes/DancingScript.ttf"),
    include_bytes!("../fontes/Sacramento.ttf"),
    include_bytes!("../fontes/GreatVibes.ttf"),
];
const CORES: [(&str, [u8; 3]); 3] = [("Preto", [0x1a, 0x1a, 0x1a]), ("Azul", [0x1f, 0x4f, 0xbf]), ("Vermelho", [0xb0, 0x20, 0x20])];
/// Quadro de desenho na tela, e quantas vezes maior sai a imagem.
const QUADRO: (f32, f32) = (260.0, 120.0);
const ESCALA: f32 = 3.0;
/// Largura com que a assinatura entra na página com um clique (pt, ~5 cm).
const LARGURA_PT: f32 = 150.0;

#[derive(Clone)]
pub struct Rubrica {
    pub modo: Modo,
    cor: usize,
    tracos: Vec<Vec<Point>>,
    nome: String,
    fonte: usize,
    /// Prévia do nome em cada fonte (refeita ao digitar ou trocar a cor).
    previas: Vec<Image>,
    imagem: Option<RgbaImage>,
    previa_imagem: Option<Image>,
    tirar_fundo: bool,
    guardar: bool,
    salvas: Vec<PathBuf>,
    salva: Option<usize>,
    erro: Option<String>,
}

impl Default for Rubrica {
    fn default() -> Self {
        let mut r = Self { modo: Modo::Desenhar, cor: 0, tracos: vec![], nome: String::new(), fonte: 0, previas: vec![],
            imagem: None, previa_imagem: None, tirar_fundo: true, guardar: false, salvas: vec![], salva: None, erro: None };
        r.refazer_previas();
        r
    }
}

#[derive(Debug, Clone)]
pub enum Rb {
    Modo(Modo),
    Cor(usize),
    Traco(Vec<Point>),
    Limpar,
    Nome(String),
    Fonte(usize),
    EscolherImagem,
    ImagemLida(Option<Result<RgbaImage, String>>),
    TirarFundo(bool),
    Guardar(bool),
    Salva(usize),
    ApagarSalva(usize),
    Colocar,
}

fn pasta() -> Option<PathBuf> { crate::recentes::pasta_config().map(|p| p.join("assinaturas")) }

/// Escreve `texto` na fonte cursiva `fonte`, `px` de altura, na cor dada.
fn escrever(texto: &str, fonte: usize, cor: [u8; 3], px: f32) -> Option<RgbaImage> {
    let f = FontRef::try_from_slice(FONTES[fonte]).ok()?;
    let escala = PxScale::from(px);
    let s = f.as_scaled(escala);
    // Fonte cursiva sai da caixa para os lados: folga antes de cortar.
    let folga = px * 0.4;
    let (mut x, mut anterior, mut glifos) = (folga, None, vec![]);
    for ch in texto.chars() {
        let id = s.glyph_id(ch);
        if let Some(a) = anterior { x += s.kern(a, id); }
        glifos.push(id.with_scale_and_position(escala, ab_glyph::point(x, folga + s.ascent())));
        x += s.h_advance(id);
        anterior = Some(id);
    }
    let (w, h) = ((x + folga).ceil() as u32, (s.ascent() - s.descent() + 2.0 * folga).ceil() as u32);
    let mut img = RgbaImage::new(w.max(1), h.max(1));
    for g in glifos {
        let Some(o) = f.outline_glyph(g) else { continue };
        let b = o.px_bounds();
        o.draw(|gx, gy, c| {
            let (px_, py_) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
            if px_ < 0 || py_ < 0 || px_ >= w as i32 || py_ >= h as i32 { return; }
            let p = img.get_pixel_mut(px_ as u32, py_ as u32);
            let a = (c.clamp(0.0, 1.0) * 255.0) as u8;
            if a > p[3] { *p = ::image::Rgba([cor[0], cor[1], cor[2], a]); }
        });
    }
    Some(img)
}

/// Corta a imagem no que tem tinta, com uma margem pequena.
fn cortar(img: RgbaImage) -> Option<RgbaImage> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 8 { (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y)); }
    }
    if x0 > x1 { return None; }
    let m = 4;
    let (x0, y0) = (x0.saturating_sub(m), y0.saturating_sub(m));
    let (x1, y1) = ((x1 + m).min(img.width() - 1), (y1 + m).min(img.height() - 1));
    Some(::image::imageops::crop_imm(&img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image())
}

/// Papel claro vira transparente; a tinta fica (com transição suave na borda do traço).
fn sem_fundo(mut img: RgbaImage) -> RgbaImage {
    for p in img.pixels_mut() {
        let luz = (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
        let a = match luz { 225.. => 0, 0..=140 => 255, l => (225 - l) * 255 / 85 };
        p[3] = p[3].min(a as u8);
    }
    img
}

fn tracos_em_imagem(tracos: &[Vec<Point>], cor: [u8; 3]) -> Option<RgbaImage> {
    let (w, h) = ((QUADRO.0 * ESCALA) as u32, (QUADRO.1 * ESCALA) as u32);
    let mut tela = tiny_skia::Pixmap::new(w, h)?;
    let mut tinta = tiny_skia::Paint::default();
    tinta.set_color_rgba8(cor[0], cor[1], cor[2], 255);
    tinta.anti_alias = true;
    let traco = tiny_skia::Stroke { width: 2.4 * ESCALA, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    for t in tracos {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(t[0].x * ESCALA, t[0].y * ESCALA);
        // Um toque sem arrastar vira ponto.
        if t.len() == 1 { pb.line_to(t[0].x * ESCALA + 0.1, t[0].y * ESCALA); }
        for p in &t[1..] { pb.line_to(p.x * ESCALA, p.y * ESCALA); }
        if let Some(caminho) = pb.finish() { tela.stroke_path(&caminho, &tinta, &traco, tiny_skia::Transform::identity(), None); }
    }
    let bytes: Vec<u8> = tela.pixels().iter().flat_map(|p| { let c = p.demultiply(); [c.red(), c.green(), c.blue(), c.alpha()] }).collect();
    RgbaImage::from_raw(w, h, bytes)
}

fn alca(img: &RgbaImage) -> Image { imagem_rgba(img.width(), img.height(), img.as_raw().clone()) }

impl Rubrica {
    fn refazer_previas(&mut self) {
        let nome = if self.nome.trim().is_empty() { "Sua assinatura" } else { self.nome.trim() };
        self.previas = (0..FONTES.len()).filter_map(|i| escrever(nome, i, CORES[self.cor].1, 44.0).and_then(cortar).map(|img| alca(&img))).collect();
    }

    fn refazer_previa_imagem(&mut self) {
        self.previa_imagem = self.imagem.clone().map(|img| if self.tirar_fundo { sem_fundo(img) } else { img }).map(|img| alca(&img));
    }

    pub fn ler_salvas(&mut self) {
        let mut v: Vec<PathBuf> = pasta().and_then(|p| std::fs::read_dir(p).ok()).into_iter().flatten().flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
        v.sort();
        v.reverse(); // a mais nova primeiro (o nome leva a hora)
        self.salvas = v;
        self.salva = self.salva.filter(|&i| i < self.salvas.len());
    }

    /// A assinatura pronta, cortada no que tem tinta.
    fn gerar(&self) -> Result<RgbaImage, String> {
        let img = match self.modo {
            Modo::Desenhar if self.tracos.is_empty() => return Err("Desenhe a assinatura no quadro.".into()),
            Modo::Desenhar => tracos_em_imagem(&self.tracos, CORES[self.cor].1),
            Modo::Digitar if self.nome.trim().is_empty() => return Err("Digite o nome.".into()),
            Modo::Digitar => escrever(self.nome.trim(), self.fonte, CORES[self.cor].1, 140.0),
            Modo::Imagem => self.imagem.clone().map(|img| if self.tirar_fundo { sem_fundo(img) } else { img }),
            Modo::Salvas => {
                let p = self.salva.and_then(|i| self.salvas.get(i)).ok_or("Escolha uma das assinaturas guardadas.")?;
                Some(::image::open(p).map_err(|e| format!("Não consegui abrir {}: {e}", p.display()))?.to_rgba8())
            }
        };
        img.and_then(cortar).ok_or_else(|| "A assinatura está vazia.".into())
    }

    fn pronta(&self) -> bool {
        match self.modo {
            Modo::Desenhar => !self.tracos.is_empty(),
            Modo::Digitar => !self.nome.trim().is_empty(),
            Modo::Imagem => self.imagem.is_some(),
            Modo::Salvas => self.salva.is_some(),
        }
    }
}

impl App {
    pub fn update_rubrica(&mut self, m: Rb) -> Task<Msg> {
        let r = &mut self.rubrica;
        r.erro = None;
        match m {
            Rb::Modo(modo) => { r.modo = modo; if modo == Modo::Salvas { r.ler_salvas(); } }
            Rb::Cor(c) => { r.cor = c; r.refazer_previas(); }
            Rb::Traco(t) => if !t.is_empty() { r.tracos.push(t) },
            Rb::Limpar => r.tracos.clear(),
            Rb::Nome(t) => { r.nome = t; r.refazer_previas(); }
            Rb::Fonte(i) => r.fonte = i,
            Rb::EscolherImagem => return Task::perform(async {
                let arquivo = rfd::AsyncFileDialog::new().add_filter("Imagem", &["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"]).pick_file().await?;
                let bytes = arquivo.read().await;
                Some(tokio::task::spawn_blocking(move || ::image::load_from_memory(&bytes).map(|i| i.to_rgba8())
                    .map_err(|e| format!("Não consegui ler a imagem: {e}"))).await.unwrap_or_else(|e| Err(e.to_string())))
            }, |r| Msg::Rub(Rb::ImagemLida(r))),
            Rb::ImagemLida(None) => {}
            Rb::ImagemLida(Some(Ok(img))) => {
                // Foto de celular tem 12 Mpx: 1600 px de largura sobram para uma assinatura.
                r.imagem = Some(if img.width() > 1600 {
                    let h = img.height() * 1600 / img.width();
                    ::image::imageops::resize(&img, 1600, h.max(1), ::image::imageops::FilterType::Triangle)
                } else { img });
                r.refazer_previa_imagem();
            }
            Rb::ImagemLida(Some(Err(e))) => r.erro = Some(e),
            Rb::TirarFundo(v) => { r.tirar_fundo = v; r.refazer_previa_imagem(); }
            Rb::Guardar(v) => r.guardar = v,
            Rb::Salva(i) => r.salva = Some(i),
            Rb::ApagarSalva(i) => if let Some(p) = r.salvas.get(i) {
                if let Err(e) = std::fs::remove_file(p) { r.erro = Some(format!("Não consegui apagar: {e}")); }
                r.salva = None;
                r.ler_salvas();
            },
            Rb::Colocar => {
                let img = match r.gerar() { Ok(i) => i, Err(e) => { r.erro = Some(e); return Task::none() } };
                if r.guardar && r.modo != Modo::Salvas {
                    let quando = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                    let gravado = pasta().ok_or("Sem pasta de configuração.".to_string()).and_then(|p| {
                        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                        img.save(p.join(format!("assinatura-{quando}.png"))).map_err(|e| e.to_string())
                    });
                    if let Err(e) = gravado { r.erro = Some(format!("Não consegui guardar a assinatura: {e}")); }
                }
                let (w, h) = img.dimensions();
                let mut rgb = Vec::with_capacity((w * h * 3) as usize);
                let mut alfa = Vec::with_capacity((w * h) as usize);
                for p in img.pixels() { rgb.extend_from_slice(&p.0[..3]); alfa.push(p.0[3]); }
                self.ins.imagem = Some((Arc::new(ImagemNova::Pixels { largura: w, altura: h, rgb, alfa: Some(alfa) }), w, h));
                self.ins.largura_imagem = Some(LARGURA_PT);
                self.escolhidos = None;
                self.ferramenta = Ferramenta::Inserir(Insercao::Imagem);
                self.status = "Clique onde vai a assinatura, ou arraste para escolher o tamanho. Esc cancela.".into();
            }
        }
        Task::none()
    }

    /// A parte de cima do painel Preencher e assinar.
    pub fn painel_rubrica(&self) -> Element<'_, Msg> {
        let r = &self.rubrica;
        let muted = tema::COR_TEXTO_MUTED();
        let aba = |rotulo: &'static str, m: Modo| button(text(rotulo).size(12).width(Length::FILL).align_x(Align::Center))
            .width(Length::FILL).padding([5, 2]).style(tema::estilo_botao_pequeno(r.modo == m)).on_press(Msg::Rub(Rb::Modo(m)));
        let cores = row(CORES.iter().enumerate().map(|(i, (nome, c))| {
            let cor = Color::rgb8(c[0], c[1], c[2]);
            let ativa = r.cor == i;
            let amostra = button(Space::new(18, 18)).padding(0).on_press(Msg::Rub(Rb::Cor(i))).style(move |_, _| Style {
                background: Some(Background::Color(cor)),
                border: Border { color: if ativa { tema::COR_AZUL_ACENTO() } else { Color::TRANSPARENT }, width: 2.0, radius: [9.0; 4] },
                ..Default::default()
            });
            tooltip(amostra, *nome, TooltipPosition::Bottom).into()
        })).spacing(8).align_y(Align::Center);
        let moldura = |e: Element<'static, Msg>| container(e).width(Length::FILL).style(|_| Style {
            background: Some(Background::Color(Color::WHITE)),
            border: Border { color: Color::rgb8(0xd0, 0xd4, 0xda), width: 1.0, radius: [4.0; 4] }, ..Default::default()
        });
        let corpo: Element<'_, Msg> = match r.modo {
            Modo::Desenhar => column![
                container(canvas::Canvas::new(Quadro { tracos: &r.tracos, cor: CORES[r.cor].1 }).width(QUADRO.0).height(QUADRO.1)),
                row![cores, Space::with_width(Length::FILL),
                    button(text("Limpar").size(12)).padding([4, 10]).style(tema::estilo_botao_pequeno(false))
                        .on_press_maybe((!r.tracos.is_empty()).then_some(Msg::Rub(Rb::Limpar)))].align_y(Align::Center),
            ].spacing(8).into(),
            Modo::Digitar => {
                let mut c = column![text_input("Seu nome", &r.nome).on_input(|t| Msg::Rub(Rb::Nome(t))).padding([6, 8]).size(13)].spacing(6);
                for (i, p) in r.previas.iter().enumerate() {
                    let ativa = r.fonte == i;
                    c = c.push(button(container(image(p.clone()).height(34)).center_x(Length::FILL)).width(Length::FILL).padding(6)
                        .on_press(Msg::Rub(Rb::Fonte(i))).style(move |_, status| Style {
                            background: Some(Background::Color(if status == Status::Hovered { Color::rgb8(0xf2, 0xf5, 0xfa) } else { Color::WHITE })),
                            border: Border { color: if ativa { tema::COR_AZUL_ACENTO() } else { Color::rgb8(0xd0, 0xd4, 0xda) }, width: if ativa { 2.0 } else { 1.0 }, radius: [4.0; 4] },
                            ..Default::default()
                        }));
                }
                c.push(cores).into()
            }
            Modo::Imagem => {
                let mut c = column![row![
                    button(text("Escolher imagem…").size(12)).padding([5, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Rub(Rb::EscolherImagem)),
                ]].spacing(8);
                match &r.previa_imagem {
                    Some(p) => c = c.push(moldura(container(image(p.clone()).height(90)).center_x(Length::FILL).padding(6).into())),
                    None => c = c.push(text("Foto ou digitalização da assinatura no papel (PNG ou JPG).").size(12).color(muted)),
                }
                c.push(checkbox("Tirar o fundo branco", r.tirar_fundo).on_toggle(|v| Msg::Rub(Rb::TirarFundo(v))).size(16).text_size(13)).into()
            }
            Modo::Salvas if r.salvas.is_empty() => dlg::aviso_vazio("Nenhuma guardada. Marque “Guardar para a próxima” ao colocar uma assinatura."),
            Modo::Salvas => column(r.salvas.iter().enumerate().map(|(i, p)| {
                let ativa = r.salva == Some(i);
                row![
                    button(container(image(imagem_arquivo(p)).height(34)).center_x(Length::FILL)).width(Length::FILL).padding(6)
                        .on_press(Msg::Rub(Rb::Salva(i))).style(move |_, _| Style {
                            background: Some(Background::Color(Color::WHITE)),
                            border: Border { color: if ativa { tema::COR_AZUL_ACENTO() } else { Color::rgb8(0xd0, 0xd4, 0xda) }, width: if ativa { 2.0 } else { 1.0 }, radius: [4.0; 4] },
                            ..Default::default()
                        }),
                    button(feather_ui::icones::icone("trash", 14.)).padding(5).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Rub(Rb::ApagarSalva(i))),
                ].spacing(6).align_y(Align::Center).into()
            })).spacing(6).into(),
        };
        let mut c = column![
            text("Sua assinatura").size(13),
            row![aba("Desenhar", Modo::Desenhar), aba("Digitar", Modo::Digitar), aba("Imagem", Modo::Imagem), aba("Salvas", Modo::Salvas)].spacing(4),
            corpo,
        ].spacing(10);
        if r.modo != Modo::Salvas {
            c = c.push(checkbox("Guardar para a próxima", r.guardar).on_toggle(|v| Msg::Rub(Rb::Guardar(v))).size(16).text_size(13));
        }
        if let Some(e) = &r.erro { c = c.push(text(e).size(12).color(tema::COR_ERRO())); }
        let colocando = self.ferramenta == Ferramenta::Inserir(Insercao::Imagem) && self.ins.largura_imagem.is_some();
        c.push(button(text(if colocando { "Clique na página…" } else { "Colocar na página" }).size(13)).padding([6, 14]).width(Length::FILL)
            .style(tema::estilo_botao_primario).on_press_maybe((r.pronta() && self.aberto.is_some() && !colocando).then_some(Msg::Rub(Rb::Colocar))))
            .into()
    }
}

/// O quadro onde se desenha com o mouse ou a caneta.
struct Quadro<'a> { tracos: &'a [Vec<Point>], cor: [u8; 3] }

impl canvas::Program<Msg> for Quadro<'_> {
    type State = Option<Vec<Point>>;

    fn update(&self, atual: &mut Self::State, evento: canvas::Event, limites: Rect, cursor: Cursor) -> (event::Status, Option<Msg>) {
        let dentro = |p: Point| Point::new(p.x.clamp(0.0, limites.width), p.y.clamp(0.0, limites.height));
        match evento {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => if let Some(p) = cursor.position_in(limites) {
                *atual = Some(vec![p]);
                return (event::Status::Captured, None);
            },
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => if let (Some(t), Some(p)) = (atual.as_mut(), cursor.position_from(limites.position())) {
                let p = dentro(p);
                if t.last().is_none_or(|u| u.distance(p) > 0.8) { t.push(p); }
                return (event::Status::Captured, None);
            },
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => if let Some(t) = atual.take() {
                return (event::Status::Captured, Some(Msg::Rub(Rb::Traco(t))));
            },
            _ => {}
        }
        (event::Status::Ignored, None)
    }

    fn draw(&self, atual: &Self::State, renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        f.fill_rectangle(Point::ORIGIN, limites.size(), Color::WHITE);
        f.stroke(&Path::rectangle(Point::ORIGIN, limites.size()), Stroke::default().with_color(Color::rgb8(0xd0, 0xd4, 0xda)).with_width(1.0));
        // Linha de base, como no papel.
        let y = limites.height * 0.75;
        f.stroke(&Path::line(Point::new(16.0, y), Point::new(limites.width - 16.0, y)), Stroke::default().with_color(Color::rgb8(0xc8, 0xcc, 0xd2)).with_width(1.0));
        if self.tracos.is_empty() && atual.is_none() {
            f.fill_text(canvas::Text { content: "Assine aqui".into(), position: Point::new(16.0, y - 22.0), color: Color::rgb8(0xa0, 0xa4, 0xaa), size: 13.0, ..Default::default() });
        }
        let cor = Color::rgb8(self.cor[0], self.cor[1], self.cor[2]);
        let caneta = Stroke { line_cap: canvas::LineCap::Round, line_join: canvas::LineJoin::Round, ..Stroke::default().with_color(cor).with_width(2.4) };
        for t in self.tracos.iter().chain(atual.iter()) {
            if t.len() == 1 { f.fill(&Path::circle(t[0], 1.2), cor); continue; }
            f.stroke(&Path::new(|b| { b.move_to(t[0]); for p in &t[1..] { b.line_to(*p); } }), caneta.clone());
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, _: &Self::State, limites: Rect, cursor: Cursor) -> mouse::Interaction {
        if cursor.is_over(limites) { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn as_tres_fontes_escrevem_e_o_corte_acha_a_tinta() {
        for i in 0..FONTES.len() {
            let img = escrever("Ana", i, [0, 0, 0], 60.0).and_then(cortar).expect("fonte sem tinta");
            assert!(img.width() > 40 && img.height() > 20, "fonte {i}: {:?}", img.dimensions());
        }
        let desenho = tracos_em_imagem(&[vec![Point::new(10.0, 10.0), Point::new(50.0, 40.0)]], [0, 0, 0]).and_then(cortar).unwrap();
        assert!(desenho.width() > 100 && desenho.width() < 150, "{:?}", desenho.dimensions());
        let mut papel = RgbaImage::from_pixel(4, 1, ::image::Rgba([250, 250, 250, 255]));
        papel.put_pixel(0, 0, ::image::Rgba([20, 20, 60, 255]));
        let limpo = sem_fundo(papel);
        assert_eq!((limpo.get_pixel(0, 0)[3], limpo.get_pixel(1, 0)[3]), (255, 0));
    }
}
