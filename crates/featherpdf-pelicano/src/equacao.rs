//! Painel Equação: digitar LaTeX e pôr a equação vetorial na página atual. O motor está em
//! `DocumentoPdf::inserir_equacao`.

use feather_ui::tema;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone)]
pub enum Eq { Abrir, Texto(String), Tamanho(f32), Inserir }

pub struct Equacao { pub latex: String, pub tamanho: f32 }

impl Default for Equacao {
    fn default() -> Self { Self { latex: r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}".into(), tamanho: 16.0 } }
}

impl App {
    pub fn update_equacao(&mut self, m: Eq) -> Task<Msg> {
        match m {
            Eq::Abrir => self.painel = if self.painel == Some(Painel::Equacao) { None } else { Some(Painel::Equacao) },
            Eq::Texto(t) => self.equacao.latex = t,
            Eq::Tamanho(t) => self.equacao.tamanho = t,
            Eq::Inserir => {
                let (pagina, latex, tamanho) = (self.pagina, self.equacao.latex.clone(), self.equacao.tamanho);
                return self.editar(move |pdf| {
                    // No alto da página, dentro da margem: depois se arrasta com a ferramenta Editar.
                    let (geo, _) = pdf.geometria(pagina)?;
                    pdf.inserir_equacao(pagina, geo[0] + 72.0, geo[3] - 72.0, &latex, tamanho)?;
                    Ok(format!("Equação na página {pagina} (arraste com Editar para o lugar; Ctrl+Z desfaz)."))
                });
            }
        }
        Task::none()
    }

    pub fn painel_equacao(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let tam = |t: f32| button(text(format!("{t:.0}")).size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno((self.equacao.tamanho - t).abs() < 0.1)).on_press(Msg::Equacao(Eq::Tamanho(t)));
        column![
            text_input("LaTeX, ex.: \\sum_{i=1}^{n} x_i^2", &self.equacao.latex).on_input(|t| Msg::Equacao(Eq::Texto(t))).on_submit(Msg::Equacao(Eq::Inserir)).size(13).padding([6, 8]),
            row![text("Tamanho").size(12).width(70), tam(12.0), tam(16.0), tam(24.0), tam(36.0)].spacing(4).align_y(Align::Center),
            button(text(format!("Inserir na página {}", self.pagina)).size(13)).width(Length::FILL).padding([6, 10]).style(tema::estilo_botao_pequeno(true))
                .on_press_maybe(self.aberto.is_some().then_some(Msg::Equacao(Eq::Inserir))),
            text("Entende \\frac, \\sqrt, ^ e _, letras gregas (\\alpha, \\Omega…), \\sum, \\int, \\infty, \\leq, \\geq, \\neq, \\pm, \\times, \\cdot, \\approx, \\to, \\partial e espaços \\, \\; \\quad.").size(11.5).color(muted),
        ].spacing(10).into()
    }
}
