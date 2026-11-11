//! PDF/A-2b (arquivamento): o que impede, o que dá para corrigir sozinho e a conversão.
//! A verificação cobre as regras que mais reprovam arquivo comum; não substitui um validador
//! completo (veraPDF), e a tela diz isso.

use std::collections::{HashMap, HashSet};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::DocumentoPdf;

/// Como cada regra está no documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao { Ok, Corrige, Impede }

#[derive(Debug, Clone, PartialEq)]
pub struct ItemPdfa { pub regra: &'static str, pub situacao: Situacao, pub detalhe: String }

/// Ações que o PDF/A proíbe (ISO 19005-2, 6.5.1).
const ACOES_PROIBIDAS: &[&[u8]] = &[b"Launch", b"Sound", b"Movie", b"ResetForm", b"ImportData", b"JavaScript", b"Hide", b"SetOCGState", b"Rendition", b"Trans", b"GoTo3DView"];

fn nome<'a>(d: &'a Dictionary, k: &[u8]) -> Option<&'a [u8]> { d.get(k).and_then(Object::as_name).ok() }

fn dicionario<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match o { Object::Reference(id) => doc.get_dictionary(*id).ok(), o => o.as_dict().ok() }
}

/// Fontes do /Resources da página: nome do recurso → id (fontes diretas, sem id, ficam de fora).
fn fontes_da_pagina(doc: &Document, pagina: ObjectId) -> HashMap<Vec<u8>, ObjectId> {
    let mut saida = HashMap::new();
    let Ok((direto, ids)) = doc.get_page_resources(pagina) else { return saida };
    let recursos = direto.into_iter().chain(ids.iter().filter_map(|id| doc.get_dictionary(*id).ok()));
    for r in recursos {
        let Some(fontes) = r.get(b"Font").ok().and_then(|f| dicionario(doc, f)) else { continue };
        for (k, v) in fontes.iter() {
            if let Ok(id) = v.as_reference() { saida.entry(k.clone()).or_insert(id); }
        }
    }
    saida
}

/// Fontes usadas no conteúdo das páginas com texto visível (modo diferente de 3).
/// shortcut: não entra em Form XObjects nem em aparências de anotação; fonte só usada lá conta como visível.
fn fontes_visiveis(doc: &Document) -> (HashSet<ObjectId>, HashSet<ObjectId>) {
    let (mut visiveis, mut vistas) = (HashSet::new(), HashSet::new());
    for pagina in doc.get_pages().into_values() {
        let fontes = fontes_da_pagina(doc, pagina);
        let Ok(c) = lopdf::content::Content::decode(&doc.get_page_content_with_limit(pagina, crate::TETO_FLUXO).unwrap_or_default()) else { continue };
        let (mut fonte, mut modo, mut pilha) = (None::<ObjectId>, 0i64, Vec::new());
        for op in &c.operations {
            match op.operator.as_str() {
                "q" => pilha.push((fonte, modo)),
                "Q" => if let Some((f, m)) = pilha.pop() { fonte = f; modo = m },
                "Tf" => fonte = op.operands.first().and_then(|o| o.as_name().ok()).and_then(|n| fontes.get(n).copied()),
                "Tr" => modo = op.operands.first().and_then(|o| o.as_i64().ok()).unwrap_or(0),
                "Tj" | "TJ" | "'" | "\"" => if let Some(f) = fonte {
                    vistas.insert(f);
                    if modo != 3 { visiveis.insert(f); }
                },
                _ => {}
            }
        }
    }
    (visiveis, vistas)
}

/// Fontes citadas por algum dicionário /Font de recursos (as soltas, que a gravação descarta, não contam).
fn fontes_citadas(doc: &Document) -> HashSet<ObjectId> {
    let mut saida = HashSet::new();
    for o in doc.objects.values() {
        let d = match o { Object::Stream(s) => &s.dict, Object::Dictionary(d) => d, _ => continue };
        // O /Font vem no próprio dicionário de recursos ou no /Resources direto da página/forma.
        let recursos = d.get(b"Resources").ok().and_then(|r| r.as_dict().ok());
        for r in std::iter::once(d).chain(recursos) {
            let Some(fontes) = r.get(b"Font").ok().and_then(|f| dicionario(doc, f)) else { continue };
            saida.extend(fontes.iter().filter_map(|(_, v)| v.as_reference().ok()));
        }
    }
    saida
}

