//! Inspetor interno (F12): o que há dentro do PDF aberto — a árvore de objetos
//! a partir do trailer, o dicionário de um objeto em sintaxe PDF, as fontes,
//! as imagens, a marcação de estrutura (tags) e os problemas. Só leitura.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::otimizar::{eh_imagem, exibicoes};
use crate::DocumentoPdf;

/// Tetos para PDF enorme: a janela mostra o começo, não trava montando tudo.
const MAX_LINHAS_ARVORE: usize = 5_000;
const MAX_LINHAS_DICIONARIO: usize = 2_000;
const MAX_NOS_ESTRUTURA: usize = 5_000;
const MAX_FLUXO: usize = 4 * 1024;
/// Stream maior que isto não é descomprimido só para mostrar o começo.
const MAX_DESCOMPRIMIR: usize = 16 * 1024 * 1024;
/// Imagem acima disto (no tamanho em que aparece) e deste peso vira candidata a Comprimir.
const DPI_DEMAIS: f32 = 250.0;
const PESO_DEMAIS: usize = 100 * 1024;
/// As 14 fontes que todo leitor tem: não precisam estar embutidas.
const PADRAO_14: [&str; 14] = ["Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic", "Helvetica",
    "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique", "Courier", "Courier-Bold", "Courier-Oblique",
    "Courier-BoldOblique", "Symbol", "ZapfDingbats"];

/// Linha do cabeçalho: "PDF 1.7 · 14 páginas · 312 objetos · xref comprimido".
#[derive(Debug, Clone)]
pub struct Resumo { pub versao: String, pub paginas: u32, pub objetos: usize, pub xref_comprimido: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gravidade { Erro, Aviso }

#[derive(Debug, Clone)]
pub struct Problema { pub gravidade: Gravidade, pub titulo: String, pub detalhe: String, pub objeto: Option<ObjectId> }

#[derive(Debug, Clone)]
pub struct FonteInfo {
    pub id: ObjectId,
    /// Nome no /Resources da página ("F2"); vazio se nenhuma página a cita direto.
    pub recurso: String,
    pub nome: String,
    pub tipo: String,
    pub embutida: bool,
    pub subconjunto: bool,
    pub to_unicode: bool,
    pub paginas: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct ImagemInfo {
    pub id: ObjectId,
    pub largura: i64,
    pub altura: i64,
    pub formato: String,
    pub bytes: usize,
    pub paginas: Vec<u32>,
    /// Resolução no maior tamanho em que aparece; `None` se não aparece no conteúdo das páginas.
    pub dpi: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct NoEstrutura { pub nivel: u16, pub tipo: String, pub detalhe: String }

/// O que o inspetor lê uma vez por documento.
#[derive(Debug, Clone)]
pub struct Inspecao {
    pub resumo: Resumo,
    pub fontes: Vec<FonteInfo>,
    pub imagens: Vec<ImagemInfo>,
    /// `None`: o PDF não tem /StructTreeRoot.
    pub estrutura: Option<Vec<NoEstrutura>>,
    pub problemas: Vec<Problema>,
}

/// Uma linha visível da árvore de objetos.
#[derive(Debug, Clone)]
pub struct LinhaArvore {
    pub nivel: u16,
    /// Chave de abertura ("Root/Pages/Kids/2"); é o que a interface guarda como aberto.
    pub caminho: String,
    pub chave: String,
    pub referencia: Option<ObjectId>,
    pub dica: String,
    pub expansivel: bool,
    pub aberto: bool,
}

/// Cor de cada pedaço do dicionário escrito em sintaxe PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cor { Pontuacao, Nome, Numero, Texto, Referencia(ObjectId), Comentario }

#[derive(Debug, Clone)]
pub struct Pedaco { pub texto: String, pub cor: Cor }

#[derive(Debug, Clone)]
pub struct DetalheObjeto {
    pub id: ObjectId,
    pub tipo: String,
    pub pagina: Option<u32>,
    pub linhas: Vec<Vec<Pedaco>>,
    /// O stream (o próprio objeto, ou o /Contents da página) e o começo dele decodificado.
    pub fluxo: Option<(ObjectId, String)>,
}

impl DetalheObjeto {
    /// O dicionário como texto, para o Copiar.
    pub fn texto(&self) -> String {
        let mut t = format!("{} {} obj\n", self.id.0, self.id.1);
        for l in &self.linhas {
            l.iter().for_each(|p| t.push_str(&p.texto));
            t.push('\n');
        }
        t.push_str("endobj");
        t
    }
}

/// "512 B", "18 kB", "1,2 MB".
pub fn tamanho(bytes: usize) -> String {
    if bytes < 1024 { format!("{bytes} B") }
    else if bytes < 1024 * 1024 { format!("{} kB", bytes / 1024) }
    else { format!("{:.1} MB", bytes as f64 / 1048576.0).replace('.', ",") }
}

fn texto_nome(n: &[u8]) -> String { String::from_utf8_lossy(n).into_owned() }

fn nome(o: &Object) -> Option<String> { o.as_name().ok().map(texto_nome) }

fn tipo_de(d: &Dictionary) -> Option<String> { d.get(b"Type").ok().and_then(nome) }

fn subtipo(d: &Dictionary) -> Option<String> { d.get(b"Subtype").ok().and_then(nome) }

fn resolver<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    match o { Object::Reference(id) => doc.get_object(*id).ok(), o => Some(o) }
}

fn dicionario<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match resolver(doc, o)? { Object::Dictionary(d) => Some(d), Object::Stream(s) => Some(&s.dict), _ => None }
}

fn numeros_das_paginas(doc: &Document) -> HashMap<ObjectId, u32> {
    doc.get_pages().into_iter().map(|(n, id)| (id, n)).collect()
}

/// Texto de string PDF: UTF-16 com BOM, ou byte a byte (PDFDocEncoding, aproximado).
fn texto_pdf(b: &[u8]) -> String {
    let t: String = if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        b.iter().map(|&c| c as char).collect()
    };
    let t: String = t.chars().map(|c| if c.is_control() { '·' } else { c }).collect();
    curto(&t, 200)
}

