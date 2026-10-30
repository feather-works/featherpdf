//! Equações em LaTeX: digitar `\frac{a^2}{\sqrt{x}} \leq \sum_{i=1}^{n} \alpha_i` e ver a
//! equação vetorial na página (Helvetica para letras e números, Symbol para grego e operadores,
//! traços para fração e raiz).
//! shortcut: subconjunto do LaTeX de matemática (sem matrizes, \left/\right que cresce, acentos); ampliar pelo que pedirem.

use feather_draw::{largura_texto, FontePdf};
use lopdf::dictionary;

use crate::inserir::{cm, em_pe, win_ansi};
use crate::DocumentoPdf;

#[derive(Debug, Clone, PartialEq)]
enum No {
    Texto(String, bool),
    Simbolo(u8, u16),
    Espaco(f32),
    Lista(Vec<No>),
    Indice(Box<No>, Option<Box<No>>, Option<Box<No>>),
    Fracao(Box<No>, Box<No>),
    Raiz(Box<No>),
}

/// Comando → (código na fonte Symbol, largura em milésimos).
const SIMBOLOS: [(&str, u8, u16); 37] = [
    ("alpha", 0x61, 631), ("beta", 0x62, 549), ("gamma", 0x67, 411), ("delta", 0x64, 494), ("epsilon", 0x65, 439),
    ("varepsilon", 0x65, 439), ("theta", 0x71, 521), ("lambda", 0x6C, 549), ("mu", 0x6D, 576), ("pi", 0x70, 549),
    ("sigma", 0x73, 603), ("phi", 0x66, 521), ("varphi", 0x6A, 603), ("omega", 0x77, 686), ("rho", 0x72, 549),
    ("tau", 0x74, 439), ("eta", 0x68, 603), ("Gamma", 0x47, 603), ("Delta", 0x44, 612), ("Theta", 0x51, 741),
    ("Lambda", 0x4C, 686), ("Pi", 0x50, 768), ("Sigma", 0x53, 592), ("Phi", 0x46, 763), ("Omega", 0x57, 768),
    ("sum", 0xE5, 713), ("int", 0xF2, 274), ("infty", 0xA5, 713), ("leq", 0xA3, 549), ("geq", 0xB3, 549),
    ("neq", 0xB9, 549), ("times", 0xB4, 549), ("cdot", 0xD7, 250), ("pm", 0xB1, 549), ("approx", 0xBB, 549),
    ("to", 0xAE, 987), ("partial", 0xB6, 494),
];
const RADICAL: (u8, u16) = (0xD6, 549);

struct Leitor<'a> { c: std::iter::Peekable<std::str::Chars<'a>> }

impl Leitor<'_> {
    fn lista(&mut self, ate_chave: bool) -> Result<Vec<No>, String> {
        let mut v: Vec<No> = Vec::new();
        while let Some(&ch) = self.c.peek() {
            match ch {
                '}' if ate_chave => { self.c.next(); return Ok(v); }
                '}' => return Err("Chave “}” sem “{” antes.".into()),
                '{' => { self.c.next(); v.push(No::Lista(self.lista(true)?)); }
                '^' | '_' => {
                    self.c.next();
                    let arg = Box::new(self.argumento()?);
                    let base = v.pop().unwrap_or(No::Lista(vec![]));
                    let (b, mut sup, mut sub) = match base { No::Indice(b, s, i) => (b, s, i), b => (Box::new(b), None, None) };
                    if ch == '^' { sup = Some(arg) } else { sub = Some(arg) }
                    v.push(No::Indice(b, sup, sub));
                }
                ' ' => { self.c.next(); }
                '\\' => { self.c.next(); v.push(self.comando()?); }
                _ => {
                    self.c.next();
                    let letra = ch.is_alphabetic();
                    if win_ansi(ch).is_none() {
                        return Err(format!("“{ch}” não tem desenho: use o comando (ex.: \\alpha).", ));
                    }
                    match v.last_mut() {
                        Some(No::Texto(t, l)) if *l == letra && !letra => t.push(ch),
                        _ => v.push(No::Texto(ch.to_string(), letra)),
                    }
                }
            }
        }
        if ate_chave { return Err("Falta fechar uma chave “}”.".into()); }
        Ok(v)
    }

    /// `{…}`, um comando ou uma letra só (como em `x^2`).
    fn argumento(&mut self) -> Result<No, String> {
        while self.c.peek() == Some(&' ') { self.c.next(); }
        match self.c.next() {
            Some('{') => Ok(No::Lista(self.lista(true)?)),
            Some('\\') => self.comando(),
            Some(ch) if win_ansi(ch).is_some() && ch != '}' => Ok(No::Texto(ch.to_string(), ch.is_alphabetic())),
            _ => Err("Falta o argumento depois de ^, _ ou do comando.".into()),
        }
    }

    fn comando(&mut self) -> Result<No, String> {
        let mut nome = String::new();
        while let Some(&ch) = self.c.peek() { if ch.is_ascii_alphabetic() { nome.push(ch); self.c.next(); } else { break; } }
        if nome.is_empty() {
            // `\,` `\;` `\ ` são espaços; `\{` vira a chave.
            return match self.c.next() {
                Some(',') => Ok(No::Espaco(0.17)), Some(';') => Ok(No::Espaco(0.28)), Some(' ') => Ok(No::Espaco(0.25)),
                Some(c @ ('{' | '}')) => Ok(No::Texto(c.to_string(), false)),
                _ => Err("Barra “\\” sozinha.".into()),
            };
        }
        Ok(match nome.as_str() {
            "frac" => No::Fracao(Box::new(self.argumento()?), Box::new(self.argumento()?)),
            "sqrt" => No::Raiz(Box::new(self.argumento()?)),
            "left" | "right" => No::Lista(vec![]),
            "quad" => No::Espaco(1.0),
            n => match SIMBOLOS.iter().find(|s| s.0 == n) {
                Some(&(_, cod, l)) => No::Simbolo(cod, l),
                None => return Err(format!("Comando \\{n} ainda não é desenhado.")),
            },
        })
    }
}

