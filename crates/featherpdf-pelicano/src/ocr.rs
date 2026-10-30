//! Janela "Reconhecer texto (OCR)": lê as páginas desenhadas com o `featherpdf-ocr`, mostra
//! a imagem e o texto lido lado a lado para conferir e corrigir, e só então grava a camada
//! de texto invisível (selecionar, copiar e buscar). Uma página por tarefa, fora da thread
//! da interface; o Aplicar é um Ctrl+Z só.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use feather_ui::{cromo, icones, tema};
use featherpdf_io::Visualizador;
use featherpdf_ocr::{Imagem, Ocr, Orientacao};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::ocr_modelos::{self as modelos, LINGUAS, ORIENTACAO};
use crate::{App, Auxiliar, Msg};

/// Lado maior da página desenhada para o OCR, em pixels.
pub(crate) const LADO: f32 = 2000.0;
/// Página com mais letras que isso já tem texto e fica de fora em "só as sem texto".
pub(crate) const MINIMO_TEXTO: usize = 20;
/// Abaixo disso a palavra vai para "Conferir".
const CERTEZA: f32 = 0.80;
/// Tamanho de cada página na tela (as duas lado a lado).
const PAGINA_TELA: (f32, f32) = (380.0, 520.0);

/// Os modelos carregam uma vez só (~10 MB embutidos, ~0,5 s); as outras escritas reaproveitam a detecção.
static MOTOR: OnceLock<Result<Arc<Ocr>, String>> = OnceLock::new();
static OUTRAS: Mutex<Option<HashMap<usize, Arc<Ocr>>>> = Mutex::new(None);
static ORIENTA: Mutex<Option<Arc<Orientacao>>> = Mutex::new(None);

pub(crate) fn motor(lingua: usize) -> Result<Arc<Ocr>, String> {
    let base = MOTOR.get_or_init(|| Ocr::embutido().map(Arc::new)).clone()?;
    let Some(m) = &LINGUAS[lingua] else { return Ok(base) };
    let mut outras = OUTRAS.lock().map_err(|e| e.to_string())?;
    if let Some(o) = outras.get_or_insert_with(HashMap::new).get(&lingua) { return Ok(o.clone()); }
    let ler = |n: &str| m.caminho(n).ok_or("Sem pasta de configuração.".to_string())
        .and_then(|p| std::fs::read(p).map_err(|e| format!("Modelo de {} não encontrado: {e}", m.nome)));
    let dicionario = String::from_utf8(ler("dict.txt")?).map_err(|e| e.to_string())?;
    let o = Arc::new(base.com_reconhecimento(&ler("rec.onnx")?, &dicionario)?);
    outras.get_or_insert_with(HashMap::new).insert(lingua, o.clone());
    Ok(o)
}

fn orientacao() -> Result<Arc<Orientacao>, String> {
    let mut o = ORIENTA.lock().map_err(|e| e.to_string())?;
    if let Some(x) = &*o { return Ok(x.clone()); }
    let bytes = ORIENTACAO.caminho("modelo.onnx").and_then(|p| std::fs::read(p).ok()).ok_or("Modelo de orientação não baixado.")?;
    let x = Arc::new(Orientacao::de_bytes(&bytes)?);
    *o = Some(x.clone());
    Ok(x)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Escopo { Atual, Todas, #[default] SemTexto }

/// Item da lista "Páginas" (o rótulo leva a contagem).
#[derive(Debug, Clone, PartialEq)]
pub struct OpcaoPaginas { escopo: Escopo, rotulo: String }

impl std::fmt::Display for OpcaoPaginas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.rotulo) }
}

/// Item da lista "Idioma".
#[derive(Debug, Clone, PartialEq)]
pub struct OpcaoLingua(usize, String);

impl std::fmt::Display for OpcaoLingua {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.1) }
}

#[derive(Debug, Clone)]
pub struct PalavraLida {
    pub texto: String,
    /// O que o OCR leu, quando a palavra foi corrigida (sozinha ou à mão).
    pub original: Option<String>,
    /// Em fração da página, y para baixo.
    pub caixa: [f32; 4],
    pub confianca: f32,
    /// Corrigida à mão: sai da lista "Conferir".
    pub revisada: bool,
}

impl PalavraLida {
    fn conferir(&self) -> bool { !self.revisada && (self.confianca < CERTEZA || self.original.is_some()) }
}

#[derive(Debug, Clone)]
pub struct LinhaLida { pub(crate) caixa: [f32; 4], pub(crate) palavras: Vec<PalavraLida> }

#[derive(Debug, Clone)]
pub struct PaginaLida {
    pagina: u32,
    /// Giro (horário) que endireita a página; o texto lido já está nessa orientação.
    giro: u32,
    linhas: Vec<LinhaLida>,
}

type Lidas = Result<(Vec<LinhaLida>, u32), String>;

#[derive(Default)]
pub struct EstadoOcr {
    pub escopo: Escopo,
    /// Páginas a ler, em ordem; `feitas` delas já voltaram.
    fila: Vec<u32>,
    feitas: usize,
    desde: Option<Instant>,
    pausado: bool,
    /// Muda a cada execução e ao trocar de documento: resultado de outra rodada é jogado fora.
    rodada: u64,
    lidas: Vec<PaginaLida>,
    /// Página lida mostrada (índice em `lidas`).
    ver: usize,
    previa: Option<(u32, Image)>,
    editando: Option<(usize, usize)>,
    caixas: bool,
    /// Páginas sem texto do documento (contadas ao abrir a janela).
    sem_texto: Vec<u32>,
    aplicado: bool,
    pub aviso: Option<Result<String, String>>,
    /// Índice em `LINGUAS` (0 = latino, embutido).
    pub(crate) lingua: usize,
    endireitar: bool,
    /// Nome do modelo sendo baixado.
    baixando: Option<&'static str>,
    /// Resultado: trocar a imagem pelo texto visível (em vez da camada invisível).
    trocar: bool,
    /// Resultado: o papel cinza do escaneado vira branco (só com o texto invisível; trocar já tira a imagem).
    limpar: bool,
    /// "Rótulo: valor" vira campo de formulário.
    formulario: bool,
    /// Os outros PDFs da pasta do arquivo aberto, e se estão sendo lidos.
    vizinhos: Vec<std::path::PathBuf>,
    lendo_pasta: bool,
}

