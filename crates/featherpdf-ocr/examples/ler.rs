//! `cargo run --release -p featherpdf-ocr --example ler -- <pasta dos modelos> <imagem>`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let ocr = featherpdf_ocr::Ocr::abrir(std::path::Path::new(&args[1])).unwrap();
    eprintln!("modelos: {:?}", t.elapsed());
    let img = image::open(&args[2]).unwrap().to_rgb8();
    let t = std::time::Instant::now();
    let _ = ocr.reconhecer(&featherpdf_ocr::Imagem { largura: img.width(), altura: img.height(), rgb: img.as_raw() }).unwrap();
    eprintln!("primeira página (monta os planos): {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let linhas = ocr.reconhecer(&featherpdf_ocr::Imagem { largura: img.width(), altura: img.height(), rgb: img.as_raw() }).unwrap();
    eprintln!("página {}x{}: {:?}, {} linhas", img.width(), img.height(), t.elapsed(), linhas.len());
    for l in linhas { println!("{:.2}  {:?}  {}", l.confianca, l.caixa.map(|v| v as i32), l.texto); }
}
