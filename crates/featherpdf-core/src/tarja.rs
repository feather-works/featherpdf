//! Tarja definitiva: apaga as letras, os pixels e as anotações debaixo das
//! marcas e pinta a caixa de preto — não é um retângulo por cima. Para achar o
//! que tarjar: busca de texto e padrões (CPF, CNPJ, e-mail, telefone, CEP).

use std::collections::BTreeMap;

use lopdf::content::Operation;
use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use crate::objetos::{achar_sem_caixa, aplicar, inversa, unir, ItemTj, TipoObjeto};
use crate::otimizar::{ler_imagem, tirar_anexos, trocar_por_jpeg};
use crate::{DocumentoPdf, ImagemCrua};

/// Pinta de preto os retângulos (x0, y0, x1, y1 em pixels, origem no alto à
/// esquerda) e devolve o JPEG; `None` quando não sabe decodificar.
pub type ApagarPixels<'a> = &'a dyn Fn(ImagemCrua<'_>, &[[u32; 4]]) -> Option<(Vec<u8>, u32, u32)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Padrao { Cpf, Cnpj, Email, Telefone, Cep, Nome }

impl Padrao {
    pub const TODOS: [Padrao; 6] = [Padrao::Cpf, Padrao::Cnpj, Padrao::Email, Padrao::Telefone, Padrao::Cep, Padrao::Nome];
    pub fn nome(self) -> &'static str {
        match self { Padrao::Cpf => "CPF", Padrao::Cnpj => "CNPJ", Padrao::Email => "E-mail", Padrao::Telefone => "Telefone", Padrao::Cep => "CEP", Padrao::Nome => "Nome" }
    }
    fn mascaras(self) -> &'static [&'static str] {
        match self {
            Padrao::Cpf => &["###.###.###-##"],
            Padrao::Cnpj => &["##.###.###/####-##"],
            Padrao::Cep => &["#####-###", "##.###-###"],
            Padrao::Telefone => &["(##) #####-####", "(##) ####-####", "(##)#####-####", "(##)####-####", "## #####-####"],
            Padrao::Email | Padrao::Nome => &[],
        }
    }
}

/// Trecho achado para tarjar.
#[derive(Debug, Clone, PartialEq)]
pub struct Achado { pub pagina: u32, pub texto: String, pub caixa: [f32; 4], pub tipo: Option<Padrao> }

/// O centro de `c` cai dentro de `r`.
fn centro_dentro(c: [f32; 4], r: [f32; 4]) -> bool {
    let (x, y) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
    x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]
}

fn tirar_texto_alt(d: &mut Dictionary) { for k in [&b"ActualText"[..], b"Alt", b"E"] { d.remove(k); } }

fn cruza(a: [f32; 4], b: [f32; 4]) -> bool { a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] }

fn contem(r: [f32; 4], c: [f32; 4]) -> bool { c[0] >= r[0] && c[1] >= r[1] && c[2] <= r[2] && c[3] <= r[3] }

/// Tamanho do trecho que casa com a máscara em `i` (`#` é algarismo).
fn casa(t: &[char], i: usize, mascara: &str) -> Option<usize> {
    let n = mascara.chars().count();
    let pedaco = t.get(i..i + n)?;
    pedaco.iter().zip(mascara.chars()).all(|(&c, m)| if m == '#' { c.is_ascii_digit() } else { c == m }).then_some(n)
}

/// Trechos `(início, fim)` em caracteres que casam com o padrão, sem pegar
/// pedaço de número maior.
fn achar_padrao(t: &[char], p: Padrao) -> Vec<(usize, usize)> {
    if p == Padrao::Nome { return achar_nomes(t); }
    let mut achados = Vec::new();
    if p == Padrao::Email {
        let local = |c: char| c.is_alphanumeric() || "._%+-".contains(c);
        let dominio = |c: char| c.is_alphanumeric() || ".-".contains(c);
        for (i, _) in t.iter().enumerate().filter(|(_, c)| **c == '@') {
            let ini = (0..i).rev().take_while(|&k| local(t[k])).last();
            let mut fim = i + 1 + t[i + 1..].iter().take_while(|&&c| dominio(c)).count();
            while fim > i + 1 && t[fim - 1] == '.' { fim -= 1; }
            if let (Some(ini), true) = (ini, t[i + 1..fim].contains(&'.')) { achados.push((ini, fim)); }
        }
        return achados;
    }
    let mut i = 0;
    while i < t.len() {
        let antes_ok = i == 0 || !t[i - 1].is_ascii_digit();
        let achou = antes_ok.then(|| p.mascaras().iter().filter_map(|m| casa(t, i, m)).max()).flatten()
            .filter(|n| t.get(i + n).is_none_or(|c| !c.is_ascii_digit()));
        match achou { Some(n) => { achados.push((i, i + n)); i += n; } None => i += 1 }
    }
    achados
}