#[derive(Debug, Clone)]
pub enum Oc {
    Abrir,
    Escopo(Escopo),
    Reconhecer,
    Pausar(bool),
    Lida(u64, u32, Lidas),
    Previa(u32, Option<Quadro>),
    Ver(usize),
    Palavra(usize, usize, usize),
    Editar(String),
    FimEdicao,
    Caixas(bool),
    Lingua(usize),
    Endireitar(bool),
    Baixado(Result<(), String>),
    Trocar(bool),
    Limpar(bool),
    Formulario(bool),
    LerPasta,
    PastaLida(Result<String, String>),
    Aplicar,
    /// Só redesenha o tempo decorrido.
    Tique,
}

/// Os pares "Rótulo: valor" de uma linha, com a caixa do valor (em fração da página). Valor
/// vazio ("Nome: ____") ganha o espaço até o próximo rótulo, para preencher depois.
pub fn campos_da_linha(linha: &LinhaLida) -> Vec<(String, String, [f32; 4])> {
    let mut ordem: Vec<&PalavraLida> = linha.palavras.iter().collect();
    ordem.sort_by(|a, b| a.caixa[0].total_cmp(&b.caixa[0]));
    let altura = linha.caixa[3] - linha.caixa[1];
    // Rótulo: as palavras até a que termina em ":"; o valor segue até um vão grande ou o próximo rótulo.
    let mut saida: Vec<(String, String, [f32; 4])> = Vec::new();
    let mut rotulo: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < ordem.len() {
        let p = ordem[i];
        let t = p.texto.trim();
        if !(t.ends_with(':') && t.len() > 1) { rotulo.push(t); i += 1; continue; }
        rotulo.push(t.trim_end_matches(':'));
        let nome = rotulo.join(" ").trim().to_string();
        rotulo.clear();
        let mut valor: Vec<&PalavraLida> = Vec::new();
        let mut j = i + 1;
        while j < ordem.len() {
            let q = ordem[j];
            if q.texto.trim().ends_with(':') && q.texto.trim().len() > 1 { break; }
            let fim = valor.last().map_or(p.caixa[2], |v| v.caixa[2]);
            if !valor.is_empty() && q.caixa[0] - fim > altura * 1.5 { break; }
            valor.push(q);
            j += 1;
        }
        let caixa = match (valor.first(), valor.last()) {
            (Some(a), Some(b)) => [a.caixa[0], linha.caixa[1], b.caixa[2], linha.caixa[3]],
            _ => {
                let fim = ordem.get(j).map_or(linha.caixa[2].max(p.caixa[2] + altura * 6.0), |q| q.caixa[0] - altura * 0.5);
                [p.caixa[2] + altura * 0.3, linha.caixa[1], fim.min(0.98), linha.caixa[3]]
            }
        };
        if !nome.is_empty() && caixa[2] > caixa[0] {
            saida.push((nome, valor.iter().map(|v| v.texto.trim()).collect::<Vec<_>>().join(" "), caixa));
        }
        i = j;
    }
    saida
}

/// Troca por algarismo a letra que o OCR confunde (S→5, O→0, l→1, B→8) quando vem logo
/// depois de um algarismo: "6S" → "65", "1O0" → "100"; "B2" e "OS" ficam.
pub fn corrigir_numero(t: &str) -> Option<String> {
    let mut saida = String::with_capacity(t.len());
    let mut anterior_digito = false;
    for c in t.chars() {
        let trocada = match c {
            'S' | 's' if anterior_digito => '5',
            'O' | 'o' if anterior_digito => '0',
            'l' | 'I' | '|' if anterior_digito => '1',
            'B' if anterior_digito => '8',
            c => c,
        };
        anterior_digito = trocada.is_ascii_digit();
        saida.push(trocada);
    }
    (saida != t).then_some(saida)
}

impl EstadoOcr {
    /// Lendo agora (não pausado nem terminado).
    pub fn rodando(&self) -> bool { self.desde.is_some() && !self.pausado }

    /// Documento trocado ou fechado: o que estiver a caminho não vale mais.
    pub fn cancelar(&mut self) {
        let (escopo, caixas, lingua, endireitar, baixando) = (self.escopo, self.caixas, self.lingua, self.endireitar, self.baixando);
        *self = EstadoOcr { escopo, caixas, lingua, endireitar, baixando, rodada: self.rodada + 1, ..Default::default() };
    }

    fn opcoes(&self, atual: u32, total: usize) -> Vec<OpcaoPaginas> {
        vec![
            OpcaoPaginas { escopo: Escopo::SemTexto, rotulo: format!("Só as sem texto ({} de {total})", self.sem_texto.len()) },
            OpcaoPaginas { escopo: Escopo::Todas, rotulo: format!("Todas ({total})") },
            OpcaoPaginas { escopo: Escopo::Atual, rotulo: format!("Só a atual (p. {atual})") },
        ]
    }
}

