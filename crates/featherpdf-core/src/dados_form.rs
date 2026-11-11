//! Dados do formulário para fora e para dentro: exportar em JSON ou XFDF (o XML de
//! dados de formulário do Acrobat) e preencher a partir de CSV ou JSON.

use crate::formularios::{Campo, TipoCampo};
use crate::DocumentoPdf;

/// Valor de cada campo com dado, um por nome: texto, o estado ligado da caixa/opção, ou
/// vazio. A caixa vem com `Some(marcada)` para o JSON sair como verdadeiro/falso.
fn valores(campos: &[Campo]) -> Vec<(String, String, Option<bool>)> {
    let mut v: Vec<(String, String, Option<bool>)> = Vec::new();
    for c in campos.iter().filter(|c| !matches!(c.tipo, TipoCampo::Assinatura | TipoCampo::Botao)) {
        let valor = match c.tipo {
            TipoCampo::Caixa | TipoCampo::Opcao => if c.marcada { c.ligado.clone() } else { String::new() },
            _ if c.valor == "Off" => String::new(),
            _ => c.valor.clone(),
        };
        match v.iter_mut().find(|x| x.0 == c.nome) {
            // Grupo de opções: vale a marcada.
            Some(x) => if x.1.is_empty() && !valor.is_empty() { x.1 = valor },
            None => v.push((c.nome.clone(), valor, (c.tipo == TipoCampo::Caixa).then_some(c.marcada))),
        }
    }
    v
}

fn json_texto(t: &str) -> String {
    let mut s = String::from("\"");
    for c in t.chars() {
        match c {
            '"' => s += "\\\"", '\\' => s += "\\\\", '\n' => s += "\\n", '\r' => s += "\\r", '\t' => s += "\\t",
            c if (c as u32) < 0x20 => s += &format!("\\u{:04x}", c as u32),
            c => s.push(c),
        }
    }
    s + "\""
}

fn xml(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// "sim", "x", "1", "true", "verdadeiro" ou qualquer texto marca; vazio, "não", "0", "off" e "falso" desmarcam.
fn marcado(v: &str) -> bool {
    !matches!(v.trim().to_lowercase().as_str(), "" | "0" | "off" | "não" | "nao" | "n" | "false" | "falso" | "no")
}

impl DocumentoPdf {
    pub fn formulario_json(&self) -> String {
        let linhas: Vec<String> = valores(&self.campos()).into_iter().map(|(n, v, caixa)| {
            let valor = match caixa { Some(m) => m.to_string(), None => json_texto(&v) };
            format!("  {}: {valor}", json_texto(&n))
        }).collect();
        format!("{{\n{}\n}}\n", linhas.join(",\n"))
    }

    /// XFDF com os valores; `arquivo` é o nome do PDF a que os dados pertencem.
    pub fn formulario_xfdf(&self, arquivo: &str) -> String {
        let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n  <f href=\"{}\"/>\n  <fields>\n", xml(arquivo));
        for (n, v, caixa) in valores(&self.campos()) {
            let v = if caixa == Some(false) || (v.is_empty() && caixa.is_some()) { "Off".to_string() } else { v };
            s += &format!("    <field name=\"{}\"><value>{}</value></field>\n", xml(&n), xml(&v));
        }
        s + "  </fields>\n</xfdf>\n"
    }

    /// Preenche os campos pelo nome. Devolve quantos foram preenchidos e os nomes que não existem.
    pub fn importar_dados(&mut self, pares: &[(String, String)]) -> Result<(usize, Vec<String>), String> {
        let (mut feitos, mut faltam) = (0, Vec::new());
        for (nome, valor) in pares {
            let campos: Vec<Campo> = self.campos().into_iter().filter(|c| c.nome == *nome).collect();
            let Some(primeiro) = campos.first().cloned() else { faltam.push(nome.clone()); continue };
            // Somente leitura não recebe dado de fora (o JSON exportado do mesmo formulário o traz).
            if primeiro.so_leitura { continue; }
            match primeiro.tipo {
                TipoCampo::Assinatura | TipoCampo::Botao => continue,
                TipoCampo::Caixa => self.preencher(&primeiro, if marcado(valor) { "x" } else { "" })?,
                TipoCampo::Opcao => {
                    let alvo = campos.iter().find(|c| c.ligado.eq_ignore_ascii_case(valor.trim()));
                    match alvo {
                        Some(c) => self.preencher(c, "x")?,
                        None if !marcado(valor) => self.preencher(&primeiro, "")?,
                        None => return Err(format!("\"{valor}\" não é uma das opções de {nome}.")),
                    }
                }
                _ => self.preencher(&primeiro, valor)?,
            }
            feitos += 1;
        }
        Ok((feitos, faltam))
    }
}

/// Divide uma linha de CSV (aspas com "" dentro) pelo separador.
fn celulas(linha: &str, sep: char) -> Vec<String> {
    let (mut v, mut atual, mut aspas) = (Vec::new(), String::new(), false);
    let mut letras = linha.chars().peekable();
    while let Some(c) = letras.next() {
        match c {
            '"' if aspas && letras.peek() == Some(&'"') => { atual.push('"'); letras.next(); }
            '"' => aspas = !aspas,
            c if c == sep && !aspas => v.push(std::mem::take(&mut atual)),
            c => atual.push(c),
        }
    }
    v.push(atual);
    v.into_iter().map(|c| c.trim().to_string()).collect()
}

