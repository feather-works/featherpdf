//! Converter → FeatherDocs: o texto das páginas volta a ser parágrafos
//! corridos (o PDF só guarda linhas soltas), com negrito, tamanho, centro,
//! recuo e as imagens no lugar em que aparecem. Formulário com bordas (o
//! comprovante de CNPJ) vira tabelas: uma por faixa de caixas, uma célula por
//! caixa, com o rótulo e o valor em parágrafos dentro dela.

use crate::tabela::{grade_da_pagina, linhas_de_texto, Item};
use crate::DocumentoPdf;

#[derive(Debug, Clone, PartialEq)]
pub enum BlocoFluxo {
    /// Trechos (texto, negrito); recuo e primeira linha em pt a partir da margem.
    Paragrafo { trechos: Vec<(String, bool)>, tamanho: f32, centro: bool, recuo: f32, primeira: f32 },
    /// Imagem da página (caixa no espaço dela), com o recuo a partir da margem.
    Imagem { caixa: [f32; 4], recuo: f32 },
    /// Uma faixa de caixas do formulário: uma linha de tabela, recuada a partir da margem.
    Tabela { recuo: f32, celulas: Vec<CelulaFluxo> },
}

/// Caixa do formulário: largura (pt) e os parágrafos (trechos, tamanho) de dentro.
#[derive(Debug, Clone, PartialEq)]
pub struct CelulaFluxo { pub largura: f32, pub paragrafos: Vec<(Vec<(String, bool)>, f32)> }

#[derive(Debug, Clone, Default)]
pub struct PaginaFluxo {
    pub pagina: u32,
    pub largura: f32,
    pub altura: f32,
    /// Margens tiradas do que está desenhado: esquerda, cima, direita, baixo.
    pub margens: [f32; 4],
    pub blocos: Vec<BlocoFluxo>,
}

/// Uma linha da página: o texto já em trechos, e onde ela está.
struct Linha { trechos: Vec<(String, bool)>, x0: f32, x1: f32, baixo: f32, topo: f32, tamanho: f32 }

impl DocumentoPdf {
    /// As páginas como texto corrido.
    /// shortcut: usa o espaço da página sem o /Rotate; página girada sai na ordem dela.
    pub fn paginas_em_fluxo(&self) -> Result<Vec<PaginaFluxo>, String> {
        let mut saida = Vec::new();
        for p in 1..=self.total_paginas() {
            let (itens, figuras, segmentos) = self.conteudo(p)?;
            let ([x0, y0, x1, y1], _) = self.geometria(p)?;
            let tudo = itens.iter().map(|i| i.caixa).chain(figuras.iter().copied()).chain(segmentos.iter().copied())
                .fold([x1, y1, x0, y0], |a, c| [a[0].min(c[0]), a[1].min(c[1]), a[2].max(c[2]), a[3].max(c[3])]);
            let mut pagina = fluxo_com_tabelas(itens, &figuras, &segmentos, (tudo[0], tudo[2]));
            pagina.pagina = p;
            pagina.largura = x1 - x0;
            pagina.altura = y1 - y0;
            pagina.margens = [(tudo[0] - x0).clamp(18.0, 108.0), (y1 - tudo[3]).clamp(18.0, 108.0),
                (x1 - tudo[2]).clamp(18.0, 108.0), (tudo[1] - y0).clamp(18.0, 108.0)];
            saida.push(pagina);
        }
        Ok(saida)
    }
}

