//! Mede abrir um PDF: `cargo run --release --example medir_abrir -- arquivo.pdf`
//! (rodar com `/usr/bin/time -v` para o pico de memória).
use std::time::Instant;

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status").ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).map(|l| l.split_whitespace().nth(1).unwrap_or("0").parse::<u64>().unwrap_or(0)))
        .unwrap_or(0) / 1024
}

fn main() {
    let caminho = std::path::PathBuf::from(std::env::args().nth(1).expect("arquivo.pdf"));
    let t = Instant::now();
    let vis = featherpdf_io::Visualizador::do_arquivo(&caminho, u32::MAX);
    println!("hayro sozinho (contagem não bate de propósito): {:?} · {} MB", t.elapsed(), rss_mb());
    drop(vis);
    let t = Instant::now();
    let pdf = featherpdf_io::abrir_com_senha(&caminho, None).unwrap();
    println!("lopdf: {} páginas em {:?} · {} MB", pdf.total_paginas(), t.elapsed(), rss_mb());
    let t = Instant::now();
    let vis = featherpdf_io::Visualizador::do_arquivo(&caminho, pdf.total_paginas()).unwrap();
    println!("hayro mapeado: {:?} · {} MB", t.elapsed(), rss_mb());
    let t = Instant::now();
    let p = vis.desenhar(1, 1.0).unwrap();
    println!("página 1 ({}×{}): {:?} · {} MB", p.largura, p.altura, t.elapsed(), rss_mb());
}
