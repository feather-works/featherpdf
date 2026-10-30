//! Regras dos campos: validação com máscara (CPF, CNPJ, CEP, telefone, moeda, e-mail, número),
//! limite de caracteres, campos calculados e a ordem do Tab.
//! Grava do jeito que o Acrobat lê (/AA com JavaScript, /MaxLen, /CO, ordem do /Annots), com uma
//! marca `FeatherPDF:` no script para ler de volta. No FeatherPDF a mesma regra é conferida em
//! Rust ao preencher (inclusive os dígitos verificadores, que o script não confere).

use lopdf::{dictionary, Dictionary, Object, ObjectId};

use crate::{Campo, DocumentoPdf, TipoCampo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regra { Cpf, Cnpj, Cep, Telefone, Moeda, Email, Numero }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCalculo { Soma, Media, Produto, Minimo, Maximo }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Calculo {
    Simples(OpCalculo, Vec<String>),
    /// `Quantidade * [Valor unitário] - Desconto`: nomes simples ou entre colchetes.
    Formula(String),
}

const MARCA: &str = "FeatherPDF:";

fn digitos(v: &str) -> Vec<u32> { v.chars().filter_map(|c| c.to_digit(10)).collect() }

fn juntar(d: &[u32], modelo: &str) -> String {
    let mut i = d.iter();
    modelo.chars().map(|c| if c == '#' { char::from_digit(*i.next().unwrap_or(&0), 10).unwrap_or('0') } else { c }).collect()
}

fn dv_cpf(d: &[u32], n: usize) -> u32 { (d[..n].iter().enumerate().map(|(i, x)| x * (n as u32 + 1 - i as u32)).sum::<u32>() * 10 % 11) % 10 }

fn dv_cnpj(d: &[u32], n: usize) -> u32 {
    let pesos = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let r = d[..n].iter().zip(&pesos[13 - n..]).map(|(x, p)| x * p).sum::<u32>() % 11;
    if r < 2 { 0 } else { 11 - r }
}

/// "1.234,56", "1234.56", "R$ 10" → número.
pub fn ler_numero(v: &str) -> Option<f64> {
    let t: String = v.chars().filter(|c| !c.is_whitespace() && *c != 'R' && *c != '$').collect();
    if t.is_empty() { return None; }
    // "1.234" sem vírgula é milhar (é assim que o próprio cálculo escreve), não 1,234.
    let milhar = |t: &str| { let g: Vec<&str> = t.trim_start_matches('-').split('.').collect();
        g.len() > 1 && (1..=3).contains(&g[0].len()) && g[1..].iter().all(|x| x.len() == 3) && g.iter().all(|x| x.bytes().all(|b| b.is_ascii_digit())) };
    let t = if t.contains(',') || milhar(&t) { t.replace('.', "").replace(',', ".") } else { t };
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// 1234.5 → "1.234,50" (com `casas` decimais).
pub fn escrever_numero(x: f64, casas: usize) -> String {
    let t = format!("{:.*}", casas, x.abs());
    let (inteira, dec) = t.split_once('.').map_or((t.as_str(), ""), |(a, b)| (a, b));
    let mut grupos = String::new();
    for (i, c) in inteira.chars().enumerate() {
        if i > 0 && (inteira.len() - i) % 3 == 0 { grupos.push('.'); }
        grupos.push(c);
    }
    format!("{}{grupos}{}{dec}", if x < 0.0 && x.abs() >= 0.5 * 10f64.powi(-(casas as i32)) { "-" } else { "" }, if dec.is_empty() { "" } else { "," })
}

impl Regra {
    pub const TODAS: [Regra; 7] = [Regra::Cpf, Regra::Cnpj, Regra::Cep, Regra::Telefone, Regra::Moeda, Regra::Email, Regra::Numero];

    pub fn nome(self) -> &'static str {
        match self {
            Regra::Cpf => "CPF", Regra::Cnpj => "CNPJ", Regra::Cep => "CEP", Regra::Telefone => "Telefone",
            Regra::Moeda => "Moeda (R$)", Regra::Email => "E-mail", Regra::Numero => "Número",
        }
    }

    fn chave(self) -> &'static str {
        match self { Regra::Cpf => "cpf", Regra::Cnpj => "cnpj", Regra::Cep => "cep", Regra::Telefone => "telefone", Regra::Moeda => "moeda", Regra::Email => "email", Regra::Numero => "numero" }
    }

    /// Confere o valor e devolve no formato da regra ("12345678909" → "123.456.789-09").
    /// Vazio sempre passa.
    pub fn aplicar(self, v: &str) -> Result<String, String> {
        let v = v.trim();
        if v.is_empty() { return Ok(String::new()); }
        let d = digitos(v);
        match self {
            Regra::Cpf => {
                if d.len() != 11 { return Err("O CPF tem 11 números.".into()); }
                if d.iter().all(|&x| x == d[0]) || dv_cpf(&d, 9) != d[9] || dv_cpf(&d, 10) != d[10] { return Err("CPF inválido (os dígitos verificadores não conferem).".into()); }
                Ok(juntar(&d, "###.###.###-##"))
            }
            Regra::Cnpj => {
                if d.len() != 14 { return Err("O CNPJ tem 14 números.".into()); }
                if d.iter().all(|&x| x == d[0]) || dv_cnpj(&d, 12) != d[12] || dv_cnpj(&d, 13) != d[13] { return Err("CNPJ inválido (os dígitos verificadores não conferem).".into()); }
                Ok(juntar(&d, "##.###.###/####-##"))
            }
            Regra::Cep => if d.len() == 8 { Ok(juntar(&d, "#####-###")) } else { Err("O CEP tem 8 números.".into()) },
            Regra::Telefone => match d.len() {
                10 => Ok(juntar(&d, "(##) ####-####")),
                11 => Ok(juntar(&d, "(##) #####-####")),
                _ => Err("Telefone com DDD: 10 ou 11 números.".into()),
            },
            Regra::Moeda => ler_numero(v).map(|x| format!("R$ {}", escrever_numero(x, 2))).ok_or_else(|| "Escreva um valor, como 1.234,56.".into()),
            Regra::Numero => ler_numero(v).map(|_| v.to_string()).ok_or_else(|| "Escreva um número.".into()),
            Regra::Email => {
                let ok = v.split_once('@').is_some_and(|(a, b)| !a.is_empty() && b.contains('.') && !b.starts_with('.') && !b.ends_with('.') && !b.contains('@')) && !v.contains(' ');
                if ok { Ok(v.to_string()) } else { Err("E-mail inválido.".into()) }
            }
        }
    }

    /// Scripts do Acrobat: (tecla, formato, validação).
    fn scripts(self) -> (Option<&'static str>, Option<&'static str>, String) {
        let mascara = |n: usize, re: &str, troca: &str, msg: &str| format!(
            "var d = event.value.replace(/\\D/g, \"\"); if (event.value != \"\") {{ if (d.length == {n}) event.value = d.replace(/{re}/, \"{troca}\"); else {{ app.alert(\"{msg}\"); event.rc = false; }} }}");
        match self {
            Regra::Cpf => (None, None, mascara(11, "(\\d{3})(\\d{3})(\\d{3})(\\d{2})", "$1.$2.$3-$4", "O CPF tem 11 números.")),
            Regra::Cnpj => (None, None, mascara(14, "(\\d{2})(\\d{3})(\\d{3})(\\d{4})(\\d{2})", "$1.$2.$3/$4-$5", "O CNPJ tem 14 números.")),
            Regra::Cep => (None, None, mascara(8, "(\\d{5})(\\d{3})", "$1-$2", "O CEP tem 8 números.")),
            Regra::Telefone => (None, None, "var d = event.value.replace(/\\D/g, \"\"); if (event.value != \"\") { if (d.length == 10) event.value = d.replace(/(\\d{2})(\\d{4})(\\d{4})/, \"($1) $2-$3\"); else if (d.length == 11) event.value = d.replace(/(\\d{2})(\\d{5})(\\d{4})/, \"($1) $2-$3\"); else { app.alert(\"Telefone com DDD: 10 ou 11 números.\"); event.rc = false; } }".into()),
            Regra::Moeda => (Some("AFNumber_Keystroke(2, 2, 0, 0, \"R$ \", true);"), Some("AFNumber_Format(2, 2, 0, 0, \"R$ \", true);"), String::new()),
            Regra::Numero => (Some("AFNumber_Keystroke(2, 2, 0, 0, \"\", true);"), None, String::new()),
            Regra::Email => (None, None, "if (event.value != \"\" && !/^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$/.test(event.value)) { app.alert(\"E-mail inválido.\"); event.rc = false; }".into()),
        }
    }
}

