//! Linearizar ("Fast Web View", ISO 32000-1 anexo F): a primeira página e o que ela usa vêm
//! logo no começo do arquivo, com uma tabela de referências só dela, e cada página seguinte fica
//! junta. Assim o navegador mostra a página 1 antes de baixar o resto.
//! O lopdf grava os objetos já na ordem nova (numerados nessa ordem); daqui só se recortam os
//! pedaços e se monta o arquivo no layout do anexo F.

use std::collections::HashMap;

use lopdf::{Document, Object, ObjectId};

use super::DocumentoPdf;

fn referencias(o: &Object, saida: &mut Vec<ObjectId>) {
    match o {
        Object::Reference(r) => saida.push(*r),
        Object::Array(v) => v.iter().for_each(|x| referencias(x, saida)),
        Object::Dictionary(d) => d.iter().for_each(|(_, x)| referencias(x, saida)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, x)| referencias(x, saida)),
        _ => {}
    }
}

fn renumerar(o: &mut Object, mapa: &HashMap<ObjectId, u32>) {
    match o {
        Object::Reference(r) => match mapa.get(r) { Some(&n) => *r = (n, 0), None => *o = Object::Null },
        Object::Array(v) => v.iter_mut().for_each(|x| renumerar(x, mapa)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| renumerar(x, mapa)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| renumerar(x, mapa)),
        _ => {}
    }
}

/// Tudo o que a página usa, sem subir para a árvore de páginas nem entrar em outra página.
fn alcance(doc: &Document, pagina: ObjectId, dono: &mut HashMap<ObjectId, usize>, indice: usize) -> Vec<ObjectId> {
    let mut ordem = vec![];
    let mut fila = vec![pagina];
    while let Some(id) = fila.pop() {
        if dono.contains_key(&id) { continue; }
        let Ok(o) = doc.get_object(id) else { continue };
        let tipo = match o { Object::Dictionary(d) => d.get(b"Type").and_then(Object::as_name).ok(), _ => None };
        if id != pagina && matches!(tipo, Some(b"Page" | b"Pages" | b"Catalog")) { continue; }
        dono.insert(id, indice);
        ordem.push(id);
        let mut filhos = vec![];
        referencias(o, &mut filhos);
        fila.extend(filhos.into_iter().rev());
    }
    ordem
}

/// Escreve os números em bits, do mais alto para o mais baixo (tabelas de dicas, F.4).
#[derive(Default)]
struct Bits { bytes: Vec<u8>, atual: u8, usados: u8 }

impl Bits {
    fn por(&mut self, valor: u64, largura: u8) {
        for i in (0..largura).rev() {
            self.atual = self.atual << 1 | ((valor >> i) & 1) as u8;
            self.usados += 1;
            if self.usados == 8 { self.bytes.push(self.atual); self.atual = 0; self.usados = 0; }
        }
    }
    fn alinhar(&mut self) { if self.usados > 0 { self.por(0, 8 - self.usados) } }
}

fn largura(x: u64) -> u8 { (64 - x.leading_zeros()) as u8 }