/// Palavras que, logo antes, dizem que vem um nome de pessoa ("Nome:", "Paciente:", "Sr.").
const DEIXAS: &[&str] = &[
    "nome", "sr", "sra", "dr", "dra", "cliente", "paciente", "requerente", "requerido", "requerida", "autor", "autora",
    "réu", "ré", "titular", "responsável", "por", "contratante", "contratado", "contratada", "locador", "locadora",
    "locatário", "locatária", "comprador", "compradora", "vendedor", "vendedora", "beneficiário", "beneficiária",
    "segurado", "segurada", "aluno", "aluna", "funcionário", "funcionária", "empregado", "empregada", "testemunha",
    "outorgante", "outorgado", "outorgada", "interessado", "interessada", "favorecido", "favorecida", "emitente",
    "destinatário", "destinatária", "mãe", "pai", "cônjuge", "filho", "filha", "assinante", "procurador", "procuradora",
];
const LIGACOES: &[&str] = &["da", "de", "do", "das", "dos", "e", "d'"];
/// Sem deixa antes, uma sequência com uma destas palavras é instituição ou lugar, não pessoa.
const NAO_PESSOA: &[&str] = &[
    "rua", "avenida", "travessa", "praça", "rodovia", "secretaria", "ministério", "tribunal", "banco", "universidade",
    "prefeitura", "estado", "governo", "departamento", "instituto", "faculdade", "escola", "hospital", "empresa",
    "comarca", "vara", "justiça", "receita", "federal", "nacional", "câmara", "conselho", "associação", "sociedade",
    "fundação", "companhia", "superior", "município", "república", "cartório", "ltda", "s/a", "programa", "centro",
];

/// Nomes de pessoa por regra (sem modelo de linguagem): depois de uma deixa ("Nome:", "Paciente:"),
/// duas ou mais palavras com inicial maiúscula (ou todas maiúsculas); sem deixa, só a forma
/// "Maria da Silva Santos" (três ou mais palavras com uma ligação) que não pareça instituição.
fn achar_nomes(t: &[char]) -> Vec<(usize, usize)> {
    // Palavras: (início, fim) de letras e apóstrofo.
    let mut palavras: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        if t[i].is_alphabetic() {
            let ini = i;
            while i < t.len() && (t[i].is_alphabetic() || t[i] == '\'') { i += 1; }
            palavras.push((ini, i));
        } else { i += 1; }
    }
    let texto = |(a, b): (usize, usize)| t[a..b].iter().collect::<String>();
    let maiuscula = |w: &str| {
        let mut c = w.chars();
        let primeira = c.next().is_some_and(char::is_uppercase);
        let resto: String = c.collect();
        primeira && w.chars().count() >= 2 && (resto.chars().all(char::is_lowercase) || resto.chars().all(|c| c.is_uppercase() || c == '\''))
    };
    // Entre duas palavras do mesmo nome só pode haver espaço.
    let juntas = |a: (usize, usize), b: (usize, usize)| t[a.1..b.0].iter().all(|c| *c == ' ');
    let mut achados = Vec::new();
    let mut k = 0;
    while k < palavras.len() {
        if !maiuscula(&texto(palavras[k])) { k += 1; continue; }
        let mut fim = k;
        let (mut nomes, mut ligacao, mut j) = (1, false, k + 1);
        while j < palavras.len() && juntas(palavras[j - 1], palavras[j]) {
            let w = texto(palavras[j]);
            if maiuscula(&w) && !LIGACOES.contains(&w.to_lowercase().as_str()) { nomes += 1; fim = j; }
            else if LIGACOES.contains(&w.to_lowercase().as_str()) { ligacao = true; }
            else { break; }
            j += 1;
        }
        let deixa = k > 0 && {
            let ant = texto(palavras[k - 1]).to_lowercase();
            let entre: String = t[palavras[k - 1].1..palavras[k].0].iter().collect();
            DEIXAS.contains(&ant.as_str()) && entre.trim().chars().all(|c| ":.-–".contains(c))
        };
        let palavras_do_nome: Vec<String> = palavras[k..=fim].iter().map(|&p| texto(p).to_lowercase()).collect();
        let sozinho = nomes >= 3 && ligacao && !palavras_do_nome.iter().any(|w| NAO_PESSOA.contains(&w.as_str()));
        if nomes >= 2 && (deixa || sozinho) { achados.push((palavras[k].0, palavras[fim].1)); }
        k = fim + 1;
    }
    achados
}

