//! Aba Páginas: grade de miniaturas para escolher, arrastar e reordenar, e no painel
//! lateral Números de página, Padronizar tamanho, Dividir, Frente e verso e Caixas da página.
//! Desenho de referência: quadros "Organizar páginas" e "Dividir" do canvas.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use feather_ui::dialogo as dlg;
use feather_ui::{icones, tema};
use featherpdf_core::{ler_intervalos, DocumentoPdf, EstiloRotulo, FormatoNumero, Numeracao, PosicaoNumero};
use featherpdf_io::PaginaRaster;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::inicio::VERMELHO;
use crate::{Aba, App, Msg, Painel};

const CEL_L: f32 = 150.0;
const IMG_L: f32 = 128.0;
const IMG_A: f32 = 172.0;
const CEL_A: f32 = IMG_A + 46.0;
const VAO: f32 = 18.0;
const MARGEM: f32 = 22.0;
const BARRA_INFO: f32 = 44.0;
const CLIQUE_DUPLO: Duration = Duration::from_millis(400);

pub fn id_organizador() -> &'static str { "featherpdf.organizador" }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tamanho { A4, Carta, A3 }

impl Tamanho {
    const TODOS: [Tamanho; 3] = [Tamanho::A4, Tamanho::Carta, Tamanho::A3];
    fn pontos(self) -> (f32, f32) {
        match self { Tamanho::A4 => (595.0, 842.0), Tamanho::Carta => (612.0, 792.0), Tamanho::A3 => (842.0, 1191.0) }
    }
    fn nome(self) -> &'static str {
        match self { Tamanho::A4 => "A4 (210 × 297 mm)", Tamanho::Carta => "Carta (216 × 279 mm)", Tamanho::A3 => "A3 (297 × 420 mm)" }
    }
}

/// Como dividir: a cada N páginas, intervalos escritos, por marcador de primeiro nível,
/// nas páginas em branco, onde um texto aparece ou pelo tamanho máximo de cada arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoDivisao { ACada, Intervalos, Marcadores, Branco, Texto, Tamanho }

#[derive(Debug, Clone, PartialEq)]
pub enum Dialogos {
    Numeros { formato: FormatoNumero, posicao: PosicaoNumero, desde: String, primeiro: String },
    Padronizar(Tamanho),
    /// `texto`: páginas por arquivo, intervalos, texto procurado ou MB, conforme o modo.
    Dividir { modo: ModoDivisao, texto: String },
    /// `invertidos`: o maço foi virado inteiro, os versos vêm de trás para a frente.
    FrenteVerso { invertidos: bool },
    /// Esquerda, direita, topo e base em mm; `todas` ou só as páginas escolhidas.
    Margens { mm: [String; 4], todas: bool, caixa: featherpdf_core::CaixaPagina },
    /// Numeração Bates: prefixo, primeiro número, algarismos e posição.
    Bates { prefixo: String, primeiro: String, digitos: usize, posicao: PosicaoNumero },
    /// Artigo em colunas vira páginas seguidas; `todas` ou só as escolhidas.
    Colunas { colunas: u32, todas: bool },
    /// Numeração lógica (/PageLabels): páginas de/até, estilo, prefixo e primeiro número.
    Rotulos { de: String, ate: String, estilo: EstiloRotulo, prefixo: String, primeiro: String },
}

#[derive(Debug, Clone)]
pub enum Pag {
    Pressionou(u32),
    Sobre(u32),
    Soltou(u32),
    SoltouFora,
    Rolou(Viewport),
    Miniatura { geracao: u64, pagina: u32, resultado: Result<PaginaRaster, String> },
    SelecionarTudo,
    Duplicar,
    InserirBranco,
    Juntar,
    JuntarEscolhidos(Option<Vec<PathBuf>>),
    /// Posição em que as páginas entram e os PDFs já lidos.
    /// Aba que pediu, posição e os PDFs lidos.
    Juntados(Option<u64>, u32, Result<Vec<DocumentoPdf>, String>),
    Extrair,
    ExtrairPara(Option<PathBuf>),
    Abrir(Dialogos),
    Fechar,
    Mudar(Dialogos),
    AplicarNumeros,
    AplicarPadrao,
    Dividir,
    DividirEm(Option<PathBuf>),
    Divididos(Result<String, String>),
    AplicarFrenteVerso,
    AplicarMargens,
    AplicarBates,
    CapitulosImpar,
    AplicarColunas,
    AplicarRotulos,
    TirarRotulos,
    /// Guarda as páginas escolhidas para colar (neste ou em outro PDF aberto).
    Copiar,
    /// Copia e exclui.
    Recortar,
    /// Põe as páginas guardadas depois da última escolhida.
    Colar,
}

#[derive(Debug, Clone, Copy)]
struct Arrasto { origem: u32, alvo: Option<u32>, moveu: bool }

#[derive(Default)]
pub struct Organizador {
    pub selecao: BTreeSet<u32>,
    ancora: u32,
    arrasto: Option<Arrasto>,
    ultimo_clique: Option<(u32, Instant)>,
    pub miniaturas: HashMap<u32, Image>,
    pedidas: HashSet<u32>,
    /// Deslocamento y, largura e altura visíveis da grade.
    vista: Option<(f32, f32, f32)>,
    pub dialogo: Option<Dialogos>,
}

impl Organizador {
    pub fn limpar_miniaturas(&mut self) {
        self.miniaturas.clear();
        self.pedidas.clear();
    }
    fn escolher_trecho(&mut self, a: u32, b: u32) {
        self.selecao = (a.min(b)..=a.max(b)).collect();
    }
}

fn colunas(largura: f32) -> usize {
    (((largura - 2.0 * MARGEM + VAO) / (CEL_L + 6.0 + VAO)).floor() as usize).max(1)
}

/// Partes de uma divisão como (primeira, última) página.
/// Só para os modos que não precisam ler o documento (a cada N, intervalos).
fn partes_divisao(modo: ModoDivisao, texto: &str, total: u32) -> Result<Vec<(u32, u32)>, String> {
    if modo == ModoDivisao::Intervalos { return ler_intervalos(texto, total); }
    let n: u32 = texto.trim().parse().ok().filter(|&n| n > 0)
        .ok_or("Escreva quantas páginas vai cada arquivo.")?;
    Ok((1..=total).step_by(n as usize).map(|a| (a, (a + n - 1).min(total))).collect())
}

/// "3–5", "2, 4, 7" ou "1–3, 6" para o texto da seleção.
fn descrever(selecao: &BTreeSet<u32>) -> String {
    let mut trechos: Vec<(u32, u32)> = Vec::new();
    for &n in selecao {
        match trechos.last_mut() {
            Some((_, b)) if *b + 1 == n => *b = n,
            _ => trechos.push((n, n)),
        }
    }
    trechos.iter().map(|&(a, b)| if a == b { a.to_string() } else { format!("{a}–{b}") })
        .collect::<Vec<_>>().join(", ")
}

