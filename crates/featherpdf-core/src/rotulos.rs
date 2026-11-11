//! Numeração lógica das páginas (/PageLabels): o nome que o leitor mostra para cada página
//! ("i", "ii", "A-1", "1"…), sem escrever nada na página.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Object};

use crate::DocumentoPdf;

/// Estilo do número do trecho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstiloRotulo {
    /// 1, 2, 3
    Decimal,
    /// i, ii, iii
    RomanoMinusculo,
    /// I, II, III
    RomanoMaiusculo,
    /// a, b, … z, aa, bb
    LetraMinuscula,
    /// A, B, … Z, AA, BB
    LetraMaiuscula,
    /// Só o prefixo.
    Nenhum,
}

impl EstiloRotulo {
    pub const TODOS: [EstiloRotulo; 6] = [Self::Decimal, Self::RomanoMinusculo, Self::RomanoMaiusculo,
        Self::LetraMinuscula, Self::LetraMaiuscula, Self::Nenhum];

    fn nome_pdf(self) -> Option<&'static [u8]> {
        match self {
            Self::Decimal => Some(b"D"), Self::RomanoMinusculo => Some(b"r"), Self::RomanoMaiusculo => Some(b"R"),
            Self::LetraMinuscula => Some(b"a"), Self::LetraMaiuscula => Some(b"A"), Self::Nenhum => None,
        }
    }

    fn do_pdf(nome: Option<&[u8]>) -> Self {
        match nome {
            Some(b"D") => Self::Decimal, Some(b"r") => Self::RomanoMinusculo, Some(b"R") => Self::RomanoMaiusculo,
            Some(b"a") => Self::LetraMinuscula, Some(b"A") => Self::LetraMaiuscula, _ => Self::Nenhum,
        }
    }

    /// O número `n` (1 em diante) escrito neste estilo.
    pub fn escrever(self, n: u32) -> String {
        // O número vem do arquivo: romano e letras de milhões de caracteres travavam a tela.
        if n > 4999 && !matches!(self, Self::Decimal | Self::Nenhum) { return n.to_string(); }
        match self {
            Self::Decimal => n.to_string(),
            Self::RomanoMinusculo => romano(n).to_lowercase(),
            Self::RomanoMaiusculo => romano(n),
            Self::LetraMinuscula => letras(n).to_lowercase(),
            Self::LetraMaiuscula => letras(n),
            Self::Nenhum => String::new(),
        }
    }
}

fn romano(mut n: u32) -> String {
    const TABELA: [(u32, &str); 13] = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"),
        (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut s = String::new();
    for (v, r) in TABELA { while n >= v { s += r; n -= v; } }
    s
}

/// Como o PDF pede: A…Z, depois AA…ZZ, AAA… (a letra repete, não é base 26).
fn letras(n: u32) -> String {
    if n == 0 { return String::new(); }
    let letra = (b'A' + ((n - 1) % 26) as u8) as char;
    letra.to_string().repeat(((n - 1) / 26 + 1) as usize)
}

/// Um trecho da numeração: começa na página `inicio` (1 em diante).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrechoRotulo {
    pub inicio: u32,
    pub estilo: EstiloRotulo,
    pub prefixo: String,
    /// Número da primeira página do trecho.
    pub primeiro: u32,
}