impl DocumentoPdf {
    /// Cópia linearizada. O arquivo aberto não muda.
    pub fn linearizado(&self) -> Result<Vec<u8>, String> {
        if self.tem_senha() { return Err("Tire a senha antes (Proteger → Senha): o PDF linearizado sai sem cifra.".into()); }
        let mut doc = self.documento.clone();
        crate::podar(&mut doc);
        let raiz = doc.trailer.get(b"Root").and_then(Object::as_reference).map_err(|_| "PDF sem catálogo.")?;
        let paginas: Vec<ObjectId> = doc.get_pages().into_values().collect();

        // Ordem do arquivo: catálogo, objetos da página 1, cada página com os seus, o resto.
        let mut dono = HashMap::new();
        dono.insert(raiz, usize::MAX);
        let grupos: Vec<Vec<ObjectId>> = paginas.iter().enumerate().map(|(i, &p)| alcance(&doc, p, &mut dono, i)).collect();
        let resto: Vec<ObjectId> = doc.objects.keys().copied().filter(|id| !dono.contains_key(id)).collect();
        // shortcut: objeto usado por várias páginas fica com a primeira que o usa, e a tabela de
        // objetos compartilhados vai vazia; os leitores buscam pela tabela de referências do mesmo jeito.

        // Números: o resto do arquivo é 1..=m (tabela principal); a primeira página vem depois
        // (dicionário de linearização, catálogo, página 1, e as dicas por último).
        let depois: Vec<ObjectId> = grupos[1..].iter().flatten().chain(&resto).copied().collect();
        let m = depois.len() as u32;
        let mut mapa: HashMap<ObjectId, u32> = depois.iter().enumerate().map(|(i, &id)| (id, i as u32 + 1)).collect();
        let n_lin = m + 1;
        mapa.insert(raiz, m + 2);
        for (i, &id) in grupos[0].iter().enumerate() { mapa.insert(id, m + 3 + i as u32); }
        let n_dicas = m + 3 + grupos[0].len() as u32;

        let mut nova = Document::with_version(doc.version.clone());
        for (id, o) in &doc.objects {
            let mut o = o.clone();
            renumerar(&mut o, &mapa);
            nova.objects.insert((mapa[id], 0), o);
        }
        nova.max_id = n_dicas - 1;
        let mut trailer = lopdf::Dictionary::new();
        for chave in [&b"Root"[..], b"Info", b"ID"] {
            if let Ok(v) = doc.trailer.get(chave) { let mut v = v.clone(); renumerar(&mut v, &mapa); trailer.set(chave, v); }
        }
        nova.trailer = trailer.clone();
        let mut gravado = Vec::new();
        nova.save_to(&mut gravado).map_err(|e| format!("Falha ao serializar PDF: {e}"))?;
        let relido = Document::load_mem(&gravado).map_err(|e| format!("Falha ao reler o PDF: {e}"))?;
        let mut inicios: Vec<(usize, u32)> = relido.reference_table.entries.iter().filter_map(|(&num, e)| match e {
            lopdf::xref::XrefEntry::Normal { offset, .. } => Some((*offset as usize, num)),
            _ => None,
        }).collect();
        inicios.sort();
        let fim_corpo = gravado.windows(5).rposition(|w| w == b"xref\n" || w == b"xref\r").ok_or("PDF gravado sem tabela.")?;
        let pedaco: HashMap<u32, &[u8]> = inicios.iter().enumerate()
            .map(|(i, &(o, num))| (num, &gravado[o..inicios.get(i + 1).map_or(fim_corpo, |p| p.0)])).collect();
        let tam = |num: u32| pedaco.get(&num).map_or(0, |p| p.len());

        // Medidas sem as dicas (é assim que as tabelas de dicas contam, F.4).
        let cabecalho = format!("%PDF-{}\n%\u{e2}\u{e3}\u{cf}\u{d3}\n", doc.version).into_bytes();
        let lin = |l: usize, h: (usize, usize), e: usize, t: usize| format!(
            "{n_lin} 0 obj\n<< /Linearized 1 /L {l:>10} /H [ {:>10} {:>10} ] /O {} /E {e:>10} /N {} /T {t:>10} >>\nendobj\n",
            h.0, h.1, m + 3, paginas.len()).into_bytes();
        let k = n_dicas - m; // entradas da tabela da primeira página: n_lin..=n_dicas
        let mut trailer_txt = format!("<< /Size {} /Root {} 0 R", n_dicas + 1, m + 2);
        if let Ok(Object::Reference(r)) = trailer.get(b"Info") { trailer_txt += &format!(" /Info {} 0 R", r.0); }
        if let Ok(Object::Array(id)) = trailer.get(b"ID") {
            let hex = |o: &Object| o.as_str().map(|b| b.iter().map(|x| format!("{x:02X}")).collect::<String>()).unwrap_or_default();
            if id.len() == 2 { trailer_txt += &format!(" /ID [<{}> <{}>]", hex(&id[0]), hex(&id[1])); }
        }
        let tabela1_tam = format!("xref\n{n_lin} {k}\n").len() + 20 * k as usize + format!("trailer\n{trailer_txt} /Prev {:>10} >>\nstartxref\n0\n%%EOF\n", 0).len();
        let lin_tam = lin(0, (0, 0), 0, 0).len();
        let inicio_cat = cabecalho.len() + lin_tam + tabela1_tam;
        let inicio_p1 = inicio_cat + tam(m + 2);
        let mut pos = inicio_p1;
        let mut primeiros = vec![(n_lin, cabecalho.len()), (m + 2, inicio_cat)];
        for num in m + 3..n_dicas { primeiros.push((num, pos)); pos += tam(num); }
        let fim_p1 = pos;
        let mut principais = vec![];
        for num in 1..=m { principais.push(pos); pos += tam(num); }
        let tabela_principal = pos;

        // Tabela de dicas das páginas (F.3): objetos e bytes de cada página.
        let mut contas = vec![(grupos[0].len() as u64, (fim_p1 - inicio_p1) as u64)];
        let mut num = 1u32;
        for g in &grupos[1..] {
            let bytes: usize = (num..num + g.len() as u32).map(tam).sum();
            contas.push((g.len() as u64, bytes as u64));
            num += g.len() as u32;
        }
        let (menor_n, menor_b) = (contas.iter().map(|c| c.0).min().unwrap_or(0), contas.iter().map(|c| c.1).min().unwrap_or(0));
        let bits_n = largura(contas.iter().map(|c| c.0 - menor_n).max().unwrap_or(0));
        let bits_b = largura(contas.iter().map(|c| c.1 - menor_b).max().unwrap_or(0));
        let mut b = Bits::default();
        b.por(menor_n, 32); b.por(inicio_p1 as u64, 32); b.por(bits_n as u64, 16);
        b.por(menor_b, 32); b.por(bits_b as u64, 16);
        b.por(0, 32); b.por(0, 16); b.por(0, 32); b.por(0, 16); // conteúdo: não informado
        b.por(0, 16); b.por(0, 16); b.por(0, 16); b.por(1, 16);
        for c in &contas { b.por(c.0 - menor_n, bits_n) }
        b.alinhar();
        for c in &contas { b.por(c.1 - menor_b, bits_b) }
        b.alinhar();
        let compartilhados = b.bytes.len();
        // Tabela de compartilhados (F.6): só as entradas da primeira página, um objeto por grupo.
        let tams: Vec<u64> = (m + 3..n_dicas).map(|num| tam(num) as u64).collect();
        let menor_t = tams.iter().copied().min().unwrap_or(0);
        let bits_t = largura(tams.iter().map(|t| t - menor_t).max().unwrap_or(0));
        b.por((m + 3) as u64, 32); b.por(inicio_p1 as u64, 32); b.por(tams.len() as u64, 32); b.por(tams.len() as u64, 32);
        b.por(0, 16); b.por(menor_t, 32); b.por(bits_t as u64, 16);
        for t in &tams { b.por(t - menor_t, bits_t) }
        b.alinhar();
        for _ in &tams { b.por(0, 1) } // sem MD5
        b.alinhar();
        let dados = b.bytes;
        let mut dicas = format!("{n_dicas} 0 obj\n<< /Length {} /S {compartilhados} >>\nstream\n", dados.len()).into_bytes();
        dicas.extend(&dados);
        dicas.extend(b"\nendstream\nendobj\n");
        let h = dicas.len();
        primeiros.push((n_dicas, inicio_p1));
        for p in primeiros.iter_mut().skip(2) { p.1 += h; }
        let principais: Vec<usize> = principais.into_iter().map(|p| p + h).collect();
        let tabela_principal = tabela_principal + h;
        let cabeca_principal = format!("xref\n0 {}\n", m + 1);
        let rodape = format!("trailer\n<< /Size {} >>\nstartxref\n{}\n%%EOF\n", m + 1, cabecalho.len() + lin_tam);
        let total = tabela_principal + cabeca_principal.len() + 20 * (m as usize + 1) + rodape.len();

        let entrada = |o: usize| format!("{o:010} 00000 n \n");
        let mut saida = cabecalho;
        saida.extend(lin(total, (inicio_p1, h), fim_p1 + h, tabela_principal + cabeca_principal.len() - 1));
        saida.extend(format!("xref\n{n_lin} {k}\n").as_bytes());
        primeiros.sort();
        for &(_, o) in &primeiros { saida.extend(entrada(o).as_bytes()); }
        saida.extend(format!("trailer\n{trailer_txt} /Prev {tabela_principal:>10} >>\nstartxref\n0\n%%EOF\n").as_bytes());
        saida.extend(pedaco.get(&(m + 2)).copied().unwrap_or_default());
        saida.extend(&dicas);
        for num in (m + 3..n_dicas).chain(1..=m) { saida.extend(pedaco.get(&num).copied().unwrap_or_default()); }
        debug_assert_eq!(saida.len(), tabela_principal);
        saida.extend(cabeca_principal.as_bytes());
        saida.extend(b"0000000000 65535 f \n");
        for o in principais { saida.extend(entrada(o).as_bytes()); }
        saida.extend(rodape.as_bytes());
        debug_assert_eq!(saida.len(), total);
        Ok(saida)
    }
}

#[cfg(test)]
mod testes {
    use crate::DocumentoPdf;

    #[test]
    fn primeira_pagina_vem_antes_e_o_arquivo_abre() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.duplicar_paginas(&[1]).unwrap();
        pdf.duplicar_paginas(&[1, 2]).unwrap();
        let bytes = pdf.linearizado().unwrap();
        if let Ok(p) = std::env::var("FEATHERPDF_LIN_SAIDA") { std::fs::write(p, &bytes).unwrap(); }
        let texto = String::from_utf8_lossy(&bytes);
        let lin = texto.find("/Linearized 1").unwrap();
        assert!(lin < 60, "o dicionário de linearização é o primeiro objeto");
        let l: usize = texto[lin..].split("/L ").nth(1).unwrap().split_whitespace().next().unwrap().parse().unwrap();
        assert_eq!(l, bytes.len());
        let relido = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        assert_eq!(relido.total_paginas(), 4);
        // A tabela principal fecha o arquivo e aponta para a da primeira página.
        let fim = texto.rsplit("startxref").next().unwrap().split_whitespace().next().unwrap().parse::<usize>().unwrap();
        assert!(texto[fim..].starts_with("xref"));
    }
}
