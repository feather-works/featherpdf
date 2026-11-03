//! Marcadores (o sumário do PDF) e arquivos anexados, para os painéis laterais.

use lopdf::{Dictionary, Object};

use super::DocumentoPdf;

#[derive(Debug, Clone, PartialEq)]
pub struct Marcador {
    /// 1 = primeiro nível.
    pub nivel: usize,
    pub titulo: String,
    pub pagina: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anexo {
    pub nome: String,
    pub tamanho: usize,
}

fn resolver<'a>(doc: &'a lopdf::Document, o: &'a Object) -> Option<&'a Object> {
    match o { Object::Reference(r) => doc.get_object(*r).ok(), o => Some(o) }
}

impl DocumentoPdf {
    /// O sumário na ordem do PDF (vazio quando o arquivo não tem).
    pub fn marcadores(&self) -> Vec<Marcador> {
        // O `get_toc` lê título sem BOM como UTF-8 ("POL\u{FFFD}CIA"); o certo é PDFDocEncoding.
        // Ele resolve os destinos, então só os títulos são trocados pelos lidos aqui.
        let titulos = self.titulos_do_sumario();
        self.documento.get_toc().map(|t| t.toc.into_iter()
            .map(|m| Marcador { nivel: m.level.max(1), titulo: titulos.get(&m.title).cloned().unwrap_or(m.title), pagina: m.page as u32 }).collect())
            .unwrap_or_default()
    }

    /// Título como o `get_toc` o deixa → título decodificado direito, para todo item do sumário.
    fn titulos_do_sumario(&self) -> std::collections::HashMap<String, String> {
        let doc = &self.documento;
        let mut mapa = std::collections::HashMap::new();
        let primeiro = |d: &Dictionary| d.get(b"First").ok().and_then(|o| o.as_reference().ok());
        let mut pilha: Vec<lopdf::ObjectId> = doc.catalog().ok().and_then(|c| c.get(b"Outlines").ok())
            .and_then(|o| resolver(doc, o)).and_then(|o| o.as_dict().ok()).and_then(primeiro).into_iter().collect();
        let mut vistos = std::collections::HashSet::new();
        while let Some(id) = pilha.pop() {
            if !vistos.insert(id) { continue; }
            let Ok(d) = doc.get_dictionary(id) else { continue };
            if let Ok(bruto) = d.get(b"Title").and_then(Object::as_str) {
                if !bruto.starts_with(&[0xFE, 0xFF]) && !bruto.starts_with(&[0xFF, 0xFE]) {
                    if let Ok(certo) = lopdf::decode_text_string(&Object::string_literal(bruto.to_vec())) {
                        mapa.insert(String::from_utf8_lossy(bruto).into_owned(), certo);
                    }
                }
            }
            if let Ok(r) = d.get(b"Next").and_then(Object::as_reference) { pilha.push(r); }
            if let Some(r) = primeiro(d) { pilha.push(r); }
        }
        mapa
    }

    /// Links da página (começando em 1): caixa em pontos do PDF e para onde vão. Só endereço
    /// http(s)/mailto e página do próprio documento; o resto (abrir arquivo, rodar programa) fica de fora.
    pub fn links(&self, pagina: u32) -> Vec<([f32; 4], crate::Destino)> {
        let doc = &self.documento;
        let Ok(id) = self.id_pagina(pagina) else { return Vec::new() };
        let numeros: std::collections::HashMap<lopdf::ObjectId, u32> = self.ids_paginas().into_iter().zip(1..).collect();
        let mut saida = Vec::new();
        for anotacao in self.lista_anotacoes(id) {
            let Some(d) = resolver(doc, &anotacao).and_then(|o| o.as_dict().ok()) else { continue };
            if d.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Link") { continue; }
            let Ok(caixa) = d.get(b"Rect").and_then(Object::as_array) else { continue };
            let v: Vec<f32> = caixa.iter().filter_map(|n| resolver(doc, n)).filter_map(|n| n.as_float().ok()).collect();
            let [x0, y0, x1, y1] = v[..] else { continue };
            let caixa = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
            let acao = d.get(b"A").ok().and_then(|a| resolver(doc, a)).and_then(|a| a.as_dict().ok());
            let destino = match acao {
                Some(a) => match a.get(b"S").and_then(Object::as_name).ok() {
                    Some(b"URI") => a.get(b"URI").ok().and_then(|u| resolver(doc, u)).and_then(|u| u.as_str().ok())
                        .map(|u| String::from_utf8_lossy(u).trim().to_string())
                        .filter(|u| ["http://", "https://", "mailto:"].iter().any(|e| u.to_ascii_lowercase().starts_with(e)))
                        .map(crate::Destino::Endereco),
                    Some(b"GoTo") => a.get(b"D").ok().and_then(|x| self.pagina_do_destino(x, &numeros)).map(crate::Destino::Pagina),
                    _ => None,
                },
                None => d.get(b"Dest").ok().and_then(|x| self.pagina_do_destino(x, &numeros)).map(crate::Destino::Pagina),
            };
            if let Some(destino) = destino { saida.push((caixa, destino)); }
        }
        saida
    }