/// Separa o formulário (o que está dentro das bordas) do texto corrido em volta.
fn fluxo_com_tabelas(itens: Vec<Item>, figuras: &[[f32; 4]], segmentos: &[[f32; 4]], margens: (f32, f32)) -> PaginaFluxo {
    let tem_tabela = segmentos.iter().any(|s| s[3] - s[1] < 0.5) && segmentos.iter().any(|s| s[2] - s[0] < 0.5);
    if !tem_tabela {
        return PaginaFluxo { blocos: fluxo_da_pagina(&itens, figuras, margens).into_iter().map(|(_, b)| b).collect(), ..Default::default() };
    }
    let area = segmentos.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, c| [a[0].min(c[0]), a[1].min(c[1]), a[2].max(c[2]), a[3].max(c[3])]);
    let dentro = |c: &[f32; 4]| {
        let (x, y) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
        x >= area[0] && x <= area[2] && y >= area[1] && y <= area[3]
    };
    let (no_formulario, fora): (Vec<Item>, Vec<Item>) = itens.into_iter().partition(|i| dentro(&i.caixa));
    let mut blocos = fluxo_da_pagina(&fora, figuras, margens);
    let g = grade_da_pagina(&no_formulario, &[], segmentos);
    let topo_da_linha = |r: usize| g.origem.1 - g.alturas[..r].iter().sum::<f32>();
    let x_da_coluna = |c: usize| g.origem.0 + g.larguras[..c].iter().sum::<f32>();
    let borda = |l: usize, c: usize| g.bordas.iter().find(|b| b.0 == l && b.1 == c).map(|b| b.2).unwrap_or_default();
    let nc = g.larguras.len();
    // Faixas: começam na linha da grade que tem borda em cima.
    let mut inicios: Vec<usize> = (0..g.alturas.len()).filter(|&l| (0..nc).any(|c| borda(l, c)[0])).collect();
    inicios.push(g.alturas.len());
    for faixa in inicios.windows(2) {
        let linhas = faixa[0]..faixa[1];
        // Caixas: cortadas pelas bordas verticais da faixa.
        let mut cortes: Vec<usize> = (0..=nc).filter(|&c| linhas.clone().any(|l| borda(l, c)[2] || (c > 0 && borda(l, c - 1)[3]))).collect();
        cortes.dedup();
        let celulas: Vec<CelulaFluxo> = cortes.windows(2).map(|w| {
            let mut paragrafos: Vec<(Vec<(String, bool)>, f32)> = Vec::new();
            for l in linhas.clone() {
                let mut nesta: Vec<_> = g.celulas.iter().filter(|c| c.linha == l && c.coluna >= w[0] && c.coluna < w[1]).collect();
                if nesta.is_empty() { continue; }
                nesta.sort_by_key(|c| c.coluna);
                let trechos = nesta.iter().enumerate().map(|(i, c)| (if i == 0 { c.texto.clone() } else { format!(" {}", c.texto) }, c.negrito)).collect();
                paragrafos.push((trechos, nesta.iter().map(|c| c.tamanho).fold(0.0, f32::max)));
            }
            CelulaFluxo { largura: x_da_coluna(w[1]) - x_da_coluna(w[0]), paragrafos }
        }).collect();
        if celulas.is_empty() { continue; }
        let recuo = cortes.first().map_or(0.0, |&c| x_da_coluna(c) - margens.0);
        blocos.push((topo_da_linha(faixa[0]), BlocoFluxo::Tabela { recuo, celulas }));
    }
    // De cima para baixo; no empate, a ordem em que entraram.
    blocos.sort_by(|a, b| b.0.total_cmp(&a.0));
    PaginaFluxo { blocos: blocos.into_iter().map(|(_, b)| b).collect(), ..Default::default() }
}

