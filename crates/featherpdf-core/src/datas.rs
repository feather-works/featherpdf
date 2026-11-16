//! Formato dos campos de data, como o Acrobat: o `AFDate_FormatEx("mm/dd/yyyy")` (ou o
//! `AFDate_Format(n)`) do /AA /F diz como a data é escrita; o que se digita é lido com folga
//! (os números na ordem do formato, mês por nome, "16112026" sem barras) e reescrito no formato.

/// O formato dos campos de data criados pelo FeatherPDF.
pub const FORMATO_DATA: &str = "dd/mm/yyyy";

const MESES: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];
const MESES_EN: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
const SEMANA: [&str; 7] = ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"];

/// A lista do `AFDate_Format(n)` do Acrobat (AForm.js).
const FORMATOS_ACROBAT: [&str; 14] = ["m/d", "m/d/yy", "mm/dd/yy", "mm/yy", "d-mmm", "d-mmm-yy", "dd-mmm-yy", "yy-mm-dd", "mmm-yy",
    "mmmm-yy", "mmm d, yyyy", "mmmm d, yyyy", "m/d/yy h:MM tt", "m/d/yy HH:MM"];

/// O formato de data escrito no script do campo (`AFDate_FormatEx("…")` ou `AFDate_Format(n)`).
pub(crate) fn formato_do_js(js: &str) -> Option<String> {
    if let Some(i) = js.find("AFDate_FormatEx(") {
        let resto = &js[i + 16..];
        let aspa = resto.trim_start().chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let resto = &resto.trim_start()[1..];
        return resto.find(aspa).map(|f| resto[..f].to_string());
    }
    let i = js.find("AFDate_Format(")?;
    let n: usize = js[i + 14..].split(')').next()?.trim().parse().ok()?;
    FORMATOS_ACROBAT.get(n).map(|f| f.to_string())
}

/// Pedaços do formato: letras repetidas (d, mm, yyyy, HH…) ou texto fixo.
fn pedacos(formato: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in formato.chars() {
        match v.last_mut() {
            Some(u) if c.is_ascii_alphabetic() && u.starts_with(c) => u.push(c),
            _ => v.push(c.to_string()),
        }
    }
    v
}

/// Formato com hora (h, H, M, s, t): a data fica como foi digitada.
fn tem_hora(formato: &str) -> bool { formato.chars().any(|c| matches!(c, 'h' | 'H' | 'M' | 's' | 't')) }

