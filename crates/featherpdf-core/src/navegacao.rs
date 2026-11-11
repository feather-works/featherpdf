//! Marcadores (o sumário do PDF) e arquivos anexados, para os painéis laterais.

use lopdf::{dictionary, Dictionary, Object};

use super::DocumentoPdf;

#[derive(Debug, Clone, PartialEq)]
pub struct Marcador {
    /// 1 = primeiro nível.
    pub nivel: usize,
    pub titulo: String,
    pub pagina: u32,
}

/// Um link da página: o índice dele em /Annots, a área clicável e para onde vai.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkPdf {
    pub indice: usize,
    pub caixa: [f32; 4],
    pub destino: Option<crate::Destino>,
}

/// Para onde [`DocumentoPdf::mover_marcador`] leva o item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoverMarcador {
    /// Troca de lugar com o irmão de cima.
    Subir,
    /// Troca de lugar com o irmão de baixo.
    Descer,
    /// Vira o último filho do irmão de cima.
    Dentro,
    /// Sai do pai e fica logo depois dele.
    Fora,
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
    /// O sumário na ordem do PDF (vazio quando o arquivo não tem). Item sem destino na
    /// página do documento sai com `pagina` 0; o índice na lista é o que as edições recebem.
    pub fn marcadores(&self) -> Vec<Marcador> {
        let doc = &self.documento;
        let numeros: std::collections::HashMap<lopdf::ObjectId, u32> = self.ids_paginas().into_iter().zip(1..).collect();
        self.itens_do_sumario().into_iter().filter_map(|(id, nivel)| {
            let d = doc.get_dictionary(id).ok()?;
            let titulo = d.get(b"Title").ok().and_then(|t| resolver(doc, t)).and_then(|t| lopdf::decode_text_string(t).ok()).unwrap_or_default();
            let destino = d.get(b"Dest").ok().or_else(|| d.get(b"A").ok().and_then(|a| resolver(doc, a)).and_then(|a| a.as_dict().ok())
                .filter(|a| a.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo")).and_then(|a| a.get(b"D").ok()));
            let pagina = destino.and_then(|x| self.pagina_do_destino(x, &numeros)).unwrap_or(0);
            Some(Marcador { nivel, titulo, pagina })
        }).collect()
    }

    /// Raiz do sumário (/Outlines do catálogo), quando é uma referência.
    fn raiz_do_sumario(&self) -> Option<lopdf::ObjectId> {
        self.documento.catalog().ok()?.get(b"Outlines").ok()?.as_reference().ok()
    }

    /// Alguns programas gravam o /Outlines dentro do catálogo: vira objeto próprio, senão
    /// criar um marcador gravaria uma raiz nova por cima do sumário que já existe.
    fn soltar_sumario_do_catalogo(&mut self) {
        let dentro = self.documento.catalog().ok().and_then(|c| c.get(b"Outlines").ok()).and_then(|o| o.as_dict().ok()).cloned();
        if let Some(d) = dentro {
            let id = self.documento.add_object(d);
            if let Ok(c) = self.documento.catalog_mut() { c.set("Outlines", id); }
        }
    }

    /// Filhos de um nó do sumário, pela corrente First → Next (com guarda contra volta em círculo).
    fn filhos_do_sumario(&self, id: lopdf::ObjectId) -> Vec<lopdf::ObjectId> {
        let doc = &self.documento;
        let primeiro = doc.get_dictionary(id).ok().and_then(|d| d.get(b"First").ok()).and_then(|o| o.as_reference().ok());
        self.corrente_do_sumario(primeiro)
    }

    /// A corrente de irmãos a partir de `primeiro` (Next → Next…).
    fn corrente_do_sumario(&self, primeiro: Option<lopdf::ObjectId>) -> Vec<lopdf::ObjectId> {
        let doc = &self.documento;
        let mut saida = Vec::new();
        let mut vistos = std::collections::HashSet::new();
        let mut proximo = primeiro;
        while let Some(f) = proximo {
            if !vistos.insert(f) || saida.len() >= 100_000 { break; }
            saida.push(f);
            proximo = doc.get_dictionary(f).ok().and_then(|d| d.get(b"Next").ok()).and_then(|o| o.as_reference().ok());
        }
        saida
    }

    /// Todos os itens do sumário em ordem de leitura, com o nível (1 = primeiro).
    fn itens_do_sumario(&self) -> Vec<(lopdf::ObjectId, usize)> {
        // O /Outlines pode ser objeto próprio ou estar dentro do catálogo (só leitura aqui).
        let (topo, raiz) = match self.raiz_do_sumario() {
            Some(r) => (self.filhos_do_sumario(r), Some(r)),
            None => (self.corrente_do_sumario(self.documento.catalog().ok().and_then(|c| c.get(b"Outlines").ok())
                .and_then(|o| o.as_dict().ok()).and_then(|d| d.get(b"First").ok()).and_then(|o| o.as_reference().ok())), None),
        };
        let mut saida = Vec::new();
        let mut vistos: std::collections::HashSet<lopdf::ObjectId> = raiz.into_iter().collect();
        let mut pilha: Vec<(lopdf::ObjectId, usize)> = topo.into_iter().rev().map(|f| (f, 1)).collect();
        while let Some((id, nivel)) = pilha.pop() {
            if !vistos.insert(id) || saida.len() >= 100_000 { continue; }
            saida.push((id, nivel));
            pilha.extend(self.filhos_do_sumario(id).into_iter().rev().map(|f| (f, nivel + 1)));
        }
        saida
    }

    /// Só nas edições: antes de mexer, o sumário embutido no catálogo vira objeto próprio.
    fn item_do_sumario(&mut self, i: usize) -> Result<lopdf::ObjectId, String> {
        self.soltar_sumario_do_catalogo();
        self.itens_do_sumario().get(i).map(|(id, _)| *id).ok_or_else(|| "Marcador inexistente.".to_string())
    }

    /// Refaz o /Count de cada nó (aberto = positivo, fechado = negativo) depois de mudar a árvore.
    fn recontar_sumario(&mut self) {
        // `vistos`: um nó que aparece de novo (sumário em círculo) não é contado outra vez;
        // sem isso dois irmãos apontando para o pai davam 2^64 chamadas.
        fn contar(pdf: &mut DocumentoPdf, id: lopdf::ObjectId, raiz: bool, fundo: usize,
            vistos: &mut std::collections::HashSet<lopdf::ObjectId>) -> i64 {
            let filhos: Vec<_> = pdf.filhos_do_sumario(id).into_iter().filter(|f| vistos.insert(*f)).collect();
            let n: i64 = if fundo > 64 { 0 } else { filhos.iter().map(|&f| 1 + contar(pdf, f, false, fundo + 1, vistos)).sum() };
            let Ok(d) = pdf.documento.get_dictionary_mut(id) else { return 0 };
            let aberto = raiz || d.get(b"Count").and_then(Object::as_i64).is_ok_and(|c| c > 0);
            if filhos.is_empty() { d.remove(b"Count"); } else { d.set("Count", if aberto { n } else { -n }); }
            if aberto { n } else { 0 }
        }
        if let Some(raiz) = self.raiz_do_sumario() { contar(self, raiz, true, 0, &mut std::iter::once(raiz).collect()); }
    }

    /// Troca o título do marcador `i` (na ordem de [`Self::marcadores`]).
    pub fn renomear_marcador(&mut self, i: usize, titulo: &str) -> Result<(), String> {
        let titulo = titulo.trim();
        if titulo.is_empty() { return Err("Escreva o nome do marcador.".into()); }
        let id = self.item_do_sumario(i)?;
        self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Title", lopdf::text_string(titulo));
        self.modificado = true;
        Ok(())
    }

    fn referencia(&self, id: lopdf::ObjectId, chave: &[u8]) -> Option<lopdf::ObjectId> {
        self.documento.get_dictionary(id).ok()?.get(chave).ok()?.as_reference().ok()
    }

    fn por_ou_tirar(&mut self, alvo: lopdf::ObjectId, chave: &str, valor: Option<lopdf::ObjectId>) {
        if let Ok(d) = self.documento.get_dictionary_mut(alvo) {
            match valor { Some(v) => d.set(chave, v), None => { d.remove(chave.as_bytes()); } }
        }
    }

    /// O pai e os irmãos de antes e de depois do nó, tirados do percurso First → Next (o que
    /// a tela mostra), não do /Parent e do /Prev do arquivo: há geradores que não os gravam,
    /// e confiar neles apagava marcadores ao excluir ou mover.
    fn lugar_no_sumario(&self, id: lopdf::ObjectId) -> Result<(lopdf::ObjectId, Option<lopdf::ObjectId>, Option<lopdf::ObjectId>), String> {
        let raiz = self.raiz_do_sumario().ok_or("Sumário sem raiz.")?;
        let mut vistos = std::collections::HashSet::from([raiz]);
        let mut pilha = vec![raiz];
        while let Some(pai) = pilha.pop() {
            let irmaos = self.filhos_do_sumario(pai);
            if let Some(p) = irmaos.iter().position(|&f| f == id) {
                return Ok((pai, p.checked_sub(1).map(|a| irmaos[a]), irmaos.get(p + 1).copied()));
            }
            pilha.extend(irmaos.into_iter().filter(|f| vistos.insert(*f)));
        }
        Err("O marcador saiu do sumário.".into())
    }

    /// Solta o nó da corrente dos irmãos (ele e os de dentro continuam no arquivo, sem pai).
    fn desligar_do_sumario(&mut self, id: lopdf::ObjectId) -> Result<(), String> {
        let (pai, antes, depois) = self.lugar_no_sumario(id)?;
        match antes { Some(a) => self.por_ou_tirar(a, "Next", depois), None => self.por_ou_tirar(pai, "First", depois) }
        match depois { Some(n) => self.por_ou_tirar(n, "Prev", antes), None => self.por_ou_tirar(pai, "Last", antes) }
        Ok(())
    }

    /// Põe o nó dentro de `pai`, logo depois de `anterior` (ou como primeiro filho).
    fn ligar_no_sumario(&mut self, id: lopdf::ObjectId, pai: lopdf::ObjectId, anterior: Option<lopdf::ObjectId>) {
        let seguinte = match anterior { Some(a) => self.referencia(a, b"Next"), None => self.referencia(pai, b"First") };
        self.por_ou_tirar(id, "Parent", Some(pai));
        self.por_ou_tirar(id, "Prev", anterior);
        self.por_ou_tirar(id, "Next", seguinte);
        match anterior { Some(a) => self.por_ou_tirar(a, "Next", Some(id)), None => self.por_ou_tirar(pai, "First", Some(id)) }
        match seguinte { Some(n) => self.por_ou_tirar(n, "Prev", Some(id)), None => self.por_ou_tirar(pai, "Last", Some(id)) }
        // O pai fechado esconderia o item: abre (Count positivo; o recontar acerta o número).
        if Some(pai) != self.raiz_do_sumario() {
            if let Ok(d) = self.documento.get_dictionary_mut(pai) { d.set("Count", 1); }
        }
    }

    fn posicao_no_sumario(&self, id: lopdf::ObjectId) -> Result<usize, String> {
        self.itens_do_sumario().iter().position(|(x, _)| *x == id).ok_or_else(|| "O marcador saiu do sumário.".into())
    }

    /// Tira o marcador `i` e os que estão dentro dele.
    pub fn excluir_marcador(&mut self, i: usize) -> Result<(), String> {
        let id = self.item_do_sumario(i)?;
        self.desligar_do_sumario(id)?;
        self.recontar_sumario();
        self.modificado = true;
        Ok(())
    }

    /// Marcador novo para a página `pagina`: logo depois do marcador `depois_de`, no mesmo
    /// nível, ou no fim do primeiro nível. Devolve o índice dele na lista.
    pub fn criar_marcador(&mut self, titulo: &str, pagina: u32, depois_de: Option<usize>) -> Result<usize, String> {
        let (id, raiz) = self.novo_marcador(titulo, pagina)?;
        let (pai, anterior) = match depois_de {
            Some(i) => { let a = self.item_do_sumario(i)?; (self.lugar_no_sumario(a).map_or(raiz, |l| l.0), Some(a)) }
            None => (raiz, self.filhos_do_sumario(raiz).last().copied()),
        };
        self.ligar_no_sumario(id, pai, anterior);
        self.recontar_sumario();
        self.modificado = true;
        self.posicao_no_sumario(id)
    }

    /// Marcador novo dentro do marcador `pai` (o último dos filhos dele). Devolve o índice.
    pub fn criar_marcador_dentro(&mut self, titulo: &str, pagina: u32, pai: usize) -> Result<usize, String> {
        let pai = self.item_do_sumario(pai)?;
        let (id, _) = self.novo_marcador(titulo, pagina)?;
        let ultimo = self.filhos_do_sumario(pai).last().copied();
        self.ligar_no_sumario(id, pai, ultimo);
        self.recontar_sumario();
        self.modificado = true;
        self.posicao_no_sumario(id)
    }

    /// O dicionário do item novo (ainda fora da árvore) e a raiz, criada se o PDF não tinha sumário.
    fn novo_marcador(&mut self, titulo: &str, pagina: u32) -> Result<(lopdf::ObjectId, lopdf::ObjectId), String> {
        let titulo = titulo.trim();
        if titulo.is_empty() { return Err("Escreva o nome do marcador.".into()); }
        let pagina_id = self.id_pagina(pagina)?;
        self.soltar_sumario_do_catalogo();
        let raiz = match self.raiz_do_sumario() {
            Some(r) => r,
            None => {
                let r = self.documento.add_object(dictionary! { "Type" => "Outlines" });
                self.documento.catalog_mut().map_err(|e| e.to_string())?.set("Outlines", r);
                r
            }
        };
        let id = self.documento.add_object(dictionary! {
            "Title" => lopdf::text_string(titulo),
            "Dest" => vec![Object::Reference(pagina_id), Object::Name(b"Fit".to_vec())],
        });
        Ok((id, raiz))
    }

    /// O marcador `i` passa a levar à página `pagina` (troca o destino ou a ação que tinha).
    pub fn trocar_pagina_marcador(&mut self, i: usize, pagina: u32) -> Result<(), String> {
        let id = self.item_do_sumario(i)?;
        let pagina_id = self.id_pagina(pagina)?;
        let d = self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?;
        d.remove(b"A");
        d.set("Dest", vec![Object::Reference(pagina_id), Object::Name(b"Fit".to_vec())]);
        self.modificado = true;
        Ok(())
    }

    /// Muda o marcador `i` de lugar (com os de dentro) e devolve o índice novo.
    pub fn mover_marcador(&mut self, i: usize, para: MoverMarcador) -> Result<usize, String> {
        let id = self.item_do_sumario(i)?;
        let raiz = self.raiz_do_sumario().ok_or("Sumário sem raiz.")?;
        let (pai, antes, depois) = self.lugar_no_sumario(id)?;
        let (novo_pai, anterior) = match para {
            MoverMarcador::Subir => {
                let a = antes.ok_or("Já é o primeiro do nível.")?;
                (pai, self.lugar_no_sumario(a)?.1)
            }
            MoverMarcador::Descer => (pai, Some(depois.ok_or("Já é o último do nível.")?)),
            MoverMarcador::Dentro => {
                let a = antes.ok_or("Não há marcador acima, no mesmo nível, para entrar.")?;
                (a, self.filhos_do_sumario(a).last().copied())
            }
            MoverMarcador::Fora => {
                if pai == raiz { return Err("Já está no primeiro nível.".into()); }
                (self.lugar_no_sumario(pai)?.0, Some(pai))
            }
        };
        self.desligar_do_sumario(id)?;
        self.ligar_no_sumario(id, novo_pai, anterior);
        self.recontar_sumario();
        self.modificado = true;
        self.posicao_no_sumario(id)
    }

    /// Links da página (começando em 1): caixa em pontos do PDF e para onde vão. Só endereço
    /// http(s)/mailto e página do próprio documento; o resto (abrir arquivo, rodar programa) fica de fora.
    pub fn links(&self, pagina: u32) -> Vec<([f32; 4], crate::Destino)> {
        self.links_da_pagina(pagina).into_iter().filter_map(|l| Some((l.caixa, l.destino?))).collect()
    }

    /// Todos os links da página, com o índice na lista de anotações (para apagar ou trocar o
    /// destino). `destino` fica `None` no que não abrimos (arquivo, programa, script…).
    pub fn links_da_pagina(&self, pagina: u32) -> Vec<LinkPdf> {
        let doc = &self.documento;
        let Ok(id) = self.id_pagina(pagina) else { return Vec::new() };
        let numeros: std::collections::HashMap<lopdf::ObjectId, u32> = self.ids_paginas().into_iter().zip(1..).collect();
        let mut saida = Vec::new();
        for (indice, anotacao) in self.lista_anotacoes(id).into_iter().enumerate() {
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
            saida.push(LinkPdf { indice, caixa, destino });
        }
        saida
    }

    fn link_existe(&self, pagina: u32, indice: usize) -> Result<(), String> {
        if self.links_da_pagina(pagina).iter().any(|l| l.indice == indice) { Ok(()) } else { Err("Esse link não existe mais.".into()) }
    }

    /// Apaga o link `indice` (índice na lista de anotações, de [`Self::links_da_pagina`]).
    pub fn apagar_link(&mut self, pagina: u32, indice: usize) -> Result<(), String> {
        self.link_existe(pagina, indice)?;
        self.apagar_anotacao(pagina, indice)
    }

    /// O link `indice` passa a abrir `destino` (endereço ou página); a área clicável fica.
    pub fn trocar_destino_link(&mut self, pagina: u32, indice: usize, destino: crate::Destino) -> Result<(), String> {
        self.link_existe(pagina, indice)?;
        let acao = self.acao_do_link(destino)?;
        let id = self.id_pagina(pagina)?;
        let alvo = self.lista_anotacoes(id).get(indice).cloned().ok_or("Esse link não existe mais.")?;
        let d = match alvo {
            Object::Reference(r) => self.documento.get_dictionary_mut(r).map_err(|e| e.to_string())?,
            // Link escrito direto na lista: vira objeto próprio para dar para mudar.
            Object::Dictionary(d) => {
                let r = self.documento.add_object(d);
                let mut lista = self.lista_anotacoes(id);
                lista[indice] = Object::Reference(r);
                self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Annots", lista);
                self.documento.get_dictionary_mut(r).map_err(|e| e.to_string())?
            }
            _ => return Err("Esse link não existe mais.".into()),
        };
        d.remove(b"Dest");
        d.set("A", acao);
        self.modificado = true;
        Ok(())
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
        // Cada nó de /Kids uma vez: um nó que se lista sem ter /Names girava para sempre.
        let mut vistos = std::collections::HashSet::new();
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
                let novos = filhos.iter().rev().filter(|k| match k { Object::Reference(r) => vistos.insert(*r), _ => true });
                pilha.extend(novos.filter_map(resolver).filter_map(|k| k.as_dict().ok()));
            }
        }
        saida
    }

    /// O fluxo do arquivo anexado (sem descomprimir).
    fn fluxo_anexo(&self, spec: &Dictionary) -> Option<&lopdf::Stream> {
        let ef = spec.get(b"EF").ok()?;
        let ef = match ef { Object::Reference(r) => self.documento.get_object(*r).ok()?, o => o }.as_dict().ok()?;
        let id = ef.get(b"UF").or_else(|_| ef.get(b"F")).ok()?.as_reference().ok()?;
        self.documento.get_object(id).ok()?.as_stream().ok()
    }

    /// Os bytes do anexo só se couberem em `teto` (descomprime parando no teto).
    fn conteudo_anexo_ate(&self, spec: &Dictionary, teto: u64) -> Option<Vec<u8>> {
        let s = self.fluxo_anexo(spec)?;
        match s.filters().unwrap_or_default()[..] {
            [] => (s.content.len() as u64 <= teto).then(|| s.content.clone()),
            [f] if f == b"FlateDecode" && !s.dict.has(b"DecodeParms") => crate::inflar_com_teto(&s.content, teto),
            _ => None,
        }
    }

    /// Nome e bytes do anexo `i`, só se couber em `teto` (a busca lê assim, sem descomprimir
    /// anexo gigante na thread da tela).
    pub fn anexo_ate(&self, i: usize, teto: u64) -> Option<(String, Vec<u8>)> {
        let (nome, spec) = self.especificacoes().into_iter().nth(i)?;
        let bytes = self.conteudo_anexo_ate(spec, teto)?;
        Some((nome, bytes))
    }

    fn conteudo_anexo(&self, spec: &Dictionary) -> Option<Vec<u8>> {
        let s = self.fluxo_anexo(spec)?;
        s.decompressed_content().ok().or_else(|| s.filters().ok().filter(|f| f.is_empty()).map(|_| s.content.clone()))
    }

    pub fn anexos(&self) -> Vec<Anexo> {
        // O tamanho vem de /Params /Size, sem descomprimir; sem ele, do fluxo sem filtro ou
        // descomprimido até 64 MB (acima disso vale "grande": usize::MAX).
        self.especificacoes().into_iter().map(|(nome, spec)| {
            let declarado = self.fluxo_anexo(spec).and_then(|s| s.dict.get(b"Params").ok())
                .and_then(|p| match p { Object::Reference(r) => self.documento.get_dictionary(*r).ok(), o => o.as_dict().ok() })
                .and_then(|p| p.get(b"Size").and_then(Object::as_i64).ok()).and_then(|n| usize::try_from(n).ok());
            let tamanho = declarado.unwrap_or_else(|| self.conteudo_anexo_ate(spec, 64 << 20).map_or(usize::MAX, |b| b.len()));
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

    /// Audit: sumário sem /Prev e /Parent (há geradores assim) e com volta em círculo.
    #[test]
    fn sumario_sem_prev_nem_parent_e_em_circulo_nao_perde_marcador_nem_trava() {
        let mut pdf = crate::testes::pdf_teste();
        for t in ["A", "B", "C", "D"] { pdf.criar_marcador(t, 1, None).unwrap(); }
        let ids: Vec<_> = pdf.itens_do_sumario().into_iter().map(|(id, _)| id).collect();
        for &id in &ids {
            let d = pdf.documento.get_dictionary_mut(id).unwrap();
            d.remove(b"Prev");
            d.remove(b"Parent");
        }
        let titulos = |p: &crate::DocumentoPdf| p.marcadores().into_iter().map(|m| m.titulo).collect::<Vec<_>>();
        pdf.excluir_marcador(2).unwrap();
        assert_eq!(titulos(&pdf), ["A", "B", "D"], "excluir C não leva A e B junto");
        pdf.mover_marcador(2, crate::MoverMarcador::Subir).unwrap();
        assert_eq!(titulos(&pdf), ["A", "D", "B"]);
        // B aponta para A como filho: a árvore volta em círculo.
        pdf.documento.get_dictionary_mut(ids[1]).unwrap().set("First", ids[0]);
        pdf.criar_marcador("E", 1, None).unwrap();
        assert_eq!(titulos(&pdf), ["A", "D", "B", "E"]);
    }

    #[test]
    fn cria_renomeia_e_exclui_marcador() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.inserir_em_branco(2).unwrap();
        assert!(pdf.marcadores().is_empty());
        assert_eq!(pdf.criar_marcador("Introdução", 1, None).unwrap(), 0);
        assert_eq!(pdf.criar_marcador("Conclusão", 2, None).unwrap(), 1);
        assert_eq!(pdf.criar_marcador("Método", 2, Some(0)).unwrap(), 1);
        assert!(pdf.criar_marcador("  ", 1, None).is_err());
        let titulos = |p: &crate::DocumentoPdf| p.marcadores().into_iter().map(|m| (m.titulo, m.pagina)).collect::<Vec<_>>();
        assert_eq!(titulos(&pdf), [("Introdução".to_string(), 1u32), ("Método".to_string(), 2u32), ("Conclusão".to_string(), 2u32)]);
        pdf.renomear_marcador(2, "Fim").unwrap();
        pdf.excluir_marcador(1).unwrap();
        let mut pdf = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(titulos(&pdf), [("Introdução".to_string(), 1u32), ("Fim".to_string(), 2u32)]);
        let raiz = pdf.raiz_do_sumario().unwrap();
        assert_eq!(pdf.documento.get_dictionary(raiz).unwrap().get(b"Count").unwrap().as_i64().unwrap(), 2);
        pdf.excluir_marcador(0).unwrap();
        pdf.excluir_marcador(0).unwrap();
        assert!(pdf.marcadores().is_empty());
        assert!(pdf.excluir_marcador(0).is_err());
    }

    #[test]
    fn sumario_dentro_do_catalogo_aparece_e_nao_e_apagado() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.criar_marcador("Antigo", 1, None).unwrap();
        // Como alguns programas gravam: o /Outlines direto no catálogo.
        let raiz = pdf.raiz_do_sumario().unwrap();
        let d = pdf.documento.get_dictionary(raiz).unwrap().clone();
        pdf.documento.catalog_mut().unwrap().set("Outlines", d);
        assert_eq!(pdf.marcadores().len(), 1, "a leitura acha o sumário embutido");
        pdf.criar_marcador("Novo", 1, None).unwrap();
        let titulos: Vec<String> = pdf.marcadores().into_iter().map(|m| m.titulo).collect();
        assert_eq!(titulos, ["Antigo", "Novo"]);
    }

    #[test]
    fn marcador_dentro_mover_e_trocar_pagina() {
        use crate::MoverMarcador as M;
        let mut pdf = crate::testes::pdf_teste();
        pdf.inserir_em_branco(2).unwrap();
        pdf.criar_marcador("A", 1, None).unwrap();
        pdf.criar_marcador("B", 1, None).unwrap();
        assert_eq!(pdf.criar_marcador_dentro("A1", 2, 0).unwrap(), 1);
        let arvore = |p: &crate::DocumentoPdf| p.marcadores().into_iter().map(|m| (m.titulo, m.nivel)).collect::<Vec<_>>();
        let par = |t: &str, n: usize| (t.to_string(), n);
        assert_eq!(arvore(&pdf), [par("A", 1), par("A1", 2), par("B", 1)]);
        // B entra em A (depois de A1), sai de novo, sobe acima de A e A desce de volta.
        assert_eq!(pdf.mover_marcador(2, M::Dentro).unwrap(), 2);
        assert_eq!(arvore(&pdf), [par("A", 1), par("A1", 2), par("B", 2)]);
        assert_eq!(pdf.mover_marcador(2, M::Fora).unwrap(), 2);
        assert_eq!(pdf.mover_marcador(2, M::Subir).unwrap(), 0);
        assert_eq!(arvore(&pdf), [par("B", 1), par("A", 1), par("A1", 2)]);
        assert!(pdf.mover_marcador(0, M::Subir).is_err());
        assert!(pdf.mover_marcador(0, M::Fora).is_err());
        assert_eq!(pdf.mover_marcador(0, M::Descer).unwrap(), 2, "B desce para depois de A e do A1");
        pdf.trocar_pagina_marcador(0, 2).unwrap();
        let pdf = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(arvore(&pdf), [par("A", 1), par("A1", 2), par("B", 1)]);
        assert_eq!(pdf.marcadores()[0].pagina, 2);
        let raiz = pdf.raiz_do_sumario().unwrap();
        assert_eq!(pdf.documento.get_dictionary(raiz).unwrap().get(b"Count").unwrap().as_i64().unwrap(), 3);
    }

    #[test]
    fn apaga_e_troca_destino_de_um_link() {
        use crate::Destino as D;
        let mut pdf = crate::testes::pdf_teste();
        pdf.inserir_em_branco(2).unwrap();
        pdf.inserir_link(1, [10.0, 10.0, 50.0, 20.0], D::Endereco("exemplo.com".into())).unwrap();
        pdf.inserir_link(1, [10.0, 30.0, 50.0, 40.0], D::Pagina(2)).unwrap();
        let links = pdf.links_da_pagina(1);
        assert_eq!(links.len(), 2);
        pdf.trocar_destino_link(1, links[0].indice, D::Pagina(1)).unwrap();
        assert_eq!(pdf.links_da_pagina(1)[0].destino, Some(D::Pagina(1)));
        pdf.apagar_link(1, links[1].indice).unwrap();
        let pdf = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(pdf.links(1), [([10.0, 10.0, 50.0, 20.0], D::Pagina(1))]);
        let mut pdf = pdf;
        assert!(pdf.apagar_link(1, 7).is_err());
        assert!(pdf.trocar_destino_link(1, 0, D::Endereco("  ".into())).is_err());
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