impl App {
    pub fn update_ocr(&mut self, m: Oc) -> Task<Msg> {
        match m {
            Oc::Abrir => {
                if self.aberto.is_none() || self.janelas.aberta(Auxiliar::Ocr) { return Task::none(); }
                let abrir = self.alternar_auxiliar(Auxiliar::Ocr);
                if !self.ocr.lidas.is_empty() || self.ocr.desde.is_some() { return abrir; }
                let Some(a) = &self.aberto else { return abrir };
                // shortcut: extrai o texto de cada página na thread da interface; levar para a tarefa se PDF de milhares de páginas travar.
                self.ocr.sem_texto = (1..=a.tamanhos.len() as u32)
                    .filter(|&p| a.pdf.texto_pagina(p).map_or(true, |t| t.trim().chars().count() <= MINIMO_TEXTO)).collect();
                self.ocr.caixas = true;
                // Outras fichas na mesma pasta: candidatas a "ler todas para uma planilha".
                self.ocr.vizinhos = a.pdf.origem().and_then(|o| Some((o.to_path_buf(), o.parent()?.to_path_buf())))
                    .map(|(o, pasta)| std::fs::read_dir(pasta).into_iter().flatten().flatten().map(|e| e.path())
                        .filter(|p| *p != o && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))).collect())
                    .unwrap_or_default();
                self.ocr.vizinhos.sort();
                if self.ocr.escopo == Escopo::SemTexto && self.ocr.sem_texto.is_empty() {
                    self.ocr.aviso = Some(Ok("Todas as páginas já têm texto. Para ler de novo, escolha “Todas” em Páginas.".into()));
                    return abrir;
                }
                return Task::batch([abrir, self.update_ocr(Oc::Reconhecer)]);
            }
            Oc::Escopo(e) => self.ocr.escopo = e,
            Oc::Tique => {}
            Oc::Reconhecer => {
                let Some(a) = &self.aberto else { return Task::none() };
                let fila: Vec<u32> = match self.ocr.escopo {
                    Escopo::Atual => vec![self.pagina],
                    Escopo::Todas => (1..=a.tamanhos.len() as u32).collect(),
                    Escopo::SemTexto => self.ocr.sem_texto.clone(),
                };
                if fila.is_empty() {
                    self.ocr.aviso = Some(Ok("Todas as páginas já têm texto. Para ler de novo, escolha “Todas” em Páginas.".into()));
                    return Task::none();
                }
                let o = &mut self.ocr;
                (o.fila, o.feitas, o.lidas, o.ver, o.previa, o.editando, o.aplicado, o.aviso, o.pausado) =
                    (fila, 0, vec![], 0, None, None, false, None, false);
                o.rodada += 1;
                o.desde = Some(Instant::now());
                return self.ler_proxima();
            }
            Oc::Pausar(p) => {
                self.ocr.pausado = p;
                if !p && self.ocr.desde.is_some() && self.ocr.feitas == self.ocr.lidas.len() { return self.ler_proxima(); }
            }
            Oc::Lida(rodada, pagina, r) => {
                if rodada != self.ocr.rodada { return Task::none(); }
                let o = &mut self.ocr;
                match r {
                    Ok((linhas, giro)) => o.lidas.push(PaginaLida { pagina, giro, linhas }),
                    Err(e) => {
                        o.aviso = Some(Err(format!("Página {pagina}: {e}")));
                        o.desde = None;
                        return Task::none();
                    }
                }
                o.feitas += 1;
                let primeira = o.lidas.len() == 1;
                let proxima = if o.feitas < o.fila.len() {
                    if o.pausado { Task::none() } else { self.ler_proxima() }
                } else {
                    self.ocr.desde = None;
                    let n = self.ocr.lidas.len();
                    self.status = format!("OCR: {n} página(s) lidas. Confira e clique em Aplicar.");
                    Task::none()
                };
                return if primeira { Task::batch([proxima, self.previa(0)]) } else { proxima };
            }
            Oc::Previa(pagina, h) => if let Some(h) = h { self.ocr.previa = Some((pagina, h.0)) },
            Oc::Ver(i) => if i < self.ocr.lidas.len() {
                self.ocr.ver = i;
                self.ocr.editando = None;
                return self.previa(i);
            },
            Oc::Palavra(p, l, w) => {
                let mudou = p != self.ocr.ver;
                self.ocr.ver = p;
                self.ocr.editando = Some((l, w));
                let foco = crate::focar(id_edicao());
                return if mudou { Task::batch([self.previa(p), foco]) } else { foco };
            }
            Oc::Editar(t) => {
                let o = &mut self.ocr;
                if let (Some((l, w)), Some(pag)) = (o.editando, o.lidas.get_mut(o.ver)) {
                    if let Some(p) = pag.linhas.get_mut(l).and_then(|x| x.palavras.get_mut(w)) {
                        if p.original.is_none() { p.original = Some(p.texto.clone()); }
                        p.texto = t;
                        p.revisada = true;
                    }
                }
            }
            Oc::FimEdicao => {
                let o = &mut self.ocr;
                // Abrir e sair sem mudar também conta como conferida.
                if let (Some((l, w)), Some(pag)) = (o.editando.take(), o.lidas.get_mut(o.ver)) {
                    if let Some(p) = pag.linhas.get_mut(l).and_then(|x| x.palavras.get_mut(w)) { p.revisada = true; }
                }
            }
            Oc::Caixas(v) => self.ocr.caixas = v,
            Oc::Lingua(i) => {
                self.ocr.lingua = i;
                if let Some(m) = &LINGUAS[i] { if !m.presente() { return self.baixar_modelo(m); } }
            }
            Oc::Endireitar(v) => {
                self.ocr.endireitar = v;
                if v && !ORIENTACAO.presente() { return self.baixar_modelo(&ORIENTACAO); }
            }
            Oc::Trocar(v) => self.ocr.trocar = v,
            Oc::Limpar(v) => self.ocr.limpar = v,
            Oc::Formulario(v) => self.ocr.formulario = v,
            Oc::LerPasta => if !self.ocr.lendo_pasta {
                let Some(a) = &self.aberto else { return Task::none() };
                let mut arquivos = self.ocr.vizinhos.clone();
                if let Some(o) = a.pdf.origem() { arquivos.insert(0, o.to_path_buf()); }
                let base = a.pdf.origem().and_then(|o| o.parent()).and_then(|p| p.file_name())
                    .map_or("Fichas".into(), |n| n.to_string_lossy().into_owned());
                let (lingua, endireitar) = (self.ocr.lingua, self.ocr.endireitar && ORIENTACAO.presente());
                self.ocr.lendo_pasta = true;
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || ler_pasta(&arquivos, &base, lingua, endireitar)).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Ocr(Oc::PastaLida(r)));
            },
            Oc::PastaLida(r) => {
                self.ocr.lendo_pasta = false;
                match r { Ok(t) => self.status = t, Err(e) => self.ocr.aviso = Some(Err(e)) }
            }
            Oc::Baixado(r) => {
                self.ocr.baixando = None;
                if let Err(e) = r {
                    self.ocr.aviso = Some(Err(e));
                    // Sem o modelo, volta ao que funciona.
                    if LINGUAS[self.ocr.lingua].as_ref().is_some_and(|m| !m.presente()) { self.ocr.lingua = 0; }
                    if !ORIENTACAO.presente() { self.ocr.endireitar = false; }
                }
            }
            Oc::Aplicar => {
                let Some(a) = &mut self.aberto else { return Task::none() };
                let antes = a.pdf.clone();
                let mut n = 0;
                for pag in &self.ocr.lidas {
                    // Página que veio torta: o giro entra antes, e o texto lido já está nessa orientação.
                    if pag.giro != 0 {
                        if let Err(e) = a.pdf.girar_paginas(&[pag.pagina], pag.giro as i64) {
                            a.pdf = antes;
                            self.ocr.aviso = Some(Err(format!("Página {}: {e}", pag.pagina)));
                            return Task::none();
                        }
                    }
                    let linhas: Vec<(String, [f32; 4])> = pag.linhas.iter().map(|l| (
                        l.palavras.iter().map(|p| p.texto.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" "), l.caixa,
                    )).filter(|(t, _)| !t.is_empty()).collect();
                    let gravado = if self.ocr.trocar { a.pdf.trocar_por_texto(pag.pagina, &linhas) } else { a.pdf.inserir_camada_ocr(pag.pagina, &linhas) };
                    match gravado {
                        Ok(0) => {}
                        Ok(_) => n += 1,
                        Err(e) => {
                            a.pdf = antes;
                            self.ocr.aviso = Some(Err(format!("Página {}: {e}", pag.pagina)));
                            return Task::none();
                        }
                    }
                }
                if n == 0 { return Task::none(); }
                let clareadas = if self.ocr.limpar && !self.ocr.trocar {
                    let paginas: Vec<u32> = self.ocr.lidas.iter().map(|p| p.pagina).collect();
                    a.pdf.limpar_fundo(&paginas, &featherpdf_io::clarear_fundo_jpeg)
                } else { 0 };
                if self.ocr.formulario {
                    let mut campos = 0;
                    for pag in &self.ocr.lidas {
                        for (nome, valor, f) in pag.linhas.iter().flat_map(campos_da_linha) {
                            // A geometria já é a da página endireitada (o giro entrou acima).
                            let Ok((pagina_pt, giro_total)) = a.pdf.geometria(pag.pagina) else { continue };
                            let caixa = em_pontos(f, pagina_pt, giro_total);
                            let criado = a.pdf.criar_campo(pag.pagina, caixa, &featherpdf_core::CampoNovo::Texto, &nome);
                            if let Ok(nome) = criado {
                                campos += 1;
                                if !valor.is_empty() {
                                    if let Some(c) = a.pdf.campos().into_iter().find(|c| c.nome == nome && c.pagina == pag.pagina) {
                                        let _ = a.pdf.preencher(&c, &valor);
                                    }
                                }
                            }
                        }
                    }
                    self.status = format!("{campos} campo(s) de formulário criados.");
                }
                self.guardar_desfazer(antes);
                self.ocr.aplicado = true;
                let fundo = if clareadas > 0 { format!(" Fundo limpo em {clareadas} imagem(ns).") } else { String::new() };
                self.status = format!("Texto reconhecido em {n} página(s) — já dá para selecionar, copiar e buscar (Ctrl+F).{fundo} Ctrl+Z desfaz.");
                self.ocr.aviso = Some(Ok(self.status.clone()));
                self.objetos.clear();
                self.selecao_texto = None;
                self.form.campos = None;
                self.texto_da_pagina.0 = 0;
                return self.refazer_vista();
            }
        }
        Task::none()
    }

    fn ler_proxima(&self) -> Task<Msg> {
        let (Some(&pagina), Some(a)) = (self.ocr.fila.get(self.ocr.feitas), &self.aberto) else { return Task::none() };
        let vis = a.vis.clone();
        let Some(&(l, h)) = a.tamanhos.get(pagina as usize - 1) else { return Task::none() };
        let escala = (LADO / l.max(h)).clamp(1.0, 4.0);
        let (rodada, lingua, endireitar) = (self.ocr.rodada, self.ocr.lingua, self.ocr.endireitar && ORIENTACAO.presente());
        Task::perform(async move {
            tokio::task::spawn_blocking(move || ler(&vis, pagina, escala, lingua, endireitar)).await
                .unwrap_or_else(|e| Err(format!("Falha na tarefa do OCR: {e}")))
        }, move |r| Msg::Ocr(Oc::Lida(rodada, pagina, r)))
    }

    fn baixar_modelo(&mut self, m: &'static modelos::Modelo) -> Task<Msg> {
        if self.ocr.baixando.is_some() { return Task::none(); }
        self.ocr.baixando = Some(m.nome);
        self.ocr.aviso = None;
        Task::perform(async move {
            tokio::task::spawn_blocking(move || modelos::baixar(m)).await.unwrap_or_else(|e| Err(e.to_string()))
        }, |r| Msg::Ocr(Oc::Baixado(r)))
    }

    /// Desenha a página lida `i` no tamanho da janela (o dobro, para tela de alta densidade).
    fn previa(&self, i: usize) -> Task<Msg> {
        let (Some(pag), Some(a)) = (self.ocr.lidas.get(i), &self.aberto) else { return Task::none() };
        let (pagina, giro) = (pag.pagina, pag.giro);
        if self.ocr.previa.as_ref().is_some_and(|p| p.0 == pagina) { return Task::none(); }
        let vis = a.vis.clone();
        let Some(&tamanho) = a.tamanhos.get(pagina as usize - 1) else { return Task::none() };
        let (l, h) = em_pe(tamanho, giro);
        let escala = 2.0 * (PAGINA_TELA.0 / l).min(PAGINA_TELA.1 / h);
        Task::perform(async move {
            tokio::task::spawn_blocking(move || {
                let r = vis.desenhar(pagina, escala).ok()?;
                let img = ::image::RgbaImage::from_raw(r.largura, r.altura, r.rgba)?;
                // A prévia sai já endireitada, no mesmo sentido do texto lido.
                let img = match giro { 90 => ::image::imageops::rotate90(&img), 180 => ::image::imageops::rotate180(&img), 270 => ::image::imageops::rotate270(&img), _ => img };
                Some(imagem_rgba(img.width(), img.height(), img.into_raw()))
            }).await.ok().flatten()
        }, move |h| Msg::Ocr(Oc::Previa(pagina, h.map(Quadro))))
    }

    pub fn view_ocr(&self, id: WindowId) -> Element<'_, Msg> {
        let o = &self.ocr;
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;
        let total_doc = self.total_paginas() as usize;
        let lendo = o.desde.is_some();

        // Barra de cima: páginas e o andamento.
        let opcoes = o.opcoes(self.pagina, total_doc);
        let escolhida = opcoes.iter().find(|x| x.escopo == o.escopo).cloned();
        let mut barra = row![
            text("Reconhecer texto (OCR)").size(13).font(negrito),
            Space::with_width(16),
            text("Páginas").size(13).color(muted),
            suspensa(opcoes, escolhida, |x| Msg::Ocr(Oc::Escopo(x.escopo))).text_size(13).padding([4, 8]),
            Space::with_width(8),
            text("Idioma").size(13).color(muted),
            suspensa((0..LINGUAS.len()).map(|i| OpcaoLingua(i, modelos::nome_lingua(i))).collect::<Vec<_>>(),
                Some(OpcaoLingua(o.lingua, modelos::nome_lingua(o.lingua))), |x| Msg::Ocr(Oc::Lingua(x.0))).text_size(13).padding([4, 8]),
            Space::with_width(8),
            checkbox("Endireitar a página", o.endireitar).on_toggle(|v| Msg::Ocr(Oc::Endireitar(v))).size(15).text_size(13),
            Space::with_width(Length::FILL),
        ].spacing(8).align_y(Align::Center);
        if let Some(nome) = o.baixando {
            barra = barra.push(text(format!("Baixando o modelo: {nome}…")).size(12).color(tema::COR_AZUL_ACENTO()));
        }
        if lendo {
            let segundos = o.desde.map_or(0, |d| d.elapsed().as_secs());
            let atual = o.fila.get(o.feitas).copied().unwrap_or(0);
            barra = barra.push(text(if o.pausado { format!("pausado em {} de {}", o.feitas, o.fila.len()) }
                    else { format!("p. {atual} · {} de {} · {}:{:02}", o.feitas + 1, o.fila.len(), segundos / 60, segundos % 60) }).size(12).color(muted))
                .push(container(progress_bar(0.0..=o.fila.len() as f32, o.feitas as f32).height(6.0)).width(110))
                .push(button(text(if o.pausado { "Continuar" } else { "Pausar" }).size(13)).padding([5, 12])
                    .style(tema::estilo_botao_pequeno(false)).on_press(Msg::Ocr(Oc::Pausar(!o.pausado))));
        } else {
            barra = barra.push(button(text(if o.lidas.is_empty() { "Reconhecer" } else { "Ler de novo" }).size(13)).padding([5, 12])
                .style(tema::estilo_botao_pequeno(false)).on_press_maybe((self.aberto.is_some() && o.baixando.is_none()).then_some(Msg::Ocr(Oc::Reconhecer))));
        }

        // Meio: a imagem e o texto lido, lado a lado.
        let meio: Element<'_, Msg> = match o.lidas.get(o.ver) {
            None => {
                let msg = match &o.aviso {
                    Some(Ok(t)) | Some(Err(t)) => t.clone(),
                    None if lendo && o.feitas == 0 => "Lendo a primeira página… a primeira demora mais: o OCR está sendo preparado.".into(),
                    None => "Escolha as páginas e clique em Reconhecer.".into(),
                };
                // Fora do scrollable: conteúdo com altura Fill dentro dele derruba o Iced.
                container(text(msg).size(13).color(muted)).width(Length::FILL).height(Length::FILL).center().into()
            }
            Some(pag) => {
                let (l, h) = em_pe(self.aberto.as_ref().map_or((595.0, 842.0), |a| a.tamanhos.get(pag.pagina as usize - 1).copied().unwrap_or((595.0, 842.0))), pag.giro);
                let s = (PAGINA_TELA.0 / l).min(PAGINA_TELA.1 / h);
                let (w, a) = (l * s, h * s);
                let rotulo = |t: &'static str| text(t).size(12).color(muted);
                scrollable(container(column![
                    row![container(rotulo("Imagem da página")).width(w), container(rotulo("Texto reconhecido · clique para corrigir")).width(w)].spacing(24),
                    row![self.pagina_imagem(pag, w, a), self.pagina_texto(pag, w, a)].spacing(24),
                ].spacing(6)).padding(20)).direction(Direction::Both)
                    .width(Length::FILL).height(Length::FILL).into()
            }
        };
        let meio = container(meio)
            .width(Length::FILL).height(Length::FILL)
            .style(|_| Style { background: Some(Background::Color(Color::rgb8(0xe9, 0xeb, 0xef))), ..Default::default() });

        // Lateral: conferir, resultado e aplicar.
        let mut conferir = column![].spacing(4);
        let mut quantos = 0;
        for (p, pag) in o.lidas.iter().enumerate() {
            for (l, linha) in pag.linhas.iter().enumerate() {
                for (w, x) in linha.palavras.iter().enumerate().filter(|(_, x)| x.conferir()) {
                    quantos += 1;
                    if quantos > 60 { continue; }
                    let (descricao, nota, cor) = match &x.original {
                        Some(orig) => (format!("“{orig}” → {}", x.texto), "✓ corrigido".to_string(), Color::rgb8(0x28, 0x78, 0x46)),
                        None => (x.texto.clone(), format!("p. {} · {:.0}%", pag.pagina, x.confianca * 100.0), Color::rgb8(0xb0, 0x7d, 0x0b)),
                    };
                    conferir = conferir.push(button(row![text(descricao).size(12).width(Length::FILL), text(nota).size(11).color(cor)].spacing(6))
                        .width(Length::FILL).padding([5, 8]).on_press(Msg::Ocr(Oc::Palavra(p, l, w)))
                        .style(move |_, status| Style {
                            background: Some(Background::Color(if status == Status::Hovered { Color::rgb8(0xfd, 0xf3, 0xd8) } else { Color::rgb8(0xfe, 0xf8, 0xe8) })),
                            text_color: Some(tema::COR_TEXTO()), border: Border { radius: [4.0; 4], ..Border::default() }, ..Default::default()
                        }));
                }
            }
        }
        if quantos > 60 { conferir = conferir.push(text(format!("… e mais {}", quantos - 60)).size(12).color(muted)); }
        if quantos == 0 && !o.lidas.is_empty() { conferir = conferir.push(text("Nada para conferir: todas as palavras com certeza alta.").size(12).color(muted)); }
        let legenda = row![
            container(Space::new(9, 9)).style(|_| Style { background: Some(Background::Color(azul())), ..Default::default() }),
            text("certeza alta").size(11).color(muted),
            container(Space::new(9, 9)).style(|_| Style { background: Some(Background::Color(ambar())), ..Default::default() }),
            text("conferir").size(11).color(muted),
        ].spacing(5).align_y(Align::Center);
        let n = o.lidas.len();
        let mut lateral = column![
            text("Conferir").size(15).font(negrito),
            legenda,
            scrollable(conferir).height(Length::Fill(3)),
            text("Resultado").size(13).font(negrito),
        ].spacing(10);
        let opcao = |rotulo: &'static str, trocar: bool| radio(rotulo, trocar, Some(o.trocar), |v| Msg::Ocr(Oc::Trocar(v))).size(15).text_size(12);
        lateral = lateral.push(opcao("Texto invisível atrás da imagem", false));
        // A Helvetica do texto visível só tem a escrita latina.
        if o.lingua == 0 { lateral = lateral.push(opcao("Trocar a imagem por texto editável", true)); }
        if !o.trocar {
            lateral = lateral.push(checkbox("Limpar o fundo (cinza vira branco)", o.limpar)
                .on_toggle(|v| Msg::Ocr(Oc::Limpar(v))).size(15).text_size(12));
        }
        let rotulos: Vec<String> = o.lidas.iter().flat_map(|p| p.linhas.iter().flat_map(campos_da_linha)).map(|c| c.0).take(3).collect();
        if !rotulos.is_empty() {
            lateral = lateral.push(checkbox("Os campos viram formulário", o.formulario)
                .on_toggle(|v| Msg::Ocr(Oc::Formulario(v))).size(15).text_size(12));
            lateral = lateral.push(text(format!("{}…", rotulos.join(", "))).size(11).color(tema::COR_TEXTO_MUTED()));
        }
        if !o.vizinhos.is_empty() {
            let n = o.vizinhos.len();
            lateral = lateral.push(container(column![
                text(format!("Tem mais {n} PDF{} na pasta? Uma linha por arquivo, com os campos “Rótulo: valor”.", if n == 1 { "" } else { "s" })).size(12),
                button(text(if o.lendo_pasta { "Lendo a pasta…" } else { "Ler todos para uma planilha" }).size(12)).padding([4, 10])
                    .style(tema::estilo_botao_pequeno(false)).on_press_maybe((!o.lendo_pasta && !lendo).then_some(Msg::Ocr(Oc::LerPasta))),
            ].spacing(6)).padding(10).width(Length::FILL).style(|_| Style {
                background: Some(Background::Color(Color::rgb8(0xe6, 0xf2, 0xe9))), border: Border { radius: [4.0; 4], ..Border::default() }, ..Default::default()
            }));
        }
        match &o.aviso {
            Some(Err(e)) => lateral = lateral.push(text(e).size(12).color(tema::COR_ERRO())),
            Some(Ok(t)) if o.aplicado => lateral = lateral.push(text(t).size(12).color(Color::rgb8(0x28, 0x78, 0x46))),
            _ => {}
        }
        lateral = lateral.push(Space::with_height(Length::FILL))
            .push(text("Roda no computador, sem internet. Motor: PaddleOCR embutido.").size(11).color(muted))
            .push(button(text(if o.aplicado { "Aplicado ✓".to_string() } else if n == 1 { "Aplicar à página".into() } else { format!("Aplicar às {n} páginas") })
                    .size(13).width(Length::FILL).align_x(Align::Center))
                .width(Length::FILL).padding([8, 14]).style(tema::estilo_botao_primario)
                .on_press_maybe((n > 0 && !lendo && !o.aplicado).then_some(Msg::Ocr(Oc::Aplicar))));
        let lateral = container(lateral).width(280).height(Length::FILL).padding(14);

        // Rodapé: página, certeza média e as caixas.
        let mut rodape = row![].spacing(10).align_y(Align::Center);
        if let Some(pag) = o.lidas.get(o.ver) {
            let (soma, qtd) = pag.linhas.iter().flat_map(|l| &l.palavras).fold((0.0, 0), |(s, q), p| (s + p.confianca, q + 1));
            let seta = |nome: &'static str, i: Option<usize>| button(icones::icone(nome, 14.)).padding(3)
                .style(tema::estilo_botao_pequeno(false)).on_press_maybe(i.map(|i| Msg::Ocr(Oc::Ver(i))));
            rodape = rodape
                .push(seta("chevron-left", o.ver.checked_sub(1)))
                .push(text(format!("Página {} de {total_doc}", pag.pagina)).size(12))
                .push(seta("chevron-right", (o.ver + 1 < n).then_some(o.ver + 1)))
                .push(text(format!("{} de {} lidas", o.ver + 1, n)).size(12).color(muted))
                .push(text(if qtd == 0 { "sem texto".into() } else { format!("{:.0}% de certeza média", soma / qtd as f32 * 100.0) }).size(12).color(muted));
        }
        rodape = rodape.push(Space::with_width(Length::FILL))
            .push(checkbox("Mostrar caixas das palavras", o.caixas).on_toggle(|v| Msg::Ocr(Oc::Caixas(v))).size(14).text_size(12));

        let corpo = column![
            container(barra).padding([10, 16]),
            crate::inicio::linha_horizontal(),
            row![meio, crate::inicio::linha_vertical(), lateral].height(Length::FILL),
            crate::inicio::linha_horizontal(),
            container(rodape).padding([6, 16]),
        ];
        cromo::janela_com_barra("Reconhecer texto (OCR)", corpo.into(), Self::acoes_janela(id))
    }

    /// A página desenhada com uma caixa em cada palavra (azul: certeza alta; âmbar: conferir).
    fn pagina_imagem<'a>(&'a self, pag: &'a PaginaLida, w: f32, a: f32) -> Element<'a, Msg> {
        let o = &self.ocr;
        let fundo: Element<'_, Msg> = match &o.previa {
            Some((p, h)) if *p == pag.pagina => image(h.clone()).width(w).height(a).into(),
            _ => container(text("Desenhando…").size(12).color(tema::COR_TEXTO_MUTED())).center_x(w).center_y(a).into(),
        };
        let mut pilha = Stack::new().width(w).height(a).push(fundo);
        if o.caixas {
            let editando = o.editando;
            for (l, linha) in pag.linhas.iter().enumerate() {
                pilha = pilha.push(em_linha(linha, w, a, |wi, p, larg, alt| {
                    let cor = if p.conferir() { ambar() } else { azul() };
                    let ativa = editando == Some((l, wi));
                    container(Space::new(larg, alt)).style(move |_| Style {
                        background: Some(Background::Color(Color { a: if ativa { 0.35 } else { 0.16 }, ..cor })),
                        border: Border { color: cor, width: 1.0, radius: [1.0; 4] }, ..Default::default()
                    }).into()
                }));
            }
        }
        container(pilha).style(sombra).into()
    }

    /// O texto lido no mesmo lugar da página; a palavra clicada vira campo para corrigir.
    fn pagina_texto<'a>(&'a self, pag: &'a PaginaLida, w: f32, a: f32) -> Element<'a, Msg> {
        let o = &self.ocr;
        let ver = o.ver;
        let mut pilha = Stack::new().width(w).height(a).push(Space::new(w, a));
        for (l, linha) in pag.linhas.iter().enumerate() {
            pilha = pilha.push(em_linha(linha, w, a, move |wi, p, larg, alt| {
                let tamanho = (alt * 0.72).clamp(5.0, 22.0);
                if o.editando == Some((l, wi)) {
                    return text_input("", &p.texto).id(id_edicao()).on_input(|t| Msg::Ocr(Oc::Editar(t))).on_submit(Msg::Ocr(Oc::FimEdicao))
                        .size(tamanho).padding(0).width(larg.max(40.0)).into();
                }
                let conferir = p.conferir();
                let corrigida = p.original.is_some();
                button(text(p.texto.clone()).size(tamanho).no_wrap()).padding(0).width(larg).height(alt)
                    .on_press(Msg::Ocr(Oc::Palavra(ver, l, wi)))
                    .style(move |_, status| Style {
                        background: if status == Status::Hovered { Some(Background::Color(Color { a: 0.12, ..azul() })) }
                            else if conferir { Some(Background::Color(Color { a: 0.22, ..ambar() })) } else { None },
                        text_color: Some(Color::rgb8(0x20, 0x21, 0x24)),
                        border: Border { color: if corrigida { ambar() } else { Color::TRANSPARENT }, width: if corrigida { 1.0 } else { 0.0 }, radius: [1.0; 4] },
                        ..Default::default()
                    }).into()
            }));
        }
        container(pilha).style(|_| Style {
            background: Some(Background::Color(Color::WHITE)),
            ..sombra(&tema::tema())
        }).into()
    }
}

