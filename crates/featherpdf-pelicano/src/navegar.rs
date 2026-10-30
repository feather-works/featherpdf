//! Painéis Marcadores (o sumário do PDF, clicar leva à página) e Anexos (salvar o arquivo anexado).

use std::path::PathBuf;

use feather_ui::{icones, tema};
use featherpdf_core::{numero, Anexo, Marcador};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

#[derive(Debug, Clone)]
pub enum Nv {
    Salvar(usize),
    /// Id do documento (`Aberto::id`), anexo e destino.
    Destino(u64, usize, Option<PathBuf>),
}

/// O que os dois painéis mostram, lido quando o painel abre.
#[derive(Debug, Default)]
pub struct Navegacao {
    pub marcadores: Vec<Marcador>,
    pub anexos: Vec<Anexo>,
}

impl App {
    pub fn ler_navegacao(&mut self) {
        self.navegacao = self.aberto.as_ref()
            .map(|a| Navegacao { marcadores: a.pdf.marcadores(), anexos: a.pdf.anexos() })
            .unwrap_or_default();
    }

    pub fn update_nav(&mut self, m: Nv) -> Task<Msg> {
        match m {
            Nv::Salvar(i) => {
                let nome = self.navegacao.anexos.get(i).map(|a| a.nome.clone()).unwrap_or_default();
                let Some(doc) = self.id_aberto() else { return Task::none() };
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().set_file_name(nome).save_file().await.map(|h| h.path().to_path_buf())
                }, move |d| Msg::Nav(Nv::Destino(doc, i, d)));
            }
            Nv::Destino(_, _, None) => {}
            Nv::Destino(doc, _, Some(_)) if Some(doc) != self.id_aberto() => self.status = "O documento mudou: o anexo não foi salvo.".into(),
            Nv::Destino(_, i, Some(destino)) => if let Some(a) = &self.aberto {
                self.status = match a.pdf.anexo(i).and_then(|(_, b)| featherpdf_io::gravar_atomico(&destino, &b)) {
                    Ok(()) => format!("Anexo salvo em {}", destino.display()),
                    Err(e) => format!("Não consegui salvar o anexo: {e}"),
                };
            },
        }
        Task::none()
    }

    pub fn painel_marcadores(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        if self.navegacao.marcadores.is_empty() {
            return text("Este PDF não tem marcadores (sumário).").size(13).color(muted).into();
        }
        // Só um destaque: o último marcador que começa até a página atual (a seção em que se está).
        let atual_i = self.navegacao.marcadores.iter().enumerate().filter(|(_, m)| m.pagina <= self.pagina)
            .max_by_key(|(i, m)| (m.pagina, *i)).map(|(i, _)| i);
        let mut lista = column![].spacing(1);
        for (i, m) in self.navegacao.marcadores.iter().enumerate() {
            let atual = Some(i) == atual_i;
            lista = lista.push(button(row![
                Space::with_width(((m.nivel - 1).min(6) * 14) as f32),
                text(m.titulo.clone()).size(13).width(Length::FILL),
                text(m.pagina.to_string()).size(12).color(muted),
            ].spacing(6).align_y(Align::Center)).width(Length::FILL).padding([4, 6])
                .style(tema::estilo_botao_pequeno(atual)).on_press(Msg::IrPagina(m.pagina)));
        }
        scrollable(lista).height(Length::FILL).into()
    }

    pub fn painel_anexos(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        if self.navegacao.anexos.is_empty() {
            return text("Este PDF não tem arquivos anexados.").size(13).color(muted).into();
        }
        let mut lista = column![].spacing(8);
        for (i, a) in self.navegacao.anexos.iter().enumerate() {
            let tamanho = if a.tamanho >= 1024 * 1024 { format!("{} MB", numero(a.tamanho as f64 / 1048576.0, 1)) }
                else { format!("{} KB", numero((a.tamanho as f64 / 1024.0).ceil(), 0)) };
            lista = lista.push(row![
                icones::icone("file-export", 16.),
                column![text(a.nome.clone()).size(13), text(tamanho).size(11.5).color(muted)].spacing(2).width(Length::FILL),
                button(text("Salvar…").size(12)).padding([4, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Nav(Nv::Salvar(i))),
            ].spacing(8).align_y(Align::Center));
        }
        scrollable(lista).height(Length::FILL).into()
    }
}