impl App {
    /// Páginas em que os comandos agem: a seleção na aba Páginas, senão a página atual.
    pub fn alvo(&self) -> Vec<u32> {
        if self.aba == Aba::Paginas && !self.org.selecao.is_empty() {
            self.org.selecao.iter().copied().collect()
        } else {
            vec![self.pagina]
        }
    }

    /// Pede as miniaturas das linhas à vista e de uma linha de cada lado.
    pub fn pedir_miniaturas(&mut self) -> Task<Msg> {
        let painel = self.painel == Some(crate::Painel::Miniaturas);
        if self.aba != Aba::Paginas && !painel { return Task::none(); }
        let Some(aberto) = &self.aberto else { return Task::none() };
        let total = aberto.tamanhos.len() as u32;
        let (inicio, fim) = if self.aba == Aba::Paginas {
            let (topo, largura, altura) = self.org.vista.unwrap_or((0.0, 1000.0, 900.0));
            let por_linha = colunas(largura) as u32;
            let linha_alt = CEL_A + VAO;
            let primeira = ((topo - MARGEM) / linha_alt).floor().max(1.0) as u32 - 1;
            let ultima = ((topo + altura) / linha_alt).ceil() as u32 + 1;
            (primeira * por_linha + 1, ((ultima + 1) * por_linha).min(total))
        } else {
            // Painel lateral: as páginas em volta da que está na tela (o painel acompanha a leitura).
            (self.pagina.saturating_sub(15).max(1), (self.pagina + 15).min(total))
        };
        // Guarda só as miniaturas perto da vista: num PDF de milhares de páginas elas somariam GBs.
        self.org.miniaturas.retain(|&p, _| p + 200 >= inicio && p <= fim + 200);
        let mut tarefas = Vec::new();
        for pagina in inicio..=fim {
            if self.org.miniaturas.contains_key(&pagina) || !self.org.pedidas.insert(pagina) { continue; }
            let (l, a) = aberto.tamanhos[pagina as usize - 1];
            // Abaixo de 0,1 o Hayro recusa; a imagem é reduzida na tela.
            let escala = ((IMG_L / l).min(IMG_A / a) * self.escala_tela).clamp(0.1, 4.0);
            let vis = aberto.vis.clone();
            let geracao = self.geracao_fundos; // só muda com o documento ou o giro: zoom não joga a miniatura fora
            tarefas.push(Task::perform(async move {
                let _vez = crate::DESENHOS.acquire().await;
                tokio::task::spawn_blocking(move || { crate::alocador::prioridade_baixa(); vis.desenhar(pagina, escala) }).await
                    .unwrap_or_else(|e| Err(format!("Falha na tarefa de desenho: {e}")))
            }, move |resultado| Msg::Pag(Pag::Miniatura { geracao, pagina, resultado })));
        }
        Task::batch(tarefas)
    }