/// Peça desenhada, com a linha de base em y = 0.
#[derive(Default)]
struct Caixa { largura: f32, sobe: f32, desce: f32, textos: Vec<(f32, f32, u8, f32, Vec<u8>)>, linhas: Vec<[f32; 4]> }

impl Caixa {
    fn por(&mut self, outra: Caixa, dx: f32, dy: f32) {
        self.textos.extend(outra.textos.into_iter().map(|(x, y, f, t, b)| (x + dx, y + dy, f, t, b)));
        self.linhas.extend(outra.linhas.into_iter().map(|l| [l[0] + dx, l[1] + dy, l[2] + dx, l[3]]));
        self.sobe = self.sobe.max(outra.sobe + dy);
        self.desce = self.desce.max(outra.desce - dy);
    }
}

/// Fontes: 0 Helvetica, 1 Helvetica-Oblique (letras), 2 Symbol.
fn montar(no: &No, t: f32) -> Caixa {
    match no {
        No::Texto(s, letra) => {
            let fonte = if *letra { FontePdf::Italico } else { FontePdf::Helvetica };
            Caixa { largura: largura_texto(s, fonte, t), sobe: 0.72 * t, desce: 0.21 * t,
                textos: vec![(0.0, 0.0, u8::from(*letra), t, s.chars().filter_map(win_ansi).collect())], ..Caixa::default() }
        }
        No::Simbolo(cod, l) => {
            // Operadores grandes (soma, integral) um pouco maiores.
            let t2 = if matches!(cod, 0xE5 | 0xF2) { t * 1.3 } else { t };
            Caixa { largura: *l as f32 * t2 / 1000.0 + if matches!(cod, 0xA3 | 0xB3 | 0xB9 | 0xB4 | 0xB1 | 0xBB | 0xAE) { 0.3 * t } else { 0.0 },
                sobe: 0.75 * t2, desce: 0.22 * t2, textos: vec![(0.0, 0.0, 2, t2, vec![*cod])], ..Caixa::default() }
        }
        No::Espaco(em) => Caixa { largura: em * t, ..Caixa::default() },
        No::Lista(v) => {
            let mut c = Caixa { sobe: 0.7 * t, desce: 0.2 * t, ..Caixa::default() };
            for n in v {
                let parte = montar(n, t);
                // Operador de relação ganha respiro dos dois lados.
                let respiro = if let No::Simbolo(cod, _) = n { if matches!(cod, 0xA3 | 0xB3 | 0xB9 | 0xB4 | 0xB1 | 0xBB | 0xAE) { 0.15 * t } else { 0.0 } } else { 0.0 };
                let (x, l) = (c.largura + respiro, parte.largura);
                c.por(parte, x, 0.0);
                c.largura = x + l;
            }
            c
        }
        No::Indice(base, sup, sub) => {
            let mut c = montar(base, t);
            let x = c.largura;
            let mut fim = x;
            if let Some(s) = sup { let p = montar(s, t * 0.7); fim = fim.max(x + p.largura); c.por(p, x + 0.05 * t, 0.42 * t); }
            if let Some(s) = sub { let p = montar(s, t * 0.7); fim = fim.max(x + p.largura); c.por(p, x + 0.05 * t, -0.25 * t); }
            c.largura = fim + 0.1 * t;
            c
        }
        No::Fracao(a, b) => {
            let (num, den) = (montar(a, t * 0.85), montar(b, t * 0.85));
            let l = num.largura.max(den.largura) + 0.3 * t;
            let eixo = 0.27 * t;
            let mut c = Caixa { largura: l, ..Caixa::default() };
            let (dn, dd) = (eixo + 0.15 * t + num.desce, eixo - 0.15 * t - den.sobe);
            let (xn, xd) = ((l - num.largura) / 2.0, (l - den.largura) / 2.0);
            c.por(num, xn, dn);
            c.por(den, xd, dd);
            c.linhas.push([0.0, eixo, l, 0.06 * t]);
            c
        }
        No::Raiz(a) => {
            let dentro = montar(a, t);
            let (cod, lr) = RADICAL;
            let lr = lr as f32 * t / 1000.0;
            let topo = dentro.sobe + 0.12 * t;
            let mut c = Caixa { largura: lr + dentro.largura + 0.1 * t, ..Caixa::default() };
            c.textos.push((0.0, 0.0, 2, t, vec![cod]));
            c.por(dentro, lr, 0.0);
            c.linhas.push([lr * 0.92, topo.max(0.78 * t), lr + c.largura - lr, 0.05 * t]);
            c.sobe = c.sobe.max(topo + 0.05 * t);
            c
        }
    }
}

