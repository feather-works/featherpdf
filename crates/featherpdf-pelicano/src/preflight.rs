//! Painel do preflight: escolhe o destino (gráfica, escritório, tela) e lista o que atrapalha,
//! no mesmo desenho da lista do PDF/A. Clicar numa página leva até ela.

use feather_ui::{icones, tema};
use featherpdf_core::{DestinoPreflight as Destino, ItemPreflight, NivelPreflight as N};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone)]
pub enum Pf { Abrir, Destino(Destino) }

pub struct Preflight { pub destino: Destino, pub itens: Vec<ItemPreflight> }

impl Default for Preflight {
    fn default() -> Self { Self { destino: Destino::Grafica, itens: vec![] } }
}

fn cores(n: N) -> (&'static str, Color, Color) {
    let (nome, icone, texto) = match n {
        N::Ok => ("circle-check", Color::rgb8(0x28, 0x78, 0x46), Color::rgb8(0x1e, 0x5c, 0x35)),
        N::Aviso => ("alert-triangle", Color::rgb8(0xa3, 0x6a, 0x00), Color::rgb8(0x6b, 0x4e, 0x05)),
        N::Erro => ("alert-triangle", Color::rgb8(0xb3, 0x26, 0x1e), Color::rgb8(0x8f, 0x22, 0x19)),
    };
    (nome, crate::inicio::texto_escuro(icone), crate::inicio::texto_escuro(texto))
}

impl App {
    pub fn update_preflight(&mut self, m: Pf) {
        match m {
            Pf::Abrir => self.painel = if self.painel == Some(Painel::Preflight) { None } else { Some(Painel::Preflight) },
            Pf::Destino(d) => self.preflight.destino = d,
        }
        if let Some(a) = &self.aberto { self.preflight.itens = a.pdf.preflight(self.preflight.destino); }
    }

    pub fn painel_preflight(&self) -> Element<'_, Msg> {
        let p = &self.preflight;
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;
        let escolha = |rotulo: &'static str, d: Destino| button(text(rotulo).size(12).center()).width(Length::FILL).padding([5, 4])
            .style(tema::estilo_botao_pequeno(p.destino == d)).on_press(Msg::Preflight(Pf::Destino(d)));
        let conta = |n: N| p.itens.iter().filter(|i| i.nivel == n).count();
        let resumo = match (conta(N::Erro), conta(N::Aviso)) {
            (0, 0) => "Pronto para esse destino.".to_string(),
            (e, a) => format!("{e} erro(s) e {a} aviso(s)."),
        };
        let mut lista = column![].spacing(6);
        for i in &p.itens {
            let (nome, cor, escura) = cores(i.nivel);
            let mut t = column![text(i.regra).size(13).font(negrito)].spacing(3);
            if i.nivel != N::Ok || !i.detalhe.is_empty() { t = t.push(text(i.detalhe.clone()).size(12).color(if i.nivel == N::Ok { muted } else { escura })); }
            if !i.paginas.is_empty() {
                let mut pags = row![text("Páginas:").size(11).color(muted)].spacing(4).align_y(Align::Center);
                for &n in i.paginas.iter().take(8) {
                    pags = pags.push(button(text(n.to_string()).size(11)).padding([1, 6]).style(tema::estilo_botao_pequeno(n == self.pagina)).on_press(Msg::IrPagina(n)));
                }
                if i.paginas.len() > 8 { pags = pags.push(text(format!("+{}", i.paginas.len() - 8)).size(11).color(muted)); }
                t = t.push(pags);
            }
            let icone = icones::icone_traco(nome, 16.).color(cor);
            lista = lista.push(container(row![icone, t].spacing(10)).width(Length::FILL).padding([8, 10]).style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL())),
                border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [6.0; 4] },
                ..Default::default()
            }));
        }
        column![
            text("Para onde vai o PDF?").size(12).color(muted),
            row![escolha("Gráfica", Destino::Grafica), escolha("Escritório", Destino::Escritorio), escolha("Tela", Destino::Tela)].spacing(4),
            row![text(resumo).size(13).font(negrito), Space::with_width(Length::FILL),
                button(text("Verificar de novo").size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Preflight(Pf::Destino(p.destino)))]
                .align_y(Align::Center),
            lista,
            text("Só confere; nada muda no arquivo. Fontes e imagens: Converter → Comprimir; comentários: Proteger → Achatar.").size(11).color(muted),
        ].spacing(10).into()
    }
}