/// Uma linha posicionada na página da tela: `item` desenha cada palavra na sua caixa.
fn em_linha<'a>(linha: &'a LinhaLida, w: f32, a: f32, item: impl Fn(usize, &'a PalavraLida, f32, f32) -> Element<'a, Msg>) -> Element<'a, Msg> {
    let y = linha.caixa[1] * a;
    let alt = ((linha.caixa[3] - linha.caixa[1]) * a).max(4.0);
    let mut fila = row![];
    let mut x = 0.0;
    // Na ordem da tela (a da leitura pode ser da direita para a esquerda).
    let mut ordem: Vec<usize> = (0..linha.palavras.len()).collect();
    ordem.sort_by(|&i, &j| linha.palavras[i].caixa[0].total_cmp(&linha.palavras[j].caixa[0]));
    for i in ordem {
        let p = &linha.palavras[i];
        let (x0, x1) = (p.caixa[0] * w, p.caixa[2] * w);
        if x0 > x { fila = fila.push(Space::with_width(x0 - x)); }
        let larg = (x1 - x0.max(x)).max(2.0);
        fila = fila.push(item(i, p, larg, alt));
        x = x0.max(x) + larg;
    }
    column![Space::with_height(y), fila].into()
}

fn sombra(_: &Theme) -> Style {
    Style {
        shadow: Some(Shadow { color: Color { a: 0.18, ..Color::BLACK }, offset: Point::new(0.0, 2.0), blur: 8.0 }),
        ..Default::default()
    }
}

