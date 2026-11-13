//! Painel Comparar: o documento aberto (versão nova) contra outro PDF (a antiga).
//! "Texto" lista o que saiu e o que entrou, com a página de cada lado; "Sobrepor" mostra a
//! página atual das duas por cima uma da outra (vermelho só na antiga, azul só na nova).
//! O motor está em `featherpdf_core::comparar_texto`/`sobrepor`.

use std::path::PathBuf;

use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::{Mudanca, Trecho};
use featherpdf_io::Visualizador;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone)]
pub enum Cmp {
    Abrir,
    Escolher,
    Escolhido(Option<PathBuf>),
    /// Caminho, bytes da antiga (o desenhador nasce na thread da janela) e as diferenças.
    Lido(Result<(PathBuf, Vec<u8>, Vec<Trecho>), String>),
    Modo(bool),
    Sobrepor,
    Sobreposta(u32, Result<(u32, u32, Vec<u8>, (i32, i32)), String>),
}

#[derive(Default)]
pub struct Comparacao {
    pub outro: Option<(PathBuf, Visualizador)>,
    pub trechos: Vec<Trecho>,
    /// `true`: sobrepor; `false`: texto.
    pub sobrepor: bool,
    pub imagem: Option<(u32, Image, (i32, i32))>,
    pub lendo: bool,
}