impl OpCalculo {
    pub const TODAS: [OpCalculo; 5] = [OpCalculo::Soma, OpCalculo::Media, OpCalculo::Produto, OpCalculo::Minimo, OpCalculo::Maximo];
    pub fn nome(self) -> &'static str {
        match self { OpCalculo::Soma => "Soma", OpCalculo::Media => "Média", OpCalculo::Produto => "Produto", OpCalculo::Minimo => "Menor", OpCalculo::Maximo => "Maior" }
    }
    fn af(self) -> &'static str {
        match self { OpCalculo::Soma => "SUM", OpCalculo::Media => "AVG", OpCalculo::Produto => "PRD", OpCalculo::Minimo => "MIN", OpCalculo::Maximo => "MAX" }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Peca { Num(f64), Nome(String), Op(char) }

fn pecas(f: &str) -> Result<Vec<Peca>, String> {
    let mut v = vec![];
    let mut c = f.chars().peekable();
    while let Some(&ch) = c.peek() {
        match ch {
            ' ' | '\t' => { c.next(); }
            '+' | '-' | '*' | '/' | '(' | ')' => { v.push(Peca::Op(ch)); c.next(); }
            '[' => {
                c.next();
                let nome: String = c.by_ref().take_while(|&x| x != ']').collect();
                v.push(Peca::Nome(nome.trim().to_string()));
            }
            '0'..='9' => {
                let mut t = String::new();
                while let Some(&x) = c.peek() { if x.is_ascii_digit() || x == ',' || x == '.' { t.push(x); c.next(); } else { break } }
                v.push(Peca::Num(t.replace(',', ".").parse().map_err(|_| format!("Número estranho: {t}"))?));
            }
            x if x.is_alphanumeric() || x == '_' => {
                let mut t = String::new();
                while let Some(&x) = c.peek() { if x.is_alphanumeric() || x == '_' || x == '.' { t.push(x); c.next(); } else { break } }
                v.push(Peca::Nome(t));
            }
            x => return Err(format!("Sinal que a fórmula não entende: {x}")),
        }
    }
    Ok(v)
}

