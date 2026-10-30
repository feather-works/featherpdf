//! Conta campos (sem valores): `cargo run --example campos -- arquivo.pdf`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pdf = featherpdf_io::abrir(std::path::Path::new(&a[1])).unwrap();
    let c = pdf.campos();
    let mut por_tipo = std::collections::BTreeMap::new();
    for x in &c { *por_tipo.entry(format!("{:?}", x.tipo)).or_insert(0) += 1; }
    println!("campos existentes: {} {:?}", c.len(), por_tipo);
    let mut tipos = std::collections::BTreeMap::new();
    let mut tam = std::collections::BTreeMap::new();
    for p in 1..=pdf.total_paginas() {
        for (c, t, _) in pdf.detectar_campos(p).unwrap_or_default() {
            *tipos.entry(format!("{t:?}")).or_insert(0) += 1;
            *tam.entry(format!("{:.0}x{:.0}", c[2] - c[0], c[3] - c[1])).or_insert(0) += 1;
        }
    }
    let mut tam: Vec<_> = tam.into_iter().collect();
    tam.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("o Detectar campos criaria: {tipos:?}; tamanhos mais comuns: {:?}", &tam[..tam.len().min(8)]);
}
