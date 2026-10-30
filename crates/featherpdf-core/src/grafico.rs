//! Gráfico para dados: dentro da área escolhida, as linhas (caminhos riscados com 3 ou mais
//! pontos) e as barras (retângulos preenchidos) viram séries, uma por cor. Os números escritos
//! nos eixos calibram a conversão de pontos da página para valores; sem eles, os valores saem
//! na escala da página. Rótulos de texto embaixo viram as categorias das barras.

use std::collections::BTreeMap;

use lopdf::content::Content;
use lopdf::{Dictionary, Object, ObjectId};

use crate::objetos::{aplicar, mult, Matriz};
use crate::regras::{escrever_numero, ler_numero};
use crate::{DocumentoPdf, TabelaPdf, TipoColuna, TipoObjeto};

const IDENTIDADE: Matriz = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Debug, Clone, PartialEq)]
/// `Vertices`: os pontos de qualquer caminho pintado (riscado ou cheio), para o imã da régua.
enum Forma { Linha(Vec<(f32, f32)>), Barra([f32; 4]), Vertices(Vec<(f32, f32)>) }

#[derive(Clone)]
struct Tinta { ctm: Matriz, contorno: [f32; 3], preenchimento: [f32; 3] }

fn cor(ops: &[Object]) -> Option<[f32; 3]> {
    let n: Vec<f32> = ops.iter().filter_map(|o| o.as_float().ok()).collect();
    match n.len() {
        1 => Some([n[0]; 3]),
        3 => Some([n[0], n[1], n[2]]),
        4 => Some([(1.0 - n[0]) * (1.0 - n[3]), (1.0 - n[1]) * (1.0 - n[3]), (1.0 - n[2]) * (1.0 - n[3])]),
        _ => None,
    }
}

