//! Achatar: comentários, carimbos e campos de formulário viram desenho da página e deixam de ser
//! editáveis. Cada anotação com aparência (/AP /N) é chamada como Form XObject no retângulo dela
//! (ISO 32000-1, 12.5.5); as escondidas somem, links ficam (não têm desenho a fundir).

use lopdf::{Dictionary, Object, ObjectId};

use super::DocumentoPdf;

const ESCONDIDA: i64 = 1 << 1;
const NAO_VER: i64 = 1 << 5;

/// Mapa da caixa `[x0 y0 x1 y1]` (já passada pela /Matrix da aparência) para o /Rect.
fn encaixe(caixa: [f32; 4], rect: [f32; 4]) -> [f32; 6] {
    let sx = (rect[2] - rect[0]) / (caixa[2] - caixa[0]).max(1e-3);
    let sy = (rect[3] - rect[1]) / (caixa[3] - caixa[1]).max(1e-3);
    [sx, 0.0, 0.0, sy, rect[0] - sx * caixa[0], rect[1] - sy * caixa[1]]
}

/// Caixa que contém a `caixa` depois da matriz `m`.
fn transformar(caixa: [f32; 4], m: [f32; 6]) -> [f32; 4] {
    let pontos = [(caixa[0], caixa[1]), (caixa[2], caixa[1]), (caixa[0], caixa[3]), (caixa[2], caixa[3])]
        .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
    let (xs, ys) = (pontos.map(|p| p.0), pontos.map(|p| p.1));
    [xs.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MAX, f32::min),
     xs.iter().copied().fold(f32::MIN, f32::max), ys.iter().copied().fold(f32::MIN, f32::max)]
}

fn numeros<const N: usize>(o: Option<&Object>) -> Option<[f32; N]> {
    let v = o?.as_array().ok()?;
    if v.len() != N { return None; }
    let mut r = [0.0; N];
    for (i, x) in v.iter().enumerate() { r[i] = x.as_float().ok()?; }
    Some(r)
}

impl DocumentoPdf {
    /// Funde as anotações na página (com `formularios`, também os campos, e o formulário deixa de existir).
    /// Devolve quantas viraram desenho.
    pub fn achatar_anotacoes(&mut self, formularios: bool) -> Result<usize, String> {
        let mut total = 0;
        let mut achatou_campo = false;
        for pagina in 1..=self.total_paginas() {
            let id = self.id_pagina(pagina)?;
            let anotacoes: Vec<Object> = match self.documento.get_dictionary(id).ok().and_then(|d| d.get(b"Annots").ok()) {
                Some(Object::Reference(r)) => self.documento.get_object(*r).and_then(Object::as_array).cloned().unwrap_or_default(),
                Some(Object::Array(v)) => v.clone(),
                _ => continue,
            };
            let mut ficam = Vec::new();
            let mut desenho = String::new();
            let mut recursos: Vec<(String, ObjectId)> = Vec::new();
            for a in anotacoes {
                let d = match &a { Object::Reference(r) => self.documento.get_dictionary(*r).ok(), Object::Dictionary(d) => Some(d), _ => None };
                let Some(d) = d else { ficam.push(a); continue };
                let tipo = d.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"");
                let campo = tipo == b"Widget";
                if tipo == b"Link" || tipo == b"Popup" && !campo || campo && !formularios {
                    // O balão do Popup sai junto com a nota que ele abre; link continua clicável.
                    if tipo != b"Popup" { ficam.push(a); }
                    continue;
                }
                let bandeiras = d.get(b"F").and_then(Object::as_i64).unwrap_or(0);
                achatou_campo |= campo;
                if bandeiras & (ESCONDIDA | NAO_VER) != 0 { continue; }
                let Some(aparencia) = self.aparencia_normal(d) else { continue };
                let Some(rect) = numeros::<4>(d.get(b"Rect").ok()) else { continue };
                let rect = [rect[0].min(rect[2]), rect[1].min(rect[3]), rect[0].max(rect[2]), rect[1].max(rect[3])];
                let Ok(forma) = self.documento.get_object(aparencia).and_then(Object::as_stream).map(|s| &s.dict) else { continue };
                let caixa = numeros::<4>(forma.get(b"BBox").ok()).unwrap_or([0.0, 0.0, rect[2] - rect[0], rect[3] - rect[1]]);
                let matriz = numeros::<6>(forma.get(b"Matrix").ok()).unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                let m = encaixe(transformar(caixa, matriz), rect);
                if let Ok(Object::Stream(s)) = self.documento.get_object_mut(aparencia) {
                    s.dict.set("Type", "XObject");
                    s.dict.set("Subtype", "Form");
                }
                let nome = format!("FPa{}", aparencia.0);
                desenho += &format!("q {:.4} {:.4} {:.4} {:.4} {:.3} {:.3} cm /{nome} Do Q\n", m[0], m[1], m[2], m[3], m[4], m[5]);
                recursos.push((nome, aparencia));
                total += 1;
            }
            if !desenho.is_empty() {
                let recursos: Vec<(&str, String, ObjectId)> = recursos.into_iter().map(|(n, a)| ("XObject", n, a)).collect();
                self.acrescentar(pagina, desenho, &recursos)?;
            }
            let pagina = self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?;
            if ficam.is_empty() { pagina.remove(b"Annots"); } else { pagina.set("Annots", ficam); }
        }
        if achatou_campo {
            if let Ok(c) = self.documento.catalog_mut() { c.remove(b"AcroForm"); }
        }
        self.modificado = true;
        Ok(total)
    }

    /// A aparência normal: o stream de /AP /N, ou o do estado /AS (caixa marcada ou não).
    fn aparencia_normal(&self, d: &Dictionary) -> Option<ObjectId> {
        let ap = match d.get(b"AP").ok()? {
            Object::Reference(r) => self.documento.get_dictionary(*r).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        match ap.get(b"N").ok()? {
            Object::Reference(r) => match self.documento.get_object(*r).ok()? {
                Object::Stream(_) => Some(*r),
                Object::Dictionary(estados) => estados.get(d.get(b"AS").ok()?.as_name().ok()?).ok()?.as_reference().ok(),
                _ => None,
            },
            Object::Dictionary(estados) => estados.get(d.get(b"AS").ok()?.as_name().ok()?).ok()?.as_reference().ok(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod testes {
    use crate::{AnotacaoNova, DocumentoPdf};

    #[test]
    fn nota_vira_desenho_e_some_da_lista() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.anotar(1, &AnotacaoNova::Carimbo { caixa: [100.0, 100.0, 200.0, 160.0], titulo: "APROVADO".into(), detalhe: String::new() }, "Teste").unwrap();
        assert_eq!(pdf.anotacoes().len(), 1);
        assert_eq!(pdf.achatar_anotacoes(true).unwrap(), 1);
        let relido = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert!(relido.anotacoes().is_empty());
        let conteudo = relido.documento.get_page_content(*relido.documento.get_pages().get(&1).unwrap());
        assert!(String::from_utf8_lossy(&conteudo).contains(" Do Q"));
    }

    #[test]
    fn encaixe_leva_a_caixa_ao_retangulo() {
        let m = super::encaixe([0.0, 0.0, 50.0, 30.0], [100.0, 100.0, 200.0, 160.0]);
        assert_eq!(m, [2.0, 0.0, 0.0, 2.0, 100.0, 100.0]);
    }
}