/// Conta a fórmula, com `valor` dando o número de cada campo pelo nome.
fn contar(p: &[Peca], i: &mut usize, valor: &dyn Fn(&str) -> f64) -> Result<f64, String> {
    fn fator(p: &[Peca], i: &mut usize, valor: &dyn Fn(&str) -> f64) -> Result<f64, String> {
        let atual = p.get(*i).cloned().ok_or("A fórmula termina no meio.")?;
        *i += 1;
        match atual {
            Peca::Num(x) => Ok(x),
            Peca::Nome(n) => Ok(valor(&n)),
            Peca::Op('-') => Ok(-fator(p, i, valor)?),
            Peca::Op('(') => { let x = contar(p, i, valor)?; if p.get(*i) != Some(&Peca::Op(')')) { return Err("Falta fechar o parêntese.".into()); } *i += 1; Ok(x) }
            Peca::Op(o) => Err(format!("Sinal fora do lugar: {o}")),
        }
    }
    fn termo(p: &[Peca], i: &mut usize, valor: &dyn Fn(&str) -> f64) -> Result<f64, String> {
        let mut x = fator(p, i, valor)?;
        while let Some(Peca::Op(o @ ('*' | '/'))) = p.get(*i) { *i += 1; let y = fator(p, i, valor)?; x = if *o == '*' { x * y } else { x / y }; }
        Ok(x)
    }
    let mut x = termo(p, i, valor)?;
    while let Some(Peca::Op(o @ ('+' | '-'))) = p.get(*i) { *i += 1; let y = termo(p, i, valor)?; x = if *o == '+' { x + y } else { x - y }; }
    Ok(x)
}

