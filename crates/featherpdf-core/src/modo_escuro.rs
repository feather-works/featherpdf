//! Modo escuro inteligente: o papel fica escuro e o texto claro, mas fotos, gráficos coloridos e
//! carimbos continuam como são. Inverte só a claridade (o tom e a saturação ficam: texto azul
//! vira azul-claro), e não toca nos retângulos das imagens da página.

/// Escurece o RGBA (opaco) no lugar. `fotos`: retângulos `[x, y, largura, altura]` em pixels
/// que ficam como estão.
pub fn escurecer(rgba: &mut [u8], largura: u32, fotos: &[[u32; 4]]) {
    let largura = largura as usize;
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        let (x, y) = ((i % largura) as u32, (i / largura) as u32);
        if fotos.iter().any(|f| x >= f[0] && x < f[0] + f[2] && y >= f[1] && y < f[1] + f[3]) { continue; }
        let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
        // Claridade invertida com o mesmo tom: soma a todos o que leva max+min a 255 - (max+min).
        let k = 255 - r.max(g).max(b) - r.min(g).min(b);
        // Papel em cinza-escuro (não preto puro) e texto quase branco, que cansa menos.
        let tom = |c: i32| (28.0 + (c + k).clamp(0, 255) as f32 * 0.88).round() as u8;
        px[0] = tom(r);
        px[1] = tom(g);
        px[2] = tom(b);
    }
}

#[cfg(test)]
mod testes {
    use super::escurecer;

    #[test]
    fn papel_escurece_texto_clareia_e_foto_fica() {
        // branco, preto, azul, e um pixel de foto (vermelho) protegido
        let mut px = vec![255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 200, 255, 200, 10, 10, 255];
        escurecer(&mut px, 4, &[[3, 0, 1, 1]]);
        assert_eq!(&px[0..3], &[28, 28, 28]);
        assert_eq!(&px[4..7], &[252, 252, 252]);
        assert!(px[10] > px[8] && px[8] > 60, "azul continua azul, mais claro: {:?}", &px[8..11]);
        assert_eq!(&px[12..15], &[200, 10, 10]);
    }
}
