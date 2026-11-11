//! Comprimir e proteger com senha. As duas devolvem os bytes de uma cópia;
//! o documento aberto não muda.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, Object, ObjectId, Permissions, SaveOptions};

use crate::objetos::{mult, Matriz};
use crate::DocumentoPdf;

/// Onde estão os bytes do arquivo (para "Onde está o peso").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Peso { pub imagens: u64, pub fontes: u64, pub resto: u64 }

/// Imagem lida do PDF para o adaptador reamostrar: `dados` é o JPEG cru
/// (`jpeg`) ou os pixels de 8 bits, `componentes` por pixel (1 ou 3).
pub struct ImagemCrua<'a> { pub largura: u32, pub altura: u32, pub componentes: u8, pub jpeg: bool, pub dados: &'a [u8] }

/// Reamostra a imagem para caber em `lado` px no lado maior (sem aumentar) e grava em JPEG
/// com a `qualidade` (1–100); devolve o JPEG e o tamanho dele.
pub type Reamostrar<'a> = &'a dyn Fn(ImagemCrua<'_>, u32, u8) -> Option<(Vec<u8>, u32, u32)>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpcoesCompressao {
    /// Resolução das imagens no tamanho em que aparecem na página (`None`: não mexe nelas).
    pub dpi: Option<f32>,
    /// Qualidade do JPEG regravado (1–100).
    pub qualidade: u8,
    /// Miniaturas, dados privados de editor (PieceInfo) e streams vazios.
    pub limpar: bool,
    pub anexos: bool,
    /// Recomprime os streams sem perda com o zopfli: ~8% menor, bem mais lento.
    pub zopfli: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissoes { pub imprimir: bool, pub alta_qualidade: bool, pub copiar: bool, pub preencher: bool, pub comentar: bool, pub editar: bool }

impl Permissoes {
    fn bits(self) -> Permissions {
        let mut p = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        for (sim, bit) in [
            (self.imprimir, Permissions::PRINTABLE), (self.imprimir && self.alta_qualidade, Permissions::PRINTABLE_IN_HIGH_QUALITY),
            (self.copiar, Permissions::COPYABLE), (self.preencher, Permissions::FILLABLE),
            (self.comentar, Permissions::ANNOTABLE), (self.editar, Permissions::MODIFIABLE | Permissions::ASSEMBLABLE),
        ] { if sim { p |= bit; } }
        p
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protecao {
    /// Vazia: abre sem senha (só as permissões valem).
    pub senha_abrir: String,
    /// O que fica liberado e a senha de quem pode mudar isso.
    pub limitar: Option<(Permissoes, String)>,
    /// AES-256 (PDF 2.0); `false` usa AES-128 para leitores antigos.
    pub aes256: bool,
}

fn aleatorio<const N: usize>() -> Result<[u8; N], String> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| format!("Sem fonte de números aleatórios: {e}"))?;
    Ok(b)
}

fn gravar(doc: &mut Document) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let opcoes = SaveOptions::builder().use_object_streams(true).use_xref_streams(true).compression_level(9).build();
    doc.save_with_options(&mut bytes, opcoes).map_err(|e| format!("Falha ao gravar o PDF: {e}"))?;
    Ok(bytes)
}

pub(crate) fn eh_imagem(o: &Object) -> bool {
    matches!(o, Object::Stream(s) if s.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image"))
}

/// Ids dos programas de fonte (FontFile, FontFile2, FontFile3) citados pelos descritores.
fn programas_de_fonte(doc: &Document) -> HashSet<ObjectId> {
    doc.objects.values().filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"FontDescriptor"))
        .flat_map(|d| [&b"FontFile"[..], b"FontFile2", b"FontFile3"].into_iter().filter_map(|k| d.get(k).and_then(Object::as_reference).ok()))
        .collect()
}

/// Troca as referências por `mapa` em todo o objeto.
pub(crate) fn trocar_refs(o: &mut Object, mapa: &HashMap<ObjectId, ObjectId>) {
    trocar_refs_ate(o, mapa, 0);
}

