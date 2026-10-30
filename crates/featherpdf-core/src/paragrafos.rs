//! Parágrafos marcados: num PDF com tags (/StructTreeRoot), as linhas de texto cujo
//! conteúdo marcado (BDC /MCID … EMC) cai no mesmo elemento de bloco (/P, /H1, /LI…)
//! são um parágrafo só. Sem tags, o palpite vem da geometria das linhas.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::{DocumentoPdf, ObjetoPdf, TipoObjeto};

/// O parágrafo pela geometria: a partir de `objeto`, as linhas vizinhas na mesma coluna,
/// mesmo tamanho e mesma entrelinha. Com `so_abaixo`, só as de baixo (o refluxo).
pub(crate) fn paragrafo_por_geometria(objs: &[ObjetoPdf], objeto: usize, so_abaixo: bool) -> Vec<usize> {
    let tamanho = |o: &ObjetoPdf| match o.tipo { TipoObjeto::Texto { tamanho, .. } => Some(tamanho), _ => None };
    let Some(tam) = objs.get(objeto).and_then(tamanho) else { return Vec::new() };
    let c0 = objs[objeto].caixa;
    let mut linhas = vec![objeto];
    let mut passo = None::<f32>;
    // `sinal` 1: desce a página; -1: sobe.
    let mut andar = |ordem: &mut dyn Iterator<Item = usize>, sinal: f32, linhas: &mut Vec<usize>| {
        let mut ultima = c0;
        for k in ordem {
            let Some(t) = tamanho(&objs[k]) else { continue };
            let o = objs[k].caixa;
            let desce = (ultima[1] - o[1]) * sinal;
            if (o[0] - c0[0]).abs() > 2.0 || (t - tam).abs() > 0.6 || desce <= 0.0 || desce > tam * 2.2
                || passo.is_some_and(|p| (desce - p).abs() > 1.5) { break; }
            passo = Some(desce);
            linhas.push(k);
            ultima = o;
        }
    };
    andar(&mut (objeto + 1..objs.len()), 1.0, &mut linhas);
    if !so_abaixo { andar(&mut (0..objeto).rev(), -1.0, &mut linhas); }
    linhas.sort_unstable();
    linhas
}

/// Elementos de linha: o parágrafo é o primeiro ancestral que não é um deles.
const EM_LINHA: &[&[u8]] = &[b"Span", b"Link", b"Annot", b"Quote", b"Note", b"Reference", b"BibEntry", b"Code", b"Ruby", b"Warichu"];
const MAX_NOS: usize = 200_000;

struct Busca<'a> {
    doc: &'a Document,
    pagina: ObjectId,
    papeis: Option<&'a Dictionary>,
    vistos: HashSet<ObjectId>,
    blocos: usize,
    /// MCID desta página → bloco.
    saida: HashMap<i64, usize>,
}

impl Busca<'_> {
    fn percorrer(&mut self, o: &Object, bloco: usize, pg: Option<ObjectId>, nivel: u16) {
        if nivel > 64 || self.vistos.len() > MAX_NOS { return; }
        if let Object::Reference(r) = o { if !self.vistos.insert(*r) { return; } }
        let Ok((_, o)) = self.doc.dereference(o) else { return };
        match o {
            Object::Integer(mcid) => if pg == Some(self.pagina) { self.saida.insert(*mcid, bloco); },
            Object::Array(a) => a.iter().for_each(|x| self.percorrer(x, bloco, pg, nivel + 1)),
            Object::Dictionary(d) => {
                let pg = d.get(b"Pg").and_then(Object::as_reference).ok().or(pg);
                if d.get(b"Type").and_then(Object::as_name).is_ok_and(|t| t == b"MCR") {
                    if let (Ok(m), true) = (d.get(b"MCID").and_then(Object::as_i64), pg == Some(self.pagina)) { self.saida.insert(m, bloco); }
                    return;
                }
                let Ok(s) = d.get(b"S").and_then(Object::as_name) else { return };
                // Nome próprio do gerador ("Normal", "Corpo") vira o padrão pelo /RoleMap.
                let s = self.papeis.and_then(|p| p.get(s).and_then(Object::as_name).ok()).unwrap_or(s);
                let bloco = if EM_LINHA.contains(&s) { bloco } else { self.blocos += 1; self.blocos };
                if let Ok(k) = d.get(b"K") { self.percorrer(k, bloco, pg, nivel + 1); }
            }
            _ => {}
        }
    }
}

