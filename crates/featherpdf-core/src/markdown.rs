//! Converter → Markdown: o mesmo texto corrido do FeatherDocs (`fluxo.rs`) em Markdown.
//! Letra bem maior que a do corpo vira título, negrito vira `**…**`, as faixas de caixas
//! de um formulário viram tabela e cada imagem deixa uma marca no lugar.

use crate::fluxo::{BlocoFluxo, PaginaFluxo};

/// Escapa o que o Markdown leria como formatação.
fn escapar(t: &str) -> String {
    let mut s = String::with_capacity(t.len());
    for c in t.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '#' | '|' | '<' | '>') { s.push('\\'); }
        s.push(c);
    }
    s
}

fn trechos(v: &[(String, bool)]) -> String {
    let mut s = String::new();
    for (t, negrito) in v {
        let (inicio, meio, fim) = (t.len() - t.trim_start().len(), t.trim(), t.len() - t.trim_end().len());
        s.push_str(&t[..inicio]);
        if *negrito && !meio.is_empty() { s += &format!("**{}**", escapar(meio)); } else { s += &escapar(meio); }
        s.push_str(&t[t.len() - fim..]);
    }
    s.trim().to_string()
}

/// Tamanho de letra do corpo: o mais comum entre os parágrafos (pesado pelo comprimento).
fn tamanho_do_corpo(paginas: &[PaginaFluxo]) -> f32 {
    let mut contagem: Vec<(i32, usize)> = Vec::new();
    for b in paginas.iter().flat_map(|p| &p.blocos) {
        if let BlocoFluxo::Paragrafo { trechos, tamanho, .. } = b {
            let k = (tamanho * 2.0).round() as i32;
            let n: usize = trechos.iter().map(|t| t.0.len()).sum();
            match contagem.iter_mut().find(|c| c.0 == k) { Some(c) => c.1 += n, None => contagem.push((k, n)) }
        }
    }
    contagem.iter().max_by_key(|c| c.1).map_or(11.0, |c| c.0 as f32 / 2.0)
}

/// As páginas em Markdown, separadas por uma linha horizontal.
pub fn markdown(paginas: &[PaginaFluxo]) -> String {
    let corpo = tamanho_do_corpo(paginas);
    let mut saida: Vec<String> = Vec::new();
    for p in paginas {
        let mut blocos: Vec<String> = Vec::new();
        let mut tabela: Vec<Vec<String>> = Vec::new();
        let fechar_tabela = |tabela: &mut Vec<Vec<String>>, blocos: &mut Vec<String>| {
            if tabela.is_empty() { return; }
            let colunas = tabela.iter().map(Vec::len).max().unwrap_or(1);
            let linha = |l: &Vec<String>| format!("| {} |", (0..colunas).map(|i| l.get(i).map_or("", |s| s.as_str())).collect::<Vec<_>>().join(" | "));
            let mut s = vec![linha(&tabela[0]), format!("|{}", " --- |".repeat(colunas))];
            s.extend(tabela[1..].iter().map(linha));
            blocos.push(s.join("\n"));
            tabela.clear();
        };
        for b in &p.blocos {
            match b {
                BlocoFluxo::Tabela { celulas, .. } => tabela.push(celulas.iter().map(|c| c.paragrafos.iter()
                    .map(|(t, _)| trechos(t)).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("<br>")).collect()),
                outro => {
                    fechar_tabela(&mut tabela, &mut blocos);
                    match outro {
                        BlocoFluxo::Paragrafo { trechos: t, tamanho, .. } => {
                            let texto = trechos(t);
                            if texto.is_empty() { continue; }
                            let titulo = if *tamanho >= corpo * 1.6 { "# " } else if *tamanho >= corpo * 1.25 { "## " } else { "" };
                            // Título sai sem as estrelas do negrito (já é destaque).
                            blocos.push(if titulo.is_empty() { texto } else { format!("{titulo}{}", texto.replace("**", "")) });
                        }
                        BlocoFluxo::Imagem { .. } => blocos.push(format!("*[Imagem da página {}]*", p.pagina)),
                        BlocoFluxo::Tabela { .. } => {}
                    }
                }
            }
        }
        fechar_tabela(&mut tabela, &mut blocos);
        if !blocos.is_empty() { saida.push(blocos.join("\n\n")); }
    }
    let mut s = saida.join("\n\n---\n\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::fluxo::CelulaFluxo;

    fn par(t: &str, negrito: bool, tamanho: f32) -> BlocoFluxo {
        BlocoFluxo::Paragrafo { trechos: vec![(t.into(), negrito)], tamanho, centro: false, recuo: 0.0, primeira: 0.0 }
    }

    #[test]
    fn titulos_negrito_e_tabela() {
        let cel = |t: &str| CelulaFluxo { largura: 100.0, paragrafos: vec![(vec![(t.into(), false)], 10.0)] };
        let p1 = PaginaFluxo { pagina: 1, blocos: vec![
            par("Relatório", true, 24.0), par("Corpo do texto com 5 * 3.", false, 11.0),
            BlocoFluxo::Paragrafo { trechos: vec![("Atenção: ".into(), true), ("leia".into(), false)], tamanho: 11.0, centro: false, recuo: 0.0, primeira: 0.0 },
            BlocoFluxo::Tabela { recuo: 0.0, celulas: vec![cel("Nome"), cel("CPF")] },
            BlocoFluxo::Tabela { recuo: 0.0, celulas: vec![cel("Ana"), cel("1")] },
        ], ..Default::default() };
        let p2 = PaginaFluxo { pagina: 2, blocos: vec![par("Seção", false, 14.0), BlocoFluxo::Imagem { caixa: [0.0; 4], recuo: 0.0 }], ..Default::default() };
        let md = markdown(&[p1, p2]);
        assert_eq!(md, "# Relatório\n\nCorpo do texto com 5 \\* 3.\n\n**Atenção:** leia\n\n| Nome | CPF |\n| --- | --- |\n| Ana | 1 |\n\n---\n\n## Seção\n\n*[Imagem da página 2]*\n");
    }
}