impl Calculo {
    /// Campos que entram na conta.
    pub fn entradas(&self) -> Vec<String> {
        match self {
            Calculo::Simples(_, v) => v.clone(),
            Calculo::Formula(f) => pecas(f).unwrap_or_default().into_iter().filter_map(|p| if let Peca::Nome(n) = p { Some(n) } else { None }).collect(),
        }
    }

    /// Confere a fórmula sem contar.
    pub fn conferir(&self) -> Result<(), String> {
        if let Calculo::Formula(f) = self {
            let p = pecas(f)?;
            let mut i = 0;
            contar(&p, &mut i, &|_| 1.0)?;
            if i < p.len() { return Err("Sobrou coisa depois do fim da fórmula.".into()); }
        }
        Ok(())
    }

    pub fn resultado(&self, valor: &dyn Fn(&str) -> f64) -> Result<f64, String> {
        match self {
            Calculo::Simples(op, nomes) => {
                let v: Vec<f64> = nomes.iter().map(|n| valor(n)).collect();
                Ok(match op {
                    OpCalculo::Soma => v.iter().sum(),
                    OpCalculo::Media => if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 },
                    OpCalculo::Produto => v.iter().product(),
                    OpCalculo::Minimo => v.iter().copied().fold(f64::INFINITY, f64::min).min(if v.is_empty() { 0.0 } else { f64::INFINITY }),
                    OpCalculo::Maximo => v.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(if v.is_empty() { 0.0 } else { f64::NEG_INFINITY }),
                })
            }
            Calculo::Formula(f) => { let p = pecas(f)?; let mut i = 0; contar(&p, &mut i, valor) }
        }
    }

    fn chave(&self) -> String {
        match self {
            Calculo::Simples(op, v) => format!("{}:{}", op.af(), v.join("|")),
            Calculo::Formula(f) => format!("=:{f}"),
        }
    }

    fn de_chave(t: &str) -> Option<Self> {
        let (a, b) = t.split_once(':')?;
        if a == "=" { return Some(Calculo::Formula(b.into())); }
        let op = OpCalculo::TODAS.into_iter().find(|o| o.af() == a)?;
        Some(Calculo::Simples(op, b.split('|').filter(|x| !x.is_empty()).map(String::from).collect()))
    }

    fn script(&self) -> String {
        let js = |n: &str| format!("\"{}\"", n.replace('\\', "\\\\").replace('"', "\\\""));
        match self {
            Calculo::Simples(op, v) => format!("AFSimple_Calculate(\"{}\", new Array({}));", op.af(), v.iter().map(|n| js(n)).collect::<Vec<_>>().join(", ")),
            Calculo::Formula(f) => {
                let corpo: String = pecas(f).unwrap_or_default().into_iter().map(|p| match p {
                    Peca::Num(x) => x.to_string(),
                    Peca::Nome(n) => format!("AFMakeNumber(this.getField({}).value)", js(&n)),
                    Peca::Op(o) => o.to_string(),
                }).collect::<Vec<_>>().join(" ");
                format!("event.value = {corpo};")
            }
        }
    }
}

fn marca_em(js: &str) -> Option<(&str, &str)> {
    let i = js.find(MARCA)? + MARCA.len();
    let fim = js[i..].find("*/").map_or(js.len(), |f| i + f);
    js[i..fim].trim().split_once('=')
}

/// As regras lidas do /AA e /MaxLen de um campo (widget e o dono do /V).
pub(crate) fn ler_regras(pdf: &DocumentoPdf, dicts: &[&Dictionary]) -> (Option<Regra>, Option<Calculo>, Option<usize>) {
    let mut regra = None;
    let mut calculo = None;
    let mut limite = None;
    for d in dicts {
        limite = limite.or_else(|| d.get(b"MaxLen").and_then(Object::as_i64).ok().map(|n| n.max(0) as usize));
        let Some(aa) = d.get(b"AA").ok().and_then(|o| pdf.dict(o)) else { continue };
        for chave in [&b"V"[..], b"K", b"C"] {
            let Some(js) = aa.get(chave).ok().and_then(|a| pdf.dict(a)).and_then(|a| a.get(b"JS").ok().and_then(|j| pdf.texto_js(j))) else { continue };
            match marca_em(&js) {
                Some(("regra", k)) => regra = regra.or_else(|| Regra::TODAS.into_iter().find(|r| r.chave() == k)),
                Some(("calc", k)) => calculo = calculo.or_else(|| Calculo::de_chave(k)),
                _ => {}
            }
        }
    }
    (regra, calculo, limite)
}