/// O objeto vem do arquivo: aninhamento sem teto estourava a pilha (PDF feito para isso).
fn trocar_refs_ate(o: &mut Object, mapa: &HashMap<ObjectId, ObjectId>, fundo: u32) {
    if fundo > 256 { return; }
    match o {
        Object::Reference(id) => if let Some(n) = mapa.get(id) { *id = *n },
        Object::Array(a) => a.iter_mut().for_each(|x| trocar_refs_ate(x, mapa, fundo + 1)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| trocar_refs_ate(x, mapa, fundo + 1)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| trocar_refs_ate(x, mapa, fundo + 1)),
        _ => {}
    }
}

/// Streams iguais (mesmo dicionário e mesmo conteúdo) viram um só: a mesma
/// logo repetida em cada página costuma vir duplicada.
fn juntar_iguais(doc: &mut Document) -> usize {
    let mut vistos: HashMap<(String, &[u8]), ObjectId> = HashMap::new();
    let mut mapa = HashMap::new();
    for (id, o) in &doc.objects {
        if let Object::Stream(s) = o {
            let chave = (format!("{:?}", s.dict), s.content.as_slice());
            match vistos.get(&chave) { Some(&primeiro) => { mapa.insert(*id, primeiro); } None => { vistos.insert(chave, *id); } }
        }
    }
    if mapa.is_empty() { return 0; }
    for id in mapa.keys() { doc.objects.remove(id); }
    for o in doc.objects.values_mut() { trocar_refs(o, &mapa); }
    trocar_refs_no_trailer(doc, &mapa);
    mapa.len()
}

fn trocar_refs_no_trailer(doc: &mut Document, mapa: &HashMap<ObjectId, ObjectId>) {
    for (_, x) in doc.trailer.iter_mut() { trocar_refs(x, mapa); }
}

/// Componentes por pixel do espaço de cor, quando dá para reamostrar.
fn componentes(doc: &Document, cs: &Object) -> Option<u8> {
    match cs {
        Object::Name(n) if n == b"DeviceRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" => Some(1),
        Object::Reference(id) => componentes(doc, doc.get_object(*id).ok()?),
        Object::Array(a) if a.first().and_then(|x| x.as_name().ok()) == Some(b"ICCBased") => {
            let perfil = doc.get_object(a.get(1)?.as_reference().ok()?).ok()?.as_stream().ok()?;
            match perfil.dict.get(b"N").and_then(Object::as_i64).ok()? { 1 => Some(1), 3 => Some(3), _ => None }
        }
        _ => None,
    }
}

/// Imagem que dá para decodificar: largura, altura, componentes, se é JPEG
/// e os dados (o JPEG cru ou os pixels de 8 bits).
pub(crate) fn ler_imagem(doc: &Document, id: ObjectId) -> Option<(u32, u32, u8, bool, Vec<u8>)> {
    let s = doc.get_object(id).and_then(Object::as_stream).ok()?;
    let d = &s.dict;
    let num = |k: &[u8]| d.get(k).and_then(Object::as_i64).ok();
    let (w, h) = (num(b"Width")?, num(b"Height")?);
    // Máscara de cor, Decode e máscara de bits não sobrevivem ao JPEG.
    if num(b"BitsPerComponent") != Some(8) || d.has(b"Mask") || d.has(b"Decode")
        || d.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false) || w <= 0 || h <= 0 { return None; }
    let comp = componentes(doc, d.get(b"ColorSpace").ok()?)?;
    let filtros = s.filters().unwrap_or_default();
    if filtros == [&b"DCTDecode"[..]] { return Some((w as u32, h as u32, comp, true, s.content.clone())); }
    if !filtros.iter().all(|f| *f == b"FlateDecode") { return None; }
    let (w, h) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    let esperado = (w as u64).checked_mul(h as u64)?.checked_mul(comp as u64)?;
    if esperado > TETO_IMAGEM { return None; }
    // Descomprime com teto antes de confiar no fluxo: 1 KB podia inflar para gigabytes.
    // O preditor (DecodeParms) põe um byte por linha a mais; o lopdf o desfaz depois.
    let limite = esperado + h as u64 + 1;
    let cru = crate::inflar_com_teto(&s.content, limite)?;
    let pixels = if d.has(b"DecodeParms") { s.decompressed_content().ok()? } else { cru };
    (pixels.len() as u64 == esperado).then_some((w, h, comp, false, pixels))
}

/// O maior bitmap de imagem que se descomprime (400 MB: uma foto de 10.000 × 13.000 em RGB).
const TETO_IMAGEM: u64 = 400 << 20;

