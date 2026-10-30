//! Juntar arquivos num PDF novo (PDFs e imagens PNG/JPG, cada imagem vira uma página),
//! criar PDF a partir de imagens e intercalar frente e verso de dois PDFs escaneados.

use std::path::{Path, PathBuf};

use featherpdf_core::{AjusteImagem, DocumentoPdf};
use featherpdf_io::Visualizador;

use pelicano::*;
use crate::{App, Msg};

pub const EXTENSOES_IMAGEM: [&str; 6] = ["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"];

#[derive(Debug, Clone)]
pub enum Jt {
    /// Escolher arquivos para um PDF novo (`true`: só imagens).
    Escolher(bool),
    Escolhidos(Option<Vec<PathBuf>>),
    /// Frente e verso em dois arquivos: escolhe as frentes, depois os versos.
    FrenteVerso { invertidos: bool },
    Frentes(bool, Option<PathBuf>),
    Versos(bool, PathBuf, Option<PathBuf>),
    Pronto(Result<(DocumentoPdf, Visualizador), String>),
}

pub fn e_imagem(caminho: &Path) -> bool {
    caminho.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSOES_IMAGEM.contains(&e))
}

/// Abre um PDF, ou faz um PDF de uma página com a imagem (bloqueante).
pub fn documento_de_arquivo(caminho: &Path) -> Result<DocumentoPdf, String> {
    if !e_imagem(caminho) { return featherpdf_io::abrir(caminho); }
    let bytes = featherpdf_io::ler_bytes(caminho)?;
    let (imagem, l, a) = crate::inserir::ler_imagem(bytes)?;
    let mut pdf = featherpdf_io::criar_a4()?;
    pdf.so_imagens(vec![(std::sync::Arc::unwrap_or_clone(imagem), l, a)], AjusteImagem::A4)?;
    Ok(pdf)
}

/// Um PDF novo com todos os arquivos, na ordem dada (bloqueante).
pub fn juntar_arquivos(caminhos: &[PathBuf]) -> Result<DocumentoPdf, String> {
    let (primeiro, resto) = caminhos.split_first().ok_or("Nenhum arquivo escolhido.")?;
    let ler = |c: &PathBuf| documento_de_arquivo(c).map_err(|e| format!("{}: {e}", c.display()));
    let mut pdf = ler(primeiro)?;
    for c in resto {
        let fim = pdf.total_paginas() + 1;
        pdf.inserir_pdf(ler(c)?, fim)?;
    }
    Ok(pdf)
}

/// O resultado nasce sem origem (e modificado): Ctrl+S pede um nome em vez de gravar por cima do primeiro arquivo.
fn com_vista(pdf: DocumentoPdf) -> Result<(DocumentoPdf, Visualizador), String> {
    let bytes = pdf.serializar()?;
    Ok((DocumentoPdf::de_bytes(&bytes, None)?, Visualizador::novo(bytes)?))
}

impl App {
    pub fn update_juntar(&mut self, m: Jt) -> Task<Msg> {
        match m {
            Jt::Escolher(so_imagens) => return Task::perform(async move {
                let d = rfd::AsyncFileDialog::new();
                let d = if so_imagens { d.add_filter("Imagens", &EXTENSOES_IMAGEM) }
                    else { d.add_filter("PDF e imagens", &["pdf", "PDF", "png", "jpg", "jpeg", "PNG", "JPG", "JPEG"]) };
                d.pick_files().await.map(|v| v.into_iter().map(|h| h.path().to_path_buf()).collect())
            }, |v| Msg::Juntar(Jt::Escolhidos(v))),
            Jt::Escolhidos(None) | Jt::Frentes(_, None) | Jt::Versos(_, _, None) => {}
            Jt::Escolhidos(Some(mut caminhos)) => {
                // O seletor devolve em qualquer ordem: em ordem de nome, como foto_01, foto_02…
                caminhos.sort();
                self.status = format!("Juntando {} arquivo(s)…", caminhos.len());
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || juntar_arquivos(&caminhos).and_then(com_vista)).await
                        .unwrap_or_else(|e| Err(format!("Falha ao juntar: {e}")))
                }, |r| Msg::Juntar(Jt::Pronto(r)));
            }
            Jt::FrenteVerso { invertidos } => {
                self.status = "Escolha o PDF das frentes.".into();
                return Task::perform(async {
                    rfd::AsyncFileDialog::new().set_title("PDF com as frentes").add_filter("PDF", &["pdf", "PDF"]).pick_file().await.map(|h| h.path().to_path_buf())
                }, move |c| Msg::Juntar(Jt::Frentes(invertidos, c)));
            }
            Jt::Frentes(invertidos, Some(frentes)) => {
                self.status = "Agora o PDF dos versos.".into();
                return Task::perform(async {
                    rfd::AsyncFileDialog::new().set_title("PDF com os versos").add_filter("PDF", &["pdf", "PDF"]).pick_file().await.map(|h| h.path().to_path_buf())
                }, move |c| Msg::Juntar(Jt::Versos(invertidos, frentes.clone(), c)));
            }
            Jt::Versos(invertidos, frentes, Some(versos)) => {
                self.status = "Intercalando frentes e versos…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let mut pdf = featherpdf_io::abrir(&frentes)?;
                        let outro = featherpdf_io::abrir(&versos)?;
                        if outro.total_paginas() != pdf.total_paginas() {
                            return Err(format!("As frentes têm {} página(s) e os versos {}: precisam ser iguais.", pdf.total_paginas(), outro.total_paginas()));
                        }
                        let fim = pdf.total_paginas() + 1;
                        pdf.inserir_pdf(outro, fim)?;
                        pdf.intercalar_frente_verso(invertidos)?;
                        com_vista(pdf)
                    }).await.unwrap_or_else(|e| Err(format!("Falha ao intercalar: {e}")))
                }, |r| Msg::Juntar(Jt::Pronto(r)));
            }
            Jt::Pronto(Ok((pdf, vis))) => {
                let tarefa = self.entrar(pdf, vis);
                self.status = format!("PDF novo com {} página(s). Salve para escolher o nome.", self.total_paginas());
                return tarefa;
            }
            Jt::Pronto(Err(e)) => self.status = e,
        }
        Task::none()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn juntado_nasce_sem_origem() {
        let bytes = featherpdf_io::criar_a4().unwrap().serializar().unwrap();
        let pdf = DocumentoPdf::de_bytes(&bytes, Some(PathBuf::from("/tmp/primeiro.pdf"))).unwrap();
        let (novo, _) = com_vista(pdf).unwrap();
        assert!(novo.origem().is_none());
        assert!(novo.modificado());
    }
}