    /// Página de um destino: `[página /Fit…]`, `<< /D [...] >>` ou um nome (catálogo `/Dests` ou árvore `/Names /Dests`).
    fn pagina_do_destino(&self, destino: &Object, numeros: &std::collections::HashMap<lopdf::ObjectId, u32>) -> Option<u32> {
        let doc = &self.documento;
        let mut atual = resolver(doc, destino)?;
        // Nome → valor → (dicionário com /D) → vetor; o limite evita volta em círculo.
        for _ in 0..4 {
            match atual {
                Object::Array(v) => return v.first().and_then(|p| p.as_reference().ok()).and_then(|p| numeros.get(&p).copied())
                    .or_else(|| v.first().and_then(|p| p.as_i64().ok()).and_then(|i| u32::try_from(i + 1).ok())),
                Object::Dictionary(d) => atual = d.get(b"D").ok().and_then(|x| resolver(doc, x))?,
                Object::Name(n) | Object::String(n, _) => atual = self.destino_com_nome(n)?,
                _ => return None,
            }
        }
        None
    }

    fn destino_com_nome(&self, nome: &[u8]) -> Option<&Object> {
        let doc = &self.documento;
        let catalogo = doc.catalog().ok()?;
        let antigo = catalogo.get(b"Dests").ok().and_then(|d| resolver(doc, d)).and_then(|d| d.as_dict().ok())
            .and_then(|d| d.get(nome).ok()).and_then(|o| resolver(doc, o));
        if antigo.is_some() { return antigo; }
        let raiz = catalogo.get(b"Names").ok().and_then(|n| resolver(doc, n)).and_then(|n| n.as_dict().ok())
            .and_then(|n| n.get(b"Dests").ok()).and_then(|d| resolver(doc, d)).and_then(|d| d.as_dict().ok())?;
        let mut pilha = vec![raiz];
        let mut visitados = 0;
        while let (Some(no), true) = (pilha.pop(), visitados < 10_000) {
            visitados += 1;
            if let Ok(nomes) = no.get(b"Names").and_then(Object::as_array) {
                for par in nomes.chunks(2) {
                    if let [chave, valor] = par {
                        if resolver(doc, chave).and_then(|c| c.as_str().ok()) == Some(nome) { return resolver(doc, valor); }
                    }
                }
            }
            if let Ok(filhos) = no.get(b"Kids").and_then(Object::as_array) {
                pilha.extend(filhos.iter().filter_map(|k| resolver(doc, k)).filter_map(|k| k.as_dict().ok()));
            }
        }
        None
    }

    /// Especificações de arquivo da árvore EmbeddedFiles, na ordem dos nomes.
    fn especificacoes(&self) -> Vec<(String, &Dictionary)> {
        let doc = &self.documento;
        let resolver = |o| resolver(doc, o);
        let Some(raiz) = doc.catalog().ok().and_then(|c| c.get(b"Names").ok()).and_then(resolver)
            .and_then(|n| n.as_dict().ok()).and_then(|n| n.get(b"EmbeddedFiles").ok()).and_then(resolver)
            .and_then(|e| e.as_dict().ok()) else { return Vec::new() };
        let mut saida = Vec::new();
        let mut pilha = vec![raiz];
        // O limite protege de uma árvore que aponta para si mesma.
        while let (Some(no), true) = (pilha.pop(), saida.len() < 10_000) {
            if let Ok(nomes) = no.get(b"Names").and_then(Object::as_array) {
                for par in nomes.chunks(2) {
                    let [nome, spec] = par else { continue };
                    let Some(spec) = resolver(spec).and_then(|s| s.as_dict().ok()) else { continue };
                    let nome = spec.get(b"UF").or_else(|_| spec.get(b"F")).ok()
                        .and_then(|n| lopdf::decode_text_string(n).ok())
                        .or_else(|| lopdf::decode_text_string(nome).ok())
                        .unwrap_or_else(|| "anexo".into());
                    saida.push((nome, spec));
                }
            }
            if let Ok(filhos) = no.get(b"Kids").and_then(Object::as_array) {
                pilha.extend(filhos.iter().rev().filter_map(resolver).filter_map(|k| k.as_dict().ok()));
            }
        }
        saida
    }