impl DocumentoPdf {
    /// Trechos da /PageLabels (índice 0 do PDF vira página 1). Vazio quando o PDF não tem.
    pub fn trechos_rotulos(&self) -> Vec<TrechoRotulo> {
        let doc = &self.documento;
        let resolver = |o: &Object| -> Option<Object> {
            match o { Object::Reference(r) => doc.get_object(*r).ok().cloned(), o => Some(o.clone()) }
        };
        let Some(raiz) = doc.catalog().ok().and_then(|c| c.get(b"PageLabels").ok()).and_then(resolver)
            .and_then(|o| o.as_dict().ok().cloned()) else { return Vec::new() };
        let mut saida = Vec::new();
        let mut pilha = vec![raiz];
        // Cada nó de /Kids uma vez só: um nó que lista a si mesmo mil vezes enchia a pilha.
        let mut vistos: std::collections::HashSet<lopdf::ObjectId> = std::collections::HashSet::new();
        // Árvore de números: folhas com /Nums [chave valor …], nós com /Kids.
        while let Some(no) = pilha.pop() {
            if let Ok(nums) = no.get(b"Nums").and_then(Object::as_array) {
                for par in nums.chunks(2) {
                    let [chave, valor] = par else { continue };
                    let (Some(i), Some(d)) = (resolver(chave).and_then(|c| c.as_i64().ok()), resolver(valor).and_then(|v| v.as_dict().ok().cloned()))
                        else { continue };
                    let Some(inicio) = u32::try_from(i).ok().and_then(|i| i.checked_add(1)) else { continue };
                    let prefixo = d.get(b"P").ok().and_then(|p| lopdf::decode_text_string(p).ok()).unwrap_or_default();
                    let primeiro = d.get(b"St").and_then(Object::as_i64).ok().and_then(|n| u32::try_from(n).ok()).filter(|&n| n > 0).unwrap_or(1);
                    saida.push(TrechoRotulo { inicio, estilo: EstiloRotulo::do_pdf(d.get(b"S").and_then(Object::as_name).ok()), prefixo, primeiro });
                }
            }
            if let Ok(filhos) = no.get(b"Kids").and_then(Object::as_array) {
                // Só referências entram: um filho direto não forma círculo.
                let novos = filhos.iter().filter(|k| match k { Object::Reference(r) => vistos.insert(*r), _ => true });
                pilha.extend(novos.filter_map(resolver).filter_map(|k| k.as_dict().ok().cloned()));
                if vistos.len() > 100_000 { break; }
            }
        }
        saida.sort_by_key(|t| t.inicio);
        saida.dedup_by_key(|t| t.inicio);
        saida
    }

    /// O nome de cada página como o leitor mostra ("i", "ii", "1"…). Sem /PageLabels, "1", "2"…
    pub fn rotulos_paginas(&self) -> Vec<String> {
        let total = self.total_paginas();
        let trechos = self.trechos_rotulos();
        (1..=total).map(|p| match trechos.iter().rev().find(|t| t.inicio <= p) {
            Some(t) => format!("{}{}", t.prefixo, t.estilo.escrever(t.primeiro.saturating_add(p - t.inicio))),
            // Antes do primeiro trecho o PDF não diz nada: fica o número da página.
            None => p.to_string(),
        }).collect()
    }

    /// O nome da página `p` quando o PDF tem numeração lógica e ele não é só o número dela
    /// (para a barra de baixo mostrar "iii" ao lado do 3). Lê só o catálogo: barato a cada desenho.
    pub fn rotulo_da_pagina(&self, p: u32) -> Option<String> {
        let trechos = self.trechos_rotulos();
        let t = trechos.iter().rev().find(|t| t.inicio <= p)?;
        let r = format!("{}{}", t.prefixo, t.estilo.escrever(t.primeiro.saturating_add(p - t.inicio)));
        (r != p.to_string()).then_some(r)
    }