fn acao(js: String) -> Object {
    Object::Dictionary(dictionary! { "S" => "JavaScript", "JS" => Object::string_literal(js) })
}

impl DocumentoPdf {
    fn texto_js(&self, o: &Object) -> Option<String> {
        match o {
            Object::Reference(r) => match self.documento.get_object(*r).ok()? {
                Object::Stream(s) => Some(String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).into_owned()),
                o => o.as_str().ok().map(|b| String::from_utf8_lossy(b).into_owned()),
            },
            o => o.as_str().ok().map(|b| String::from_utf8_lossy(b).into_owned()),
        }
    }

    /// Grava regra, cálculo e limite de caracteres no campo (cada `None` tira o que era nosso).
    pub fn definir_regras(&mut self, c: &Campo, regra: Option<Regra>, calculo: Option<Calculo>, limite: Option<usize>) -> Result<(), String> {
        if !matches!(c.tipo, TipoCampo::Texto { .. }) && (regra.is_some() || calculo.is_some() || limite.is_some()) {
            return Err("Regras, cálculo e limite valem para campo de texto.".into());
        }
        if let Some(k) = &calculo {
            k.conferir()?;
            if k.entradas().contains(&c.nome) { return Err("O campo não pode entrar na própria conta.".into()); }
        }
        let mut aa = self.documento.get_dictionary(c.campo).ok().and_then(|d| d.get(b"AA").ok().cloned()).and_then(|o| self.dict(&o)).unwrap_or_default();
        // Tira só o que nós gravamos; script de outro programa fica se a pessoa não trocou.
        let nosso = |pdf: &Self, aa: &Dictionary, k: &[u8]| aa.get(k).ok().and_then(|a| pdf.dict(a))
            .and_then(|a| a.get(b"JS").ok().and_then(|j| pdf.texto_js(j))).is_some_and(|js| js.contains(MARCA) || js.starts_with("AFNumber_"));
        for k in [&b"K"[..], b"F", b"V"] { if regra.is_some() || nosso(self, &aa, k) { aa.remove(k); } }
        if calculo.is_some() || nosso(self, &aa, b"C") { aa.remove(b"C"); }
        if let Some(r) = regra {
            let (tecla, formato, valida) = r.scripts();
            if let Some(t) = tecla { aa.set("K", acao(format!("/* {MARCA}regra={} */ {t}", r.chave()))); }
            if let Some(f) = formato { aa.set("F", acao(f.to_string())); }
            aa.set("V", acao(format!("/* {MARCA}regra={} */ {valida}", r.chave())));
        }
        if let Some(k) = &calculo { aa.set("C", acao(format!("/* {MARCA}calc={} */ {}", k.chave(), k.script()))); }
        let d = self.documento.get_dictionary_mut(c.campo).map_err(|e| e.to_string())?;
        if aa.is_empty() { d.remove(b"AA"); } else { d.set("AA", aa); }
        match limite { Some(n) if n > 0 => d.set("MaxLen", n as i64), _ => { d.remove(b"MaxLen"); } }
        // /CO: a ordem em que o Acrobat recalcula.
        let mut co: Vec<Object> = self.acroform().and_then(|af| af.get(b"CO").ok().cloned()).and_then(|o| match o {
            Object::Reference(r) => self.documento.get_object(r).ok().and_then(|x| x.as_array().ok().cloned()),
            Object::Array(v) => Some(v),
            _ => None,
        }).unwrap_or_default();
        co.retain(|o| o.as_reference().ok() != Some(c.campo));
        if calculo.is_some() { co.push(Object::Reference(c.campo)); }
        if let Some(af) = self.acroform_mut() { if co.is_empty() { af.remove(b"CO"); } else { af.set("CO", co); } }
        self.modificado = true;
        self.recalcular()?;
        Ok(())
    }

    fn acroform_mut(&mut self) -> Option<&mut Dictionary> {
        let alvo = match self.documento.catalog().ok()?.get(b"AcroForm").ok()? { Object::Reference(r) => Some(*r), _ => None };
        match alvo {
            Some(r) => self.documento.get_dictionary_mut(r).ok(),
            None => self.documento.catalog_mut().ok()?.get_mut(b"AcroForm").ok()?.as_dict_mut().ok(),
        }
    }

    /// Refaz os campos calculados, em cadeia (um total que usa um subtotal), até parar de mudar.
    pub fn recalcular(&mut self) -> Result<usize, String> {
        let mut mudou_total = 0;
        for _ in 0..8 {
            let campos = self.campos();
            let numero = |n: &str| campos.iter().find(|c| c.nome == n).and_then(|c| ler_numero(&c.valor)).unwrap_or(0.0);
            let mut trocas = vec![];
            for c in campos.iter().filter(|c| c.calculo.is_some()) {
                let x = c.calculo.as_ref().unwrap().resultado(&numero)?;
                let texto = if !x.is_finite() { String::new() }
                    else if c.regra == Some(Regra::Moeda) { format!("R$ {}", escrever_numero(x, 2)) }
                    else { let t = escrever_numero(x, 2); t.strip_suffix(",00").map(String::from).unwrap_or(t) };
                if texto != c.valor && !trocas.iter().any(|(k, _): &(Campo, String)| k.campo == c.campo) { trocas.push((c.clone(), texto)); }
            }
            if trocas.is_empty() { break; }
            for (c, t) in trocas { self.gravar_valor(&c, &t)?; mudou_total += 1; }
        }
        Ok(mudou_total)
    }

    /// Os campos de uma página na ordem em que a tecla Tab passa.
    pub fn ordem_tab(&self, pagina: u32) -> Result<Vec<Campo>, String> {
        let id = self.id_pagina(pagina)?;
        let campos = self.campos();
        Ok(self.lista_anotacoes(id).iter().filter_map(|a| a.as_reference().ok())
            .filter_map(|w| campos.iter().find(|c| c.widget == w).cloned()).collect())
    }

    /// Nova ordem do Tab: os widgets na ordem dada, depois as outras anotações da página.
    pub fn definir_ordem_tab(&mut self, pagina: u32, widgets: &[ObjectId]) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let atuais = self.lista_anotacoes(id);
        let mut nova: Vec<Object> = widgets.iter().filter(|w| atuais.iter().any(|a| a.as_reference().ok() == Some(**w))).map(|w| Object::Reference(*w)).collect();
        nova.extend(atuais.into_iter().filter(|a| !a.as_reference().is_ok_and(|r| widgets.contains(&r))));
        let d = self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?;
        d.set("Annots", nova);
        // Sem /Tabs o Acrobat segue a ordem do /Annots; /S (estrutura) ou /R (linhas) passariam por cima.
        d.remove(b"Tabs");
        self.modificado = true;
        Ok(())
    }

    /// Ordem do Tab por linhas: de cima para baixo, e da esquerda para a direita na mesma linha.
    pub fn ordem_tab_por_linhas(&mut self, pagina: u32) -> Result<(), String> {
        let mut campos = self.ordem_tab(pagina)?;
        campos.sort_by(|a, b| {
            let mesma = (a.caixa[3] - b.caixa[3]).abs() < (a.caixa[3] - a.caixa[1]).min(b.caixa[3] - b.caixa[1]) / 2.0;
            if mesma { a.caixa[0].total_cmp(&b.caixa[0]) } else { b.caixa[3].total_cmp(&a.caixa[3]) }
        });
        self.definir_ordem_tab(pagina, &campos.iter().map(|c| c.widget).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::CampoNovo;

    #[test]
    fn mascaras_conferem_digitos() {
        assert_eq!(Regra::Cpf.aplicar("52998224725").unwrap(), "529.982.247-25");
        assert!(Regra::Cpf.aplicar("529.982.247-24").is_err());
        assert!(Regra::Cpf.aplicar("111.111.111-11").is_err());
        assert_eq!(Regra::Cnpj.aplicar("11222333000181").unwrap(), "11.222.333/0001-81");
        assert!(Regra::Cnpj.aplicar("11222333000182").is_err());
        assert_eq!(Regra::Cep.aplicar("01310100").unwrap(), "01310-100");
        assert_eq!(Regra::Telefone.aplicar("11987654321").unwrap(), "(11) 98765-4321");
        assert_eq!(Regra::Moeda.aplicar("1234,5").unwrap(), "R$ 1.234,50");
        assert_eq!(Regra::Moeda.aplicar("1234567.891").unwrap(), "R$ 1.234.567,89");
        assert!(Regra::Email.aplicar("fulano@dominio").is_err());
        assert_eq!(Regra::Cpf.aplicar("").unwrap(), "");
    }

    #[test]
    fn formula_conta_com_precedencia_e_colchetes() {
        let k = Calculo::Formula("Qtd * [Valor unitário] - 2,5 * (1 + 1)".into());
        let v = |n: &str| match n { "Qtd" => 3.0, "Valor unitário" => 10.0, _ => 0.0 };
        assert_eq!(k.resultado(&v).unwrap(), 25.0);
        assert_eq!(k.entradas(), vec!["Qtd", "Valor unitário"]);
        assert!(Calculo::Formula("a + (b".into()).conferir().is_err());
    }

    #[test]
    fn ponto_de_milhar_nao_vira_decimal() {
        assert_eq!(ler_numero("1.234"), Some(1234.0));
        assert_eq!(ler_numero("1.234.567"), Some(1234567.0));
        assert_eq!(ler_numero("-1.500"), Some(-1500.0));
        assert_eq!(ler_numero("1.5"), Some(1.5));
        assert_eq!(ler_numero("1234.56"), Some(1234.56));
        assert_eq!(ler_numero("1.234,56"), Some(1234.56));
        assert_eq!(ler_numero(&escrever_numero(1234.0, 2).replace(",00", "")), Some(1234.0));
    }

    #[test]
    fn total_se_atualiza_e_regras_sobrevivem_ao_salvar() {
        let mut pdf = crate::testes::pdf_teste();
        for (i, nome) in ["Qtd", "Preco", "Total", "Doc"].iter().enumerate() {
            let y = 700.0 - i as f32 * 40.0;
            pdf.criar_campo(1, [100.0, y, 250.0, y + 20.0], &CampoNovo::Texto, nome).unwrap();
        }
        let achar = |pdf: &DocumentoPdf, n: &str| pdf.campos().into_iter().find(|c| c.nome == n).unwrap();
        let total = achar(&pdf, "Total");
        pdf.definir_regras(&total, Some(Regra::Moeda), Some(Calculo::Formula("Qtd * Preco".into())), None).unwrap();
        let doc = achar(&pdf, "Doc");
        pdf.definir_regras(&doc, Some(Regra::Cpf), None, Some(14)).unwrap();
        pdf.preencher(&achar(&pdf, "Qtd"), "3").unwrap();
        pdf.preencher(&achar(&pdf, "Preco"), "12,5").unwrap();
        assert_eq!(achar(&pdf, "Total").valor, "R$ 37,50");
        assert!(pdf.preencher(&achar(&pdf, "Doc"), "123").is_err());
        pdf.preencher(&achar(&pdf, "Doc"), "52998224725").unwrap();
        let relido = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let doc = achar(&relido, "Doc");
        assert_eq!((doc.regra, doc.limite, doc.valor.as_str()), (Some(Regra::Cpf), Some(14), "529.982.247-25"));
        assert_eq!(achar(&relido, "Total").calculo, Some(Calculo::Formula("Qtd * Preco".into())));
        // Ordem do Tab: inverter e voltar por linhas.
        let ids: Vec<ObjectId> = relido.ordem_tab(1).unwrap().iter().rev().map(|c| c.widget).collect();
        let mut pdf = relido;
        pdf.definir_ordem_tab(1, &ids).unwrap();
        assert_eq!(pdf.ordem_tab(1).unwrap()[0].nome, "Doc");
        pdf.ordem_tab_por_linhas(1).unwrap();
        assert_eq!(pdf.ordem_tab(1).unwrap().iter().map(|c| c.nome.as_str()).collect::<Vec<_>>(), ["Qtd", "Preco", "Total", "Doc"]);
    }
}
