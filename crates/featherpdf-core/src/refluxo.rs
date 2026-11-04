//! Substituir com refluxo: trocar uma palavra por uma frase (ou o contrário) e refazer as
//! quebras do parágrafo, para o texto não passar da margem nem atropelar a linha de baixo.
//! As palavras andam entre as linhas que já existem; se ainda sobrar, a última linha aperta
//! (largura das letras) até 75%, e abaixo disso a troca é recusada.

use crate::{DocumentoPdf, EstiloTexto, ObjetoPdf, TipoObjeto};

/// Menor aperto aceito na última linha (%).
const APERTO_MINIMO: f32 = 75.0;

impl DocumentoPdf {
    /// Troca `busca` por `novo` na linha `objeto` e refaz o parágrafo dela. Um Ctrl+Z (quem
    /// chama guarda o documento antes); em erro, o documento pode ter ficado pela metade e
    /// quem chama volta a cópia.
    pub fn substituir_com_refluxo(&mut self, pagina: u32, objeto: usize, busca: &str, novo: &str) -> Result<String, String> {
        if busca.is_empty() { return Err("Escreva o que procurar.".into()); }
        let objs = self.objetos(pagina)?;
        let texto_de = |o: &ObjetoPdf| match &o.tipo { TipoObjeto::Texto { texto, tamanho, .. } => Some((texto.clone(), *tamanho)), _ => None };
        let (t0, _) = objs.get(objeto).and_then(texto_de).ok_or("Escolha uma linha de texto.")?;
        let (trocado, n) = crate::objetos::trocar_sem_caixa(&t0, busca, novo);
        if n == 0 { return Err(format!("“{busca}” não está nessa linha.")); }
        // O parágrafo: as linhas seguintes na mesma coluna, mesmo tamanho e mesma entrelinha.
        let c0 = objs[objeto].caixa;
        let linhas = crate::paragrafos::paragrafo_por_geometria(&objs, objeto, true);
        let textos: Vec<String> = linhas.iter().map(|&k| texto_de(&objs[k]).map_or(String::new(), |t| t.0)).collect();
        // Margem direita: a da linha mais comprida; com uma linha só, a das outras linhas que
        // começam no mesmo x (a coluna, para não invadir a do lado) ou, sem elas, a mesma folga da esquerda.
        let (geo, _) = self.geometria(pagina)?;
        let mut margem = linhas.iter().map(|&k| objs[k].caixa[2]).fold(f32::MIN, f32::max);
        if linhas.len() == 1 {
            let coluna = objs.iter().filter(|o| matches!(o.tipo, TipoObjeto::Texto { .. }) && (o.caixa[0] - c0[0]).abs() <= 2.0)
                .map(|o| o.caixa[2]).fold(margem, f32::max);
            margem = if coluna > margem { coluna } else { margem.max(geo[2] - (c0[0] - geo[0])) };
        }
        let palavras: Vec<String> = std::iter::once(trocado.as_str()).chain(textos[1..].iter().map(String::as_str))
            .flat_map(str::split_whitespace).map(String::from).collect();
        // Medidas: cada palavra e o espaço, na fonte da linha, levadas a pontos pela largura que a linha já tem.
        let mut pedidos: Vec<&str> = palavras.iter().map(String::as_str).collect();
        pedidos.extend([" ", t0.as_str()]);
        let u = self.unidades_texto(pagina, objeto, &pedidos)?;
        let (espaco, original) = (u[palavras.len()], u[palavras.len() + 1]);
        let escala = (c0[2] - c0[0]) / original.max(1.0);
        let largura = |de: usize, ate: usize| (u[de..ate].iter().sum::<f32>() + espaco * (ate - de).saturating_sub(1) as f32) * escala;
        let mut novas: Vec<String> = Vec::new();
        let mut i = 0;
        for (j, &k) in linhas.iter().enumerate() {
            let cabe = margem - objs[k].caixa[0] + 0.5;
            let ultima_linha = j + 1 == linhas.len();
            let mut fim = i;
            while fim < palavras.len() && (fim == i || largura(i, fim + 1) <= cabe) { fim += 1; }
            if ultima_linha { fim = palavras.len(); }
            novas.push(palavras[i..fim].join(" "));
            i = fim;
        }
        // O que sobrou foi para a última linha: aperta se passou da margem.
        let k_ult = *linhas.last().unwrap_or(&objeto);
        let cabe = margem - objs[k_ult].caixa[0] + 0.5;
        let inicio_ult = palavras.len() - novas.last().map_or(0, |l| l.split_whitespace().count());
        let w = largura(inicio_ult, palavras.len());
        let aperto = if w > cabe && inicio_ult < palavras.len() { Some(cabe / w * 100.0) } else { None };
        if let Some(a) = aperto.filter(|&a| a < APERTO_MINIMO) {
            return Err(format!("A frase nova não cabe no parágrafo (precisaria apertar a última linha para {a:.0}%). Encurte o texto."));
        }
        let mut mexidas = 0;
        let mut vazias = Vec::new();
        for ((&k, nova), antiga) in linhas.iter().zip(&novas).zip(&textos) {
            if nova.is_empty() { vazias.push(k); continue; }
            if nova.trim() == antiga.trim() { continue; }
            self.editar_texto(pagina, k, nova)?;
            mexidas += 1;
        }
        if let Some(a) = aperto { self.estilizar_texto(pagina, &[k_ult], &EstiloTexto { horizontal: Some(a), ..Default::default() })?; }
        if !vazias.is_empty() { self.excluir_objetos(pagina, &vazias)?; }
        let mut aviso = format!("{n} troca(s); {mexidas} linha(s) do parágrafo refeitas");
        if let Some(a) = aperto { aviso += &format!(", a última apertada para {a:.0}%"); }
        if !vazias.is_empty() { aviso += &format!(", {} linha(s) a menos", vazias.len()); }
        Ok(aviso + ".")
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;
    use crate::TipoObjeto;

    fn linhas(pdf: &crate::DocumentoPdf) -> Vec<(String, f32)> {
        pdf.objetos(1).unwrap().into_iter().filter_map(|o| match o.tipo { TipoObjeto::Texto { texto, .. } => Some((texto, o.caixa[2])), _ => None }).collect()
    }

    #[test]
    fn frase_comprida_desce_para_a_linha_de_baixo() {
        let mut pdf = pdf_com("BT /F1 12 Tf 100 700 Td (O gato subiu no muro alto) Tj 0 -14 Td (e ficou por la a tarde toda) Tj 0 -14 Td (ate escurecer.) Tj ET");
        let margem = linhas(&pdf).iter().map(|l| l.1).fold(0.0, f32::max);
        let aviso = pdf.substituir_com_refluxo(1, 0, "gato", "gato preto").unwrap();
        let depois = linhas(&pdf);
        assert_eq!(depois.len(), 3, "{depois:?}");
        let tudo = depois.iter().map(|l| l.0.as_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(tudo, "O gato preto subiu no muro alto e ficou por la a tarde toda ate escurecer.");
        assert!(depois.iter().all(|l| l.1 <= margem + 1.0), "nenhuma linha passa da margem {margem}: {depois:?} ({aviso})");
        // Ao contrário: encurtar puxa palavras para cima.
        let mut pdf = pdf_com("BT /F1 12 Tf 100 700 Td (O gato preto e muito curioso subiu) Tj 0 -14 Td (no muro.) Tj ET");
        pdf.substituir_com_refluxo(1, 0, " preto e muito curioso", "").unwrap();
        assert_eq!(linhas(&pdf).iter().map(|l| l.0.clone()).collect::<Vec<_>>(), vec!["O gato subiu no muro."]);
        assert!(pdf.substituir_com_refluxo(1, 0, "cachorro", "x").is_err());
        // Frase que não cabe nem apertando: recusa sem mexer.
        let mut pdf = pdf_com("BT /F1 12 Tf 100 700 Td (O gato subiu no muro alto) Tj 0 -14 Td (e ficou por la a tarde toda) Tj 0 -14 Td (ate escurecer.) Tj ET");
        let antes = linhas(&pdf);
        assert!(pdf.substituir_com_refluxo(1, 0, "gato", "gato preto e muito curioso").is_err());
        assert_eq!(linhas(&pdf), antes);
    }

    #[test]
    fn linha_sozinha_nao_invade_a_coluna_do_lado() {
        // Duas colunas: a linha sozinha (x=50) fica longe da outra linha da mesma coluna, que
        // marca a margem; a coluna da direita começa em x=320.
        let mut pdf = pdf_com("BT /F1 10 Tf 50 700 Td (Um gato) Tj 0 -200 Td (Esta linha da esquerda vai ate aqui) Tj ET \
            BT /F1 10 Tf 320 700 Td (Coluna da direita com texto) Tj ET");
        let margem = linhas(&pdf)[1].1;
        assert!(margem < 320.0);
        let aviso = pdf.substituir_com_refluxo(1, 0, "gato", "gato preto e branco subiu no muro").unwrap();
        assert!(aviso.contains("apertada"), "{aviso}");
        assert!(linhas(&pdf).iter().filter(|l| l.0.contains("gato")).all(|l| l.1 <= margem + 1.0), "{:?}", linhas(&pdf));
    }
}
