//! Os nomes do Iced que os módulos vindos da versão antiga usam e o pelicano diz de outro
//! jeito: alinhamento de linha e coluna, espaços, centralizar o conteúdo.
//! shortcut: ponte da migração; trocar pelos nomes do pelicano quando cada módulo for mexido.

use pelicano::*;

pub trait FlexCompat {
    fn align_x(self, a: Align) -> Self;
    fn align_y(self, a: Align) -> Self;
}

impl<'a, M: 'a> FlexCompat for Flex<'a, M> {
    fn align_x(self, a: Align) -> Self { self.align(a) }
    fn align_y(self, a: Align) -> Self { self.align(a) }
}

pub trait ContainerCompat {
    fn center_x(self, largura: impl Into<Length>) -> Self;
    fn center_y(self, altura: impl Into<Length>) -> Self;
}

impl<'a, M: 'a> ContainerCompat for Container<'a, M> {
    fn center_x(self, largura: impl Into<Length>) -> Self { self.width(largura).align_x(Align::Center) }
    fn center_y(self, altura: impl Into<Length>) -> Self { self.height(altura).align_y(Align::Center) }
}

pub trait SpaceCompat {
    fn with_width(l: impl Into<Length>) -> Space;
    fn with_height(a: impl Into<Length>) -> Space;
}

impl SpaceCompat for Space {
    fn with_width(l: impl Into<Length>) -> Space { Space::new(l, 0.0) }
    fn with_height(a: impl Into<Length>) -> Space { Space::new(0.0, a) }
}

pub trait StackCompat<'a, M> {
    fn new() -> Self;
    fn push_maybe(self, e: Option<impl Into<Element<'a, M>>>) -> Self;
}

impl<'a, M: 'a> StackCompat<'a, M> for Stack<'a, M> {
    fn new() -> Self { stack(Vec::new()) }
    fn push_maybe(self, e: Option<impl Into<Element<'a, M>>>) -> Self {
        match e { Some(e) => self.push(e), None => self }
    }
}

/// `text_size` da caixa de marcar: no pelicano o `size` é o do texto (a marca é fixa).
pub trait ToggleCompat {
    fn text_size(self, s: impl Into<Num>) -> Self;
}

impl<M> ToggleCompat for Toggle<'_, M> {
    fn text_size(self, s: impl Into<Num>) -> Self { self.size(s) }
}

pub trait PickListCompat {
    fn text_size(self, s: impl Into<Num>) -> Self;
}

impl<T: Clone + PartialEq + ToString, M> PickListCompat for PickList<'_, T, M> {
    fn text_size(self, s: impl Into<Num>) -> Self { self.size(s) }
}


/// `Rect::position` e `intersection` do Iced.
pub trait RectCompat {
    fn position(&self) -> Point;
    fn intersection(&self, o: &Rect) -> Option<Rect>;
    fn intersects(&self, o: &Rect) -> bool;
    fn expand(&self, d: f32) -> Rect;
    fn union(&self, o: &Rect) -> Rect;
    fn center_x(&self) -> f32 { self.position().x + self.size().width / 2.0 }
    fn center_y(&self) -> f32 { self.position().y + self.size().height / 2.0 }
    fn size(&self) -> Size;
}

impl RectCompat for Rect {
    fn position(&self) -> Point { Point::new(self.x, self.y) }
    fn size(&self) -> Size { Size::new(self.width, self.height) }
    fn intersection(&self, o: &Rect) -> Option<Rect> { Some(self.intersect(o)).filter(|r| r.width > 0.0 && r.height > 0.0) }
    fn intersects(&self, o: &Rect) -> bool { self.intersection(o).is_some() }
    fn expand(&self, d: f32) -> Rect { Rect { x: self.x - d, y: self.y - d, width: self.width + 2.0 * d, height: self.height + 2.0 * d } }
    fn union(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect { x, y, width: (self.x + self.width).max(o.x + o.width) - x, height: (self.y + self.height).max(o.y + o.height) - y }
    }
}

pub trait PointCompat {
    fn distance(&self, o: Point) -> f32;
}