fn curto(t: &str, max: usize) -> String {
    if t.chars().count() <= max { return t.into(); }
    let mut s: String = t.chars().take(max).collect();
    s.push('…');
    s
}

fn formato_imagem(s: &Stream) -> String {
    let filtros = s.filters().unwrap_or_default();
    match filtros.last().map(|f| &f[..]) {
        Some(b"DCTDecode") => "JPEG".into(),
        Some(b"JPXDecode") => "JPEG 2000".into(),
        Some(b"JBIG2Decode") => "JBIG2".into(),
        Some(b"CCITTFaxDecode") => "CCITT".into(),
        Some(b"FlateDecode") => "Flate".into(),
        Some(b"LZWDecode") => "LZW".into(),
        Some(b"RunLengthDecode") => "RLE".into(),
        Some(f) => texto_nome(f),
        None => "sem filtro".into(),
    }
}

fn medida_imagem(s: &Stream) -> (i64, i64) {
    let n = |k: &[u8]| s.dict.get(k).and_then(Object::as_i64).unwrap_or(0);
    (n(b"Width"), n(b"Height"))
}

/// Uma linha só, como a sintaxe PDF escreve (o filtro da árvore procura nela).
fn em_linha(o: &Object, saida: &mut String) {
    match o {
        Object::Null => saida.push_str("null"),
        Object::Boolean(b) => saida.push_str(if *b { "true" } else { "false" }),
        Object::Integer(i) => saida.push_str(&i.to_string()),
        Object::Real(r) => saida.push_str(&r.to_string()),
        Object::Name(n) => { saida.push('/'); saida.push_str(&texto_nome(n)); }
        Object::String(b, _) => { saida.push('('); saida.push_str(&texto_pdf(b)); saida.push(')'); }
        Object::Array(a) => {
            saida.push('[');
            for (i, x) in a.iter().enumerate() { if i > 0 { saida.push(' '); } em_linha(x, saida); }
            saida.push(']');
        }
        Object::Dictionary(d) => dicionario_em_linha(d, saida),
        Object::Stream(s) => { dicionario_em_linha(&s.dict, saida); saida.push_str(" stream"); }
        Object::Reference((n, g)) => saida.push_str(&format!("{n} {g} R")),
    }
}

fn dicionario_em_linha(d: &Dictionary, saida: &mut String) {
    saida.push_str("<<");
    for (k, v) in d { saida.push_str(" /"); saida.push_str(&texto_nome(k)); saida.push(' '); em_linha(v, saida); }
    saida.push_str(" >>");
}

/// Dica cinza da árvore: o tipo, a contagem ou o tamanho do que o valor aponta.
fn dica(doc: &Document, valor: &Object, paginas: &HashMap<ObjectId, u32>) -> String {
    let id = valor.as_reference().ok();
    let Some(alvo) = resolver(doc, valor) else { return "não existe".into() };
    match alvo {
        Object::Stream(s) if subtipo(&s.dict).as_deref() == Some("Image") => {
            let (w, h) = medida_imagem(s);
            format!("imagem {w}×{h} {}", formato_imagem(s))
        }
        Object::Stream(s) if tipo_de(&s.dict).as_deref() == Some("Metadata") => "XMP".into(),
        Object::Stream(s) => format!("stream {}", tamanho(s.content.len())),
        Object::Dictionary(d) => match tipo_de(d).as_deref() {
            Some("Page") => id.and_then(|i| paginas.get(&i)).map_or("Page".into(), |n| format!("Page {n}")),
            Some("Pages") => format!("{} páginas", d.get(b"Count").and_then(Object::as_i64).unwrap_or(0)),
            Some("Outlines") => match d.get(b"Count").and_then(Object::as_i64) {
                Ok(n) => format!("{} marcadores", n.abs()),
                Err(_) => "Outlines".into(),
            },
            Some("Font") => format!("Font {}", d.get(b"BaseFont").ok().and_then(nome).unwrap_or_default()),
            Some(t) => t.into(),
            None => match d.get(b"Fields").ok().and_then(|f| resolver(doc, f)).and_then(|f| f.as_array().ok()) {
                Some(campos) => format!("{} campos", campos.len()),
                None => format!("{} chaves", d.len()),
            },
        },
        Object::Array(a) => format!("[{}]", a.len()),
        outro => { let mut t = String::new(); em_linha(outro, &mut t); curto(&t, 60) }
    }
}

