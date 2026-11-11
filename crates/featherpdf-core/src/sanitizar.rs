//! Limpar e sanitizar antes de mandar o PDF para fora: metadados, histórico de edição,
//! anexos, JavaScript, autores dos comentários, camadas ocultas e links externos.
//! Devolve o relatório do que saiu, uma linha por item.

use std::collections::HashSet;

use lopdf::content::Operation;
use lopdf::{Dictionary, Object, ObjectId};

use crate::DocumentoPdf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpcoesLimpeza {
    pub metadados: bool,
    pub historico: bool,
    pub anexos: bool,
    pub javascript: bool,
    pub autores: bool,
    pub camadas_ocultas: bool,
    pub links_externos: bool,
    /// Tira os comentários inteiros (notas, marcações, desenhos de revisão); links e campos ficam.
    pub comentarios: bool,
}

impl Default for OpcoesLimpeza {
    fn default() -> Self {
        Self { metadados: true, historico: true, anexos: true, javascript: true, autores: true, camadas_ocultas: true, links_externos: true, comentarios: false }
    }
}

/// Ação que roda código ou sai do documento.
fn acao_perigosa(d: &Dictionary, js: bool, externos: bool) -> bool {
    let s = d.get(b"S").and_then(Object::as_name).unwrap_or(b"");
    // /Rendition e outras levam script em /JS mesmo sem /S /JavaScript.
    (js && (s == b"JavaScript" || s == b"RichMediaExecute" || d.has(b"JS"))) || (externos && matches!(s, b"URI" | b"Launch" | b"GoToR" | b"GoToE" | b"SubmitForm" | b"ImportData"))
}

/// O JPEG sem os segmentos APP1 (EXIF/XMP) e APP13 (IPTC); `None` se não tinha ou não deu para ler.
fn sem_exif(jpeg: &[u8]) -> Option<Vec<u8>> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) { return None; }
    let (mut saida, mut i, mut tirou) = (vec![0xFF, 0xD8], 2, false);
    loop {
        if *jpeg.get(i)? != 0xFF { return None; }
        let marca = *jpeg.get(i + 1)?;
        // Do SOS em diante são os dados da imagem: vão inteiros.
        if marca == 0xDA { saida.extend_from_slice(&jpeg[i..]); break; }
        let n = u16::from_be_bytes([*jpeg.get(i + 2)?, *jpeg.get(i + 3)?]) as usize;
        let fim = i + 2 + n;
        if n < 2 || fim > jpeg.len() { return None; }
        if matches!(marca, 0xE1 | 0xED) { tirou = true; } else { saida.extend_from_slice(&jpeg[i..fim]); }
        i = fim;
    }
    tirou.then_some(saida)
}

impl DocumentoPdf {
    fn dicionario<'a>(&'a self, o: &'a Object) -> Option<&'a Dictionary> {
        match o { Object::Reference(r) => self.documento.get_dictionary(*r).ok(), o => o.as_dict().ok() }
    }

    /// Para a cópia que vai à impressora: com `imprimir`, liga a marca "imprimir" (/F 4) dos
    /// comentários (sem ela o leitor não os põe no papel); sem, tira os comentários. Devolve quantos.
    pub fn comentarios_na_impressao(&mut self, imprimir: bool) -> Result<usize, String> {
        let todos = self.anotacoes();
        if !imprimir {
            let nada = OpcoesLimpeza { metadados: false, historico: false, anexos: false, javascript: false, autores: false,
                camadas_ocultas: false, links_externos: false, comentarios: true };
            self.sanitizar(nada)?;
            return Ok(todos.len().saturating_sub(self.anotacoes().len()));
        }
        // Página por página, uma passada (antes: a árvore de páginas inteira por comentário).
        let mut alvos = Vec::new();
        for (_, id) in self.documento.get_pages() {
            for o in self.lista_anotacoes(id) {
                let Ok(r) = o.as_reference() else { continue };
                let comentario = self.documento.get_dictionary(r).ok()
                    .and_then(|d| d.get(b"Subtype").and_then(Object::as_name).ok())
                    .is_some_and(|s| crate::anotacoes::tipo_de_comentario(s).is_some());
                if comentario { alvos.push(r); }
            }
        }
        let mut n = 0;
        for r in alvos {
            let Ok(d) = self.documento.get_dictionary_mut(r) else { continue };
            let f = d.get(b"F").and_then(Object::as_i64).unwrap_or(0);
            // Escondida (2) fica escondida; o resto ganha a marca de impressão.
            if f & 2 == 0 && f & 4 == 0 { d.set("F", f | 4); n += 1; }
        }
        if n > 0 { self.modificado = true; }
        Ok(n)
    }