impl PointCompat for Point {
    fn distance(&self, o: Point) -> f32 { (self.x - o.x).hypot(self.y - o.y) }
}

/// `center()` do texto do Iced: centrado na largura que tiver.
impl TextCompat2 for Text<'_> {
    fn center(self) -> Self { self.align_x(Align::Center) }
}

pub trait TextCompat2 {
    fn center(self) -> Self;
}

pub trait InputCompat<'a, M> {
    fn on_submit_maybe(self, m: Option<M>) -> Self;
}

impl<'a, M> InputCompat<'a, M> for TextInput<'a, M> {
    fn on_submit_maybe(self, m: Option<M>) -> Self { match m { Some(m) => self.on_submit(m), None => self } }
}

pub trait ToggleMaybe<'a, M> {
    fn on_toggle_maybe(self, f: Option<impl Fn(bool) -> M + 'a>) -> Self;
}

impl<'a, M> ToggleMaybe<'a, M> for Toggle<'a, M> {
    fn on_toggle_maybe(self, f: Option<impl Fn(bool) -> M + 'a>) -> Self { match f { Some(f) => self.on_toggle(f), None => self } }
}

pub mod event {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Status { Captured, Ignored }
}

pub mod mouse {
    use pelicano::{CursorIcon, Point, Rect};
    pub use pelicano::MouseButton as Button;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Event {
        CursorMoved { position: Point },
        ButtonPressed(Button),
        ButtonReleased(Button),
    }

    /// O cursor do Iced: posição na janela, ou nenhuma (fora dela).
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Cursor { Available(Point), Unavailable }

    impl Cursor {
        pub fn position(&self) -> Option<Point> {
            match *self { Cursor::Available(p) => Some(p), Cursor::Unavailable => None }
        }
        pub fn is_over(&self, b: Rect) -> bool { self.position().is_some_and(|p| b.contains(p)) }
        pub fn position_in(&self, b: Rect) -> Option<Point> {
            self.position().filter(|p| b.contains(*p)).map(|p| Point::new(p.x - b.x, p.y - b.y))
        }
        pub fn position_from(&self, o: Point) -> Option<Point> { self.position().map(|p| Point::new(p.x - o.x, p.y - o.y)) }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Interaction {
        #[default]
        Idle,
        Pointer,
        Crosshair,
        Grab,
        Grabbing,
        Text,
        ResizingHorizontally,
        ResizingVertically,
    }

    impl Interaction {
        pub(crate) fn icone(self) -> CursorIcon {
            match self {
                Interaction::Idle => CursorIcon::Default,
                Interaction::Pointer => CursorIcon::Pointer,
                Interaction::Crosshair => CursorIcon::Crosshair,
                Interaction::Grab => CursorIcon::Grab,
                Interaction::Grabbing => CursorIcon::Grabbing,
                Interaction::Text => CursorIcon::Text,
                Interaction::ResizingHorizontally => CursorIcon::EwResize,
                Interaction::ResizingVertically => CursorIcon::NsResize,
            }
        }
    }
}

/// O canvas do Iced (Frame que grava, Program que devolve mensagem) por cima do `Program`
/// do pelicano: o desenho é gravado em coordenadas do retângulo e repetido com um deslocamento.
pub mod canvas {
    use super::{event, mouse};
    use pelicano::{Align, Color, Element, Font, Length, Point, Rect, Renderer, Size, Theme, Transform};
    pub use pelicano::{LineCap, LineJoin};

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Event { Mouse(mouse::Event) }

    #[derive(Clone)]
    pub struct Path(Option<pelicano::Path>);

    pub struct Builder(pelicano::PathBuilder);

    impl Builder {
        pub fn move_to(&mut self, p: Point) { self.0.move_to(p); }
        pub fn line_to(&mut self, p: Point) { self.0.line_to(p); }
        pub fn quadratic_curve_to(&mut self, c: Point, p: Point) { self.0.quadratic_to(c, p); }
        pub fn bezier_curve_to(&mut self, c1: Point, c2: Point, p: Point) { self.0.bezier_to(c1, c2, p); }
        pub fn close(&mut self) { self.0.close(); }
    }