impl DocumentoPdf {
    /// Trechos com `busca` (sem diferenciar maiúscula) e com os `padroes`, em todas as páginas.
    pub fn achar_para_tarja(&self, busca: &str, padroes: &[Padrao]) -> Vec<Achado> {
        let mut achados = Vec::new();
        for pagina in 1..=self.total_paginas() {
            let Ok(a) = self.analisar_com(pagina, true) else { continue };
            for lugar in a.lugares.iter().filter(|l| !l.escritas.is_empty()) {
                // A linha letra a letra, com a caixa de cada uma (o espaço do recuo não tem caixa).
                let (mut t, mut caixas): (Vec<char>, Vec<Option<[f32; 4]>>) = (Vec::new(), Vec::new());
                for item in lugar.escritas.iter().filter_map(|i| a.letras.get(i)).flatten() {
                    match item {
                        ItemTj::Glifo { texto, caixa, .. } => for c in texto.chars() { t.push(c); caixas.push(Some(*caixa)); },
                        ItemTj::Recuo(n) if n.as_float().unwrap_or(0.0) < -180.0 && t.last() != Some(&' ') => { t.push(' '); caixas.push(None); }
                        ItemTj::Recuo(_) => {}
                    }
                }
                let mut trechos: Vec<(usize, usize, Option<Padrao>)> = Vec::new();
                if !busca.trim().is_empty() {
                    let n = busca.chars().count();
                    trechos.extend(achar_sem_caixa(&t.iter().collect::<String>(), busca).into_iter().map(|i| (i, i + n, None)));
                }
                for &p in padroes { trechos.extend(achar_padrao(&t, p).into_iter().map(|(i, f)| (i, f, Some(p)))); }
                for (i, f, tipo) in trechos {
                    let Some(caixa) = caixas[i..f].iter().flatten().fold(None, |acc, c| Some(unir(acc, *c))) else { continue };
                    achados.push(Achado { pagina, texto: t[i..f].iter().collect(), caixa, tipo });
                }
            }
        }
        achados
    }