/// CSV para (campo, valor): cabeçalho com os nomes e a primeira linha de dados; ou duas
/// colunas "Campo;Valor", um campo por linha. Aceita `;` (Excel em português) ou `,`.
pub fn ler_csv_formulario(texto: &str) -> Result<Vec<(String, String)>, String> {
    let texto = texto.trim_start_matches('\u{feff}');
    let linhas: Vec<&str> = texto.lines().filter(|l| !l.trim().is_empty()).collect();
    let cab = linhas.first().ok_or("O CSV está vazio.")?;
    let sep = if cab.matches(';').count() >= cab.matches(',').count() { ';' } else { ',' };
    let cabecalho = celulas(cab, sep);
    let vertical = cabecalho.len() == 2 && ["campo", "field"].contains(&cabecalho[0].to_lowercase().as_str())
        && ["valor", "value"].contains(&cabecalho[1].to_lowercase().as_str());
    if vertical {
        return Ok(linhas[1..].iter().map(|l| celulas(l, sep)).filter(|c| !c[0].is_empty())
            .map(|c| (c[0].clone(), c.get(1).cloned().unwrap_or_default())).collect());
    }
    let dados = celulas(linhas.get(1).ok_or("O CSV precisa de uma linha de dados embaixo dos nomes dos campos.")?, sep);
    Ok(cabecalho.into_iter().zip(dados.into_iter().chain(std::iter::repeat(String::new()))).filter(|(n, _)| !n.is_empty()).collect())
}

/// JSON plano {"campo": "valor", "caixa": true, "numero": 3} para (campo, valor).
pub fn ler_json_formulario(texto: &str) -> Result<Vec<(String, String)>, String> {
    let erro = || "JSON inválido: use um objeto simples, como {\"Nome\": \"Ana\", \"Aceito\": true}.".to_string();
    let t: Vec<char> = texto.trim().trim_start_matches('\u{feff}').chars().collect();
    let mut i = 0;
    let pular = |i: &mut usize| while *i < t.len() && t[*i].is_whitespace() { *i += 1 };
    let ler_texto = |i: &mut usize| -> Option<String> {
        if t.get(*i) != Some(&'"') { return None; }
        *i += 1;
        let mut s = String::new();
        while let Some(&c) = t.get(*i) {
            *i += 1;
            match c {
                '"' => return Some(s),
                '\\' => {
                    let e = *t.get(*i)?;
                    *i += 1;
                    match e {
                        'n' => s.push('\n'), 't' => s.push('\t'), 'r' => s.push('\r'), 'b' | 'f' => {}
                        'u' => { let h: String = t.get(*i..*i + 4)?.iter().collect(); *i += 4; s.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?).unwrap_or('?')); }
                        c => s.push(c),
                    }
                }
                c => s.push(c),
            }
        }
        None
    };
    pular(&mut i);
    if t.get(i) != Some(&'{') { return Err(erro()); }
    i += 1;
    let mut pares = Vec::new();
    loop {
        pular(&mut i);
        if t.get(i) == Some(&'}') { break; }
        let chave = ler_texto(&mut i).ok_or_else(erro)?;
        pular(&mut i);
        if t.get(i) != Some(&':') { return Err(erro()); }
        i += 1;
        pular(&mut i);
        let valor = if t.get(i) == Some(&'"') { ler_texto(&mut i).ok_or_else(erro)? } else {
            let ini = i;
            while i < t.len() && !matches!(t[i], ',' | '}') && !t[i].is_whitespace() { i += 1; }
            let v: String = t[ini..i].iter().collect();
            match v.as_str() {
                "true" => "sim".into(), "false" | "null" => String::new(),
                n if n.parse::<f64>().is_ok() => n.to_string(),
                _ => return Err(erro()),
            }
        };
        pares.push((chave, valor));
        pular(&mut i);
        match t.get(i) { Some(',') => i += 1, Some('}') => break, _ => return Err(erro()) }
    }
    Ok(pares)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::objetos::testes::pdf_com;
    use crate::CampoNovo;

    #[test]
    fn importa_e_exporta() {
        let mut pdf = pdf_com("q Q");
        pdf.criar_campo(1, [100.0, 700.0, 300.0, 720.0], &CampoNovo::Texto, "Nome").unwrap();
        pdf.criar_campo(1, [100.0, 650.0, 115.0, 665.0], &CampoNovo::Caixa, "Aceito").unwrap();
        let pares = ler_json_formulario(r#"{ "Nome": "Ana \"A\" Lima", "Aceito": true, "Idade": 30 }"#).unwrap();
        assert_eq!(pares[2], ("Idade".to_string(), "30".to_string()));
        let (n, faltam) = pdf.importar_dados(&pares).unwrap();
        assert_eq!((n, faltam), (2, vec!["Idade".to_string()]));
        let json = pdf.formulario_json();
        assert!(json.contains(r#""Nome": "Ana \"A\" Lima""#) && json.contains(r#""Aceito": true"#), "{json}");
        let xfdf = pdf.formulario_xfdf("ficha.pdf");
        assert!(xfdf.contains(r#"<field name="Nome"><value>Ana &quot;A&quot; Lima</value></field>"#) && xfdf.contains(r#"<f href="ficha.pdf"/>"#), "{xfdf}");
        assert_eq!(ler_json_formulario(&pdf.formulario_json()).unwrap()[0].1, "Ana \"A\" Lima");
        // CSV horizontal (cabeçalho + dados) e vertical (Campo;Valor).
        assert_eq!(ler_csv_formulario("Nome;Aceito\n\"Bia; B\";não\n").unwrap(), [("Nome".into(), "Bia; B".into()), ("Aceito".into(), "não".into())]);
        assert_eq!(ler_csv_formulario("Campo,Valor\nNome,Caio\n").unwrap(), [("Nome".to_string(), "Caio".to_string())]);
        pdf.importar_dados(&ler_csv_formulario("Nome;Aceito\nBia;não").unwrap()).unwrap();
        assert!(pdf.formulario_json().contains(r#""Aceito": false"#));
        assert!(ler_json_formulario("[1, 2]").is_err());
    }
}