fn azul() -> Color { Color::rgb8(0x23, 0x6a, 0xc7) }
fn ambar() -> Color { Color::rgb8(0xd9, 0x9a, 0x06) }

fn id_edicao() -> &'static str { "ocr-edicao" }

/// Desenha a página e devolve as linhas e palavras em fração da página (0..1, y para baixo).
pub(crate) fn ler(vis: &Visualizador, pagina: u32, escala: f32, lingua: usize, endireitar: bool) -> Lidas {
    let ocr = motor(lingua)?;
    let r = vis.desenhar(pagina, escala)?;
    let rgb: Vec<u8> = r.rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut img = (r.largura, r.altura, rgb);
    let mut giro = 0;
    if endireitar {
        let (graus, certeza) = orientacao()?.detectar(&Imagem { largura: img.0, altura: img.1, rgb: &img.2 })?;
        // Com dúvida, deixa como está: girar errado estraga mais que não girar.
        if graus != 0 && certeza > 0.6 {
            giro = 360 - graus;
            img = featherpdf_ocr::girar(&Imagem { largura: img.0, altura: img.1, rgb: &img.2 }, giro);
        }
    }
    let (l, a) = (img.0 as f32, img.1 as f32);
    let fracao = |c: [f32; 4]| [c[0] / l, c[1] / a, c[2] / l, c[3] / a];
    let rtl = LINGUAS[lingua].as_ref().is_some_and(|m| m.direita_esquerda);
    let linhas = ocr.reconhecer(&Imagem { largura: img.0, altura: img.1, rgb: &img.2 })?.into_iter().map(|x| {
        let mut palavras: Vec<PalavraLida> = x.palavras.into_iter().map(|p| {
            // shortcut: o árabe vira só invertendo a ordem (letras e palavras), sem algoritmo bidi; número no meio do texto sai invertido.
            let lido = if rtl { p.texto.chars().rev().collect() } else { p.texto };
            let original = corrigir_numero(&lido).map(|_| lido.clone());
            PalavraLida { texto: corrigir_numero(&lido).unwrap_or(lido), original, caixa: fracao(p.caixa), confianca: p.confianca, revisada: false }
        }).collect();
        if rtl { palavras.reverse(); }
        LinhaLida { caixa: fracao(x.caixa), palavras }
    }).collect();
    Ok((linhas, giro))
}

