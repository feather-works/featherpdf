//! Converter → FeatherSheet: a página vira uma grade de planilha. Com linhas de
//! tabela desenhadas (comprovante de CNPJ, nota, extrato), as colunas e linhas
//! seguem as bordas: cada caixa vira uma célula mesclada com a borda dela, e o
//! rótulo e o valor de dentro ficam em linhas próprias. Sem bordas, as colunas
//! saem dos inícios de texto alinhados.

use crate::objetos::TipoObjeto;
use crate::DocumentoPdf;

/// Distância (pt) até a qual dois inícios de texto contam como a mesma coluna.
const TOLERANCIA_COLUNA: f32 = 3.0;
/// Bordas mais perto que isso (pt) são a mesma (traço duplo, contorno grosso).
const TOLERANCIA_BORDA: f32 = 2.0;
/// Fresta vazia entre duas caixas mais fina que isso (pt) some: as bordas viram uma só.
const FRESTA: f32 = 8.0;

/// Uma célula com texto; `ate_coluna > coluna` quando mesclada para a direita.
#[derive(Debug, Clone, PartialEq)]
pub struct CelulaGrade {
    pub linha: usize,
    pub coluna: usize,
    pub ate_coluna: usize,
    pub texto: String,
    pub tamanho: f32,
    pub negrito: bool,
}

/// Imagem da página presa a uma célula: deslocamento e tamanho em pontos, e a
/// caixa original na página (para desenhar o recorte).
#[derive(Debug, Clone, PartialEq)]
pub struct ImagemGrade {
    pub linha: usize,
    pub coluna: usize,
    pub dx: f32,
    pub dy: f32,
    pub largura: f32,
    pub altura: f32,
    pub caixa: [f32; 4],
}

/// Uma página como planilha. Larguras e alturas em pontos (vazias: tamanho padrão).
#[derive(Debug, Clone, Default)]
pub struct GradePagina {
    pub pagina: u32,
    pub larguras: Vec<f32>,
    /// Canto de cima à esquerda da grade, no espaço da página (sem bordas: zero).
    pub origem: (f32, f32),
    pub alturas: Vec<f32>,
    pub celulas: Vec<CelulaGrade>,
    /// Bordas de cada célula (linha, coluna): cima, baixo, esquerda, direita.
    pub bordas: Vec<(usize, usize, [bool; 4])>,
    /// Imagens e desenhos agrupados (brasão, código QR, código de barras).
    pub imagens: Vec<ImagemGrade>,
}

pub(crate) struct Item { pub texto: String, pub caixa: [f32; 4], pub tamanho: f32, pub negrito: bool }

impl DocumentoPdf {
    /// Uma grade por página com texto ou imagem.
    /// shortcut: usa o espaço da página sem o /Rotate; página girada sai deitada.
    pub fn paginas_em_grade(&self) -> Result<Vec<GradePagina>, String> {
        let mut saida = Vec::new();
        for p in 1..=self.total_paginas() {
            let (itens, figuras, segmentos) = self.conteudo(p)?;
            if itens.is_empty() && figuras.is_empty() { continue; }
            saida.push(GradePagina { pagina: p, ..grade_da_pagina(&itens, &figuras, &segmentos) });
        }
        Ok(saida)
    }

    /// Texto (com o tamanho que aparece), imagens já juntas e as bordas da página.
    /// Código de barras desenhado com traços vira imagem, e os traços dele saem das bordas.
    pub(crate) fn conteudo(&self, p: u32) -> Result<(Vec<Item>, Vec<[f32; 4]>, Vec<[f32; 4]>), String> {
        let objetos = self.objetos(p)?;
        let (barras, segmentos) = codigos_de_barras(self.segmentos(p)?);
        let figuras: Vec<[f32; 4]> = juntar_figuras(objetos.iter()
            .filter(|o| matches!(o.tipo, TipoObjeto::Imagem { .. } | TipoObjeto::Grupo))
            .map(|o| o.caixa).chain(barras).collect())
            .into_iter().filter(|c| c[2] - c[0] >= 4.0 && c[3] - c[1] >= 4.0).collect();
        let itens: Vec<Item> = objetos.into_iter().filter_map(|o| match o.tipo {
            // Tamanho que aparece na página: o Firefox, por exemplo, usa `Tf 1` e escala pela matriz.
            TipoObjeto::Texto { texto, fonte, .. } if !texto.trim().is_empty() => Some(Item {
                texto: texto.trim().to_string(), caixa: o.caixa, tamanho: ((o.caixa[3] - o.caixa[1]) * 2.0).round() / 2.0,
                negrito: fonte.to_lowercase().contains("bold") || fonte.to_lowercase().contains("negrito"),
            }),
            _ => None,
        }).collect();
        Ok((itens, figuras, segmentos))
    }
}