/// O descritor da fonte (no Type0, o da descendente) tem programa embutido.
fn embutida(doc: &Document, fonte: &Dictionary) -> bool {
    if nome(fonte, b"Subtype") == Some(b"Type3") { return true; }
    let alvo = if nome(fonte, b"Subtype") == Some(b"Type0") {
        fonte.get(b"DescendantFonts").and_then(Object::as_array).ok()
            .and_then(|a| a.first()).and_then(|d| dicionario(doc, d))
    } else { Some(fonte) };
    alvo.and_then(|f| f.get(b"FontDescriptor").ok()).and_then(|d| dicionario(doc, d))
        .is_some_and(|d| [&b"FontFile"[..], b"FontFile2", b"FontFile3"].iter().any(|k| d.has(k)))
}

/// Algum ponto do arquivo pinta com CMYK do dispositivo (o perfil de saída que entra é RGB).
fn usa_cmyk(doc: &Document) -> bool {
    let em_espaco = |o: &Object| matches!(o, Object::Name(n) if n == b"DeviceCMYK");
    let nos_objetos = doc.objects.values().any(|o| {
        let d = match o { Object::Stream(s) => &s.dict, Object::Dictionary(d) => d, _ => return false };
        d.get(b"ColorSpace").is_ok_and(em_espaco)
            || d.get(b"ColorSpace").and_then(Object::as_dict).is_ok_and(|cs| cs.iter().any(|(_, v)| em_espaco(v)))
    });
    nos_objetos || doc.get_pages().into_values().any(|p| {
        lopdf::content::Content::decode(&doc.get_page_content_with_limit(p, crate::TETO_FLUXO).unwrap_or_default())
            .is_ok_and(|c| c.operations.iter().any(|op| op.operator == "k" || op.operator == "K"))
    })
}

fn anotacoes(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().filter_map(|p| doc.get_dictionary(p).ok()?.get(b"Annots").ok()?.as_array().ok().cloned())
        .flatten().filter_map(|a| a.as_reference().ok()).collect()
}

/// Ação (ou cadeia /Next) proibida no PDF/A.
fn acao_proibida(doc: &Document, o: &Object) -> bool {
    let Some(d) = dicionario(doc, o) else { return false };
    nome(d, b"S").is_some_and(|s| ACOES_PROIBIDAS.contains(&s)) || d.get(b"Next").is_ok_and(|n| acao_proibida(doc, n))
}

/// "D:20261019143000-03'00'" → "2026-10-19T14:30:00-03:00" (o par que o Info e o XMP precisam ter igual).
fn data_iso(pdf: &str) -> Option<String> {
    let s = pdf.strip_prefix("D:").unwrap_or(pdf);
    let num = |a: usize, b: usize, padrao: &str| s.get(a..b).filter(|t| t.chars().all(|c| c.is_ascii_digit())).unwrap_or(padrao).to_string();
    s.get(0..4).filter(|t| t.chars().all(|c| c.is_ascii_digit()))?;
    let fuso = match s.get(14..15) {
        Some("Z") | None => "Z".to_string(),
        Some(sinal @ ("+" | "-")) => format!("{sinal}{}:{}", num(15, 17, "00"), num(18, 20, "00")),
        _ => return None,
    };
    Some(format!("{}-{}-{}T{}:{}:{}{fuso}", num(0, 4, "0000"), num(4, 6, "01"), num(6, 8, "01"), num(8, 10, "00"), num(10, 12, "00"), num(12, 14, "00")))
}