    pub fn sanitizar(&mut self, op: OpcoesLimpeza) -> Result<Vec<String>, String> {
        let mut relatorio = Vec::new();
        let ids = self.achatar()?;
        if op.metadados {
            let info = self.documento.trailer.remove(b"Info").is_some();
            let xmp = self.documento.catalog_mut().map(|c| c.remove(b"Metadata").is_some()).unwrap_or(false);
            if info || xmp { relatorio.push("Metadados do documento (autor, título, programa, datas).".into()); }
            // Imagens e fontes trazem o próprio XMP; a foto JPEG traz EXIF (GPS, câmera) e IPTC.
            let mut n = 0;
            for o in self.documento.objects.values_mut() {
                let Object::Stream(st) = o else { continue };
                if st.dict.remove(b"Metadata").is_some() { n += 1; }
                let dct = match st.dict.get(b"Filter") {
                    Ok(Object::Name(f)) => f == b"DCTDecode",
                    Ok(Object::Array(v)) => v.len() == 1 && v[0].as_name().is_ok_and(|f| f == b"DCTDecode"),
                    _ => false,
                };
                if let Some(limpo) = dct.then(|| sem_exif(&st.content)).flatten() { st.set_content(limpo); n += 1; }
            }
            if n > 0 { relatorio.push(format!("Metadados de {n} imagem(ns) ou fonte(s) (EXIF, XMP).")); }
        }
        if op.historico {
            let mut n = 0;
            if self.documento.trailer.remove(b"Prev").is_some() { n += 1; }
            if let Ok(c) = self.documento.catalog_mut() { for k in [&b"PieceInfo"[..], b"LastModified"] { if c.remove(k).is_some() { n += 1; } } }
            for &id in &ids {
                if let Ok(p) = self.documento.get_dictionary_mut(id) {
                    for k in [&b"PieceInfo"[..], b"LastModified", b"Thumb", b"Metadata"] { if p.remove(k).is_some() { n += 1; } }
                }
            }
            if n > 0 { relatorio.push(format!("Histórico de edição: {n} item(ns) (dados privados de programas, miniaturas, revisões anteriores).")); }
        }
        if op.anexos {
            let antes = self.anexos().len();
            crate::otimizar::tirar_anexos(&mut self.documento);
            if antes > 0 { relatorio.push(format!("{antes} arquivo(s) anexado(s).")); }
        }
        if op.javascript || op.links_externos { self.tirar_acoes(&ids, op, &mut relatorio); }
        if op.autores {
            let mut n = 0;
            for &id in &ids {
                for o in self.lista_anotacoes(id) {
                    let Ok(r) = o.as_reference() else { continue };
                    let Ok(d) = self.documento.get_dictionary_mut(r) else { continue };
                    // No campo de formulário o /T é o nome do campo, não o autor.
                    if d.get(b"Subtype").and_then(Object::as_name).is_ok_and(|s| s == b"Widget") || d.has(b"FT") { continue; }
                    if d.remove(b"T").is_some() { n += 1; }
                }
            }
            if n > 0 { relatorio.push(format!("Autor de {n} comentário(s).")); }
        }
        if op.comentarios {
            let mut n = 0;
            for &id in &ids {
                let mut lista = self.lista_anotacoes(id);
                let antes = lista.len();
                // Só os tipos de comentário: vídeo, som, 3D, links e campos ficam.
                const COMENTARIOS: &[&[u8]] = &[b"Text", b"FreeText", b"Highlight", b"Underline", b"StrikeOut", b"Squiggly", b"Ink",
                    b"Square", b"Circle", b"Line", b"Polygon", b"PolyLine", b"Stamp", b"Caret", b"Popup"];
                lista.retain(|o| self.dicionario(o).is_none_or(|d| d.has(b"FT")
                    || !d.get(b"Subtype").and_then(Object::as_name).is_ok_and(|s| COMENTARIOS.contains(&s))));
                n += antes - lista.len();
                if antes != lista.len() { if let Ok(p) = self.documento.get_dictionary_mut(id) { p.set("Annots", lista); } }
            }
            if n > 0 { relatorio.push(format!("{n} comentário(s) e marcação(ões).")); }
        }
        if op.camadas_ocultas {
            let (camadas, blocos) = self.tirar_camadas_ocultas(&ids)?;
            if camadas > 0 { relatorio.push(format!("{camadas} camada(s) oculta(s), com {blocos} trecho(s) de conteúdo.")); }
        }
        self.modificado = true;
        Ok(relatorio)
    }

