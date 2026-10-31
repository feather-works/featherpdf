//! Objetos de uma página: linhas de texto e imagens, com a caixa de cada um,
//! para escolher, mover, apagar e trocar o texto.
//!
//! Uma "linha" é o trecho de um bloco de texto (BT…ET) que começa num
//! posicionamento (BT, Td, TD, Tm, T*) e vai até o próximo: é como os
//! geradores escrevem uma linha ou um pedaço dela. Mover uma linha põe um
//! `Td` depois do posicionamento e o `Td` contrário depois da última escrita,
//! então as linhas seguintes não andam junto.
//! Texto dentro de Form XObject não é aberto: o grupo inteiro é um objeto.

use std::collections::HashMap;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId, Stream};

use super::DocumentoPdf;

const MAX_CONTEUDO: usize = 64 * 1024 * 1024;

/// Matriz PDF [a b c d e f], vetor-linha: ponto × matriz.
pub(crate) type Matriz = [f32; 6];
const IDENTIDADE: Matriz = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

pub(crate) fn mult(m: Matriz, n: Matriz) -> Matriz {
    [
        m[0] * n[0] + m[1] * n[2], m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2], m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4], m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

pub(crate) fn inversa(m: Matriz) -> Option<Matriz> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-9 { return None; }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)])
}

pub(crate) fn aplicar(m: Matriz, x: f32, y: f32) -> (f32, f32) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

/// Caixa (x0, y0, x1, y1) que contém o retângulo transformado.
fn caixa_de(m: Matriz, x0: f32, y0: f32, x1: f32, y1: f32) -> [f32; 4] {
    let pts = [aplicar(m, x0, y0), aplicar(m, x1, y0), aplicar(m, x0, y1), aplicar(m, x1, y1)];
    let mut c = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in pts {
        c = [c[0].min(x), c[1].min(y), c[2].max(x), c[3].max(y)];
    }
    c
}

pub(crate) fn unir(a: Option<[f32; 4]>, b: [f32; 4]) -> [f32; 4] {
    match a { Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])], None => b }
}

fn numero(o: &Object) -> f32 { o.as_float().unwrap_or(0.0) }

#[derive(Debug, Clone, PartialEq)]
pub enum TipoObjeto {
    Texto { texto: String, fonte: String, tamanho: f32 },
    Imagem { largura: u32, altura: u32 },
    /// Form XObject: desenho agrupado (logotipo, gráfico, carimbo).
    Grupo,
    /// Caminho vetorial solto: borda de tabela, linha, retângulo, curva.
    Desenho,
}

/// O que mudar num desenho: `None` deixa como está; `Some(None)` tira o
/// contorno ou o preenchimento.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EstiloDesenho {
    pub contorno: Option<Option<[f32; 3]>>,
    pub preenchimento: Option<Option<[f32; 3]>>,
    pub espessura: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjetoPdf {
    pub tipo: TipoObjeto,
    /// Caixa no espaço da página (pontos, sem o /Rotate): x0, y0, x1, y1.
    pub caixa: [f32; 4],
}

/// Uma letra (ou um recuo do TJ) de uma escrita, para a tarja apagar só o
/// que está debaixo dela.
#[derive(Debug, Clone)]
pub(crate) enum ItemTj {
    /// `k`: o recuo do TJ que ocupa o mesmo espaço da letra.
    Glifo { bytes: Vec<u8>, texto: String, caixa: [f32; 4], k: f32 },
    Recuo(Object),
}

/// Onde cada objeto está na lista de operações (já normalizada).
#[derive(Debug, Clone)]
pub(crate) struct Lugar {
    /// Texto: ponto logo depois do posicionamento; imagem: a própria operação `Do`.
    pub(crate) insercao: usize,
    /// Texto: logo depois da última escrita. Imagem: `insercao + 1`.
    pub(crate) fim: usize,
    /// Operações que escrevem texto (Tj/TJ) desta linha.
    pub(crate) escritas: Vec<usize>,
    /// Matriz da linha (texto: Tlm × CTM) ou CTM (imagem) no começo do objeto.
    pub(crate) matriz: Matriz,
    /// Nome do recurso da fonte na primeira escrita.
    fonte: Vec<u8>,
}

/// Larguras e codificação de uma fonte, para medir e escrever texto.
struct Fonte<'a> {
    codificacao: Option<Encoding<'a>>,
    dois_bytes: bool,
    larguras: HashMap<u32, f32>,
    padrao: f32,
    nome: String,
}

/// Cópia da fonte Type0 sem o /Encoding, para a codificação sair do /ToUnicode.
/// O lopdf não lê CMap de codificação embutido (o Word grava assim) e cai num
/// código de 1 byte que não decifra nada; com o /ToUnicode ele acerta.
fn copia_para_codificar(dict: &Dictionary) -> Option<Dictionary> {
    let tipo0 = dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|s| s == b"Type0");
    if !tipo0 || !dict.has(b"ToUnicode") { return None; }
    let mut c = dict.clone();
    c.remove(b"Encoding");
    Some(c)
}

