//! Cópias que saem do documento aberto sem mudá-lo (linearizada para a web, apresentação…):
//! escolhe o destino, gera fora da janela e grava.

use std::path::PathBuf;


use pelicano::*;
use crate::{App, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copia { Web }

impl Copia {
    fn sufixo(self) -> (&'static str, &'static str) {
        match self { Copia::Web => (" (web)", "pdf") }
    }
}

#[derive(Debug, Clone)]
pub enum Cp {
    Pedir(Copia),
    /// Id do documento (`Aberto::id`) que pediu, a cópia e o destino.
    Destino(u64, Copia, Option<PathBuf>),
    Feita(Result<(String, PathBuf), String>),
}

impl App {
    pub fn update_copias(&mut self, m: Cp) -> Task<Msg> {
        match m {
            Cp::Pedir(c) => {
                let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
                let base = std::path::Path::new(&nome).file_stem().map_or("documento".into(), |n| n.to_string_lossy().into_owned());
                let (sufixo, ext) = c.sufixo();
                let sugestao = format!("{base}{sufixo}.{ext}");
                let Some(doc) = self.id_aberto() else { return Task::none() };
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(sugestao).save_file().await
                        .map(|h| h.path().to_path_buf())
                }, move |d| Msg::Copia(Cp::Destino(doc, c, d)));
            }
            Cp::Destino(_, _, None) => {}
            Cp::Destino(doc, _, Some(_)) if Some(doc) != self.id_aberto() => self.status = "O documento mudou: a cópia não foi gerada.".into(),
            Cp::Destino(_, c, Some(destino)) => if let Some(a) = &self.aberto {
                let pdf = a.pdf.clone();
                self.status = format!("Gerando {}…", destino.display());
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let bytes = match c { Copia::Web => pdf.linearizado()? };
                        featherpdf_io::gravar_pdf(&destino, &bytes)?;
                        Ok((match c { Copia::Web => format!("Cópia para a web (abre a página 1 antes de baixar tudo) salva em {}", destino.display()) }, destino))
                    }).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Copia(Cp::Feita(r)));
            },
            Cp::Feita(Ok((t, destino))) => self.avisar_salvo(t, destino),
            Cp::Feita(Err(e)) => self.status = e,
        }
        Task::none()
    }
}
