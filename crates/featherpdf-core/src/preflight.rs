//! Preflight: o PDF está pronto para o destino (gráfica, impressora do escritório ou tela)?
//! Confere fontes, resolução real das imagens, espaços de cor, transparência, caixas da página e
//! o que não sai impresso. Só lê; o que dá para corrigir já tem ferramenta (PDF/A, Comprimir, Achatar).

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::DocumentoPdf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino { Grafica, Escritorio, Tela }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NivelPreflight { Erro, Aviso, Ok }

#[derive(Debug, Clone, PartialEq)]
pub struct ItemPreflight {
    pub regra: &'static str,
    pub nivel: NivelPreflight,
    pub detalhe: String,
    /// Páginas onde aparece (vazio: o documento todo).
    pub paginas: Vec<u32>,
}

/// O que cada página usa de cor e transparência.
#[derive(Debug, Default)]
struct Uso { rgb: bool, cmyk: bool, especial: BTreeSet<String>, transparencia: bool }

fn dicionario<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match o {
        Object::Reference(r) => match doc.get_object(*r).ok()? { Object::Dictionary(d) => Some(d), Object::Stream(s) => Some(&s.dict), _ => None },
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

fn resolver<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o { Object::Reference(r) => doc.get_object(*r).unwrap_or(o), _ => o }
}

/// Anota no `uso` o espaço de cor `cs` (nome ou array, como no /ColorSpace).
fn espaco(doc: &Document, cs: &Object, uso: &mut Uso) {
    match resolver(doc, cs) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceRGB" | b"CalRGB" => uso.rgb = true,
            b"DeviceCMYK" => uso.cmyk = true,
            _ => {}
        },
        Object::Array(v) => match v.first().and_then(|o| o.as_name().ok()) {
            Some(b"ICCBased") => {
                let n = v.get(1).and_then(|s| dicionario(doc, s)).and_then(|d| d.get(b"N").ok()?.as_i64().ok());
                if n == Some(3) { uso.rgb = true } else if n == Some(4) { uso.cmyk = true }
            }
            Some(b"Separation") => if let Some(Ok(nome)) = v.get(1).map(Object::as_name) {
                if !matches!(nome, b"All" | b"None") { uso.especial.insert(String::from_utf8_lossy(nome).into_owned()); }
            },
            Some(b"DeviceN") => if let Some(Ok(nomes)) = v.get(1).map(|o| resolver(doc, o).as_array()) {
                for n in nomes.iter().filter_map(|n| n.as_name().ok()) {
                    if !matches!(n, b"Cyan" | b"Magenta" | b"Yellow" | b"Black" | b"None") { uso.especial.insert(String::from_utf8_lossy(n).into_owned()); }
                }
            },
            Some(b"Indexed") => if let Some(base) = v.get(1) { espaco(doc, base, uso) },
            Some(b"CalRGB") => uso.rgb = true,
            _ => {}
        },
        _ => {}
    }
}

/// Recursos (e os dos Form XObjects dentro deles, até 8 níveis): espaços de cor, imagens e ExtGState.
fn recursos(doc: &Document, res: &Dictionary, uso: &mut Uso, vistos: &mut BTreeSet<ObjectId>, nivel: u8) {
    if nivel > 8 { return; }
    if let Some(cs) = res.get(b"ColorSpace").ok().and_then(|o| dicionario(doc, o)) {
        for (_, v) in cs.iter() { espaco(doc, v, uso); }
    }
    if let Some(gs) = res.get(b"ExtGState").ok().and_then(|o| dicionario(doc, o)) {
        for (_, v) in gs.iter() {
            let Some(g) = dicionario(doc, v) else { continue };
            let alfa = |k: &[u8]| g.get(k).ok().and_then(|o| o.as_float().ok()).is_some_and(|a| a < 0.999);
            let mascara = g.get(b"SMask").ok().is_some_and(|m| !matches!(resolver(doc, m), Object::Name(n) if n == b"None"));
            let mistura = g.get(b"BM").ok().and_then(|o| o.as_name().ok()).is_some_and(|b| b != b"Normal" && b != b"Compatible");
            uso.transparencia |= alfa(b"ca") || alfa(b"CA") || mascara || mistura;
        }
    }
    if let Some(xo) = res.get(b"XObject").ok().and_then(|o| dicionario(doc, o)) {
        for (_, v) in xo.iter() {
            if let Object::Reference(r) = v { if !vistos.insert(*r) { continue; } }
            let Some(d) = dicionario(doc, v) else { continue };
            match d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) {
                Some(b"Image") => {
                    if let Ok(cs) = d.get(b"ColorSpace") { espaco(doc, cs, uso); }
                    uso.transparencia |= d.has(b"SMask");
                }
                Some(b"Form") => {
                    uso.transparencia |= d.get(b"Group").ok().and_then(|g| dicionario(doc, g))
                        .is_some_and(|g| g.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"Transparency"));
                    if let Some(r) = d.get(b"Resources").ok().and_then(|o| dicionario(doc, o)) { recursos(doc, r, uso, vistos, nivel + 1); }
                }
                _ => {}
            }
        }
    }
}

