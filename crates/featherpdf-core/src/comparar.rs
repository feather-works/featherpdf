//! Comparar duas versões. Texto: as palavras do documento inteiro numa fila só (por isso uma
//! linha que pula de página não vira "saiu e entrou"), com o diff de Myers. Desenho: as duas
//! páginas sobrepostas — só na antiga em vermelho, só na nova em azul, igual em cinza —, com a
//! nova deslocada para a tinta das duas começar no mesmo ponto.

use super::DocumentoPdf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mudanca { Igual, Saiu, Entrou }

#[derive(Debug, Clone, PartialEq)]
pub struct Trecho {
    pub mudanca: Mudanca,
    pub texto: String,
    /// Página na versão antiga (o que saiu ou ficou) e na nova (o que entrou ou ficou).
    pub pagina_antiga: Option<u32>,
    pub pagina_nova: Option<u32>,
}

/// Diff de Myers (O((n+m)·D)): a sequência de operações que leva `a` em `b`.
fn myers<T: PartialEq>(a: &[T], b: &[T]) -> Vec<Mudanca> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let off = n + m + 1;
    let mut v = vec![0isize; 2 * off as usize + 1];
    // Uma fotografia de `v` no começo de cada passo d, para voltar depois.
    let mut fotos: Vec<Vec<isize>> = Vec::new();
    'fora: for d in 0..=n + m {
        fotos.push(v.clone());
        for k in (-d..=d).step_by(2) {
            let i = (k + off) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) { v[i + 1] } else { v[i - 1] + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] { x += 1; y += 1; }
            v[i] = x;
            if x >= n && y >= m { break 'fora; }
        }
    }
    let mut ops = Vec::new();
    let (mut x, mut y) = (n, m);
    for (d, v) in fotos.iter().enumerate().rev() {
        let (d, k) = (d as isize, x - y);
        let at = |k: isize| v[(k + off) as usize];
        let antes = if k == -d || (k != d && at(k - 1) < at(k + 1)) { k + 1 } else { k - 1 };
        let (px, py) = (at(antes), at(antes) - antes);
        while x > px && y > py { ops.push(Mudanca::Igual); x -= 1; y -= 1; }
        if d > 0 {
            if x == px { ops.push(Mudanca::Entrou) } else { ops.push(Mudanca::Saiu) }
            (x, y) = (px, py);
        }
    }
    ops.reverse();
    ops
}

fn palavras(pdf: &DocumentoPdf) -> Vec<(String, u32)> {
    (1..=pdf.total_paginas()).flat_map(|p| {
        pdf.texto_pagina(p).unwrap_or_default().split_whitespace().map(|w| (w.to_string(), p)).collect::<Vec<_>>()
    }).collect()
}

/// O que mudou de `antiga` para `nova`, em trechos seguidos do mesmo tipo.
pub fn comparar_texto(antiga: &DocumentoPdf, nova: &DocumentoPdf) -> Vec<Trecho> {
    let (a, b) = (palavras(antiga), palavras(nova));
    let sa: Vec<&str> = a.iter().map(|w| w.0.as_str()).collect();
    let sb: Vec<&str> = b.iter().map(|w| w.0.as_str()).collect();
    let mut saida: Vec<Trecho> = Vec::new();
    let (mut i, mut j) = (0, 0);
    for op in myers(&sa, &sb) {
        let (texto, pa, pb) = match op {
            Mudanca::Igual => { let r = (sa[i], Some(a[i].1), Some(b[j].1)); i += 1; j += 1; r }
            Mudanca::Saiu => { let r = (sa[i], Some(a[i].1), None); i += 1; r }
            Mudanca::Entrou => { let r = (sb[j], None, Some(b[j].1)); j += 1; r }
        };
        match saida.last_mut() {
            Some(t) if t.mudanca == op => { t.texto.push(' '); t.texto += texto; }
            _ => saida.push(Trecho { mudanca: op, texto: texto.to_string(), pagina_antiga: pa, pagina_nova: pb }),
        }
    }
    saida
}

/// Caixa da tinta (pixels mais escuros que o papel): x0, y0, x1, y1.
fn tinta(rgba: &[u8], largura: usize) -> Option<(usize, usize, usize, usize)> {
    let mut c: Option<(usize, usize, usize, usize)> = None;
    for (i, px) in rgba.chunks_exact(4).enumerate() {
        if (px[0] as u32 + px[1] as u32 + px[2] as u32) < 600 {
            let (x, y) = (i % largura, i / largura);
            c = Some(c.map_or((x, y, x, y), |(a, b, cc, d)| (a.min(x), b.min(y), cc.max(x), d.max(y))));
        }
    }
    c
}