/// Filhos de um valor já resolvido: (pedaço do caminho, rótulo, valor).
fn filhos(alvo: &Object) -> Vec<(String, String, &Object)> {
    match alvo {
        Object::Dictionary(d) => d.iter().map(|(k, v)| (texto_nome(k), format!("/{}", texto_nome(k)), v)).collect(),
        Object::Stream(s) => s.dict.iter().map(|(k, v)| (texto_nome(k), format!("/{}", texto_nome(k)), v)).collect(),
        Object::Array(a) => a.iter().enumerate()
            .map(|(i, v)| (i.to_string(), if v.as_reference().is_ok() { String::new() } else { format!("[{i}]") }, v)).collect(),
        _ => Vec::new(),
    }
}

struct Arvore<'a> {
    doc: &'a Document,
    paginas: HashMap<ObjectId, u32>,
    abertos: &'a HashSet<String>,
    ancestrais: Vec<ObjectId>,
    saida: Vec<LinhaArvore>,
}

impl Arvore<'_> {
    fn ramo(&mut self, nivel: u16, caminho: String, chave: String, valor: &Object) {
        if self.saida.len() >= MAX_LINHAS_ARVORE { return; }
        let referencia = valor.as_reference().ok();
        let alvo = resolver(self.doc, valor);
        let tem_filhos = alvo.is_some_and(|a| !filhos(a).is_empty());
        // Sem isto o /Parent de uma página reabriria a árvore inteira dentro dela.
        let ciclo = referencia.is_some_and(|r| self.ancestrais.contains(&r));
        let expansivel = tem_filhos && !ciclo;
        let aberto = expansivel && self.abertos.contains(&caminho);
        let dica = if ciclo { "↑ já aberto acima".into() } else { dica(self.doc, valor, &self.paginas) };
        self.saida.push(LinhaArvore { nivel, caminho: caminho.clone(), chave, referencia, dica, expansivel, aberto });
        let (true, Some(alvo)) = (aberto, alvo) else { return };
        if let Some(r) = referencia { self.ancestrais.push(r); }
        for (parte, rotulo, v) in filhos(alvo) { self.ramo(nivel + 1, format!("{caminho}/{parte}"), rotulo, v); }
        if referencia.is_some() { self.ancestrais.pop(); }
    }
}

/// Só estas chaves do trailer abrem a árvore (o dicionário do xref stream traz /W, /Index…).
const RAIZES: [&[u8]; 4] = [b"Root", b"Info", b"Encrypt", b"ID"];

fn normalizar(t: &str) -> String { t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }

/// Fluxo à mostra: o começo do stream decodificado, ou por que não dá.
fn previa(s: &Stream) -> String {
    if subtipo(&s.dict).as_deref() == Some("Image") {
        let (w, h) = medida_imagem(s);
        return format!("(imagem {w}×{h} {} — os pixels não são mostrados)", formato_imagem(s));
    }
    let filtros = s.filters().unwrap_or_default();
    let (dados, decodificado) = if filtros.is_empty() { (s.content.clone(), true) }
        else if s.content.len() > MAX_DESCOMPRIMIR { (s.content.clone(), false) }
        else { match s.decompressed_content() { Ok(d) => (d, true), Err(_) => (s.content.clone(), false) } };
    let corte = &dados[..dados.len().min(MAX_FLUXO)];
    let estranhos = corte.iter().filter(|&&b| b < 9 || (b > 13 && b < 32)).count();
    if estranhos * 10 > corte.len() {
        return format!("(conteúdo binário — {}{})", tamanho(dados.len()), if decodificado { " decodificados" } else { ", não decodificado" });
    }
    // Byte fora do ASCII (código de glifo num Tj) vira escape octal do PDF: como "�" ele não existia na
    // fonte mono, e a janela do F12 procurava outra fonte a cada quadro (~40 ms com o fluxo à vista).
    let mut t: String = corte.iter().map(|&b| if b.is_ascii() { (b as char).to_string() } else { format!("\\{b:03o}") }).collect();
    if !decodificado { t.insert_str(0, "(não decodificado)\n"); }
    if dados.len() > MAX_FLUXO { t.push_str(&format!("\n… ({} no total)", tamanho(dados.len()))); }
    t
}