/// Doze ou mais traços verticais lado a lado (vão de até 4 pt, alturas que se
/// cobrem) são um código de barras: a caixa dele vira figura e os traços saem
/// da lista de bordas.
fn codigos_de_barras(segmentos: Vec<[f32; 4]>) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
    let mut verticais: Vec<usize> = (0..segmentos.len()).filter(|&i| segmentos[i][2] - segmentos[i][0] < 0.5).collect();
    verticais.sort_by(|&a, &b| segmentos[a][0].total_cmp(&segmentos[b][0]));
    let mut grupos: Vec<Vec<usize>> = Vec::new();
    for i in verticais {
        let s = segmentos[i];
        let junto = grupos.iter_mut().rev().find(|g| {
            let u = segmentos[*g.last().unwrap()];
            s[0] - u[0] <= 4.0 && s[1] < u[3] && u[1] < s[3]
        });
        match junto { Some(g) => g.push(i), None => grupos.push(vec![i]) }
    }
    let mut fora = vec![false; segmentos.len()];
    let mut barras = Vec::new();
    for g in grupos.into_iter().filter(|g| g.len() >= 12) {
        let c = g.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |c, &i| {
            let s = segmentos[i];
            [c[0].min(s[0]), c[1].min(s[1]), c[2].max(s[2]), c[3].max(s[3])]
        });
        barras.push([c[0] - 1.0, c[1], c[2] + 1.0, c[3]]);
        for i in g { fora[i] = true; }
    }
    let resto = segmentos.into_iter().enumerate().filter(|(i, _)| !fora[*i]).map(|(_, s)| s).collect();
    (barras, resto)
}

/// Imagens encostadas viram uma só: o código de barras do comprovante da
/// Receita é uma fileira de imagens de 1 pt (uma por barra).
fn juntar_figuras(mut caixas: Vec<[f32; 4]>) -> Vec<[f32; 4]> {
    let encosta = |a: &[f32; 4], b: &[f32; 4]| a[0] <= b[2] + 1.5 && b[0] <= a[2] + 1.5 && a[1] <= b[3] + 1.5 && b[1] <= a[3] + 1.5;
    let mut i = 0;
    while i < caixas.len() {
        match (i + 1..caixas.len()).find(|&j| encosta(&caixas[i], &caixas[j])) {
            Some(j) => {
                let b = caixas.swap_remove(j);
                let a = &mut caixas[i];
                *a = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
            }
            None => i += 1,
        }
    }
    caixas
}

/// Cortes ordenados com a tolerância de cada um. Uma fresta vazia (`vazia`)
/// mais fina que `FRESTA` vira um corte só, no meio, que aceita as duas bordas.
fn sem_frestas(cortes: Vec<f32>, vazia: impl Fn(f32, f32) -> bool) -> Vec<(f32, f32)> {
    let mut saida: Vec<(f32, f32)> = Vec::new();
    for y in cortes {
        match saida.last_mut() {
            Some((a, tol)) if *tol <= TOLERANCIA_BORDA && y - *a < FRESTA && vazia(*a, y) =>
                { *tol = (y - *a) / 2.0 + TOLERANCIA_BORDA; *a = (*a + y) / 2.0; }
            _ => saida.push((y, TOLERANCIA_BORDA)),
        }
    }
    saida
}

