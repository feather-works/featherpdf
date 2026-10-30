//! Converter → planilha: as tabelas do PDF, uma a uma (linhas com várias células e
//! colunas alinhadas), com o título logo acima, o tipo provável de cada coluna e a
//! continuação na página seguinte. O `tabela.rs` é o outro caminho: a página inteira
//! como grade, do jeito que está.

use crate::tabela::linhas_de_texto;
use crate::DocumentoPdf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoColuna { Texto, Data, Numero, Moeda }

impl TipoColuna {
    pub const TODOS: [TipoColuna; 4] = [TipoColuna::Texto, TipoColuna::Data, TipoColuna::Numero, TipoColuna::Moeda];
}

impl std::fmt::Display for TipoColuna {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self { TipoColuna::Texto => "Texto", TipoColuna::Data => "Data", TipoColuna::Numero => "Número (1,5)", TipoColuna::Moeda => "Moeda (R$)" })
    }
}

/// O valor de uma célula lido no tipo da coluna.
#[derive(Debug, Clone, PartialEq)]
pub enum Valor { Texto(String), Numero(f64), Data(i32, u32, u32), Moeda(f64) }

#[derive(Debug, Clone, PartialEq)]
pub struct TabelaPdf {
    pub pagina: u32,
    /// Última página (maior que `pagina` quando a tabela continua na seguinte).
    pub ate_pagina: u32,
    pub titulo: String,
    /// Caixa na primeira página (pontos do PDF, y para cima).
    pub caixa: [f32; 4],
    pub cabecalho: Vec<String>,
    pub linhas: Vec<Vec<String>>,
    pub tipos: Vec<TipoColuna>,
}

/// Número no jeito brasileiro ("1.240,50", "512", "-3,2"); aceita "1240.5" quando não há vírgula.
fn numero(t: &str) -> Option<f64> {
    let t = t.trim().replace(['\u{a0}', ' '], "");
    if t.is_empty() || !t.chars().any(|c| c.is_ascii_digit()) { return None; }
    let limpo = if t.contains(',') { t.replace('.', "").replace(',', ".") }
        else if t.matches('.').count() > 1 || t.split('.').nth(1).is_some_and(|d| d.len() == 3) { t.replace('.', "") }
        else { t };
    limpo.parse().ok()
}

fn data(t: &str) -> Option<(i32, u32, u32)> {
    let t = t.trim();
    let partes: Vec<&str> = t.split(['/', '-', '.']).collect();
    if partes.len() != 3 { return None; }
    let n: Vec<i64> = partes.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let (a, m, d) = if partes[0].len() == 4 { (n[0], n[1], n[2]) } else { (if n[2] < 100 { n[2] + 2000 } else { n[2] }, n[1], n[0]) };
    ((1..=12).contains(&m) && (1..=31).contains(&d) && (1900..=2200).contains(&a)).then_some((a as i32, m as u32, d as u32))
}

/// Lê o texto no tipo; `None` quando não serve (a célula fica como texto e marcada).
pub fn ler_valor(t: &str, tipo: TipoColuna) -> Option<Valor> {
    match tipo {
        TipoColuna::Texto => Some(Valor::Texto(t.to_string())),
        TipoColuna::Data => data(t).map(|(a, m, d)| Valor::Data(a, m, d)),
        TipoColuna::Numero => numero(t).map(Valor::Numero),
        TipoColuna::Moeda => {
            let sem = t.trim().trim_start_matches("R$").trim_start_matches("US$").trim_start_matches(['$', '€']);
            numero(sem).map(Valor::Moeda)
        }
    }
}

/// O tipo em que pelo menos 80% das células cheias da coluna se leem.
pub fn tipo_provavel(valores: &[&str]) -> TipoColuna {
    let cheios: Vec<&str> = valores.iter().copied().filter(|v| !v.trim().is_empty()).collect();
    if cheios.is_empty() { return TipoColuna::Texto; }
    let serve = |tipo| cheios.iter().filter(|v| ler_valor(v, tipo).is_some()).count() * 5 >= cheios.len() * 4;
    if cheios.iter().any(|v| v.trim().starts_with("R$")) && serve(TipoColuna::Moeda) { return TipoColuna::Moeda; }
    [TipoColuna::Data, TipoColuna::Numero].into_iter().find(|&t| serve(t)).unwrap_or(TipoColuna::Texto)
}