/// Caixa em fração da página vista (y para baixo) -> pontos do PDF, desfazendo o /Rotate.
pub(crate) fn em_pontos(f: [f32; 4], [x0, y0, x1, y1]: [f32; 4], giro: i64) -> [f32; 4] {
    let (l, h) = if giro.rem_euclid(180) == 90 { (y1 - y0, x1 - x0) } else { (x1 - x0, y1 - y0) };
    let ponto = |u: f32, v: f32| {
        let (u, v) = (u * l, v * h);
        match giro.rem_euclid(360) { 90 => (x0 + v, y0 + u), 180 => (x1 - u, y0 + v), 270 => (x1 - v, y1 - u), _ => (x0 + u, y1 - v) }
    };
    let (p, q) = (ponto(f[0], f[1]), ponto(f[2], f[3]));
    [p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1)]
}

/// Lê todos os PDFs (todas as páginas) e leva os campos "Rótulo: valor" a uma planilha.
fn ler_pasta(arquivos: &[std::path::PathBuf], base: &str, lingua: usize, endireitar: bool) -> Result<String, String> {
    let mut linhas = Vec::new();
    for p in arquivos {
        let Ok(pdf) = featherpdf_io::abrir(p) else { continue };
        let Ok(vis) = pdf.serializar().and_then(Visualizador::novo) else { continue };
        let mut pares: Vec<(String, String)> = Vec::new();
        for n in 1..=pdf.total_paginas() {
            let Ok(([x0, y0, x1, y1], _)) = pdf.geometria(n) else { continue };
            let (l, h) = (x1 - x0, y1 - y0);
            let Ok((lidas, _)) = ler(&vis, n, (LADO / l.max(h)).clamp(1.0, 4.0), lingua, endireitar) else { continue };
            for (nome, valor, _) in lidas.iter().flat_map(campos_da_linha) {
                if !pares.iter().any(|(k, _)| *k == nome) { pares.push((nome, valor)); }
            }
        }
        if !pares.is_empty() { linhas.push((p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), pares)); }
    }
    if linhas.is_empty() { return Err("Não achei campos “Rótulo: valor” nos PDFs da pasta.".into()); }
    crate::formularios::para_sheet(&linhas, base)
}