impl DocumentoPdf {
    /// As linhas de texto do mesmo parágrafo que `objeto`: pelas tags do PDF quando ele
    /// tem (`true`), senão pela geometria (`false`). Vazio: `objeto` não é texto.
    pub fn paragrafo(&self, pagina: u32, objeto: usize) -> Result<(Vec<usize>, bool), String> {
        if let Some(linhas) = self.paragrafo_marcado(pagina, objeto)? { return Ok((linhas, true)); }
        Ok((paragrafo_por_geometria(&self.objetos(pagina)?, objeto, false), false))
    }

    /// As linhas de texto do mesmo parágrafo que `objeto`, pelas tags do PDF.
    /// `None`: o PDF não tem tags ou essa linha não está marcada.
    pub fn paragrafo_marcado(&self, pagina: u32, objeto: usize) -> Result<Option<Vec<usize>>, String> {
        let doc = &self.documento;
        let raiz = doc.catalog().ok().and_then(|c| c.get(b"StructTreeRoot").ok()).and_then(|r| doc.dereference(r).ok())
            .and_then(|(_, o)| o.as_dict().ok());
        let Some(raiz) = raiz else { return Ok(None) };
        let mut busca = Busca {
            doc, pagina: self.id_pagina(pagina)?,
            papeis: raiz.get(b"RoleMap").ok().and_then(|r| doc.dereference(r).ok()).and_then(|(_, o)| o.as_dict().ok()),
            vistos: HashSet::new(), blocos: 0, saida: HashMap::new(),
        };
        if let Ok(k) = raiz.get(b"K") { busca.percorrer(k, 0, None, 0); }

        let analise = self.analisar(pagina)?;
        // MCID em vigor em cada operação: o do BDC aberto mais de dentro.
        let mut pilha: Vec<Option<i64>> = Vec::new();
        let mut mcid_em = Vec::with_capacity(analise.ops.len());
        for op in &analise.ops {
            match op.operator.as_str() {
                "BDC" => pilha.push(op.operands.get(1).and_then(|p| p.as_dict().ok()).and_then(|d| d.get(b"MCID").and_then(Object::as_i64).ok())),
                "BMC" => pilha.push(None),
                "EMC" => { pilha.pop(); }
                _ => {}
            }
            mcid_em.push(pilha.iter().rev().find_map(|m| *m));
        }
        let bloco_de = |k: usize| -> Option<usize> {
            if !matches!(analise.objetos.get(k)?.tipo, TipoObjeto::Texto { .. }) { return None; }
            busca.saida.get(&mcid_em[*analise.lugares[k].escritas.first()?]?).copied().filter(|&b| b > 0)
        };
        let Some(alvo) = bloco_de(objeto) else { return Ok(None) };
        Ok(Some((0..analise.objetos.len()).filter(|&k| bloco_de(k) == Some(alvo)).collect()))
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;
    use lopdf::{dictionary, Object};

    #[test]
    fn linhas_do_mesmo_p_sao_um_paragrafo() {
        // Dois parágrafos: o primeiro com duas linhas (a segunda num Span), o segundo com uma.
        let mut pdf = pdf_com("/P <</MCID 0>> BDC BT /F1 12 Tf 72 700 Td (Linha um) Tj 0 -14 Td (Linha dois) Tj ET EMC \
                               /Span <</MCID 1>> BDC BT /F1 12 Tf 72 672 Td (Linha tres) Tj ET EMC \
                               /P <</MCID 2>> BDC BT /F1 12 Tf 72 600 Td (Outro) Tj ET EMC");
        assert_eq!(pdf.paragrafo_marcado(1, 0).unwrap(), None);
        // Sem tags, a geometria: entrelinha de 14 pt junta as três; "Outro" fica longe.
        assert_eq!(pdf.paragrafo(1, 1).unwrap(), (vec![0, 1, 2], false));
        assert_eq!(pdf.paragrafo(1, 3).unwrap(), (vec![3], false));
        let pagina = pdf.objeto_da_pagina(1).unwrap();
        let doc = &mut pdf.documento;
        let span = doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Span", "K" => 1 });
        let p1 = doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Corpo", "Pg" => pagina, "K" => vec![0.into(), Object::Reference(span)] });
        let p2 = doc.add_object(dictionary! { "Type" => "StructElem", "S" => "P", "Pg" => pagina, "K" => 2 });
        let raiz = doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Document", "K" => vec![p1.into(), p2.into()] });
        let arvore = doc.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => raiz, "RoleMap" => dictionary! { "Corpo" => "P" } });
        doc.catalog_mut().unwrap().set("StructTreeRoot", arvore);
        assert_eq!(pdf.paragrafo_marcado(1, 1).unwrap(), Some(vec![0, 1, 2]));
        assert_eq!(pdf.paragrafo(1, 2).unwrap(), (vec![0, 1, 2], true));
        assert_eq!(pdf.paragrafo_marcado(1, 3).unwrap(), Some(vec![3]));
    }
}