    impl Path {
        pub fn new(f: impl FnOnce(&mut Builder)) -> Path {
            let mut b = Builder(pelicano::PathBuilder::new());
            f(&mut b);
            Path(b.0.build())
        }
        pub fn line(a: Point, b: Point) -> Path { Path(Some(pelicano::Path::line(a, b))) }
        pub fn rectangle(p: Point, s: Size) -> Path { Path(pelicano::Path::rect(Rect::new(p, s))) }
        pub fn circle(c: Point, r: f32) -> Path { Path(pelicano::Path::circle(c, r)) }
        pub fn rounded_rectangle(p: Point, s: Size, r: f32) -> Path {
            let r = r.min(s.width / 2.0).min(s.height / 2.0);
            let (x0, y0, x1, y1) = (p.x, p.y, p.x + s.width, p.y + s.height);
            Path::new(|b| {
                b.move_to(Point::new(x0 + r, y0));
                b.line_to(Point::new(x1 - r, y0));
                b.quadratic_curve_to(Point::new(x1, y0), Point::new(x1, y0 + r));
                b.line_to(Point::new(x1, y1 - r));
                b.quadratic_curve_to(Point::new(x1, y1), Point::new(x1 - r, y1));
                b.line_to(Point::new(x0 + r, y1));
                b.quadratic_curve_to(Point::new(x0, y1), Point::new(x0, y1 - r));
                b.line_to(Point::new(x0, y0 + r));
                b.quadratic_curve_to(Point::new(x0, y0), Point::new(x0 + r, y0));
                b.close();
            })
        }
    }

    #[derive(Debug, Clone, Copy, Default)]
    /// `offset` fica de enfeite: o pelicano começa o tracejado sempre do início.
    #[allow(dead_code)]
    pub struct LineDash<'a> { pub segments: &'a [f32], pub offset: usize }