/// Nome do papel para o comentário do /MediaBox.
fn papel(caixa: &[Object]) -> Option<String> {
    let v: Vec<f32> = caixa.iter().filter_map(|x| x.as_float().ok()).collect();
    if v.len() != 4 { return None; }
    let (l, a) = ((v[2] - v[0]).abs(), (v[3] - v[1]).abs());
    let (menor, maior) = (l.min(a), l.max(a));
    let nome = [("A5", 420.0, 595.0), ("A4", 595.0, 842.0), ("A3", 842.0, 1191.0), ("Carta", 612.0, 792.0), ("Ofício", 612.0, 1008.0)]
        .into_iter().find(|&(_, x, y)| (menor - x).abs() < 2.0 && (maior - y).abs() < 2.0).map(|(n, _, _)| n);
    Some(match nome {
        Some(n) if l > a => format!("{n} deitado"),
        Some(n) => n.into(),
        None => format!("{l:.0}×{a:.0} pt"),
    })
}

/// Escreve um objeto em sintaxe PDF, uma linha por entrada de dicionário, com cor e comentários.
struct Escritor<'a> {
    doc: &'a Document,
    paginas: &'a HashMap<ObjectId, u32>,
    linhas: Vec<Vec<Pedaco>>,
    atual: Vec<Pedaco>,
    cortado: bool,
}

