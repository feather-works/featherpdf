//! Comentários (anotações do PDF: nota, caixa de texto, destaque, sublinhado,
//! riscado, tinta e carimbo) e camadas (conteúdo opcional, OCG). Cada
//! anotação nova leva a aparência pronta (/AP), para qualquer leitor
//! desenhar igual.

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use crate::inserir::{cor, win_ansi};
use crate::objetos::ItemTj;
use crate::DocumentoPdf;
use feather_draw::{largura_texto, FontePdf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marcacao { Destacar, Sublinhar, Riscar, Ondulado }

#[derive(Debug, Clone, PartialEq)]
pub enum AnotacaoNova {
    /// Ícone de nota com o canto de cima à esquerda no ponto.
    Nota { ponto: (f32, f32), texto: String },
    CaixaTexto { caixa: [f32; 4], texto: String },
    /// Uma caixa por linha de texto marcada; `texto` é o trecho marcado.
    Marcacao { tipo: Marcacao, caixas: Vec<[f32; 4]>, texto: String },
    /// Traços à mão livre, no espaço da página.
    Tinta { tracos: Vec<Vec<(f32, f32)>> },
    Carimbo { caixa: [f32; 4], titulo: String, detalhe: String },
}

/// Comentário existente, para o painel. `responde_a` é o índice (na mesma página) do
/// comentário respondido (/IRT); `estado` vem da última marca de revisão (/State).
#[derive(Debug, Clone, PartialEq)]
pub struct Anotacao {
    pub pagina: u32, pub indice: usize, pub tipo: &'static str, pub texto: String, pub autor: String, pub caixa: [f32; 4],
    pub responde_a: Option<usize>, pub data: String, pub estado: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Camada { pub id: (u32, u16), pub nome: String, pub visivel: bool, /** Travada (/Locked): o leitor não deixa ligar nem desligar. */ pub travada: bool, /** Quando aparece (/Usage). */ pub uso: crate::Visibilidade }

const AMARELO: [f32; 3] = [1.0, 0.85, 0.2];
const VERMELHO: [f32; 3] = [0.85, 0.15, 0.13];
const AZUL: [f32; 3] = [0.1, 0.35, 0.85];

/// Cor do carimbo pelo sentido do texto: verde (aprovado, pago), vermelho (reprovado), azul.
pub fn cor_do_carimbo(titulo: &str) -> [f32; 3] {
    let t = titulo.to_lowercase();
    if ["aprov", "pago", "conferido", "ok"].iter().any(|p| t.contains(p)) { [0.13, 0.55, 0.25] }
    else if ["reprov", "confiden", "cancel", "urgente", "não"].iter().any(|p| t.contains(p)) { VERMELHO }
    else { AZUL }
}

pub(crate) fn retangulo(c: [f32; 4]) -> Object { c.iter().map(|&v| v.into()).collect::<Vec<Object>>().into() }

/// Texto em hexa WinAnsi (letra fora vira `?`).
pub(crate) fn hexa(t: &str) -> String {
    let b: String = t.chars().map(|c| win_ansi(c).unwrap_or(b'?')).map(|b| format!("{b:02X}")).collect();
    format!("<{b}>")
}

/// Linha em zigue-zague no pé da caixa (aparência do sublinhado ondulado).
fn ondas(tinta: [f32; 3], q: [f32; 4]) -> String {
    let alto = q[3] - q[1];
    let (passo, amp, base) = ((alto * 0.25).max(1.5), (alto * 0.06).max(0.7), q[1] + alto * 0.1);
    let mut s = format!("{} RG {:.2} w {:.2} {:.2} m", cor(tinta), (alto * 0.05).max(0.5), q[0], base);
    let mut x = q[0];
    let mut cima = true;
    while x < q[2] {
        x = (x + passo).min(q[2]);
        s += &format!(" {x:.2} {:.2} l", base + if cima { amp } else { -amp });
        cima = !cima;
    }
    s + " S\n"
}

/// Quebra em linhas que cabem em `largura` (Helvetica `tamanho`).
fn quebrar(texto: &str, largura: f32, tamanho: f32, fonte: FontePdf) -> Vec<String> {
    let mut linhas = Vec::new();
    for paragrafo in texto.lines() {
        let mut atual = String::new();
        for palavra in paragrafo.split(' ') {
            let teste = if atual.is_empty() { palavra.to_string() } else { format!("{atual} {palavra}") };
            if largura_texto(&teste, fonte, tamanho) > largura && !atual.is_empty() {
                linhas.push(std::mem::replace(&mut atual, palavra.to_string()));
            } else { atual = teste; }
        }
        linhas.push(atual);
    }
    linhas
}

/// Data no formato do PDF (D:AAAAMMDDHHmmSSZ), em UTC.
fn data_pdf() -> String {
    let (a, m, d, s) = agora();
    format!("D:{a:04}{m:02}{d:02}{:02}{:02}{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

/// Hoje como "dd/mm/aaaa" (UTC), para o carimbo.
pub fn hoje() -> String {
    let (a, m, d, _) = agora();
    format!("{d:02}/{m:02}/{a:04}")
}

/// Ano, mês, dia e segundos do dia agora, em UTC.
pub(crate) fn agora() -> (i64, i64, i64, i64) {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (dias, resto) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    // Dias desde 1970 para ano/mês/dia (algoritmo civil de Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d, resto)
}

impl DocumentoPdf {
    /// Form XObject da aparência, desenhado no espaço da página (BBox = caixa).
    fn aparencia(&mut self, caixa: [f32; 4], conteudo: String, recursos: Dictionary) -> ObjectId {
        self.documento.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => retangulo(caixa), "Resources" => recursos,
        }, conteudo.into_bytes()))
    }

    pub(crate) fn fonte_helvetica(negrito: bool) -> Dictionary {
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "Encoding" => "WinAnsiEncoding",
            "BaseFont" => if negrito { "Helvetica-Bold" } else { "Helvetica" } }
    }

    /// Caixas (uma por linha) das letras com o centro dentro de `r`, e o texto delas.
    pub fn caixas_de_texto(&self, pagina: u32, r: [f32; 4]) -> Result<(Vec<[f32; 4]>, String), String> {
        let a = self.analisar_com(pagina, true)?;
        let (mut caixas, mut texto) = (Vec::new(), String::new());
        for lugar in a.lugares.iter().filter(|l| !l.escritas.is_empty()) {
            let mut caixa: Option<[f32; 4]> = None;
            let mut linha = String::new();
            for item in lugar.escritas.iter().filter_map(|i| a.letras.get(i)).flatten() {
                let ItemTj::Glifo { texto: t, caixa: c, .. } = item else { continue };
                let (x, y) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
                if x < r[0] || x > r[2] || y < r[1] || y > r[3] { continue; }
                caixa = Some(caixa.map_or(*c, |k| [k[0].min(c[0]), k[1].min(c[1]), k[2].max(c[2]), k[3].max(c[3])]));
                linha.push_str(t);
            }
            if let Some(c) = caixa {
                caixas.push(c);
                if !texto.is_empty() { texto.push(' '); }
                texto.push_str(linha.trim());
            }
        }
        Ok((caixas, texto))
    }

    /// Cria a anotação na página, com a aparência pronta. `autor` vai no /T.
    pub fn anotar(&mut self, pagina: u32, nova: &AnotacaoNova, autor: &str) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let mut d = dictionary! { "Type" => "Annot", "M" => Object::string_literal(data_pdf()), "F" => 4 };
        if !autor.is_empty() { d.set("T", lopdf::text_string(autor)); }
        let texto_pdf = lopdf::text_string;
        let (caixa, ap) = match nova {
            AnotacaoNova::Nota { ponto: (x, y), texto } => {
                let c = [*x, y - 20.0, x + 20.0, *y];
                d.set("Subtype", "Text");
                d.set("Name", "Comment");
                d.set("Contents", texto_pdf(texto));
                d.set("C", AMARELO.iter().map(|&v| v.into()).collect::<Vec<Object>>());
                let conteudo = format!("{} rg 0.45 0.35 0 RG 1 w {:.2} {:.2} 18 16 re B 0 g {} \n0.3 0.25 0 RG 0.8 w {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} m {:.2} {:.2} l S",
                    cor(AMARELO), x + 1.0, y - 17.0, "",
                    x + 4.0, y - 5.0, x + 15.0, y - 5.0, x + 4.0, y - 8.5, x + 15.0, y - 8.5, x + 4.0, y - 12.0, x + 11.0, y - 12.0);
                (c, self.aparencia(c, conteudo, Dictionary::new()))
            }
            AnotacaoNova::CaixaTexto { caixa: c, texto } => {
                let tamanho = 11.0;
                d.set("Subtype", "FreeText");
                d.set("Contents", texto_pdf(texto));
                d.set("DA", Object::string_literal(format!("/Helv {tamanho} Tf 0 g")));
                let mut conteudo = format!("0.3 0.3 0.3 RG 0.8 w {:.2} {:.2} {:.2} {:.2} re S\nBT /Helv {tamanho} Tf 0 g\n", c[0] + 0.4, c[1] + 0.4, c[2] - c[0] - 0.8, c[3] - c[1] - 0.8);
                let mut y = c[3] - 4.0 - tamanho * 0.8;
                for linha in quebrar(texto, c[2] - c[0] - 8.0, tamanho, FontePdf::Helvetica) {
                    if y < c[1] + 2.0 { break; }
                    conteudo += &format!("1 0 0 1 {:.2} {:.2} Tm {} Tj\n", c[0] + 4.0, y, hexa(&linha));
                    y -= tamanho * 1.2;
                }
                conteudo += "ET";
                let rec = dictionary! { "Font" => dictionary! { "Helv" => Self::fonte_helvetica(false) } };
                (*c, self.aparencia(*c, conteudo, rec))
            }
            AnotacaoNova::Marcacao { tipo, caixas, texto } => {
                if caixas.is_empty() { return Err("Nenhum texto debaixo da marca.".into()); }
                let c = caixas.iter().fold(caixas[0], |k, c| [k[0].min(c[0]), k[1].min(c[1]), k[2].max(c[2]), k[3].max(c[3])]);
                let (sub, tinta) = match tipo { Marcacao::Destacar => ("Highlight", AMARELO), Marcacao::Sublinhar => ("Underline", AZUL), Marcacao::Riscar => ("StrikeOut", VERMELHO), Marcacao::Ondulado => ("Squiggly", VERMELHO) };
                d.set("Subtype", sub);
                if !texto.is_empty() { d.set("Contents", texto_pdf(texto)); }
                d.set("C", tinta.iter().map(|&v| v.into()).collect::<Vec<Object>>());
                d.set("QuadPoints", caixas.iter().flat_map(|q| [q[0], q[3], q[2], q[3], q[0], q[1], q[2], q[1]]).map(Object::from).collect::<Vec<Object>>());
                let mut conteudo = String::new();
                let mut rec = Dictionary::new();
                for q in caixas {
                    let alto = q[3] - q[1];
                    conteudo += &match tipo {
                        Marcacao::Destacar => format!("/G0 gs {} rg {:.2} {:.2} {:.2} {:.2} re f\n", cor(tinta), q[0], q[1], q[2] - q[0], alto),
                        Marcacao::Sublinhar => format!("{} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S\n", cor(tinta), (alto * 0.07).max(0.6), q[0], q[1] + alto * 0.12, q[2], q[1] + alto * 0.12),
                        Marcacao::Riscar => format!("{} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S\n", cor(tinta), (alto * 0.07).max(0.6), q[0], q[1] + alto * 0.5, q[2], q[1] + alto * 0.5),
                        Marcacao::Ondulado => ondas(tinta, *q),
                    };
                }
                if *tipo == Marcacao::Destacar {
                    rec.set("ExtGState", dictionary! { "G0" => dictionary! { "Type" => "ExtGState", "BM" => "Multiply" } });
                }
                (c, self.aparencia(c, conteudo, rec))
            }
            AnotacaoNova::Tinta { tracos } => {
                let pontos: Vec<(f32, f32)> = tracos.iter().flatten().copied().collect();
                if pontos.len() < 2 { return Err("Traço curto demais.".into()); }
                let m = 2.0;
                let c = pontos.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |k, p| [k[0].min(p.0), k[1].min(p.1), k[2].max(p.0), k[3].max(p.1)]);
                let c = [c[0] - m, c[1] - m, c[2] + m, c[3] + m];
                d.set("Subtype", "Ink");
                d.set("C", AZUL.iter().map(|&v| v.into()).collect::<Vec<Object>>());
                d.set("BS", dictionary! { "W" => 1.5 });
                d.set("InkList", tracos.iter().map(|t| Object::Array(t.iter().flat_map(|p| [p.0.into(), p.1.into()]).collect())).collect::<Vec<Object>>());
                let mut conteudo = format!("{} RG 1.5 w 1 J 1 j\n", cor(AZUL));
                for t in tracos.iter().filter(|t| !t.is_empty()) {
                    conteudo += &format!("{:.2} {:.2} m\n", t[0].0, t[0].1);
                    for p in &t[1..] { conteudo += &format!("{:.2} {:.2} l\n", p.0, p.1); }
                    conteudo += "S\n";
                }
                (c, self.aparencia(c, conteudo, Dictionary::new()))
            }
            AnotacaoNova::Carimbo { caixa: c, titulo, detalhe } => {
                let tinta = cor_do_carimbo(titulo);
                d.set("Subtype", "Stamp");
                d.set("Name", lopdf::Object::Name(titulo.replace(' ', "").into_bytes()));
                d.set("Contents", texto_pdf(format!("{titulo} {detalhe}").trim()));
                d.set("C", tinta.iter().map(|&v| v.into()).collect::<Vec<Object>>());
                let (l, a) = (c[2] - c[0], c[3] - c[1]);
                let tem_detalhe = !detalhe.trim().is_empty();
                let titulo_m = titulo.to_uppercase();
                let largura_t = largura_texto(&titulo_m, FontePdf::Negrito, 1.0).max(0.1);
                let t = ((l - 12.0) / largura_t).min(a * if tem_detalhe { 0.5 } else { 0.7 }).max(4.0);
                let base_t = if tem_detalhe { c[1] + a * 0.42 } else { c[1] + (a - t * 0.72) / 2.0 };
                let mut conteudo = format!("{} RG {} rg 2 w {:.2} {:.2} {:.2} {:.2} re S\n1 w {:.2} {:.2} {:.2} {:.2} re S\nBT /F1 {t:.2} Tf 1 0 0 1 {:.2} {:.2} Tm {} Tj ET\n",
                    cor(tinta), cor(tinta), c[0] + 1.0, c[1] + 1.0, l - 2.0, a - 2.0, c[0] + 4.0, c[1] + 4.0, l - 8.0, a - 8.0,
                    c[0] + (l - largura_t * t) / 2.0, base_t, hexa(&titulo_m));
                if tem_detalhe {
                    let td = (a * 0.16).clamp(4.0, 10.0);
                    let ld = largura_texto(detalhe, FontePdf::Helvetica, td);
                    conteudo += &format!("BT /F2 {td:.2} Tf 1 0 0 1 {:.2} {:.2} Tm {} Tj ET\n", c[0] + ((l - ld) / 2.0).max(6.0), c[1] + a * 0.18, hexa(detalhe));
                }
                let rec = dictionary! { "Font" => dictionary! { "F1" => Self::fonte_helvetica(true), "F2" => Self::fonte_helvetica(false) } };
                (*c, self.aparencia(*c, conteudo, rec))
            }
        };
        d.set("Rect", retangulo(caixa));
        d.set("P", Object::Reference(id));
        d.set("AP", dictionary! { "N" => Object::Reference(ap) });
        let anotacao = self.documento.add_object(d);
        self.por_anotacao(id, anotacao)
    }

    /// Comentários de todas as páginas (sem links, campos de formulário e balões).
    pub fn anotacoes(&self) -> Vec<Anotacao> {
        let mut todas = Vec::new();
        for (pagina, id) in self.documento.get_pages() {
            let lista = self.lista_anotacoes(id);
            for (indice, o) in lista.iter().enumerate() {
                let d = match o { Object::Reference(r) => self.documento.get_dictionary(*r).ok(), o => o.as_dict().ok() };
                let Some(d) = d else { continue };
                let tipo = match d.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"") {
                    b"Text" => "Nota", b"FreeText" => "Caixa de texto", b"Highlight" => "Destaque", b"Underline" => "Sublinhado",
                    b"StrikeOut" => "Riscado", b"Squiggly" => "Ondulado", b"Ink" => "Desenho", b"Stamp" => "Carimbo",
                    b"Square" => "Retângulo", b"Circle" => "Elipse", b"Line" => "Linha", b"Polygon" => "Polígono",
                    b"PolyLine" => "Polilinha", b"Caret" => "Inserção", b"FileAttachment" => "Anexo", b"Sound" => "Som",
                    _ => continue,
                };
                let texto_de = |k: &[u8]| d.get(k).ok().and_then(|o| lopdf::decode_text_string(o).ok()).unwrap_or_default();
                let n: Vec<f32> = d.get(b"Rect").and_then(Object::as_array).map(|v| v.iter().filter_map(|x| x.as_float().ok()).collect()).unwrap_or_default();
                let caixa = if n.len() == 4 { [n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])] } else { [0.0; 4] };
                let data = d.get(b"M").ok().and_then(|o| o.as_str().ok()).map(|b| crate::revisao::data_legivel(&String::from_utf8_lossy(b))).unwrap_or_default();
                let responde_a = d.get(b"IRT").and_then(Object::as_reference).ok()
                    .and_then(|r| lista.iter().position(|o| o.as_reference().ok() == Some(r)));
                todas.push(Anotacao { pagina, indice, tipo, texto: texto_de(b"Contents"), autor: texto_de(b"T"), caixa, responde_a, data,
                    estado: d.get(b"State").ok().and_then(|o| lopdf::decode_text_string(o).ok()) });
            }
        }
        todas
    }

    /// Apaga a anotação `indice` da página (e o balão dela, se tiver).
    pub fn apagar_anotacao(&mut self, pagina: u32, indice: usize) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let mut lista = self.lista_anotacoes(id);
        if indice >= lista.len() { return Err("Esse comentário não existe mais.".into()); }
        let alvo = lista.remove(indice);
        if let Object::Reference(r) = alvo {
            // Vão junto o balão (/Parent) e as respostas e marcas de estado (/IRT).
            let pai = |o: &Object| matches!(o, Object::Reference(x) if self.documento.get_dictionary(*x).is_ok_and(|d|
                [&b"Parent"[..], b"IRT"].iter().any(|k| d.get(k).and_then(Object::as_reference).is_ok_and(|p| p == r))));
            lista.retain(|o| !pai(o));
        }
        self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Annots", lista);
        self.modificado = true;
        Ok(())
    }

    /// OCProperties e a configuração padrão (/D) dela, já resolvidas.
    fn config_camadas(&self) -> Option<(Dictionary, Dictionary)> {
        let resolver = |o: &Object| match o { Object::Reference(r) => self.documento.get_dictionary(*r).ok().cloned(), o => o.as_dict().ok().cloned() };
        let props = resolver(self.documento.catalog().ok()?.get(b"OCProperties").ok()?)?;
        let d = resolver(props.get(b"D").ok()?)?;
        Some((props, d))
    }

    /// Camadas do documento, na ordem do /OCGs, com a visibilidade padrão.
    pub fn camadas(&self) -> Vec<Camada> {
        let Some((props, d)) = self.config_camadas() else { return vec![] };
        let ids = |o: Result<&Object, lopdf::Error>| -> Vec<ObjectId> {
            o.and_then(Object::as_array).map(|v| v.iter().filter_map(|x| x.as_reference().ok()).collect()).unwrap_or_default()
        };
        let base_off = d.get(b"BaseState").and_then(Object::as_name).is_ok_and(|n| n == b"OFF");
        let (on, off, travadas) = (ids(d.get(b"ON")), ids(d.get(b"OFF")), ids(d.get(b"Locked")));
        ids(props.get(b"OCGs")).into_iter().map(|id| {
            let nome = self.documento.get_dictionary(id).ok().and_then(|g| g.get(b"Name").ok())
                .and_then(|n| lopdf::decode_text_string(n).ok()).unwrap_or_else(|| format!("Camada {}", id.0));
            let visivel = if off.contains(&id) { false } else if on.contains(&id) { true } else { !base_off };
            Camada { id, nome, visivel, travada: travadas.contains(&id), uso: crate::camadas::uso_da_camada(self, id) }
        }).collect()
    }

    /// Liga ou desliga a camada na configuração padrão (vale para todo leitor).
    pub fn mostrar_camada(&mut self, id: (u32, u16), visivel: bool) -> Result<(), String> {
        let (mut props, mut d) = self.config_camadas().ok_or("Este PDF não tem camadas.")?;
        let tirar = |d: &mut Dictionary, k: &[u8]| {
            let v: Vec<Object> = d.get(k).and_then(Object::as_array).cloned().unwrap_or_default()
                .into_iter().filter(|o| o.as_reference().ok() != Some(id)).collect();
            d.set(k.to_vec(), v);
        };
        tirar(&mut d, b"ON");
        tirar(&mut d, b"OFF");
        let chave: &[u8] = if visivel { b"ON" } else { b"OFF" };
        let mut v = d.get(chave).and_then(Object::as_array).cloned().unwrap_or_default();
        v.push(Object::Reference(id));
        d.set(chave.to_vec(), v);
        // Grava de volta onde estava: objeto próprio ou dentro do pai.
        let props_ref = self.documento.catalog().ok().and_then(|c| c.get(b"OCProperties").ok()).and_then(|o| o.as_reference().ok());
        match props.get(b"D").ok().and_then(|o| o.as_reference().ok()) {
            Some(r) => { self.documento.objects.insert(r, Object::Dictionary(d)); }
            None => props.set("D", d),
        }
        match props_ref {
            Some(r) => { self.documento.objects.insert(r, Object::Dictionary(props)); }
            None => self.documento.catalog_mut().map_err(|e| e.to_string())?.set("OCProperties", props),
        }
        self.modificado = true;
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::objetos::testes::pdf_com;

    #[test]
    fn comentarios_entram_e_saem() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Texto para destacar) Tj ET");
        let (caixas, texto) = pdf.caixas_de_texto(1, [60.0, 690.0, 300.0, 720.0]).unwrap();
        assert_eq!((caixas.len(), texto.as_str()), (1, "Texto para destacar"));
        pdf.anotar(1, &AnotacaoNova::Marcacao { tipo: Marcacao::Destacar, caixas, texto }, "Ana").unwrap();
        pdf.anotar(1, &AnotacaoNova::Nota { ponto: (300.0, 700.0), texto: "Olá".into() }, "Ana").unwrap();
        pdf.anotar(1, &AnotacaoNova::Carimbo { caixa: [300.0, 400.0, 450.0, 450.0], titulo: "Aprovado".into(), detalhe: "Ana · 19/10/2026".into() }, "").unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let lista = pdf.anotacoes();
        assert_eq!(lista.iter().map(|a| a.tipo).collect::<Vec<_>>(), ["Destaque", "Nota", "Carimbo"]);
        assert_eq!((lista[1].texto.as_str(), lista[1].autor.as_str()), ("Olá", "Ana"));
        let mut pdf = pdf;
        pdf.apagar_anotacao(1, 1).unwrap();
        assert_eq!(pdf.anotacoes().len(), 2);
        assert!(data_pdf().starts_with("D:20"));
    }

    #[test]
    fn camadas_ligam_e_desligam() {
        let mut pdf = pdf_com("q Q");
        let g = pdf.documento.add_object(dictionary! { "Type" => "OCG", "Name" => lopdf::text_string("Elétrica") });
        let props = dictionary! { "OCGs" => vec![Object::Reference(g)], "D" => dictionary! { "Order" => vec![Object::Reference(g)] } };
        pdf.documento.catalog_mut().unwrap().set("OCProperties", props);
        assert_eq!(pdf.camadas()[0].visivel, true);
        pdf.mostrar_camada(g, false).unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!((pdf.camadas()[0].nome.as_str(), pdf.camadas()[0].visivel), ("Elétrica", false));
    }
}