/// Agrupa valores próximos (ordenados) num só, pela média.
fn agrupar(mut v: Vec<f32>, tolerancia: f32) -> Vec<f32> {
    v.sort_by(f32::total_cmp);
    let mut grupos: Vec<(f32, f32, usize)> = Vec::new();
    for x in v {
        match grupos.last_mut() {
            Some((ultimo, soma, n)) if x - *ultimo <= tolerancia => { *ultimo = x; *soma += x; *n += 1; }
            _ => grupos.push((x, x, 1)),
        }
    }
    grupos.into_iter().map(|(_, soma, n)| soma / n as f32).collect()
}

/// Linhas de texto de cima para baixo: o meio do texto cai na faixa de quem abriu a linha.
pub(crate) fn linhas_de_texto(caixas: &[[f32; 4]]) -> Vec<(f32, f32, Vec<usize>)> {
    let mut ordem: Vec<usize> = (0..caixas.len()).collect();
    ordem.sort_by(|&a, &b| caixas[b][3].total_cmp(&caixas[a][3]));
    let mut linhas: Vec<(f32, f32, Vec<usize>)> = Vec::new();
    for k in ordem {
        let [_, y0, _, y1] = caixas[k];
        let meio = (y0 + y1) / 2.0;
        match linhas.last_mut() {
            Some((b0, b1, v)) if meio >= *b0 && meio <= *b1 => { *b0 = b0.min(y0); v.push(k); }
            _ => linhas.push((y0, y1, vec![k])),
        }
    }
    linhas
}