impl Escritor<'_> {
    fn por(&mut self, texto: impl Into<String>, cor: Cor) { self.atual.push(Pedaco { texto: texto.into(), cor }); }

    fn quebra(&mut self) {
        let linha = std::mem::take(&mut self.atual);
        if self.linhas.len() >= MAX_LINHAS_DICIONARIO { self.cortado = true; } else { self.linhas.push(linha); }
    }

    fn recuo(&mut self, n: usize) { if n > 0 { self.por("  ".repeat(n), Cor::Pontuacao); } }

    fn valor(&mut self, o: &Object, recuo: usize) {
        match o {
            Object::Dictionary(d) => self.dicionario(d, recuo),
            Object::Stream(s) => self.dicionario(&s.dict, recuo),
            Object::Array(a) if a.len() <= 12 && a.iter().all(|x| !matches!(x, Object::Array(_) | Object::Dictionary(_) | Object::Stream(_))) => {
                self.por("[", Cor::Pontuacao);
                for (i, x) in a.iter().enumerate() {
                    if i > 0 { self.por(" ", Cor::Pontuacao); }
                    self.valor(x, recuo);
                }
                self.por("]", Cor::Pontuacao);
            }
            Object::Array(a) => {
                self.por("[", Cor::Pontuacao);
                self.quebra();
                for x in a {
                    if self.cortado { return; }
                    self.recuo(recuo + 1);
                    self.valor(x, recuo + 1);
                    self.comentario(b"", x);
                    self.quebra();
                }
                self.recuo(recuo);
                self.por("]", Cor::Pontuacao);
            }
            Object::Reference((n, g)) => self.por(format!("{n} {g} R"), Cor::Referencia((*n, *g))),
            Object::Name(n) => self.por(format!("/{}", texto_nome(n)), Cor::Nome),
            Object::String(b, _) => self.por(format!("({})", texto_pdf(b)), Cor::Texto),
            outro => { let mut t = String::new(); em_linha(outro, &mut t); self.por(t, Cor::Numero); }
        }
    }

    fn dicionario(&mut self, d: &Dictionary, recuo: usize) {
        if d.is_empty() { self.por("<< >>", Cor::Pontuacao); return; }
        self.por("<<", Cor::Pontuacao);
        self.quebra();
        for (k, v) in d {
            if self.cortado { return; }
            self.recuo(recuo + 1);
            self.por(format!("/{}", texto_nome(k)), Cor::Nome);
            self.por(" ", Cor::Pontuacao);
            self.valor(v, recuo + 1);
            self.comentario(k, v);
            self.quebra();
        }
        self.recuo(recuo);
        self.por(">>", Cor::Pontuacao);
    }

    /// "% A4" no MediaBox, "% 1240×930 JPEG, 412 kB" ao lado de uma imagem, "% página 3"…
    fn comentario(&mut self, chave: &[u8], v: &Object) {
        let nota = match (chave, v) {
            (b"MediaBox" | b"CropBox" | b"TrimBox" | b"BleedBox" | b"ArtBox", Object::Array(a)) => papel(a),
            (_, Object::Reference(id)) => match self.doc.get_object(*id) {
                Err(_) => Some("não existe".into()),
                Ok(Object::Stream(s)) if subtipo(&s.dict).as_deref() == Some("Image") => {
                    let (w, h) = medida_imagem(s);
                    Some(format!("{w}×{h} {}, {}", formato_imagem(s), tamanho(s.content.len())))
                }
                Ok(Object::Stream(s)) => Some(format!("stream {}", tamanho(s.content.len()))),
                Ok(Object::Dictionary(d)) => match tipo_de(d).as_deref() {
                    Some("Page") => self.paginas.get(id).map(|n| format!("página {n}")),
                    Some("Font") => d.get(b"BaseFont").ok().and_then(nome),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        if let Some(n) = nota { self.por(format!("   % {n}"), Cor::Comentario); }
    }
}

/// Nome do recurso e páginas de cada fonte citada no /Resources das páginas.
fn usos_das_fontes(doc: &Document) -> HashMap<ObjectId, (String, BTreeSet<u32>)> {
    let mut usos: HashMap<ObjectId, (String, BTreeSet<u32>)> = HashMap::new();
    for (n, pid) in doc.get_pages() {
        let (recursos, herdados) = doc.get_page_resources(pid).unwrap_or((None, vec![]));
        let dicts = recursos.into_iter().chain(herdados.iter().filter_map(|h| doc.get_dictionary(*h).ok()));
        for r in dicts {
            let Some(fontes) = r.get(b"Font").ok().and_then(|f| dicionario(doc, f)) else { continue };
            for (k, v) in fontes {
                let Ok(fid) = v.as_reference() else { continue };
                usos.entry(fid).or_insert_with(|| (texto_nome(k), BTreeSet::new())).1.insert(n);
            }
        }
    }
    usos
}

fn ler_fontes(doc: &Document, problemas: &mut Vec<Problema>) -> Vec<FonteInfo> {
    let usos = usos_das_fontes(doc);
    let mut fontes = Vec::new();
    for (id, o) in &doc.objects {
        let Ok(d) = o.as_dict() else { continue };
        let tipo = subtipo(d).unwrap_or_default();
        // A CIDFont descendente aparece dentro da Type0 dona dela.
        if tipo_de(d).as_deref() != Some("Font") || tipo.starts_with("CIDFont") { continue; }
        let nome_fonte = d.get(b"BaseFont").ok().and_then(nome).unwrap_or_else(|| "(sem nome)".into());
        let descendente = d.get(b"DescendantFonts").ok().and_then(|x| resolver(doc, x)).and_then(|x| x.as_array().ok())
            .and_then(|a| a.first()).and_then(|x| dicionario(doc, x));
        let descritor = descendente.unwrap_or(d).get(b"FontDescriptor").ok().and_then(|x| dicionario(doc, x));
        let embutida = tipo == "Type3" || descritor.is_some_and(|f| [&b"FontFile"[..], b"FontFile2", b"FontFile3"].iter().any(|k| f.has(k)));
        let b = nome_fonte.as_bytes();
        let subconjunto = b.len() > 7 && b[6] == b'+' && b[..6].iter().all(u8::is_ascii_uppercase);
        let to_unicode = d.has(b"ToUnicode");
        let (recurso, paginas) = usos.get(id).map(|(r, p)| (r.clone(), p.iter().copied().collect())).unwrap_or_default();
        let rotulo = if recurso.is_empty() { format!("{} {} R", id.0, id.1) } else { recurso.clone() };
        // Sem ToUnicode, copiar texto depende da codificação: Type0 sem CMap Unicode e
        // fonte simbólica embutida sem /Encoding (a codificação é a do programa da fonte) saem embaralhadas.
        let simbolica = descritor.and_then(|f| f.get(b"Flags").and_then(Object::as_i64).ok()).is_some_and(|f| f & 4 != 0);
        let sem_mapa = !to_unicode && match tipo.as_str() {
            "Type0" => !d.get(b"Encoding").ok().and_then(nome).is_some_and(|e| e.starts_with("Uni")),
            "Type3" => false,
            _ => embutida && simbolica && !d.has(b"Encoding"),
        };
        if sem_mapa {
            problemas.push(Problema { gravidade: Gravidade::Aviso, titulo: format!("Fonte {rotulo} sem ToUnicode"),
                detalhe: format!("Copiar texto dessa fonte ({nome_fonte}) sai embaralhado."), objeto: Some(*id) });
        }
        let base = nome_fonte.split_once('+').map_or(nome_fonte.as_str(), |(_, n)| n);
        if !embutida && !PADRAO_14.contains(&base) {
            problemas.push(Problema { gravidade: Gravidade::Aviso, titulo: format!("Fonte {rotulo} não embutida"),
                detalhe: format!("{nome_fonte} depende de o computador de quem abre ter a fonte; a aparência pode mudar."), objeto: Some(*id) });
        }
        fontes.push(FonteInfo { id: *id, recurso, nome: nome_fonte, tipo, embutida, subconjunto, to_unicode, paginas });
    }
    fontes
}

fn ler_imagens(doc: &Document, problemas: &mut Vec<Problema>) -> Vec<ImagemInfo> {
    let vistas = exibicoes(doc);
    let mut imagens = Vec::new();
    for (id, o) in &doc.objects {
        let Object::Stream(s) = o else { continue };
        if !eh_imagem(o) { continue; }
        let (largura, altura) = medida_imagem(s);
        let vista = vistas.get(id);
        let dpi = vista.and_then(|((l, a), _)| {
            let por = |px: i64, pt: f32| (pt > 0.0).then(|| px as f32 * 72.0 / pt);
            match (por(largura, *l), por(altura, *a)) { (Some(x), Some(y)) => Some(x.min(y)), (x, y) => x.or(y) }
        });
        let bytes = s.content.len();
        if dpi.is_some_and(|d| d >= DPI_DEMAIS) && bytes >= PESO_DEMAIS {
            problemas.push(Problema { gravidade: Gravidade::Aviso,
                titulo: format!("Imagem {} {} R aparece a {:.0} dpi", id.0, id.1, dpi.unwrap_or_default()),
                detalhe: format!("Mais que o necessário para tela e impressão ({}) — candidata a Comprimir.", tamanho(bytes)),
                objeto: Some(*id) });
        }
        imagens.push(ImagemInfo { id: *id, largura, altura, formato: formato_imagem(s), bytes,
            paginas: vista.map(|(_, p)| p.iter().copied().collect()).unwrap_or_default(), dpi });
    }
    imagens
}

/// Referências para objetos que não estão no arquivo (o leitor as trata como null).
fn referencias_quebradas(doc: &Document) -> Option<Problema> {
    fn contar(doc: &Document, o: &Object, faltam: &mut Vec<ObjectId>) {
        match o {
            Object::Reference(id) => if !doc.objects.contains_key(id) { faltam.push(*id) },
            Object::Array(a) => a.iter().for_each(|x| contar(doc, x, faltam)),
            Object::Dictionary(d) => d.iter().for_each(|(_, x)| contar(doc, x, faltam)),
            Object::Stream(s) => s.dict.iter().for_each(|(_, x)| contar(doc, x, faltam)),
            _ => {}
        }
    }
    let mut faltam = Vec::new();
    doc.objects.values().for_each(|o| contar(doc, o, &mut faltam));
    let primeiro = *faltam.first()?;
    Some(Problema { gravidade: Gravidade::Erro,
        titulo: format!("{} referência(s) a objetos que não existem", faltam.len()),
        detalhe: format!("A primeira aponta para {} {} R; o leitor trata como vazio.", primeiro.0, primeiro.1), objeto: None })
}

fn ler_estrutura(doc: &Document, paginas: &HashMap<ObjectId, u32>) -> Option<Vec<NoEstrutura>> {
    fn elemento(doc: &Document, o: &Object, nivel: u16, paginas: &HashMap<ObjectId, u32>,
                vistos: &mut HashSet<ObjectId>, saida: &mut Vec<NoEstrutura>) {
        if saida.len() >= MAX_NOS_ESTRUTURA || nivel > 64 { return; }
        if let Object::Reference(r) = o { if !vistos.insert(*r) { return; } }
        match resolver(doc, o) {
            Some(Object::Array(a)) => a.iter().for_each(|x| elemento(doc, x, nivel, paginas, vistos, saida)),
            Some(Object::Dictionary(d)) if d.has(b"S") => {
                let texto = |k: &[u8]| d.get(k).ok().and_then(|t| t.as_str().ok()).map(texto_pdf);
                let mut detalhe = texto(b"T").or_else(|| texto(b"Alt")).or_else(|| texto(b"ActualText")).unwrap_or_default();
                if let Some(n) = d.get(b"Pg").ok().and_then(|p| p.as_reference().ok()).and_then(|p| paginas.get(&p)) {
                    if !detalhe.is_empty() { detalhe.push_str(" · "); }
                    detalhe.push_str(&format!("p. {n}"));
                }
                saida.push(NoEstrutura { nivel, tipo: d.get(b"S").ok().and_then(nome).unwrap_or_default(), detalhe });
                if let Ok(k) = d.get(b"K") { elemento(doc, k, nivel + 1, paginas, vistos, saida); }
            }
            // MCID (número) ou referência a conteúdo marcado: folha, não elemento.
            _ => {}
        }
    }
    let raiz = doc.catalog().ok()?.get(b"StructTreeRoot").ok().and_then(|r| dicionario(doc, r))?;
    let mut saida = Vec::new();
    if let Ok(k) = raiz.get(b"K") { elemento(doc, k, 0, paginas, &mut HashSet::new(), &mut saida); }
    Some(saida)
}

impl DocumentoPdf {
    /// Lê fontes, imagens, estrutura e problemas. Percorre o conteúdo das páginas:
    /// chamar quando o documento muda, não a cada quadro.
    pub fn inspecionar(&self) -> Inspecao {
        let doc = &self.documento;
        let paginas = numeros_das_paginas(doc);
        let resumo = Resumo {
            versao: doc.version.clone(),
            paginas: paginas.len() as u32,
            objetos: doc.objects.len(),
            xref_comprimido: matches!(doc.reference_table.cross_reference_type, lopdf::xref::XrefType::CrossReferenceStream),
        };
        let mut problemas = Vec::new();
        problemas.extend(referencias_quebradas(doc));
        let fontes = ler_fontes(doc, &mut problemas);
        let imagens = ler_imagens(doc, &mut problemas);
        problemas.sort_by_key(|p| p.gravidade == Gravidade::Aviso);
        Inspecao { resumo, fontes, imagens, estrutura: ler_estrutura(doc, &paginas), problemas }
    }

    /// Linhas visíveis da árvore: a partir do trailer, ou, com `filtro`, os objetos
    /// cujo texto contém o filtro ("/Type /Font").
    pub fn arvore_objetos(&self, abertos: &HashSet<String>, filtro: &str) -> Vec<LinhaArvore> {
        let doc = &self.documento;
        let mut arvore = Arvore { doc, paginas: numeros_das_paginas(doc), abertos, ancestrais: Vec::new(), saida: Vec::new() };
        let filtro = normalizar(filtro);
        if filtro.is_empty() {
            for k in RAIZES {
                if let Ok(v) = doc.trailer.get(k) { arvore.ramo(0, texto_nome(k), format!("/{}", texto_nome(k)), v); }
            }
        } else {
            for (id, o) in &doc.objects {
                if arvore.saida.len() >= MAX_LINHAS_ARVORE { break; }
                let mut t = String::new();
                em_linha(o, &mut t);
                if normalizar(&t).contains(&filtro) {
                    arvore.ramo(0, format!("#{} {}", id.0, id.1), String::new(), &Object::Reference(*id));
                }
            }
        }
        arvore.saida
    }

    /// Caminhos a abrir na árvore para o objeto `alvo` aparecer (o menor caminho a partir do trailer).
    pub fn caminho_ate(&self, alvo: ObjectId) -> Option<Vec<String>> {
        let doc = &self.documento;
        let mut fila: VecDeque<(String, &Object)> = RAIZES.iter()
            .filter_map(|k| Some((texto_nome(k), doc.trailer.get(k).ok()?))).collect();
        let mut vistos = HashSet::new();
        while let Some((caminho, valor)) = fila.pop_front() {
            if let Ok(r) = valor.as_reference() {
                if r == alvo {
                    let partes: Vec<&str> = caminho.split('/').collect();
                    return Some((1..partes.len()).map(|i| partes[..i].join("/")).collect());
                }
                if !vistos.insert(r) || vistos.len() > 200_000 { continue; }
            }
            let Some(resolvido) = resolver(doc, valor) else { continue };
            for (parte, _, v) in filhos(resolvido) { fila.push_back((format!("{caminho}/{parte}"), v)); }
        }
        None
    }

    /// Id do objeto de uma página (para o inspetor abrir nela).
    pub fn objeto_da_pagina(&self, pagina: u32) -> Option<ObjectId> { self.id_pagina(pagina).ok() }

    /// O objeto em sintaxe PDF, com cores, comentários e o começo do fluxo de conteúdo.
    pub fn detalhe_objeto(&self, id: ObjectId) -> Option<DetalheObjeto> {
        let doc = &self.documento;
        let o = doc.get_object(id).ok()?;
        let paginas = numeros_das_paginas(doc);
        let tipo = match o {
            Object::Stream(s) => tipo_de(&s.dict).or_else(|| subtipo(&s.dict)).unwrap_or_else(|| "Stream".into()),
            Object::Dictionary(d) => tipo_de(d).unwrap_or_else(|| "Dicionário".into()),
            Object::Array(_) => "Matriz".into(),
            _ => "Valor".into(),
        };
        let mut e = Escritor { doc, paginas: &paginas, linhas: Vec::new(), atual: Vec::new(), cortado: false };
        e.valor(o, 0);
        if let Object::Stream(s) = o {
            e.quebra();
            e.por(format!("stream … endstream   % {} gravados", tamanho(s.content.len())), Cor::Comentario);
        }
        e.quebra();
        if e.cortado { e.linhas.push(vec![Pedaco { texto: "% … cortado aqui".into(), cor: Cor::Comentario }]); }
        let fluxo = match o {
            Object::Stream(s) => Some((id, previa(s))),
            Object::Dictionary(d) if tipo_de(d).as_deref() == Some("Page") => {
                let conteudo = d.get(b"Contents").ok()?;
                let primeiro = match resolver(doc, conteudo) { Some(Object::Array(a)) => a.first(), _ => Some(conteudo) };
                primeiro.and_then(|c| c.as_reference().ok())
                    .and_then(|c| Some((c, previa(doc.get_object(c).ok()?.as_stream().ok()?))))
            }
            _ => None,
        };
        Some(DetalheObjeto { id, tipo, pagina: paginas.get(&id).copied(), linhas: e.linhas, fluxo })
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes::pdf_teste;
    use lopdf::dictionary;

    /// Página 1 com uma fonte simbólica embutida sem ToUnicode (F2) e uma imagem de
    /// 600×600 px desenhada em 2 polegadas (300 dpi).
    fn pdf_com_fonte_e_imagem() -> (DocumentoPdf, ObjectId, ObjectId) {
        let mut pdf = pdf_teste();
        let doc = &mut pdf.documento;
        let programa = doc.add_object(Stream::new(dictionary! {}, vec![1, 2, 3]));
        let descritor = doc.add_object(dictionary! { "Type" => "FontDescriptor", "Flags" => 4, "FontFile2" => programa });
        let fonte = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "ABCDEF+Arial", "FontDescriptor" => descritor });
        let imagem = doc.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 600, "Height" => 600,
            "BitsPerComponent" => 8, "ColorSpace" => "DeviceGray" }, vec![0; 360_000]));
        let conteudo = doc.add_object(Stream::new(dictionary! {}, b"q 144 0 0 144 0 0 cm /Im1 Do Q".to_vec()));
        let pagina = pdf.id_pagina(1).unwrap();
        let p = pdf.documento.get_dictionary_mut(pagina).unwrap();
        p.set("Resources", dictionary! { "Font" => dictionary! { "F2" => fonte }, "XObject" => dictionary! { "Im1" => imagem } });
        p.set("Contents", conteudo);
        (pdf, fonte, imagem)
    }

    #[test]
    fn arvore_abre_do_trailer_ate_a_pagina() {
        let pdf = pdf_teste();
        let fechada = pdf.arvore_objetos(&HashSet::new(), "");
        assert!(fechada.iter().any(|l| l.chave == "/Root" && l.expansivel && l.dica == "Catalog"));
        let pagina = pdf.objeto_da_pagina(1).unwrap();
        let abertos: HashSet<String> = pdf.caminho_ate(pagina).unwrap().into_iter().collect();
        assert!(abertos.contains("Root") && abertos.contains("Root/Pages"));
        let linhas = pdf.arvore_objetos(&abertos, "");
        let achada = linhas.iter().find(|l| l.referencia == Some(pagina)).unwrap();
        assert_eq!(achada.dica, "Page 1");
        // O /Parent da página aponta para cima: não abre de novo.
        let mut todos = abertos.clone();
        todos.insert(achada.caminho.clone());
        let pai = pdf.arvore_objetos(&todos, "").into_iter().find(|l| l.caminho.ends_with("/Parent")).unwrap();
        assert!(!pai.expansivel);
    }

    #[test]
    fn fonte_sem_tounicode_e_imagem_pesada_viram_problema() {
        let (pdf, fonte, imagem) = pdf_com_fonte_e_imagem();
        let i = pdf.inspecionar();
        assert_eq!(i.resumo.paginas, 1);
        let f = i.fontes.iter().find(|f| f.id == fonte).unwrap();
        assert_eq!((f.recurso.as_str(), f.embutida, f.subconjunto, f.to_unicode, f.paginas.as_slice()), ("F2", true, true, false, &[1][..]));
        let im = i.imagens.iter().find(|m| m.id == imagem).unwrap();
        assert!((im.dpi.unwrap() - 300.0).abs() < 1.0, "{:?}", im.dpi);
        assert_eq!(im.paginas, [1]);
        assert!(i.problemas.iter().any(|p| p.titulo == "Fonte F2 sem ToUnicode"));
        assert!(i.problemas.iter().any(|p| p.objeto == Some(imagem) && p.titulo.contains("300 dpi")));
        assert!(i.estrutura.is_none());
    }

    #[test]
    fn previa_do_fluxo_fica_em_ascii() {
        let s = Stream::new(lopdf::Dictionary::new(), b"BT (caf\xe9) Tj ET".to_vec());
        assert_eq!(previa(&s), "BT (caf\\351) Tj ET");
    }

    #[test]
    fn detalhe_da_pagina_tem_referencias_comentarios_e_fluxo() {
        let (pdf, fonte, imagem) = pdf_com_fonte_e_imagem();
        let pagina = pdf.objeto_da_pagina(1).unwrap();
        let d = pdf.detalhe_objeto(pagina).unwrap();
        assert_eq!((d.tipo.as_str(), d.pagina), ("Page", Some(1)));
        let pedacos: Vec<&Pedaco> = d.linhas.iter().flatten().collect();
        assert!(pedacos.iter().any(|p| p.cor == Cor::Referencia(imagem)));
        assert!(pedacos.iter().any(|p| p.cor == Cor::Comentario && p.texto.contains("A4")));
        assert!(pedacos.iter().any(|p| p.cor == Cor::Comentario && p.texto.contains("600×600")));
        assert!(d.fluxo.as_ref().unwrap().1.contains("/Im1 Do"));
        assert!(d.texto().contains("/Type /Page"));
        // O filtro acha a fonte pelo texto do dicionário.
        let achados = pdf.arvore_objetos(&HashSet::new(), "/type  /FONT");
        assert!(achados.iter().any(|l| l.referencia == Some(fonte)));
    }

    #[test]
    fn estrutura_lista_os_elementos() {
        let mut pdf = pdf_teste();
        let pagina = pdf.objeto_da_pagina(1).unwrap();
        let doc = &mut pdf.documento;
        let p = doc.add_object(dictionary! { "Type" => "StructElem", "S" => "P", "Pg" => pagina, "K" => 0 });
        let raiz = doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Document", "K" => vec![Object::Reference(p)] });
        let arvore = doc.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => raiz });
        doc.catalog_mut().unwrap().set("StructTreeRoot", arvore);
        let nos = pdf.inspecionar().estrutura.unwrap();
        assert_eq!(nos.iter().map(|n| (n.nivel, n.tipo.as_str())).collect::<Vec<_>>(), [(0, "Document"), (1, "P")]);
        assert_eq!(nos[1].detalhe, "p. 1");
    }
}