    fn conteudo_anexo(&self, spec: &Dictionary) -> Option<Vec<u8>> {
        let ef = spec.get(b"EF").ok()?;
        let ef = match ef { Object::Reference(r) => self.documento.get_object(*r).ok()?, o => o }.as_dict().ok()?;
        let id = ef.get(b"UF").or_else(|_| ef.get(b"F")).ok()?.as_reference().ok()?;
        let s = self.documento.get_object(id).ok()?.as_stream().ok()?;
        s.decompressed_content().ok().or_else(|| s.filters().ok().filter(|f| f.is_empty()).map(|_| s.content.clone()))
    }

    pub fn anexos(&self) -> Vec<Anexo> {
        self.especificacoes().into_iter().map(|(nome, spec)| {
            let tamanho = self.conteudo_anexo(spec).map_or(0, |b| b.len());
            Anexo { nome, tamanho }
        }).collect()
    }

    /// Nome e bytes do anexo `i` (na ordem de `anexos`).
    pub fn anexo(&self, i: usize) -> Result<(String, Vec<u8>), String> {
        let (nome, spec) = self.especificacoes().into_iter().nth(i).ok_or("Anexo inexistente.")?;
        let bytes = self.conteudo_anexo(spec).ok_or_else(|| format!("Não consegui ler o anexo {nome}."))?;
        Ok((nome, bytes))
    }
}

#[cfg(test)]
mod testes {
    use lopdf::{dictionary, Object, Stream};

    #[test]
    fn le_anexo_e_marcador() {
        let mut pdf = crate::testes::pdf_teste();
        let doc = &mut pdf.documento;
        let arquivo = doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"a;b\n1;2".to_vec()));
        let spec = doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("dados.csv"), "EF" => dictionary! { "F" => arquivo } });
        let pagina = *doc.get_pages().get(&1).unwrap();
        let item = doc.new_object_id();
        let raiz = doc.add_object(dictionary! { "Type" => "Outlines", "First" => item, "Last" => item, "Count" => 1 });
        doc.objects.insert(item, Object::Dictionary(dictionary! {
            "Title" => Object::string_literal(b"POL\xCDCIA".to_vec()), "Parent" => raiz, "Dest" => vec![pagina.into(), "Fit".into()],
        }));
        let c = doc.catalog_mut().unwrap();
        c.set("Names", dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![Object::string_literal("dados.csv"), spec.into()] } });
        c.set("Outlines", raiz);
        assert_eq!(pdf.anexos(), [crate::Anexo { nome: "dados.csv".into(), tamanho: 7 }]);
        assert_eq!(pdf.anexo(0).unwrap().1, b"a;b\n1;2");
        assert_eq!(pdf.marcadores(), [crate::Marcador { nivel: 1, titulo: "POLÍCIA".into(), pagina: 1 }]);
    }

    #[test]
    fn links_da_pagina_endereco_pagina_e_nome() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.inserir_link(1, [10.0, 10.0, 50.0, 20.0], crate::Destino::Endereco("exemplo.com".into())).unwrap();
        pdf.inserir_link(1, [10.0, 30.0, 50.0, 40.0], crate::Destino::Pagina(1)).unwrap();
        let doc = &mut pdf.documento;
        let pagina = *doc.get_pages().get(&1).unwrap();
        // Destino com nome (árvore /Names /Dests) e um link que tenta abrir programa.
        let nomeado = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 5.into(), 5.into()], "Dest" => Object::string_literal("cap1") });
        let programa = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 5.into(), 5.into()],
            "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("file:///c:/windows/calc.exe") } });
        doc.catalog_mut().unwrap().set("Names", dictionary! { "Dests" => dictionary! { "Names" => vec![Object::string_literal("cap1"), vec![pagina.into(), "Fit".into()].into()] } });
        let annots = doc.get_dictionary(pagina).unwrap().get(b"Annots").unwrap().clone();
        let mut lista = match annots { Object::Array(v) => v, Object::Reference(r) => doc.get_object(r).unwrap().as_array().unwrap().clone(), _ => vec![] };
        lista.extend([nomeado.into(), programa.into()]);
        doc.get_dictionary_mut(pagina).unwrap().set("Annots", lista);
        let destinos: Vec<crate::Destino> = pdf.links(1).into_iter().map(|l| l.1).collect();
        assert_eq!(destinos, [crate::Destino::Endereco("https://exemplo.com".into()), crate::Destino::Pagina(1), crate::Destino::Pagina(1)]);
    }
}