/// Largura e altura da página vista depois do giro.
fn em_pe((l, h): (f32, f32), giro: u32) -> (f32, f32) { if giro % 180 == 90 { (h, l) } else { (l, h) } }

#[cfg(test)]
mod testes {
    use super::corrigir_numero;

    #[test]
    fn letra_depois_de_algarismo_vira_algarismo() {
        assert_eq!(corrigir_numero("6S").as_deref(), Some("65"));
        assert_eq!(corrigir_numero("1O0").as_deref(), Some("100"));
        assert_eq!(corrigir_numero("14/1O/2026").as_deref(), Some("14/10/2026"));
        assert_eq!(corrigir_numero("B2"), None);
        assert_eq!(corrigir_numero("OS"), None);
        assert_eq!(corrigir_numero("Data"), None);
    }

    #[test]
    fn rotulo_e_valor_viram_campo() {
        use super::{campos_da_linha, LinhaLida, PalavraLida};
        let p = |t: &str, x0: f32, x1: f32| PalavraLida { texto: t.into(), original: None, caixa: [x0, 0.1, x1, 0.12], confianca: 0.9, revisada: false };
        // "Data: 14/10/2026        Inspetor: J. Andrade        Nome:"
        let linha = LinhaLida { caixa: [0.05, 0.1, 0.9, 0.12], palavras: vec![
            p("Data:", 0.05, 0.10), p("14/10/2026", 0.11, 0.20), p("Inspetor:", 0.40, 0.48), p("J.", 0.49, 0.51), p("Andrade", 0.52, 0.60), p("Nome:", 0.75, 0.80),
        ] };
        let c = campos_da_linha(&linha);
        let pares: Vec<_> = c.iter().map(|x| (x.0.as_str(), x.1.as_str())).collect();
        assert_eq!(pares, [("Data", "14/10/2026"), ("Inspetor", "J. Andrade"), ("Nome", "")]);
        assert!(c[2].2[0] > 0.80 && c[2].2[2] > c[2].2[0], "{:?}", c[2].2);
    }
}
