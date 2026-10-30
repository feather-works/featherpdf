//! Confere o classificador de orientação: uma página com texto girada nos quatro sentidos.
//! `cargo run --profile medir --example orientacao -- caminho/doc_ori.onnx`

fn main() {
    let modelo = std::fs::read(std::env::args().nth(1).expect("caminho do doc_ori.onnx")).unwrap();
    let o = featherpdf_ocr::Orientacao::de_bytes(&modelo).unwrap();
    let mut pdf = featherpdf_io::criar_a4().unwrap();
    for i in 0..18 {
        pdf.inserir_texto(1, 60.0, 80.0 + i as f32 * 36.0, "Relatório de inspeção da subestação, linha de teste", 16.0, false, [0.0; 3]).unwrap();
    }
    let r = featherpdf_io::Visualizador::novo(pdf.serializar().unwrap()).unwrap().desenhar(1, 1.5).unwrap();
    let rgb: Vec<u8> = r.rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let img = featherpdf_ocr::Imagem { largura: r.largura, altura: r.altura, rgb: &rgb };
    for g in [0, 90, 180, 270] {
        let (w, h, girada) = featherpdf_ocr::girar(&img, g);
        let det = o.detectar(&featherpdf_ocr::Imagem { largura: w, altura: h, rgb: &girada }).unwrap();
        println!("girada {g}° no sentido horário -> classe {det:?}");
    }
}
