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

fn retangulo_de(o: &Object) -> Option<[f32; 4]> {
    let v: Vec<f32> = o.as_array().ok()?.iter().filter_map(|x| x.as_float().ok()).collect();
    (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
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

    /// Troca o texto do comentário (nota, marcação, caixa de texto…), como "Editar" no Acrobat.
    /// A caixa de texto mostra o texto na página: a aparência é refeita pelo mesmo desenho do criar.
    pub fn editar_comentario(&mut self, pagina: u32, indice: usize, texto: &str) -> Result<(), String> {
        let (id, alvo, rect) = self.alvo_anotacao(pagina, indice)?;
        let texto = texto.trim();
        let d = self.documento.get_dictionary(alvo).map_err(|e| e.to_string())?;
        let caixa_texto = d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"FreeText");
        if caixa_texto && texto.is_empty() { return Err("A caixa de texto não pode ficar vazia; apague o comentário.".into()); }
        let mut refeito = None;
        if caixa_texto {
            // Desenha uma caixa nova igual, guarda a aparência e tira a nova da página.
            let r = retangulo_de(&rect).ok_or("A caixa de texto não tem posição.")?;
            self.anotar(pagina, &AnotacaoNova::CaixaTexto { caixa: r, texto: texto.to_string() }, "")?;
            let mut lista = self.lista_anotacoes(id);
            let nova = lista.pop().and_then(|o| o.as_reference().ok()).ok_or("Falha ao refazer a caixa.")?;
            self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Annots", lista);
            let n = self.documento.objects.remove(&nova).and_then(|o| o.as_dict().ok().cloned()).ok_or("Falha ao refazer a caixa.")?;
            refeito = Some((n.get(b"AP").cloned().ok(), n.get(b"DA").cloned().ok()));
        }
        let d = self.documento.get_dictionary_mut(alvo).map_err(|e| e.to_string())?;
        if texto.is_empty() { d.remove(b"Contents"); } else { d.set("Contents", lopdf::text_string(texto)); }
        d.set("M", agora_pdf());
        if let Some((ap, da)) = refeito {
            if let Some(ap) = ap { d.set("AP", ap); }
            if let Some(da) = da { d.set("DA", da); }
        }
        Ok(())
    }

    /// Opacidade do comentário `indice` (0,1 a 1): o leitor aplica o /CA à aparência dele.
    pub fn definir_opacidade(&mut self, pagina: u32, indice: usize, opacidade: f32) -> Result<(), String> {
        if !opacidade.is_finite() { return Err("Opacidade inválida.".into()); }
        let (_, alvo, _) = self.alvo_anotacao(pagina, indice)?;
        let d = self.documento.get_dictionary_mut(alvo).map_err(|e| e.to_string())?;
        let v = opacidade.clamp(0.1, 1.0);
        if v >= 1.0 { d.remove(b"CA"); } else { d.set("CA", v); }
        self.modificado = true;
        Ok(())
    }

    /// Marca o estado de revisão (Accepted, Rejected, Completed, Cancelled ou None para limpar).
    pub fn marcar_estado(&mut self, pagina: u32, indice: usize, estado: &str, autor: &str) -> Result<(), String> {
        if estado != "None" && !ESTADOS.iter().any(|(k, _)| *k == estado) { return Err(format!("Estado desconhecido: {estado}.")); }
        let texto = if estado == "None" { "Estado removido".to_string() } else { format!("Marcado como {}", nome_estado(estado)) };
        self.nota_ligada(pagina, indice, dictionary! {
            "State" => lopdf::text_string(estado), "StateModel" => lopdf::text_string("Review"), "Contents" => lopdf::text_string(&texto),
        }, autor)?;
        if estado == "Accepted" { self.aplicar_substituicao(pagina, indice)?; }
        Ok(())
    }

    /// "Substituir texto" aceito: troca o trecho riscado pelo texto do Caret na página (com refluxo)
    /// e tira o riscado; o Caret fica, marcado como aceito. Outro comentário: não faz nada.
    fn aplicar_substituicao(&mut self, pagina: u32, indice: usize) -> Result<(), String> {
        let (id, caret, _) = self.alvo_anotacao(pagina, indice)?;
        let d = self.documento.get_dictionary(caret).map_err(|e| e.to_string())?;
        if d.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Caret") { return Ok(()); }
        let texto_de = |d: &lopdf::Dictionary| d.get(b"Contents").ok().and_then(|o| lopdf::decode_text_string(o).ok()).unwrap_or_default();
        let novo = texto_de(d);
        let lista = self.lista_anotacoes(id);
        let Some((k, risco)) = lista.iter().enumerate().find_map(|(k, o)| {
            let r = self.documento.get_dictionary(o.as_reference().ok()?).ok()?;
            (r.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"StrikeOut") && r.get(b"IRT").and_then(Object::as_reference).ok() == Some(caret)).then_some((k, r))
        }) else { return Ok(()) };
        let busca = texto_de(risco);
        let q: Vec<f32> = risco.get(b"QuadPoints").and_then(Object::as_array).map(|v| v.iter().filter_map(|x| x.as_float().ok()).collect()).unwrap_or_default();
        if q.len() < 8 { return Err("O trecho riscado não tem caixa.".into()); }
        let (cx, cy) = ((q[0] + q[2]) / 2.0, (q[1] + q[5]) / 2.0);
        let objeto = self.objetos(pagina)?.iter().position(|o| matches!(o.tipo, crate::TipoObjeto::Texto { .. })
            && o.caixa[0] <= cx && cx <= o.caixa[2] && o.caixa[1] <= cy && cy <= o.caixa[3])
            .ok_or("Não achei a linha do trecho riscado.")?;
        self.substituir_com_refluxo(pagina, objeto, busca.trim(), novo.trim())?;
        self.apagar_anotacao(pagina, k)
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
    fn opacidade_do_comentario() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Texto) Tj ET");
        pdf.anotar(1, &AnotacaoNova::Nota { ponto: (300.0, 700.0), texto: "x".into() }, "Ana").unwrap();
        assert_eq!(pdf.anotacoes()[0].opacidade, 1.0);
        pdf.definir_opacidade(1, 0, 0.4).unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert!((pdf.anotacoes()[0].opacidade - 0.4).abs() < 1e-4);
        let mut pdf = pdf;
        pdf.definir_opacidade(1, 0, 0.0).unwrap();
        assert!((pdf.anotacoes()[0].opacidade - 0.1).abs() < 1e-4, "nunca some de vez");
        pdf.definir_opacidade(1, 0, 1.0).unwrap();
        assert_eq!(pdf.anotacoes()[0].opacidade, 1.0);
        assert!(pdf.definir_opacidade(1, 0, f32::NAN).is_err());
        assert!(pdf.definir_opacidade(1, 9, 0.5).is_err());
    }

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
    fn editar_comentario_troca_o_texto_e_refaz_a_caixa() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Texto) Tj ET");
        pdf.anotar(1, &AnotacaoNova::Nota { ponto: (300.0, 700.0), texto: "Antes".into() }, "Ana").unwrap();
        pdf.anotar(1, &AnotacaoNova::CaixaTexto { caixa: [72.0, 500.0, 300.0, 560.0], texto: "Velho".into() }, "Ana").unwrap();
        pdf.responder(1, 0, "Resposta", "Bia").unwrap();
        pdf.editar_comentario(1, 0, "Depois").unwrap();
        pdf.editar_comentario(1, 1, "Caixa nova").unwrap();
        assert!(pdf.editar_comentario(1, 1, "  ").is_err(), "caixa de texto vazia");
        assert!(pdf.editar_comentario(1, 9, "x").is_err());
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let lista = pdf.anotacoes();
        assert_eq!(lista.len(), 3, "a caixa refeita não fica sobrando na página: {lista:?}");
        assert_eq!((lista[0].texto.as_str(), lista[0].autor.as_str()), ("Depois", "Ana"));
        assert_eq!(lista[1].texto, "Caixa nova");
        assert_eq!((lista[2].responde_a, lista[2].texto.as_str()), (Some(0), "Resposta"), "a resposta continua ligada");
        // A aparência da caixa desenha o texto novo (hexadecimal na fonte Helvetica).
        let id = pdf.id_pagina(1).unwrap();
        let caixa = pdf.lista_anotacoes(id)[1].as_reference().unwrap();
        let ap = pdf.documento.get_dictionary(caixa).unwrap().get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap().as_reference().unwrap();
        let fluxo = String::from_utf8_lossy(&pdf.documento.get_object(ap).unwrap().as_stream().unwrap().content).to_string();
        let hexa_novo: String = "Caixa".bytes().map(|b| format!("{b:02X}")).collect();
        assert!(fluxo.to_uppercase().contains(&hexa_novo), "{fluxo}");
    }

    #[test]
    fn substituicao_aceita_troca_o_texto() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Valor 1742 total) Tj ET");
        let (caixas, texto) = pdf.caixas_de_texto(1, [100.0, 695.0, 130.0, 712.0]).unwrap();
        assert_eq!(texto.trim(), "1742");
        pdf.substituir_texto(1, caixas, texto, "123", "Ana").unwrap();
        pdf.marcar_estado(1, 1, "Accepted", "Bia").unwrap();
        let linha = pdf.objetos(1).unwrap().into_iter().find_map(|o| match o.tipo { crate::TipoObjeto::Texto { texto, .. } => Some(texto), _ => None }).unwrap();
        assert_eq!(linha, "Valor 123 total");
        // O riscado sai; a inserção fica, aceita.
        let lista = pdf.anotacoes();
        assert!(!lista.iter().any(|a| a.tipo == "Riscado"), "{lista:?}");
        let ins = lista.iter().find(|a| a.tipo == "Inserção").unwrap();
        assert_eq!(estado_atual(&lista, ins), Some("Accepted"));
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