/// As tabelas de uma página a partir do texto (texto, caixa em pontos com y para cima).
/// Serve tanto para o texto do PDF quanto para as palavras lidas pelo OCR.
pub fn tabelas_da_pagina(pagina: u32, itens: &[(String, [f32; 4])]) -> Vec<TabelaPdf> {
    let caixas: Vec<[f32; 4]> = itens.iter().map(|i| i.1).collect();
    // Cada linha de texto em células: vão maior que a altura da letra separa duas células.
    let linhas: Vec<(f32, f32, Vec<(f32, f32, String)>)> = linhas_de_texto(&caixas).into_iter().map(|(y0, y1, mut v)| {
        v.sort_by(|&a, &b| caixas[a][0].total_cmp(&caixas[b][0]));
        let altura = (y1 - y0).max(4.0);
        let mut celulas: Vec<(f32, f32, String)> = Vec::new();
        for k in v {
            let [x0, _, x1, _] = caixas[k];
            match celulas.last_mut() {
                Some((_, fim, t)) if x0 - *fim < altura * 1.2 => { *fim = fim.max(x1); t.push(' '); t.push_str(itens[k].0.trim()); }
                _ => celulas.push((x0, x1, itens[k].0.trim().to_string())),
            }
        }
        (y0, y1, celulas)
    }).collect();
    let mut saida = Vec::new();
    let mut i = 0;
    while i < linhas.len() {
        if linhas[i].2.len() < 2 { i += 1; continue; }
        // Bloco: linhas seguidas de várias células, sem vão grande entre elas.
        let mut j = i + 1;
        while j < linhas.len() && linhas[j].2.len() >= 2 && linhas[j - 1].0 - linhas[j].1 < (linhas[j - 1].1 - linhas[j - 1].0).max(4.0) * 1.8 { j += 1; }
        if j - i >= 2 {
            if let Some(t) = montar(pagina, &linhas[i..j], i.checked_sub(1).map(|k| &linhas[k])) { saida.push(t); }
        }
        i = j;
    }
    saida
}

type LinhaCelulas = (f32, f32, Vec<(f32, f32, String)>);

fn montar(pagina: u32, bloco: &[LinhaCelulas], acima: Option<&LinhaCelulas>) -> Option<TabelaPdf> {
    // Colunas: os intervalos das células que se cobrem viram uma faixa só.
    let mut faixas: Vec<(f32, f32)> = bloco.iter().flat_map(|l| l.2.iter().map(|c| (c.0, c.1))).collect();
    faixas.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut colunas: Vec<(f32, f32)> = Vec::new();
    for (a, b) in faixas {
        match colunas.last_mut() {
            Some((_, fim)) if a <= *fim + 2.0 => *fim = fim.max(b),
            _ => colunas.push((a, b)),
        }
    }
    if colunas.len() < 2 { return None; }
    let grade: Vec<Vec<String>> = bloco.iter().map(|l| {
        let mut linha = vec![String::new(); colunas.len()];
        for (x0, x1, t) in &l.2 {
            let meio = (x0 + x1) / 2.0;
            let c = colunas.iter().position(|&(a, b)| meio >= a - 1.0 && meio <= b + 1.0).unwrap_or(0);
            if !linha[c].is_empty() { linha[c].push(' '); }
            linha[c].push_str(t);
        }
        linha
    }).collect();
    let tipos_de = |linhas: &[Vec<String>]| (0..colunas.len()).map(|c| tipo_provavel(&linhas.iter().map(|l| l[c].as_str()).collect::<Vec<_>>())).collect::<Vec<_>>();
    // A primeira linha é cabeçalho, a não ser que ela mesma se leia nos tipos das colunas.
    let tipos = tipos_de(&grade[1..]);
    let primeira_e_dado = tipos.iter().any(|&t| t != TipoColuna::Texto)
        && tipos.iter().zip(&grade[0]).all(|(&t, v)| t == TipoColuna::Texto || ler_valor(v, t).is_some());
    let (cabecalho, linhas) = if primeira_e_dado {
        ((0..colunas.len()).map(|c| format!("Coluna {}", c + 1)).collect(), grade.clone())
    } else { (grade[0].clone(), grade[1..].to_vec()) };
    let tipos = tipos_de(&linhas);
    let altura = bloco[0].1 - bloco[0].0;
    let titulo = acima.filter(|l| l.0 - bloco[0].1 < altura.max(4.0) * 2.5 && l.2.len() == 1).map(|l| l.2[0].2.clone()).unwrap_or_default();
    let caixa = [colunas[0].0, bloco.last()?.0, colunas.last()?.1, bloco[0].1];
    Some(TabelaPdf { pagina, ate_pagina: pagina, titulo, caixa, cabecalho, linhas, tipos })
}