pub(crate) fn grade_da_pagina(itens: &[Item], figuras: &[[f32; 4]], segmentos: &[[f32; 4]]) -> GradePagina {
    let horizontais: Vec<[f32; 4]> = segmentos.iter().copied().filter(|s| s[3] - s[1] < 0.5).collect();
    let verticais: Vec<[f32; 4]> = segmentos.iter().copied().filter(|s| s[2] - s[0] < 0.5).collect();
    if horizontais.is_empty() || verticais.is_empty() { return grade_sem_bordas(itens, figuras); }
    let caixas: Vec<[f32; 4]> = itens.iter().map(|i| i.caixa).collect();
    let tudo = caixas.iter().chain(segmentos).chain(figuras).fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, c|
        [a[0].min(c[0]), a[1].min(c[1]), a[2].max(c[2]), a[3].max(c[3])]);

    // Colunas: as bordas verticais, mais as pontas do que estiver fora da tabela.
    let xs = agrupar(verticais.iter().map(|s| s[0]).chain([tudo[0], tudo[2]]).collect(), TOLERANCIA_BORDA);
    let (xs, tx): (Vec<f32>, Vec<f32>) = sem_frestas(xs, |a, b| !caixas.iter().any(|c| c[0] + TOLERANCIA_BORDA >= a && c[0] + TOLERANCIA_BORDA < b))
        .into_iter().unzip();
    // Linhas: as bordas horizontais, mais um corte entre duas linhas de texto sem borda no meio.
    let mut cortes: Vec<f32> = horizontais.iter().map(|s| s[1]).chain([tudo[1], tudo[3]]).collect();
    let texto = linhas_de_texto(&caixas);
    for par in texto.windows(2) {
        let (baixo_de_cima, topo_de_baixo) = (par[0].0, par[1].1);
        let (lo, hi) = (topo_de_baixo.min(baixo_de_cima), topo_de_baixo.max(baixo_de_cima));
        if !cortes.iter().any(|&y| y >= lo - 0.5 && y <= hi + 0.5) { cortes.push((lo + hi) / 2.0); }
    }
    // Rótulo sozinho numa caixa alta (sem valor embaixo): a linha dele termina
    // logo abaixo do texto, senão o texto cairia no pé da caixa.
    for &(baixo, topo, _) in &texto {
        let abaixo = cortes.iter().copied().filter(|&y| y < baixo - 0.5).fold(f32::MIN, f32::max);
        if abaixo > f32::MIN && baixo - abaixo > (topo - baixo).max(6.0) * 1.2 { cortes.push(baixo - 1.0); }
    }
    let ys = agrupar(cortes, TOLERANCIA_BORDA);
    let (mut ys, mut ty): (Vec<f32>, Vec<f32>) = sem_frestas(ys, |a, b| !texto.iter().any(|l| (l.0 + l.1) / 2.0 > a && (l.0 + l.1) / 2.0 < b))
        .into_iter().unzip();
    ys.reverse();
    ty.reverse();
    let (nc, nl) = (xs.len().saturating_sub(1).max(1), ys.len().saturating_sub(1).max(1));

    // Há borda cobrindo (pelo menos metade de) um lado?
    let cobre = |lista: &[[f32; 4]], (fixo, tol): (f32, f32), horizontal: bool, a: f32, b: f32| {
        let coberto: f32 = lista.iter().filter(|s| {
            let pos = if horizontal { s[1] } else { s[0] };
            (pos - fixo).abs() <= tol
        }).map(|s| {
            let (s0, s1) = if horizontal { (s[0], s[2]) } else { (s[1], s[3]) };
            (s1.min(b) - s0.max(a)).max(0.0)
        }).sum();
        coberto.min(b - a) >= (b - a) * 0.5
    };
    let borda_esquerda = |l: usize, c: usize| c < xs.len() && cobre(&verticais, (xs[c], tx[c]), false, ys[l + 1], ys[l]);
    let mut bordas = Vec::new();
    for l in 0..nl {
        for c in 0..nc {
            let b = [
                cobre(&horizontais, (ys[l], ty[l]), true, xs[c], xs[c + 1]),
                cobre(&horizontais, (ys[l + 1], ty[l + 1]), true, xs[c], xs[c + 1]),
                borda_esquerda(l, c),
                borda_esquerda(l, c + 1),
            ];
            if b.iter().any(|&x| x) { bordas.push((l, c, b)); }
        }
    }

    // Cada texto na célula onde começa; vários na mesma célula se juntam.
    let mut por_celula: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
    for (k, it) in itens.iter().enumerate() {
        let meio = (it.caixa[1] + it.caixa[3]) / 2.0;
        let l = ys.windows(2).position(|w| meio <= w[0] && meio >= w[1]).unwrap_or(0);
        let c = xs.windows(2).position(|w| it.caixa[0] + TOLERANCIA_BORDA >= w[0] && it.caixa[0] + TOLERANCIA_BORDA < w[1]).unwrap_or(nc - 1);
        por_celula.entry((l, c)).or_default().push(k);
    }
    let mut celulas = Vec::new();
    for (&(l, c), ks) in &por_celula {
        let mut ks = ks.clone();
        ks.sort_by(|&a, &b| itens[a].caixa[0].total_cmp(&itens[b].caixa[0]));
        // Mescla para a direita até a próxima borda vertical ou célula com texto.
        let mut ate = c;
        while ate + 1 < nc && !borda_esquerda(l, ate + 1) && !por_celula.contains_key(&(l, ate + 1)) { ate += 1; }
        let maior = ks.iter().map(|&k| itens[k].tamanho).fold(0.0, f32::max);
        celulas.push(CelulaGrade {
            linha: l, coluna: c, ate_coluna: ate,
            texto: ks.iter().map(|&k| itens[k].texto.as_str()).collect::<Vec<_>>().join(" "),
            tamanho: maior, negrito: ks.iter().all(|&k| itens[k].negrito),
        });
    }
    // A imagem fica presa à célula do canto de cima à esquerda dela.
    let imagens = figuras.iter().map(|&caixa| {
        let c = xs.iter().rposition(|&x| x <= caixa[0]).unwrap_or(0).min(nc - 1);
        let l = ys.iter().rposition(|&y| y >= caixa[3]).unwrap_or(0).min(nl - 1);
        ImagemGrade { linha: l, coluna: c, dx: (caixa[0] - xs[c]).max(0.0), dy: (ys[l] - caixa[3]).max(0.0),
            largura: caixa[2] - caixa[0], altura: caixa[3] - caixa[1], caixa }
    }).collect();
    GradePagina {
        origem: (xs[0], ys[0]),
        larguras: xs.windows(2).map(|w| w[1] - w[0]).collect(),
        alturas: ys.windows(2).map(|w| w[0] - w[1]).collect(),
        celulas, bordas, imagens, ..Default::default()
    }
}

