//! Painel Conferir assinaturas: cada lugar que pede assinatura (linha, rótulo, campo digital)
//! com "assinado" ou "em branco" pela tinta na página, e as páginas sem rubrica quando as outras
//! têm. O motor está em `featherpdf_core::{locais_de_assinatura, tinta, rubricas_faltando}`.

use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::{LocalAssinatura, TINTA_ASSINADO};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

/// Canto de baixo à direita, onde vai a rubrica (fração da página, y para baixo).
const CANTO: [f32; 4] = [0.55, 0.86, 1.0, 1.0];
/// shortcut: a rubrica só é conferida até esta página, para não desenhar um livro inteiro; subir se pedirem.
const MAX_PAGINAS: u32 = 300;

#[derive(Debug, Clone)]
pub enum Rc {
    Abrir,
    Lido(Result<(Vec<(LocalAssinatura, bool)>, Vec<u32>), String>),
}

#[derive(Default)]
pub struct Conferencia { pub resultado: Option<Result<(Vec<(LocalAssinatura, bool)>, Vec<u32>), String>> }

impl App {
    pub fn update_reconhecer(&mut self, m: Rc) -> Task<Msg> {
        match m {
            Rc::Abrir => {
                self.painel = if self.painel == Some(Painel::Reconhecer) { None } else { Some(Painel::Reconhecer) };
                let Some(a) = &self.aberto else { return Task::none() };
                if self.painel.is_none() { return Task::none(); }
                let locais = match a.pdf.locais_de_assinatura() { Ok(l) => l, Err(e) => { self.reconhecer.resultado = Some(Err(e)); return Task::none() } };
                // Frações calculadas aqui (o documento fica na janela); o desenho vai para outra thread.
                let fracoes: Vec<([f32; 4], Vec<[f32; 4]>)> = locais.iter().map(|l| (
                    a.pdf.fracao_na_pagina(l.pagina, l.caixa).unwrap_or_default(),
                    l.ignorar.iter().filter_map(|&c| a.pdf.fracao_na_pagina(l.pagina, c).ok()).collect(),
                )).collect();
                let (vis, total) = (a.vis.clone(), a.pdf.total_paginas().min(MAX_PAGINAS));
                self.reconhecer.resultado = None;
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let mut assinado = vec![false; locais.len()];
                        let mut canto = Vec::new();
                        for p in 1..=total {
                            let r = vis.desenhar(p, 1.5)?;
                            for (i, l) in locais.iter().enumerate().filter(|(_, l)| l.pagina == p) {
                                assinado[i] = l.digital.unwrap_or_else(|| featherpdf_core::tinta(&r.rgba, r.largura, r.altura, fracoes[i].0, &fracoes[i].1) > TINTA_ASSINADO);
                            }
                            canto.push(featherpdf_core::tinta(&r.rgba, r.largura, r.altura, CANTO, &[]));
                        }
                        let sem_rubrica = featherpdf_core::rubricas_faltando(&canto).into_iter().map(|i| i as u32 + 1).collect();
                        Ok((locais.into_iter().zip(assinado).collect(), sem_rubrica))
                    }).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Reconhecer(Rc::Lido(r)));
            }
            Rc::Lido(r) => {
                if let Ok((l, _)) = &r {
                    let brancos = l.iter().filter(|(_, a)| !a).count();
                    self.status = if l.is_empty() { "Nenhum lugar de assinatura achado.".into() } else { format!("{} lugar(es) de assinatura, {brancos} em branco.", l.len()) };
                }
                self.reconhecer.resultado = Some(r);
            }
        }
        Task::none()
    }

    pub fn painel_reconhecer(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let (locais, sem_rubrica) = match &self.reconhecer.resultado {
            None => return text("Procurando assinaturas…").size(13).color(muted).into(),
            Some(Err(e)) => return text(e.clone()).size(13).color(muted).into(),
            Some(Ok(r)) => r,
        };
        let mut corpo = column![].spacing(6);
        if locais.is_empty() {
            corpo = corpo.push(text("Não achei linha de assinatura, rótulo “Assinatura”/“Rubrica” nem campo de assinatura. Em página escaneada, rode o OCR antes.").size(13).color(muted));
        }
        for (l, assinado) in locais {
            let (rotulo, cor, fundo) = if *assinado {
                ("Assinado", Color::rgb8(0x1e, 0x5c, 0x35), Color::rgb8(0xe6, 0xf1, 0xea))
            } else {
                ("Em branco", Color::rgb8(0x8f, 0x22, 0x19), Color::rgb8(0xfd, 0xec, 0xeb))
            };
            let tipo = if l.digital.is_some() { "assinatura digital" } else { "à mão" };
            corpo = corpo.push(button(column![
                row![text(rotulo).size(11).color(cor), Space::with_width(Length::FILL), text(format!("p. {}", l.pagina)).size(11).color(muted)],
                text(l.rotulo.clone()).size(12.5),
                text(tipo).size(11).color(muted),
            ].spacing(2)).width(Length::FILL).padding([6, 8]).style(move |_, _| Style {
                background: Some(Background::Color(fundo)), border: Border { radius: [6.0; 4], ..Border::default() }, ..Default::default()
            }).on_press(Msg::IrPagina(l.pagina)));
        }
        if !sem_rubrica.is_empty() {
            let lista = sem_rubrica.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
            corpo = corpo.push(dlg::aviso("alert-triangle", format!("Pode faltar rubrica na(s) página(s) {lista}: as outras têm marca no canto de baixo.")));
        }
        corpo = corpo.push(text("A tinta é medida na área em cima da linha ou do rótulo; confira na página antes de dar por assinado.").size(11.5).color(muted));
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