/// Sobrepõe as duas páginas (RGBA do mesmo tamanho): vermelho só na antiga, azul só na nova,
/// cinza nas duas. Devolve a imagem e o deslocamento (dx, dy) aplicado à nova.
pub fn sobrepor(antiga: &[u8], nova: &[u8], largura: u32, altura: u32) -> (Vec<u8>, (i32, i32)) {
    let (l, h) = (largura as usize, altura as usize);
    let (dx, dy) = match (tinta(antiga, l), tinta(nova, l)) {
        (Some(a), Some(b)) => (a.0 as i32 - b.0 as i32, a.1 as i32 - b.1 as i32),
        _ => (0, 0),
    };
    let escuro = |px: &[u8]| 1.0 - (px[0] as f32 + px[1] as f32 + px[2] as f32) / 765.0;
    let mut saida = vec![255u8; l * h * 4];
    for y in 0..h {
        for x in 0..l {
            let i = (y * l + x) * 4;
            let ta = escuro(&antiga[i..i + 4]);
            let (nx, ny) = (x as i32 - dx, y as i32 - dy);
            let tb = if nx >= 0 && ny >= 0 && (nx as usize) < l && (ny as usize) < h { let j = (ny as usize * l + nx as usize) * 4; escuro(&nova[j..j + 4]) } else { 0.0 };
            let comum = ta.min(tb);
            let (so_a, so_b) = (ta - comum, tb - comum);
            // Cinza no que é comum; vermelho e azul no que sobra de cada lado.
            let canal = |base: f32, tira: f32| (255.0 * (1.0 - base * 0.75 - tira)).clamp(0.0, 255.0) as u8;
            saida[i] = canal(comum, so_b);
            saida[i + 1] = canal(comum, so_a + so_b);
            saida[i + 2] = canal(comum, so_a);
        }
    }
    (saida, (dx, dy))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn myers_acha_o_minimo() {
        let a: Vec<char> = "ABCABBA".chars().collect();
        let b: Vec<char> = "CBABAC".chars().collect();
        let ops = myers(&a, &b);
        assert_eq!(ops.iter().filter(|o| **o != Mudanca::Igual).count(), 5);
        // Refaz b a partir de a pelas operações.
        let (mut i, mut j, mut refeito) = (0, 0, String::new());
        for o in ops {
            match o { Mudanca::Igual => { assert_eq!(a[i], b[j]); refeito.push(a[i]); i += 1; j += 1 } Mudanca::Saiu => i += 1, Mudanca::Entrou => { refeito.push(b[j]); j += 1 } }
        }
        assert_eq!(refeito, "CBABAC");
        assert_eq!(myers::<char>(&[], &['x']), vec![Mudanca::Entrou]);
        assert_eq!(myers(&['x'], &['x']), vec![Mudanca::Igual]);
    }

    #[test]
    fn contrato_com_clausula_trocada() {
        let antiga = crate::objetos::testes::pdf_com("BT /F1 12 Tf 50 700 Td (O prazo e de 30 dias corridos.) Tj ET");
        let nova = crate::objetos::testes::pdf_com("BT /F1 12 Tf 50 700 Td (O prazo e de 45 dias uteis.) Tj ET");
        let t = comparar_texto(&antiga, &nova);
        let saiu: Vec<&str> = t.iter().filter(|x| x.mudanca == Mudanca::Saiu).map(|x| x.texto.as_str()).collect();
        let entrou: Vec<&str> = t.iter().filter(|x| x.mudanca == Mudanca::Entrou).map(|x| x.texto.as_str()).collect();
        assert_eq!(saiu, ["30", "corridos."]);
        assert_eq!(entrou, ["45", "uteis."]);
    }

    #[test]
    fn sobrepor_alinha_e_pinta() {
        // 4×1: tinta no x=1 na antiga e no x=2 na nova (mesma coisa, deslocada) → tudo cinza.
        let mut a = vec![255u8; 16];
        let mut b = vec![255u8; 16];
        a[4..7].copy_from_slice(&[0, 0, 0]);
        b[8..11].copy_from_slice(&[0, 0, 0]);
        let (img, d) = sobrepor(&a, &b, 4, 1);
        assert_eq!(d, (-1, 0));
        assert!(img[4] == img[6] && img[4] < 100, "{:?}", &img[4..7]);
    }
}
