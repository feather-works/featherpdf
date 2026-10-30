//! Versões dentro do arquivo: cada atualização incremental (assinatura, salvamento do Acrobat
//! "rápido") acrescenta um pedaço terminado em `%%EOF` e deixa o anterior intacto. Cortar os
//! bytes logo depois de um `%%EOF` dá o documento como era naquela versão.

use lopdf::Object;

use super::DocumentoPdf;

#[derive(Debug, Clone, PartialEq)]
pub struct Versao {
    /// Bytes do arquivo até o fim desta versão (`&bytes[..fim]` abre como era).
    pub fim: usize,
    pub paginas: u32,
    /// /ModDate (ou /CreationDate) do /Info, já legível.
    pub data: Option<String>,
    /// Assinaturas já presentes nesta versão.
    pub assinaturas: usize,
}

/// Onde termina cada versão: depois de cada `%%EOF` (e da quebra de linha que o segue).
fn fins(bytes: &[u8]) -> Vec<usize> {
    let mut v = Vec::new();
    let mut i = 0;
    while let Some(p) = bytes[i..].windows(5).position(|w| w == b"%%EOF") {
        let inicio = i + p;
        let mut fim = inicio + 5;
        while fim < bytes.len() && matches!(bytes[fim], b'\r' | b'\n') { fim += 1; }
        // Fim de versão de verdade vem logo depois de `startxref <número>`; o resto é %%EOF dentro de stream ou comentário.
        let antes = &bytes[inicio.saturating_sub(40)..inicio];
        let depois_startxref = antes.windows(9).rposition(|w| w == b"startxref").map(|k| &antes[k + 9..]);
        if depois_startxref.is_some_and(|r| r.iter().all(|b| b.is_ascii_digit() || b.is_ascii_whitespace())) { v.push(fim); }
        i = fim;
    }
    // Lixo depois do último %%EOF (comum em arquivo baixado) não é versão nova.
    v
}

/// As versões do arquivo, da mais antiga para a atual. Pedaço que não abre sozinho (um
/// `%%EOF` dentro de um stream, por exemplo) é pulado.
pub fn versoes(bytes: &[u8]) -> Vec<Versao> {
    let fins = fins(bytes);
    // Cada versão reabre o arquivo até ali: um arquivo montado com milhares delas travaria o painel.
    // shortcut: só as 50 mais recentes, aumentar se aparecer documento real com mais revisões.
    fins[fins.len().saturating_sub(50)..].iter().copied().filter_map(|fim| {
        let pdf = DocumentoPdf::de_bytes(&bytes[..fim], None).ok()?;
        let doc = &pdf.documento;
        let info = doc.trailer.get(b"Info").ok().and_then(|o| match o {
            Object::Reference(r) => doc.get_dictionary(*r).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        });
        let data = info.and_then(|d| d.get(b"ModDate").or_else(|_| d.get(b"CreationDate")).ok())
            .and_then(|o| o.as_str().ok()).map(|s| crate::data_legivel(&String::from_utf8_lossy(s)));
        let assinaturas = crate::assinatura::verificar_assinaturas(&bytes[..fim]).map_or(0, |a| a.len());
        Some(Versao { fim, paginas: pdf.total_paginas(), data, assinaturas })
    }).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn atualizacao_incremental_vira_segunda_versao() {
        let pdf = crate::testes::pdf_teste();
        let base = pdf.serializar().unwrap();
        // Uma atualização incremental de verdade: a página ganha /Rotate num pedaço acrescentado.
        let doc = lopdf::Document::load_mem(&base).unwrap();
        let mut inc = lopdf::IncrementalDocument::create_from(base.clone(), doc);
        let pagina = *inc.get_prev_documents().get_pages().get(&1).unwrap();
        inc.opt_clone_object_to_new_document(pagina).unwrap();
        inc.new_document.get_dictionary_mut(pagina).unwrap().set("Rotate", 90);
        let mut bytes = Vec::new();
        inc.save_to(&mut bytes).unwrap();
        let v = versoes(&bytes);
        assert_eq!(v.len(), 2);
        assert!((base.len()..base.len() + 3).contains(&v[0].fim));
        let antiga = DocumentoPdf::de_bytes(&bytes[..v[0].fim], None).unwrap();
        let atual = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        assert_eq!((antiga.geometria(1).unwrap().1, atual.geometria(1).unwrap().1), (0, 90));
        // %%EOF solto (num comentário ou stream) não é versão, e o total tem teto.
        let mut lixo = bytes.clone();
        for _ in 0..10_000 { lixo.extend_from_slice(b"%%EOF\n"); }
        assert_eq!(fins(&lixo).len(), 2);
        let mut muitas = bytes.clone();
        for _ in 0..100 { muitas.extend_from_slice(b"startxref\n0\n%%EOF\n"); }
        assert_eq!(fins(&muitas).len(), 102);
        assert!(versoes(&muitas).len() <= 50);
    }
}