/// Junta a tabela que continua na página seguinte: última de uma página e primeira da
/// próxima, com as mesmas colunas (início de cada uma a até 12 pt). O cabeçalho repetido sai.
pub fn juntar_continuacoes(tabelas: Vec<TabelaPdf>) -> Vec<TabelaPdf> {
    let mut saida: Vec<TabelaPdf> = Vec::new();
    for t in tabelas {
        let continua = saida.last().is_some_and(|a: &TabelaPdf| {
            a.ate_pagina + 1 == t.pagina && a.cabecalho.len() == t.cabecalho.len() && (a.caixa[0] - t.caixa[0]).abs() < 12.0
        }) && !saida.iter().any(|x| x.pagina == t.pagina);
        if !continua { saida.push(t); continue; }
        let a = saida.last_mut().unwrap();
        a.ate_pagina = t.pagina;
        // Sem cabeçalho repetido, a "primeira linha" da continuação é dado.
        if t.cabecalho != a.cabecalho && !t.cabecalho.iter().all(|c| c.starts_with("Coluna ")) { a.linhas.push(t.cabecalho); }
        a.linhas.extend(t.linhas);
        a.tipos = (0..a.cabecalho.len()).map(|c| tipo_provavel(&a.linhas.iter().map(|l| l[c].as_str()).collect::<Vec<_>>())).collect();
    }
    saida
}

impl DocumentoPdf {
    /// As tabelas de todas as páginas com texto.
    pub fn tabelas(&self, juntar: bool) -> Result<Vec<TabelaPdf>, String> {
        let mut v = Vec::new();
        for p in 1..=self.total_paginas() {
            let (itens, _, _) = self.conteudo(p)?;
            let itens: Vec<(String, [f32; 4])> = itens.into_iter().map(|i| (i.texto, i.caixa)).collect();
            v.extend(tabelas_da_pagina(p, &itens));
        }
        Ok(if juntar { juntar_continuacoes(v) } else { v })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn linha(y: f32, celulas: &[(&str, f32)]) -> Vec<(String, [f32; 4])> {
        celulas.iter().map(|&(t, x)| (t.to_string(), [x, y, x + 5.0 * t.len() as f32, y + 10.0])).collect()
    }

    fn medicoes(pagina_y: f32, com_titulo: bool) -> Vec<(String, [f32; 4])> {
        let mut v = Vec::new();
        if com_titulo { v.extend(linha(pagina_y + 30.0, &[("Medições de isolação", 60.0)])); }
        v.extend(linha(pagina_y, &[("Equipamento", 60.0), ("Data", 200.0), ("Isolação", 300.0), ("Custo", 420.0)]));
        v.extend(linha(pagina_y - 14.0, &[("Motor M-12", 60.0), ("02/10/2026", 200.0), ("512", 300.0), ("R$ 1.240,00", 420.0)]));
        v.extend(linha(pagina_y - 28.0, &[("Trafo T-2", 60.0), ("03/10/2026", 200.0), ("1,2 k?", 300.0), ("R$ 3.100,00", 420.0)]));
        v.extend(linha(pagina_y - 42.0, &[("Painel P-1", 60.0), ("04/10/2026", 200.0), ("890", 300.0), ("R$ 450,00", 420.0)]));
        v
    }

    #[test]
    fn acha_a_tabela_com_titulo_e_tipos() {
        let t = tabelas_da_pagina(3, &medicoes(500.0, true));
        assert_eq!(t.len(), 1, "{t:?}");
        let t = &t[0];
        assert_eq!(t.titulo, "Medições de isolação");
        assert_eq!(t.cabecalho, ["Equipamento", "Data", "Isolação", "Custo"]);
        assert_eq!(t.linhas.len(), 3);
        // "1,2 k?" não lê como número, mas 2 de 3 também não chegam a 80%: a coluna fica Texto.
        assert_eq!(t.tipos, [TipoColuna::Texto, TipoColuna::Data, TipoColuna::Texto, TipoColuna::Moeda]);
        assert_eq!(ler_valor("R$ 1.240,00", TipoColuna::Moeda), Some(Valor::Moeda(1240.0)));
        assert_eq!(ler_valor("1,2 k?", TipoColuna::Numero), None);
        assert_eq!(ler_valor("02/10/2026", TipoColuna::Data), Some(Valor::Data(2026, 10, 2)));
    }

    #[test]
    fn tabela_que_continua_na_pagina_seguinte_vira_uma() {
        let a = tabelas_da_pagina(9, &medicoes(120.0, false));
        let b = tabelas_da_pagina(10, &medicoes(780.0, false));
        let juntas = juntar_continuacoes(a.into_iter().chain(b).collect());
        assert_eq!(juntas.len(), 1);
        assert_eq!((juntas[0].pagina, juntas[0].ate_pagina, juntas[0].linhas.len()), (9, 10, 6));
    }
}
