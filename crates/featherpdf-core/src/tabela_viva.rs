//! Tabelas de verdade no PDF: acrescentar uma linha (no meio ou no fim) ou uma coluna (no fim)
//! numa tabela que já está na página, e o resto se ajusta: o que está embaixo desce (ou à
//! direita anda), as bordas verticais e horizontais que atravessam o corte ganham o pedaço que
//! falta, e o texto novo entra alinhado com as colunas.
//! shortcut: tabela desenhada como um caminho só (a grade inteira num objeto) é recusada; esticar exigiria reescrever o caminho.

use feather_draw::{Caminho, Comando, Ponto};

use crate::{DesenhoNovo, DocumentoPdf, ObjetoPdf, TipoObjeto};

/// Linhas (topo do texto, de cima para baixo) e colunas (x do começo) da tabela, com o
/// tamanho da letra e a altura de uma linha.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeTabela { pub linhas: Vec<f32>, pub colunas: Vec<f32>, pub tamanho: f32, pub altura: f32, pub largura: f32 }

/// Fina o bastante para ser borda (pt).
const FINA: f32 = 2.5;

fn texto(o: &ObjetoPdf) -> Option<f32> { match &o.tipo { TipoObjeto::Texto { tamanho, .. } => Some(*tamanho), _ => None } }
fn dentro(c: [f32; 4], t: [f32; 4]) -> bool { c[0] >= t[0] - 3.0 && c[2] <= t[2] + 3.0 && c[1] >= t[1] - 3.0 && c[3] <= t[3] + 3.0 }
fn mediana(mut v: Vec<f32>) -> Option<f32> { v.sort_by(f32::total_cmp); v.get(v.len() / 2).copied() }

impl DocumentoPdf {
    pub fn grade_da_tabela(&self, pagina: u32, caixa: [f32; 4]) -> Result<GradeTabela, String> {
        let objs = self.objetos(pagina)?;
        let textos: Vec<&ObjetoPdf> = objs.iter().filter(|o| texto(o).is_some() && dentro(o.caixa, caixa)).collect();
        let mut linhas: Vec<(f32, Vec<f32>)> = Vec::new();
        for o in &textos {
            match linhas.iter_mut().find(|(y, _)| (y - o.caixa[3]).abs() < 2.5) {
                Some((_, xs)) => xs.push(o.caixa[0]),
                None => linhas.push((o.caixa[3], vec![o.caixa[0]])),
            }
        }
        if linhas.len() < 2 { return Err("A tabela precisa de pelo menos duas linhas de texto.".into()); }
        linhas.sort_by(|a, b| b.0.total_cmp(&a.0));
        let altura = mediana(linhas.windows(2).map(|w| w[0].0 - w[1].0).collect()).unwrap_or(14.0);
        // Colunas: os começos da linha mais cheia.
        let mut colunas = linhas.iter().max_by_key(|l| l.1.len()).map(|l| l.1.clone()).unwrap_or_default();
        colunas.sort_by(f32::total_cmp);
        let largura = mediana(colunas.windows(2).map(|w| w[1] - w[0]).collect()).unwrap_or(80.0);
        let tamanho = mediana(textos.iter().filter_map(|o| texto(o)).collect()).unwrap_or(10.0);
        Ok(GradeTabela { linhas: linhas.into_iter().map(|l| l.0).collect(), colunas, tamanho, altura, largura })
    }