/// Parágrafos e imagens de cima para baixo, cada um com a altura do topo.
fn fluxo_da_pagina(itens: &[Item], figuras: &[[f32; 4]], (esquerda, direita): (f32, f32)) -> Vec<(f32, BlocoFluxo)> {
    let caixas: Vec<[f32; 4]> = itens.iter().map(|i| i.caixa).collect();
    let linhas: Vec<Linha> = linhas_de_texto(&caixas).into_iter().map(|(baixo, topo, mut v)| {
        v.sort_by(|&a, &b| caixas[a][0].total_cmp(&caixas[b][0]));
        // Pedaços seguidos com o mesmo peso viram um trecho só.
        let mut trechos: Vec<(String, bool)> = Vec::new();
        for &k in &v {
            match trechos.last_mut() {
                Some((t, n)) if *n == itens[k].negrito => { t.push(' '); t.push_str(&itens[k].texto); }
                Some(_) => trechos.push((format!(" {}", itens[k].texto), itens[k].negrito)),
                None => trechos.push((itens[k].texto.clone(), itens[k].negrito)),
            }
        }
        Linha {
            trechos, baixo, topo,
            x0: v.iter().map(|&k| caixas[k][0]).fold(f32::MAX, f32::min),
            x1: v.iter().map(|&k| caixas[k][2]).fold(f32::MIN, f32::max),
            tamanho: v.iter().map(|&k| itens[k].tamanho).fold(0.0, f32::max),
        }
    }).collect();
    let (meio, largura) = ((esquerda + direita) / 2.0, direita - esquerda);
    let centrada = |l: &Linha| (l.x1 - l.x0) < largura * 0.85 && (((l.x0 + l.x1) / 2.0) - meio).abs() < 6.0;

    let mut blocos: Vec<(f32, BlocoFluxo)> = Vec::new();
    // Parágrafo aberto: blocos[i], x da segunda linha e a linha anterior.
    let mut aberto: Option<(usize, Option<f32>)> = None;
    for (i, l) in linhas.iter().enumerate() {
        let continua = match (aberto, i.checked_sub(1).map(|j| &linhas[j])) {
            (Some((_, segunda)), Some(ant)) => {
                let vao = ant.baixo - l.topo;
                (ant.tamanho - l.tamanho).abs() <= 0.6
                    && vao < l.tamanho * 0.9
                    && !centrada(ant) && !centrada(l)
                    // Linha anterior que acaba bem antes da margem direita fecha o parágrafo.
                    && ant.x1 > direita - (3.0 * l.tamanho).max(40.0)
                    && segunda.is_none_or(|x| (l.x0 - x).abs() < 10.0)
            }
            _ => false,
        };
        if continua {
            let (k, segunda) = aberto.as_mut().unwrap();
            if segunda.is_none() { *segunda = Some(l.x0); }
            if let BlocoFluxo::Paragrafo { trechos, recuo, primeira, .. } = &mut blocos[*k].1 {
                let inicio = *recuo + *primeira;
                *recuo = l.x0 - esquerda;
                *primeira = inicio - *recuo;
                // Cada trecho depois do primeiro já começa com o espaço.
                for (t, n) in &l.trechos {
                    let t = t.trim_start();
                    match trechos.last_mut() {
                        Some((u, m)) if m == n => { u.push(' '); u.push_str(t); }
                        _ => trechos.push((format!(" {t}"), *n)),
                    }
                }
            }
        } else {
            blocos.push((l.topo, BlocoFluxo::Paragrafo {
                trechos: l.trechos.clone(), tamanho: l.tamanho, centro: centrada(l), recuo: l.x0 - esquerda, primeira: 0.0,
            }));
            aberto = Some((blocos.len() - 1, None));
        }
    }
    // Imagens entram pela altura do topo; na mesma altura, da esquerda para a direita.
    for &f in figuras {
        let pos = blocos.iter().position(|(topo, _)| *topo < f[3]).unwrap_or(blocos.len());
        blocos.insert(pos, (f[3], BlocoFluxo::Imagem { caixa: f, recuo: f[0] - esquerda }));
    }
    blocos
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(texto: &str, x0: f32, x1: f32, y: f32, tamanho: f32, negrito: bool) -> Item {
        Item { texto: texto.into(), caixa: [x0, y, x1, y + tamanho], tamanho, negrito }
    }

    #[test]
    fn linhas_viram_paragrafos() {
        let itens = vec![
            t("Titulo do documento", 285.0, 379.0, 780.0, 14.0, true),
            // Parágrafo justificado de três linhas (a última curta), 11 pt com 14 pt entre linhas.
            t("O BNDES e a Secretaria-Geral tornam publica a chamada para", 72.0, 590.0, 740.0, 11.0, false),
            t("recebimento de propostas de", 72.0, 200.0, 726.0, 11.0, false),
            t("entidades executoras", 207.0, 305.0, 726.0, 11.0, true),
            t("para a realizacao de projetos estruturantes no territorio", 312.0, 592.0, 726.0, 11.0, false),
            t("atingido.", 72.0, 115.0, 712.0, 11.0, false),
            t("Segundo paragrafo comeca aqui e vai ate o fim da linha certo", 72.0, 591.0, 690.0, 11.0, false),
        ];
        let p = fluxo_com_tabelas(itens, &[[72.0, 790.0, 150.0, 810.0]], &[], (72.0, 592.0));
        let textos: Vec<String> = p.blocos.iter().map(|b| match b {
            BlocoFluxo::Paragrafo { trechos, .. } => trechos.iter().map(|t| t.0.as_str()).collect::<String>(),
            BlocoFluxo::Imagem { .. } => "[imagem]".into(),
            BlocoFluxo::Tabela { .. } => "[tabela]".into(),
        }).collect();
        assert_eq!(textos, [
            "[imagem]",
            "Titulo do documento",
            "O BNDES e a Secretaria-Geral tornam publica a chamada para recebimento de propostas de entidades executoras para a realizacao de projetos estruturantes no territorio atingido.",
            "Segundo paragrafo comeca aqui e vai ate o fim da linha certo",
        ]);
        let BlocoFluxo::Paragrafo { centro, tamanho, .. } = &p.blocos[1] else { panic!() };
        assert!(*centro && *tamanho == 14.0);
        let BlocoFluxo::Paragrafo { trechos, .. } = &p.blocos[2] else { panic!() };
        assert_eq!((trechos[1].0.trim(), trechos[1].1), ("entidades executoras", true), "o negrito do meio vira um trecho");
    }

    #[test]
    fn formulario_vira_uma_tabela_por_faixa() {
        // Faixa 1: duas caixas (40–300, 300–560); faixa 2: uma caixa larga. Título fora.
        let segs = vec![
            [40.0, 720.0, 560.0, 720.0], [40.0, 680.0, 560.0, 680.0], [40.0, 640.0, 560.0, 640.0],
            [40.0, 640.0, 40.0, 720.0], [300.0, 680.0, 300.0, 720.0], [560.0, 640.0, 560.0, 720.0],
        ];
        let itens = vec![
            t("CADASTRO NACIONAL", 230.0, 370.0, 740.0, 12.0, true),
            t("NOME EMPRESARIAL", 43.0, 110.0, 710.0, 6.0, false), t("DATA", 303.0, 320.0, 710.0, 6.0, false),
            t("INSTITUTO", 43.0, 100.0, 690.0, 9.0, true), t("09/08/2013", 303.0, 350.0, 690.0, 9.0, true),
            t("MOTIVO", 43.0, 80.0, 670.0, 6.0, false),
        ];
        let p = fluxo_com_tabelas(itens, &[], &segs, (40.0, 560.0));
        assert!(matches!(&p.blocos[0], BlocoFluxo::Paragrafo { trechos, .. } if trechos[0].0 == "CADASTRO NACIONAL"));
        let BlocoFluxo::Tabela { celulas, .. } = &p.blocos[1] else { panic!("{:?}", p.blocos[1]) };
        assert_eq!(celulas.len(), 2);
        assert_eq!(celulas[0].largura, 260.0);
        let textos: Vec<&str> = celulas[1].paragrafos.iter().map(|p| p.0[0].0.as_str()).collect();
        assert_eq!(textos, ["DATA", "09/08/2013"], "rótulo em cima, valor embaixo, na mesma célula");
        let BlocoFluxo::Tabela { celulas, .. } = &p.blocos[2] else { panic!() };
        assert_eq!((celulas.len(), celulas[0].largura), (1, 520.0));
    }
}
