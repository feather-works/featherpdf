//! Formulários → Dados: preencher os campos a partir de um CSV ou JSON e exportar os
//! valores em JSON ou XFDF (o formato de dados de formulário do Acrobat).

use std::path::PathBuf;


use pelicano::*;
use crate::{App, Msg};

#[derive(Debug, Clone)]
pub enum Fd {
    Importar,
    ImportarDe(Option<PathBuf>),
    Exportar,
    ExportarPara(Option<PathBuf>),
}

impl App {
    pub fn update_form_dados(&mut self, m: Fd) -> Task<Msg> {
        match m {
            Fd::Importar => return Task::perform(async {
                rfd::AsyncFileDialog::new().add_filter("CSV ou JSON", &["csv", "json", "CSV", "JSON"]).pick_file().await.map(|h| h.path().to_path_buf())
            }, |c| Msg::FormDados(Fd::ImportarDe(c))),
            Fd::ExportarPara(None) | Fd::ImportarDe(None) => {}
            Fd::ImportarDe(Some(origem)) => {
                let lido = std::fs::read(&origem).map_err(|e| format!("Não consegui ler {}: {e}", origem.display()))
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .and_then(|t| if origem.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
                        featherpdf_core::ler_json_formulario(&t)
                    } else { featherpdf_core::ler_csv_formulario(&t) });
                let pares = match lido { Ok(p) => p, Err(e) => { self.status = e; return Task::none(); } };
                return self.editar(move |pdf| {
                    let (n, faltam) = pdf.importar_dados(&pares)?;
                    if n == 0 { return Err("Nenhum nome do arquivo bate com um campo deste PDF.".into()); }
                    Ok(if faltam.is_empty() { format!("{n} campo(s) preenchido(s). Ctrl+Z desfaz.") }
                        else { format!("{n} campo(s) preenchido(s); sem campo com esse nome: {}.", faltam.join(", ")) })
                });
            }
            Fd::Exportar => {
                let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
                let base = nome.strip_suffix(".pdf").unwrap_or(&nome).to_string();
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().add_filter("JSON", &["json"]).add_filter("XFDF (Acrobat)", &["xfdf"])
                        .set_file_name(format!("{base} - dados.json")).save_file().await.map(|h| h.path().to_path_buf())
                }, |d| Msg::FormDados(Fd::ExportarPara(d)));
            }
            Fd::ExportarPara(Some(destino)) => if let Some(a) = &self.aberto {
                let xfdf = destino.extension().is_some_and(|e| e.eq_ignore_ascii_case("xfdf"));
                let conteudo = if xfdf { a.pdf.formulario_xfdf(&self.nome_documento().unwrap_or_default()) } else { a.pdf.formulario_json() };
                self.status = match featherpdf_io::gravar_atomico(&destino, conteudo.as_ref()) {
                    Ok(()) => format!("Dados do formulário salvos em {}", destino.display()),
                    Err(e) => format!("Não consegui gravar os dados: {e}"),
                };
            },
        }
        Task::none()
    }
}
