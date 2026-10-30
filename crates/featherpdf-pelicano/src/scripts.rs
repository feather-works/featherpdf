//! Painel Scripts: JavaScript no sandbox das macros da suíte (`feather-macros`, sem arquivo,
//! rede nem DOM) sobre o PDF aberto. A API é `pdf.*`; o script roda numa cópia e só vale se
//! terminar sem erro (um Ctrl+Z desfaz tudo o que ele fez).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use feather_macros::HospedeiroMacros;
use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::DocumentoPdf;
use serde_json::{json, Value as Json};

use pelicano::*;
use pelicano::column;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone)]
pub enum Sc { Abrir, Editar, Rodar, Pronto(u64, Result<(Option<DocumentoPdf>, Vec<String>), String>) }

pub struct Scripts {
    pub codigo: Content,
    pub saida: Option<Result<Vec<String>, String>>,
    /// Sobe a cada mudança no documento: o resultado de um script de antes dela é descartado.
    pub rodada: u64,
    pub rodando: bool,
}

const EXEMPLO: &str = "// Numera o rodapé de cada página e lista os campos do formulário.\nfor (let p = 1; p <= pdf.pages(); p++) {\n  pdf.addText(p, 500, 30, 'Pág. ' + p, 9);\n}\nlog(pdf.fields());\n";

impl Default for Scripts {
    fn default() -> Self { Self { codigo: Content::with_text(EXEMPLO), saida: None, rodada: 0, rodando: false } }
}

/// O que um script pode fazer no PDF: só estas operações, todas sobre a cópia.
const API: &str = r#"
const pdf = Object.freeze({
  pages: () => __ft('pages', null),
  text: (page) => __ft('text', [page]),
  addText: (page, x, y, text, size) => __ft('addText', [page, x, y, String(text), size || 11]),
  rotate: (pages, degrees) => __ft('rotate', [[].concat(pages), degrees]),
  deletePages: (pages) => __ft('deletePages', [[].concat(pages)]),
  fields: () => __ft('fields', null),
  setField: (name, value) => __ft('setField', [name, String(value)]),
  find: (term) => __ft('find', [String(term)]),
});
"#;

/// A cópia do documento e se alguma operação a mudou.
struct Hospede(RefCell<DocumentoPdf>, Cell<bool>);

impl HospedeiroMacros for Hospede {
    fn chamar(&self, op: &str, d: &Json) -> Result<Json, String> {
        let mut pdf = self.0.borrow_mut();
        if matches!(op, "addText" | "rotate" | "deletePages" | "setField") { self.1.set(true); }
        let n = |i: usize| d.get(i).and_then(Json::as_f64).ok_or_else(|| format!("{op}: argumento {} precisa ser número", i + 1));
        let paginas = |i: usize| -> Vec<u32> { d.get(i).and_then(Json::as_array).map(|v| v.iter().filter_map(Json::as_u64).map(|p| p as u32).collect()).unwrap_or_default() };
        Ok(match op {
            "pages" => json!(pdf.total_paginas()),
            "text" => json!(pdf.texto_pagina(n(0)? as u32)?),
            "addText" => {
                let t = d.get(3).and_then(Json::as_str).unwrap_or_default();
                pdf.inserir_texto(n(0)? as u32, n(1)? as f32, n(2)? as f32, t, n(4)? as f32, false, [0.0; 3])?;
                Json::Null
            }
            "rotate" => { pdf.girar_paginas(&paginas(0), n(1)? as i64)?; Json::Null }
            "deletePages" => { pdf.excluir_paginas(&paginas(0))?; Json::Null }
            "fields" => json!(pdf.campos().into_iter().map(|c| json!({"name": c.nome, "value": c.valor, "page": c.pagina})).collect::<Vec<_>>()),
            "setField" => {
                let (nome, valor) = (d.get(0).and_then(Json::as_str).unwrap_or_default(), d.get(1).and_then(Json::as_str).unwrap_or_default());
                let campo = pdf.campos().into_iter().find(|c| c.nome == nome).ok_or_else(|| format!("Campo “{nome}” não existe"))?;
                pdf.preencher(&campo, valor)?;
                Json::Null
            }
            "find" => {
                let termo = d.get(0).and_then(Json::as_str).unwrap_or_default().to_lowercase();
                json!((1..=pdf.total_paginas()).filter(|&p| pdf.texto_pagina(p).is_ok_and(|t| t.to_lowercase().contains(&termo))).collect::<Vec<_>>())
            }
            _ => return Err(format!("pdf não tem a operação “{op}”")),
        })
    }
}

