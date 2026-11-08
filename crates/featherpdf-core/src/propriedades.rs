//! Arquivo ▸ Propriedades do documento, como no Acrobat: título, autor, assunto e palavras-chave
//! (editáveis, no dicionário Info) e o que o arquivo diz de si (programa, datas, versão, páginas).
use lopdf::{Dictionary, Object};

use crate::DocumentoPdf;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Propriedades {
    pub titulo: String,
    pub autor: String,
    pub assunto: String,
    pub palavras: String,
    /// Programa que criou o original (`Creator`) e o que gerou o PDF (`Producer`).
    pub criador: String,
    pub produtor: String,
    /// Já no formato dd/mm/aaaa hh:mm (vazio se o arquivo não diz).
    pub criado: String,
    pub modificado: String,
    pub versao: String,
    pub paginas: u32,
    /// Tamanho da primeira página em milímetros.
    pub tamanho_mm: Option<(f32, f32)>,
    /// PDF marcado (com estrutura para leitor de tela).
    pub marcado: bool,
}

/// Texto de um `String` do PDF: UTF-16 com BOM ou PDFDocEncoding (≈ Latin-1).
fn texto(o: &Object) -> Option<String> {
    let Object::String(b, _) = o else { return None };
    Some(if b.starts_with(&[0xFE, 0xFF]) {
        String::from_utf16_lossy(&b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
    } else if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&b[3..]).into_owned()
    } else { b.iter().map(|&c| c as char).collect() })
}

/// Texto para gravar: ASCII como está, o resto em UTF-16 (acentos aparecem em qualquer leitor).
fn objeto_texto(t: &str) -> Object {
    if t.is_ascii() { return Object::string_literal(t); }
    let mut b = vec![0xFE, 0xFF];
    b.extend(t.encode_utf16().flat_map(u16::to_be_bytes));
    Object::String(b, lopdf::StringFormat::Hexadecimal)
}

/// `D:20261108183000-03'00'` → `08/11/2026 18:30`.
pub fn data_legivel(d: &str) -> String {
    let d = d.trim().trim_start_matches("D:");
    let n = |a: usize, b: usize| d.get(a..b).filter(|s| s.bytes().all(|c| c.is_ascii_digit()));
    match (n(0, 4), n(4, 6), n(6, 8)) {
        (Some(a), Some(m), Some(dia)) => match (n(8, 10), n(10, 12)) {
            (Some(h), Some(min)) => format!("{dia}/{m}/{a} {h}:{min}"),
            _ => format!("{dia}/{m}/{a}"),
        },
        _ => String::new(),
    }
}

impl DocumentoPdf {
    fn info(&self) -> Option<&Dictionary> {
        match self.documento.trailer.get(b"Info").ok()? {
            Object::Reference(id) => self.documento.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    pub fn propriedades(&self) -> Propriedades {
        let info = self.info();
        let campo = |k: &[u8]| info.and_then(|i| i.get(k).ok()).and_then(texto).unwrap_or_default().trim().to_string();
        let catalogo = self.documento.catalog().ok();
        let marcado = catalogo.and_then(|c| c.get(b"MarkInfo").ok())
            .and_then(|o| match o { Object::Reference(id) => self.documento.get_dictionary(*id).ok(), o => o.as_dict().ok() })
            .and_then(|d| d.get(b"Marked").ok()).and_then(|o| o.as_bool().ok()).unwrap_or(false);
        // A versão do catálogo vale quando é maior que a do cabeçalho (PDF atualizado por cima).
        let versao = catalogo.and_then(|c| c.get(b"Version").ok()).and_then(|o| o.as_name().ok())
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .filter(|v| v.as_str() > self.documento.version.as_str())
            .unwrap_or_else(|| self.documento.version.clone());
        const MM_POR_PT: f32 = 25.4 / 72.0;
        Propriedades {
            titulo: campo(b"Title"), autor: campo(b"Author"), assunto: campo(b"Subject"), palavras: campo(b"Keywords"),
            criador: campo(b"Creator"), produtor: campo(b"Producer"),
            criado: data_legivel(&campo(b"CreationDate")), modificado: data_legivel(&campo(b"ModDate")),
            versao, paginas: self.total_paginas(),
            tamanho_mm: self.tamanhos().first().map(|(l, a)| (l * MM_POR_PT, a * MM_POR_PT)),
            marcado,
        }
    }

    /// Grava título, autor, assunto e palavras-chave (vazio tira o campo). `Ok(false)` se nada mudou.
    // shortcut: o XMP (`/Metadata`) não é reescrito; leitor que prefere o XMP mostra o valor antigo e um
    // PDF/A deixa de bater Info com XMP. Reescrever o XMP junto se aparecer relato.
    pub fn definir_propriedades(&mut self, titulo: &str, autor: &str, assunto: &str, palavras: &str) -> Result<bool, String> {
        let atual = self.propriedades();
        if (atual.titulo.as_str(), atual.autor.as_str(), atual.assunto.as_str(), atual.palavras.as_str())
            == (titulo.trim(), autor.trim(), assunto.trim(), palavras.trim()) { return Ok(false); }
        let doc = &mut self.documento;
        let id = match doc.trailer.get(b"Info") {
            Ok(Object::Reference(id)) if doc.get_dictionary(*id).is_ok() => *id,
            Ok(Object::Dictionary(d)) => { let d = d.clone(); doc.add_object(d) }
            _ => doc.add_object(Dictionary::new()),
        };
        doc.trailer.set("Info", id);
        let (a, m, d, s) = crate::anotacoes::agora();
        let agora = format!("D:{a:04}{m:02}{d:02}{:02}{:02}{:02}Z", s / 3600, s / 60 % 60, s % 60);
        let info = doc.get_dictionary_mut(id).map_err(|e| e.to_string())?;
        for (chave, valor) in [("Title", titulo), ("Author", autor), ("Subject", assunto), ("Keywords", palavras)] {
            let v = valor.trim();
            if v.is_empty() { info.remove(chave.as_bytes()); } else { info.set(chave, objeto_texto(v)); }
        }
        info.set("ModDate", Object::string_literal(agora));
        self.modificado = true;
        Ok(true)
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;

    #[test]
    fn le_e_grava_as_propriedades_com_acento() {
        let mut pdf = pdf_com("BT ET");
        let p = pdf.propriedades();
        assert_eq!((p.paginas, p.titulo.as_str()), (1, ""));
        assert!(pdf.definir_propriedades(" Relatório de março ", "Ana", "", "vendas, 2026").unwrap());
        assert!(!pdf.definir_propriedades("Relatório de março", "Ana", "", "vendas, 2026").unwrap(), "igual não muda");
        let volta = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap().propriedades();
        assert_eq!((volta.titulo.as_str(), volta.autor.as_str(), volta.assunto.as_str(), volta.palavras.as_str()),
            ("Relatório de março", "Ana", "", "vendas, 2026"));
        assert!(!volta.modificado.is_empty());
        pdf.definir_propriedades("", "Ana", "", "").unwrap();
        assert_eq!(pdf.propriedades().titulo, "", "vazio tira");
    }

    #[test]
    fn data_do_pdf_fica_legivel() {
        assert_eq!(super::data_legivel("D:20261108183000-03'00'"), "08/11/2026 18:30");
        assert_eq!(super::data_legivel("D:20260102"), "02/01/2026");
        assert_eq!(super::data_legivel("lixo"), "");
    }
}