/// Página sem linhas de tabela: colunas pelos inícios de texto alinhados.
/// shortcut: número alinhado à direita sem nada alinhado à esquerda na mesma
/// coluna abre colunas a mais.
fn grade_sem_bordas(itens: &[Item], figuras: &[[f32; 4]]) -> GradePagina {
    let caixas: Vec<[f32; 4]> = itens.iter().map(|i| i.caixa).collect();
    let colunas = agrupar(caixas.iter().map(|c| c[0]).collect(), TOLERANCIA_COLUNA);
    let coluna = |x: f32| colunas.iter().rposition(|&c| c <= x + TOLERANCIA_COLUNA).unwrap_or(0);
    let mut celulas: Vec<CelulaGrade> = Vec::new();
    let linhas = linhas_de_texto(&caixas);
    // Sem medidas, a imagem vai para a coluna A, na linha do texto logo abaixo do topo dela.
    let imagens = figuras.iter().map(|&caixa| ImagemGrade {
        linha: linhas.iter().filter(|l| l.1 > caixa[3]).count(), coluna: 0, dx: 0.0, dy: 0.0,
        largura: caixa[2] - caixa[0], altura: caixa[3] - caixa[1], caixa,
    }).collect();
    for (l, (_, _, mut v)) in linhas.into_iter().enumerate() {
        v.sort_by(|&a, &b| caixas[a][0].total_cmp(&caixas[b][0]));
        for k in v {
            let c = coluna(caixas[k][0]);
            match celulas.iter_mut().find(|x| x.linha == l && x.coluna == c) {
                Some(x) => { x.texto.push(' '); x.texto.push_str(&itens[k].texto); }
                None => celulas.push(CelulaGrade { linha: l, coluna: c, ate_coluna: c, texto: itens[k].texto.clone(),
                    tamanho: itens[k].tamanho, negrito: itens[k].negrito }),
            }
        }
    }
    GradePagina { celulas, imagens, ..Default::default() }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(texto: &str, x: f32, y: f32, tamanho: f32) -> Item {
        Item { texto: texto.into(), caixa: [x, y, x + 4.0 * texto.len() as f32, y + tamanho], tamanho, negrito: false }
    }

    #[test]
    fn caixas_com_borda_viram_celulas_mescladas_com_borda() {
        // Duas caixas em cima (40–300 e 300–560), uma larga embaixo (40–560),
        // cada uma com rótulo e valor, como no comprovante de CNPJ.
        let segs = vec![
            [40.0, 720.0, 560.0, 720.0], [40.0, 680.0, 560.0, 680.0], [40.0, 640.0, 560.0, 640.0],
            [40.0, 640.0, 40.0, 720.0], [300.0, 680.0, 300.0, 720.0], [560.0, 640.0, 560.0, 720.0],
        ];
        let itens = vec![
            t("NÚMERO DE INSCRIÇÃO", 43.0, 710.0, 6.0), t("DATA DE ABERTURA", 303.0, 710.0, 6.0),
            t("01.234.567/0001-89", 43.0, 690.0, 9.0), t("15/03/2010", 303.0, 690.0, 9.0),
            t("NOME EMPRESARIAL", 43.0, 670.0, 6.0), t("EMPRESA EXEMPLO LTDA", 43.0, 650.0, 9.0),
        ];
        let g = grade_da_pagina(&itens, &[[45.0, 642.0, 85.0, 662.0]], &segs);
        assert_eq!(g.larguras, [260.0, 260.0]);
        assert_eq!(g.alturas.len(), 4, "rótulo e valor de cada caixa em linhas próprias: {:?}", g.alturas);
        let cel = |texto: &str| g.celulas.iter().find(|c| c.texto == texto).unwrap();
        assert_eq!((cel("01.234.567/0001-89").coluna, cel("01.234.567/0001-89").ate_coluna), (0, 0));
        assert_eq!((cel("15/03/2010").linha, cel("15/03/2010").coluna), (1, 1));
        // A caixa de baixo não tem borda no meio: o texto mescla as duas colunas.
        assert_eq!((cel("EMPRESA EXEMPLO LTDA").linha, cel("EMPRESA EXEMPLO LTDA").ate_coluna), (3, 1));
        let borda = |l: usize, c: usize| g.bordas.iter().find(|b| b.0 == l && b.1 == c).map(|b| b.2);
        // Rótulo: borda em cima, nada embaixo (o valor está na mesma caixa); meio dividido.
        assert_eq!(borda(0, 0), Some([true, false, true, true]));
        assert_eq!(borda(3, 1), Some([false, true, false, true]));
        // Imagem no pé da caixa larga: presa à linha do valor, 5 pt para dentro.
        let im = &g.imagens[0];
        assert_eq!((im.linha, im.coluna, im.dx, im.largura), (3, 0, 5.0, 40.0));
    }

    #[test]
    fn fresta_entre_caixas_some_e_rotulo_sozinho_fica_em_cima() {
        // Duas caixas empilhadas com 4 pt de fresta; a de baixo só tem o rótulo, no topo.
        let caixa = |y0: f32, y1: f32| vec![[40.0, y1, 560.0, y1], [40.0, y0, 560.0, y0], [40.0, y0, 40.0, y1], [560.0, y0, 560.0, y1]];
        let segs: Vec<[f32; 4]> = caixa(690.0, 720.0).into_iter().chain(caixa(646.0, 686.0)).collect();
        let itens = vec![
            t("SITUAÇÃO CADASTRAL", 43.0, 711.0, 6.0), t("ATIVA", 43.0, 693.0, 9.0),
            t("MOTIVO DE SITUAÇÃO CADASTRAL", 43.0, 677.0, 6.0),
        ];
        let g = grade_da_pagina(&itens, &[], &segs);
        // Linhas: rótulo, valor, rótulo do motivo, o vazio da caixa — sem a fresta de 4 pt.
        assert_eq!(g.alturas.len(), 4, "{:?}", g.alturas);
        assert!(g.alturas.iter().all(|&a| a > 5.0), "{:?}", g.alturas);
        let motivo = g.celulas.iter().find(|c| c.texto.starts_with("MOTIVO")).unwrap();
        assert!(g.alturas[motivo.linha] < 15.0, "o rótulo fica numa linha baixa, no topo da caixa: {:?}", g.alturas);
        // A borda entre as caixas continua lá (as duas viram uma).
        assert!(g.bordas.iter().any(|b| b.0 == motivo.linha && b.2[0]));
        // Código de barras feito de traços: vira figura e não vira borda.
        let tracos: Vec<[f32; 4]> = (0..20).map(|i| { let x = 300.0 + i as f32 * 2.5; [x, 50.0, x, 80.0] })
            .chain([[40.0, 40.0, 560.0, 40.0]]).collect();
        let (barras, resto) = codigos_de_barras(tracos);
        assert_eq!(barras, vec![[299.0, 50.0, 348.5, 80.0]]);
        assert_eq!(resto, vec![[40.0, 40.0, 560.0, 40.0]]);
        // Barras de 1 pt lado a lado viram uma imagem só.
        let barras: Vec<[f32; 4]> = (0..30).map(|i| { let x = 100.0 + i as f32 * 1.2; [x, 100.0, x + 1.0, 130.0] }).collect();
        assert_eq!(juntar_figuras(barras), vec![[100.0, 100.0, 135.8, 130.0]]);
    }

    #[test]
    fn sem_bordas_colunas_pelos_inicios() {
        let itens = vec![t("Nome", 40.0, 700.0, 10.0), t("Valor", 300.0, 700.0, 10.0), t("Ana", 40.0, 686.0, 10.0), t("10", 301.0, 686.0, 10.0)];
        let g = grade_da_pagina(&itens, &[], &[]);
        assert!(g.celulas.contains(&CelulaGrade { linha: 1, coluna: 1, ate_coluna: 1, texto: "10".into(), tamanho: 10.0, negrito: false }));
        assert!(g.bordas.is_empty());
    }
}