    /// Na ação `o` ou nas seguintes dela (/Next): `Some(true)` se roda JavaScript,
    /// `Some(false)` se sai do documento, `None` se nenhuma das duas.
    fn acao_na_cadeia(&self, o: &Object, op: OpcoesLimpeza) -> Option<bool> {
        let mut fila = vec![o];
        let mut vistos = HashSet::new();
        for _ in 0..4096 {
            let o = fila.pop()?;
            match o {
                // /Next que volta para trás não roda de novo.
                Object::Reference(r) => if vistos.insert(*r) { fila.extend(self.documento.get_object(*r).ok()) },
                Object::Array(v) => fila.extend(v.iter()),
                Object::Dictionary(d) => {
                    if acao_perigosa(d, op.javascript, false) { return Some(true); }
                    if acao_perigosa(d, false, op.links_externos) { return Some(false); }
                    fila.extend(d.get(b"Next").ok());
                }
                _ => {}
            }
        }
        // Cadeia longa demais para olhar inteira: na dúvida, é perigosa.
        (!fila.is_empty()).then_some(op.javascript)
    }

    /// JavaScript (nomes, abertura, ações automáticas) e links para fora do documento.
    fn tirar_acoes(&mut self, ids: &[ObjectId], op: OpcoesLimpeza, relatorio: &mut Vec<String>) {
        let (mut js, mut links) = (0, 0);
        if op.javascript {
            let nomes = self.documento.catalog().ok().and_then(|c| c.get(b"Names").ok()).cloned();
            match nomes {
                Some(Object::Reference(r)) => if let Ok(d) = self.documento.get_dictionary_mut(r) { if d.remove(b"JavaScript").is_some() { js += 1; } },
                Some(Object::Dictionary(mut d)) => if d.remove(b"JavaScript").is_some() {
                    js += 1;
                    if let Ok(c) = self.documento.catalog_mut() { c.set("Names", d); }
                },
                _ => {}
            }
            // Formulário XFA traz os próprios scripts; sem ele o leitor usa o AcroForm comum.
            let form = self.documento.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).cloned();
            let form = match form {
                Some(Object::Reference(r)) => self.documento.get_dictionary_mut(r).ok(),
                Some(Object::Dictionary(_)) => self.documento.catalog_mut().ok().and_then(|c| c.get_mut(b"AcroForm").ok()).and_then(|f| f.as_dict_mut().ok()),
                _ => None,
            };
            if let Some(f) = form { if f.remove(b"XFA").is_some() { js += 1; } f.remove(b"NeedsRendering"); }
            if let Ok(c) = self.documento.catalog_mut() { c.remove(b"NeedsRendering"); }
        }
        for &id in ids {
            let mut lista = self.lista_anotacoes(id);
            let antes = lista.len();
            lista.retain(|o| {
                let Some(d) = self.dicionario(o) else { return true };
                let link = d.get(b"Subtype").and_then(Object::as_name).is_ok_and(|s| s == b"Link");
                !(link && op.links_externos && d.get(b"A").ok().and_then(|a| self.dicionario(a)).is_some_and(|a| acao_perigosa(a, false, true)))
            });
            links += antes - lista.len();
            if antes != lista.len() { if let Ok(p) = self.documento.get_dictionary_mut(id) { p.set("Annots", lista); } }
        }
        // Uma passada em todos os dicionários (catálogo, páginas, anotações, campos, marcadores):
        // ação perigosa em /A, /OpenAction, /Next (seguindo a cadeia) ou /AA sai.
        crate::podar(&mut self.documento);
        let alvos: Vec<ObjectId> = self.documento.objects.keys().copied().collect();
        for id in alvos {
            let d = match self.documento.get_object(id) { Ok(Object::Dictionary(d)) => d, Ok(Object::Stream(s)) => &s.dict, _ => continue };
            let mut tirar: Vec<(&[u8], bool)> = [&b"A"[..], b"OpenAction", b"Next"].into_iter()
                .filter_map(|k| Some((k, self.acao_na_cadeia(d.get(k).ok()?, op)?))).collect();
            if let Ok(aa) = d.get(b"AA") {
                let pior = self.dicionario(aa).into_iter().flat_map(|x| x.iter()).filter_map(|(_, v)| self.acao_na_cadeia(v, op)).max();
                if op.javascript { tirar.push((b"AA", true)); } else if let Some(j) = pior { tirar.push((b"AA", j)); }
            }
            if tirar.is_empty() { continue; }
            let d = match self.documento.get_object_mut(id) { Ok(Object::Dictionary(d)) => d, Ok(Object::Stream(s)) => &mut s.dict, _ => continue };
            for (k, eh_js) in tirar { d.remove(k); if eh_js { js += 1 } else { links += 1 } }
        }
        if js > 0 { relatorio.push(format!("{js} ação(ões) JavaScript.")); }
        if links > 0 { relatorio.push(format!("{links} link(s) para fora do documento (sites, outros arquivos, envio de formulário).")); }
    }

    /// Camadas usadas fora do conteúdo das páginas: /OC de grupos, imagens e anotações,
    /// /Properties de Form XObject e membros de OCMD.
    fn camadas_fora_das_paginas(&self) -> HashSet<ObjectId> {
        let mut usadas = HashSet::new();
        let juntar = |o: &Object, usadas: &mut HashSet<ObjectId>| match self.dicionario(o) {
            Some(d) if d.get(b"Type").and_then(Object::as_name).is_ok_and(|t| t == b"OCMD") => match d.get(b"OCGs") {
                Ok(Object::Array(v)) => usadas.extend(v.iter().filter_map(|g| g.as_reference().ok())),
                Ok(Object::Reference(r)) => { usadas.insert(*r); }
                _ => {}
            },
            _ => if let Ok(r) = o.as_reference() { usadas.insert(r); },
        };
        for (&id, o) in &self.documento.objects {
            let (d, forma) = match o {
                Object::Dictionary(d) => (d, false),
                Object::Stream(s) => (&s.dict, s.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|t| t == b"Form")),
                _ => continue,
            };
            if let Ok(oc) = d.get(b"OC") { juntar(oc, &mut usadas); }
            if d.get(b"Type").and_then(Object::as_name).is_ok_and(|t| t == b"OCMD") { juntar(&Object::Reference(id), &mut usadas); }
            if forma {
                let props = d.get(b"Resources").ok().and_then(|r| self.dicionario(r))
                    .and_then(|r| r.get(b"Properties").ok()).and_then(|p| self.dicionario(p));
                for (_, v) in props.into_iter().flat_map(|p| p.iter()) { juntar(v, &mut usadas); }
            }
        }
        usadas
    }

    /// Camadas desligadas na configuração padrão: o conteúdo marcado com elas sai das
    /// páginas (BDC /OC … EMC) e a camada sai da lista. Devolve (camadas, trechos).
    fn tirar_camadas_ocultas(&mut self, ids: &[ObjectId]) -> Result<(usize, usize), String> {
        let ocultas: HashSet<ObjectId> = self.camadas().into_iter().filter(|c| !c.visivel).map(|c| c.id).collect();
        if ocultas.is_empty() { return Ok((0, 0)); }
        let (mut blocos, mut presas) = (0, HashSet::new());
        for &id in ids {
            // Nome da propriedade nos recursos da página → camada.
            let nomes: std::collections::HashMap<Vec<u8>, ObjectId> = self.documento.get_dictionary(id).ok()
                .and_then(|p| p.get(b"Resources").ok()).and_then(|r| self.dicionario(r))
                .and_then(|r| r.get(b"Properties").ok()).and_then(|p| self.dicionario(p))
                .map(|p| p.iter().filter_map(|(k, v)| Some((k.clone(), v.as_reference().ok().filter(|r| ocultas.contains(r))?))).collect())
                .unwrap_or_default();
            if nomes.is_empty() { continue; }
            let Ok(conteudo) = self.documento.get_and_decode_page_content(id) else {
                // Conteúdo que não deu para ler continua com a camada: ela fica na lista, oculta.
                presas.extend(nomes.values());
                continue;
            };
            let mut novas: Vec<Operation> = Vec::with_capacity(conteudo.operations.len());
            let mut pular = 0usize;
            for o in conteudo.operations {
                let abre = matches!(o.operator.as_str(), "BDC" | "BMC");
                if pular > 0 {
                    if abre { pular += 1; } else if o.operator == "EMC" { pular -= 1; }
                    continue;
                }
                let oculto = o.operator == "BDC" && o.operands.first().and_then(|n| n.as_name().ok()) == Some(b"OC")
                    && o.operands.get(1).and_then(|n| n.as_name().ok()).is_some_and(|n| nomes.contains_key(n));
                if oculto { pular = 1; blocos += 1; continue; }
                novas.push(o);
            }
            // O `Content::encode` do lopdf estraga imagem embutida.
            let bytes = crate::objetos::codificar(&novas)?;
            self.documento.change_page_content(id, bytes).map_err(|e| e.to_string())?;
        }
        // Tira das listas só as camadas cujo conteúdo saiu: as que ficaram em grupos, anotações
        // ou OCMD (ou em página ilegível) continuam ocultas; sem a camada, o leitor as mostraria.
        presas.extend(self.camadas_fora_das_paginas());
        let ocultas: HashSet<ObjectId> = ocultas.difference(&presas).copied().collect();
        if ocultas.is_empty() { return Ok((0, blocos)); }
        let props = self.documento.catalog().ok().and_then(|c| c.get(b"OCProperties").ok()).cloned();
        let limpar = |v: &Object| -> Object {
            match v { Object::Array(a) => Object::Array(a.iter().filter(|o| !o.as_reference().is_ok_and(|r| ocultas.contains(&r))).cloned().collect()), o => o.clone() }
        };
        let limpar_dic = |d: &Dictionary| -> Dictionary {
            let mut n = d.clone();
            for (k, v) in d.iter() { n.set(k.clone(), limpar(v)); }
            n
        };
        match props {
            Some(Object::Reference(r)) => if let Ok(d) = self.documento.get_dictionary(r).cloned() {
                let mut d = limpar_dic(&d);
                if let Ok(Object::Dictionary(padrao)) = d.get(b"D").cloned() { d.set("D", limpar_dic(&padrao)); }
                self.documento.objects.insert(r, Object::Dictionary(d));
            },
            Some(Object::Dictionary(d)) => {
                let mut d = limpar_dic(&d);
                if let Ok(Object::Dictionary(padrao)) = d.get(b"D").cloned() { d.set("D", limpar_dic(&padrao)); }
                self.documento.catalog_mut().map_err(|e| e.to_string())?.set("OCProperties", d);
            }
            _ => {}
        }
        Ok((ocultas.len(), blocos))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::objetos::testes::pdf_com;
    use lopdf::dictionary;

    #[test]
    fn sanitizar_tira_acao_em_cadeia_e_widget_e_guarda_imagem_embutida() {
        use crate::objetos::testes::no_arquivo;
        let mut pdf = pdf_com("q 9 0 0 9 9 9 cm BI /W 3 /H 1 /BPC 8 /CS /DeviceGray ID abc EI Q /OC /Oc1 BDC BT /F1 12 Tf 72 600 Td (Segredo) Tj ET EMC");
        let g = pdf.documento.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("R") });
        pdf.documento.catalog_mut().unwrap().set("OCProperties", dictionary! {
            "OCGs" => vec![Object::Reference(g)], "D" => dictionary! { "OFF" => vec![Object::Reference(g)] } });
        let id = pdf.id_pagina(1).unwrap();
        pdf.documento.get_dictionary_mut(id).unwrap().get_mut(b"Resources").unwrap().as_dict_mut().unwrap()
            .set("Properties", dictionary! { "Oc1" => Object::Reference(g) });
        // Abertura inofensiva com Launch no /Next; widget com URI.
        let lanca = pdf.documento.add_object(dictionary! { "S" => "Launch", "F" => Object::string_literal("LANCAR.exe") });
        pdf.documento.catalog_mut().unwrap().set("OpenAction", dictionary! { "S" => "GoTo", "D" => vec![Object::Reference(id), "Fit".into()], "Next" => lanca });
        pdf.criar_campo(1, [100.0, 400.0, 250.0, 420.0], &crate::CampoNovo::Texto, "Campo").unwrap();
        let w = pdf.lista_anotacoes(id)[0].as_reference().unwrap();
        pdf.documento.get_dictionary_mut(w).unwrap().set("A", dictionary! { "S" => "URI", "URI" => Object::string_literal("https://SITEFORA.example") });
        pdf.sanitizar(OpcoesLimpeza::default()).unwrap();
        let bytes = pdf.serializar().unwrap();
        assert!(!no_arquivo(&bytes, b"LANCAR") && !no_arquivo(&bytes, b"SITEFORA") && !no_arquivo(&bytes, b"Segredo"));
        let pdf = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        let conteudo = pdf.documento.get_page_content(pdf.id_pagina(1).unwrap());
        assert!(conteudo.windows(6).any(|w| w == b"ID abc"), "{}", String::from_utf8_lossy(&conteudo));
    }

    #[test]
    fn sanitizar_pega_js_escondido() {
        use crate::objetos::testes::no_arquivo;
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (oi) Tj ET");
        let id = pdf.id_pagina(1).unwrap();
        // JS no fim de um /Next com 300 ações inofensivas antes (e diretas, fora da passada global).
        let mut proximas: Vec<Object> = (0..300).map(|_| Object::Dictionary(dictionary! { "S" => "GoTo", "D" => vec![Object::Reference(id), "Fit".into()] })).collect();
        proximas.push(Object::Dictionary(dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("CADEIAJS()") }));
        pdf.documento.catalog_mut().unwrap().set("OpenAction", dictionary! { "S" => "GoTo", "D" => vec![Object::Reference(id), "Fit".into()], "Next" => proximas });
        // Rendition com /JS e formulário XFA.
        pdf.documento.get_dictionary_mut(id).unwrap().set("AA", dictionary! { "O" => dictionary! { "S" => "Rendition", "OP" => 4, "JS" => Object::string_literal("RENDJS()") } });
        pdf.documento.catalog_mut().unwrap().set("AcroForm", dictionary! { "Fields" => Vec::<Object>::new(), "XFA" => Object::string_literal("<script>XFAJS()</script>") });
        let op = OpcoesLimpeza { javascript: true, ..OpcoesLimpeza::default() };
        pdf.sanitizar(op).unwrap();
        let bytes = pdf.serializar().unwrap();
        for js in [&b"CADEIAJS"[..], b"RENDJS", b"XFAJS"] { assert!(!no_arquivo(&bytes, js), "{}", String::from_utf8_lossy(js)); }
        // Rendition sem /JS, só na cadeia, também conta.
        let d = dictionary! { "S" => "Rendition", "JS" => Object::string_literal("x") };
        assert!(acao_perigosa(&d, true, false));
    }

    #[test]
    fn sanitizar_tira_comentarios_e_deixa_link() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (oi) Tj ET");
        let id = pdf.id_pagina(1).unwrap();
        let nota = pdf.documento.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Text", "Contents" => Object::string_literal("nota") });
        let link = pdf.documento.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link" });
        let video = pdf.documento.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Screen" });
        pdf.documento.get_dictionary_mut(id).unwrap().set("Annots", vec![Object::Reference(nota), Object::Reference(link), Object::Reference(video)]);
        assert!(pdf.sanitizar(OpcoesLimpeza::default()).unwrap().iter().all(|r| !r.contains("comentário(s) e")), "por padrão os comentários ficam");
        let r = pdf.sanitizar(OpcoesLimpeza { comentarios: true, ..OpcoesLimpeza::default() }).unwrap();
        assert!(r.iter().any(|r| r.starts_with("1 comentário")), "{r:?}");
        assert_eq!(pdf.lista_anotacoes(id), vec![Object::Reference(link), Object::Reference(video)], "link e vídeo ficam");
    }

    #[test]
    fn comentarios_na_impressao_liga_ou_tira() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (oi) Tj ET");
        let id = pdf.id_pagina(1).unwrap();
        let nota = pdf.documento.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![0.into(), 0.into(), 9.into(), 9.into()] });
        let escondida = pdf.documento.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square", "F" => 2, "Rect" => vec![0.into(), 0.into(), 9.into(), 9.into()] });
        let link = pdf.documento.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link" });
        pdf.documento.get_dictionary_mut(id).unwrap().set("Annots", vec![Object::Reference(nota), Object::Reference(escondida), Object::Reference(link)]);
        let mut com = pdf.clone();
        assert_eq!(com.comentarios_na_impressao(true).unwrap(), 1);
        assert_eq!(com.documento.get_dictionary(nota).unwrap().get(b"F").unwrap().as_i64().unwrap(), 4);
        assert_eq!(com.documento.get_dictionary(escondida).unwrap().get(b"F").unwrap().as_i64().unwrap(), 2);
        assert_eq!(pdf.comentarios_na_impressao(false).unwrap(), 2);
        assert_eq!(pdf.lista_anotacoes(id), vec![Object::Reference(link)], "o link fica");
    }

    #[test]
    fn camada_usada_num_grupo_continua_oculta() {
        let mut pdf = pdf_com("/OC /Oc1 BDC BT /F1 12 Tf 72 600 Td (Segredo) Tj ET EMC q /Fm1 Do Q");
        let g = pdf.documento.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("R") });
        pdf.documento.catalog_mut().unwrap().set("OCProperties", dictionary! {
            "OCGs" => vec![Object::Reference(g)], "D" => dictionary! { "OFF" => vec![Object::Reference(g)] } });
        let id = pdf.id_pagina(1).unwrap();
        let fm = pdf.documento.add_object(lopdf::Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 500.into(), 500.into()],
            "Resources" => dictionary! { "Properties" => dictionary! { "Oc1" => Object::Reference(g) } } },
            b"/OC /Oc1 BDC BT /F1 12 Tf 10 10 Td (NoGrupo) Tj ET EMC".to_vec()));
        let rec = pdf.documento.get_dictionary_mut(id).unwrap().get_mut(b"Resources").unwrap().as_dict_mut().unwrap();
        rec.set("Properties", dictionary! { "Oc1" => Object::Reference(g) });
        rec.get_mut(b"XObject").unwrap().as_dict_mut().unwrap().set("Fm1", fm);
        pdf.sanitizar(OpcoesLimpeza::default()).unwrap();
        // O conteúdo dentro do grupo não saiu: a camada tem de continuar na lista, desligada.
        assert!(pdf.camadas().iter().any(|c| c.id == g && !c.visivel));
        assert!(!pdf.texto_pagina(1).unwrap().contains("Segredo"));
    }

    #[test]
    fn jpeg_perde_exif_e_guarda_o_resto() {
        let app1 = [0xFF, 0xE1, 0x00, 0x06, b'E', b'x', b'i', b'f'];
        let app0 = [0xFF, 0xE0, 0x00, 0x04, 1, 2];
        let sos = [0xFF, 0xDA, 0x00, 0x02, 9, 9, 0xFF, 0xD9];
        let jpeg = [&[0xFF, 0xD8][..], &app0, &app1, &sos].concat();
        assert_eq!(sem_exif(&jpeg).unwrap(), [&[0xFF, 0xD8][..], &app0, &sos].concat());
        assert_eq!(sem_exif(&[&[0xFF, 0xD8][..], &app0, &sos].concat()), None);
        assert_eq!(sem_exif(&[0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF]), None);
    }

    #[test]
    fn sanitizar_tira_tudo_e_relata() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Visivel) Tj ET /OC /Oc1 BDC BT /F1 12 Tf 72 600 Td (Segredo) Tj ET EMC");
        let g = pdf.documento.add_object(dictionary! { "Type" => "OCG", "Name" => lopdf::text_string("Rascunho") });
        pdf.documento.catalog_mut().unwrap().set("OCProperties", dictionary! {
            "OCGs" => vec![Object::Reference(g)], "D" => dictionary! { "OFF" => vec![Object::Reference(g)] } });
        let id = pdf.id_pagina(1).unwrap();
        let mut rec = pdf.documento.get_dictionary(id).unwrap().get(b"Resources").ok().and_then(|r| pdf.dicionario(r)).cloned().unwrap_or_default();
        rec.set("Properties", dictionary! { "Oc1" => Object::Reference(g) });
        pdf.documento.get_dictionary_mut(id).unwrap().set("Resources", rec);
        pdf.documento.catalog_mut().unwrap().set("OpenAction", dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") });
        pdf.documento.trailer.set("Info", dictionary! { "Author" => Object::string_literal("Fulano") });
        pdf.inserir_link(1, [10.0, 10.0, 50.0, 30.0], crate::Destino::Endereco("https://exemplo.com".into())).unwrap();
        pdf.anotar(1, &crate::AnotacaoNova::Nota { ponto: (300.0, 700.0), texto: "oi".into() }, "Ana").unwrap();
        let relatorio = pdf.sanitizar(OpcoesLimpeza::default()).unwrap();
        assert_eq!(relatorio.len(), 5, "{relatorio:?}");
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let texto = pdf.texto_pagina(1).unwrap();
        assert!(texto.contains("Visivel") && !texto.contains("Segredo"), "{texto}");
        assert!(pdf.camadas().is_empty());
        assert_eq!(pdf.anotacoes().iter().map(|a| a.autor.as_str()).collect::<Vec<_>>(), [""]);
        assert!(pdf.documento.catalog().unwrap().get(b"OpenAction").is_err());
        assert!(pdf.documento.trailer.get(b"Info").is_err());
    }
}
