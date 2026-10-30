//! Confere um modelo de leitura baixado: carrega com `com_reconhecimento` e lê uma página.
//! `cargo run --profile medir --example outra_lingua -- rec.onnx dict.txt`

fn main() {
    let mut a = std::env::args().skip(1);
    let (rec, dict) = (std::fs::read(a.next().unwrap()).unwrap(), std::fs::read_to_string(a.next().unwrap()).unwrap());
    let ocr = featherpdf_ocr::Ocr::embutido().unwrap().com_reconhecimento(&rec, &dict).unwrap();
    let mut pdf = featherpdf_io::criar_a4().unwrap();
    pdf.inserir_texto(1, 72.0, 100.0, "ATHINA 2026 KAPA", 30.0, false, [0.0; 3]).unwrap();
    let r = featherpdf_io::Visualizador::novo(pdf.serializar().unwrap()).unwrap().desenhar(1, 2.0).unwrap();
    let rgb: Vec<u8> = r.rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    for l in ocr.reconhecer(&featherpdf_ocr::Imagem { largura: r.largura, altura: r.altura, rgb: &rgb }).unwrap() {
        println!("{:?} {:.2}", l.texto, l.confianca);
    }
}