pub(crate) fn trocar_por_jpeg(doc: &mut Document, id: ObjectId, jpeg: Vec<u8>, largura: u32, altura: u32) {
    let Ok(s) = doc.get_object_mut(id).and_then(Object::as_stream_mut) else { return };
    s.dict.remove(b"DecodeParms");
    s.dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    s.dict.set("Width", largura as i64);
    s.dict.set("Height", altura as i64);
    s.allows_compression = false;
    s.set_content(jpeg);
}

fn matriz_4(o: &Object) -> Option<[f32; 4]> {
    let v: Vec<f32> = o.as_array().ok()?.iter().filter_map(|x| x.as_float().ok()).collect();
    (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
}

fn matriz(o: &Object) -> Option<Matriz> {
    let v: Vec<f32> = o.as_array().ok()?.iter().filter_map(|x| x.as_float().ok()).collect();
    (v.len() == 6).then(|| [v[0], v[1], v[2], v[3], v[4], v[5]])
}

/// Nome -> id dos XObjects de um dicionário /Resources.
fn xobjetos(doc: &Document, recursos: Option<&Object>) -> HashMap<Vec<u8>, ObjectId> {
    let dict = |o: &Object| match o { Object::Reference(r) => doc.get_dictionary(*r).ok().cloned(), o => o.as_dict().ok().cloned() };
    recursos.and_then(dict).and_then(|r| r.get(b"XObject").ok().and_then(dict))
        .map(|x| x.iter().filter_map(|(k, v)| Some((k.clone(), v.as_reference().ok()?))).collect()).unwrap_or_default()
}

/// Maior tamanho (pt) em que cada imagem aparece e as páginas onde aparece,
/// percorrendo o conteúdo das páginas e dos formulários.
pub(crate) fn exibicoes(doc: &Document) -> HashMap<ObjectId, ((f32, f32), BTreeSet<u32>)> {
    type Saida = HashMap<ObjectId, ((f32, f32), BTreeSet<u32>)>;
    fn percorrer(doc: &Document, conteudo: &[u8], recursos: HashMap<Vec<u8>, ObjectId>, base: Matriz,
                 pagina: u32, saida: &mut Saida, fundo: usize, orcamento: &mut u32) {
        let Ok(c) = lopdf::content::Content::decode(conteudo) else { return };
        let (mut ctm, mut pilha) = (base, Vec::new());
        for op in &c.operations {
            match op.operator.as_str() {
                "q" => pilha.push(ctm),
                "Q" => ctm = pilha.pop().unwrap_or(base),
                "cm" => if let Some(m) = matriz(&Object::Array(op.operands.clone())) { ctm = mult(m, ctm) },
                "Do" => {
                    let Some(&id) = op.operands.first().and_then(|n| n.as_name().ok()).and_then(|n| recursos.get(n)) else { continue };
                    let Ok(s) = doc.get_object(id).and_then(Object::as_stream) else { continue };
                    match s.dict.get(b"Subtype").and_then(Object::as_name) {
                        Ok(b"Image") => {
                            let t = (ctm[0].hypot(ctm[1]), ctm[2].hypot(ctm[3]));
                            let e = saida.entry(id).or_default();
                            e.0 = (e.0 .0.max(t.0), e.0 .1.max(t.1));
                            e.1.insert(pagina);
                        }
                        // O orçamento corta o formulário que se desenha várias vezes a si mesmo (cresce exponencial).
                        Ok(b"Form") if fundo < 12 && *orcamento > 0 => {
                            *orcamento -= 1;
                            let m = s.dict.get(b"Matrix").ok().and_then(matriz).unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                            let proprios = xobjetos(doc, s.dict.get(b"Resources").ok());
                            let recursos = if proprios.is_empty() { recursos.clone() } else { proprios };
                            let dados = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
                            percorrer(doc, &dados, recursos, mult(m, ctm), pagina, saida, fundo + 1, orcamento);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    let mut saida = Saida::new();
    let mut orcamento = 10_000;
    for (pagina, id) in doc.get_pages() {
        let (recursos, herdados) = doc.get_page_resources(id).unwrap_or((None, vec![]));
        let mut nomes = xobjetos(doc, recursos.map(|r| Object::Dictionary(r.clone())).as_ref());
        for h in herdados { for (k, v) in xobjetos(doc, Some(&Object::Reference(h))) { nomes.entry(k).or_insert(v); } }
        let conteudo = doc.get_page_content(id);
        percorrer(doc, &conteudo, nomes, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], pagina, &mut saida, 0, &mut orcamento);
    }
    // A máscara de transparência aparece onde a imagem dona dela aparece.
    let mascaras: Vec<(ObjectId, ((f32, f32), BTreeSet<u32>))> = saida.iter().filter_map(|(id, t)| {
        let m = doc.get_object(*id).and_then(Object::as_stream).ok()?.dict.get(b"SMask").ok()?.as_reference().ok()?;
        Some((m, t.clone()))
    }).collect();
    for (m, t) in mascaras { saida.entry(m).or_insert(t); }
    saida
}

/// Reamostra as imagens que passam de 1,2 × `dpi` no tamanho em que aparecem (o Ghostscript usa 1,5)
/// e regrava em JPEG as demais quando isso as deixa ao menos 10% menores.
fn reduzir_imagens(doc: &mut Document, dpi: f32, qualidade: u8, reamostrar: Reamostrar<'_>) -> usize {
    let exibidas: HashMap<ObjectId, (f32, f32)> = exibicoes(doc).into_iter().map(|(id, (t, _))| (id, t)).collect();
    // Imagem fora do conteúdo das páginas (aparência de anotação, padrão): limite pela página.
    let pagina = doc.get_pages().into_values().filter_map(|id| doc.get_dictionary(id).ok()?.get(b"MediaBox").ok().and_then(matriz_4))
        .map(|c| (c[2] - c[0]).abs().max((c[3] - c[1]).abs())).fold(0.0f32, f32::max).max(612.0);
    let ids: Vec<ObjectId> = doc.objects.iter().filter(|(_, o)| eh_imagem(o)).map(|(id, _)| *id).collect();
    let mut trocadas = 0;
    for id in ids {
        let Some((w, h, componentes, jpeg, dados)) = ler_imagem(doc, id) else { continue };
        // Imagem que não aparece em lugar nenhum (ou só em padrão/anotação): só regrava, sem reduzir.
        let alvo = exibidas.get(&id).map(|&(l, a)| ((l / 72.0 * dpi).max(a / 72.0 * dpi / h as f32 * w as f32)) as u32);
        let lado = match alvo {
            Some(alvo) if (w as f32) > alvo as f32 * 1.2 => (alvo.max(16) as f32 * w.max(h) as f32 / w as f32) as u32,
            Some(_) => w.max(h),
            None => w.max(h).min((pagina / 72.0 * dpi) as u32),
        };
        let antes = doc.get_object(id).and_then(Object::as_stream).map_or(0, |s| s.content.len());
        let Some((novo, nw, nh)) = reamostrar(ImagemCrua { largura: w, altura: h, componentes, jpeg, dados: &dados }, lado, qualidade) else { continue };
        if novo.len() as f32 > antes as f32 * 0.9 { continue; }
        trocar_por_jpeg(doc, id, novo, nw, nh);
        trocadas += 1;
    }
    trocadas
}

/// Recomprime com o zopfli os streams que não são imagem (conteúdo de página, formas, fontes),
/// os maiores primeiro, uma thread por núcleo. Fica só o que encolheu.
fn recomprimir_zopfli(doc: &mut Document) {
    let mut fila: Vec<(ObjectId, Vec<u8>, usize)> = doc.objects.iter().filter_map(|(id, o)| {
        let Object::Stream(s) = o else { return None };
        if eh_imagem(o) || s.dict.has(b"DecodeParms") { return None; }
        let filtros = s.filters().unwrap_or_default();
        let cru = match filtros.as_slice() {
            [] => s.content.clone(),
            [f] if *f == b"FlateDecode" => s.decompressed_content().ok()?,
            _ => return None,
        };
        (cru.len() > 64).then_some((*id, cru, s.content.len()))
    }).collect();
    fila.sort_by_key(|(_, cru, _)| std::cmp::Reverse(cru.len()));
    let proximo = std::sync::atomic::AtomicUsize::new(0);
    let nucleos = std::thread::available_parallelism().map_or(4, |n| n.get());
    let prontos: Vec<(ObjectId, Vec<u8>)> = std::thread::scope(|e| {
        let tarefas: Vec<_> = (0..nucleos).map(|_| e.spawn(|| {
            let mut meus = Vec::new();
            loop {
                let i = proximo.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some((id, cru, antes)) = fila.get(i) else { break };
                let mut saida = Vec::new();
                if zopfli::compress(zopfli::Options::default(), zopfli::Format::Zlib, &cru[..], &mut saida).is_ok() && saida.len() < *antes {
                    meus.push((*id, saida));
                }
            }
            meus
        })).collect();
        tarefas.into_iter().flat_map(|t| t.join().unwrap_or_default()).collect()
    });
    for (id, z) in prontos {
        if let Ok(s) = doc.get_object_mut(id).and_then(Object::as_stream_mut) {
            s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
            s.allows_compression = false;
            s.set_content(z);
        }
    }
}

/// Tira os arquivos anexados (a árvore EmbeddedFiles e as anotações de anexo).
pub(crate) fn tirar_anexos(doc: &mut Document) {
    if let Ok(nomes) = doc.catalog().and_then(|c| c.get(b"Names")).cloned() {
        let nomes = match nomes { Object::Reference(id) => doc.get_dictionary_mut(id).ok(), _ => doc.catalog_mut().ok().and_then(|c| c.get_mut(b"Names").ok()).and_then(|n| n.as_dict_mut().ok()) };
        if let Some(n) = nomes { n.remove(b"EmbeddedFiles"); }
    }
    let anexo = |o: &Object| o.as_dict().ok().and_then(|d| d.get(b"Subtype").ok()).and_then(|s| s.as_name().ok()) == Some(b"FileAttachment");
    for id in doc.get_pages().into_values() {
        // A lista pode vir por referência (`/Annots 12 0 R`).
        let anotacoes = match doc.get_dictionary(id).and_then(|p| p.get(b"Annots")) {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(r)) => match doc.get_object(*r) { Ok(Object::Array(a)) => a.clone(), _ => continue },
            _ => continue,
        };
        let ficam: Vec<Object> = anotacoes.into_iter().filter(|a| match a {
            Object::Reference(r) => !doc.get_object(*r).is_ok_and(anexo), o => !anexo(o),
        }).collect();
        if let Ok(p) = doc.get_dictionary_mut(id) { p.set("Annots", ficam); }
    }
}

/// Foto tirada de dentro do PDF, como está no arquivo: o JPEG cru (`jpeg`) ou os pixels de
/// 8 bits (`componentes` 1 = cinza, 3 = RGB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagemExtraida {
    /// Primeira página (das pedidas) em que aparece.
    pub pagina: u32,
    pub largura: u32,
    pub altura: u32,
    pub componentes: u8,
    pub jpeg: bool,
    pub dados: Vec<u8>,
}

impl DocumentoPdf {
    /// As fotos que aparecem nas `paginas` (vazio = todas), cada uma uma vez, na ordem das
    /// páginas; e quantas ficaram de fora por estarem num formato que ainda não sai
    /// (máscara de bits, paleta, 1 bit, CMYK sem JPEG, JPEG 2000, fax).
    /// shortcut: a máscara de transparência (/SMask) não acompanha a foto; juntar em PNG com alfa se pedirem.
    /// As fotos das `paginas` (vazio = todas), cada uma uma vez, na página em que aparece
    /// primeiro; a máscara de transparência não conta. Só a lista: ler é com [`Self::extrair_foto`],
    /// uma por vez (um escaneado inteiro lido de uma vez passava de vários GB).
    pub fn fotos_para_extrair(&self, paginas: &[u32]) -> Vec<(u32, ObjectId)> {
        let doc = &self.documento;
        let exibidas = exibicoes(doc);
        let mascaras: HashSet<ObjectId> = exibidas.keys().filter_map(|id| {
            doc.get_object(*id).and_then(Object::as_stream).ok()?.dict.get(b"SMask").ok()?.as_reference().ok()
        }).collect();
        let mut achadas: Vec<(u32, ObjectId)> = exibidas.iter().filter(|(id, _)| !mascaras.contains(*id))
            .filter_map(|(id, (_, pags))| pags.iter().copied().find(|p| paginas.is_empty() || paginas.contains(p)).map(|p| (p, *id)))
            .collect();
        achadas.sort();
        achadas
    }

    /// Lê uma foto da lista de [`Self::fotos_para_extrair`]. `None`: formato que ainda não sai.
    pub fn extrair_foto(&self, pagina: u32, id: ObjectId) -> Option<ImagemExtraida> {
        let doc = &self.documento;
        let s = doc.get_object(id).and_then(Object::as_stream).ok()?;
        let num = |k: &[u8]| s.dict.get(k).and_then(Object::as_i64).ok().and_then(|n| u32::try_from(n).ok());
        // JPEG sai como está, em qualquer espaço de cor: é o arquivo original da foto.
        if s.filters().unwrap_or_default() == [&b"DCTDecode"[..]] {
            if let (Some(l), Some(a)) = (num(b"Width"), num(b"Height")) {
                return Some(ImagemExtraida { pagina, largura: l, altura: a, componentes: 3, jpeg: true, dados: s.content.clone() });
            }
        }
        let (largura, altura, componentes, jpeg, dados) = ler_imagem(doc, id)?;
        Some(ImagemExtraida { pagina, largura, altura, componentes, jpeg, dados })
    }

    /// Passa `clarear` nas imagens que aparecem nas `paginas` (escaneado com papel cinza) e
    /// devolve quantas mudaram. A imagem que também aparece em outra página muda lá também.
    pub fn limpar_fundo(&mut self, paginas: &[u32], clarear: &dyn Fn(ImagemCrua<'_>) -> Option<(Vec<u8>, u32, u32)>) -> usize {
        let ids: Vec<ObjectId> = exibicoes(&self.documento).into_iter()
            .filter(|(_, (_, pags))| paginas.iter().any(|p| pags.contains(p)))
            .map(|(id, _)| id).collect();
        let mut n = 0;
        for id in ids {
            let Some((w, h, componentes, jpeg, dados)) = ler_imagem(&self.documento, id) else { continue };
            let Some((novo, nw, nh)) = clarear(ImagemCrua { largura: w, altura: h, componentes, jpeg, dados: &dados }) else { continue };
            trocar_por_jpeg(&mut self.documento, id, novo, nw, nh);
            n += 1;
        }
        if n > 0 { self.modificado = true; }
        n
    }

    /// Bytes de imagem, de fonte e o resto, dentro de um arquivo de `total` bytes.
    pub fn peso(&self, total: u64) -> Peso {
        let fontes = programas_de_fonte(&self.documento);
        let mut p = Peso::default();
        for (id, o) in &self.documento.objects {
            let Object::Stream(s) = o else { continue };
            let n = s.content.len() as u64;
            if eh_imagem(o) { p.imagens += n } else if fontes.contains(id) { p.fontes += n }
        }
        p.resto = total.saturating_sub(p.imagens + p.fontes);
        p
    }

    /// Cópia menor: imagens reamostradas, streams iguais juntados, tudo
    /// recomprimido e os objetos em streams de objetos.
    pub fn comprimido(&self, opcoes: OpcoesCompressao, reamostrar: Reamostrar<'_>) -> Result<Vec<u8>, String> {
        let mut doc = self.documento.clone();
        if opcoes.limpar {
            // Sem `delete_zero_length_streams`: ele apaga também a referência (/AP /N, /XObject /Fm0) e deixa o PDF inválido.
            for id in doc.get_pages().into_values() {
                if let Ok(p) = doc.get_dictionary_mut(id) { p.remove(b"Thumb"); p.remove(b"PieceInfo"); }
            }
            if let Ok(c) = doc.catalog_mut() { c.remove(b"PieceInfo"); }
        }
        if opcoes.anexos { tirar_anexos(&mut doc); }
        if let Some(dpi) = opcoes.dpi { reduzir_imagens(&mut doc, dpi, opcoes.qualidade, reamostrar); }
        crate::podar(&mut doc);
        juntar_iguais(&mut doc);
        if opcoes.zopfli { recomprimir_zopfli(&mut doc); }
        doc.compress();
        gravar(&mut doc)
    }

    /// Cópia criptografada com senha de abrir e/ou permissões.
    pub fn protegido(&self, p: &Protecao) -> Result<Vec<u8>, String> {
        if p.senha_abrir.is_empty() && p.limitar.is_none() { return Err("Escolha uma senha ou o que limitar.".into()); }
        let mut doc = self.documento.clone();
        if doc.is_encrypted() { return Err("O PDF já está criptografado.".into()); }
        // Trocar a senha: a proteção com que o PDF foi aberto sai, só a nova vale (senão cifra duas vezes).
        doc.encryption_state = None;
        crate::podar(&mut doc);
        if !doc.trailer.has(b"ID") {
            let id = Object::String(aleatorio::<16>()?.to_vec(), lopdf::StringFormat::Hexadecimal);
            doc.trailer.set("ID", vec![id.clone(), id]);
        }
        // Sem limite, a senha do dono é sorteada: ninguém a sabe e tudo fica liberado.
        let sorteada = aleatorio::<16>()?.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let (permissoes, dono) = match &p.limitar {
            Some((perm, dono)) if !dono.is_empty() => (perm.bits(), dono.as_str()),
            Some(_) => return Err("Falta a senha de quem pode mudar as permissões.".into()),
            None => (Permissions::all(), sorteada.as_str()),
        };
        let chave = aleatorio::<32>()?;
        let estado = if p.aes256 {
            let filtro: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
            EncryptionState::try_from(EncryptionVersion::V5 {
                encrypt_metadata: true, crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filtro)]),
                file_encryption_key: &chave, stream_filter: b"StdCF".to_vec(), string_filter: b"StdCF".to_vec(),
                owner_password: dono, user_password: &p.senha_abrir, permissions: permissoes,
            })
        } else {
            let filtro: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
            EncryptionState::try_from(EncryptionVersion::V4 {
                document: &doc, encrypt_metadata: true, crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filtro)]),
                stream_filter: b"StdCF".to_vec(), string_filter: b"StdCF".to_vec(),
                owner_password: dono, user_password: &p.senha_abrir, permissions: permissoes,
            })
        }.map_err(|e| format!("Falha ao preparar a criptografia: {e}"))?;
        doc.compress();
        doc.encrypt(&estado).map_err(|e| format!("Falha ao criptografar: {e}"))?;
        gravar(&mut doc)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exemplo() -> DocumentoPdf {
        let cena = feather_draw::Cena::nova(595.0, 842.0);
        let bytes = feather_io::Registro::padrao().gravar("pdf", &feather_io::Documento::Desenho(cena)).unwrap();
        DocumentoPdf::de_bytes(&bytes, None).unwrap()
    }

    #[test]
    fn extrai_fotos_sem_a_mascara() {
        let mut pdf = exemplo();
        pdf.inserir_em_branco(2).unwrap();
        let rgb = vec![200u8; 16 * 16 * 3];
        let mut alfa = vec![255u8; 16 * 16];
        alfa[0] = 0;
        pdf.inserir_imagem(1, [10.0, 10.0, 110.0, 110.0], crate::ImagemNova::Pixels { largura: 16, altura: 16, rgb: rgb.clone(), alfa: Some(alfa) }).unwrap();
        let jpeg = b"\xFF\xD8 de mentira".to_vec();
        pdf.inserir_imagem(2, [10.0, 10.0, 110.0, 110.0], crate::ImagemNova::Jpeg { bytes: jpeg.clone(), largura: 4, altura: 4, cinza: false }).unwrap();
        let todas: Vec<_> = pdf.fotos_para_extrair(&[]).into_iter().map(|(p, id)| pdf.extrair_foto(p, id).unwrap()).collect();
        assert_eq!(todas.len(), 2, "a máscara não sai como foto");
        assert_eq!((todas[0].pagina, todas[0].jpeg, todas[0].componentes, &todas[0].dados), (1, false, 3, &rgb));
        assert_eq!((todas[1].pagina, todas[1].jpeg, &todas[1].dados), (2, true, &jpeg));
        assert_eq!(pdf.fotos_para_extrair(&[2]).len(), 1);
    }

    #[test]
    fn senha_de_abrir_e_permissoes() {
        let pdf = exemplo();
        for aes256 in [true, false] {
            let perm = Permissoes { imprimir: true, alta_qualidade: false, copiar: false, preencher: true, comentar: false, editar: false };
            let p = Protecao { senha_abrir: "abre".into(), limitar: Some((perm, "dono".into())), aes256 };
            let bytes = pdf.protegido(&p).unwrap();
            let abrir = |senha: &str| Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(senha));
            assert!(abrir("errada").is_err());
            assert!(!abrir("abre").unwrap().get_pages().is_empty());
            let aberto = DocumentoPdf::de_bytes_com_senha(&bytes, None, Some("abre")).unwrap();
            let s = aberto.seguranca().unwrap();
            assert!(s.contains(if aes256 { "AES-256" } else { "128 bits" }) && s.contains("copiar") && !s.contains("imprimir"), "{s}");
            // Trocar a senha de quem já tem: a nova abre, a antiga não.
            let nova = aberto.protegido(&Protecao { senha_abrir: "nova".into(), limitar: None, aes256 }).unwrap();
            assert!(DocumentoPdf::de_bytes_com_senha(&nova, None, Some("nova")).is_ok_and(|d| d.total_paginas() == 1));
            assert!(DocumentoPdf::de_bytes_com_senha(&nova, None, Some("abre")).is_err());
        }
        assert!(pdf.seguranca().is_none());
        assert!(pdf.protegido(&Protecao { senha_abrir: String::new(), limitar: None, aes256: true }).is_err());
    }

    #[test]
    fn comprimir_junta_streams_iguais() {
        let mut pdf = exemplo();
        let imagem = |doc: &mut Document| doc.add_object(lopdf::Stream::new(
            lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2, "BitsPerComponent" => 8, "ColorSpace" => "DeviceGray" },
            vec![0, 255, 255, 0]));
        let (a, b) = (imagem(&mut pdf.documento), imagem(&mut pdf.documento));
        let pagina = pdf.id_pagina(1).unwrap();
        pdf.documento.get_dictionary_mut(pagina).unwrap().set("Teste", vec![Object::Reference(a), Object::Reference(b)]);
        let bytes = pdf.comprimido(OpcoesCompressao { dpi: None, qualidade: 75, limpar: true, anexos: true, zopfli: true }, &|_, _, _| None).unwrap();
        let d = Document::load_mem(&bytes).unwrap();
        let imagens = d.objects.values().filter(|o| eh_imagem(o)).count();
        assert_eq!(imagens, 1);
        assert_eq!(DocumentoPdf::de_bytes(&bytes, None).unwrap().peso(bytes.len() as u64).imagens > 0, true);
    }

    #[test]
    fn zopfli_devolve_o_mesmo_conteudo() {
        let mut pdf = exemplo();
        let conteudo: Vec<u8> = (0..400).flat_map(|i| format!("{i} 0 m {} 10 l S\n", i * 2).into_bytes()).collect();
        let id = pdf.documento.add_object(lopdf::Stream::new(lopdf::dictionary! {}, conteudo.clone()));
        let pagina = pdf.id_pagina(1).unwrap();
        pdf.documento.get_dictionary_mut(pagina).unwrap().set("Teste", Object::Reference(id));
        let bytes = pdf.comprimido(OpcoesCompressao { dpi: None, qualidade: 75, limpar: false, anexos: false, zopfli: true }, &|_, _, _| None).unwrap();
        let d = Document::load_mem(&bytes).unwrap();
        let achou = d.objects.values().filter_map(|o| o.as_stream().ok())
            .any(|s| s.filters().unwrap_or_default() == [&b"FlateDecode"[..]] && s.decompressed_content().ok().as_deref() == Some(&conteudo[..]));
        assert!(achou);
    }

    #[test]
    fn abre_com_senha_mantem_ao_salvar_e_tira() {
        use crate::{DocumentoPdf, PRECISA_SENHA, SENHA_ERRADA};
        let cifrado = crate::testes::pdf_teste().protegido(&Protecao { senha_abrir: "abre".into(), limitar: None, aes256: true }).unwrap();
        assert_eq!(DocumentoPdf::de_bytes(&cifrado, None).unwrap_err(), PRECISA_SENHA);
        assert_eq!(DocumentoPdf::de_bytes_com_senha(&cifrado, None, Some("outra")).unwrap_err(), SENHA_ERRADA);
        let mut pdf = DocumentoPdf::de_bytes_com_senha(&cifrado, None, Some("abre")).unwrap();
        assert!(pdf.tem_senha());
        // Salvo de novo, continua pedindo a mesma senha.
        let salvo = pdf.bytes_para_gravar().unwrap();
        assert_eq!(DocumentoPdf::de_bytes(&salvo, None).unwrap_err(), PRECISA_SENHA);
        assert_eq!(DocumentoPdf::de_bytes_com_senha(&salvo, None, Some("abre")).unwrap().total_paginas(), 1);
        pdf.tirar_senha();
        assert!(!pdf.tem_senha() && pdf.modificado());
        assert!(!DocumentoPdf::de_bytes(&pdf.bytes_para_gravar().unwrap(), None).unwrap().tem_senha());
    }
}