    /// Abre espaço `d` no corte `em`: `vertical` = linha nova (o de baixo desce), senão coluna
    /// (o da direita anda). Só mexe no que está na faixa da tabela.
    fn abrir_espaco(&mut self, pagina: u32, caixa: [f32; 4], vertical: bool, em: f32, d: f32) -> Result<(), String> {
        let objs = self.objetos(pagina)?;
        let (geo, _) = self.geometria(pagina)?;
        let (mut mover, mut emendas) = (Vec::new(), Vec::new());
        for (k, o) in objs.iter().enumerate() {
            let c = o.caixa;
            let na_faixa = if vertical { c[2] > caixa[0] - 3.0 && c[0] < caixa[2] + 3.0 } else { c[3] > caixa[1] - 3.0 && c[1] < caixa[3] + 3.0 };
            if !na_faixa { continue; }
            let (a, b) = if vertical { (c[1], c[3]) } else { (c[0], c[2]) };
            // Do lado que anda: embaixo do corte (linha) ou à direita (coluna).
            let anda = if vertical { b <= em + 0.5 } else { a >= em - 0.5 };
            let atravessa = a < em - 0.5 && b > em + 0.5;
            let espessura = if vertical { c[2] - c[0] } else { c[3] - c[1] };
            if anda {
                mover.push(k);
                // A borda que estava no corte desce junto; outra igual fica no lugar dela.
                if texto(o).is_none() && (if vertical { c[3] - c[1] } else { c[2] - c[0] }) < FINA && ((a + b) / 2.0 - em).abs() < 1.5 {
                    emendas.push(if vertical { [c[0], em - 0.25, c[2], em + 0.25] } else { [em - 0.25, c[1], em + 0.25, c[3]] });
                }
            } else if atravessa {
                if texto(o).is_some() || espessura >= FINA { return Err("A tabela é desenhada como uma peça só; não dá para abrir espaço nela.".into()); }
                // Borda que cruza o corte: ganha o pedaço do espaço aberto.
                emendas.push(if vertical { [c[0], em - d, c[2], em] } else { [em, c[1], em + d, c[3]] });
            } else if !vertical && texto(o).is_none() && espessura < FINA && (b - em).abs() < 1.5 {
                // Borda horizontal que termina na borda direita: estica até a coluna nova.
                emendas.push([em, c[1], em + d, c[3]]);
            }
        }
        if vertical {
            let fundo = mover.iter().map(|&k| objs[k].caixa[1]).fold(f32::MAX, f32::min);
            if fundo - d < geo[1] + 5.0 { return Err("A linha nova empurraria o texto para fora da página.".into()); }
            if !mover.is_empty() { self.mover_objetos(pagina, &mover, 0.0, -d)?; }
        } else {
            let fim = mover.iter().map(|&k| objs[k].caixa[2]).fold(em, f32::max);
            if fim + d > geo[2] - 5.0 { return Err("A coluna nova passaria da borda da página.".into()); }
            if !mover.is_empty() { self.mover_objetos(pagina, &mover, d, 0.0)?; }
        }
        for e in emendas {
            // Retângulo fino preenchido: a mesma cara da borda de uma linha só.
            let (l, a) = ((e[2] - e[0]).max(0.3), (e[3] - e[1]).max(0.3));
            let mut caminho = Caminho::novo();
            for (i, (x, y)) in [(0.0, 0.0), (l, 0.0), (l, a), (0.0, a)].into_iter().enumerate() {
                let p = Ponto { x, y };
                caminho.adicionar(if i == 0 { Comando::Mover(p) } else { Comando::Linha(p) });
            }
            caminho.adicionar(Comando::Fechar);
            self.inserir_desenho(pagina, [e[0], e[1], e[0] + l, e[1] + a], &DesenhoNovo {
                preenchida: caminho, tracos: Caminho::novo(), preenchimento: Some([0.0; 3]), contorno: None, espessura: 0.0,
            })?;
        }
        Ok(())
    }

    /// Linha nova depois da linha `depois` (0 = cabeçalho), com um valor por coluna.
    pub fn acrescentar_linha(&mut self, pagina: u32, caixa: [f32; 4], depois: usize, valores: &[String]) -> Result<(), String> {
        let g = self.grade_da_tabela(pagina, caixa)?;
        let i = depois.min(g.linhas.len() - 1);
        let topo = g.linhas[i];
        // O corte fica no meio do vão entre a linha e a de baixo (ou logo abaixo da última).
        let em = topo - g.tamanho - (g.altura - g.tamanho) / 2.0;
        let caixa_cheia = [caixa[0], caixa[1].min(em - 1.0), caixa[2], caixa[3]];
        self.abrir_espaco(pagina, caixa_cheia, true, em, g.altura)?;
        for (x, v) in g.colunas.iter().zip(valores) {
            if v.trim().is_empty() { continue; }
            self.inserir_texto(pagina, *x, topo - g.altura, v.trim(), g.tamanho, false, [0.0; 3])?;
        }
        Ok(())
    }

