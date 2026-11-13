//! Painel Scripts: JavaScript no sandbox das macros da suíte (`feather-macros`, sem arquivo,
//! rede nem DOM) sobre o PDF aberto. A API é `pdf.*`; o script roda numa cópia e só vale se
//! terminar sem erro (um Ctrl+Z desfaz tudo o que ele fez). Roda num processo à parte (o próprio
//! executável com `--rodar-script`): laço sem fim é parado no prazo, e memória demais derruba só
//! esse processo, não o app com as abas não salvas.

use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

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

/// O argumento que faz o executável só rodar um script (lido da entrada) e sair.
pub const ARG_SCRIPT: &str = "--rodar-script";
/// Mais que isso é laço sem fim (o limite de voltas do Boa é por chamada, não pela execução).
const PRAZO: Duration = Duration::from_secs(120);

/// Resposta do processo do script: `E` + erro, ou `O` + linhas do `log` + o PDF (vazio se não mudou).
fn codificar(r: &Result<(Option<Vec<u8>>, Vec<String>), String>) -> Vec<u8> {
    let mut v = Vec::new();
    match r {
        Err(e) => { v.push(b'E'); v.extend_from_slice(e.as_bytes()); }
        Ok((pdf, log)) => {
            v.push(b'O');
            v.extend_from_slice(&(log.len() as u32).to_le_bytes());
            for l in log { v.extend_from_slice(&(l.len() as u32).to_le_bytes()); v.extend_from_slice(l.as_bytes()); }
            if let Some(b) = pdf { v.extend_from_slice(b); }
        }
    }
    v
}

fn decodificar(v: &[u8]) -> Result<(Option<Vec<u8>>, Vec<String>), String> {
    let quebrado = || "Resposta do script incompleta.".to_string();
    let (tipo, resto) = v.split_first().ok_or_else(quebrado)?;
    if *tipo == b'E' { return Err(String::from_utf8_lossy(resto).into_owned()); }
    let mut i = 0;
    let u32_em = |i: &mut usize| -> Result<usize, String> {
        let b = resto.get(*i..*i + 4).ok_or_else(quebrado)?;
        *i += 4;
        Ok(u32::from_le_bytes(b.try_into().unwrap()) as usize)
    };
    let n = u32_em(&mut i)?;
    let mut log = Vec::with_capacity(n.min(10_000));
    for _ in 0..n {
        let t = u32_em(&mut i)?;
        log.push(String::from_utf8_lossy(resto.get(i..i + t).ok_or_else(quebrado)?).into_owned());
        i += t;
    }
    let pdf = (i < resto.len()).then(|| resto[i..].to_vec());
    Ok((pdf, log))
}

/// O processo do script: lê (tamanho do código, código, PDF) da entrada, roda e responde na saída.
pub fn processo_do_script() -> i32 {
    // Sem memória, o sistema derruba este processo antes do app (Linux; nos outros, sem efeito).
    let _ = std::fs::write("/proc/self/oom_score_adj", "1000");
    let mut entrada = Vec::new();
    if std::io::stdin().read_to_end(&mut entrada).is_err() || entrada.len() < 8 { return 2; }
    let n = u64::from_le_bytes(entrada[..8].try_into().unwrap()) as usize;
    let Some(codigo) = entrada.get(8..8 + n).map(|c| String::from_utf8_lossy(c).into_owned()) else { return 2 };
    let r = featherpdf_core::DocumentoPdf::de_bytes(&entrada[8 + n..], None)
        .and_then(|pdf| rodar(pdf, &codigo))
        .and_then(|(novo, log)| Ok((novo.map(|p| p.serializar()).transpose()?, log)));
    let mut saida = std::io::stdout().lock();
    if saida.write_all(&codificar(&r)).and_then(|_| saida.flush()).is_err() { return 2; }
    0
}

/// [`rodar`] no processo à parte, com prazo. Nos testes roda aqui mesmo (o executável é o dos testes).
fn rodar_isolado(pdf: &DocumentoPdf, codigo: &str) -> Result<(Option<DocumentoPdf>, Vec<String>), String> {
    if cfg!(test) { return rodar(pdf.clone(), codigo); }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut filho = std::process::Command::new(exe).arg(ARG_SCRIPT)
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("Não consegui rodar o script: {e}"))?;
    let mut pacote = (codigo.len() as u64).to_le_bytes().to_vec();
    pacote.extend_from_slice(codigo.as_bytes());
    pacote.extend_from_slice(&pdf.serializar()?);
    // Escrever e ler em threads: com tudo numa só, entrada e saída cheias se esperariam para sempre.
    let mut entrada = filho.stdin.take().ok_or("sem entrada")?;
    let escritor = std::thread::spawn(move || entrada.write_all(&pacote));
    let mut saida = filho.stdout.take().ok_or("sem saída")?;
    let leitor = std::thread::spawn(move || { let mut v = Vec::new(); saida.read_to_end(&mut v).map(|_| v) });
    let inicio = Instant::now();
    let estado = loop {
        match filho.try_wait() {
            Ok(Some(e)) => break e,
            Ok(None) if inicio.elapsed() > PRAZO => {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err(format!("O script passou de {} s e foi parado (laço sem fim?). O documento não mudou.", PRAZO.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let _ = escritor.join();
    let v = leitor.join().map_err(|_| "Falha ao ler o script.".to_string())?.map_err(|e| e.to_string())?;
    if !estado.success() || v.is_empty() {
        return Err("O script parou sem terminar (memória demais?). O documento não mudou.".into());
    }
    let (bytes, log) = decodificar(&v)?;
    let novo = bytes.map(|b| DocumentoPdf::de_bytes(&b, None)).transpose()?;
    Ok((novo, log))
}

impl App {
    pub fn update_scripts(&mut self, m: Sc) -> Task<Msg> {
        match m {
            Sc::Abrir => self.painel = if self.painel == Some(Painel::Scripts) { None } else { Some(Painel::Scripts) },
            Sc::Editar => {}
            Sc::Rodar => {
                let Some(a) = &self.aberto else { return Task::none() };
                if self.scripts.rodando { return Task::none(); }
                // Em outro processo: um laço sem fim é parado no prazo, e memória demais derruba só ele.
                let (pdf, codigo, rodada) = (a.pdf.clone(), self.scripts.codigo.text(), self.scripts.rodada);
                self.scripts.rodando = true;
                self.status = "Rodando o script…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || rodar_isolado(&pdf, &codigo)).await.unwrap_or_else(|e| Err(e.to_string()))
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
                        // Só o conteúdo: o arquivo de origem e a senha continuam os do documento aberto.
                        return self.editar(move |pdf| { pdf.trocar_conteudo(novo); Ok("Script rodou (Ctrl+Z desfaz o que ele fez).".into()) });
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
    fn resposta_do_processo_vai_e_volta() {
        use super::{codificar, decodificar};
        let ok: Result<(Option<Vec<u8>>, Vec<String>), String> = Ok((Some(b"%PDF-1.7 x".to_vec()), vec!["um".into(), "dois é".into()]));
        assert_eq!(decodificar(&codificar(&ok)), ok);
        let so_log: Result<(Option<Vec<u8>>, Vec<String>), String> = Ok((None, vec![]));
        assert_eq!(decodificar(&codificar(&so_log)), so_log);
        assert_eq!(decodificar(&codificar(&Err("falhou".into()))), Err("falhou".into()));
        assert!(decodificar(b"O\x05\x00").is_err(), "resposta cortada não derruba");
    }

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