    /// Dá às páginas `de..=ate` a numeração `estilo`, `prefixo` e `primeiro`; as páginas depois
    /// de `ate` continuam com o nome que tinham. Um Ctrl+Z (quem chama guarda o documento).
    pub fn numerar_logico(&mut self, de: u32, ate: u32, estilo: EstiloRotulo, prefixo: &str, primeiro: u32) -> Result<(), String> {
        let total = self.total_paginas();
        if de == 0 || de > ate || ate > total { return Err(format!("Use páginas de 1 a {total}, a primeira antes da última.")); }
        if primeiro == 0 { return Err("O primeiro número começa em 1.".into()); }
        if estilo == EstiloRotulo::Nenhum && prefixo.is_empty() { return Err("Sem estilo, escreva um prefixo (senão a página fica sem nome).".into()); }
        let antes = self.trechos_rotulos();
        let mut mapa: BTreeMap<u32, TrechoRotulo> = antes.iter().map(|t| (t.inicio, t.clone())).collect();
        // O trecho em vigor logo depois de `ate` precisa continuar de onde estava.
        if ate < total && !mapa.contains_key(&(ate + 1)) {
            let seguinte = ate + 1;
            let continua = match antes.iter().rev().find(|t| t.inicio <= seguinte) {
                Some(t) => TrechoRotulo { inicio: seguinte, primeiro: t.primeiro.saturating_add(seguinte - t.inicio), ..t.clone() },
                None => TrechoRotulo { inicio: seguinte, estilo: EstiloRotulo::Decimal, prefixo: String::new(), primeiro: seguinte },
            };
            mapa.insert(seguinte, continua);
        }
        mapa.retain(|&i, _| i < de || i > ate);
        mapa.insert(de, TrechoRotulo { inicio: de, estilo, prefixo: prefixo.to_string(), primeiro });
        // A página 1 sem trecho: o PDF pede a chave 0; o nome dela continua "1", "2"….
        mapa.entry(1).or_insert(TrechoRotulo { inicio: 1, estilo: EstiloRotulo::Decimal, prefixo: String::new(), primeiro: 1 });
        let mut nums = Vec::new();
        for t in mapa.values() {
            let mut d = Dictionary::new();
            d.set("Type", "PageLabel");
            if let Some(s) = t.estilo.nome_pdf() { d.set("S", Object::Name(s.to_vec())); }
            if !t.prefixo.is_empty() { d.set("P", lopdf::text_string(&t.prefixo)); }
            if t.primeiro != 1 { d.set("St", i64::from(t.primeiro)); }
            nums.push(Object::Integer(i64::from(t.inicio - 1)));
            nums.push(Object::Dictionary(d));
        }
        let mut raiz = Dictionary::new();
        raiz.set("Nums", nums);
        self.documento.catalog_mut().map_err(|e| e.to_string())?.set("PageLabels", raiz);
        self.modificado = true;
        Ok(())
    }

    /// Depois de mudar a ordem das páginas: cada página continua com o nome que tinha (`rotulos`,
    /// na ordem de `antes`); página nova leva o número da posição. Sequência decimal vira um trecho só.
    pub(crate) fn remapear_rotulos(&mut self, antes: &[lopdf::ObjectId], rotulos: &[String]) -> Result<(), String> {
        let nome: std::collections::HashMap<lopdf::ObjectId, &String> = antes.iter().copied().zip(rotulos).collect();
        let mut nums = Vec::new();
        let mut anterior: Option<u32> = None;
        for (i, id) in self.ids_paginas().iter().enumerate() {
            let r = nome.get(id).map_or_else(|| (i + 1).to_string(), |s| s.to_string());
            let n = r.parse::<u32>().ok().filter(|&n| n > 0 && !r.starts_with('0') && !r.starts_with('+'));
            if n.is_some() && anterior.and_then(|a| a.checked_add(1)) == n { anterior = n; continue; }
            let mut d = Dictionary::new();
            d.set("Type", "PageLabel");
            match n {
                Some(n) => { d.set("S", Object::Name(b"D".to_vec())); if n != 1 { d.set("St", i64::from(n)); } }
                None => d.set("P", lopdf::text_string(&r)),
            }
            nums.push(Object::Integer(i as i64));
            nums.push(Object::Dictionary(d));
            anterior = n;
        }
        let mut raiz = Dictionary::new();
        raiz.set("Nums", nums);
        self.documento.catalog_mut().map_err(|e| e.to_string())?.set("PageLabels", raiz);
        Ok(())
    }