    /// Coluna nova no fim, com um valor por linha (de cima para baixo; o primeiro é o cabeçalho).
    pub fn acrescentar_coluna(&mut self, pagina: u32, caixa: [f32; 4], valores: &[String]) -> Result<(), String> {
        let g = self.grade_da_tabela(pagina, caixa)?;
        // A borda direita: a vertical mais à direita da tabela, ou o fim do texto.
        let segs = self.segmentos(pagina)?;
        let borda = segs.iter().filter(|s| (s[2] - s[0]).abs() < FINA && s[3] > caixa[1] && s[1] < caixa[3] && s[0] >= caixa[2] - 3.0 && s[0] <= caixa[2] + g.largura)
            .map(|s| s[0]).fold(caixa[2], f32::max);
        let recuo = g.colunas.first().map_or(4.0, |c| (c - caixa[0]).clamp(2.0, 12.0));
        self.abrir_espaco(pagina, [caixa[0], caixa[1], borda, caixa[3]], false, borda, g.largura)?;
        for (y, v) in g.linhas.iter().zip(valores) {
            if v.trim().is_empty() { continue; }
            self.inserir_texto(pagina, borda + recuo.max(4.0), *y, v.trim(), g.tamanho, false, [0.0; 3])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;
    use crate::TipoObjeto;

    fn textos(pdf: &crate::DocumentoPdf) -> Vec<(String, [f32; 4])> {
        pdf.objetos(1).unwrap().into_iter().filter_map(|o| match o.tipo { TipoObjeto::Texto { texto, .. } => Some((texto, o.caixa)), _ => None }).collect()
    }

    #[test]
    fn linha_no_meio_empurra_o_resto_e_estica_as_bordas() {
        // Tabela 2 colunas × 3 linhas, borda vertical no meio (x = 190) e texto embaixo.
        let mut pdf = pdf_com("BT /F1 10 Tf 100 700 Td (Item) Tj 100 0 Td (Valor) Tj ET\n\
            BT /F1 10 Tf 100 685 Td (Lapis) Tj 100 0 Td (2,00) Tj ET\n\
            BT /F1 10 Tf 100 670 Td (Caneta) Tj 100 0 Td (5,00) Tj ET\n\
            190 664 0.5 50 re f\n\
            BT /F1 10 Tf 100 600 Td (Rodape da pagina) Tj ET");
        let tabela = [98.0, 668.0, 240.0, 712.0];
        let antes = textos(&pdf);
        let y = |v: &[(String, [f32; 4])], t: &str| v.iter().find(|x| x.0 == t).map(|x| x.1[1]).unwrap();
        pdf.acrescentar_linha(1, tabela, 1, &["Borracha".into(), "1,50".into()]).unwrap();
        let depois = textos(&pdf);
        assert!((y(&depois, "Caneta") - (y(&antes, "Caneta") - 15.0)).abs() < 0.6, "a linha de baixo desce uma linha");
        assert!((y(&depois, "Rodape da pagina") - (y(&antes, "Rodape da pagina") - 15.0)).abs() < 0.6, "o texto embaixo da tabela também");
        assert!((y(&depois, "Lapis") - y(&antes, "Lapis")).abs() < 0.01, "a de cima fica");
        let borracha = depois.iter().find(|x| x.0 == "Borracha").expect("linha nova");
        assert!((borracha.1[0] - 100.0).abs() < 1.0 && borracha.1[1] < y(&antes, "Lapis") && borracha.1[1] > y(&depois, "Caneta"));
        assert!(depois.iter().any(|x| x.0 == "1,50" && (x.1[0] - 200.0).abs() < 1.0), "valor na segunda coluna");
        // A borda do meio ganhou o pedaço do espaço aberto.
        let emenda = pdf.objetos(1).unwrap().into_iter().any(|o| !matches!(o.tipo, TipoObjeto::Texto { .. })
            && (o.caixa[0] - 189.5).abs() < 1.0 && ((o.caixa[3] - o.caixa[1]) - 15.0).abs() < 1.0);
        assert!(emenda, "{:?}", pdf.objetos(1).unwrap());

        // Coluna no fim: cabeçalho e valores à direita da última coluna.
        pdf.acrescentar_coluna(1, [98.0, 650.0, 240.0, 712.0], &["Qtd".into(), "3".into()]).unwrap();
        let qtd = textos(&pdf).into_iter().find(|x| x.0 == "Qtd").expect("cabeçalho novo");
        assert!(qtd.1[0] > 225.0 && (qtd.1[3] - 710.0).abs() < 4.0, "{qtd:?}");
    }
}
