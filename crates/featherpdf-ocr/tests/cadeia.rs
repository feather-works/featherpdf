//! A cadeia inteira sem arquivo de ninguém: escreve um texto num PDF, desenha a página,
//! lê com o OCR e grava a camada invisível de volta.

#[test]
fn le_o_texto_desenhado_e_grava_a_camada() {
    let mut pdf = featherpdf_io::criar_a4().unwrap();
    pdf.inserir_texto(1, 72.0, 100.0, "FATURA 2026", 36.0, false, [0.0; 3]).unwrap();
    let r = featherpdf_io::Visualizador::novo(pdf.serializar().unwrap()).unwrap().desenhar(1, 2.0).unwrap();
    let rgb: Vec<u8> = r.rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let ocr = featherpdf_ocr::Ocr::embutido().unwrap();
    let linhas = ocr.reconhecer(&featherpdf_ocr::Imagem { largura: r.largura, altura: r.altura, rgb: &rgb }).unwrap();
    let lido: Vec<_> = linhas.iter().map(|l| l.texto.as_str()).collect();
    assert!(lido.iter().any(|t| t.contains("FATURA")), "lido: {lido:?}");
    // As palavras: "FATURA" começa onde o texto foi escrito (72 pt × 2) e termina antes do "2026".
    let linha = linhas.iter().find(|l| l.texto.contains("FATURA")).unwrap();
    let p: Vec<_> = linha.palavras.iter().map(|p| (p.texto.as_str(), p.caixa[0].round(), p.caixa[2].round())).collect();
    assert_eq!(p.iter().map(|x| x.0).collect::<Vec<_>>(), ["FATURA", "2026"], "{p:?}");
    assert!((p[0].1 - 144.0).abs() < 16.0 && p[0].2 < p[1].1 + 4.0, "{p:?}");

    // Uma página só com a camada: o texto vem do OCR, não do inserir_texto.
    let mut limpo = featherpdf_io::criar_a4().unwrap();
    let (l, a) = (r.largura as f32, r.altura as f32);
    let camada: Vec<_> = linhas.iter().map(|x| (x.texto.clone(), [x.caixa[0] / l, x.caixa[1] / a, x.caixa[2] / l, x.caixa[3] / a])).collect();
    assert!(limpo.inserir_camada_ocr(1, &camada).unwrap() > 0);
    assert!(limpo.texto_pagina(1).unwrap().contains("FATURA"));
    // A camada em outra escrita (fonte Unicode sem desenho) não atrapalha o desenho da página.
    limpo.inserir_camada_ocr(1, &[("Αθήνα".into(), [0.1, 0.5, 0.4, 0.53])]).unwrap();
    featherpdf_io::Visualizador::novo(limpo.serializar().unwrap()).unwrap().desenhar(1, 1.0).unwrap();
}
