//! Revisão de comentários: respostas (/IRT, como o Acrobat grava a conversa), estado
//! de revisão (/State com /StateModel /Review), "substituir texto" (riscado + Caret
//! agrupados) e a lista de comentários em CSV para o FeatherSheet.

use lopdf::{dictionary, Object, ObjectId, Stream};

use crate::anotacoes::{retangulo, Anotacao, AnotacaoNova, Marcacao};
use crate::DocumentoPdf;

/// Estados de revisão do PDF e o nome em português.
pub const ESTADOS: [(&str, &str); 4] = [("Accepted", "Aceito"), ("Rejected", "Rejeitado"), ("Completed", "Concluído"), ("Cancelled", "Cancelado")];

/// "D:20261019143005Z" ou "D:20261019143005-03'00'" como "19/10/2026 14:30"; o resto fica como veio.
pub fn data_legivel(d: &str) -> String {
    let n: String = d.trim_start_matches("D:").chars().take(12).collect();
    if n.len() < 8 || !n.chars().all(|c| c.is_ascii_digit()) { return d.to_string(); }
    let data = format!("{}/{}/{}", &n[6..8], &n[4..6], &n[0..4]);
    if n.len() == 12 { format!("{data} {}:{}", &n[8..10], &n[10..12]) } else { data }
}

/// Nome em português do estado gravado (Accepted → Aceito).
pub fn nome_estado(estado: &str) -> &str {
    ESTADOS.iter().find(|(k, _)| *k == estado).map_or(estado, |(_, v)| *v)
}

fn agora_pdf() -> Object {
    let (a, m, d, s) = crate::anotacoes::agora();
    Object::string_literal(format!("D:{a:04}{m:02}{d:02}{:02}{:02}{:02}Z", s / 3600, s / 60 % 60, s % 60))
}

impl DocumentoPdf {
    /// Referência e caixa da anotação `indice` da página.
    fn alvo_anotacao(&self, pagina: u32, indice: usize) -> Result<(ObjectId, ObjectId, Object), String> {
        let id = self.id_pagina(pagina)?;
        let alvo = self.lista_anotacoes(id).get(indice).and_then(|o| o.as_reference().ok())
            .ok_or("Esse comentário não existe mais (ou não pode ser respondido).")?;
        let rect = self.documento.get_dictionary(alvo).and_then(|d| d.get(b"Rect")).cloned().map_err(|e| e.to_string())?;
        Ok((id, alvo, rect))
    }

    /// Nota-resposta sem desenho próprio, ligada ao comentário pelo /IRT.
    fn nota_ligada(&mut self, pagina: u32, indice: usize, extra: lopdf::Dictionary, autor: &str) -> Result<(), String> {
        let (id, alvo, rect) = self.alvo_anotacao(pagina, indice)?;
        // Aparência vazia: a resposta só aparece na lista, não por cima da página.
        let vazia = self.documento.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        }, Vec::new()));
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Text", "Rect" => rect, "P" => Object::Reference(id),
            "IRT" => Object::Reference(alvo), "M" => agora_pdf(), "F" => 4 | 2 | 32, "Name" => "Comment",
            "AP" => dictionary! { "N" => Object::Reference(vazia) },
        };
        if !autor.is_empty() { d.set("T", lopdf::text_string(autor)); }
        for (k, v) in extra.into_iter() { d.set(k, v); }
        let nova = self.documento.add_object(d);
        self.por_anotacao(id, nova)
    }

    /// Responde ao comentário: entra na conversa dele em qualquer leitor que mostre respostas.
    pub fn responder(&mut self, pagina: u32, indice: usize, texto: &str, autor: &str) -> Result<(), String> {
        if texto.trim().is_empty() { return Err("Escreva a resposta.".into()); }
        self.nota_ligada(pagina, indice, dictionary! { "Contents" => lopdf::text_string(texto.trim()) }, autor)
    }

    /// Marca o estado de revisão (Accepted, Rejected, Completed, Cancelled ou None para limpar).
    pub fn marcar_estado(&mut self, pagina: u32, indice: usize, estado: &str, autor: &str) -> Result<(), String> {
        if estado != "None" && !ESTADOS.iter().any(|(k, _)| *k == estado) { return Err(format!("Estado desconhecido: {estado}.")); }
        let texto = if estado == "None" { "Estado removido".to_string() } else { format!("Marcado como {}", nome_estado(estado)) };
        self.nota_ligada(pagina, indice, dictionary! {
            "State" => lopdf::text_string(estado), "StateModel" => lopdf::text_string("Review"), "Contents" => lopdf::text_string(&texto),
        }, autor)
    }

    /// "Substituir texto": risca o trecho e põe um sinal de inserção (Caret) com o texto novo,
    /// os dois agrupados (/IRT + /RT /Group), como o Acrobat faz.
    pub fn substituir_texto(&mut self, pagina: u32, caixas: Vec<[f32; 4]>, texto: String, novo: &str, autor: &str) -> Result<(), String> {
        if novo.trim().is_empty() { return Err("Escreva o texto que entra no lugar.".into()); }
        let ultima = *caixas.last().ok_or("Nenhum texto debaixo da marca.")?;
        self.anotar(pagina, &AnotacaoNova::Marcacao { tipo: Marcacao::Riscar, caixas, texto }, autor)?;
        let id = self.id_pagina(pagina)?;
        let risco = self.lista_anotacoes(id).last().and_then(|o| o.as_reference().ok()).ok_or("Falha ao riscar.")?;
        // Sinal ^ no fim do trecho, logo abaixo da linha.
        let alto = (ultima[3] - ultima[1]).max(6.0);
        let c = [ultima[2] - alto * 0.3, ultima[1] - alto * 0.35, ultima[2] + alto * 0.3, ultima[1] + alto * 0.25];
        let ap = self.documento.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => retangulo(c),
        }, format!("0.1 0.35 0.85 rg {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l f", c[0], c[1], (c[0] + c[2]) / 2.0, c[3], c[2], c[1]).into_bytes()));
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Caret", "Rect" => retangulo(c), "P" => Object::Reference(id),
            "Contents" => lopdf::text_string(novo.trim()), "M" => agora_pdf(), "F" => 4,
            "C" => vec![0.1.into(), 0.35.into(), 0.85.into()], "Sy" => "None",
            "AP" => dictionary! { "N" => Object::Reference(ap) },
        };
        if !autor.is_empty() { d.set("T", lopdf::text_string(autor)); }
        let caret = self.documento.add_object(d);
        self.por_anotacao(id, caret)?;
        let r = self.documento.get_dictionary_mut(risco).map_err(|e| e.to_string())?;
        r.set("IRT", Object::Reference(caret));
        r.set("RT", "Group");
        Ok(())
    }
}

