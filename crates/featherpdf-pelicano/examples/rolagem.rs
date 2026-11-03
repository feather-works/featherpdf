//! Teste só do pelicano, sem PDF: 30 páginas falsas numa rolagem, Ctrl+roda dá zoom.
//! Serve para separar o peso do pelicano (pintura, rolagem, imagem esticada) do peso do FeatherPDF.
//! `cargo run --release -p featherpdf-pelicano --example rolagem`; `PELICANO_DANO=1` mostra o tempo de cada quadro.
//! `PELICANO_GPU=nenhuma` força a pintura pela CPU.
use pelicano::*;

const PAGINAS: usize = 30;
// A4 em 150 dpi: o tamanho de um desenho de página com zoom ~150%.
const L: u32 = 1240;
const A: u32 = 1754;

struct Teste { paginas: Vec<Image>, zoom: f32, ctrl: bool }

#[derive(Debug, Clone)]
enum Msg { Roda(f32), Ctrl(bool) }

/// Página com "linhas de texto" e um bloco colorido, para a imagem não ser lisa.
fn pagina(n: usize) -> Image {
    let mut rgba = vec![255u8; (L * A * 4) as usize];
    for y in 0..A {
        for x in 0..L {
            let i = ((y * L + x) * 4) as usize;
            let linha = y > 150 && y < A - 150 && x > 120 && x < L - 120 && (y / 18) % 2 == 0 && (x / 7 + y / 36 + n as u32) % 13 != 0;
            let bloco = y > 300 && y < 700 && x > 700 && x < 1100;
            if bloco { rgba[i..i + 3].copy_from_slice(&[(n * 40 % 255) as u8, 120, 200]); }
            else if linha { rgba[i..i + 3].copy_from_slice(&[40, 40, 40]); }
        }
    }
    Image::from_rgba(L, A, rgba).expect("imagem")
}

impl App for Teste {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Roda(dy) => self.zoom = (self.zoom * if dy > 0.0 { 1.1 } else { 1.0 / 1.1 }).clamp(0.1, 8.0),
            Msg::Ctrl(c) => self.ctrl = c,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let (l, a) = (595.0 * self.zoom, 842.0 * self.zoom);
        let mut coluna = pelicano::column![].spacing(16.0).padding(16.0);
        for p in &self.paginas { coluna = coluna.push(image(p.clone()).width(l).height(a)); }
        let mut dentro = mouse_area(coluna.width(Length::FILL));
        // Igual ao FeatherPDF: com Ctrl a área pega a roda antes do scrollable.
        if self.ctrl { dentro = dentro.on_scroll(|_, dy, _| Msg::Roda(dy.signum())); }
        pelicano::column![
            text(format!("Zoom {:.0}% · Ctrl+roda para zoom", self.zoom * 100.0)).size(14.0),
            scrollable(dentro).direction(Direction::Both).width(Length::FILL).height(Length::FILL),
        ].into()
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        match ev { Event::ModifiersChanged(m) => Some(Msg::Ctrl(m.control_key())), _ => None }
    }
}

fn main() -> Result<(), Error> {
    pelicano::run(Settings { title: "Teste de rolagem".into(), size: Size::new(1280.0, 820.0), ..Default::default() }, || {
        (Teste { paginas: (0..PAGINAS).map(pagina).collect(), zoom: 1.0, ctrl: false }, Task::none())
    })
}