    pub fn update_paginas(&mut self, msg: Pag) -> Task<Msg> {
        let total = self.total_paginas();
        match msg {
            Pag::Pressionou(n) => {
                let mods = self.modificadores;
                if mods.control_key() {
                    if !self.org.selecao.remove(&n) { self.org.selecao.insert(n); }
                    self.org.ancora = n;
                } else if mods.shift_key() {
                    let ancora = if self.org.ancora == 0 { n } else { self.org.ancora };
                    self.org.escolher_trecho(ancora, n);
                } else {
                    if !self.org.selecao.contains(&n) {
                        self.org.selecao = BTreeSet::from([n]);
                        self.org.ancora = n;
                    }
                    self.org.arrasto = Some(Arrasto { origem: n, alvo: None, moveu: false });
                }
                self.pagina = n;
            }
            Pag::Sobre(n) => if let Some(a) = &mut self.org.arrasto {
                if n != a.origem { a.moveu = true; }
                a.alvo = (n != a.origem).then_some(n);
            },
            Pag::Soltou(n) => {
                let Some(a) = self.org.arrasto.take() else { return Task::none() };
                if a.moveu {
                    if n == a.origem || self.org.selecao.contains(&n) { return Task::none(); }
                    let escolhidas: Vec<u32> = self.org.selecao.iter().copied().collect();
                    let antes_de = if n < a.origem { n } else { n + 1 };
                    let k = escolhidas.len() as u32;
                    let mut primeira = 0;
                    let tarefa = self.editar(|pdf| {
                        primeira = pdf.mover_paginas(&escolhidas, antes_de)?;
                        Ok(format!("{k} página(s) movida(s)."))
                    });
                    if primeira > 0 {
                        self.org.escolher_trecho(primeira, primeira + k - 1);
                        self.org.ancora = primeira;
                        self.pagina = primeira;
                    }
                    return tarefa;
                }
                // Clique sem arrastar: fica só esta; clique duplo abre a página para ler.
                self.org.selecao = BTreeSet::from([n]);
                self.org.ancora = n;
                let duplo = self.org.ultimo_clique.is_some_and(|(p, t)| p == n && t.elapsed() < CLIQUE_DUPLO);
                self.org.ultimo_clique = Some((n, Instant::now()));
                if duplo {
                    self.aba = Aba::Inicio;
                    return self.update(Msg::IrPagina(n));
                }
            }
            Pag::SoltouFora => self.org.arrasto = None,
            Pag::Rolou(vp) => {
                let (b, o) = (vp.bounds, vp.offset);
                self.org.vista = Some((o.y, b.width, b.height));
                return self.pedir_miniaturas();
            }
            Pag::Miniatura { geracao, pagina, resultado } => {
                if geracao != self.geracao_fundos { return Task::none(); }
                self.org.pedidas.remove(&pagina);
                match resultado {
                    Ok(r) => { self.org.miniaturas.insert(pagina, imagem_rgba(r.largura, r.altura, r.rgba)); }
                    Err(e) => self.status = format!("Página {pagina}: {e}"),
                }
            }
            Pag::SelecionarTudo => self.org.selecao = (1..=total).collect(),
            Pag::Duplicar => {
                let alvo = self.alvo();
                return self.editar(|pdf| {
                    pdf.duplicar_paginas(&alvo)?;
                    Ok(format!("{} página(s) duplicada(s).", alvo.len()))
                });
            }
            Pag::InserirBranco => {
                let depois = self.alvo().last().copied().unwrap_or(total);
                let tarefa = self.editar(|pdf| {
                    pdf.inserir_em_branco(depois + 1)?;
                    Ok(format!("Página em branco inserida depois da {depois}."))
                });
                self.org.selecao = BTreeSet::from([depois + 1]);
                self.pagina = depois + 1;
                return tarefa;
            }
            Pag::Juntar => return Task::perform(async {
                rfd::AsyncFileDialog::new().add_filter("PDF e imagens", &["pdf", "PDF", "png", "jpg", "jpeg", "PNG", "JPG", "JPEG"]).pick_files().await
                    .map(|v| v.into_iter().map(|h| h.path().to_path_buf()).collect())
            }, |v| Msg::Pag(Pag::JuntarEscolhidos(v))),
            Pag::JuntarEscolhidos(Some(caminhos)) => return self.juntar(caminhos, total + 1),
            Pag::JuntarEscolhidos(None) | Pag::ExtrairPara(None) | Pag::DividirEm(None) => {}
            Pag::Juntados(doc, _, _) if doc != self.id_aberto() => self.status = "A aba mudou enquanto os arquivos eram lidos: nada foi juntado.".into(),
            Pag::Juntados(_, posicao, Ok(pdfs)) => {
                let tarefa = self.editar(|pdf| {
                    let mut n = 0;
                    for outro in pdfs { n += pdf.inserir_pdf(outro, posicao + n)?; }
                    Ok(format!("{n} página(s) juntada(s) a partir da {posicao}."))
                });
                return tarefa;
            }
            Pag::Juntados(_, _, Err(e)) => self.status = e,
            Pag::Extrair => {
                let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
                let base = nome.strip_suffix(".pdf").unwrap_or(&nome).to_string();
                let sugestao = format!("{base} (páginas {}).pdf", descrever(&self.alvo().into_iter().collect()));
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(sugestao).save_file().await
                        .map(|h| h.path().to_path_buf())
                }, |d| Msg::Pag(Pag::ExtrairPara(d)));
            }
            Pag::ExtrairPara(Some(destino)) => if let Some(a) = &self.aberto {
                let alvo = self.alvo();
                self.status = match a.pdf.extrair_para_gravar(&alvo).and_then(|b| featherpdf_io::gravar_pdf(&destino, &b)) {
                    Ok(()) => format!("{} página(s) salva(s) em {}", alvo.len(), destino.display()),
                    Err(e) => e,
                };
            },
            Pag::Copiar | Pag::Recortar => if let Some(a) = &self.aberto {
                let alvo = self.alvo();
                let recortar = matches!(msg, Pag::Recortar);
                if recortar && alvo.len() >= self.total_paginas() as usize {
                    self.status = "O PDF precisa de ao menos uma página: copie em vez de recortar.".into();
                    return Task::none();
                }
                // shortcut: guarda as páginas inteiras na memória (um PDF delas); com muitas páginas pesadas, guardar só a lista e ler ao colar.
                match a.pdf.extrair(&alvo).and_then(|b| DocumentoPdf::de_bytes(&b, None)) {
                    Ok(copia) => {
                        self.paginas_copiadas = Some(copia);
                        if recortar { return self.update_interno(Msg::Excluir); }
                        self.status = format!("{} página(s) copiada(s). Colar põe depois da página escolhida, aqui ou em outro PDF.", alvo.len());
                    }
                    Err(e) => self.status = e,
                }
            },
            Pag::Colar => if let Some(copia) = self.paginas_copiadas.clone() {
                let posicao = self.alvo().last().copied().unwrap_or(total) + 1;
                return self.editar(move |pdf| {
                    let n = pdf.inserir_pdf(copia, posicao)?;
                    Ok(format!("{n} página(s) colada(s) a partir da {posicao}. Ctrl+Z desfaz."))
                });
            },
            Pag::Abrir(d) => { self.org.dialogo = Some(d); self.painel = Some(Painel::Paginas); }
            Pag::Fechar => self.fechar_dialogo(),
            Pag::AplicarFrenteVerso => if let Some(Dialogos::FrenteVerso { invertidos }) = self.org.dialogo {
                self.fechar_dialogo();
                return self.editar(|pdf| {
                    pdf.intercalar_frente_verso(invertidos)?;
                    Ok("Frentes e versos intercalados (Ctrl+Z desfaz).".into())
                });
            },
            Pag::AplicarMargens => if let Some(Dialogos::Margens { mm, todas, caixa }) = self.org.dialogo.clone() {
                let Some(pt) = margens_pt(&mm) else { return Task::none() };
                let paginas: Vec<u32> = if todas { (1..=total).collect() } else { self.alvo() };
                self.fechar_dialogo();
                return self.editar(|pdf| {
                    let n = pdf.definir_caixa(&paginas, caixa, pt)?;
                    let zero = pt.iter().all(|v| *v == 0.0);
                    Ok(match caixa {
                        featherpdf_core::CaixaPagina::Corte if zero => format!("Corte desfeito em {n} página(s)."),
                        featherpdf_core::CaixaPagina::Corte => format!("Margens cortadas em {n} página(s). O conteúdo continua no arquivo, só escondido."),
                        _ if zero => format!("{} tirada de {n} página(s).", caixa.nome()),
                        _ => format!("{} definida em {n} página(s).", caixa.nome()),
                    })
                });
            },
            Pag::AplicarBates => if let Some(Dialogos::Bates { prefixo, primeiro, digitos, posicao }) = self.org.dialogo.clone() {
                let Ok(n) = primeiro.trim().parse::<u32>() else { return Task::none() };
                self.fechar_dialogo();
                return self.editar(move |pdf| {
                    let proximo = pdf.numerar_bates(&prefixo, n, digitos, posicao, 9.0)?;
                    Ok(format!("Bates {prefixo}{n:0digitos$} a {prefixo}{:0digitos$}. O próximo lote começa em {proximo}.", proximo - 1))
                });
            },
            Pag::CapitulosImpar => {
                let inicios: Vec<u32> = self.aberto.as_ref().map(|a| a.pdf.marcadores().into_iter().filter(|m| m.nivel == 1 && m.pagina > 0).map(|m| m.pagina).collect()).unwrap_or_default();
                self.fechar_dialogo();
                return self.editar(move |pdf| match pdf.capitulos_na_impar(&inicios)? {
                    0 if inicios.is_empty() => Err("O PDF não tem marcadores de capítulo (painel Marcadores).".into()),
                    0 => Err("Todos os capítulos já começam em página ímpar.".into()),
                    n => Ok(format!("{n} página(s) em branco inseridas: cada capítulo abre numa página ímpar.")),
                });
            }
            Pag::AplicarColunas => if let Some(Dialogos::Colunas { colunas, todas }) = self.org.dialogo {
                let paginas: Vec<u32> = if todas { (1..=total).collect() } else { self.alvo() };
                self.fechar_dialogo();
                return self.editar(move |pdf| match pdf.dividir_colunas(&paginas, colunas)? {
                    0 => Err("Nenhuma página dividida: em todas, uma figura atravessa o corte ou a página está deitada.".into()),
                    n => Ok(format!("{n} página(s) viraram {colunas} cada (as com figura larga e as deitadas ficaram inteiras).")),
                });
            },
            Pag::Mudar(d) => if self.org.dialogo.is_some() { self.org.dialogo = Some(d) },
            Pag::AplicarRotulos => if let Some(Dialogos::Rotulos { de, ate, estilo, prefixo, primeiro }) = self.org.dialogo.clone() {
                let (Ok(de), Ok(ate), Ok(primeiro)) = (de.trim().parse::<u32>(), ate.trim().parse::<u32>(), primeiro.trim().parse::<u32>()) else { return Task::none() };
                self.fechar_dialogo();
                return self.editar(move |pdf| {
                    pdf.numerar_logico(de, ate, estilo, &prefixo, primeiro)?;
                    Ok(format!("Numeração lógica nas páginas {de} a {ate}. Ctrl+Z desfaz."))
                });
            },
            Pag::TirarRotulos => {
                self.fechar_dialogo();
                return self.editar(|pdf| match pdf.tirar_numeracao_logica()? {
                    true => Ok("Numeração lógica tirada: as páginas voltam a 1, 2, 3… Ctrl+Z desfaz.".into()),
                    false => Err("Este PDF não tem numeração lógica.".into()),
                });
            }
            Pag::AplicarNumeros => if let Some(Dialogos::Numeros { formato, posicao, desde, primeiro }) = self.org.dialogo.clone() {
                let (Ok(a_partir_de), Ok(primeiro)) = (desde.trim().parse(), primeiro.trim().parse()) else { return Task::none() };
                self.fechar_dialogo();
                return self.editar(|pdf| {
                    pdf.numerar(Numeracao { formato, posicao, a_partir_de, primeiro, tamanho: 10.0 })?;
                    Ok("Números de página escritos.".into())
                });
            },
            Pag::AplicarPadrao => if let Some(Dialogos::Padronizar(t)) = self.org.dialogo {
                self.fechar_dialogo();
                let (l, a) = t.pontos();
                return self.editar(|pdf| match pdf.padronizar_tamanho(l, a)? {
                    0 => Err("Todas as páginas já estavam nesse tamanho.".into()),
                    n => Ok(format!("{n} página(s) passaram a {}.", t.nome())),
                });
            },
            Pag::Dividir => return Task::perform(async {
                rfd::AsyncFileDialog::new().pick_folder().await.map(|h| h.path().to_path_buf())
            }, |p| Msg::Pag(Pag::DividirEm(p))),
            Pag::DividirEm(Some(pasta)) => {
                let (Some(a), Some(Dialogos::Dividir { modo, texto })) = (&self.aberto, &self.org.dialogo) else { return Task::none() };
                let (modo, texto) = (*modo, texto.clone());
                let pdf = a.pdf.clone();
                let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
                let base = nome.strip_suffix(".pdf").unwrap_or(&nome).to_string();
                self.fechar_dialogo();
                self.status = "Dividindo…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let partes: Vec<featherpdf_core::Parte> = match modo {
                            ModoDivisao::ACada | ModoDivisao::Intervalos =>
                                partes_divisao(modo, &texto, total)?.into_iter().map(|(a, b)| (a, b, String::new())).collect(),
                            ModoDivisao::Marcadores => pdf.partes_por_marcadores()?,
                            ModoDivisao::Branco => pdf.partes_por_branco()?,
                            ModoDivisao::Texto => pdf.partes_por_texto(&texto)?,
                            ModoDivisao::Tamanho => {
                                let mb = megabytes(&texto).ok_or("Escreva o tamanho máximo em MB.")?;
                                pdf.partes_por_tamanho((mb * 1024.0 * 1024.0) as usize)?
                            }
                        };
                        for (i, (a, b, rotulo)) in partes.iter().enumerate() {
                            let paginas: Vec<u32> = (*a..=*b).collect();
                            // O título do marcador vira o nome, sem os caracteres que o sistema recusa.
                            let rotulo: String = rotulo.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect::<String>().trim().chars().take(80).collect();
                            let destino = if rotulo.is_empty() { pasta.join(format!("{base} - parte {} (p. {a}-{b}).pdf", i + 1)) }
                                else { pasta.join(format!("{base} - {:02} {rotulo}.pdf", i + 1)) };
                            featherpdf_io::gravar_pdf(&destino, &pdf.extrair_para_gravar(&paginas)?)?;
                        }
                        Ok(format!("{} arquivos salvos em {}", partes.len(), pasta.display()))
                    }).await.unwrap_or_else(|e| Err(format!("Falha ao dividir: {e}")))
                }, |r| Msg::Pag(Pag::Divididos(r)));
            }
            Pag::Divididos(r) => self.status = r.unwrap_or_else(|e| e),
        }
        Task::none()
    }

    /// Lê os PDFs fora da janela e junta a partir de `posicao`.
    pub fn juntar(&mut self, caminhos: Vec<PathBuf>, posicao: u32) -> Task<Msg> {
        self.status = format!("Lendo {} arquivo(s)…", caminhos.len());
        let aba = self.id_aberto();
        Task::perform(async move {
            tokio::task::spawn_blocking(move || {
                caminhos.iter().map(|c| crate::juntar::documento_de_arquivo(c)
                    .map_err(|e| format!("{}: {e}", c.display()))).collect()
            }).await.unwrap_or_else(|e| Err(format!("Falha ao ler: {e}")))
        }, move |r| Msg::Pag(Pag::Juntados(aba, posicao, r)))
    }

    // ---------- Tela ----------

    pub fn organizador(&self) -> Element<'_, Msg> {
        let Some(aberto) = &self.aberto else { return Space::with_width(Length::FILL).into() };
        let negrito = Font::SEMIBOLD;
        let muted = tema::COR_TEXTO_MUTED();
        let sel = &self.org.selecao;
        let titulo = match sel.len() {
            0 => "Nenhuma página selecionada".to_string(),
            1 => "1 página selecionada".to_string(),
            n => format!("{n} páginas selecionadas"),
        };
        let titulo = if sel.len() > 1 { format!("{titulo} ({})", descrever(sel)) } else { titulo };
        let resumo = format!("{} páginas", aberto.tamanhos.len());
        let info = container(row![
            text(titulo).size(13).font(negrito),
            text("Arraste para reordenar · Shift+clique: trecho · Ctrl+clique: soma")
                .size(12).color(muted).no_wrap(),
            Space::with_width(Length::FILL),
            text(resumo).size(12).color(muted),
        ].spacing(14).align_y(Align::Center)).padding([0, MARGEM as u16]).height(BARRA_INFO).width(Length::FILL)
            .align_y(Align::Center);

        let grade = responsive(move |tamanho| {
            let por_linha = colunas(tamanho.width);
            let mut linhas = column![].spacing(VAO);
            let mut linha = row![].spacing(VAO);
            let mut na_linha = 0;
            for (i, &t) in aberto.tamanhos.iter().enumerate() {
                linha = linha.push(self.celula(i as u32 + 1, t));
                na_linha += 1;
                if na_linha == por_linha {
                    linhas = linhas.push(linha);
                    linha = row![].spacing(VAO);
                    na_linha = 0;
                }
            }
            linha = linha.push(self.cartao_juntar());
            linhas = linhas.push(linha);
            let conteudo = container(linhas).padding(Padding { top: 4.0, right: MARGEM, bottom: MARGEM, left: MARGEM })
                .width(tamanho.width - 14.0);
            mouse_area(scrollable(conteudo).id(id_organizador()).on_scroll(|v| Msg::Pag(Pag::Rolou(v)))
                .width(Length::FILL).height(Length::FILL))
                .on_release(Msg::Pag(Pag::SoltouFora))
                .into()
        });
        container(column![info, grade]).width(Length::FILL).height(Length::FILL)
            .style(|_| Style { background: Some(Background::Color(tema::COR_MESA())), ..Default::default() })
            .into()
    }

    fn celula(&self, n: u32, (l, a): (f32, f32)) -> Element<'_, Msg> {
        let escolhida = self.org.selecao.contains(&n);
        let s = (IMG_L / l).min(IMG_A / a);
        let (w, h) = (l * s, a * s);
        let img: Element<'_, Msg> = match self.org.miniaturas.get(&n) {
            Some(m) => image(m.clone()).width(w).height(h).into(),
            None => Space::new(w, h).into(),
        };
        let papel = container(img).style(move |_| Style {
            background: Some(Background::Color(Color::WHITE)),
            border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [2.0; 4] },
            shadow: Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.15), offset: Point::new(0.0, 1.0), blur: 3.0 }),
            ..Default::default()
        });
        let paisagem = l > a * 1.05;
        let rotulo = text(if paisagem { format!("{n} · paisagem") } else { n.to_string() }).size(12);
        // Escolhida: a célula inteira (página e número) num cartão azul, como no Acrobat e no Finder.
        let cel = container(column![
            container(papel).width(Length::FILL).height(IMG_A + 4.0).align_x(Align::Center).align_y(Align::End),
            rotulo,
        ].spacing(8).align_x(Align::Center)).width(CEL_L).height(CEL_A).padding([6, 6]).style(move |_| Style {
            background: escolhida.then(|| Background::Color(Color { a: 0.12, ..tema::COR_AZUL_ACENTO() })),
            border: Border { color: if escolhida { tema::COR_AZUL_ACENTO() } else { Color::TRANSPARENT }, width: 2.0, radius: [8.0; 4] },
            ..Default::default()
        });

        // Barra vermelha do lado em que as páginas arrastadas vão entrar.
        let lado = self.org.arrasto.filter(|a| a.moveu && a.alvo == Some(n) && !escolhida)
            .map(|a| if n < a.origem { -1 } else { 1 });
        let barra = |aqui: bool| container(Space::new(3, IMG_A)).style(move |_| Style {
            background: aqui.then(|| Background::Color(VERMELHO)),
            border: Border { radius: [2.0; 4], ..Default::default() },
            ..Default::default()
        });
        mouse_area(row![barra(lado == Some(-1)), cel, barra(lado == Some(1))])
            .on_press(Msg::Pag(Pag::Pressionou(n)))
            .on_enter(Msg::Pag(Pag::Sobre(n)))
            .on_release(Msg::Pag(Pag::Soltou(n)))
            .on_right_press(Msg::Ctx(crate::contexto::Ct::Abrir(crate::contexto::Alvo::Pagina(n))))
            .cursor_icon(CursorIcon::Grab)
            .into()
    }

    fn cartao_juntar(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        button(column![
            icones::icone_traco("file-plus", 26.),
            text("Soltar PDF aqui ou clicar para juntar").size(12).color(muted).align_x(Align::Center),
        ].spacing(8).align_x(Align::Center).width(Length::FILL))
            .width(CEL_L).height(IMG_A).padding(12)
            .style(|_, status| Style {
                background: Some(Background::Color(if matches!(status, Status::Hovered) { tema::COR_HOVER() } else { Color::TRANSPARENT })),
                border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [6.0; 4] },
                text_color: Some(tema::COR_TEXTO()),
                ..Default::default()
            })
            .on_press(Msg::Pag(Pag::Juntar))
            .into()
    }

    fn fechar_dialogo(&mut self) {
        self.org.dialogo = None;
        if self.painel == Some(Painel::Paginas) { self.painel = None; }
    }

    /// Ferramenta de página aberta no painel lateral: título e conteúdo.
    pub fn painel_paginas<'a>(&'a self) -> (&'static str, Element<'a, Msg>) {
        let Some(d) = &self.org.dialogo else {
            return ("Páginas", text("Escolha uma ferramenta na aba Páginas.").size(13).color(tema::COR_TEXTO_MUTED()).into());
        };
        let total = self.total_paginas();
        let muted = tema::COR_TEXTO_MUTED();
        let opcao = |rotulo: String, ativo: bool, msg: Msg| -> Element<'a, Msg> {
            button(text(rotulo).size(13)).padding([5, 10]).width(Length::FILL)
                .style(tema::estilo_botao_pequeno(ativo)).on_press(msg).into()
        };
        let campo = |rotulo: &'a str, valor: &str, f: Box<dyn Fn(String) -> Msg + 'a>| -> Element<'a, Msg> {
            row![text(rotulo).size(13).width(Length::FILL), text_input("", valor.to_owned()).on_input(f).width(70).padding([4, 8]).size(13)]
                .align_y(Align::Center).into()
        };
        let botoes = |rotulo: String, msg: Option<Msg>| -> Element<'a, Msg> {
            row![Space::with_width(Length::FILL), dlg::botao_secundario("Cancelar", Some(Msg::Pag(Pag::Fechar))), dlg::botao_primario(rotulo, msg)]
                .spacing(8).into()
        };
        let (titulo, corpo): (&'static str, Column<'a, Msg>) = match d.clone() {
            Dialogos::Numeros { formato, posicao, desde, primeiro } => {
                let com = |formato, posicao, desde: String, primeiro: String|
                    Msg::Pag(Pag::Mudar(Dialogos::Numeros { formato, posicao, desde, primeiro }));
                let mut formatos = column![].spacing(4);
                for (f, r) in [(FormatoNumero::Simples, "1"), (FormatoNumero::DeTotal, "1 de 9"), (FormatoNumero::PaginaDeTotal, "Página 1 de 9")] {
                    formatos = formatos.push(opcao(r.into(), f == formato, com(f, posicao, desde.clone(), primeiro.clone())));
                }
                let mut posicoes = column![].spacing(4);
                for (p, r) in [(PosicaoNumero::RodapeEsquerda, "Rodapé à esquerda"), (PosicaoNumero::RodapeCentro, "Rodapé ao centro"),
                    (PosicaoNumero::RodapeDireita, "Rodapé à direita"), (PosicaoNumero::TopoDireita, "Topo à direita")] {
                    posicoes = posicoes.push(opcao(r.into(), p == posicao, com(formato, p, desde.clone(), primeiro.clone())));
                }
                let d1 = desde.clone();
                let p1 = primeiro.clone();
                let valido = desde.trim().parse::<u32>().is_ok_and(|d| (1..=total).contains(&d))
                    && primeiro.trim().parse::<u32>().is_ok();
                let mut corpo = column![
                    dlg::secao("Formato"), formatos,
                    dlg::secao("Posição"), posicoes,
                    dlg::secao("Começo"),
                    campo("A partir da página", &desde, Box::new(move |t| com(formato, posicao, t, p1.clone()))),
                    campo("Primeiro número", &primeiro, Box::new(move |t| com(formato, posicao, d1.clone(), t))),
                ].spacing(10);
                if !valido { corpo = corpo.push(dlg::aviso("alert-triangle", format!("Use números: página entre 1 e {total}."))); }
                corpo = corpo.push(text("Helvetica 10 pt, preto. O número entra como texto na página; Ctrl+Z desfaz.").size(12).color(muted))
                    .push(botoes("Numerar".into(), valido.then_some(Msg::Pag(Pag::AplicarNumeros))));
                ("Números de página", corpo)
            }
            Dialogos::Padronizar(atual) => {
                let mut corpo = column![dlg::secao("Tamanho")].spacing(8);
                for t in Tamanho::TODOS {
                    corpo = corpo.push(opcao(t.nome().into(), t == atual, Msg::Pag(Pag::Mudar(Dialogos::Padronizar(t)))));
                }
                corpo = corpo.push(dlg::secao("Agora")).push(text(self.mistura_de_tamanhos()).size(13))
                    .push(text("Cada página mantém retrato ou paisagem; o conteúdo é reduzido ou ampliado e centrado.").size(12).color(muted))
                    .push(botoes("Padronizar".into(), Some(Msg::Pag(Pag::AplicarPadrao))));
                ("Padronizar tamanho", corpo)
            }
            Dialogos::Dividir { modo, texto } => {
                use ModoDivisao as M;
                let mut modos = column![].spacing(4);
                for (m, rotulo, inicial) in [
                    (M::ACada, "A cada N páginas", "1".to_string()), (M::Intervalos, "Intervalos (1-3, 4-9…)", format!("1-{}", total.div_ceil(2))),
                    (M::Marcadores, "Por marcador (capítulos do sumário)", String::new()), (M::Branco, "Nas páginas em branco", String::new()),
                    (M::Texto, "Onde um texto aparece", String::new()), (M::Tamanho, "Por tamanho de arquivo (MB)", "10".to_string()),
                ] {
                    modos = modos.push(opcao(rotulo.into(), modo == m, Msg::Pag(Pag::Mudar(Dialogos::Dividir { modo: m, texto: inicial }))));
                }
                let mut corpo = column![modos].spacing(12);
                let dica = match modo { M::ACada => Some("Páginas por arquivo"), M::Intervalos => Some("1-3, 4-9, 10-"),
                    M::Texto => Some("Ex.: Capítulo, Nota fiscal nº"), M::Tamanho => Some("MB por arquivo"), _ => None };
                if let Some(dica) = dica {
                    corpo = corpo.push(text_input(dica, texto.clone()).on_input(move |t| Msg::Pag(Pag::Mudar(Dialogos::Dividir { modo, texto: t }))).padding([5, 8]).size(13));
                }
                let partes = match modo {
                    M::ACada | M::Intervalos => partes_divisao(modo, &texto, total),
                    M::Marcadores => self.aberto.as_ref().ok_or_else(String::new).and_then(|a| a.pdf.partes_por_marcadores())
                        .map(|v| v.into_iter().map(|p| (p.0, p.1)).collect()),
                    M::Texto if texto.trim().is_empty() => Err("Escreva o texto que marca o começo de cada parte.".into()),
                    M::Tamanho if megabytes(&texto).is_none() => Err("Escreva o tamanho máximo em MB (ex.: 10 ou 2,5).".into()),
                    _ => Ok(Vec::new()),
                };
                if matches!(modo, M::Branco | M::Texto | M::Tamanho) && partes.is_ok() {
                    corpo = corpo.push(text(match modo {
                        M::Branco => "Cada página em branco separa uma parte e não entra em nenhuma.",
                        M::Texto => "Cada página que tem esse texto começa um arquivo novo.",
                        _ => "Cada arquivo leva o máximo de páginas que cabe no tamanho; uma página maior fica sozinha.",
                    }).size(12.5));
                }
                match &partes {
                    Ok(p) if p.is_empty() => {}
                    Ok(p) => {
                        let mut tabela = column![row![
                            text("Arquivo").size(12).color(muted).width(Length::FILL),
                            text("Páginas").size(12).color(muted).width(80),
                        ]].spacing(4);
                        for (i, &(a, b)) in p.iter().enumerate().take(12) {
                            tabela = tabela.push(row![
                                text(format!("parte {}", i + 1)).size(13).width(Length::FILL),
                                text(if a == b { a.to_string() } else { format!("{a}–{b}") }).size(13).width(80),
                            ]);
                        }
                        if p.len() > 12 { tabela = tabela.push(text(format!("… e mais {}", p.len() - 12)).size(12).color(muted)); }
                        corpo = corpo.push(container(tabela).padding([8, 12]).style(dlg::estilo_bloco));
                    }
                    Err(e) => corpo = corpo.push(dlg::aviso("alert-triangle", e.clone())),
                }
                let rotulo = match &partes { Ok(p) if !p.is_empty() => format!("Dividir em {}…", p.len()), _ => "Dividir…".into() };
                corpo = corpo.push(text("O arquivo aberto não muda; as partes vão para a pasta escolhida.").size(12).color(muted))
                    .push(botoes(rotulo, partes.is_ok().then_some(Msg::Pag(Pag::Dividir))));
                ("Dividir", corpo)
            }
            Dialogos::FrenteVerso { invertidos } => {
                let par = total % 2 == 0 && total >= 2;
                let metade = total / 2;
                let exemplo = if invertidos { format!("1, {total}, 2, {}, 3, {}…", total.saturating_sub(1), total.saturating_sub(2)) }
                    else { format!("1, {}, 2, {}, 3, {}…", metade + 1, metade + 2, metade + 3) };
                let mut corpo = column![
                    text("Para quem escaneou só a frente das folhas e depois o maço virado: a primeira metade são as frentes, a segunda os versos.")
                        .size(12.5),
                    dlg::secao("Os versos estão"),
                    opcao("De trás para a frente (maço virado inteiro)".into(), invertidos, Msg::Pag(Pag::Mudar(Dialogos::FrenteVerso { invertidos: true }))),
                    opcao("Na mesma ordem das frentes".into(), !invertidos, Msg::Pag(Pag::Mudar(Dialogos::FrenteVerso { invertidos: false }))),
                    dlg::secao("Nova ordem"),
                    text(if par { exemplo } else { String::new() }).size(13),
                ].spacing(8);
                if !par { corpo = corpo.push(dlg::aviso("alert-triangle", format!("São {total} páginas: precisa ser um número par."))); }
                corpo = corpo.push(botoes("Intercalar".into(), par.then_some(Msg::Pag(Pag::AplicarFrenteVerso))))
                    .push(dlg::secao("Para imprimir"))
                    .push(text("Cada capítulo (marcador de primeiro nível) passa a abrir numa página ímpar, com uma em branco antes quando preciso.").size(12).color(muted))
                    .push(opcao("Capítulos na página ímpar".into(), false, Msg::Pag(Pag::CapitulosImpar)))
                    .push(dlg::secao("Ou em dois arquivos"))
                    .push(text("Frentes num PDF e versos em outro: escolha os dois e o resultado abre como PDF novo.").size(12).color(muted))
                    .push(opcao("Escolher frentes e versos…".into(), false, Msg::Juntar(crate::juntar::Jt::FrenteVerso { invertidos })));
                ("Frente e verso", corpo)
            }
            Dialogos::Margens { mm, todas, caixa } => {
                use featherpdf_core::CaixaPagina;
                let com = move |mm: [String; 4], todas| Msg::Pag(Pag::Mudar(Dialogos::Margens { mm, todas, caixa }));
                // Caixas que a página atual já tem, em mm a partir da MediaBox.
                let atuais = self.aberto.as_ref().and_then(|a| a.pdf.caixas_da_pagina(self.alvo()[0]).ok()).unwrap_or_default();
                let em_mm = |v: f32| format!("{:.1}", v * 25.4 / 72.0).replace(".0", "").replace('.', ",");
                let mut corpo = column![dlg::secao("Caixa")].spacing(8);
                for c in CaixaPagina::TODAS {
                    let mm_atual = atuais.iter().find(|a| a.0 == c).map(|a| a.1.map(em_mm)).unwrap_or_default();
                    let rotulo = if atuais.iter().any(|a| a.0 == c) { format!("{} ✓", c.nome()) } else { c.nome().to_string() };
                    corpo = corpo.push(opcao(rotulo, c == caixa, Msg::Pag(Pag::Mudar(Dialogos::Margens { mm: mm_atual, todas, caixa: c }))));
                }
                corpo = corpo.push(text(match caixa {
                    CaixaPagina::Corte => "Esconde a borda das páginas (área branca, carimbo de scanner). O conteúdo continua no arquivo; zero em tudo desfaz o corte.",
                    CaixaPagina::Sangria => "Até onde a impressão vai antes do corte da gráfica (costuma ser 3 mm além do refile). Não muda a tela.",
                    CaixaPagina::Refile => "O tamanho final da folha depois de cortada na gráfica. Não muda a tela.",
                    CaixaPagina::Arte => "A área com o conteúdo que importa (usada ao montar a página em outro documento). Não muda a tela.",
                }).size(12.5)).push(dlg::secao("Margens a partir da folha (mm)"));
                for (i, nome) in ["Esquerda", "Direita", "Topo", "Base"].into_iter().enumerate() {
                    let atual = mm.clone();
                    corpo = corpo.push(campo(nome, &mm[i], Box::new(move |t| { let mut v = atual.clone(); v[i] = t; com(v, todas) })));
                }
                let escolhidas = self.alvo().len();
                corpo = corpo.push(dlg::secao("Em quais páginas"))
                    .push(opcao(format!("Todas ({total})"), todas, com(mm.clone(), true)))
                    .push(opcao(format!("Escolhidas ({escolhidas})"), !todas, com(mm.clone(), false)));
                let valido = margens_pt(&mm).is_some();
                if !valido { corpo = corpo.push(dlg::aviso("alert-triangle", "Use números de 0 a 200 (vírgula ou ponto).")); }
                corpo = corpo.push(text("✓ = a página já tem essa caixa; escolher mostra as margens dela. Zero em tudo tira a caixa.").size(12).color(muted))
                    .push(botoes(if caixa == CaixaPagina::Corte { "Cortar".into() } else { "Definir".into() }, valido.then_some(Msg::Pag(Pag::AplicarMargens))));
                ("Caixas da página", corpo)
            }
            Dialogos::Bates { prefixo, primeiro, digitos, posicao } => {
                let com = |prefixo: String, primeiro: String, digitos, posicao| Msg::Pag(Pag::Mudar(Dialogos::Bates { prefixo, primeiro, digitos, posicao }));
                let (p1, n1) = (prefixo.clone(), primeiro.clone());
                let n = primeiro.trim().parse::<u32>().ok();
                let mut algarismos = row![].spacing(4);
                for d in [4, 6, 8] {
                    algarismos = algarismos.push(button(text(format!("{d}")).size(13).center()).width(Length::FILL).padding([5, 4])
                        .style(tema::estilo_botao_pequeno(d == digitos)).on_press(com(prefixo.clone(), primeiro.clone(), d, posicao)));
                }
                let mut posicoes = column![].spacing(4);
                for (p, r) in [(PosicaoNumero::RodapeDireita, "Rodapé à direita"), (PosicaoNumero::RodapeEsquerda, "Rodapé à esquerda"),
                    (PosicaoNumero::RodapeCentro, "Rodapé ao centro"), (PosicaoNumero::TopoDireita, "Topo à direita")] {
                    posicoes = posicoes.push(opcao(r.into(), p == posicao, com(prefixo.clone(), primeiro.clone(), digitos, p)));
                }
                let mut corpo = column![
                    text("Um número único em todas as páginas, para citar cada folha num processo ou num lote de documentos.").size(12.5),
                    row![text("Prefixo").size(13).width(Length::FILL), text_input("PROC-", prefixo.clone()).on_input(move |t| com(t, n1.clone(), digitos, posicao)).width(120).padding([4, 8]).size(13)].align_y(Align::Center),
                    row![text("Primeiro número").size(13).width(Length::FILL), text_input("1", primeiro.clone()).on_input(move |t| com(p1.clone(), t, digitos, posicao)).width(120).padding([4, 8]).size(13)].align_y(Align::Center),
                    dlg::secao("Algarismos"), algarismos,
                    dlg::secao("Posição"), posicoes,
                ].spacing(10);
                match n {
                    Some(n) => corpo = corpo.push(container(text(format!("{prefixo}{n:0digitos$} … {prefixo}{:0digitos$}", n + total - 1)).size(13)).padding([8, 12]).style(dlg::estilo_bloco)),
                    None => corpo = corpo.push(dlg::aviso("alert-triangle", "O primeiro número precisa ser um número inteiro.")),
                }
                corpo = corpo.push(text("Para o próximo arquivo do lote, comece no número que aparece no fim. Ctrl+Z desfaz.").size(12).color(muted))
                    .push(botoes("Numerar".into(), n.map(|_| Msg::Pag(Pag::AplicarBates))));
                ("Numeração Bates", corpo)
            }
            Dialogos::Rotulos { de, ate, estilo, prefixo, primeiro } => {
                let com = |de: String, ate: String, estilo, prefixo: String, primeiro: String|
                    Msg::Pag(Pag::Mudar(Dialogos::Rotulos { de, ate, estilo, prefixo, primeiro }));
                let mut estilos = column![].spacing(4);
                for (e, r) in [(EstiloRotulo::Decimal, "1, 2, 3"), (EstiloRotulo::RomanoMinusculo, "i, ii, iii"), (EstiloRotulo::RomanoMaiusculo, "I, II, III"),
                    (EstiloRotulo::LetraMinuscula, "a, b, c"), (EstiloRotulo::LetraMaiuscula, "A, B, C"), (EstiloRotulo::Nenhum, "Só o prefixo")] {
                    estilos = estilos.push(opcao(r.into(), e == estilo, com(de.clone(), ate.clone(), e, prefixo.clone(), primeiro.clone())));
                }
                let (n_de, n_ate, n_primeiro) = (de.trim().parse::<u32>().ok(), ate.trim().parse::<u32>().ok(), primeiro.trim().parse::<u32>().ok());
                let valido = matches!((n_de, n_ate, n_primeiro), (Some(a), Some(b), Some(p)) if a >= 1 && a <= b && b <= total && p >= 1)
                    && !(estilo == EstiloRotulo::Nenhum && prefixo.is_empty());
                let s1 = (ate.clone(), prefixo.clone(), primeiro.clone());
                let s2 = (de.clone(), prefixo.clone(), primeiro.clone());
                let s3 = (de.clone(), ate.clone(), primeiro.clone());
                let s4 = (de.clone(), ate.clone(), prefixo.clone());
                let mut corpo = column![
                    text("O nome que o leitor mostra para cada página (capa sem número, prefácio em i, ii…), sem escrever nada na folha.").size(12.5),
                    dlg::secao("Páginas"),
                    campo("Da página", &de, Box::new(move |t| com(t, s1.0.clone(), estilo, s1.1.clone(), s1.2.clone()))),
                    campo("Até a página", &ate, Box::new(move |t| com(s2.0.clone(), t, estilo, s2.1.clone(), s2.2.clone()))),
                    dlg::secao("Estilo"), estilos,
                    row![text("Prefixo").size(13).width(Length::FILL), text_input("Ex.: A-", prefixo.clone())
                        .on_input(move |t| com(s3.0.clone(), s3.1.clone(), estilo, t, s3.2.clone())).width(120).padding([4, 8]).size(13)].align_y(Align::Center),
                    campo("Primeiro número", &primeiro, Box::new(move |t| com(s4.0.clone(), s4.1.clone(), estilo, s4.2.clone(), t))),
                ].spacing(10);
                match (valido, n_primeiro) {
                    (true, Some(p)) => {
                        let exemplo: Vec<String> = (0..3).map(|i| format!("{prefixo}{}", estilo.escrever(p.saturating_add(i)))).collect();
                        corpo = corpo.push(container(text(format!("{} …", exemplo.join(", "))).size(13)).padding([8, 12]).style(dlg::estilo_bloco));
                    }
                    _ => corpo = corpo.push(dlg::aviso("alert-triangle", format!("Use páginas de 1 a {total} (a primeira antes da última) e número a partir de 1; sem estilo, escreva um prefixo."))),
                }
                corpo = corpo.push(text("As páginas depois do trecho continuam com o nome que tinham. Ctrl+Z desfaz.").size(12).color(muted))
                    .push(row![
                        dlg::botao_secundario("Tirar numeração", Some(Msg::Pag(Pag::TirarRotulos))),
                        Space::with_width(Length::FILL),
                        dlg::botao_primario("Numerar", valido.then_some(Msg::Pag(Pag::AplicarRotulos))),
                    ].spacing(8));
                ("Numeração lógica", corpo)
            }
            Dialogos::Colunas { colunas, todas } => {
                let com = move |colunas, todas| Msg::Pag(Pag::Mudar(Dialogos::Colunas { colunas, todas }));
                let mut quantas = row![].spacing(4);
                for c in [2, 3] {
                    quantas = quantas.push(button(text(format!("{c} colunas")).size(13).center()).width(Length::FILL).padding([5, 4])
                        .style(tema::estilo_botao_pequeno(c == colunas)).on_press(com(c, todas)));
                }
                let escolhidas = self.alvo().len();
                let corpo = column![
                    text("Artigo em colunas vira páginas seguidas, uma coluna por página: bom para ler no celular ou no leitor de e-book.").size(12.5),
                    dlg::secao("Colunas"), quantas,
                    dlg::secao("Em quais páginas"),
                    opcao(format!("Todas ({total})"), todas, com(colunas, true)),
                    opcao(format!("Escolhidas ({escolhidas})"), !todas, com(colunas, false)),
                    text("O corte vai no vão entre as colunas de texto; página com figura atravessando o corte fica inteira. Ctrl+Z desfaz.").size(12).color(muted),
                    botoes("Dividir colunas".into(), Some(Msg::Pag(Pag::AplicarColunas))),
                ].spacing(10);
                ("Dividir colunas", corpo)
            }
        };
        (titulo, scrollable(corpo.padding([0, 4])).height(Length::FILL).into())
    }

    /// "11 × A4, 1 × A3 paisagem".
    fn mistura_de_tamanhos(&self) -> String {
        let Some(a) = &self.aberto else { return String::new() };
        let mut contagem: Vec<(String, u32)> = Vec::new();
        for &(l, h) in &a.tamanhos {
            let mm = |pt: f32| (pt * 25.4 / 72.0).round() as i32;
            let (x, y) = (mm(l).min(mm(h)), mm(l).max(mm(h)));
            let nome = match (x, y) {
                (210, 297) => "A4".into(), (216, 279) => "Carta".into(), (297, 420) => "A3".into(),
                _ => format!("{} × {} mm", mm(l), mm(h)),
            };
            let nome = if l > h * 1.05 && !nome.contains("mm") { format!("{nome} paisagem") } else { nome };
            match contagem.iter_mut().find(|(n, _)| *n == nome) {
                Some((_, c)) => *c += 1,
                None => contagem.push((nome, 1)),
            }
        }
        contagem.iter().map(|(n, c)| format!("{c} × {n}")).collect::<Vec<_>>().join(", ")
    }
}

/// "2,5" ou "10" MB; `None` se não for número positivo.
fn megabytes(t: &str) -> Option<f64> {
    t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| *v > 0.0 && v.is_finite())
}

/// Os quatro campos de margem em pontos, ou `None` se algum não for número de 0 a 200 mm.
fn margens_pt(mm: &[String; 4]) -> Option<[f32; 4]> {
    let mut v = [0.0; 4];
    for (i, t) in mm.iter().enumerate() {
        let t = t.trim().replace(',', ".");
        let n: f32 = if t.is_empty() { 0.0 } else { t.parse().ok()? };
        if !(0.0..=200.0).contains(&n) { return None; }
        v[i] = n * 72.0 / 25.4;
    }
    Some(v)
}
