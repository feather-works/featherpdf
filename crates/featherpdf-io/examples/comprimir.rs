//! `cargo run --example comprimir -- entrada.pdf saida.pdf dpi qualidade [zopfli]`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pdf = featherpdf_io::abrir(std::path::Path::new(&a[1])).unwrap();
    let t = std::time::Instant::now();
    let op = featherpdf_core::OpcoesCompressao { dpi: a[3].parse().ok(), qualidade: a[4].parse().unwrap(), limpar: true, anexos: false, zopfli: a.get(5).is_some_and(|z| z == "zopfli") };
    let b = pdf.comprimido(op, &featherpdf_io::reamostrar_jpeg).unwrap();
    std::fs::write(&a[2], &b).unwrap();
    eprintln!("{} bytes em {:?}", b.len(), t.elapsed());
}