    #[derive(Debug, Clone, Copy)]
    pub struct Stroke<'a> {
        pub style: Color,
        pub width: f32,
        pub line_cap: LineCap,
        pub line_join: LineJoin,
        pub line_dash: LineDash<'a>,
    }

    impl Default for Stroke<'_> {
        fn default() -> Self {
            Stroke { style: Color::BLACK, width: 1.0, line_cap: LineCap::Butt, line_join: LineJoin::Miter, line_dash: LineDash::default() }
        }
    }

    impl<'a> Stroke<'a> {
        pub fn with_color(self, c: Color) -> Self { Stroke { style: c, ..self } }
        pub fn with_width(self, w: f32) -> Self { Stroke { width: w, ..self } }
        pub fn with_line_cap(self, c: LineCap) -> Self { Stroke { line_cap: c, ..self } }
        pub fn with_line_join(self, j: LineJoin) -> Self { Stroke { line_join: j, ..self } }
    }

    #[derive(Debug, Clone)]
    pub struct Text {
        pub content: String,
        pub position: Point,
        pub color: Color,
        pub size: f32,
        pub font: Font,
        pub horizontal_alignment: Align,
        pub vertical_alignment: Align,
    }

    impl Default for Text {
        fn default() -> Self {
            Text { content: String::new(), position: Point::ORIGIN, color: Color::BLACK, size: 16.0, font: Font::default(),
                horizontal_alignment: Align::Start, vertical_alignment: Align::Start }
        }
    }

    enum Op {
        Encher(pelicano::Path, Color),
        Tracar(pelicano::Path, pelicano::Stroke, Color),
        Texto(Text),
    }

    pub struct Geometry(Vec<Op>);

    pub struct Frame { ops: Vec<Op> }

    impl Frame {
        pub fn new(_: &Renderer, _: Size) -> Frame { Frame { ops: Vec::new() } }
        pub fn fill(&mut self, p: &Path, c: impl Into<Color>) {
            if let Some(p) = &p.0 { self.ops.push(Op::Encher(p.clone(), c.into())); }
        }
        pub fn fill_rectangle(&mut self, p: Point, s: Size, c: impl Into<Color>) { self.fill(&Path::rectangle(p, s), c); }
        pub fn stroke(&mut self, p: &Path, s: Stroke<'_>) {
            let Some(p) = &p.0 else { return };
            let t = pelicano::Stroke::new(s.width).cap(s.line_cap).join(s.line_join).dash(s.line_dash.segments.to_vec());
            self.ops.push(Op::Tracar(p.clone(), t, s.style));
        }
        pub fn fill_text(&mut self, t: impl Into<Text>) { self.ops.push(Op::Texto(t.into())); }
        pub fn into_geometry(self) -> Geometry { Geometry(self.ops) }
    }

    pub trait Program<M> {
        type State: Default + 'static;
        fn update(&self, _: &mut Self::State, _: Event, _: Rect, _: mouse::Cursor) -> (event::Status, Option<M>) {
            (event::Status::Ignored, None)
        }
        fn draw(&self, state: &Self::State, renderer: &Renderer, theme: &Theme, bounds: Rect, cursor: mouse::Cursor) -> Vec<Geometry>;
        fn mouse_interaction(&self, _: &Self::State, _: Rect, _: mouse::Cursor) -> mouse::Interaction {
            mouse::Interaction::default()
        }
    }

    pub struct Canvas<P> { p: Adaptador<P>, w: Length, h: Length }

    impl<P> Canvas<P> {
        pub fn new(p: P) -> Self { Canvas { p: Adaptador(p), w: Length::FILL, h: Length::FILL } }
        pub fn width(self, w: impl Into<Length>) -> Self { Canvas { w: w.into(), ..self } }
        pub fn height(self, h: impl Into<Length>) -> Self { Canvas { h: h.into(), ..self } }
    }

    pub struct Adaptador<P>(P);

    fn cursor(c: Option<Point>) -> mouse::Cursor { c.map_or(mouse::Cursor::Unavailable, mouse::Cursor::Available) }

    impl<M, P: Program<M>> pelicano::Program<M> for Adaptador<P> {
        type State = P::State;
        fn draw(&self, st: &P::State, r: &mut Renderer, b: Rect, theme: &Theme, c: Option<Point>) {
            let geo = self.0.draw(st, r, theme, b, cursor(c));
            r.push_transform(Transform::translate(b.x, b.y));
            for Geometry(ops) in geo {
                for op in ops {
                    match op {
                        Op::Encher(p, cor) => r.fill_path(&p, cor, pelicano::FillRule::NonZero),
                        Op::Tracar(p, t, cor) => r.stroke_path(&p, &t, cor),
                        Op::Texto(t) => {
                            let m = r.measure(&t.content, t.size, t.font, None);
                            let dx = match t.horizontal_alignment { Align::Center => m.width / 2.0, Align::End => m.width, _ => 0.0 };
                            let dy = match t.vertical_alignment { Align::Center => m.height / 2.0, Align::End => m.height, _ => 0.0 };
                            r.fill_text(&t.content, t.size, t.font, t.color, Point::new(t.position.x - dx, t.position.y - dy));
                        }
                    }
                }
            }
            r.pop_transform();
        }
        fn update(&self, st: &mut P::State, ev: &pelicano::Event, b: Rect, c: Option<Point>, publish: &mut dyn FnMut(M)) -> pelicano::Captured {
            let ev = match *ev {
                pelicano::Event::CursorMoved(p) => mouse::Event::CursorMoved { position: p },
                pelicano::Event::MousePressed(bt) => mouse::Event::ButtonPressed(bt),
                pelicano::Event::MouseReleased(bt) => mouse::Event::ButtonReleased(bt),
                _ => return pelicano::Captured::No,
            };
            let (s, m) = self.0.update(st, Event::Mouse(ev), b, cursor(c));
            if let Some(m) = m { publish(m); }
            if s == event::Status::Captured { pelicano::Captured::Yes } else { pelicano::Captured::No }
        }
        fn mouse_interaction(&self, st: &P::State, b: Rect, c: Option<Point>) -> pelicano::CursorIcon {
            self.0.mouse_interaction(st, b, cursor(c)).icone()
        }
    }

    impl<'a, M: 'a, P: Program<M> + 'a> From<Canvas<P>> for Element<'a, M> {
        fn from(c: Canvas<P>) -> Self { pelicano::program(c.p).width(c.w).height(c.h).into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector { pub x: f32, pub y: f32 }

impl Vector {
    pub const fn new(x: f32, y: f32) -> Self { Vector { x, y } }
}

impl std::ops::Add for Vector {
    type Output = Vector;
    fn add(self, o: Vector) -> Vector { Vector::new(self.x + o.x, self.y + o.y) }
}

/// A imagem dos pixels RGBA; com tamanho errado, um pixel transparente (não derruba a tela).
pub fn imagem_rgba(largura: u32, altura: u32, rgba: Vec<u8>) -> Image {
    Image::from_rgba(largura, altura, rgba).unwrap_or_else(|| Image::from_rgba(1, 1, vec![0; 4]).expect("1×1"))
}

/// A imagem de um arquivo PNG ou JPEG (o `Image::from_path` do Iced).
pub fn imagem_arquivo(caminho: impl AsRef<std::path::Path>) -> Image {
    std::fs::read(caminho).ok()
        .and_then(|b| Image::from_png(&b).or_else(|| {
            let i = image::load_from_memory(&b).ok()?.to_rgba8();
            Image::from_rgba(i.width(), i.height(), i.into_raw())
        }))
        .unwrap_or_else(|| imagem_rgba(1, 1, vec![0; 4]))
}

/// O botão só de texto do Iced (`button::text`): sem fundo, texto no tom do tema, realce leve.
pub fn botao_texto(_: &Theme, s: Status) -> Style {
    Style {
        background: matches!(s, Status::Hovered | Status::Pressed).then(|| Background::Color(feather_ui::tema::COR_HOVER())),
        text_color: Some(feather_ui::tema::COR_TEXTO()),
        border: Border { radius: [4.0; 4], ..Border::default() },
        ..Style::default()
    }
}

/// O campo de texto padrão do Iced (`text_input::default`): fundo do painel, borda, azul no foco.
pub fn campo_padrao(_: &Theme, s: Status) -> Style {
    use feather_ui::tema::*;
    Style {
        background: Some(Background::Color(COR_PAINEL())),
        text_color: Some(COR_TEXTO()),
        border: Border { color: if s == Status::Focused { COR_AZUL_ACENTO() } else { COR_BORDA() }, width: 1.0, radius: [4.0; 4] },
        ..Style::default()
    }
}

/// Imagem dentro de mensagem: o `Image` do pelicano não tem `Debug`.
#[derive(Clone)]
pub struct Quadro(pub Image);

impl std::fmt::Debug for Quadro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "Quadro({:?})", self.0.size()) }
}

/// O `pick_list` do Iced, que aceitava a lista por valor; o do pelicano empresta `&'a [T]`.
/// shortcut: cada lista diferente (pelo texto das opções) fica guardada para sempre; trocar por
/// listas guardadas no estado se alguma lista crescer sem limite.
pub fn suspensa<'a, T, M>(opcoes: impl Into<Vec<T>>, escolhida: Option<T>, f: impl Fn(T) -> M + 'a) -> PickList<'a, T, M>
where T: Clone + PartialEq + ToString + 'static {
    use std::{any::{Any, TypeId}, cell::RefCell, collections::HashMap};
    thread_local! { static GUARDADAS: RefCell<HashMap<(TypeId, Vec<String>), Box<dyn Any>>> = RefCell::default(); }
    let v: Vec<T> = opcoes.into();
    let chave = (TypeId::of::<T>(), v.iter().map(ToString::to_string).collect());
    let lista: &'static [T] = GUARDADAS.with(|g| {
        let mut g = g.borrow_mut();
        let e = g.entry(chave).or_insert_with(|| Box::new(&*Box::leak(v.into_boxed_slice())));
        *e.downcast_ref::<&'static [T]>().expect("mesmo tipo")
    });
    pick_list(lista, escolhida, f)
}