    /// Tira a numeração lógica: as páginas voltam a "1", "2"…; devolve se havia.
    pub fn tirar_numeracao_logica(&mut self) -> Result<bool, String> {
        let havia = self.documento.catalog_mut().map_err(|e| e.to_string())?.remove(b"PageLabels").is_some();
        if havia { self.modificado = true; }
        Ok(havia)
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn numero_enorme_do_arquivo_nao_vira_texto_gigante() {
        assert_eq!(EstiloRotulo::RomanoMaiusculo.escrever(2_000_000_000), "2000000000");
        assert_eq!(EstiloRotulo::LetraMinuscula.escrever(4_000_000_000), "4000000000");
        assert_eq!(EstiloRotulo::RomanoMinusculo.escrever(14), "xiv");
    }

    use super::*;

    /// Audit: um nó de /Kids que lista a si mesmo mil vezes, e chave no limite do u32.
    #[test]
    fn arvore_em_circulo_e_chave_enorme_nao_derrubam() {
        use lopdf::{dictionary, Object};
        let mut pdf = crate::testes::pdf_teste();
        let no = pdf.documento.new_object_id();
        let folha = pdf.documento.add_object(dictionary! { "Nums" => vec![
            Object::Integer(0), Object::Dictionary(dictionary! { "S" => "r" }),
            Object::Integer(u32::MAX as i64), Object::Dictionary(dictionary! { "S" => "D" })] });
        let mut filhos = vec![Object::Reference(no); 1000];
        filhos.push(Object::Reference(folha));
        pdf.documento.objects.insert(no, Object::Dictionary(dictionary! { "Kids" => filhos }));
        pdf.documento.catalog_mut().unwrap().set("PageLabels", no);
        let trechos = pdf.trechos_rotulos();
        assert_eq!(trechos.len(), 1, "a chave que estoura sai; a folha conta uma vez: {trechos:?}");
        assert_eq!(pdf.rotulo_da_pagina(1).as_deref(), Some("i"));
    }

    #[test]
    fn estilos_escrevem_como_o_pdf() {
        assert_eq!(EstiloRotulo::RomanoMinusculo.escrever(14), "xiv");
        assert_eq!(EstiloRotulo::RomanoMaiusculo.escrever(1994), "MCMXCIV");
        assert_eq!(EstiloRotulo::LetraMaiuscula.escrever(1), "A");
        assert_eq!(EstiloRotulo::LetraMinuscula.escrever(28), "bb");
        assert_eq!(EstiloRotulo::Nenhum.escrever(3), "");
    }

    #[test]
    fn prefacio_em_romano_e_o_resto_continua() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.inserir_em_branco(2).unwrap();
        pdf.inserir_em_branco(3).unwrap();
        pdf.inserir_em_branco(4).unwrap();
        assert_eq!(pdf.rotulos_paginas(), ["1", "2", "3", "4"]);
        pdf.numerar_logico(1, 2, EstiloRotulo::RomanoMinusculo, "", 1).unwrap();
        assert_eq!(pdf.rotulos_paginas(), ["i", "ii", "3", "4"]);
        pdf.numerar_logico(3, 4, EstiloRotulo::Decimal, "A-", 1).unwrap();
        let mut pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(pdf.rotulos_paginas(), ["i", "ii", "A-1", "A-2"]);
        pdf.numerar_logico(2, 2, EstiloRotulo::LetraMaiuscula, "", 5).unwrap();
        assert_eq!(pdf.rotulos_paginas(), ["i", "E", "A-1", "A-2"]);
        assert_eq!(pdf.rotulo_da_pagina(3).as_deref(), Some("A-1"));
        // Excluir e inserir: cada página leva o nome que tinha; a nova, o número da posição.
        let mut outro = pdf.clone();
        outro.excluir_paginas(&[2]).unwrap();
        assert_eq!(outro.rotulos_paginas(), ["i", "A-1", "A-2"]);
        outro.inserir_em_branco(1).unwrap();
        assert_eq!(outro.rotulos_paginas(), ["1", "i", "A-1", "A-2"]);
        assert!(pdf.numerar_logico(3, 9, EstiloRotulo::Decimal, "", 1).is_err());
        assert!(pdf.tirar_numeracao_logica().unwrap());
        assert_eq!(pdf.rotulos_paginas(), ["1", "2", "3", "4"]);
        assert_eq!(pdf.rotulo_da_pagina(3), None);
    }
}