fn texto_pdf(o: &Object) -> Option<String> {
    let Object::String(b, _) = o else { return None };
    Some(if b.starts_with(&[0xFE, 0xFF]) {
        String::from_utf16_lossy(&b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
    } else { b.iter().map(|&c| c as char).collect() })
}

fn xml(t: &str) -> String { t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

impl DocumentoPdf {
    pub fn verificar_pdfa(&self) -> Vec<ItemPdfa> {
        let doc = &self.documento;
        let mut v = Vec::new();
        let mut item = |regra, situacao, detalhe: String| v.push(ItemPdfa { regra, situacao, detalhe });

        let criptografado = doc.trailer.has(b"Encrypt");
        item("Sem senha nem criptografia", if criptografado { Situacao::Impede } else { Situacao::Ok },
            if criptografado { "Tire a proteção em Proteger antes.".into() } else { String::new() });

        let (visiveis, vistas) = fontes_visiveis(doc);
        let citadas = fontes_citadas(doc);
        let mut faltam: Vec<String> = doc.objects.iter()
            .filter(|(id, _)| citadas.contains(id))
            .filter_map(|(id, o)| Some((id, o.as_dict().ok()?)))
            .filter(|(_, d)| nome(d, b"Type") == Some(b"Font") && nome(d, b"Subtype") != Some(b"CIDFontType0") && nome(d, b"Subtype") != Some(b"CIDFontType2"))
            .filter(|(id, d)| !embutida(doc, d) && (visiveis.contains(id) || !vistas.contains(id)))
            .map(|(_, d)| d.get(b"BaseFont").and_then(Object::as_name).map_or("sem nome".into(), |n| String::from_utf8_lossy(n).into_owned()))
            .collect();
        faltam.sort();
        faltam.dedup();
        item("Todas as fontes embutidas", if faltam.is_empty() { Situacao::Ok } else { Situacao::Impede },
            if faltam.is_empty() { "Texto invisível do OCR não precisa.".into() } else { format!("Não embutidas: {}. Imprimir em PDF de novo, ou OCR com \u{201c}Trocar por texto\u{201d} desligado.", faltam.join(", ")) });

        let cmyk = usa_cmyk(doc);
        item("Cores com perfil", if cmyk { Situacao::Impede } else { Situacao::Corrige },
            if cmyk { "Usa CMYK de impressão sem perfil próprio.".into() } else { "Entra o perfil sRGB como intenção de saída.".into() });

        let lzw = doc.objects.values().any(|o| matches!(o, Object::Stream(s) if s.filters().is_ok_and(|f| f.iter().any(|x| x == b"LZWDecode"))));
        item("Sem compressão LZW", if lzw { Situacao::Corrige } else { Situacao::Ok }, if lzw { "Recomprime em Flate.".into() } else { String::new() });

        let anexos = doc.catalog().ok().and_then(|c| c.get(b"Names").ok()).and_then(|n| dicionario(doc, n)).is_some_and(|n| n.has(b"EmbeddedFiles"))
            || anotacoes(doc).iter().any(|a| doc.get_dictionary(*a).is_ok_and(|d| nome(d, b"Subtype") == Some(b"FileAttachment")));
        item("Sem anexos", if anexos { Situacao::Corrige } else { Situacao::Ok }, if anexos { "Os anexos saem.".into() } else { String::new() });

        let js = doc.catalog().ok().and_then(|c| c.get(b"Names").ok()).and_then(|n| dicionario(doc, n)).is_some_and(|n| n.has(b"JavaScript"))
            || doc.objects.values().any(|o| {
                let Ok(d) = o.as_dict() else { return false };
                d.has(b"AA") || d.get(b"A").is_ok_and(|a| acao_proibida(doc, a)) || d.get(b"OpenAction").is_ok_and(|a| acao_proibida(doc, a))
            });
        item("Sem JavaScript nem ações automáticas", if js { Situacao::Corrige } else { Situacao::Ok }, if js { "Scripts e ações proibidas saem.".into() } else { String::new() });

        let sem_aparencia = anotacoes(doc).iter().filter(|a| doc.get_dictionary(**a).is_ok_and(|d| {
            !matches!(nome(d, b"Subtype"), Some(b"Popup" | b"Link")) && !d.has(b"AP")
                && !d.get(b"Rect").and_then(Object::as_array).is_ok_and(|r| r.len() == 4 && r[0].as_float().ok() == r[2].as_float().ok())
        })).count();
        item("Anotações com aparência própria", if sem_aparencia > 0 { Situacao::Impede } else { Situacao::Ok },
            if sem_aparencia > 0 { format!("{sem_aparencia} anotação(ões) ou campo(s) sem desenho próprio.") } else { String::new() });

        let bandeiras = anotacoes(doc).iter().any(|a| doc.get_dictionary(*a).is_ok_and(|d| {
            nome(d, b"Subtype") != Some(b"Popup") && d.get(b"F").and_then(Object::as_i64).map_or(true, |f| f & 4 == 0 || f & 0b100011 != 0)
        }));
        item("Anotações imprimíveis", if bandeiras { Situacao::Corrige } else { Situacao::Ok }, if bandeiras { "Marca \u{201c}imprimir\u{201d} nelas.".into() } else { String::new() });

        let versao = doc.version.as_str() > "1.7";
        item("Versão até PDF 1.7", if versao { Situacao::Corrige } else { Situacao::Ok }, if versao { format!("Está em {}.", doc.version) } else { String::new() });

        item("Identificação e metadados XMP", Situacao::Corrige, "Grava o XMP de PDF/A-2b e o ID do arquivo.".into());
        v
    }

    /// Converte para PDF/A-2b se nada impedir; `icc` é o perfil sRGB da intenção de saída.
    pub fn converter_pdfa(&mut self, icc: &[u8]) -> Result<(), String> {
        if let Some(i) = self.verificar_pdfa().into_iter().find(|i| i.situacao == Situacao::Impede) {
            return Err(format!("{}: {}", i.regra, i.detalhe));
        }
        let doc = &mut self.documento;
        if doc.version.as_str() > "1.7" { doc.version = "1.7".into(); }

        // LZW → Flate.
        for o in doc.objects.values_mut() {
            if let Object::Stream(s) = o {
                if s.filters().is_ok_and(|f| f.iter().any(|x| x == b"LZWDecode")) {
                    s.decompress().map_err(|e| format!("Fluxo LZW: {e}"))?;
                    s.compress().map_err(|e| format!("Fluxo LZW: {e}"))?;
                }
            }
        }
        crate::otimizar::tirar_anexos(doc);
        // Scripts, ações automáticas e ações proibidas.
        let proibidas: Vec<ObjectId> = doc.objects.iter().filter(|(_, o)| o.as_dict().is_ok_and(|d| nome(d, b"S").is_some_and(|s| ACOES_PROIBIDAS.contains(&s)))).map(|(id, _)| *id).collect();
        for o in doc.objects.values_mut() {
            let d = match o { Object::Dictionary(d) => d, Object::Stream(s) => &mut s.dict, _ => continue };
            d.remove(b"AA");
            for k in [&b"A"[..], b"OpenAction"] {
                let ruim = match d.get(k) {
                    Ok(Object::Reference(id)) => proibidas.contains(id),
                    Ok(Object::Dictionary(a)) => nome(a, b"S").is_some_and(|s| ACOES_PROIBIDAS.contains(&s)),
                    _ => false,
                };
                if ruim { d.remove(k); }
            }
        }
        if let Some(Ok(id)) = doc.catalog().ok().and_then(|c| c.get(b"Names").ok()).map(|n| n.as_reference()) {
            if let Ok(n) = doc.get_dictionary_mut(id) { n.remove(b"JavaScript"); }
        } else if let Ok(n) = doc.catalog_mut().and_then(|c| c.get_mut(b"Names")).and_then(Object::as_dict_mut) {
            n.remove(b"JavaScript");
        }
        if let Some(form) = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).and_then(|f| f.as_reference().ok()) {
            if let Ok(f) = doc.get_dictionary_mut(form) { f.remove(b"NeedAppearances"); }
        } else if let Ok(f) = doc.catalog_mut().and_then(|c| c.get_mut(b"AcroForm")).and_then(Object::as_dict_mut) {
            f.remove(b"NeedAppearances");
        }
        // Anotação imprimível e visível.
        for a in anotacoes(doc) {
            if let Ok(d) = doc.get_dictionary_mut(a) {
                if nome(d, b"Subtype") == Some(b"Popup") { continue; }
                let f = d.get(b"F").and_then(Object::as_i64).unwrap_or(0);
                d.set("F", (f | 4) & !0b100011);
            }
        }
        // Intenção de saída sRGB.
        let perfil = doc.add_object(Stream::new(dictionary! { "N" => 3 }, icc.to_vec()));
        let intencao = doc.add_object(dictionary! {
            "Type" => "OutputIntent", "S" => "GTS_PDFA1", "OutputConditionIdentifier" => Object::string_literal("sRGB IEC61966-2.1"),
            "Info" => Object::string_literal("sRGB IEC61966-2.1"), "DestOutputProfile" => perfil,
        });
        // Info e XMP com os mesmos valores.
        let info_id = doc.trailer.get(b"Info").and_then(Object::as_reference).ok();
        let info = info_id.and_then(|id| doc.get_dictionary(id).ok()).cloned().unwrap_or_default();
        let campo = |k: &[u8]| info.get(k).ok().and_then(texto_pdf).filter(|t| !t.trim().is_empty());
        let (titulo, autor, assunto) = (campo(b"Title"), campo(b"Author"), campo(b"Subject"));
        let criado = campo(b"CreationDate").and_then(|d| data_iso(&d).map(|iso| (d, iso)));
        let (a, m, d, s) = crate::anotacoes::agora();
        let agora_pdf = format!("D:{a:04}{m:02}{d:02}{:02}{:02}{:02}Z", s / 3600, s / 60 % 60, s % 60);
        let agora_iso = data_iso(&agora_pdf).unwrap_or_default();
        let produtor = "FeatherPDF";
        let mut novo = Dictionary::new();
        let mut extra = String::new();
        if let Some(t) = &titulo { novo.set("Title", Object::string_literal(t.as_str())); extra += &format!("<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>", xml(t)); }
        if let Some(t) = &autor { novo.set("Author", Object::string_literal(t.as_str())); extra += &format!("<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>", xml(t)); }
        if let Some(t) = &assunto { novo.set("Subject", Object::string_literal(t.as_str())); extra += &format!("<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>", xml(t)); }
        if let Some((pdf, iso)) = &criado { novo.set("CreationDate", Object::string_literal(pdf.as_str())); extra += &format!("<xmp:CreateDate>{iso}</xmp:CreateDate>"); }
        novo.set("ModDate", Object::string_literal(agora_pdf.as_str()));
        novo.set("Producer", Object::string_literal(produtor));
        // Texto não-ASCII no Info precisa ir em UTF-16 para casar com o XMP.
        for (_, v) in novo.iter_mut() {
            if let Some(t) = texto_pdf(v).filter(|t| !t.is_ascii()) {
                let mut b = vec![0xFE, 0xFF];
                b.extend(t.encode_utf16().flat_map(u16::to_be_bytes));
                *v = Object::String(b, lopdf::StringFormat::Hexadecimal);
            }
        }
        let xmp = format!(concat!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" ",
            "xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">",
            "<pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance><pdf:Producer>{produtor}</pdf:Producer>",
            "<xmp:ModifyDate>{agora}</xmp:ModifyDate>{extra}</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"),
            produtor = produtor, agora = agora_iso, extra = extra);
        let mut fluxo = Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, xmp.into_bytes());
        fluxo.allows_compression = false;
        let metadados = doc.add_object(fluxo);
        let info_novo = doc.add_object(novo);
        doc.trailer.set("Info", info_novo);
        let catalogo = doc.catalog_mut().map_err(|e| e.to_string())?;
        catalogo.set("Metadata", metadados);
        catalogo.set("OutputIntents", vec![Object::Reference(intencao)]);
        // ID do arquivo: dois valores iguais quando não havia.
        if !doc.trailer.has(b"ID") {
            let semente: Vec<u8> = format!("{agora_pdf}{}", doc.objects.len()).bytes().collect();
            let id: Vec<u8> = (0..16).map(|i| semente.iter().enumerate().fold(i as u8, |h, (j, b)| h.wrapping_mul(31).wrapping_add(b ^ j as u8))).collect();
            doc.trailer.set("ID", vec![Object::String(id.clone(), lopdf::StringFormat::Hexadecimal), Object::String(id, lopdf::StringFormat::Hexadecimal)]);
        }
        self.modificado = true;
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Página 1 com "Oi" em Helvetica não embutida, no modo de texto `modo` (3 = invisível).
    fn com_texto(pdf: &mut DocumentoPdf, modo: i64) {
        let doc = &mut pdf.documento;
        let pagina = doc.get_pages()[&1];
        let fonte = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let conteudo = doc.add_object(Stream::new(dictionary! {}, format!("BT /F9 12 Tf {modo} Tr 10 10 Td (Oi) Tj ET").into_bytes()));
        let p = doc.get_dictionary_mut(pagina).unwrap();
        p.set("Resources", dictionary! { "Font" => dictionary! { "F9" => fonte } });
        p.set("Contents", conteudo);
    }

    #[test]
    fn datas_do_pdf_viram_iso() {
        assert_eq!(data_iso("D:20261019143000-03'00'").as_deref(), Some("2026-10-19T14:30:00-03:00"));
        assert_eq!(data_iso("D:2026").as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(data_iso("lixo"), None);
    }

    #[test]
    fn fonte_nao_embutida_impede_e_conversao_grava_xmp() {
        // Helvetica não embutida com texto visível: impede.
        let mut pdf = crate::testes::pdf_teste();
        let mut ocr = pdf.clone();
        com_texto(&mut pdf, 0);
        let i = pdf.verificar_pdfa();
        assert!(i.iter().any(|x| x.regra == "Todas as fontes embutidas" && x.situacao == Situacao::Impede), "{i:?}");
        assert!(pdf.converter_pdfa(b"icc").is_err());
        // Mesmo texto só invisível (camada do OCR): passa e ganha XMP, perfil e ID.
        com_texto(&mut ocr, 3);
        ocr.converter_pdfa(b"icc").unwrap();
        let bytes = ocr.serializar().unwrap();
        let texto = String::from_utf8_lossy(&bytes);
        assert!(texto.contains("<pdfaid:part>2</pdfaid:part>") && texto.contains("GTS_PDFA1"));
        let lido = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        assert!(lido.documento.trailer.has(b"ID") && lido.documento.version.as_str() <= "1.7");
    }
}
