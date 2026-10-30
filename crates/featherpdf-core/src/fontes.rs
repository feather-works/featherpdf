//! Fontes: embutir a que ficou de fora (ou trocar por uma parecida, quando o computador não a
//! tem), para o documento abrir igual em qualquer lugar. Quem acha o arquivo da fonte no sistema
//! e lê as medidas dele é o `featherpdf-io`; aqui só se grava no PDF.

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use crate::DocumentoPdf;

/// Programa TrueType pronto para entrar no PDF, com as medidas que o descritor pede.
#[derive(Debug, Clone)]
pub struct FonteNova {
    /// Nome PostScript da fonte que vai dentro ("LiberationSans-Bold").
    pub nome_ps: String,
    pub programa: Vec<u8>,
    /// Largura (milésimos do em) dos códigos 32..=255 em WinAnsi.
    pub larguras: Vec<f32>,
    pub ascent: f32,
    pub descent: f32,
    pub cap: f32,
    pub caixa: [f32; 4],
    pub italico: f32,
    pub fixa: bool,
    pub serifa: bool,
}

const SIMBOLICAS: [&str; 2] = ["Symbol", "ZapfDingbats"];

impl DocumentoPdf {
    /// Fontes simples (TrueType/Type1) sem o programa dentro do arquivo, que dá para embutir
    /// com a codificação WinAnsi: id e /BaseFont.
    /// shortcut: Type0 (CID), Type3, simbólica e /Encoding com /Differences ficam de fora; embutir essas pede mapear código a glifo.
    pub fn fontes_sem_embutir(&self) -> Vec<(ObjectId, String)> {
        let doc = &self.documento;
        let mut v: Vec<(ObjectId, String)> = doc.objects.iter().filter_map(|(id, o)| {
            let d = o.as_dict().ok()?;
            if d.get(b"Type").and_then(Object::as_name).ok()? != b"Font" { return None; }
            if !matches!(d.get(b"Subtype").and_then(Object::as_name).ok()?, b"TrueType" | b"Type1" | b"MMType1") { return None; }
            let nome = String::from_utf8_lossy(d.get(b"BaseFont").and_then(Object::as_name).ok()?).into_owned();
            if SIMBOLICAS.iter().any(|s| nome.contains(s)) { return None; }
            // Sem /Encoding: a padrão da fonte, que no texto comum bate com a WinAnsi.
            match d.get(b"Encoding") { Ok(e) => if e.as_name().ok()? != b"WinAnsiEncoding" { return None }, Err(_) => {} }
            let descritor = d.get(b"FontDescriptor").ok().and_then(|x| doc.dereference(x).ok()).and_then(|(_, x)| x.as_dict().ok());
            let embutida = descritor.is_some_and(|f| [&b"FontFile"[..], b"FontFile2", b"FontFile3"].iter().any(|k| f.has(k)));
            let simbolica = descritor.and_then(|f| f.get(b"Flags").and_then(Object::as_i64).ok()).is_some_and(|f| f & 4 != 0);
            (!embutida && !simbolica).then_some((*id, nome))
        }).collect();
        v.sort();
        v
    }

    /// Põe o programa `f` dentro da fonte `id`: vira TrueType WinAnsi com as larguras dele.
    /// Um Ctrl+Z (quem chama guarda o documento antes).
    pub fn embutir_fonte(&mut self, id: ObjectId, f: FonteNova) -> Result<(), String> {
        if f.larguras.len() != 224 { return Err("A fonte precisa da largura dos códigos 32 a 255.".into()); }
        let doc = &mut self.documento;
        let tamanho = f.programa.len() as i64;
        let mut programa = Stream::new(dictionary! { "Length1" => tamanho }, f.programa);
        let _ = programa.compress();
        let programa = doc.add_object(programa);
        let flags = 32 | i64::from(f.fixa) | (i64::from(f.serifa) << 1) | (i64::from(f.italico != 0.0) << 6);
        let descritor = doc.add_object(dictionary! {
            "Type" => "FontDescriptor", "FontName" => Object::Name(f.nome_ps.clone().into_bytes()), "Flags" => flags,
            "FontBBox" => f.caixa.iter().map(|&v| Object::Real(v)).collect::<Vec<_>>(),
            "ItalicAngle" => f.italico, "Ascent" => f.ascent, "Descent" => f.descent, "CapHeight" => f.cap,
            "StemV" => 80, "FontFile2" => programa,
        });
        let d: &mut Dictionary = doc.get_object_mut(id).and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
        d.set("Subtype", "TrueType");
        d.set("BaseFont", Object::Name(f.nome_ps.into_bytes()));
        d.set("Encoding", "WinAnsiEncoding");
        d.set("FirstChar", 32);
        d.set("LastChar", 255);
        d.set("Widths", f.larguras.iter().map(|&w| Object::Real(w.round())).collect::<Vec<_>>());
        d.set("FontDescriptor", descritor);
        Ok(())
    }
}

/// O caractere de um código WinAnsi (para medir a largura no programa da fonte).
pub fn caractere_win_ansi(codigo: u8) -> Option<char> { crate::inserir::de_win_ansi(codigo) }

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn fonte_de_fora_entra_no_arquivo() {
        let mut pdf = crate::testes::pdf_teste();
        let fonte = pdf.documento.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Arial,Bold", "Encoding" => "WinAnsiEncoding" });
        let simbolo = pdf.documento.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "ZapfDingbats" });
        let lista = pdf.fontes_sem_embutir();
        assert!(lista.contains(&(fonte, "Arial,Bold".into())) && !lista.iter().any(|(i, _)| *i == simbolo), "{lista:?}");
        let nova = FonteNova { nome_ps: "LiberationSans-Bold".into(), programa: vec![0, 1, 0, 0, 7, 7], larguras: vec![556.0; 224],
            ascent: 905.0, descent: -212.0, cap: 729.0, caixa: [-184.0, -303.0, 1062.0, 1033.0], italico: 0.0, fixa: false, serifa: false };
        pdf.embutir_fonte(fonte, nova).unwrap();
        assert!(!pdf.fontes_sem_embutir().iter().any(|(i, _)| *i == fonte));
        let relido = pdf;
        let f = relido.inspecionar().fontes.into_iter().find(|f| f.nome == "LiberationSans-Bold").expect("a fonte com o nome novo");
        assert!(f.embutida);
        let d = relido.documento.get_dictionary(fonte).unwrap();
        assert_eq!(d.get(b"Widths").unwrap().as_array().unwrap().len(), 224);
        assert_eq!(caractere_win_ansi(0x80), Some('€'));
    }
}