/// Roda o script numa cópia; devolve o documento mudado (`None` se nada mudou) e o que ele escreveu com `log`.
pub fn rodar(pdf: DocumentoPdf, codigo: &str) -> Result<(Option<DocumentoPdf>, Vec<String>), String> {
    let host = Rc::new(Hospede(RefCell::new(pdf), Cell::new(false)));
    let r = feather_macros::executar_com_origem(host.clone(), API, codigo, Some("script"));
    if let Some(e) = r.erro { return Err(e); }
    let doc = host.1.get().then(|| host.0.borrow().clone());
    Ok((doc, r.saida))
}

impl App {
    pub fn update_scripts(&mut self, m: Sc) -> Task<Msg> {
        match m {
            Sc::Abrir => self.painel = if self.painel == Some(Painel::Scripts) { None } else { Some(Painel::Scripts) },
            Sc::Editar => {}
            Sc::Rodar => {
                let Some(a) = &self.aberto else { return Task::none() };
                if self.scripts.rodando { return Task::none(); }
                // Fora da tela: um laço sem fim trava só esta tarefa, não a janela (nem as abas não salvas).
                let (pdf, codigo, rodada) = (a.pdf.clone(), self.scripts.codigo.text(), self.scripts.rodada);
                self.scripts.rodando = true;
                self.status = "Rodando o script…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || rodar(pdf, &codigo)).await.unwrap_or_else(|e| Err(e.to_string()))
                }, move |r| Msg::Scripts(Sc::Pronto(rodada, r)));
            }
            Sc::Pronto(rodada, r) => {
                self.scripts.rodando = false;
                if rodada != self.scripts.rodada {
                    self.status = "O documento mudou enquanto o script rodava: nada foi aplicado. Rode de novo.".into();
                    return Task::none();
                }
                match r {
                    Err(e) => { self.status = e.clone(); self.scripts.saida = Some(Err(e)); }
                    Ok((novo, log)) => {
                        self.scripts.saida = Some(Ok(log));
                        let Some(novo) = novo else { self.status = "Script rodou sem mudar o documento.".into(); return Task::none() };
                        return self.editar(move |pdf| { *pdf = novo; Ok("Script rodou (Ctrl+Z desfaz o que ele fez).".into()) });
                    }
                }
            }
        }
        Task::none()
    }

    pub fn painel_scripts(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let mut corpo = column![
            text_editor(&self.scripts.codigo).on_change(Msg::Scripts(Sc::Editar)).font(Font::MONO).size(12.5),
            button(text(if self.scripts.rodando { "Rodando…" } else { "Rodar" }).size(13)).width(Length::FILL).padding([6, 10]).style(tema::estilo_botao_pequeno(true))
                .on_press_maybe((!self.scripts.rodando).then_some(Msg::Scripts(Sc::Rodar))),
            text("pdf.pages(), pdf.text(p), pdf.find(termo), pdf.addText(p, x, y, texto, tamanho), pdf.rotate(páginas, graus), pdf.deletePages(páginas), pdf.fields(), pdf.setField(nome, valor), log(…). Sem acesso a arquivo nem rede.").size(11.5).color(muted),
        ].spacing(10);
        match &self.scripts.saida {
            Some(Ok(l)) if !l.is_empty() => corpo = corpo.push(column(l.iter().map(|x| text(x.clone()).size(12).font(Font::MONO).into())).spacing(2)),
            Some(Err(e)) => corpo = corpo.push(dlg::aviso("alert-triangle", e.clone())),
            _ => {}
        }
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn script_escreve_le_e_erro_nao_mexe() {
        let pdf = featherpdf_io::criar_a4().unwrap();
        let (novo, log) = super::rodar(pdf.clone(), "pdf.addText(1, 72, 700, 'Oi do script', 12); log(pdf.pages(), pdf.find('script'));").unwrap();
        assert!(novo.unwrap().texto_pagina(1).unwrap().contains("Oi do script"));
        assert_eq!(log, vec!["1 [1]"]);
        assert!(super::rodar(pdf.clone(), "log(pdf.pages())").unwrap().0.is_none());
        assert!(super::rodar(pdf.clone(), "pdf.apagarTudo()").is_err());
        assert!(super::rodar(pdf, "pdf.deletePages([9])").is_err());
    }
}