    /// Aplica as tarjas (página, caixa): some com as letras cujo centro cai
    /// na caixa, pinta de preto os pixels das imagens debaixo (ou tira a
    /// imagem, quando não sabe decodificar), tira grupos e anotações que
    /// tocam a caixa e desenhos inteiros dentro dela. `rotulo` vai em branco
    /// sobre a tarja. Devolve quantas letras saíram.
    pub fn tarjar(&mut self, marcas: &[(u32, [f32; 4])], rotulo: &str, apagar: ApagarPixels<'_>) -> Result<usize, String> {
        let mut por_pagina: BTreeMap<u32, Vec<[f32; 4]>> = BTreeMap::new();
        for &(p, c) in marcas { por_pagina.entry(p).or_default().push(c); }
        let mut letras = 0;
        for (&pagina, caixas) in &por_pagina {
            let id = self.id_pagina(pagina)?;
            let mut a = self.analisar_com(pagina, true)?;
            for (&i, itens) in &a.letras {
                let some = |c: &[f32; 4]| caixas.iter().any(|r| centro_dentro(*c, *r));
                if !itens.iter().any(|it| matches!(it, ItemTj::Glifo { caixa, .. } if some(caixa))) { continue; }
                let mut arr = Vec::new();
                let mut atual: Vec<u8> = Vec::new();
                let soltar = |atual: &mut Vec<u8>, arr: &mut Vec<Object>| if !atual.is_empty() {
                    arr.push(Object::String(std::mem::take(atual), lopdf::StringFormat::Hexadecimal));
                };
                for it in itens {
                    match it {
                        ItemTj::Glifo { bytes, caixa, k, .. } if some(caixa) => { soltar(&mut atual, &mut arr); arr.push(Object::Real(*k)); letras += 1; }
                        ItemTj::Glifo { bytes, .. } => atual.extend_from_slice(bytes),
                        ItemTj::Recuo(o) => { soltar(&mut atual, &mut arr); arr.push(o.clone()); }
                    }
                }
                soltar(&mut atual, &mut arr);
                a.ops[i] = Operation::new("TJ", vec![Object::Array(arr)]);
            }
            let (_, xobjetos) = self.recursos(id);
            let nome_em = |op: &Operation| op.operands.first().and_then(|n| n.as_name().ok()).map(<[u8]>::to_vec);
            let mut remover: Vec<usize> = Vec::new();
            let mut pixels: Vec<(ObjectId, usize, Vec<[f32; 4]>, Vec<u8>)> = Vec::new();
            // Nome de XObject cujo `Do` saiu: sai também dos recursos, senão o fluxo fica no arquivo.
            let mut sem_uso: Vec<Vec<u8>> = Vec::new();
            for (k, o) in a.objetos.iter().enumerate() {
                let toca: Vec<[f32; 4]> = caixas.iter().copied().filter(|c| cruza(o.caixa, *c)).collect();
                if toca.is_empty() { continue; }
                let lugar = &a.lugares[k];
                let nome = nome_em(&a.ops[lugar.insercao]).filter(|_| a.ops[lugar.insercao].operator == "Do");
                match o.tipo {
                    TipoObjeto::Imagem { .. } => match nome.as_ref().and_then(|n| xobjetos.get(n)).copied() {
                        Some(x) => pixels.push((x, k, toca, nome.unwrap_or_default())),
                        // Imagem embutida (BI…EI): sai do conteúdo.
                        None => remover.extend(lugar.insercao..lugar.fim),
                    },
                    // shortcut: o grupo (Form XObject) sai inteiro, mesmo o pedaço fora da caixa; ler o conteúdo dele quando houver carimbo grande tarjado.
                    TipoObjeto::Grupo => { remover.extend(lugar.insercao..lugar.fim); sem_uso.extend(nome); }
                    TipoObjeto::Desenho if toca.iter().any(|c| contem(*c, o.caixa)) => remover.extend(lugar.insercao..lugar.fim),
                    _ => {}
                }
            }
            let mut preta = None;
            for (x, k, toca, nome) in pixels {
                let lugar = &a.lugares[k];
                if self.apagar_pixels(x, lugar.matriz, &toca, apagar) { continue; }
                // Sem decodificar, a imagem inteira vira um pixel preto: a página digitalizada não some da tela.
                let p = *preta.get_or_insert_with(|| self.documento.add_object(Stream::new(dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                }, vec![0])));
                self.trocar_xobjeto(id, &nome, Some(p));
            }
            remover.sort_unstable_by(|a, b| b.cmp(a));
            remover.dedup();
            for i in remover { a.ops.remove(i); }
            for nome in sem_uso {
                // O mesmo nome desenhado fora da caixa continua valendo.
                if !a.ops.iter().any(|op| op.operator == "Do" && nome_em(op).as_ref() == Some(&nome)) { self.trocar_xobjeto(id, &nome, None); }
            }
            // ActualText, Alt e E guardam o texto tarjado em claro: no próprio BDC ou, por nome, em /Properties.
            let mut nomes: Vec<Vec<u8>> = Vec::new();
            for op in a.ops.iter_mut().filter(|op| op.operator == "BDC") {
                match op.operands.get_mut(1) {
                    Some(Object::Dictionary(d)) => tirar_texto_alt(d),
                    Some(Object::Name(n)) => nomes.push(n.clone()),
                    _ => {}
                }
            }
            if !nomes.is_empty() { self.tirar_texto_das_propriedades(id, &nomes); }
            // A miniatura da página mostra o que foi tarjado.
            if let Ok(p) = self.documento.get_dictionary_mut(id) { p.remove(b"Thumb"); }
            // O conteúdo antigo fica entre q/Q: o preto sai no espaço da página.
            a.ops.insert(0, Operation::new("q", vec![]));
            a.ops.push(Operation::new("Q", vec![]));
            a.ops.push(Operation::new("q", vec![]));
            a.ops.push(Operation::new("rg", vec![0.into(), 0.into(), 0.into()]));
            for c in caixas {
                a.ops.push(Operation::new("re", vec![c[0].into(), c[1].into(), (c[2] - c[0]).into(), (c[3] - c[1]).into()]));
            }
            a.ops.push(Operation::new("f", vec![]));
            a.ops.push(Operation::new("Q", vec![]));
            self.gravar_conteudo(pagina, &a.ops)?;
            self.tirar_anotacoes(id, caixas);
        }
        if !rotulo.trim().is_empty() {
            for &(p, c) in marcas {
                let tamanho = ((c[3] - c[1]) * 0.7).clamp(4.0, 10.0);
                let x = c[0] + 2.0;
                let y = (c[1] + c[3]) / 2.0 - tamanho * 0.35;
                // Rótulo que não cabe na fonte padrão não impede a tarja.
                let _ = self.inserir_texto(p, x, y, rotulo, tamanho, true, [1.0, 1.0, 1.0]);
            }
        }
        // Nada do que a tarja tirou (conteúdo antigo, campo, aparência) pode ficar solto no arquivo.
        crate::podar(&mut self.documento);
        self.modificado = true;
        Ok(letras)
    }

    /// Pinta de preto o pedaço da imagem `x` debaixo das caixas. `matriz` leva
    /// o quadrado 0..1 da imagem para a página.
    fn apagar_pixels(&mut self, x: ObjectId, matriz: crate::objetos::Matriz, caixas: &[[f32; 4]], apagar: ApagarPixels<'_>) -> bool {
        let Some(inv) = inversa(matriz) else { return false };
        let Some((w, h, componentes, jpeg, dados)) = ler_imagem(&self.documento, x) else { return false };
        let retangulos: Vec<[u32; 4]> = caixas.iter().map(|c| {
            let cantos = [(c[0], c[1]), (c[2], c[1]), (c[0], c[3]), (c[2], c[3])].map(|(px, py)| aplicar(inv, px, py));
            let (u0, u1) = (cantos.iter().map(|p| p.0).fold(f32::MAX, f32::min), cantos.iter().map(|p| p.0).fold(f32::MIN, f32::max));
            let (v0, v1) = (cantos.iter().map(|p| p.1).fold(f32::MAX, f32::min), cantos.iter().map(|p| p.1).fold(f32::MIN, f32::max));
            let px = |u: f32, n: u32, cima: bool| ((if cima { u.ceil() } else { u.floor() }).clamp(0.0, n as f32)) as u32;
            [px(u0.clamp(0.0, 1.0) * w as f32, w, false), px((1.0 - v1.clamp(0.0, 1.0)) * h as f32, h, false),
             px(u1.clamp(0.0, 1.0) * w as f32, w, true), px((1.0 - v0.clamp(0.0, 1.0)) * h as f32, h, true)]
        }).collect();
        let Some((novo, nw, nh)) = apagar(ImagemCrua { largura: w, altura: h, componentes, jpeg, dados: &dados }, &retangulos) else { return false };
        trocar_por_jpeg(&mut self.documento, x, novo, nw, nh);
        // A máscara de transparência guarda o contorno do que foi tarjado (a assinatura).
        if let Ok(s) = self.documento.get_object_mut(x).and_then(Object::as_stream_mut) { s.dict.remove(b"SMask"); }
        true
    }

    /// Troca (ou tira, com `None`) o XObject `nome` nos recursos da página. Os recursos
    /// podem ser herdados ou divididos com outras páginas: a página ganha cópia própria.
    fn trocar_xobjeto(&mut self, pagina: ObjectId, nome: &[u8], novo: Option<ObjectId>) {
        let doc = &self.documento;
        let Ok((direto, herdados)) = doc.get_page_resources(pagina) else { return };
        let dicts: Vec<&Dictionary> = direto.into_iter().chain(herdados.iter().filter_map(|r| doc.get_dictionary(*r).ok())).collect();
        let xobj = |d: &Dictionary| match d.get(b"XObject") {
            Ok(Object::Reference(r)) => doc.get_dictionary(*r).ok().cloned(),
            Ok(Object::Dictionary(x)) => Some(x.clone()),
            _ => None,
        };
        let Some(mut lista) = dicts.iter().filter_map(|d| xobj(d)).find(|x| x.has(nome)) else { return };
        let mut recursos = dicts.first().map(|d| (*d).clone()).unwrap_or_default();
        match novo { Some(n) => lista.set(nome.to_vec(), Object::Reference(n)), None => { lista.remove(nome); } }
        recursos.set("XObject", lista);
        if let Ok(p) = self.documento.get_dictionary_mut(pagina) { p.set("Resources", recursos); }
    }

    /// Tira o texto alternativo das propriedades `nomes` dos recursos da página (diretas ou por referência).
    fn tirar_texto_das_propriedades(&mut self, pagina: ObjectId, nomes: &[Vec<u8>]) {
        fn limpar(props: &mut Dictionary, nomes: &[Vec<u8>], refs: &mut Vec<ObjectId>) {
            for n in nomes {
                match props.get_mut(n) { Ok(Object::Dictionary(d)) => tirar_texto_alt(d), Ok(Object::Reference(r)) => refs.push(*r), _ => {} }
            }
        }
        fn em(rec: &mut Dictionary, nomes: &[Vec<u8>], refs: &mut Vec<ObjectId>, props: &mut Vec<ObjectId>) {
            match rec.get_mut(b"Properties") { Ok(Object::Dictionary(p)) => limpar(p, nomes, refs), Ok(Object::Reference(r)) => props.push(*r), _ => {} }
        }
        let Ok((direto, herdados)) = self.documento.get_page_resources(pagina) else { return };
        let direto = direto.is_some();
        let (mut refs, mut props) = (Vec::new(), Vec::new());
        if direto {
            if let Ok(Object::Dictionary(rec)) = self.documento.get_dictionary_mut(pagina).and_then(|p| p.get_mut(b"Resources")) { em(rec, nomes, &mut refs, &mut props); }
        }
        for h in herdados { if let Ok(rec) = self.documento.get_dictionary_mut(h) { em(rec, nomes, &mut refs, &mut props); } }
        for r in props { if let Ok(p) = self.documento.get_dictionary_mut(r) { limpar(p, nomes, &mut refs); } }
        for r in refs { if let Ok(d) = self.documento.get_dictionary_mut(r) { tirar_texto_alt(d); } }
    }

    fn tirar_anotacoes(&mut self, pagina: ObjectId, caixas: &[[f32; 4]]) {
        let anotacoes = self.lista_anotacoes(pagina);
        if anotacoes.is_empty() { return; }
        let doc = &self.documento;
        fn dicionario<'a>(doc: &'a lopdf::Document, o: &'a Object) -> Option<&'a Dictionary> {
            match o { Object::Reference(r) => doc.get_dictionary(*r).ok(), o => o.as_dict().ok() }
        }
        let toca = |o: &Object| -> bool {
            let Some(r) = dicionario(doc, o).and_then(|d| d.get(b"Rect").ok()).and_then(|r| r.as_array().ok()) else { return false };
            let n: Vec<f32> = r.iter().filter_map(|x| x.as_float().ok()).collect();
            n.len() == 4 && caixas.iter().any(|c| cruza([n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])], *c))
        };
        let (mut saem, mut ficam): (Vec<Object>, Vec<Object>) = anotacoes.into_iter().partition(|o| toca(o));
        // O popup (na margem) e as respostas guardam o texto do comentário tarjado: saem junto.
        loop {
            let ids: std::collections::HashSet<ObjectId> = saem.iter().filter_map(|o| o.as_reference().ok()).collect();
            let preso = |o: &Object| dicionario(doc, o).is_some_and(|d| [&b"Parent"[..], b"IRT"].iter()
                .any(|k| d.get(k).and_then(Object::as_reference).is_ok_and(|r| ids.contains(&r))));
            let (mais, resto): (Vec<Object>, Vec<Object>) = ficam.into_iter().partition(|o| preso(o));
            ficam = resto;
            if mais.is_empty() { break; }
            saem.extend(mais);
        }
        if let Ok(p) = self.documento.get_dictionary_mut(pagina) { p.set("Annots", ficam); }
        // O valor do campo mora no /AcroForm, não na página: o campo tarjado sai de lá também.
        let saem: std::collections::HashSet<ObjectId> = saem.iter().filter_map(|o| o.as_reference().ok()).collect();
        if !saem.is_empty() { self.tirar_campos(&saem); }
    }

    /// Tira título, autor, programa e datas (Info e XMP) e os anexos.
    pub fn limpar_metadados(&mut self) {
        self.documento.trailer.remove(b"Info");
        if let Ok(c) = self.documento.catalog_mut() { c.remove(b"Metadata"); }
        tirar_anexos(&mut self.documento);
        self.modificado = true;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    use crate::objetos::testes::{no_arquivo, pdf_com};

    /// Põe o fluxo nos recursos da página 1 com o `nome`.
    fn com_xobjeto(pdf: &mut DocumentoPdf, nome: &str, fluxo: Stream) -> ObjectId {
        let x = pdf.documento.add_object(fluxo);
        let id = pdf.id_pagina(1).unwrap();
        let rec = pdf.documento.get_dictionary_mut(id).unwrap().get_mut(b"Resources").unwrap().as_dict_mut().unwrap();
        rec.get_mut(b"XObject").unwrap().as_dict_mut().unwrap().set(nome, x);
        x
    }

    const PAGINA: [f32; 4] = [0.0, 0.0, 595.0, 842.0];

    #[test]
    fn tarja_tira_do_arquivo_imagem_grupo_e_embutida() {
        // Imagem que o adaptador não decodifica: vira um pixel preto, a original sai do arquivo.
        let mut pdf = pdf_com("q 100 0 0 100 100 100 cm /Im2 Do Q");
        com_xobjeto(&mut pdf, "Im2", Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 9, "Height" => 1,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, b"IMGSECRET".to_vec()));
        pdf.tarjar(&[(1, PAGINA)], "", &|_, _| None).unwrap();
        assert!(!no_arquivo(&pdf.serializar().unwrap(), b"IMGSECRET"));
        assert!(matches!(pdf.objetos(1).unwrap()[0].tipo, TipoObjeto::Imagem { largura: 1, altura: 1 }));
        // Form XObject com texto.
        let mut pdf = pdf_com("q /Fm1 Do Q");
        com_xobjeto(&mut pdf, "Fm1", Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 500.into(), 500.into()] }, b"BT /F1 12 Tf 10 10 Td (FORMSECRET) Tj ET".to_vec()));
        pdf.tarjar(&[(1, PAGINA)], "", &|_, _| None).unwrap();
        assert!(!no_arquivo(&pdf.serializar().unwrap(), b"FORMSECRET"));
        // Imagem embutida no conteúdo.
        let mut pdf = pdf_com("q 100 0 0 100 100 100 cm BI /W 9 /H 1 /BPC 8 /CS /DeviceGray ID INLSECRET EI Q");
        assert!(matches!(pdf.objetos(1).unwrap()[0].tipo, TipoObjeto::Imagem { largura: 9, altura: 1 }));
        pdf.tarjar(&[(1, [150.0, 150.0, 160.0, 160.0])], "", &|_, _| None).unwrap();
        assert!(!no_arquivo(&pdf.serializar().unwrap(), b"INLSECRET"));
    }

    #[test]
    fn tarja_tira_mascara_e_texto_alternativo() {
        let mut pdf = pdf_com("q 100 0 0 100 100 100 cm /Im2 Do Q");
        let m = pdf.documento.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 9, "Height" => 1,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, b"MASKSECRE".to_vec()));
        com_xobjeto(&mut pdf, "Im2", Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 9, "Height" => 1,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "SMask" => m }, b"abcdefghi".to_vec()));
        pdf.tarjar(&[(1, PAGINA)], "", &|_, _| Some((b"\xff\xd8JPEGPRETO".to_vec(), 9, 1))).unwrap();
        assert!(!no_arquivo(&pdf.serializar().unwrap(), b"MASKSECRE"));

        let mut pdf = pdf_com("/Span <</ActualText (123.456.789-09) /Alt (123.456.789-09)>> BDC BT /F1 12 Tf 72 700 Td (123.456.789-09) Tj ET EMC");
        let achados = pdf.achar_para_tarja("", &[Padrao::Cpf]);
        pdf.tarjar(&[(1, achados[0].caixa)], "", &|_, _| None).unwrap();
        assert!(!no_arquivo(&pdf.serializar().unwrap(), b"123.456.789-09"));
    }

    #[test]
    fn padroes_brasileiros() {
        let t: Vec<char> = "CPF 123.456.789-09, CNPJ 12.345.678/0001-90, tel (11) 98765-4321, maria.silva@email.com. CEP 01310-100; 1234.567.890-123".chars().collect();
        let texto = |p| achar_padrao(&t, p).into_iter().map(|(i, f)| t[i..f].iter().collect::<String>()).collect::<Vec<_>>();
        assert_eq!(texto(Padrao::Cpf), ["123.456.789-09"]);
        assert_eq!(texto(Padrao::Cnpj), ["12.345.678/0001-90"]);
        assert_eq!(texto(Padrao::Telefone), ["(11) 98765-4321"]);
        assert_eq!(texto(Padrao::Email), ["maria.silva@email.com"]);
        assert_eq!(texto(Padrao::Cep), ["01310-100"]);
    }

    #[test]
    fn nomes_por_deixa_e_pela_forma() {
        let t: Vec<char> = "Nome: JOÃO PEDRO SOUZA, CPF 1. Paciente - Ana Lima. Assinado por Maria da Silva Santos na \
            Secretaria da Fazenda Estadual, Rua Augusta. Então Pedro".chars().collect();
        let achados: Vec<String> = achar_nomes(&t).into_iter().map(|(i, f)| t[i..f].iter().collect()).collect();
        assert_eq!(achados, ["JOÃO PEDRO SOUZA", "Ana Lima", "Maria da Silva Santos"]);
    }

    #[test]
    fn tarja_tira_o_campo_e_o_valor_do_arquivo() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (texto) Tj ET");
        pdf.criar_campo(1, [100.0, 400.0, 250.0, 420.0], &crate::CampoNovo::Texto, "Senha").unwrap();
        pdf.criar_campo(1, [100.0, 300.0, 250.0, 320.0], &crate::CampoNovo::Texto, "Fica").unwrap();
        let campo = pdf.campos().into_iter().find(|c| c.nome == "Senha").unwrap();
        pdf.preencher(&campo, "SEGREDO123").unwrap();
        pdf.tarjar(&[(1, [90.0, 390.0, 260.0, 430.0])], "", &|_, _| None).unwrap();
        assert_eq!(pdf.campos().iter().map(|c| c.nome.as_str()).collect::<Vec<_>>(), ["Fica"]);
        assert!(!pdf.documento.objects.values().any(tem), "o valor tarjado ficou no arquivo");
    }

    /// "SEGREDO" em qualquer lugar do objeto (strings, dicionários, conteúdo de stream).
    fn tem(o: &Object) -> bool {
        let achar = |b: &[u8]| b.windows(7).any(|w| w == b"SEGREDO");
        match o {
            Object::String(b, _) => achar(b),
            Object::Array(v) => v.iter().any(tem),
            Object::Dictionary(d) => d.iter().any(|(_, v)| tem(v)),
            Object::Stream(st) => achar(&st.decompressed_content().unwrap_or_else(|_| st.content.clone())) || st.dict.iter().any(|(_, v)| tem(v)),
            _ => false,
        }
    }

    #[test]
    fn tarja_nao_deixa_o_segredo_em_lugar_nenhum() {
        let mut pdf = pdf_com("/Span /MC0 BDC BT /F1 12 Tf 72 700 Td (texto) Tj ET EMC");
        let id = pdf.id_pagina(1).unwrap();
        let doc = &mut pdf.documento;
        // Nota sobre o texto, com o popup na margem apontando para ela, numa lista /Annots por referência.
        let nota = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![70.into(), 695.into(), 90.into(), 715.into()],
            "Contents" => Object::string_literal("SEGREDO na nota") });
        let popup = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Popup", "Rect" => vec![500.into(), 700.into(), 590.into(), 800.into()],
            "Parent" => nota });
        doc.get_dictionary_mut(nota).unwrap().set("Popup", popup);
        let lista = doc.add_object(vec![Object::Reference(nota), Object::Reference(popup)]);
        let miniatura = doc.add_object(Stream::new(dictionary! { "Width" => 7, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, b"SEGREDO".to_vec()));
        let pagina = doc.get_dictionary_mut(id).unwrap();
        pagina.set("Annots", lista);
        pagina.set("Thumb", miniatura);
        pagina.get_mut(b"Resources").unwrap().as_dict_mut().unwrap()
            .set("Properties", dictionary! { "MC0" => dictionary! { "ActualText" => Object::string_literal("SEGREDO alternativo") } });
        pdf.tarjar(&[(1, [60.0, 690.0, 200.0, 720.0])], "", &|_, _| None).unwrap();
        assert!(!pdf.documento.objects.values().any(tem), "o segredo ficou no arquivo");
        assert!(pdf.lista_anotacoes(id).is_empty());
    }

    #[test]
    fn tarja_apaga_so_as_letras_debaixo() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (CPF 123.456.789-09 fim) Tj ET");
        let achados = pdf.achar_para_tarja("", &[Padrao::Cpf]);
        assert_eq!(achados.len(), 1);
        assert_eq!(achados[0].texto, "123.456.789-09");
        pdf.tarjar(&[(1, achados[0].caixa)], "", &|_, _| None).unwrap();
        let texto = pdf.texto_pagina(1).unwrap();
        assert!(!texto.contains("123"), "{texto}");
        assert!(texto.contains("CPF") && texto.contains("fim"), "{texto}");
        // A posição de "fim" não muda: o recuo ocupa o lugar das letras.
        let fim = |p: &DocumentoPdf| p.objetos(1).unwrap()[0].caixa[2];
        assert!((fim(&pdf) - fim(&pdf_com("BT /F1 12 Tf 72 700 Td (CPF 123.456.789-09 fim) Tj ET"))).abs() < 0.5);
    }
}