fn uso_da_pagina(doc: &Document, id: ObjectId) -> Uso {
    let mut uso = Uso::default();
    for op in lopdf::content::Content::decode(&doc.get_page_content(id)).map(|c| c.operations).unwrap_or_default() {
        match op.operator.as_str() {
            "rg" | "RG" => uso.rgb = true,
            "k" | "K" => uso.cmyk = true,
            _ => {}
        }
    }
    if let Ok((Some(res), _)) = doc.get_page_resources(id) { recursos(doc, res, &mut uso, &mut BTreeSet::new(), 0); }
    for r in doc.get_page_resources(id).map(|(_, ids)| ids).unwrap_or_default() {
        if let Ok(res) = doc.get_dictionary(r) { recursos(doc, res, &mut uso, &mut BTreeSet::new(), 0); }
    }
    uso
}

fn lista(paginas: &[u32]) -> String {
    match paginas.len() {
        0 => String::new(),
        n if n > 12 => format!("{n} páginas"),
        1 => format!("p. {}", paginas[0]),
        _ => format!("p. {}", paginas.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")),
    }
}

impl DocumentoPdf {
    pub fn preflight(&self, destino: Destino) -> Vec<ItemPreflight> {
        use NivelPreflight::*;
        let doc = &self.documento;
        let insp = self.inspecionar();
        let mut v = Vec::new();
        let mut item = |regra, nivel, detalhe: String, paginas: Vec<u32>| v.push(ItemPreflight { regra, nivel, detalhe, paginas });

        // Fontes: na gráfica e no escritório a letra trocada muda o documento.
        // As 14 fontes-padrão todo leitor tem; só a gráfica (PDF/X) exige até elas no arquivo.
        let padrao = |n: &str| ["Times", "Helvetica", "Courier", "Symbol", "ZapfDingbats"].iter().any(|p| n.split_once('+').map_or(n, |(_, r)| r).starts_with(p));
        let faltam: Vec<_> = insp.fontes.iter().filter(|f| !f.embutida && (destino == Destino::Grafica || !padrao(&f.nome))).collect();
        let pags: Vec<u32> = faltam.iter().flat_map(|f| f.paginas.iter().copied()).collect::<BTreeSet<_>>().into_iter().collect();
        item("Fontes embutidas", match (faltam.is_empty(), destino) { (true, _) => Ok, (false, Destino::Tela) => Aviso, _ => Erro },
            if faltam.is_empty() { format!("{} fonte(s), todas no arquivo.", insp.fontes.len()) }
            else { format!("Fora do arquivo: {}.", faltam.iter().map(|f| f.nome.as_str()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", ")) },
            pags);

        // Resolução no tamanho em que a imagem aparece.
        let (minimo, bom) = match destino { Destino::Grafica => (150.0, 300.0), Destino::Escritorio => (100.0, 150.0), Destino::Tela => (0.0, 0.0) };
        if destino == Destino::Tela {
            let pesadas: Vec<_> = insp.imagens.iter().filter(|i| i.dpi.is_some_and(|d| d > 200.0)).collect();
            item("Imagens no tamanho da tela", if pesadas.is_empty() { Ok } else { Aviso },
                if pesadas.is_empty() { "Nenhuma imagem acima de 200 dpi.".into() }
                else { format!("{} imagem(ns) acima de 200 dpi: Comprimir deixa o arquivo menor sem perder na tela.", pesadas.len()) },
                pesadas.iter().flat_map(|i| i.paginas.iter().copied()).collect::<BTreeSet<_>>().into_iter().collect());
        } else {
            let ruins: Vec<_> = insp.imagens.iter().filter(|i| i.dpi.is_some_and(|d| d < minimo)).collect();
            let baixas: Vec<_> = insp.imagens.iter().filter(|i| i.dpi.is_some_and(|d| d >= minimo && d < bom)).collect();
            let pior = insp.imagens.iter().filter_map(|i| i.dpi).fold(f32::MAX, f32::min);
            let pags = |l: &[&crate::ImagemInfo]| l.iter().flat_map(|i| i.paginas.iter().copied()).collect::<BTreeSet<_>>().into_iter().collect();
            item("Resolução das imagens", if !ruins.is_empty() { Erro } else if !baixas.is_empty() { Aviso } else { Ok },
                if insp.imagens.is_empty() { "O documento não tem imagens.".into() }
                else if ruins.is_empty() && baixas.is_empty() { format!("Todas com {bom:.0} dpi ou mais no tamanho impresso.") }
                else { format!("{} abaixo de {minimo:.0} dpi e {} abaixo de {bom:.0} dpi; a pior tem {pior:.0} dpi. Sai serrilhada no papel.", ruins.len(), baixas.len()) },
                if ruins.is_empty() { pags(&baixas) } else { pags(&ruins) });
        }

        // Cor e transparência, página a página.
        let mut rgb = Vec::new();
        let mut cmyk = Vec::new();
        let mut transp = Vec::new();
        let mut especiais = BTreeSet::new();
        for (n, id) in doc.get_pages() {
            let u = uso_da_pagina(doc, id);
            if u.rgb { rgb.push(n) }
            if u.cmyk { cmyk.push(n) }
            if u.transparencia { transp.push(n) }
            especiais.extend(u.especial);
        }
        match destino {
            Destino::Grafica => {
                item("Cores em CMYK", if rgb.is_empty() { Ok } else { Aviso },
                    if rgb.is_empty() { "Nenhuma cor em RGB.".into() } else { format!("Há cor em RGB ({}): a gráfica converte, e o tom pode mudar (azul e verde vivos ficam mais apagados).", lista(&rgb)) },
                    rgb.clone());
                item("Cores especiais (Pantone)", if especiais.is_empty() { Ok } else { Aviso },
                    if especiais.is_empty() { "Nenhuma cor especial.".into() } else { format!("{}: confirme com a gráfica se vão como chapa própria.", especiais.iter().cloned().collect::<Vec<_>>().join(", ")) },
                    vec![]);
                item("Transparência", if transp.is_empty() { Ok } else { Aviso },
                    if transp.is_empty() { "Nenhuma transparência.".into() } else { "Sombras, opacidade ou mesclagem: RIPs antigos podem achatar com falhas. Peça um PDF de prova.".into() },
                    transp.clone());
                let sem_corte: Vec<u32> = doc.get_pages().into_iter()
                    .filter(|(_, id)| doc.get_dictionary(*id).is_ok_and(|d| !d.has(b"TrimBox") && !d.has(b"ArtBox"))).map(|(n, _)| n).collect();
                item("Tamanho final e sangria (TrimBox/BleedBox)", if sem_corte.is_empty() { Ok } else { Aviso },
                    if sem_corte.is_empty() { "Todas as páginas dizem onde a folha é cortada.".into() }
                    else { "Sem TrimBox: a gráfica usa a página inteira como tamanho final, sem sangria para o corte.".into() },
                    sem_corte);
            }
            Destino::Escritorio | Destino::Tela => {
                let tem_cmyk = !cmyk.is_empty();
                item("Cores para a tela e impressora comum", if tem_cmyk && destino == Destino::Tela { Aviso } else { Ok },
                    if tem_cmyk { format!("Há cor em CMYK ({}): na tela ela é convertida e pode ficar diferente.", lista(&cmyk)) } else { "Cores em RGB ou cinza.".into() },
                    cmyk.clone());
            }
        }

        // Tamanhos de página.
        let tamanhos: BTreeSet<(i32, i32)> = (1..=self.total_paginas()).filter_map(|n| self.geometria(n).ok())
            .map(|(c, giro)| { let (l, a) = ((c[2] - c[0]).round() as i32, (c[3] - c[1]).round() as i32); if giro % 180 == 0 { (l, a) } else { (a, l) } }).collect();
        if destino != Destino::Tela {
            item("Páginas do mesmo tamanho", if tamanhos.len() <= 1 { Ok } else { Aviso },
                if tamanhos.len() <= 1 { "Todas do mesmo tamanho.".into() }
                else { format!("{} tamanhos diferentes: Páginas → Padronizar tamanho deixa todas iguais.", tamanhos.len()) }, vec![]);
        }

        // O que não sai no papel.
        let anot: BTreeSet<u32> = self.anotacoes().into_iter().map(|a| a.pagina).collect();
        if destino != Destino::Tela {
            item("Comentários e campos", if anot.is_empty() { Ok } else { Aviso },
                if anot.is_empty() { "Nada por cima das páginas.".into() }
                else { "Comentários e campos podem não sair na impressão da gráfica: Proteger → Achatar funde na página.".into() },
                anot.into_iter().collect());
        }
        if doc.trailer.has(b"Encrypt") || self.tem_senha() {
            item("Sem senha", if destino == Destino::Grafica { Erro } else { Aviso }, "Com senha, a gráfica e alguns programas não abrem ou não imprimem.".into(), vec![]);
        }
        v.sort_by_key(|i| i.nivel);
        v
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pagina_em_rgb_avisa_na_grafica_e_passa_no_escritorio() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.acrescentar(1, "1 0 0 rg 10 10 50 50 re f".into(), &[]).unwrap();
        let grafica = pdf.preflight(Destino::Grafica);
        let cor = grafica.iter().find(|i| i.regra == "Cores em CMYK").unwrap();
        assert_eq!((cor.nivel, cor.paginas.clone()), (NivelPreflight::Aviso, vec![1]));
        assert!(grafica.iter().any(|i| i.regra.starts_with("Tamanho final") && i.nivel == NivelPreflight::Aviso));
        let escritorio = pdf.preflight(Destino::Escritorio);
        assert!(escritorio.iter().all(|i| i.nivel == NivelPreflight::Ok), "{escritorio:?}");
    }
}
