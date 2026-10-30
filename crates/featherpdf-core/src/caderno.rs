//! Limpar foto de caderno: tira a sombra (o papel vira branco por igual, mesmo com a mão ou o
//! celular fazendo sombra num canto), a pauta e o quadriculado (traços claros), e deixa só a
//! tinta, com a cor dela (caneta azul continua azul).

/// Bloco da estimativa do papel: maior que a letra, menor que a variação da sombra.
const BLOCO: usize = 24;

/// Limpa a imagem RGB no lugar.
pub fn limpar_caderno(rgb: &mut [u8], largura: u32, altura: u32) {
    let (l, h) = (largura as usize, altura as usize);
    if l == 0 || h == 0 || rgb.len() < l * h * 3 { return; }
    // 1. O papel em cada bloco: o pixel mais claro de cada canal (tinta e pauta são mais escuras).
    let (gl, gh) = (l.div_ceil(BLOCO), h.div_ceil(BLOCO));
    let mut papel = vec![[0f32; 3]; gl * gh];
    for y in 0..h {
        for x in 0..l {
            let (i, g) = ((y * l + x) * 3, (y / BLOCO) * gl + x / BLOCO);
            for c in 0..3 { papel[g][c] = papel[g][c].max(rgb[i + c] as f32); }
        }
    }
    // Suaviza a grade (bloco só de tinta, numa letra grande, puxa o vizinho).
    for _ in 0..2 {
        let antes = papel.clone();
        for gy in 0..gh {
            for gx in 0..gl {
                let mut soma = [0f32; 3];
                let mut n = 0.0;
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (0, 0)] {
                    let (x, y) = (gx as i32 + dx, gy as i32 + dy);
                    if x < 0 || y < 0 || x as usize >= gl || y as usize >= gh { continue; }
                    let v = antes[y as usize * gl + x as usize];
                    for c in 0..3 { soma[c] += v[c]; }
                    n += 1.0;
                }
                // O máximo entre a média e o próprio: a sombra suaviza sem escurecer o papel.
                for c in 0..3 { papel[gy * gl + gx][c] = (soma[c] / n).max(antes[gy * gl + gx][c] * 0.97); }
            }
        }
    }
    // 2. Cada pixel dividido pelo papel ali (interpolado entre os centros dos blocos).
    let amostra = |x: usize, y: usize| -> [f32; 3] {
        let fx = ((x as f32 + 0.5) / BLOCO as f32 - 0.5).clamp(0.0, (gl - 1) as f32);
        let fy = ((y as f32 + 0.5) / BLOCO as f32 - 0.5).clamp(0.0, (gh - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(gl - 1), (y0 + 1).min(gh - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let p = |x: usize, y: usize| papel[y * gl + x];
        let mut r = [0f32; 3];
        for c in 0..3 {
            let cima = p(x0, y0)[c] * (1.0 - tx) + p(x1, y0)[c] * tx;
            let baixo = p(x0, y1)[c] * (1.0 - tx) + p(x1, y1)[c] * tx;
            r[c] = (cima * (1.0 - ty) + baixo * ty).max(1.0);
        }
        r
    };
    for y in 0..h {
        for x in 0..l {
            let i = (y * l + x) * 3;
            let p = amostra(x, y);
            let v: [f32; 3] = std::array::from_fn(|c| (rgb[i + c] as f32 * 255.0 / p[c]).min(255.0));
            // 3. Claro (pauta, quadriculado, papel) vira branco; a tinta fica, mais firme.
            let lum = 0.299 * v[0] + 0.587 * v[1] + 0.114 * v[2];
            let branco = ((lum - 150.0) / (200.0 - 150.0)).clamp(0.0, 1.0);
            for c in 0..3 {
                let firme = (v[c] * 0.85).min(255.0);
                rgb[i + c] = (firme * (1.0 - branco) + 255.0 * branco).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::limpar_caderno;

    #[test]
    fn sombra_e_pauta_saem_tinta_fica() {
        let (l, h) = (200usize, 120usize);
        let mut img = vec![0u8; l * h * 3];
        for y in 0..h {
            for x in 0..l {
                // Papel com sombra: claro à esquerda, cinza à direita.
                let papel = 240.0 - x as f32 * 0.5;
                let mut px = [papel, papel, papel * 0.98];
                if y % 20 == 0 { px = [papel * 0.75, papel * 0.85, papel]; } // pauta azul-clara
                if (60..64).contains(&y) && (20..180).contains(&x) { px = [30.0, 40.0, 120.0]; } // caneta azul
                img[(y * l + x) * 3..][..3].copy_from_slice(&px.map(|v| v as u8));
            }
        }
        limpar_caderno(&mut img, l as u32, h as u32);
        let px = |x: usize, y: usize| &img[(y * l + x) * 3..][..3];
        assert_eq!(px(10, 10), [255, 255, 255], "papel claro");
        assert_eq!(px(190, 10), [255, 255, 255], "papel na sombra");
        assert_eq!(px(100, 40), [255, 255, 255], "pauta some");
        let tinta = px(100, 62);
        assert!(tinta[2] > tinta[0] && tinta[2] < 200, "caneta continua azul e escura: {tinta:?}");
    }
}