fn dias_no_mes(ano: i32, mes: u32) -> u32 {
    match mes { 2 if (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0 => 29, 2 => 28, 4 | 6 | 9 | 11 => 30, _ => 31 }
}

/// 0 = domingo (Sakamoto).
fn dia_da_semana(ano: i32, mes: u32, dia: u32) -> usize {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let a = if mes < 3 { ano - 1 } else { ano };
    (a + a.div_euclid(4) - a.div_euclid(100) + a.div_euclid(400) + T[mes as usize - 1] + dia as i32).rem_euclid(7) as usize
}

/// (ano, mês, dia) escrito no formato do campo.
pub fn escrever_data(formato: &str, (ano, mes, dia): (i32, u32, u32)) -> String {
    pedacos(formato).iter().map(|p| match p.as_str() {
        "d" => dia.to_string(),
        "dd" => format!("{dia:02}"),
        "ddd" => SEMANA[dia_da_semana(ano, mes, dia)][..3].to_string(),
        "dddd" => SEMANA[dia_da_semana(ano, mes, dia)].to_string(),
        "m" => mes.to_string(),
        "mm" => format!("{mes:02}"),
        "mmm" => MESES[mes as usize - 1].chars().take(3).collect(),
        "mmmm" => MESES[mes as usize - 1].to_string(),
        "yy" => format!("{:02}", ano.rem_euclid(100)),
        "yyyy" => format!("{ano:04}"),
        p => p.to_string(),
    }).collect()
}

fn mes_por_nome(palavra: &str) -> Option<u32> {
    let p: String = palavra.to_lowercase().chars().take(3).collect();
    if p.chars().count() < 3 { return None; }
    MESES.iter().chain(&MESES_EN).position(|m| m.starts_with(&p)).map(|i| i as u32 % 12 + 1)
}

/// Lê o que foi digitado com a ordem do formato ("3/4/26" em "mm/dd/yyyy" é 4 de março).
/// `None`: não é uma data que existe.
pub fn ler_data(formato: &str, texto: &str, ano_atual: i32) -> Option<(i32, u32, u32)> {
    let partes = pedacos(formato);
    let mut numeros: Vec<String> = Vec::new();
    let mut mes = None;
    let mut atual = String::new();
    for c in texto.chars().chain([' ']) {
        if c.is_alphanumeric() && (atual.is_empty() || atual.chars().all(|x| x.is_ascii_digit()) == c.is_ascii_digit()) { atual.push(c); continue; }
        if atual.chars().all(|x| x.is_ascii_digit()) && !atual.is_empty() { numeros.push(std::mem::take(&mut atual)); }
        else if !atual.is_empty() { mes = mes.or(mes_por_nome(&atual)); atual.clear(); }
        if c.is_alphanumeric() { atual.push(c); }
    }
    // A ordem dos números no formato: dia, mês (só o mês em número) e ano.
    let mut ordem: Vec<&String> = partes.iter().filter(|p| matches!(p.as_str(), "d" | "dd" | "yy" | "yyyy") || (matches!(p.as_str(), "m" | "mm") && mes.is_none())).collect();
    // "16112026" sem separador: corta pela largura de cada pedaço do formato.
    if numeros.len() == 1 && ordem.len() > 1 {
        let larguras: Vec<usize> = ordem.iter().map(|p| if p.as_str() == "yyyy" { 4 } else { 2 }).collect();
        if numeros[0].len() == larguras.iter().sum::<usize>() {
            let s = numeros.remove(0);
            let mut i = 0;
            for l in larguras { numeros.push(s[i..i + l].to_string()); i += l; }
        }
    }
    let (mut d, mut a) = (None, None);
    // Quatro algarismos é o ano, onde quer que esteja ("16/11/2026" num campo "yyyy-mm-dd").
    if let Some(i) = numeros.iter().position(|n| n.len() == 4) {
        a = numeros.remove(i).parse().ok();
        // Ano do outro lado do formato ("16/11/2026" num campo "yyyy-mm-dd"): o resto vem ao contrário.
        if ordem.iter().position(|p| p.starts_with('y')).is_some_and(|j| (j == 0) != (i == 0)) { ordem.reverse(); }
        ordem.retain(|p| !p.starts_with('y'));
    }
    if numeros.len() > ordem.len() || (numeros.is_empty() && a.is_none()) { return None; }
    for (p, n) in ordem.iter().zip(&numeros) {
        let n: i64 = n.parse().ok()?;
        match p.chars().next() { Some('d') => d = Some(n), Some('m') => mes = Some(u32::try_from(n).ok()?), _ => a = Some(n) }
    }
    // Ano de dois algarismos como o Acrobat: até 49 é 20xx, de 50 em diante é 19xx.
    let ano = match a { Some(a) if a < 50 => 2000 + a, Some(a) if a < 100 => 1900 + a, Some(a) => a, None => ano_atual as i64 };
    let ano = i32::try_from(ano).ok()?;
    let (mes, dia) = (mes?, u32::try_from(d.unwrap_or(1)).ok()?);
    ((1..=12).contains(&mes) && (1..=dias_no_mes(ano, mes)).contains(&dia)).then_some((ano, mes, dia))
}

/// Confere e reescreve o valor de um campo de data no formato dele.
pub(crate) fn aplicar(formato: &str, nome: &str, valor: &str, ano_atual: i32) -> Result<String, String> {
    if valor.trim().is_empty() || tem_hora(formato) { return Ok(valor.to_string()); }
    ler_data(formato, valor, ano_atual).map(|d| escrever_data(formato, d))
        .ok_or_else(|| format!("“{nome}”: “{}” não é uma data válida. Use o formato {formato}.", valor.trim()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn data_lida_e_escrita_no_formato_do_campo() {
        assert_eq!(formato_do_js("AFDate_FormatEx(\"mm/dd/yyyy\");").as_deref(), Some("mm/dd/yyyy"));
        assert_eq!(formato_do_js("AFDate_Format(2);").as_deref(), Some("mm/dd/yy"));
        // A mesma digitação, lida na ordem de cada formato.
        assert_eq!(ler_data("dd/mm/yyyy", "3/4/26", 2026), Some((2026, 4, 3)));
        assert_eq!(ler_data("mm/dd/yyyy", "3/4/26", 2026), Some((2026, 3, 4)));
        assert_eq!(ler_data("dd/mm/yyyy", "16112026", 2026), Some((2026, 11, 16)));
        assert_eq!(ler_data("dd/mm/yyyy", "05/03/85", 2026), Some((1985, 3, 5)));
        assert_eq!(ler_data("d-mmm-yy", "5 março 2026", 2026), Some((2026, 3, 5)));
        assert_eq!(ler_data("mm/dd", "12/25", 2026), Some((2026, 12, 25)));
        assert_eq!(ler_data("dd/mm/yyyy", "31/02/2026", 2026), None);
        assert_eq!(ler_data("dd/mm/yyyy", "amanhã", 2026), None);
        assert_eq!(escrever_data("dd/mm/yyyy", (2026, 3, 5)), "05/03/2026");
        assert_eq!(escrever_data("mmm d, yyyy", (2026, 3, 5)), "mar 5, 2026");
        assert_eq!(escrever_data("dddd, d", (2026, 11, 16)).split(',').next(), Some("segunda-feira"));
        assert_eq!(aplicar("yyyy-mm-dd", "Data", "16/11/2026", 2026).as_deref(), Ok("2026-11-16"));
        assert!(aplicar("dd/mm/yyyy", "Data", "31/04/2026", 2026).is_err());
        assert_eq!(aplicar("m/d/yy h:MM tt", "Data", "qualquer", 2026).as_deref(), Ok("qualquer"));
    }
}
