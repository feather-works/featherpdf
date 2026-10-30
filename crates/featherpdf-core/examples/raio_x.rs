//! Estrutura de um PDF, sem o conteúdo: imagens por filtro/cor/bits e o que pesa no resto.
//! `cargo run --example raio_x -- arquivo.pdf`
use std::collections::BTreeMap;
use lopdf::{Document, Object};

fn nome(o: Option<&Object>, doc: &Document) -> String {
    match o.map(|o| doc.dereference(o).map(|x| x.1).unwrap_or(o)) {
        Some(Object::Name(n)) => String::from_utf8_lossy(n).into(),
        Some(Object::Array(v)) => v.iter().map(|x| nome(Some(x), doc)).collect::<Vec<_>>().join("+"),
        Some(o) => format!("{:?}", o).chars().take(20).collect(),
        None => "-".into(),
    }
}

fn main() {
    let p = std::env::args().nth(1).unwrap();
    let doc = Document::load(&p).unwrap();
    let mut img: BTreeMap<String, (usize, usize, u64)> = BTreeMap::new();
    let mut resto: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (_, o) in &doc.objects {
        let Object::Stream(s) = o else { continue };
        let d = &s.dict;
        let filtro = nome(d.get(b"Filter").ok(), &doc);
        if d.get(b"Subtype").ok().and_then(|x| x.as_name().ok()) == Some(b"Image") {
            let w = d.get(b"Width").and_then(Object::as_i64).unwrap_or(0);
            let h = d.get(b"Height").and_then(Object::as_i64).unwrap_or(0);
            let cs = nome(d.get(b"ColorSpace").ok(), &doc);
            let cs: String = cs.chars().take(30).collect();
            let k = format!("{filtro} | {cs} | parms {} | bpc {} | mask {} smask {}", d.get(b"DecodeParms").map(|p| format!("{p:?}").chars().take(40).collect::<String>()).unwrap_or_default(), d.get(b"BitsPerComponent").and_then(Object::as_i64).unwrap_or(0),
                d.has(b"Mask"), d.has(b"SMask"));
            let e = img.entry(k).or_default();
            e.0 += 1; e.1 += s.content.len(); e.2 += (w * h) as u64;
        } else {
            let t = d.get(b"Type").ok().map(|x| nome(Some(x), &doc)).unwrap_or_default();
            let st = d.get(b"Subtype").ok().map(|x| nome(Some(x), &doc)).unwrap_or_default();
            let k = format!("{t}/{st} [{filtro}]");
            let e = resto.entry(k).or_default();
            e.0 += 1; e.1 += s.content.len();
        }
    }
    println!("páginas {}  objetos {}", doc.get_pages().len(), doc.objects.len());
    println!("IMAGENS (qtd, MB, megapixels):");
    for (k, (n, b, px)) in &img { println!("  {n:4} {:7.2} MB {:8.1} Mpx  {k}", *b as f64 / 1e6, *px as f64 / 1e6); }
    println!("OUTROS STREAMS:");
    let mut r: Vec<_> = resto.into_iter().collect();
    r.sort_by_key(|x| std::cmp::Reverse(x.1 .1));
    for (k, (n, b)) in r.iter().take(12) { println!("  {n:4} {:7.2} MB  {k}", *b as f64 / 1e6); }
}