impl App {
    pub fn update_comparar(&mut self, m: Cmp) -> Task<Msg> {
        match m {
            Cmp::Abrir => {
                self.painel = if self.painel == Some(Painel::Comparar) { None } else { Some(Painel::Comparar) };
                if self.painel.is_some() && self.comparar.outro.is_none() { return self.update_comparar(Cmp::Escolher); }
            }
            Cmp::Escolher => return Task::perform(async {
                rfd::AsyncFileDialog::new().set_title("Versão antiga para comparar").add_filter("PDF", &["pdf", "PDF"]).pick_file().await.map(|h| h.path().to_path_buf())
            }, |p| Msg::Comparar(Cmp::Escolhido(p))),
            Cmp::Escolhido(None) => {}
            Cmp::Escolhido(Some(p)) => if let Some(a) = &self.aberto {
                let nova = a.pdf.clone();
                self.comparar.lendo = true;
                self.status = "Comparando…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let antiga = featherpdf_io::abrir(&p)?;
                        let trechos = featherpdf_core::comparar_texto(&antiga, &nova);
                        Ok((p, antiga.serializar()?, trechos))
                    }).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Comparar(Cmp::Lido(r)));
            },
            Cmp::Lido(r) => {
                self.comparar.lendo = false;
                match r {
                    Ok((p, bytes, trechos)) => {
                        let vis = match Visualizador::novo(bytes) { Ok(v) => v, Err(e) => { self.status = e; return Task::none() } };
                        let (saiu, entrou) = (trechos.iter().filter(|t| t.mudanca == Mudanca::Saiu).count(), trechos.iter().filter(|t| t.mudanca == Mudanca::Entrou).count());
                        self.status = format!("{saiu} trecho(s) saíram e {entrou} entraram em relação a {}.", p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()));
                        self.comparar.outro = Some((p, vis));
                        self.comparar.trechos = trechos;
                        self.comparar.imagem = None;
                        if self.comparar.sobrepor { return self.update_comparar(Cmp::Sobrepor); }
                    }
                    Err(e) => self.status = e,
                }
            }
            Cmp::Modo(s) => { self.comparar.sobrepor = s; if s { return self.update_comparar(Cmp::Sobrepor); } }
            Cmp::Sobrepor => {
                let (Some(a), Some((_, outro))) = (&self.aberto, &self.comparar.outro) else { return Task::none() };
                let (nova, antiga, pagina) = (a.vis.clone(), outro.clone(), self.pagina);
                if pagina > antiga.total_paginas() { self.comparar.imagem = None; self.status = format!("A versão antiga não tem a página {pagina}."); return Task::none(); }
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        // A mesma escala para as duas: a largura da nova em 900 px.
                        let (l, _) = nova.tamanhos()[pagina as usize - 1];
                        let escala = 900.0 / l.max(1.0);
                        let b = nova.desenhar(pagina, escala)?;
                        let a = antiga.desenhar(pagina, escala)?;
                        // Recorta a antiga no tamanho da nova (páginas de tamanho diferente ficam alinhadas no canto).
                        let mut a2 = vec![255u8; (b.largura * b.altura * 4) as usize];
                        for y in 0..b.altura.min(a.altura) {
                            let n = (b.largura.min(a.largura) * 4) as usize;
                            let (o, d) = ((y * a.largura * 4) as usize, (y * b.largura * 4) as usize);
                            a2[d..d + n].copy_from_slice(&a.rgba[o..o + n]);
                        }
                        let (img, desl) = featherpdf_core::sobrepor(&a2, &b.rgba, b.largura, b.altura);
                        Ok((b.largura, b.altura, img, desl))
                    }).await.unwrap_or_else(|e| Err(e.to_string()))
                }, move |r| Msg::Comparar(Cmp::Sobreposta(pagina, r)));
            }
            Cmp::Sobreposta(p, Ok((l, h, img, d))) => self.comparar.imagem = Some((p, imagem_rgba(l, h, img), d)),
            Cmp::Sobreposta(_, Err(e)) => self.status = e,
        }
        Task::none()
    }

    pub fn painel_comparar(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let c = &self.comparar;
        let Some((caminho, _)) = &c.outro else {
            return column![
                text(if c.lendo { "Comparando…" } else { "Escolha a versão antiga do mesmo documento. O aberto é tratado como a nova." }).size(13).color(muted),
                button(text("Escolher a versão antiga…").size(13)).padding([6, 12]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Comparar(Cmp::Escolher)),
            ].spacing(10).into();
        };
        let modo = |rotulo: &'static str, s: bool| button(text(rotulo).size(12).center()).width(Length::FILL).padding([5, 4])
            .style(tema::estilo_botao_pequeno(c.sobrepor == s)).on_press(Msg::Comparar(Cmp::Modo(s)));
        let nome = caminho.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let mut corpo = column![
            row![text(format!("Contra: {nome}")).size(12).color(muted).width(Length::FILL),
                button(text("Trocar").size(12)).padding([2, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Comparar(Cmp::Escolher))].align_y(Align::Center),
            row![modo("Texto", false), modo("Sobrepor", true)].spacing(4),
        ].spacing(10);
        if c.sobrepor {
            let legenda = |cor: Color, t: &'static str| row![container(Space::new(10, 10)).style(move |_| Style { background: Some(Background::Color(cor)), ..Default::default() }), text(t).size(11.5)].spacing(4).align_y(Align::Center);
            corpo = corpo.push(row![legenda(Color::rgb8(0xd0, 0x30, 0x30), "só na antiga"), legenda(Color::rgb8(0x30, 0x50, 0xd0), "só na nova"), legenda(Color::rgb8(0x60, 0x60, 0x60), "igual")].spacing(10));
            match &c.imagem {
                Some((p, h, d)) if *p == self.pagina => {
                    corpo = corpo.push(image(h.clone()).width(Length::FILL));
                    if *d != (0, 0) { corpo = corpo.push(text(format!("A nova foi alinhada {} px na horizontal e {} px na vertical.", d.0, d.1)).size(11.5).color(muted)); }
                }
                _ => corpo = corpo.push(button(text(format!("Sobrepor a página {}", self.pagina)).size(12)).padding([5, 10]).width(Length::FILL)
                    .style(tema::estilo_botao_pequeno(false)).on_press(Msg::Comparar(Cmp::Sobrepor))),
            }
        } else {
            let mudancas: Vec<(usize, &Trecho)> = c.trechos.iter().enumerate().filter(|(_, t)| t.mudanca != Mudanca::Igual).collect();
            if mudancas.is_empty() { corpo = corpo.push(dlg::aviso("circle-check", "O texto é o mesmo nas duas versões.")); }
            for (i, t) in mudancas.into_iter().take(300) {
                let (cor, fundo, rotulo) = match t.mudanca {
                    Mudanca::Saiu => (Color::rgb8(0x8f, 0x22, 0x19), Color::rgb8(0xfd, 0xec, 0xeb), "Saiu"),
                    _ => (Color::rgb8(0x1e, 0x5c, 0x35), Color::rgb8(0xe6, 0xf1, 0xea), "Entrou"),
                };
                let (cor, fundo) = (crate::inicio::texto_escuro(cor), crate::inicio::fundo_claro(fundo));
                // Um pouco do texto igual em volta, para achar o lugar.
                let antes = c.trechos[..i].iter().rev().find(|x| x.mudanca == Mudanca::Igual).map(|x| x.texto.split(' ').rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ")).unwrap_or_default();
                let pagina = t.pagina_nova.or_else(|| c.trechos[..i].iter().rev().find_map(|x| x.pagina_nova)).unwrap_or(1);
                let mut texto: String = t.texto.chars().take(240).collect();
                if t.texto.chars().count() > 240 { texto.push('…'); }
                corpo = corpo.push(button(column![
                    row![text(rotulo).size(11).color(cor), Space::with_width(Length::FILL), text(format!("p. {pagina}")).size(11).color(muted)],
                    text(format!("…{antes}")).size(11.5).color(muted),
                    text(texto).size(12.5).color(cor),
                ].spacing(2)).width(Length::FILL).padding([6, 8]).style(move |_, _| Style {
                    background: Some(Background::Color(fundo)), border: Border { radius: [6.0; 4], ..Border::default() }, ..Default::default()
                }).on_press(Msg::IrPagina(pagina)));
            }
        }
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
