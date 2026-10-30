//! Painel Versões: a linha do tempo das atualizações incrementais guardadas no próprio arquivo
//! (cada assinatura e cada salvamento rápido do Acrobat deixa uma). Abrir uma versão traz o
//! documento como era, sem nome, para comparar ou salvar à parte.

use std::path::PathBuf;

use feather_ui::tema;
use featherpdf_core::Versao;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone)]
pub enum Vs {
    Abrir,
    Lidas(PathBuf, Result<Vec<Versao>, String>),
    AbrirVersao(usize),
}

#[derive(Default)]
pub struct Versoes { pub arquivo: Option<PathBuf>, pub lista: Option<Result<Vec<Versao>, String>> }

impl App {
    pub fn update_versoes(&mut self, m: Vs) -> Task<Msg> {
        match m {
            Vs::Abrir => {
                self.painel = if self.painel == Some(Painel::Versoes) { None } else { Some(Painel::Versoes) };
                let Some(caminho) = self.aberto.as_ref().and_then(|a| a.pdf.origem()).map(|p| p.to_path_buf()) else {
                    self.versoes.lista = Some(Err("O documento ainda não foi salvo: as versões ficam no arquivo.".into()));
                    return Task::none();
                };
                self.versoes.lista = None;
                return Task::perform(async move {
                    let c = caminho.clone();
                    let r = tokio::task::spawn_blocking(move || std::fs::read(&c).map_err(|e| e.to_string()).map(|b| featherpdf_core::versoes(&b)))
                        .await.unwrap_or_else(|e| Err(e.to_string()));
                    (caminho, r)
                }, |(c, r)| Msg::Versoes(Vs::Lidas(c, r)));
            }
            Vs::Lidas(c, r) => { self.versoes.arquivo = Some(c); self.versoes.lista = Some(r); }
            Vs::AbrirVersao(i) => {
                let (Some(caminho), Some(Ok(lista))) = (&self.versoes.arquivo, &self.versoes.lista) else { return Task::none() };
                let Some(fim) = lista.get(i).map(|v| v.fim) else { return Task::none() };
                let aberto = std::fs::read(caminho).map_err(|e| e.to_string()).and_then(|b| {
                    let bytes = b[..fim.min(b.len())].to_vec();
                    Ok((featherpdf_core::DocumentoPdf::de_bytes(&bytes, None)?, featherpdf_io::Visualizador::novo(bytes)?))
                });
                match aberto {
                    Ok((pdf, vis)) => {
                        let tarefa = self.entrar(pdf, vis);
                        self.status = format!("Versão {} aberta como documento novo (o arquivo não muda). Salve à parte com Ctrl+Shift+S.", i + 1);
                        return tarefa;
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        Task::none()
    }

    pub fn painel_versoes(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let corpo = match &self.versoes.lista {
            None => column![text("Lendo o arquivo…").size(13).color(muted)],
            Some(Err(e)) => column![text(e.clone()).size(13).color(muted)],
            Some(Ok(v)) if v.len() <= 1 => column![text("Este arquivo tem uma versão só: ninguém o assinou nem salvou por cima com atualização incremental.").size(13).color(muted)],
            Some(Ok(v)) => {
                let mut c = column![text(format!("{} versões guardadas no arquivo, da mais nova para a mais antiga.", v.len())).size(12.5).color(muted)].spacing(6);
                for (i, ver) in v.iter().enumerate().rev() {
                    let atual = i + 1 == v.len();
                    let mut detalhe = format!("{} página(s) · {}", ver.paginas, featherpdf_core::tamanho(ver.fim));
                    if ver.assinaturas > 0 { detalhe += &format!(" · {} assinatura(s)", ver.assinaturas); }
                    let mut linha = row![column![
                        text(format!("Versão {}{}", i + 1, if atual { " (atual)" } else { "" })).size(13),
                        text(ver.data.clone().unwrap_or_else(|| "sem data".into())).size(11.5).color(muted),
                        text(detalhe).size(11.5).color(muted),
                    ].spacing(1), Space::with_width(Length::FILL)].align_y(Align::Center);
                    if !atual {
                        linha = linha.push(button(text("Abrir").size(12)).padding([3, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Versoes(Vs::AbrirVersao(i))));
                    }
                    c = c.push(container(linha).padding([8, 10]).width(Length::FILL).style(|_| Style {
                        background: Some(Background::Color(tema::COR_PAINEL())),
                        border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [6.0; 4] },
                        ..Default::default()
                    }));
                }
                c.push(text("Uma versão antiga mostra o documento como era antes das mudanças seguintes — útil para ver o que mudou depois de uma assinatura.").size(11.5).color(muted))
            }
        };
        scrollable(corpo.spacing(8).padding([0, 4])).height(Length::FILL).into()
    }
}