impl<'a> Fonte<'a> {
    /// `cod` é o próprio `dict` ou a cópia de [`copia_para_codificar`].
    fn ler(doc: &'a Document, dict: &'a Dictionary, cod: &'a Dictionary) -> Self {
        let resolver = |o: &'a Object| -> Option<&'a Object> {
            match o { Object::Reference(r) => doc.get_object(*r).ok(), o => Some(o) }
        };
        let nome = dict.get(b"BaseFont").and_then(Object::as_name).map(|n| {
            let n = String::from_utf8_lossy(n).into_owned();
            // Subconjunto: "ABCDEF+Calibri" vira "Calibri".
            match n.split_once('+') { Some((p, r)) if p.len() == 6 => r.to_string(), _ => n }
        }).unwrap_or_else(|_| "?".into());
        let tipo0 = dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|s| s == b"Type0");
        let mut larguras = HashMap::new();
        let mut padrao = 500.0;
        if tipo0 {
            padrao = 1000.0;
            let desc = dict.get(b"DescendantFonts").ok().and_then(resolver)
                .and_then(|o| o.as_array().ok()).and_then(|v| v.first()).and_then(resolver)
                .and_then(|o| o.as_dict().ok());
            if let Some(desc) = desc {
                if let Ok(dw) = desc.get(b"DW") { padrao = numero(dw); }
                if let Some(w) = desc.get(b"W").ok().and_then(resolver).and_then(|o| o.as_array().ok()) {
                    let mut i = 0;
                    while i + 1 < w.len() {
                        let c = w[i].as_i64().unwrap_or(0) as u32;
                        match resolver(&w[i + 1]) {
                            Some(Object::Array(lista)) => {
                                for (k, v) in lista.iter().enumerate() { larguras.insert(c + k as u32, numero(v)); }
                                i += 2;
                            }
                            _ if i + 2 < w.len() => {
                                let fim = w[i + 1].as_i64().unwrap_or(0) as u32;
                                let v = numero(&w[i + 2]);
                                for k in c..=fim.min(c + 65_535) { larguras.insert(k, v); }
                                i += 3;
                            }
                            _ => break,
                        }
                    }
                }
            }
        } else {
            let primeiro = dict.get(b"FirstChar").and_then(Object::as_i64).unwrap_or(0) as u32;
            if let Some(w) = dict.get(b"Widths").ok().and_then(resolver).and_then(|o| o.as_array().ok()) {
                for (k, v) in w.iter().enumerate() { larguras.insert(primeiro + k as u32, numero(v)); }
            }
            let falta = dict.get(b"FontDescriptor").ok().and_then(resolver).and_then(|o| o.as_dict().ok())
                .and_then(|d| d.get(b"MissingWidth").ok()).map(numero);
            if let Some(m) = falta { padrao = m; }
            // Type3 mede no espaço do glifo; a /FontMatrix leva aos milésimos que as outras usam.
            let tipo3 = dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|s| s == b"Type3");
            let matriz = dict.get(b"FontMatrix").ok().and_then(resolver).and_then(|o| o.as_array().ok()).and_then(|m| m.first()).map(numero);
            if let (true, Some(a)) = (tipo3, matriz.filter(|a| a.abs() > f32::EPSILON)) {
                larguras.values_mut().for_each(|w| *w *= a * 1000.0);
                if falta.is_some() { padrao *= a * 1000.0; }
            }
        }
        Fonte { codificacao: cod.get_font_encoding(doc).ok(), dois_bytes: tipo0, larguras, padrao, nome }
    }

    fn codigos<'b>(&self, bytes: &'b [u8]) -> impl Iterator<Item = u32> + 'b {
        let passo = if self.dois_bytes { 2 } else { 1 };
        bytes.chunks(passo).map(|c| c.iter().fold(0u32, |a, &b| a << 8 | b as u32))
    }

    fn texto(&self, bytes: &[u8]) -> String {
        self.codificacao.as_ref().and_then(|e| Document::decode_text(e, bytes).ok())
            .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Estado gráfico que importa para achar onde as coisas estão.
#[derive(Clone)]
struct Estado {
    ctm: Matriz,
    fonte: Vec<u8>,
    tamanho: f32,
    tc: f32,
    tw: f32,
    th: f32,
    tl: f32,
    subida: f32,
    /// Espessura do traço (`w`), para a caixa da linha não ter altura zero.
    traco: f32,
}

pub(crate) struct Analise {
    pub(crate) ops: Vec<Operation>,
    pub(crate) objetos: Vec<ObjetoPdf>,
    pub(crate) lugares: Vec<Lugar>,
    /// Letras de cada Tj/TJ, pelo índice da operação (só com `letras` ligado).
    pub(crate) letras: HashMap<usize, Vec<ItemTj>>,
    /// Traços retos (horizontais e verticais) pintados na página: bordas de tabela.
    pub(crate) segmentos: Vec<[f32; 4]>,
}

/// `'` e `"` viram `T*` + `Tj` (e `Tw`/`Tc`): assim posicionar e escrever ficam separados.
fn normalizar(ops: Vec<Operation>) -> Vec<Operation> {
    let mut saida = Vec::with_capacity(ops.len());
    for op in ops {
        match op.operator.as_str() {
            "'" => {
                saida.push(Operation::new("T*", vec![]));
                saida.push(Operation::new("Tj", op.operands));
            }
            "\"" if op.operands.len() == 3 => {
                let mut o = op.operands.into_iter();
                let (aw, ac, s) = (o.next().unwrap(), o.next().unwrap(), o.next().unwrap());
                saida.push(Operation::new("Tw", vec![aw]));
                saida.push(Operation::new("Tc", vec![ac]));
                saida.push(Operation::new("T*", vec![]));
                saida.push(Operation::new("Tj", vec![s]));
            }
            _ => saida.push(op),
        }
    }
    saida
}

/// Operação que pinta algo na página (o `n` do recorte não conta).
fn pinta(op: &str) -> bool {
    matches!(op, "Tj" | "TJ" | "'" | "\"" | "Do" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s" | "sh" | "BI")
}

/// O bloco `q … Q` com recorte (`W`) que envolve só o objeto em `inicio..fim`:
/// devolve (o `q`, logo depois do `Q`). `None` sem recorte ou com outra coisa pintada no bloco.
pub(crate) fn bloco_do_recorte(ops: &[Operation], inicio: usize, fim: usize) -> Option<(usize, usize)> {
    let mut nivel = 0;
    let mut recorte = false;
    for j in (0..inicio).rev() {
        match ops[j].operator.as_str() {
            "Q" => nivel += 1,
            "q" if nivel > 0 => nivel -= 1,
            "q" if recorte => {
                // Fecha o bloco e confere que depois do objeto não se pinta mais nada nele.
                let mut n = 1;
                for (k, op) in ops.iter().enumerate().skip(fim) {
                    match op.operator.as_str() {
                        "q" => n += 1,
                        "Q" => { n -= 1; if n == 0 { return Some((j, k + 1)); } }
                        o if pinta(o) => return None,
                        _ => {}
                    }
                }
                return None;
            }
            "q" => {}
            "W" | "W*" => recorte = true,
            o if pinta(o) => return None,
            _ => {}
        }
    }
    None
}

/// Matriz de transformação em vigor antes da operação `ate`.
pub(crate) fn ctm_em(ops: &[Operation], ate: usize) -> Matriz {
    let (mut ctm, mut pilha) = (IDENTIDADE, Vec::new());
    for op in &ops[..ate] {
        let n: Vec<f32> = op.operands.iter().map(numero).collect();
        match op.operator.as_str() {
            "q" => pilha.push(ctm),
            "Q" => ctm = pilha.pop().unwrap_or(ctm),
            "cm" if n.len() == 6 => ctm = mult([n[0], n[1], n[2], n[3], n[4], n[5]], ctm),
            _ => {}
        }
    }
    ctm
}

/// Codifica as operações de volta. Imagem embutida (BI…ID…EI) não passa pelo
/// `Content::encode`, que a escreveria como objeto de fluxo.
pub(crate) fn codificar(ops: &[Operation]) -> Result<Vec<u8>, String> {
    let mut saida = Vec::new();
    let mut trecho: Vec<Operation> = Vec::new();
    let despejar = |trecho: &mut Vec<Operation>, saida: &mut Vec<u8>| -> Result<(), String> {
        if trecho.is_empty() { return Ok(()); }
        saida.extend(Content { operations: std::mem::take(trecho) }.encode().map_err(|e| e.to_string())?);
        saida.push(b'\n');
        Ok(())
    };
    for op in ops {
        if op.operator != "BI" { trecho.push(op.clone()); continue; }
        despejar(&mut trecho, &mut saida)?;
        // O lopdf lê imagem embutida com filtro ou cor abreviada como vazia: regravar a apagaria.
        let Some(Object::Stream(s)) = op.operands.first() else {
            return Err("Esta página tem uma imagem embutida no conteúdo que ainda não sei regravar; \
                editar a página a apagaria.".into());
        };
        saida.extend_from_slice(b"BI\n");
        for (chave, valor) in s.dict.iter() {
            // Um operador vazio faz o `encode` escrever só os operandos.
            let par = Content { operations: vec![Operation::new("", vec![Object::Name(chave.clone()), valor.clone()])] };
            saida.extend(par.encode().map_err(|e| e.to_string())?);
            saida.push(b'\n');
        }
        saida.extend_from_slice(b"ID ");
        saida.extend_from_slice(&s.content);
        saida.extend_from_slice(b"\nEI\n");
    }
    despejar(&mut trecho, &mut saida)?;
    Ok(saida)
}

/// Reta horizontal ou vertical (ordenada), ou nada se for inclinada ou um ponto.
fn reta_de_borda([x0, y0, x1, y1]: [f32; 4]) -> Option<[f32; 4]> {
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    ((dx < 0.5 && dy > 1.0) || (dy < 0.5 && dx > 1.0)).then(|| [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
}

/// O Word e outros geradores quebram uma linha em pedaços ("Terri" + "torial").
/// Pedaços seguidos, na mesma base, com a mesma fonte e só `Td`/`T*` entre eles
/// viram um objeto. `Tm` no meio não junta: ele reposicionaria sem levar o
/// deslocamento de quem foi movido.
fn juntar_pedacos(ops: &[Operation], objetos: Vec<ObjetoPdf>, lugares: Vec<Lugar>) -> (Vec<ObjetoPdf>, Vec<Lugar>) {
    let mut obj_saida: Vec<ObjetoPdf> = Vec::with_capacity(objetos.len());
    let mut lug_saida: Vec<Lugar> = Vec::with_capacity(lugares.len());
    for (o, l) in objetos.into_iter().zip(lugares) {
        if let (Some(po), Some(pl)) = (obj_saida.last_mut(), lug_saida.last_mut()) {
            if let (TipoObjeto::Texto { texto: ta, fonte: fa, tamanho: sa }, TipoObjeto::Texto { texto: tb, fonte: fb, tamanho: sb }) = (&mut po.tipo, &o.tipo) {
                let so_relativo = !pl.escritas.is_empty() && !l.escritas.is_empty()
                    && ops[pl.fim..l.escritas[0]].iter().all(|op| matches!(op.operator.as_str(),
                        "Td" | "TD" | "T*" | "Tc" | "Tw" | "Tz" | "TL" | "Ts" | "Tr" | "Tf"));
                let mesma_base = (po.caixa[1] - o.caixa[1]).abs() < 0.5 && (po.caixa[3] - o.caixa[3]).abs() < 0.5;
                let vao = o.caixa[0] - po.caixa[2];
                if so_relativo && pl.fonte == l.fonte && fa == fb && (*sa - sb).abs() < 0.01 && mesma_base
                    && vao > -0.5 * *sa && vao < 0.6 * *sa
                {
                    // Palavras escritas em pedaços separados: o vão vira espaço no texto.
                    if vao > 0.15 * *sa && !ta.ends_with(' ') && !tb.starts_with(' ') { ta.push(' '); }
                    ta.push_str(tb);
                    po.caixa = unir(Some(po.caixa), o.caixa);
                    pl.escritas.extend(l.escritas);
                    pl.fim = l.fim;
                    continue;
                }
            }
        }
        obj_saida.push(o);
        lug_saida.push(l);
    }
    (obj_saida, lug_saida)
}

/// Posições (em caracteres) de `busca` em `texto`, sem diferenciar maiúscula.
pub fn achar_sem_caixa(texto: &str, busca: &str) -> Vec<usize> {
    let minusc = |s: &str| -> Vec<char> { s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect() };
    let (t, b) = (minusc(texto), minusc(busca));
    if b.is_empty() || b.len() > t.len() { return vec![]; }
    let mut achados = Vec::new();
    let mut i = 0;
    while i + b.len() <= t.len() {
        if t[i..i + b.len()] == b[..] { achados.push(i); i += b.len(); } else { i += 1; }
    }
    achados
}

pub(crate) fn trocar_sem_caixa(texto: &str, busca: &str, novo: &str) -> (String, usize) {
    let achados = achar_sem_caixa(texto, busca);
    let chars: Vec<char> = texto.chars().collect();
    let n = busca.chars().count();
    let mut saida = String::new();
    let mut i = 0;
    for &p in &achados {
        saida.extend(&chars[i..p]);
        saida.push_str(novo);
        i = p + n;
    }
    saida.extend(&chars[i..]);
    (saida, achados.len())
}

impl DocumentoPdf {
    /// Troca `busca` por `novo` nas linhas `alvos` (página, objeto), sem diferenciar
    /// maiúscula. Devolve quantas trocas fez. Letra que a fonte não tem para tudo.
    pub fn trocar_texto(&mut self, alvos: &[(u32, usize)], busca: &str, novo: &str) -> Result<usize, String> {
        if busca.is_empty() { return Err("Escreva o que procurar.".into()); }
        let mut total = 0;
        // De trás para a frente em cada página: uma troca pode juntar pedaços vizinhos e
        // mudar a numeração dos objetos seguintes, nunca a dos anteriores.
        let mut alvos = alvos.to_vec();
        alvos.sort_unstable_by_key(|&(p, o)| (p, std::cmp::Reverse(o)));
        alvos.dedup();
        for &(pagina, objeto) in &alvos {
            let lista = self.objetos(pagina)?;
            let Some(TipoObjeto::Texto { texto, .. }) = lista.get(objeto).map(|o| &o.tipo) else { continue };
            let (trocado, n) = trocar_sem_caixa(texto, busca, novo);
            if n == 0 { continue; }
            self.editar_texto(pagina, objeto, &trocado).map_err(|e| format!("Página {pagina}: {e}"))?;
            total += n;
        }
        Ok(total)
    }

    /// Fontes e XObjects da página, com herança.
    pub(crate) fn recursos(&self, id: ObjectId) -> (HashMap<Vec<u8>, &Dictionary>, HashMap<Vec<u8>, ObjectId>) {
        let fontes = self.documento.get_page_fonts(id).unwrap_or_default().into_iter().collect();
        let mut xobjetos = HashMap::new();
        if let Ok((direto, herdados)) = self.documento.get_page_resources(id) {
            let mut dicts: Vec<&Dictionary> = direto.into_iter().collect();
            dicts.extend(herdados.iter().filter_map(|r| self.documento.get_dictionary(*r).ok()));
            for d in dicts {
                let x = match d.get(b"XObject") {
                    Ok(Object::Reference(r)) => self.documento.get_dictionary(*r).ok(),
                    Ok(Object::Dictionary(x)) => Some(x),
                    _ => None,
                };
                for (nome, valor) in x.into_iter().flat_map(|x| x.iter()) {
                    if let Ok(r) = valor.as_reference() { xobjetos.entry(nome.clone()).or_insert(r); }
                }
            }
        }
        (fontes, xobjetos)
    }

    pub(crate) fn analisar(&self, pagina: u32) -> Result<Analise, String> { self.analisar_com(pagina, false) }

    pub(crate) fn analisar_com(&self, pagina: u32, com_letras: bool) -> Result<Analise, String> {
        let id = self.id_pagina(pagina)?;
        let bytes = self.documento.get_page_content_with_limit(id, MAX_CONTEUDO)
            .map_err(|e| format!("Não foi possível ler o conteúdo da página: {e}"))?;
        let ops = normalizar(Content::decode(&bytes)
            .map_err(|e| format!("O conteúdo da página tem um trecho que não sei ler: {e}"))?.operations);
        let (dicts_fontes, xobjetos) = self.recursos(id);
        let copias: HashMap<Vec<u8>, Dictionary> = dicts_fontes.iter()
            .filter_map(|(n, d)| copia_para_codificar(d).map(|c| (n.clone(), c))).collect();
        let mut fontes: HashMap<Vec<u8>, Fonte> = HashMap::new();

        let mut objetos = Vec::new();
        let mut lugares = Vec::new();
        let mut pilha: Vec<Estado> = Vec::new();
        let mut e = Estado { ctm: IDENTIDADE, fonte: Vec::new(), tamanho: 0.0, tc: 0.0, tw: 0.0, th: 1.0, tl: 0.0, subida: 0.0, traco: 1.0 };
        let (mut tm, mut tlm) = (IDENTIDADE, IDENTIDADE);
        // Linha aberta: lugar, texto e caixa até agora.
        let mut linha: Option<(Lugar, String, Option<[f32; 4]>, f32)> = None;
        let mut no_bloco = false;
        // Caminho em construção: primeira operação e caixa até agora.
        let mut caminho: Option<(usize, Option<[f32; 4]>)> = None;
        // Do caminho aberto: ponto atual, começo do trecho, retas (valem se riscadas)
        // e retângulos finos (bordas feitas de retângulo preenchido).
        let (mut atual, mut inicio_sub) = ((0.0, 0.0), (0.0, 0.0));
        let (mut retas, mut finos): (Vec<[f32; 4]>, Vec<[f32; 4]>) = (Vec::new(), Vec::new());
        let mut segmentos: Vec<[f32; 4]> = Vec::new();
        let mut letras: HashMap<usize, Vec<ItemTj>> = HashMap::new();

        let fechar = |linha: &mut Option<(Lugar, String, Option<[f32; 4]>, f32)>, objetos: &mut Vec<ObjetoPdf>,
                      lugares: &mut Vec<Lugar>, fontes: &HashMap<Vec<u8>, Fonte>| {
            if let Some((lugar, texto, Some(caixa), tamanho)) = linha.take() {
                if lugar.escritas.is_empty() { return; }
                let fonte = fontes.get(&lugar.fonte).map(|f| f.nome.clone()).unwrap_or_default();
                objetos.push(ObjetoPdf { tipo: TipoObjeto::Texto { texto, fonte, tamanho }, caixa });
                lugares.push(lugar);
            }
        };

        for (i, op) in ops.iter().enumerate() {
            let a = &op.operands;
            let n = |k: usize| a.get(k).map(numero).unwrap_or(0.0);
            let posiciona = matches!(op.operator.as_str(), "Td" | "TD" | "Tm" | "T*");
            if posiciona || op.operator == "BT" {
                // Posicionamento depois de escrever fecha a linha; vários seguidos são um só começo.
                if linha.as_ref().is_some_and(|l| !l.0.escritas.is_empty()) {
                    fechar(&mut linha, &mut objetos, &mut lugares, &fontes);
                }
            }
            match op.operator.as_str() {
                "q" => pilha.push(e.clone()),
                "Q" => if let Some(anterior) = pilha.pop() { e = anterior },
                "cm" if a.len() == 6 => e.ctm = mult([n(0), n(1), n(2), n(3), n(4), n(5)], e.ctm),
                "BT" => { tm = IDENTIDADE; tlm = IDENTIDADE; no_bloco = true; }
                "ET" => { fechar(&mut linha, &mut objetos, &mut lugares, &fontes); no_bloco = false; }
                "Td" => { tlm = mult([1.0, 0.0, 0.0, 1.0, n(0), n(1)], tlm); tm = tlm; }
                "TD" => { e.tl = -n(1); tlm = mult([1.0, 0.0, 0.0, 1.0, n(0), n(1)], tlm); tm = tlm; }
                "Tm" if a.len() == 6 => { tlm = [n(0), n(1), n(2), n(3), n(4), n(5)]; tm = tlm; }
                "T*" => { tlm = mult([1.0, 0.0, 0.0, 1.0, 0.0, -e.tl], tlm); tm = tlm; }
                "Tf" => {
                    e.fonte = a.first().and_then(|o| o.as_name().ok()).map(<[u8]>::to_vec).unwrap_or_default();
                    e.tamanho = n(1);
                    if !fontes.contains_key(&e.fonte) {
                        // Fonte que falta nos recursos: o leitor desenha com uma substituta. Sem medida,
                        // o texto sumiria da busca e escaparia da tarja; vai com a largura média.
                        let f = match dicts_fontes.get(&e.fonte) {
                            Some(d) => Fonte::ler(&self.documento, d, copias.get(&e.fonte).unwrap_or(d)),
                            None => Fonte { codificacao: None, dois_bytes: false, larguras: HashMap::new(), padrao: 500.0, nome: "?".into() },
                        };
                        fontes.insert(e.fonte.clone(), f);
                    }
                }
                "w" => e.traco = n(0),
                "m" | "l" | "c" | "v" | "y" | "re" | "h" if !no_bloco => {
                    let c = caminho.get_or_insert((i, None));
                    let pontos: Vec<(f32, f32)> = if op.operator == "re" {
                        let (x, y, w, h) = (n(0), n(1), n(2), n(3));
                        vec![(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
                    } else {
                        a.chunks(2).filter(|p| p.len() == 2).map(|p| (numero(&p[0]), numero(&p[1]))).collect()
                    };
                    let na_pagina: Vec<(f32, f32)> = pontos.iter().map(|&(x, y)| aplicar(e.ctm, x, y)).collect();
                    for &(px, py) in &na_pagina {
                        c.1 = Some(unir(c.1, [px, py, px, py]));
                    }
                    let ultimo = na_pagina.last().copied().unwrap_or(atual);
                    match op.operator.as_str() {
                        "m" => { atual = ultimo; inicio_sub = ultimo; }
                        "l" => { retas.push([atual.0, atual.1, ultimo.0, ultimo.1]); atual = ultimo; }
                        "h" => { retas.push([atual.0, atual.1, inicio_sub.0, inicio_sub.1]); atual = inicio_sub; }
                        "re" => {
                            let [a0, a1, a2, a3] = [na_pagina[0], na_pagina[1], na_pagina[3], na_pagina[2]];
                            for (p, q) in [(a0, a1), (a1, a2), (a2, a3), (a3, a0)] { retas.push([p.0, p.1, q.0, q.1]); }
                            let r = unir(Some([a0.0, a0.1, a0.0, a0.1]), [a2.0, a2.1, a2.0, a2.1]);
                            let (l, a) = (r[2] - r[0], r[3] - r[1]);
                            if a < 2.5 && l > a { finos.push([r[0], (r[1] + r[3]) / 2.0, r[2], (r[1] + r[3]) / 2.0]); }
                            else if l < 2.5 && a > l { finos.push([(r[0] + r[2]) / 2.0, r[1], (r[0] + r[2]) / 2.0, r[3]]); }
                            atual = a0;
                            inicio_sub = a0;
                        }
                        _ => atual = ultimo,
                    }
                }
                "n" => { caminho = None; retas.clear(); finos.clear(); }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                    let riscado = !matches!(op.operator.as_str(), "f" | "F" | "f*");
                    let preenchido = !matches!(op.operator.as_str(), "S" | "s");
                    if riscado { segmentos.extend(retas.iter().filter_map(|&r| reta_de_borda(r))); }
                    if preenchido { segmentos.extend(finos.iter().filter_map(|&r| reta_de_borda(r))); }
                    retas.clear();
                    finos.clear();
                    if let Some((inicio, Some(c))) = caminho.take() {
                        // Meia espessura do traço em volta, na escala da página.
                        let meio = e.traco.max(0.5) / 2.0 * (e.ctm[0] * e.ctm[3] - e.ctm[1] * e.ctm[2]).abs().sqrt();
                        objetos.push(ObjetoPdf { tipo: TipoObjeto::Desenho, caixa: [c[0] - meio, c[1] - meio, c[2] + meio, c[3] + meio] });
                        lugares.push(Lugar { insercao: inicio, fim: i + 1, escritas: vec![], matriz: e.ctm, fonte: vec![] });
                    }
                }
                "Tc" => e.tc = n(0),
                "Tw" => e.tw = n(0),
                "Tz" => e.th = n(0) / 100.0,
                "TL" => e.tl = n(0),
                "Ts" => e.subida = n(0),
                "Tj" | "TJ" if no_bloco => {
                    let Some(fonte) = fontes.get(&e.fonte) else { continue };
                    let l = linha.get_or_insert_with(|| (Lugar {
                        insercao: i, fim: i, escritas: Vec::new(), matriz: mult(tlm, e.ctm), fonte: e.fonte.clone(),
                    }, String::new(), None, e.tamanho));
                    if l.0.escritas.is_empty() { l.0.fonte = e.fonte.clone(); l.3 = e.tamanho; }
                    l.0.escritas.push(i);
                    l.0.fim = i + 1;
                    let itens: Vec<&Object> = if op.operator == "TJ" {
                        a.first().and_then(|o| o.as_array().ok()).map(|v| v.iter().collect()).unwrap_or_default()
                    } else { a.iter().collect() };
                    let inicio = tm;
                    let m = mult(inicio, e.ctm);
                    let base = e.subida;
                    let mut avanco = 0.0;
                    let mut desta: Vec<ItemTj> = Vec::new();
                    for item in itens {
                        match item {
                            Object::String(bytes, _) => {
                                l.1.push_str(&fonte.texto(bytes));
                                let passo = if fonte.dois_bytes { 2 } else { 1 };
                                for (c, pedaco) in fonte.codigos(bytes).zip(bytes.chunks(passo)) {
                                    let w = fonte.larguras.get(&c).copied().unwrap_or(fonte.padrao);
                                    let espaco = if !fonte.dois_bytes && c == 32 { e.tw } else { 0.0 };
                                    let passo_x = (w / 1000.0 * e.tamanho + e.tc + espaco) * e.th;
                                    if com_letras {
                                        let escala = e.tamanho * e.th;
                                        desta.push(ItemTj::Glifo {
                                            bytes: pedaco.to_vec(), texto: fonte.texto(pedaco),
                                            caixa: caixa_de(m, avanco, base - 0.22 * e.tamanho, avanco + passo_x, base + 0.78 * e.tamanho),
                                            k: if escala.abs() > f32::EPSILON { -passo_x * 1000.0 / escala } else { 0.0 },
                                        });
                                    }
                                    avanco += passo_x;
                                }
                            }
                            num => {
                                if com_letras { desta.push(ItemTj::Recuo(num.clone())); }
                                let k = numero(num);
                                // Recuo grande no TJ costuma ser o espaço entre palavras.
                                if k < -180.0 && !l.1.ends_with(' ') { l.1.push(' '); }
                                avanco -= k / 1000.0 * e.tamanho * e.th;
                            }
                        }
                    }
                    if com_letras { letras.insert(i, desta); }
                    l.2 = Some(unir(l.2, caixa_de(m, 0.0, base - 0.22 * e.tamanho, avanco.max(0.5), base + 0.78 * e.tamanho)));
                    tm = mult([1.0, 0.0, 0.0, 1.0, avanco, 0.0], tm);
                }
                // Imagem embutida: o lopdf a lê como um `BI` só (o fluxo de operando, ou nada se não decifrou).
                "BI" => {
                    let dim = |k: &[u8], longo: &[u8]| match a.first() {
                        Some(Object::Stream(s)) => s.dict.get(k).or_else(|_| s.dict.get(longo)).and_then(Object::as_i64).unwrap_or(0) as u32,
                        _ => 0,
                    };
                    objetos.push(ObjetoPdf { tipo: TipoObjeto::Imagem { largura: dim(b"W", b"Width"), altura: dim(b"H", b"Height") }, caixa: caixa_de(e.ctm, 0.0, 0.0, 1.0, 1.0) });
                    lugares.push(Lugar { insercao: i, fim: i + 1, escritas: vec![], matriz: e.ctm, fonte: vec![] });
                }
                "Do" => {
                    let Some(xid) = a.first().and_then(|o| o.as_name().ok()).and_then(|n| xobjetos.get(n)) else { continue };
                    let Ok(Object::Stream(s)) = self.documento.get_object(*xid) else { continue };
                    let sub = s.dict.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"");
                    let (tipo, caixa) = if sub == b"Image" {
                        let dim = |k: &[u8]| s.dict.get(k).and_then(Object::as_i64).unwrap_or(0) as u32;
                        (TipoObjeto::Imagem { largura: dim(b"Width"), altura: dim(b"Height") },
                         caixa_de(e.ctm, 0.0, 0.0, 1.0, 1.0))
                    } else {
                        let v: Vec<f32> = s.dict.get(b"BBox").and_then(Object::as_array)
                            .map(|v| v.iter().map(numero).collect()).unwrap_or_default();
                        if v.len() != 4 { continue; }
                        let mat: Vec<f32> = s.dict.get(b"Matrix").and_then(Object::as_array)
                            .map(|v| v.iter().map(numero).collect()).unwrap_or_default();
                        let mat = if mat.len() == 6 { [mat[0], mat[1], mat[2], mat[3], mat[4], mat[5]] } else { IDENTIDADE };
                        (TipoObjeto::Grupo, caixa_de(mult(mat, e.ctm), v[0], v[1], v[2], v[3]))
                    };
                    objetos.push(ObjetoPdf { tipo, caixa });
                    lugares.push(Lugar { insercao: i, fim: i + 1, escritas: vec![], matriz: e.ctm, fonte: vec![] });
                }
                _ => {}
            }
            if (posiciona || op.operator == "BT") && no_bloco {
                // Começo (ou continuação do começo) de uma linha: o Td de mover entra depois daqui.
                match &mut linha {
                    Some(l) if l.0.escritas.is_empty() => { l.0.insercao = i + 1; l.0.matriz = mult(tlm, e.ctm); }
                    _ => linha = Some((Lugar { insercao: i + 1, fim: i + 1, escritas: vec![], matriz: mult(tlm, e.ctm), fonte: vec![] },
                        String::new(), None, e.tamanho)),
                }
            }
        }
        fechar(&mut linha, &mut objetos, &mut lugares, &fontes);
        let (objetos, lugares) = juntar_pedacos(&ops, objetos, lugares);
        Ok(Analise { ops, objetos, lugares, letras, segmentos })
    }

    /// Linhas de texto, imagens e grupos da página, na ordem em que são desenhados.
    pub fn objetos(&self, pagina: u32) -> Result<Vec<ObjetoPdf>, String> {
        Ok(self.analisar(pagina)?.objetos)
    }

    /// Traços horizontais e verticais da página (x0, y0, x1, y1, com x0 ≤ x1 e y0 ≤ y1).
    pub fn segmentos(&self, pagina: u32) -> Result<Vec<[f32; 4]>, String> {
        Ok(self.analisar(pagina)?.segmentos)
    }

    /// Caixa visível da página (x0, y0, x1, y1) e o giro, para pôr os objetos na tela.
    pub fn geometria(&self, pagina: u32) -> Result<([f32; 4], i64), String> {
        let id = self.id_pagina(pagina)?;
        let (x0, y0, x1, y1) = self.caixa(id);
        let giro = self.herdado(id, b"Rotate").and_then(|g| g.as_i64().ok()).unwrap_or(0);
        Ok(([x0, y0, x1, y1], giro.rem_euclid(360)))
    }

    pub(crate) fn gravar_conteudo(&mut self, pagina: u32, ops: &[Operation]) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let mut fluxo = Stream::new(Dictionary::new(), codificar(ops)?);
        let _ = fluxo.compress();
        // Fluxo novo, nunca o antigo reescrito: páginas duplicadas dividem o conteúdo.
        let novo = self.documento.add_object(fluxo);
        self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Contents", Object::Reference(novo));
        self.modificado = true;
        Ok(())
    }

    pub(crate) fn validar_indices(analise: &Analise, objetos: &[usize]) -> Result<(), String> {
        if objetos.is_empty() { return Err("Nenhum objeto escolhido.".into()); }
        match objetos.iter().find(|&&i| i >= analise.objetos.len()) {
            Some(i) => Err(format!("O objeto {i} não existe mais nesta página.")),
            None => Ok(()),
        }
    }

    /// Move os objetos `dx`, `dy` pontos no espaço da página (y para cima).
    pub fn mover_objetos(&mut self, pagina: u32, objetos: &[usize], dx: f32, dy: f32) -> Result<(), String> {
        let mut analise = self.analisar(pagina)?;
        Self::validar_indices(&analise, objetos)?;
        let mut insercoes: Vec<(usize, Vec<Operation>)> = Vec::new();
        let real = |v: f32| Object::Real(v);
        for &k in objetos {
            let lugar = &analise.lugares[k];
            // Imagem ou texto dentro do próprio recorte (q … W n … Do/Tj Q; plantas do AutoCAD põem cada
            // texto num) anda com o recorte, senão sai cortado.
            // Recortes um dentro do outro (o AutoCAD põe dois): vai até o de fora que ainda é só dele.
            let mut recorte = bloco_do_recorte(&analise.ops, lugar.insercao, lugar.fim);
            while let Some(fora) = recorte.and_then(|(i, f)| bloco_do_recorte(&analise.ops, i, f)) { recorte = Some(fora); }
            if lugar.escritas.is_empty() || recorte.is_some() {
                let (inicio, fim, ctm) = match recorte {
                    Some((i, f)) => (i, f, ctm_em(&analise.ops, i)),
                    None => (lugar.insercao, lugar.fim, lugar.matriz),
                };
                let inv = inversa(ctm).ok_or("A imagem tem tamanho zero.")?;
                let m = mult(mult(ctm, [1.0, 0.0, 0.0, 1.0, dx, dy]), inv);
                insercoes.push((inicio, vec![Operation::new("q", vec![]), Operation::new("cm", m.iter().map(|&v| real(v)).collect())]));
                insercoes.push((fim, vec![Operation::new("Q", vec![])]));
            } else {
                // O deslocamento da página vira deslocamento no espaço da linha.
                let lin = lugar.matriz;
                let inv = inversa([lin[0], lin[1], lin[2], lin[3], 0.0, 0.0]).ok_or("A linha tem escala zero.")?;
                let (tx, ty) = aplicar(inv, dx, dy);
                insercoes.push((lugar.insercao, vec![Operation::new("Td", vec![real(tx), real(ty)])]));
                insercoes.push((lugar.fim, vec![Operation::new("Td", vec![real(-tx), real(-ty)])]));
            }
        }
        insercoes.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        for (i, novas) in insercoes { analise.ops.splice(i..i, novas); }
        self.gravar_conteudo(pagina, &analise.ops)
    }

    /// Troca cor e espessura dos desenhos (bordas, linhas, formas) escolhidos;
    /// os outros objetos ficam como estão. Devolve quantos mudaram.
    pub fn estilizar_desenhos(&mut self, pagina: u32, objetos: &[usize], estilo: &EstiloDesenho) -> Result<usize, String> {
        let mut analise = self.analisar(pagina)?;
        Self::validar_indices(&analise, objetos)?;
        let mut alvos: Vec<usize> = objetos.iter().copied()
            .filter(|&k| analise.objetos[k].tipo == TipoObjeto::Desenho).collect();
        if alvos.is_empty() { return Err("Cor e espessura valem para desenhos (bordas, linhas e formas).".into()); }
        alvos.sort_unstable_by(|a, b| b.cmp(a));
        let real = |v: f32| Object::Real(v);
        for &k in &alvos {
            let lugar = analise.lugares[k].clone();
            let pinta = analise.ops[lugar.fim - 1].operator.clone();
            let (mut traco, mut preenche) = (!matches!(pinta.as_str(), "f" | "F" | "f*"), !matches!(pinta.as_str(), "S" | "s"));
            if let Some(c) = estilo.contorno { traco = c.is_some(); }
            if let Some(c) = estilo.preenchimento { preenche = c.is_some(); }
            let (fecha, par) = (matches!(pinta.as_str(), "s" | "b" | "b*"), matches!(pinta.as_str(), "f*" | "B*" | "b*"));
            let novo = match (traco, preenche) {
                (true, true) => match (fecha, par) { (true, true) => "b*", (true, false) => "b", (false, true) => "B*", _ => "B" },
                (true, false) => if fecha { "s" } else { "S" },
                (false, true) => if par { "f*" } else { "f" },
                _ => "n",
            };
            analise.ops[lugar.fim - 1] = Operation::new(novo, vec![]);
            let mut antes = vec![Operation::new("q", vec![])];
            if let Some(e) = estilo.espessura {
                // Espessura em pontos da página, levada ao espaço do caminho.
                let m = lugar.matriz;
                let escala = (m[0] * m[3] - m[1] * m[2]).abs().sqrt().max(1e-6);
                antes.push(Operation::new("w", vec![real(e / escala)]));
            }
            if let Some(Some([r, g, b])) = estilo.contorno { antes.push(Operation::new("RG", vec![real(r), real(g), real(b)])); }
            if let Some(Some([r, g, b])) = estilo.preenchimento { antes.push(Operation::new("rg", vec![real(r), real(g), real(b)])); }
            analise.ops.insert(lugar.fim, Operation::new("Q", vec![]));
            analise.ops.splice(lugar.insercao..lugar.insercao, antes);
        }
        self.gravar_conteudo(pagina, &analise.ops)?;
        Ok(alvos.len())
    }

    /// Apaga os objetos: a linha perde as escritas, a imagem perde o `Do` e o
    /// desenho perde o caminho inteiro.
    pub fn excluir_objetos(&mut self, pagina: u32, objetos: &[usize]) -> Result<(), String> {
        let mut analise = self.analisar(pagina)?;
        Self::validar_indices(&analise, objetos)?;
        let mut remover: Vec<usize> = objetos.iter().flat_map(|&k| {
            let l = &analise.lugares[k];
            if l.escritas.is_empty() { (l.insercao..l.fim).collect() } else { l.escritas.clone() }
        }).collect();
        remover.sort_unstable_by(|a, b| b.cmp(a));
        remover.dedup();
        for i in remover { analise.ops.remove(i); }
        self.gravar_conteudo(pagina, &analise.ops)
    }

    /// Largura de cada texto na fonte da linha `objeto`, em milésimos do em (sem Tc/Tw):
    /// serve para comparar com a largura que a linha já tem.
    pub(crate) fn unidades_texto(&self, pagina: u32, objeto: usize, textos: &[&str]) -> Result<Vec<f32>, String> {
        let analise = self.analisar(pagina)?;
        Self::validar_indices(&analise, &[objeto])?;
        let lugar = &analise.lugares[objeto];
        if lugar.escritas.is_empty() { return Err("Esse objeto não é texto.".into()); }
        let (dicts, _) = self.recursos(self.id_pagina(pagina)?);
        let dict = dicts.get(&lugar.fonte).ok_or("A fonte desta linha não foi encontrada.")?;
        let copia = copia_para_codificar(dict);
        let fonte = Fonte::ler(&self.documento, dict, copia.as_ref().unwrap_or(dict));
        let cod = fonte.codificacao.as_ref().ok_or_else(|| format!("A fonte {} não diz quais letras tem.", fonte.nome))?;
        Ok(textos.iter().map(|t| fonte.codigos(&Document::encode_text(cod, t)).map(|c| fonte.larguras.get(&c).copied().unwrap_or(fonte.padrao)).sum()).collect())
    }

    /// Troca o texto de uma linha, na mesma posição. Na fonte original quando ela tem todas as
    /// letras; a embutida costuma ter só as usadas no documento (CAD grava 40, 50 letras), e aí
    /// a linha vai em Helvetica no mesmo tamanho, como o Acrobat faz, em vez de virar quadrado.
    pub fn editar_texto(&mut self, pagina: u32, objeto: usize, novo: &str) -> Result<(), String> {
        let mut analise = self.analisar(pagina)?;
        Self::validar_indices(&analise, &[objeto])?;
        let lugar = analise.lugares[objeto].clone();
        if lugar.escritas.is_empty() { return Err("Esse objeto não é texto.".into()); }
        let id = self.id_pagina(pagina)?;
        // Na fonte original: os bytes, se ela tem todas as letras (vai e volta igual).
        let (na_original, nome_fonte) = {
            let (dicts, _) = self.recursos(id);
            let dict = dicts.get(&lugar.fonte).ok_or("A fonte desta linha não foi encontrada.")?;
            let copia = copia_para_codificar(dict);
            let fonte = Fonte::ler(&self.documento, dict, copia.as_ref().unwrap_or(dict));
            let formato = if fonte.dois_bytes { lopdf::StringFormat::Hexadecimal } else { lopdf::StringFormat::Literal };
            let bytes = fonte.codificacao.as_ref().map(|cod| (Document::encode_text(cod, novo), cod))
                .filter(|(b, cod)| Document::decode_text(cod, b).ok().as_deref() == Some(novo)).map(|(b, _)| (b, formato));
            (bytes, fonte.nome.clone())
        };
        let primeira = lugar.escritas[0];
        for &i in lugar.escritas[1..].iter().rev() { analise.ops.remove(i); }
        match na_original {
            Some((bytes, formato)) => analise.ops[primeira] = Operation::new("Tj", vec![Object::String(bytes, formato)]),
            None => {
                if let Some(c) = novo.chars().find(|&c| crate::inserir::win_ansi(c).is_none()) {
                    return Err(format!("Nem a fonte {nome_fonte} embutida no PDF nem a Helvetica têm “{c}”."));
                }
                let tamanho = match &analise.objetos[objeto].tipo { TipoObjeto::Texto { tamanho, .. } => *tamanho, _ => 12.0 };
                let f = self.fonte_com_larguras(false);
                let nome = format!("FPo{}", f.0);
                self.por_recurso(id, "Font", &nome, f)?;
                let bytes: Vec<u8> = novo.chars().filter_map(crate::inserir::win_ansi).collect();
                // Troca a fonte só para esta escrita e devolve a original logo depois.
                analise.ops[primeira] = Operation::new("Tf", vec![Object::Name(nome.into_bytes()), tamanho.into()]);
                analise.ops.insert(primeira + 1, Operation::new("Tj", vec![Object::String(bytes, lopdf::StringFormat::Literal)]));
                analise.ops.insert(primeira + 2, Operation::new("Tf", vec![Object::Name(lugar.fonte.clone()), tamanho.into()]));
            }
        }
        self.gravar_conteudo(pagina, &analise.ops)
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use lopdf::dictionary;

    /// Página com duas linhas em Helvetica e uma imagem 2×2.
    fn pdf_com_texto() -> DocumentoPdf {
        pdf_com("BT /F1 12 Tf 100 700 Td (Ola mundo) Tj 0 -20 Td [(Segunda) -250 (li)] TJ 57 0 Td (nha) Tj ET\n\
                 q 50 0 0 40 300 500 cm /Im1 Do Q")
    }

    /// `agulha` aparece no arquivo salvo, cru ou dentro de um fluxo descomprimido.
    pub(crate) fn no_arquivo(bytes: &[u8], agulha: &[u8]) -> bool {
        let tem = |b: &[u8]| b.windows(agulha.len()).any(|w| w == agulha);
        tem(bytes) || Document::load_mem(bytes).unwrap().objects.values().any(|o| matches!(o, Object::Stream(s)
            if tem(&s.decompressed_content().unwrap_or_else(|_| s.content.clone()))))
    }

    pub(crate) fn pdf_com(conteudo: &str) -> DocumentoPdf {
        let mut doc = Document::with_version("1.5");
        let paginas = doc.new_object_id();
        let fonte = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let imagem = doc.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        }, vec![0, 255, 255, 0]));
        let c = doc.add_object(Stream::new(Dictionary::new(), conteudo.as_bytes().to_vec()));
        let pagina = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => paginas, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => c,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => fonte }, "XObject" => dictionary! { "Im1" => imagem } },
        });
        doc.objects.insert(paginas, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![pagina.into()], "Count" => 1,
        }));
        let catalogo = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => paginas });
        doc.trailer.set("Root", catalogo);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        DocumentoPdf::de_bytes(&bytes, None).unwrap()
    }

    fn textos(pdf: &DocumentoPdf) -> Vec<String> {
        pdf.objetos(1).unwrap().into_iter().filter_map(|o| match o.tipo {
            TipoObjeto::Texto { texto, .. } => Some(texto), _ => None,
        }).collect()
    }

    #[test]
    fn borda_troca_cor_espessura_e_perde_preenchimento() {
        let mut pdf = pdf_com("0 0 1 rg 2 w 50 50 20 20 re B");
        let estilo = EstiloDesenho { contorno: Some(Some([1.0, 0.0, 0.0])), preenchimento: Some(None), espessura: Some(3.0) };
        assert_eq!(pdf.estilizar_desenhos(1, &[0], &estilo).unwrap(), 1);
        let ops: Vec<String> = pdf.analisar(1).unwrap().ops.iter().map(|o| o.operator.clone()).collect();
        assert_eq!(ops, ["rg", "w", "q", "w", "RG", "re", "S", "Q"]);
        // Continua um objeto só, para escolher e mexer de novo.
        assert_eq!(pdf.objetos(1).unwrap().len(), 1);
    }

    #[test]
    fn bordas_viram_objetos_moviveis() {
        // Linha de 2 pt, um recorte (não conta) e um quadrado cheio.
        let mut pdf = pdf_com("2 w 100 100 m 300 100 l S q 10 10 50 50 re W n Q 50 50 20 20 re f");
        let obj = pdf.objetos(1).unwrap();
        assert_eq!(obj.iter().map(|o| (o.tipo.clone(), o.caixa)).collect::<Vec<_>>(), [
            (TipoObjeto::Desenho, [99.0, 99.0, 301.0, 101.0]),
            (TipoObjeto::Desenho, [49.0, 49.0, 71.0, 71.0]),
        ]);
        pdf.mover_objetos(1, &[0], 0.0, 50.0).unwrap();
        assert_eq!(pdf.objetos(1).unwrap()[0].caixa, [99.0, 149.0, 301.0, 151.0]);
        pdf.excluir_objetos(1, &[1]).unwrap();
        let obj = pdf.objetos(1).unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj[0].caixa[1], 149.0);
    }

    #[test]
    fn acha_linhas_e_imagem_com_caixas() {
        let pdf = pdf_com_texto();
        let obj = pdf.objetos(1).unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(textos(&pdf), ["Ola mundo", "Segunda linha"]);
        let TipoObjeto::Texto { fonte, tamanho, .. } = &obj[0].tipo else { panic!() };
        assert_eq!((fonte.as_str(), *tamanho), ("Helvetica", 12.0));
        // "Ola mundo" em Helvetica 12 sem /Widths: 9 letras × 6 pt.
        assert!((obj[0].caixa[0] - 100.0).abs() < 0.01 && (obj[0].caixa[2] - 154.0).abs() < 0.01);
        assert!(obj[1].caixa[1] < 680.0 && obj[1].caixa[3] > 680.0);
        assert_eq!(obj[2].tipo, TipoObjeto::Imagem { largura: 2, altura: 2 });
        assert_eq!(obj[2].caixa, [300.0, 500.0, 350.0, 540.0]);
    }

    #[test]
    fn imagem_recortada_anda_com_o_recorte() {
        let mut pdf = pdf_com("q 100 100 50 50 re W n q 50 0 0 50 100 100 cm /Im1 Do Q Q");
        pdf.mover_objetos(1, &[0], 200.0, 0.0).unwrap();
        let caixa = pdf.objetos(1).unwrap()[0].caixa;
        assert!((caixa[0] - 300.0).abs() < 0.01, "{caixa:?}");
        let ops = pdf.analisar(1).unwrap().ops;
        let recorte = ops.iter().position(|o| o.operator == "re").unwrap();
        // O deslocamento entra antes do recorte, não só antes do Do.
        assert!(ops[..recorte].iter().any(|o| o.operator == "cm"));
    }

    #[test]
    fn texto_no_proprio_recorte_anda_com_o_recorte() {
        // Como as plantas do AutoCAD: cada texto numa caixa justa; mover só o texto o cortava.
        let mut pdf = pdf_com("q 90 690 80 30 re W n q 95 695 70 20 re W n BT /F1 12 Tf 100 700 Td (teste 123) Tj ET Q Q");
        pdf.mover_objetos(1, &[0], 200.0, 0.0).unwrap();
        let caixa = pdf.objetos(1).unwrap()[0].caixa;
        assert!((caixa[0] - 300.0).abs() < 0.01, "{caixa:?}");
        let ops = pdf.analisar(1).unwrap().ops;
        let recorte = ops.iter().position(|o| o.operator == "re").unwrap();
        assert!(ops[..recorte].iter().any(|o| o.operator == "cm"), "os dois recortes andam juntos");
    }

    #[test]
    fn mover_texto_nao_arrasta_a_linha_seguinte() {
        let mut pdf = pdf_com_texto();
        let antes = pdf.objetos(1).unwrap();
        pdf.mover_objetos(1, &[0, 2], 10.0, -5.0).unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let depois = pdf.objetos(1).unwrap();
        assert_eq!(depois.len(), 3);
        for k in [0, 2] {
            assert!((depois[k].caixa[0] - antes[k].caixa[0] - 10.0).abs() < 0.01);
            assert!((depois[k].caixa[1] - antes[k].caixa[1] + 5.0).abs() < 0.01);
        }
        assert_eq!(depois[1].caixa, antes[1].caixa);
    }

    #[test]
    fn editar_e_excluir_texto() {
        let mut pdf = pdf_com_texto();
        pdf.editar_texto(1, 1, "Linha nova, ação").unwrap();
        assert_eq!(textos(&pdf), ["Ola mundo", "Linha nova, ação"]);
        let err = pdf.editar_texto(1, 0, "Olá 日本").unwrap_err();
        assert!(err.contains('日') && !err.contains('á'), "{err}");
        pdf.excluir_objetos(1, &[0, 2]).unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(textos(&pdf), ["Linha nova, ação"]);
        assert_eq!(pdf.objetos(1).unwrap().len(), 1);
        assert!(pdf.texto_pagina(1).unwrap().contains("Linha nova"));
    }

    #[test]
    fn localizar_e_trocar_sem_caixa() {
        assert_eq!(achar_sem_caixa("Ação e AÇÃO, ação", "ação"), [0, 7, 13]);
        assert_eq!(trocar_sem_caixa("Ola OLA", "ola", "Oi"), ("Oi Oi".to_string(), 2));
        let mut pdf = pdf_com_texto();
        assert_eq!(pdf.trocar_texto(&[(1, 0), (1, 1)], "LINHA", "fila").unwrap(), 1);
        assert_eq!(textos(&pdf), ["Ola mundo", "Segunda fila"]);
        assert!(pdf.trocar_texto(&[(1, 0)], "mundo", "世界").is_err());
    }

    #[test]
    fn imagem_embutida_volta_igual() {
        let ops = Content::decode(b"q BI /W 2 /H 1 /BPC 8 /CS /DeviceGray ID \x00\xff EI Q").unwrap().operations;
        let bytes = codificar(&ops).unwrap();
        let de_novo = Content::decode(&bytes).unwrap().operations;
        assert_eq!(de_novo.len(), 3);
        let Some(Object::Stream(s)) = de_novo[1].operands.first() else { panic!("{:?}", de_novo) };
        assert_eq!(s.content, [0, 255]);
        // Comprimida: o lopdf não lê, e regravar é recusado.
        let comprimida = Content::decode(b"q BI /W 2 /H 1 /BPC 8 /CS /DeviceGray /F /AHx ID 00ff> EI Q").unwrap().operations;
        assert!(codificar(&comprimida).is_err());
    }
}