fn analisar(latex: &str) -> Result<No, String> {
    let v = Leitor { c: latex.trim().trim_matches('$').chars().peekable() }.lista(false)?;
    if v.is_empty() { return Err("Escreva a equação em LaTeX.".into()); }
    Ok(No::Lista(v))
}

impl DocumentoPdf {
    /// Desenha a equação com o canto de cima à esquerda em (x, y) da página. Devolve a caixa (pt).
    pub fn inserir_equacao(&mut self, pagina: u32, x: f32, y: f32, latex: &str, tamanho: f32) -> Result<[f32; 4], String> {
        let c = montar(&analisar(latex)?, tamanho);
        let (_, giro) = self.geometria(pagina)?;
        let (m, _, _) = em_pe([x, y, x, y], giro);
        let fontes = [("Helvetica", true), ("Helvetica-Oblique", true), ("Symbol", false)].map(|(base, ansi)| {
            let mut d = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base };
            if ansi { d.set("Encoding", "WinAnsiEncoding"); }
            self.documento.add_object(d)
        });
        let nomes: Vec<String> = fontes.iter().map(|f| format!("FPe{}", f.0)).collect();
        let mut s = format!("q {} 0 0 0 rg 0 0 0 RG 0 {:.3} cm\n", cm([m[0], m[1], m[2], m[3], x, y]), -c.sobe);
        for [x0, y0, x1, w] in &c.linhas { s += &format!("{w:.3} w {x0:.3} {y0:.3} m {x1:.3} {y0:.3} l S\n"); }
        for (tx, ty, f, t, bytes) in &c.textos {
            let hexa: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
            s += &format!("BT /{} {t:.2} Tf {tx:.3} {ty:.3} Td <{hexa}> Tj ET\n", nomes[*f as usize]);
        }
        s += "Q";
        let recursos: Vec<(&str, String, lopdf::ObjectId)> = fontes.iter().zip(&nomes).map(|(id, n)| ("Font", n.clone(), *id)).collect();
        self.acrescentar(pagina, s, &recursos)?;
        Ok([x, y - c.sobe - c.desce, x + c.largura, y])
    }
}

#[cfg(test)]
mod testes {

    #[test]
    fn equacao_vira_desenho() {
        let mut pdf = crate::testes::pdf_teste();
        let caixa = pdf.inserir_equacao(1, 72.0, 700.0, r"\frac{a^2+b}{\sqrt{x}} \leq \sum_{i=1}^{n} \alpha_i", 14.0).unwrap();
        assert!(caixa[2] - caixa[0] > 60.0 && caixa[3] - caixa[1] > 20.0, "{caixa:?}");
        let objetos = pdf.objetos(1).unwrap();
        assert!(objetos.iter().filter(|o| matches!(o.tipo, crate::TipoObjeto::Texto { .. })).count() >= 6, "{objetos:?}");
        assert!(objetos.iter().any(|o| matches!(o.tipo, crate::TipoObjeto::Desenho)), "traço da fração");
        let bytes = pdf.serializar().unwrap();
        assert!(crate::DocumentoPdf::de_bytes(&bytes, None).is_ok());
        assert!(pdf.inserir_equacao(1, 72.0, 600.0, r"\matrix{1}", 12.0).unwrap_err().contains("matrix"));
        assert!(pdf.inserir_equacao(1, 72.0, 600.0, r"\frac{1}{2", 12.0).is_err());
    }
}
