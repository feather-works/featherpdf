//! Embutir as fontes que ficaram de fora do PDF: acha a mesma no sistema (Arial, Times…) ou,
//! sem ela, uma de medidas parecidas (Liberation, DejaVu), e grava pelo
//! `DocumentoPdf::embutir_fonte`.
//! shortcut: embute a fonte inteira (sem subconjunto dos glifos usados); uma família comum soma ~0,5 MB.

use featherpdf_core::{caractere_win_ansi, DocumentoPdf, FonteNova};

/// Família, negrito e itálico pelo /BaseFont ("ABCDEF+Arial,BoldItalic", "TimesNewRomanPS-BoldMT").
pub(crate) fn ler_nome(base: &str) -> (String, bool, bool) {
    let base = base.split_once('+').filter(|(p, _)| p.len() == 6).map_or(base, |(_, n)| n);
    let (familia, estilo) = base.split_once([',', '-']).unwrap_or((base, ""));
    let estilo = estilo.to_lowercase();
    let negrito = ["bold", "black", "heavy", "semibold"].iter().any(|s| estilo.contains(s));
    let italico = estilo.contains("italic") || estilo.contains("oblique");
    let familia = familia.trim_end_matches("MT").trim_end_matches("PS");
    // "TimesNewRoman" → "Times New Roman": o sistema guarda a família com espaços.
    let mut f = String::new();
    for (i, c) in familia.chars().enumerate() {
        if i > 0 && c.is_uppercase() && !f.ends_with(' ') { f.push(' '); }
        f.push(c);
    }
    (f, negrito, italico)
}

/// Famílias a tentar, a primeira é a própria; depois as de mesmas medidas.
fn candidatas(familia: &str) -> Vec<String> {
    let f = familia.to_lowercase();
    let parecidas: &[&str] = if f.starts_with("times") { &["Times New Roman", "Liberation Serif", "Tinos", "DejaVu Serif"] }
        else if f.starts_with("courier") { &["Courier New", "Liberation Mono", "Cousine", "DejaVu Sans Mono"] }
        else if f.starts_with("helvetica") || f.starts_with("arial") { &["Arial", "Liberation Sans", "Arimo", "DejaVu Sans"] }
        else { &["Liberation Sans", "DejaVu Sans"] };
    std::iter::once(familia.to_string()).chain(parecidas.iter().map(|s| s.to_string())).collect()
}

/// Embute cada fonte de fora que achar. Devolve (original, a que entrou) de cada uma e as que
/// não acharam nenhuma.
pub fn embutir_fontes_do_sistema(pdf: &mut DocumentoPdf) -> Result<(Vec<(String, String)>, Vec<String>), String> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let (mut feitas, mut faltaram) = (Vec::new(), Vec::new());
    for (id, base) in pdf.fontes_sem_embutir() {
        let (familia, negrito, italico) = ler_nome(&base);
        let nova = candidatas(&familia).iter().find_map(|f| {
            let consulta = fontdb::Query {
                families: &[fontdb::Family::Name(f)],
                weight: if negrito { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
                style: if italico { fontdb::Style::Italic } else { fontdb::Style::Normal },
                stretch: fontdb::Stretch::Normal,
            };
            let face = db.query(&consulta)?;
            db.with_face_data(face, |dados, indice| medir(dados, indice)).flatten()
        });
        match nova {
            Some(f) => {
                feitas.push((base, f.nome_ps.clone()));
                pdf.embutir_fonte(id, f)?;
            }
            None => faltaram.push(base),
        }
    }
    Ok((feitas, faltaram))
}

/// Lê o programa e as medidas. Só TrueType de verdade (contorno `glyf`, arquivo de uma fonte só):
/// é o que o `FontFile2` aceita.
fn medir(dados: &[u8], indice: u32) -> Option<FonteNova> {
    if indice != 0 || dados.starts_with(b"ttcf") { return None; }
    let face = ttf_parser::Face::parse(dados, 0).ok()?;
    face.tables().glyf?;
    let em = face.units_per_em() as f32;
    let k = 1000.0 / em;
    let largura = |c: char| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map_or(0.0, |a| a as f32 * k);
    let larguras = (32u8..=255).map(|b| caractere_win_ansi(b).map_or(0.0, largura)).collect();
    let nome_ps = face.names().into_iter().find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME).and_then(|n| n.to_string())?;
    let r = face.global_bounding_box();
    Some(FonteNova {
        nome_ps: nome_ps.replace(' ', ""),
        programa: dados.to_vec(),
        larguras,
        ascent: face.ascender() as f32 * k,
        descent: face.descender() as f32 * k,
        cap: face.capital_height().map_or(face.ascender() as f32 * 0.7, f32::from) * k,
        caixa: [r.x_min as f32 * k, r.y_min as f32 * k, r.x_max as f32 * k, r.y_max as f32 * k],
        italico: face.italic_angle().unwrap_or(0.0),
        fixa: face.is_monospaced(),
        serifa: nome_ps.to_lowercase().contains("serif") && !nome_ps.to_lowercase().contains("sans") || nome_ps.starts_with("Times"),
    })
}

#[cfg(test)]
mod testes {
    use super::ler_nome;

    #[test]
    fn nome_da_fonte_vira_familia_e_estilo() {
        assert_eq!(ler_nome("ABCDEF+Arial,BoldItalic"), ("Arial".into(), true, true));
        assert_eq!(ler_nome("TimesNewRomanPS-BoldMT"), ("Times New Roman".into(), true, false));
        assert_eq!(ler_nome("Helvetica"), ("Helvetica".into(), false, false));
        assert_eq!(ler_nome("Courier-Oblique"), ("Courier".into(), false, true));
    }

    #[test]
    fn helvetica_do_texto_inserido_entra_no_arquivo() {
        let mut pdf = crate::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "Olá, fonte embutida", 12.0, false, [0.0; 3]).unwrap();
        let (feitas, faltaram) = super::embutir_fontes_do_sistema(&mut pdf).unwrap();
        if feitas.is_empty() { eprintln!("sem fonte parecida neste sistema: {faltaram:?}"); return; }
        assert!(pdf.fontes_sem_embutir().is_empty());
        let bytes = pdf.serializar().unwrap();
        assert!(crate::Visualizador::novo(bytes).unwrap().desenhar(1, 0.5).is_ok());
    }
}