impl DocumentoPdf {
    /// Caminhos pintados num conteúdo (e nos Form XObjects dele), com a cor de cada um.
    fn formas(&self, bytes: &[u8], recursos: Option<&Dictionary>, base: Matriz, saida: &mut Vec<(Forma, [f32; 3])>, nivel: u8, orcamento: &mut u32) {
        let Ok(c) = Content::decode(bytes) else { return };
        let mut t = Tinta { ctm: base, contorno: [0.0; 3], preenchimento: [0.0; 3] };
        let mut pilha: Vec<Tinta> = vec![];
        let mut caminho: Vec<Vec<(f32, f32)>> = vec![];
        let mut retangulos: Vec<[f32; 4]> = vec![];
        let mut cantos: Vec<Vec<(f32, f32)>> = vec![];
        let num = |o: &Object| o.as_float().unwrap_or(0.0);
        for op in c.operations {
            let a = &op.operands;
            match op.operator.as_str() {
                "q" => pilha.push(t.clone()),
                "Q" => if let Some(x) = pilha.pop() { t = x },
                "cm" if a.len() == 6 => t.ctm = mult([num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3]), num(&a[4]), num(&a[5])], t.ctm),
                "RG" | "G" | "K" | "SC" | "SCN" => if let Some(c) = cor(a) { t.contorno = c },
                "rg" | "g" | "k" | "sc" | "scn" => if let Some(c) = cor(a) { t.preenchimento = c },
                "m" if a.len() == 2 => caminho.push(vec![aplicar(t.ctm, num(&a[0]), num(&a[1]))]),
                "l" if a.len() == 2 => if let Some(s) = caminho.last_mut() { s.push(aplicar(t.ctm, num(&a[0]), num(&a[1]))) },
                // Curva: fica o ponto final (o gráfico de linha suavizado ainda passa pelos pontos).
                "c" | "v" | "y" if a.len() >= 2 => if let Some(s) = caminho.last_mut() { s.push(aplicar(t.ctm, num(&a[a.len() - 2]), num(&a[a.len() - 1]))) },
                "re" if a.len() == 4 => {
                    let (x, y, w, h) = (num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3]));
                    let (p, q) = (aplicar(t.ctm, x, y), aplicar(t.ctm, x + w, y + h));
                    retangulos.push([p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1)]);
                    // Os 4 cantos de verdade (com o CTM girado, o retângulo não fica alinhado).
                    cantos.push(vec![p, aplicar(t.ctm, x + w, y), q, aplicar(t.ctm, x, y + h)]);
                }
                o @ ("S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n") => {
                    let riscado = matches!(o, "S" | "s" | "B" | "B*" | "b" | "b*");
                    let preenchido = matches!(o, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
                    if riscado || preenchido { saida.extend(caminho.iter().chain(&cantos).map(|s| (Forma::Vertices(s.clone()), t.contorno))); }
                    if riscado { for s in caminho.iter().filter(|s| s.len() >= 3) { saida.push((Forma::Linha(s.clone()), t.contorno)); } }
                    if preenchido { for r in &retangulos { saida.push((Forma::Barra(*r), t.preenchimento)); } }
                    caminho.clear();
                    retangulos.clear();
                    cantos.clear();
                }
                // O orçamento corta o formulário que se desenha várias vezes a si mesmo.
                "Do" if nivel < 4 && *orcamento > 0 => {
                    *orcamento -= 1;
                    let Some(nome) = a.first().and_then(|o| o.as_name().ok()) else { continue };
                    let Some(xo) = recursos.and_then(|r| r.get(b"XObject").ok()).and_then(|o| self.dict(o)) else { continue };
                    let Some(id) = xo.get(nome).ok().and_then(|o| o.as_reference().ok()) else { continue };
                    let Ok(s) = self.documento.get_object(id).and_then(Object::as_stream) else { continue };
                    if s.dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Form") { continue; }
                    let m = s.dict.get(b"Matrix").and_then(Object::as_array).ok().map(|v| v.iter().map(num).collect::<Vec<_>>())
                        .filter(|v| v.len() == 6).map_or(IDENTIDADE, |v| [v[0], v[1], v[2], v[3], v[4], v[5]]);
                    let dentro = s.dict.get(b"Resources").ok().and_then(|o| self.dict(o));
                    let bytes = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
                    self.formas(&bytes, dentro.as_ref().or(recursos), mult(m, t.ctm), saida, nivel + 1, orcamento);
                }
                _ => {}
            }
        }
    }

    /// Os vértices de tudo o que a página desenha (pontas de linha, cantos de parede e de
    /// retângulo, também dentro de blocos), em pontos da página, para o imã da régua.
    pub fn vertices(&self, pagina: u32) -> Result<Vec<(f32, f32)>, String> {
        let id: ObjectId = self.id_pagina(pagina)?;
        let recursos = self.documento.get_dictionary(id).ok().and_then(|d| d.get(b"Resources").ok()).and_then(|o| self.dict(o));
        let mut formas = vec![];
        self.formas(&self.documento.get_page_content(id), recursos.as_ref(), IDENTIDADE, &mut formas, 0, &mut 10_000);
        let mut v: Vec<(f32, f32)> = formas.into_iter().flat_map(|(f, _)| match f { Forma::Vertices(p) => p, _ => vec![] }).collect();
        // Pontas repetidas (o fim de uma linha é o começo da outra) contam uma vez.
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        v.dedup_by(|a, b| (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05);
        Ok(v)
    }

    /// As séries do gráfico desenhado em `caixa` (pontos da página), uma tabela por série.
    pub fn dados_do_grafico(&self, pagina: u32, caixa: [f32; 4]) -> Result<Vec<TabelaPdf>, String> {
        let id: ObjectId = self.id_pagina(pagina)?;
        let recursos = self.documento.get_dictionary(id).ok().and_then(|d| d.get(b"Resources").ok()).and_then(|o| self.dict(o));
        let mut formas = vec![];
        self.formas(&self.documento.get_page_content(id), recursos.as_ref(), IDENTIDADE, &mut formas, 0, &mut 10_000);
        let dentro = |(x, y): (f32, f32)| x >= caixa[0] - 1.0 && x <= caixa[2] + 1.0 && y >= caixa[1] - 1.0 && y <= caixa[3] + 1.0;
        let area = (caixa[2] - caixa[0]) * (caixa[3] - caixa[1]);
        // Uma série por cor (linhas e barras separadas).
        let mut series: BTreeMap<(bool, [u8; 3]), Vec<Forma>> = BTreeMap::new();
        for (f, c) in formas {
            let ok = match &f {
                // Linha reta só horizontal ou vertical é eixo ou grade.
                Forma::Linha(p) => p.iter().all(|&q| dentro(q)) && {
                    let (xs, ys): (Vec<f32>, Vec<f32>) = p.iter().copied().unzip();
                    let largura = |v: &[f32]| v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min);
                    largura(&xs) > 2.0 && largura(&ys) > 2.0
                },
                // Fundo do gráfico (grande) e traço fino de grade não são barra.
                Forma::Vertices(_) => false,
                Forma::Barra(r) => dentro((r[0], r[1])) && dentro((r[2], r[3])) && (r[2] - r[0]) * (r[3] - r[1]) < area * 0.4 && (r[2] - r[0]) > 1.5 && (r[3] - r[1]) > 0.5
                    && c.iter().any(|&v| v < 0.97),
            };
            if ok { series.entry((matches!(f, Forma::Barra(_)), c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8))).or_default().push(f); }
        }
        if series.is_empty() { return Err("Nenhuma linha ou barra de gráfico vetorial nesta área (gráfico em imagem não tem os pontos).".into()); }

        // Números e rótulos perto da área: à esquerda calibram o y, embaixo calibram o x.
        let textos: Vec<(String, [f32; 4])> = self.objetos(pagina)?.into_iter().filter_map(|o| match o.tipo {
            TipoObjeto::Texto { texto, .. } => Some((texto.trim().to_string(), o.caixa)), _ => None,
        }).filter(|(_, c)| c[2] > caixa[0] - 60.0 && c[0] < caixa[2] + 20.0 && c[3] > caixa[1] - 40.0 && c[1] < caixa[3] + 10.0).collect();
        let ajuste = |pares: &[(f32, f64)]| -> Option<(f64, f64)> {
            if pares.len() < 2 { return None; }
            let n = pares.len() as f64;
            let (sx, sy) = pares.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 as f64, a.1 + p.1));
            let (mx, my) = (sx / n, sy / n);
            let var: f64 = pares.iter().map(|p| (p.0 as f64 - mx).powi(2)).sum();
            if var < 1e-6 { return None; }
            let a = pares.iter().map(|p| (p.0 as f64 - mx) * (p.1 - my)).sum::<f64>() / var;
            Some((a, my - a * mx))
        };
        let eixo_y: Vec<(f32, f64)> = textos.iter().filter(|(_, c)| c[2] <= caixa[0] + 30.0).filter_map(|(t, c)| ler_numero(t).map(|v| ((c[1] + c[3]) / 2.0, v))).collect();
        let base_x: Vec<&(String, [f32; 4])> = textos.iter().filter(|(_, c)| c[3] <= caixa[1] + 25.0).collect();
        let eixo_x: Vec<(f32, f64)> = base_x.iter().filter_map(|(t, c)| ler_numero(t).map(|v| ((c[0] + c[2]) / 2.0, v))).collect();
        let fy = ajuste(&eixo_y);
        let fx = ajuste(&eixo_x);
        let categorias: Vec<(f32, String)> = if fx.is_none() { base_x.iter().filter(|(t, _)| ler_numero(t).is_none()).map(|(t, c)| ((c[0] + c[2]) / 2.0, t.clone())).collect() } else { vec![] };
        let valor_y = |y: f32| fy.map_or(y as f64, |(a, b)| a * y as f64 + b);
        let valor_x = |x: f32| -> String {
            if let Some((a, b)) = fx { return escrever_numero(a * x as f64 + b, 2); }
            if let Some((_, t)) = categorias.iter().min_by(|p, q| (p.0 - x).abs().total_cmp(&(q.0 - x).abs())).filter(|c| (c.0 - x).abs() < 40.0) { return t.clone(); }
            escrever_numero(x as f64, 1)
        };

        let mut tabelas = vec![];
        for (i, ((barra, c), f)) in series.into_iter().enumerate() {
            let mut pontos: Vec<(f32, f32)> = f.iter().flat_map(|f| match f {
                Forma::Linha(p) => p.clone(),
                // Barra de pé: o topo; barra deitada (mais larga que alta): a ponta direita.
                Forma::Barra(r) if (r[2] - r[0]) > (r[3] - r[1]) * 2.0 && fy.is_none() => vec![(r[2], (r[1] + r[3]) / 2.0)],
                Forma::Barra(r) => vec![((r[0] + r[2]) / 2.0, r[3])],
                Forma::Vertices(_) => vec![],
            }).collect();
            pontos.sort_by(|a, b| a.0.total_cmp(&b.0));
            pontos.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01);
            let nome = format!("{} {} (#{:02x}{:02x}{:02x})", if barra { "Barras" } else { "Linha" }, i + 1, c[0], c[1], c[2]);
            tabelas.push(TabelaPdf {
                pagina, ate_pagina: pagina, titulo: nome.clone(), caixa,
                cabecalho: vec![if categorias.is_empty() { "X".into() } else { "Categoria".into() }, nome],
                linhas: pontos.iter().map(|&(x, y)| vec![valor_x(x), escrever_numero(valor_y(y), 2)]).collect(),
                tipos: vec![if categorias.is_empty() { TipoColuna::Numero } else { TipoColuna::Texto }, TipoColuna::Numero],
            });
        }
        Ok(tabelas)
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;

    #[test]
    fn vertices_de_linhas_retangulos_e_paredes() {
        // Cota (linha de 2 pontos), parede cheia e um retângulo deslocado por cm.
        let pdf = pdf_com("100 500 m 246 500 l S 50 50 m 90 50 l 90 80 l f q 1 0 0 1 300 300 cm 0 0 20 10 re S Q");
        let v = pdf.vertices(1).unwrap();
        for p in [(100.0, 500.0), (246.0, 500.0), (90.0, 80.0), (300.0, 300.0), (320.0, 310.0)] {
            assert!(v.iter().any(|q| (q.0 - p.0).abs() < 0.01 && (q.1 - p.1).abs() < 0.01), "{p:?} não está em {v:?}");
        }
    }

    #[test]
    fn linha_vermelha_com_eixo_calibrado() {
        // Eixo y: "0" em y=100 e "100" em y=300; linha vermelha passando por y=100, 200 e 300.
        let pdf = pdf_com("BT /F1 8 Tf 60 97 Td (0) Tj ET BT /F1 8 Tf 52 297 Td (100) Tj ET \
            0 0 0 RG 80 100 m 80 300 l S \
            1 0 0 RG 100 100 m 200 200 l 300 300 l S");
        let t = pdf.dados_do_grafico(1, [70.0, 90.0, 320.0, 320.0]).unwrap();
        assert_eq!(t.len(), 1, "o eixo preto vertical não é série: {t:?}");
        let ys: Vec<f64> = t[0].linhas.iter().map(|l| crate::ler_numero(&l[1]).unwrap()).collect();
        for (y, esperado) in ys.iter().zip([0.0, 50.0, 100.0]) { assert!((y - esperado).abs() < 1.0, "{ys:?}"); }
    }

    #[test]
    fn barras_com_categorias() {
        let pdf = pdf_com("BT /F1 8 Tf 105 85 Td (Jan) Tj ET BT /F1 8 Tf 155 85 Td (Fev) Tj ET \
            0 0 1 rg 100 100 20 50 re f 150 100 20 80 re f");
        let t = pdf.dados_do_grafico(1, [90.0, 95.0, 200.0, 200.0]).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].linhas.iter().map(|l| l[0].as_str()).collect::<Vec<_>>(), ["Jan", "Fev"]);
        assert_eq!(t[0].linhas[1][1], "180,00");
    }
}