/// Estado atual de cada comentário principal: a última marca de revisão dirigida a ele.
pub fn estado_atual<'a>(lista: &'a [Anotacao], principal: &Anotacao) -> Option<&'a str> {
    lista.iter().filter(|a| a.pagina == principal.pagina && a.responde_a == Some(principal.indice))
        .filter_map(|a| a.estado.as_deref()).last().filter(|e| *e != "None")
}

/// Lista de comentários em CSV (separador `;`, como o Excel e o FeatherSheet em pt-BR abrem).
pub fn comentarios_csv(lista: &[Anotacao]) -> String {
    let campo = |t: &str| if t.contains([';', '"', '\n']) { format!("\"{}\"", t.replace('"', "\"\"")) } else { t.to_string() };
    let mut s = String::from("Página;Tipo;Autor;Data;Estado;Texto;Resposta a\n");
    for a in lista.iter().filter(|a| a.estado.is_none()) {
        let (estado, pai) = match a.responde_a {
            Some(i) => (String::new(), lista.iter().find(|p| p.pagina == a.pagina && p.indice == i)
                .map(|p| format!("{} ({})", p.tipo, p.autor)).unwrap_or_default()),
            None => (estado_atual(lista, a).map(nome_estado).unwrap_or("").to_string(), String::new()),
        };
        s += &[a.pagina.to_string(), a.tipo.to_string(), a.autor.clone(), a.data.clone(), estado, a.texto.clone(), pai]
            .iter().map(|t| campo(t)).collect::<Vec<_>>().join(";");
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::objetos::testes::pdf_com;

    #[test]
    fn respostas_estado_e_csv() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Texto antigo aqui) Tj ET");
        pdf.anotar(1, &AnotacaoNova::Nota { ponto: (300.0, 700.0), texto: "Revisar; ok?".into() }, "Ana").unwrap();
        pdf.responder(1, 0, "Feito", "Bia").unwrap();
        pdf.marcar_estado(1, 0, "Accepted", "Bia").unwrap();
        assert!(pdf.marcar_estado(1, 0, "Talvez", "Bia").is_err());
        let (caixas, texto) = pdf.caixas_de_texto(1, [60.0, 690.0, 300.0, 720.0]).unwrap();
        pdf.substituir_texto(1, caixas, texto, "Texto novo", "Ana").unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let lista = pdf.anotacoes();
        assert_eq!(lista.iter().map(|a| a.tipo).collect::<Vec<_>>(), ["Nota", "Nota", "Nota", "Riscado", "Inserção"]);
        assert_eq!((lista[1].responde_a, lista[1].texto.as_str()), (Some(0), "Feito"));
        assert_eq!(estado_atual(&lista, &lista[0]), Some("Accepted"));
        assert!(lista[0].data.contains('/'));
        let csv = comentarios_csv(&lista);
        assert!(csv.contains("1;Nota;Ana;") && csv.contains("Aceito;\"Revisar; ok?\";"), "{csv}");
        assert!(csv.contains("Feito;Nota (Ana)") && csv.contains("Inserção;Ana;") && csv.lines().count() == 5, "{csv}");
        assert_eq!(data_legivel("D:20261019143005-03'00'"), "19/10/2026 14:30");
        // Apagar o comentário leva as respostas; apagar a inserção leva o riscado do grupo.
        let mut pdf = pdf;
        pdf.apagar_anotacao(1, 4).unwrap();
        pdf.apagar_anotacao(1, 0).unwrap();
        assert!(pdf.anotacoes().is_empty());
    }

    #[test]
    fn sublinhado_ondulado_vira_squiggly() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Ondas) Tj ET");
        let (caixas, texto) = pdf.caixas_de_texto(1, [60.0, 690.0, 300.0, 720.0]).unwrap();
        pdf.anotar(1, &AnotacaoNova::Marcacao { tipo: Marcacao::Ondulado, caixas, texto }, "").unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(pdf.anotacoes()[0].tipo, "Ondulado");
    }
}
