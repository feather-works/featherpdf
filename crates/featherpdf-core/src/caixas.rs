//! Caixas da página além da CropBox: sangria (BleedBox), refile (TrimBox) e arte (ArtBox),
//! que a gráfica usa. Definidas por margens a partir da MediaBox, como o corte.

use lopdf::Object;

use crate::DocumentoPdf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaixaPagina { Corte, Sangria, Refile, Arte }

impl CaixaPagina {
    pub const TODAS: [CaixaPagina; 4] = [CaixaPagina::Corte, CaixaPagina::Sangria, CaixaPagina::Refile, CaixaPagina::Arte];
    pub fn chave(self) -> &'static str {
        match self { CaixaPagina::Corte => "CropBox", CaixaPagina::Sangria => "BleedBox", CaixaPagina::Refile => "TrimBox", CaixaPagina::Arte => "ArtBox" }
    }
    pub fn nome(self) -> &'static str {
        match self { CaixaPagina::Corte => "Corte (CropBox)", CaixaPagina::Sangria => "Sangria (BleedBox)", CaixaPagina::Refile => "Refile (TrimBox)", CaixaPagina::Arte => "Arte (ArtBox)" }
    }
}

impl DocumentoPdf {
    fn retangulo(&self, id: lopdf::ObjectId, chave: &[u8]) -> Option<[f32; 4]> {
        let d = self.documento.get_dictionary(id).ok()?;
        let o = match d.get(chave).ok()? { Object::Reference(r) => self.documento.get_object(*r).ok()?, o => o };
        let n: Vec<f32> = o.as_array().ok()?.iter().filter_map(|x| x.as_float().ok()).collect();
        (n.len() == 4).then(|| [n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])])
    }

    /// Margens (esquerda, direita, topo, base, em pontos) de cada caixa que a página tem, a partir da MediaBox.
    pub fn caixas_da_pagina(&self, pagina: u32) -> Result<Vec<(CaixaPagina, [f32; 4])>, String> {
        let id = self.id_pagina(pagina)?;
        // A MediaBox pode vir herdada de um nó acima da página.
        let mut no = Some(id);
        let mut m = None;
        for _ in 0..64 {
            let Some(n) = no else { break };
            if let Some(r) = self.retangulo(n, b"MediaBox") { m = Some(r); break; }
            no = self.documento.get_dictionary(n).ok().and_then(|d| d.get(b"Parent").and_then(Object::as_reference).ok());
        }
        let m = m.ok_or("MediaBox inválida.")?;
        Ok(CaixaPagina::TODAS.into_iter().filter_map(|c| self.retangulo(id, c.chave().as_bytes())
            .map(|r| (c, [r[0] - m[0], m[2] - r[2], m[3] - r[3], r[1] - m[1]]))).collect())
    }

    /// Define a caixa nas páginas por margens a partir da MediaBox; tudo zero tira a caixa.
    pub fn definir_caixa(&mut self, paginas: &[u32], caixa: CaixaPagina, [esq, dir, topo, base]: [f32; 4]) -> Result<u32, String> {
        let ids = self.achatar()?;
        let mut mudou = 0;
        for &p in paginas {
            let id = *ids.get((p as usize).wrapping_sub(1)).ok_or_else(|| format!("Página {p} inexistente."))?;
            let [x0, y0, x1, y1] = self.retangulo(id, b"MediaBox").ok_or("MediaBox inválida.")?;
            let d = self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?;
            if esq + dir + topo + base <= 0.0 {
                d.remove(caixa.chave().as_bytes());
            } else {
                let (nx0, ny0, nx1, ny1) = (x0 + esq, y0 + base, x1 - dir, y1 - topo);
                if nx1 - nx0 < 36.0 || ny1 - ny0 < 36.0 { return Err("Margens grandes demais: sobraria menos de 1,3 cm.".into()); }
                d.set(caixa.chave(), vec![nx0.into(), ny0.into(), nx1.into(), ny1.into()]);
            }
            mudou += 1;
        }
        self.modificado = true;
        Ok(mudou)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes::pdf_teste;

    #[test]
    fn sangria_refile_e_arte() {
        let mut pdf = pdf_teste();
        assert!(pdf.caixas_da_pagina(1).unwrap().is_empty());
        pdf.definir_caixa(&[1], CaixaPagina::Refile, [10.0, 10.0, 20.0, 20.0]).unwrap();
        pdf.definir_caixa(&[1], CaixaPagina::Sangria, [5.0, 5.0, 5.0, 5.0]).unwrap();
        let mut pdf2 = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(pdf2.caixas_da_pagina(1).unwrap(), [(CaixaPagina::Sangria, [5.0; 4]), (CaixaPagina::Refile, [10.0, 10.0, 20.0, 20.0])]);
        pdf2.definir_caixa(&[1], CaixaPagina::Refile, [0.0; 4]).unwrap();
        assert_eq!(pdf2.caixas_da_pagina(1).unwrap().len(), 1);
        assert!(pdf2.definir_caixa(&[1], CaixaPagina::Arte, [300.0, 300.0, 0.0, 0.0]).is_err());
        assert!(pdf2.definir_caixa(&[2], CaixaPagina::Arte, [1.0; 4]).is_err());
    }
}
