//! Organizar páginas: ordem, cópia, inserção, extração, numeração e tamanho.
//! Toda operação começa achatando a árvore de páginas (um /Pages só, com os
//! atributos herdados copiados para cada página): assim a ordem é só a lista /Kids.
//! Os números de página são 1, 2, 3… como a pessoa vê.

use std::collections::HashSet;

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use super::DocumentoPdf;

const HERDADOS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const A4: (f32, f32) = (595.0, 842.0);
const NOME_FONTE: &str = "FPnum";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatoNumero { Simples, DeTotal, PaginaDeTotal }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosicaoNumero { RodapeCentro, RodapeDireita, RodapeEsquerda, TopoDireita }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Numeracao {
    pub formato: FormatoNumero,
    pub posicao: PosicaoNumero,
    /// Primeira página que recebe número (a capa costuma ficar sem).
    pub a_partir_de: u32,
    /// Número escrito nessa primeira página.
    pub primeiro: u32,
    pub tamanho: f32,
}

fn erro(e: impl std::fmt::Display) -> String { format!("PDF com estrutura inesperada: {e}") }

impl DocumentoPdf {
    fn raiz_paginas(&self) -> Result<ObjectId, String> {
        self.documento.catalog().map_err(erro)?
            .get(b"Pages").and_then(Object::as_reference).map_err(erro)
    }

    /// Ids das páginas em ordem, com a árvore já achatada.
    pub(crate) fn achatar(&mut self) -> Result<Vec<ObjectId>, String> {
        let ids = self.ids_paginas();
        for &id in &ids {
            let pagina = self.documento.get_dictionary(id).map_err(erro)?;
            let mut faltam: Vec<&[u8]> = HERDADOS.into_iter().filter(|c| !pagina.has(c)).collect();
            let mut achados = Vec::new();
            let mut pai = pagina.get(b"Parent").and_then(Object::as_reference).ok();
            // O limite protege contra um /Parent que aponta para si mesmo.
            for _ in 0..64 {
                let (Some(p), false) = (pai, faltam.is_empty()) else { break };
                let Ok(d) = self.documento.get_dictionary(p) else { break };
                faltam.retain(|c| match d.get(c) {
                    Ok(v) => { achados.push((c.to_vec(), v.clone())); false }
                    Err(_) => true,
                });
                pai = d.get(b"Parent").and_then(Object::as_reference).ok();
            }
            let pagina = self.documento.get_dictionary_mut(id).map_err(erro)?;
            for (chave, valor) in achados { pagina.set(chave, valor); }
        }
        self.definir_ordem(&ids)?;
        Ok(ids)
    }

    fn definir_ordem(&mut self, ids: &[ObjectId]) -> Result<(), String> {
        // A /PageLabels conta por posição: excluir, mover ou colar trocaria o nome das páginas.
        let antes = self.ids_paginas();
        let tem_rotulos = self.documento.catalog().is_ok_and(|c| c.has(b"PageLabels"));
        let rotulos = (tem_rotulos && antes != ids).then(|| self.rotulos_paginas());
        let raiz = self.raiz_paginas()?;
        for &id in ids {
            self.documento.get_dictionary_mut(id).map_err(erro)?.set("Parent", Object::Reference(raiz));
        }
        let d = self.documento.get_dictionary_mut(raiz).map_err(erro)?;
        d.set("Kids", ids.iter().map(|&i| Object::Reference(i)).collect::<Vec<_>>());
        d.set("Count", ids.len() as i64);
        self.modificado = true;
        if let Some(r) = rotulos { self.remapear_rotulos(&antes, &r)?; }
        Ok(())
    }

    /// Índices (a partir de 0) sem repetição, em ordem; recusa página inexistente.
    fn indices(&self, paginas: &[u32]) -> Result<Vec<usize>, String> {
        let total = self.total_paginas();
        let mut v: Vec<usize> = paginas.iter().map(|&p| {
            if (1..=total).contains(&p) { Ok(p as usize - 1) } else { Err(format!("Página {p} inexistente.")) }
        }).collect::<Result<_, _>>()?;
        if v.is_empty() { return Err("Nenhuma página escolhida.".into()); }
        v.sort_unstable();
        v.dedup();
        Ok(v)
    }

    pub fn girar_paginas(&mut self, paginas: &[u32], graus: i64) -> Result<(), String> {
        if !matches!(graus, -270 | -180 | -90 | 90 | 180 | 270) {
            return Err("Giro permitido: 90, 180 ou 270 graus.".into());
        }
        let indices = self.indices(paginas)?;
        let ids = self.achatar()?;
        for i in indices {
            let dict = self.documento.get_dictionary_mut(ids[i]).map_err(erro)?;
            let atual = dict.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
            dict.set("Rotate", (atual + graus).rem_euclid(360));
        }
        Ok(())
    }

    pub fn excluir_paginas(&mut self, paginas: &[u32]) -> Result<(), String> {
        let indices = self.indices(paginas)?;
        if indices.len() >= self.total_paginas() as usize {
            return Err("O documento deve manter ao menos uma página.".into());
        }
        let ids = self.achatar()?;
        let resto: Vec<ObjectId> = ids.iter().enumerate()
            .filter(|(i, _)| indices.binary_search(i).is_err()).map(|(_, &id)| id).collect();
        self.definir_ordem(&resto)?;
        self.soltar_paginas(&ids, &resto);
        Ok(())
    }

    /// Estrutura (/Pg), destinos, marcadores e widgets (/P) ainda apontam para as páginas que
    /// saíram: sem virar null, o conteúdo delas iria junto ao salvar. Os campos delas saem do /AcroForm.
    fn soltar_paginas(&mut self, todas: &[ObjectId], ficam: &[ObjectId]) {
        let anotacoes = |pdf: &Self, ids: &mut dyn Iterator<Item = &ObjectId>| -> HashSet<ObjectId> {
            ids.flat_map(|&id| pdf.lista_anotacoes(id)).filter_map(|o| o.as_reference().ok()).collect()
        };
        let saem: Vec<ObjectId> = todas.iter().copied().filter(|id| !ficam.contains(id)).collect();
        let ficam_anot = anotacoes(self, &mut ficam.iter());
        let fora: HashSet<ObjectId> = anotacoes(self, &mut saem.iter()).into_iter().filter(|a| !ficam_anot.contains(a)).collect();
        if !fora.is_empty() { self.tirar_campos(&fora); }
        for id in saem.into_iter().chain(fora) { self.documento.objects.insert(id, Object::Null); }
    }

    /// Põe as páginas escolhidas, na ordem em que estão, antes da página `antes_de`
    /// (`total + 1` = no fim). Devolve o novo número da primeira delas.
    pub fn mover_paginas(&mut self, paginas: &[u32], antes_de: u32) -> Result<u32, String> {
        let indices = self.indices(paginas)?;
        let total = self.total_paginas();
        if !(1..=total + 1).contains(&antes_de) { return Err(format!("Posição {antes_de} inexistente.")); }
        let ids = self.achatar()?;
        let alvo = antes_de as usize - 1;
        let fica = |i: &usize| indices.binary_search(i).is_err();
        let antes: Vec<ObjectId> = (0..alvo).filter(fica).map(|i| ids[i]).collect();
        let primeira = antes.len() as u32 + 1;
        let ordem: Vec<ObjectId> = antes.into_iter()
            .chain(indices.iter().map(|&i| ids[i]))
            .chain((alvo..ids.len()).filter(fica).map(|i| ids[i]))
            .collect();
        self.definir_ordem(&ordem)?;
        Ok(primeira)
    }

    /// Cada página escolhida ganha uma cópia logo depois dela.
    pub fn duplicar_paginas(&mut self, paginas: &[u32]) -> Result<(), String> {
        let indices = self.indices(paginas)?;
        let ids = self.achatar()?;
        let mut ordem = Vec::with_capacity(ids.len() + indices.len());
        for (i, &id) in ids.iter().enumerate() {
            ordem.push(id);
            if indices.binary_search(&i).is_ok() {
                let mut copia = self.documento.get_dictionary(id).map_err(erro)?.clone();
                // shortcut: a cópia sai sem anotações (um campo de formulário não pode ter
                // dois donos); copiar comentários e links junto quando a aba Comentar existir.
                copia.remove(b"Annots");
                ordem.push(self.documento.add_object(copia));
            }
        }
        self.definir_ordem(&ordem)
    }

    /// Página em branco que passa a ser a de número `posicao` (`total + 1` = no fim),
    /// do tamanho da vizinha.
    pub fn inserir_em_branco(&mut self, posicao: u32) -> Result<(), String> {
        let total = self.total_paginas();
        if !(1..=total + 1).contains(&posicao) { return Err(format!("Posição {posicao} inexistente.")); }
        let mut ids = self.achatar()?;
        let vizinha = ids[(posicao as usize).saturating_sub(2).min(ids.len() - 1)];
        let caixa = self.documento.get_dictionary(vizinha).ok()
            .and_then(|d| d.get(b"MediaBox").ok().cloned())
            .unwrap_or_else(|| vec![0.into(), 0.into(), A4.0.into(), A4.1.into()].into());
        // Scan deitado é guardado em pé com /Rotate 90: a página em branco deita junto.
        let giro = self.documento.get_dictionary(vizinha).ok().and_then(|d| d.get(b"Rotate").ok().cloned());
        let conteudo = self.documento.add_object(Stream::new(Dictionary::new(), Vec::new()));
        let nova = self.documento.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => caixa,
            "Resources" => Dictionary::new(),
            "Contents" => Object::Reference(conteudo),
        });
        if let (Some(g), Ok(d)) = (giro, self.documento.get_dictionary_mut(nova)) { d.set("Rotate", g); }
        ids.insert(posicao as usize - 1, nova);
        self.definir_ordem(&ids)
    }

    /// Junta as páginas de outro PDF a partir da posição `posicao`. Devolve quantas entraram.
    pub fn inserir_pdf(&mut self, mut outro: DocumentoPdf, posicao: u32) -> Result<u32, String> {
        let total = self.total_paginas();
        if !(1..=total + 1).contains(&posicao) { return Err(format!("Posição {posicao} inexistente.")); }
        let mut ids = self.achatar()?;
        outro.achatar()?;
        let mut doc = outro.documento;
        doc.renumber_objects_with(self.documento.max_id + 1);
        // Depois de achatado, get_pages segue a ordem do /Kids.
        let novas: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let n = novas.len() as u32;
        // Os campos do outro moram no /AcroForm dele, que fica solto: sem levá-los, os widgets morrem.
        fn resolver<'a>(doc: &'a lopdf::Document, o: &'a Object) -> Option<&'a Object> {
            match o { Object::Reference(r) => doc.get_object(*r).ok(), o => Some(o) }
        }
        let resolver = |o| resolver(&doc, o);
        let campos: Vec<Object> = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).and_then(resolver)
            .and_then(|af| af.as_dict().ok()?.get(b"Fields").ok()).and_then(resolver)
            .and_then(|f| f.as_array().ok()).map(|v| v.iter().filter(|x| x.as_reference().is_ok()).cloned().collect()).unwrap_or_default();
        self.documento.max_id = doc.max_id;
        // O catálogo e a raiz do outro ficam soltos; o `serializar` os descarta.
        self.documento.objects.extend(doc.objects);
        let p = posicao as usize - 1;
        ids.splice(p..p, novas);
        self.definir_ordem(&ids)?;
        if !campos.is_empty() {
            let af = self.garantir_acroform()?;
            let d = self.documento.get_dictionary(af).map_err(erro)?;
            let mut todos = d.get(b"Fields").ok().and_then(|o| self.resolver_array(o)).unwrap_or_default();
            // Dois formulários iguais (o "Nome" de cada pessoa): campo raiz com nome repetido é proibido
            // e o leitor trataria os dois como um só. O que chega ganha "Nome 2", "Nome 3"…
            let nome = |doc: &lopdf::Document, o: &Object| o.as_reference().ok().and_then(|r| doc.get_dictionary(r).ok())
                .and_then(|d| d.get(b"T").ok()).and_then(|t| lopdf::decode_text_string(t).ok());
            let mut usados: std::collections::HashSet<String> = todos.iter().filter_map(|o| nome(&self.documento, o)).collect();
            for c in &campos {
                let (Some(t), Ok(r)) = (nome(&self.documento, c), c.as_reference()) else { continue };
                let livre = if usados.contains(&t) { (2..).map(|n| format!("{t} {n}")).find(|n| !usados.contains(n)).unwrap_or_default() } else { t };
                if let Ok(d) = self.documento.get_dictionary_mut(r) { d.set("T", lopdf::text_string(&livre)); }
                usados.insert(livre);
            }
            todos.extend(campos);
            self.documento.get_dictionary_mut(af).map_err(erro)?.set("Fields", todos);
        }
        Ok(n)
    }

    /// Um PDF novo só com as páginas escolhidas, na ordem do documento.
    /// Marcadores saem: apontariam para páginas que não estão no arquivo novo.
    pub fn extrair(&self, paginas: &[u32]) -> Result<Vec<u8>, String> {
        // A cópia de `so_paginas` já é descartável: copiar de novo dobrava o custo (Dividir por tamanho mede muitas vezes).
        crate::serializar_documento(self.so_paginas(paginas)?.documento)
    }

    /// Como `extrair`, para gravar em arquivo: PDF com senha sai com a mesma senha.
    pub fn extrair_para_gravar(&self, paginas: &[u32]) -> Result<Vec<u8>, String> {
        self.so_paginas(paginas)?.bytes_para_gravar()
    }

    fn so_paginas(&self, paginas: &[u32]) -> Result<DocumentoPdf, String> {
        let indices = self.indices(paginas)?;
        let mut copia = self.clone();
        let ids = copia.achatar()?;
        let ficam: Vec<ObjectId> = indices.iter().map(|&i| ids[i]).collect();
        copia.definir_ordem(&ficam)?;
        copia.soltar_paginas(&ids, &ficam);
        let catalogo = copia.documento.trailer.get(b"Root").and_then(Object::as_reference).map_err(erro)?;
        if let Ok(c) = copia.documento.get_dictionary_mut(catalogo) {
            c.remove(b"Outlines");
            c.remove(b"PageLabels");
        }
        Ok(copia)
    }

    /// Largura e altura de cada página em pontos, já com o giro, lidas do documento (sem
    /// interpretar o arquivo inteiro): a tela já sabe as páginas enquanto o desenhador se prepara.
    pub fn tamanhos(&self) -> Vec<(f32, f32)> {
        self.ids_paginas().into_iter().map(|id| {
            let (x0, y0, x1, y1) = self.caixa(id);
            let giro = self.herdado(id, b"Rotate").and_then(|g| g.as_i64().ok()).unwrap_or(0).rem_euclid(360);
            let (l, a) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
            if giro == 90 || giro == 270 { (a, l) } else { (l, a) }
        }).collect()
    }

    /// Bytes dos streams (conteúdo, imagens, fontes): quanto uma cópia do documento pesa na memória, mais ou menos.
    pub fn peso_na_memoria(&self) -> usize {
        self.documento.objects.values().map(|o| if let Object::Stream(s) = o { s.content.len() } else { 0 }).sum()
    }

    /// Caixa visível da página (CropBox ou MediaBox) como (x0, y0, x1, y1).
    pub(crate) fn caixa(&self, id: ObjectId) -> (f32, f32, f32, f32) {
        let ler = |chave: &[u8]| -> Option<(f32, f32, f32, f32)> {
            let o = self.herdado(id, chave)?;
            let o = match o { Object::Reference(r) => self.documento.get_object(*r).ok()?, o => o };
            let v = o.as_array().ok()?;
            let n: Vec<f32> = v.iter().filter_map(|x| x.as_float().ok()).collect();
            (n.len() == 4).then(|| (n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])))
        };
        // O CropBox vale recortado pela MediaBox (como o Hayro desenha): maior que ela, deslocava a
        // camada do OCR, a numeração e as marcas na tela.
        let midia = ler(b"MediaBox");
        match (ler(b"CropBox"), midia) {
            (Some(c), Some(m)) => {
                let r = (c.0.max(m.0), c.1.max(m.1), c.2.min(m.2), c.3.min(m.3));
                if r.0 < r.2 && r.1 < r.3 { r } else { m }
            }
            (c, m) => c.or(m).unwrap_or((0.0, 0.0, A4.0, A4.1)),
        }
    }

    /// `chave` na página ou no nó mais próximo acima dela (Resources, MediaBox, CropBox, Rotate).
    pub(crate) fn herdado(&self, id: ObjectId, chave: &[u8]) -> Option<&Object> {
        let mut no = Some(id);
        // O limite protege contra um /Parent que aponta para si mesmo.
        for _ in 0..64 {
            let d = self.documento.get_dictionary(no?).ok()?;
            if let Ok(v) = d.get(chave) { return Some(v); }
            no = d.get(b"Parent").and_then(Object::as_reference).ok();
        }
        None
    }

    /// Embrulha o conteúdo da página: `antes` vai na frente, `depois` no fim.
    pub(crate) fn envolver(&mut self, id: ObjectId, antes: ObjectId, depois: ObjectId) -> Result<(), String> {
        let atual = self.documento.get_dictionary(id).map_err(erro)?.get(b"Contents").ok().cloned();
        let mut partes = vec![Object::Reference(antes)];
        match atual {
            Some(Object::Array(v)) => partes.extend(v),
            Some(Object::Reference(r)) => match self.documento.get_object(r) {
                Ok(Object::Array(v)) => partes.extend(v.clone()),
                _ => partes.push(Object::Reference(r)),
            },
            _ => {}
        }
        partes.push(Object::Reference(depois));
        self.documento.get_dictionary_mut(id).map_err(erro)?.set("Contents", partes);
        Ok(())
    }

    pub(crate) fn fluxo(&mut self, texto: String) -> ObjectId {
        self.documento.add_object(Stream::new(Dictionary::new(), texto.into_bytes()))
    }

    /// Escreve o número em cada página a partir de `a_partir_de`, com Helvetica
    /// (fonte padrão do PDF, sem embutir nada).
    /// shortcut: a posição é na caixa sem giro; numa página com /Rotate o número sai
    /// de lado. Compensar o giro quando alguém numerar PDF digitalizado deitado.
    pub fn numerar(&mut self, op: Numeracao) -> Result<(), String> {
        let ids = self.achatar()?;
        let total = ids.len() as u32;
        if !(1..=total).contains(&op.a_partir_de) { return Err("Página inicial fora do documento.".into()); }
        let ultimo = op.primeiro + (total - op.a_partir_de);
        let textos = ids.iter().enumerate().skip(op.a_partir_de as usize - 1).map(|(i, &id)| {
            let n = op.primeiro + (i as u32 + 1 - op.a_partir_de);
            (id, match op.formato {
                FormatoNumero::Simples => n.to_string(),
                FormatoNumero::DeTotal => format!("{n} de {ultimo}"),
                FormatoNumero::PaginaDeTotal => format!("Página {n} de {ultimo}"),
            })
        }).collect();
        self.carimbar(textos, op.posicao, op.tamanho)
    }

    /// Numeração Bates (jurídico): prefixo e número com zeros à esquerda em todas as páginas,
    /// seguindo de um lote para o outro pelo `primeiro`. Devolve o próximo número livre.
    pub fn numerar_bates(&mut self, prefixo: &str, primeiro: u32, digitos: usize, posicao: PosicaoNumero, tamanho: f32) -> Result<u32, String> {
        let ids = self.achatar()?;
        let fim = primeiro + ids.len() as u32;
        let textos = ids.into_iter().zip(primeiro..).map(|(id, n)| (id, format!("{prefixo}{n:0digitos$}"))).collect();
        self.carimbar(textos, posicao, tamanho)?;
        Ok(fim)
    }

    /// Escreve cada texto na sua página, em Helvetica, na posição pedida.
    fn carimbar(&mut self, textos: Vec<(ObjectId, String)>, posicao: PosicaoNumero, tamanho: f32) -> Result<(), String> {
        let fonte = self.documento.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let abre = self.fluxo("q\n".into());
        for (id, texto) in textos {
            let (x0, y0, x1, y1) = self.caixa(id);
            let largura = feather_draw::largura_texto(&texto, feather_draw::FontePdf::Helvetica, tamanho);
            let margem = 28.0;
            let x = match posicao {
                PosicaoNumero::RodapeCentro => (x0 + x1 - largura) / 2.0,
                PosicaoNumero::RodapeDireita | PosicaoNumero::TopoDireita => x1 - margem - largura,
                PosicaoNumero::RodapeEsquerda => x0 + margem,
            };
            let y = if posicao == PosicaoNumero::TopoDireita { y1 - margem - tamanho } else { y0 + margem };
            let fecha = self.fluxo(format!(
                "Q\nq BT /{NOME_FONTE} {} Tf 0 g {x:.2} {y:.2} Td ({}) Tj ET Q\n",
                tamanho, literal_pdf(&texto)));
            self.envolver(id, abre, fecha)?;
            self.por_recurso(id, "Font", NOME_FONTE, fonte)?;
        }
        self.modificado = true;
        Ok(())
    }

    /// Põe `nome` → `alvo` na categoria (Font, XObject) dos recursos próprios da
    /// página. Copia o dicionário (que pode ser compartilhado com outras páginas)
    /// em vez de mexer no original.
    pub(crate) fn por_recurso(&mut self, id: ObjectId, categoria: &str, nome: &str, alvo: ObjectId) -> Result<(), String> {
        let resolver = |o: Option<&Object>| -> Dictionary {
            match o {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => self.documento.get_dictionary(*r).cloned().unwrap_or_default(),
                _ => Dictionary::new(),
            }
        };
        // Recursos herdados do /Pages entram na cópia: sem eles a página perderia fontes e imagens.
        let mut recursos = resolver(self.herdado(id, b"Resources"));
        let mut lista = resolver(recursos.get(categoria.as_bytes()).ok());
        lista.set(nome, Object::Reference(alvo));
        recursos.set(categoria, lista);
        self.documento.get_dictionary_mut(id).map_err(erro)?.set("Resources", recursos);
        Ok(())
    }

    /// Para imprimir frente e verso: põe uma página em branco antes de cada capítulo que
    /// começaria numa página par, para todos abrirem do lado direito. `inicios` são as páginas
    /// onde começam os capítulos (os marcadores de primeiro nível). Devolve quantas entraram.
    pub fn capitulos_na_impar(&mut self, inicios: &[u32]) -> Result<u32, String> {
        let mut inicios: Vec<u32> = inicios.iter().copied().filter(|&p| p > 1 && p <= self.total_paginas()).collect();
        inicios.sort_unstable();
        inicios.dedup();
        let mut extra = 0;
        for p in inicios {
            if (p + extra) % 2 == 0 { self.inserir_em_branco(p + extra)?; extra += 1; }
        }
        Ok(extra)
    }

    /// Artigo em colunas vira páginas seguidas: cada página escolhida se repete uma vez por
    /// coluna, cada cópia recortada (CropBox) na sua coluna. O corte vai no vão entre os textos
    /// mais perto da divisão; página com figura atravessando o corte fica inteira.
    /// Devolve quantas páginas foram divididas.
    pub fn dividir_colunas(&mut self, paginas: &[u32], colunas: u32) -> Result<u32, String> {
        if colunas < 2 { return Err("Escolha 2 ou mais colunas.".into()); }
        let mut feitas = 0;
        let mut paginas: Vec<u32> = paginas.to_vec();
        paginas.sort_unstable();
        // De trás para a frente, para as cópias não mudarem o número das próximas.
        for &p in paginas.iter().rev() {
            let ([x0, y0, x1, y1], giro) = self.geometria(p)?;
            // shortcut: página deitada (/Rotate 90, comum em scan) fica como está: as colunas que se veem correm no y do PDF e o corte em x faria faixas; cortar em y se pedirem.
            if giro % 180 != 0 { continue; }
            let objetos = self.objetos(p)?;
            let mut cortes = vec![x0];
            for i in 1..colunas {
                let alvo = x0 + (x1 - x0) * i as f32 / colunas as f32;
                // Candidatos: as bordas dos textos; fica o x livre de texto mais perto do alvo.
                let livre = |x: f32| !objetos.iter().any(|o| matches!(o.tipo, crate::TipoObjeto::Texto { .. }) && o.caixa[0] < x && o.caixa[2] > x);
                let mut melhor = alvo;
                let mut candidatos: Vec<f32> = objetos.iter().flat_map(|o| [o.caixa[0] - 0.5, o.caixa[2] + 0.5]).collect();
                candidatos.push(alvo);
                candidatos.retain(|&x| livre(x) && (x - alvo).abs() < (x1 - x0) / colunas as f32 / 2.0);
                if let Some(&x) = candidatos.iter().min_by(|a, b| (*a - alvo).abs().total_cmp(&(*b - alvo).abs())) { melhor = x; }
                cortes.push(melhor);
            }
            cortes.push(x1);
            let larga = objetos.iter().any(|o| !matches!(o.tipo, crate::TipoObjeto::Texto { .. })
                && cortes[1..cortes.len() - 1].iter().any(|&c| o.caixa[0] < c - 2.0 && o.caixa[2] > c + 2.0));
            if larga { continue; }
            for _ in 1..colunas { self.duplicar_paginas(&[p])?; }
            let ids = self.achatar()?;
            for i in 0..colunas as usize {
                let id = ids[p as usize - 1 + i];
                let caixa: Vec<Object> = [cortes[i], y0, cortes[i + 1], y1].into_iter().map(Object::Real).collect();
                self.documento.get_dictionary_mut(id).map_err(erro)?.set("CropBox", caixa);
            }
            feitas += 1;
        }
        if feitas > 0 { self.modificado = true; }
        Ok(feitas)
    }

    /// Junta um escaneamento de uma face só: a primeira metade são as frentes, a segunda os
    /// versos (de trás para a frente quando o maço foi virado inteiro). Fica f1, v1, f2, v2…
    pub fn intercalar_frente_verso(&mut self, versos_invertidos: bool) -> Result<(), String> {
        let ids = self.achatar()?;
        if ids.len() < 2 || ids.len() % 2 != 0 {
            return Err(format!("São {} páginas: precisa ser um número par (metade frentes, metade versos).", ids.len()));
        }
        let (frentes, versos) = ids.split_at(ids.len() / 2);
        let ordem: Vec<ObjectId> = frentes.iter().enumerate().flat_map(|(i, &f)| {
            [f, if versos_invertidos { versos[versos.len() - 1 - i] } else { versos[i] }]
        }).collect();
        self.definir_ordem(&ordem)
    }

    /// Deixa todas as páginas do tamanho dado (em pontos), mantendo a orientação de
    /// cada uma; o conteúdo é reduzido ou ampliado e centrado. Devolve quantas mudaram.
    /// shortcut: comentários e links não acompanham a escala (o /Rect fica onde estava);
    /// transformar o /Rect quando a aba Comentar ler anotações.
    pub fn padronizar_tamanho(&mut self, largura: f32, altura: f32) -> Result<u32, String> {
        let ids = self.achatar()?;
        let mut mudou = 0;
        for id in ids {
            let (x0, y0, x1, y1) = self.caixa(id);
            let (w, h) = (x1 - x0, y1 - y0);
            if w <= 0.0 || h <= 0.0 { continue; }
            let (tw, th) = if (w > h) == (largura > altura) { (largura, altura) } else { (altura, largura) };
            if (w - tw).abs() < 1.0 && (h - th).abs() < 1.0 { continue; }
            let s = (tw / w).min(th / h);
            let tx = (tw - w * s) / 2.0 - x0 * s;
            let ty = (th - h * s) / 2.0 - y0 * s;
            let abre = self.fluxo(format!("q {s:.5} 0 0 {s:.5} {tx:.3} {ty:.3} cm\n"));
            let fecha = self.fluxo("Q\n".into());
            self.envolver(id, abre, fecha)?;
            let d = self.documento.get_dictionary_mut(id).map_err(erro)?;
            d.set("MediaBox", vec![0.into(), 0.into(), tw.into(), th.into()]);
            for caixa in [&b"CropBox"[..], b"BleedBox", b"TrimBox", b"ArtBox"] { d.remove(caixa); }
            mudou += 1;
        }
        if mudou > 0 { self.modificado = true; }
        Ok(mudou)
    }
}

/// Lê "1-3, 5; 8-" como [(1, 3), (5, 5), (8, total)], na ordem escrita.
pub fn ler_intervalos(texto: &str, total: u32) -> Result<Vec<(u32, u32)>, String> {
    let numero = |t: &str, padrao: u32| -> Result<u32, String> {
        let t = t.trim();
        if t.is_empty() { return Ok(padrao); }
        t.parse().map_err(|_| format!("\"{t}\" não é número de página."))
    };
    let mut partes = Vec::new();
    for pedaco in texto.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match pedaco.split_once(['-', '–']) {
            Some((a, b)) => (numero(a, 1)?, numero(b, total)?),
            None => { let n = numero(pedaco, 0)?; (n, n) }
        };
        if a == 0 || b > total || a > b {
            return Err(format!("Intervalo {pedaco} fora das páginas 1–{total}."));
        }
        partes.push((a, b));
    }
    if partes.is_empty() { return Err("Escreva pelo menos um intervalo, como 1-3.".into()); }
    Ok(partes)
}

/// Texto em WinAnsi dentro de `( )`, com os escapes do PDF.
fn literal_pdf(texto: &str) -> String {
    texto.chars().map(|c| match c {
        '(' | ')' | '\\' => format!("\\{c}"),
        ' '..='~' => c.to_string(),
        // Latin-1 coincide com WinAnsi nas letras acentuadas.
        c if (0xA0..=0xFF).contains(&(c as u32)) => format!("\\{:03o}", c as u32),
        _ => "?".into(),
    }).collect()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes::pdf_teste;

    #[test]
    fn tamanhos_seguem_o_giro() {
        let mut pdf = pdf_teste();
        assert_eq!(pdf.tamanhos(), vec![(595.0, 842.0)]);
        pdf.girar_paginas(&[1], 90).unwrap();
        assert_eq!(pdf.tamanhos(), vec![(842.0, 595.0)]);
        assert!(pdf.peso_na_memoria() > 0);
    }

    #[test]
    fn cropbox_maior_que_a_pagina_e_recortado_e_branca_deita_com_a_vizinha() {
        let mut pdf = pdf_teste();
        let id = pdf.id_pagina(1).unwrap();
        pdf.documento.get_dictionary_mut(id).unwrap().set("CropBox", vec![0.into(), 0.into(), 700.into(), 900.into()]);
        assert_eq!(pdf.tamanhos(), vec![(595.0, 842.0)], "o Hayro desenha a interseção com a MediaBox");
        pdf.girar_paginas(&[1], 90).unwrap();
        pdf.inserir_em_branco(2).unwrap();
        assert_eq!(pdf.tamanhos()[1], (842.0, 595.0), "a branca deita como o scan do lado");
    }

    #[test]
    fn bates_capitulos_e_colunas() {
        let mut pdf = quatro();
        assert_eq!(pdf.numerar_bates("PROC-", 7, 6, PosicaoNumero::RodapeDireita, 9.0).unwrap(), 11);
        assert!(pdf.texto_pagina(4).unwrap().contains("PROC-000010"));
        // Capítulos nas páginas 2 e 3: a 2 ganha uma branca antes; a 3 vira 4 e ganha outra.
        assert_eq!(pdf.capitulos_na_impar(&[2, 3]).unwrap(), 2);
        assert_eq!(pdf.total_paginas(), 6);
        let mut duas = crate::objetos::testes::pdf_com("BT /F1 10 Tf 50 700 Td (esquerda) Tj ET BT /F1 10 Tf 350 700 Td (direita) Tj ET");
        assert_eq!(duas.dividir_colunas(&[1], 2).unwrap(), 1);
        assert_eq!(duas.total_paginas(), 2);
        let corte = duas.geometria(1).unwrap().0[2];
        assert!(corte > 100.0 && corte < 350.0, "{corte}");
        // Página deitada não é cortada em faixas.
        let mut deitada = crate::objetos::testes::pdf_com("BT /F1 10 Tf 50 700 Td (esquerda) Tj ET BT /F1 10 Tf 350 700 Td (direita) Tj ET");
        deitada.girar_paginas(&[1], 90).unwrap();
        assert_eq!(deitada.dividir_colunas(&[1], 2).unwrap(), 0);
        assert_eq!(deitada.total_paginas(), 1);
    }

    /// PDF de 4 páginas com larguras 101..104, para reconhecer cada uma pela largura.
    fn quatro() -> DocumentoPdf {
        let mut pdf = pdf_teste();
        for _ in 0..3 { pdf.inserir_em_branco(1).unwrap(); }
        let ids = pdf.achatar().unwrap();
        for (i, id) in ids.into_iter().enumerate() {
            pdf.documento.get_dictionary_mut(id).unwrap()
                .set("MediaBox", vec![0.into(), 0.into(), (101 + i as i64).into(), 200.into()]);
        }
        pdf
    }

    fn larguras(pdf: &DocumentoPdf) -> Vec<i64> {
        pdf.documento.get_pages().values().map(|&id| pdf.caixa(id).2 as i64).collect()
    }

    #[test]
    fn excluir_e_extrair_nao_levam_a_pagina_que_saiu() {
        use crate::objetos::testes::{no_arquivo, pdf_com};
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (capa) Tj ET");
        pdf.inserir_em_branco(2).unwrap();
        let p2 = pdf.id_pagina(2).unwrap();
        let c = pdf.fluxo("BT /F1 12 Tf 72 700 Td (PAGINASECRETA) Tj ET".into());
        pdf.documento.get_dictionary_mut(p2).unwrap().set("Contents", c);
        // A estrutura marcada aponta para a página 2 pelo /Pg.
        let el = pdf.documento.add_object(dictionary! { "S" => "P", "Pg" => p2, "K" => 0 });
        let raiz = pdf.documento.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => vec![el.into()] });
        pdf.documento.catalog_mut().unwrap().set("StructTreeRoot", raiz);
        pdf.criar_campo(2, [100.0, 400.0, 250.0, 420.0], &crate::CampoNovo::Texto, "Senha").unwrap();
        let campo = pdf.campos().into_iter().find(|c| c.nome == "Senha").unwrap();
        pdf.preencher(&campo, "VALORSECRETO").unwrap();
        let so_capa = pdf.extrair(&[1]).unwrap();
        assert!(!no_arquivo(&so_capa, b"PAGINASECRETA") && !no_arquivo(&so_capa, b"VALORSECRETO"));
        pdf.excluir_paginas(&[2]).unwrap();
        let bytes = pdf.serializar().unwrap();
        assert!(!no_arquivo(&bytes, b"PAGINASECRETA") && !no_arquivo(&bytes, b"VALORSECRETO"));
        assert!(pdf.campos().is_empty());
    }

    #[test]
    fn frente_e_verso() {
        let mut pdf = quatro();
        pdf.intercalar_frente_verso(true).unwrap();
        assert_eq!(larguras(&pdf), [101, 104, 102, 103]);
        let mut pdf = quatro();
        pdf.intercalar_frente_verso(false).unwrap();
        assert_eq!(larguras(&pdf), [101, 103, 102, 104]);
        pdf.excluir_paginas(&[1]).unwrap();
        assert!(pdf.intercalar_frente_verso(false).is_err());
    }

    #[test]
    fn mover_duplicar_excluir() {
        let mut pdf = quatro();
        assert_eq!(pdf.mover_paginas(&[1, 2], 4).unwrap(), 2);
        assert_eq!(larguras(&pdf), [103, 101, 102, 104]);
        assert_eq!(pdf.mover_paginas(&[4], 1).unwrap(), 1);
        assert_eq!(larguras(&pdf), [104, 103, 101, 102]);
        assert_eq!(pdf.mover_paginas(&[1], 5).unwrap(), 4);
        assert_eq!(larguras(&pdf), [103, 101, 102, 104]);
        pdf.duplicar_paginas(&[2, 4]).unwrap();
        assert_eq!(larguras(&pdf), [103, 101, 101, 102, 104, 104]);
        pdf.excluir_paginas(&[1, 3]).unwrap();
        assert_eq!(larguras(&pdf), [101, 102, 104, 104]);
        assert!(pdf.excluir_paginas(&[1, 2, 3, 4]).is_err());
        assert!(pdf.mover_paginas(&[9], 1).is_err());
    }

    #[test]
    fn inserir_pdf_e_extrair_ida_e_volta() {
        let mut pdf = quatro();
        let parte = pdf.extrair(&[3, 1]).unwrap();
        let parte = DocumentoPdf::de_bytes(&parte, None).unwrap();
        assert_eq!(larguras(&parte), [101, 103]);
        assert_eq!(pdf.inserir_pdf(parte, 2).unwrap(), 2);
        assert_eq!(larguras(&pdf), [101, 101, 103, 102, 103, 104]);
        let relido = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(larguras(&relido), [101, 101, 103, 102, 103, 104]);
    }

    #[test]
    fn recortar_e_colar_em_outro_documento() {
        // O que a GUI faz em Recortar/Colar: extrair, excluir, inserir depois da escolhida.
        let mut origem = quatro();
        let copia = DocumentoPdf::de_bytes(&origem.extrair(&[2]).unwrap(), None).unwrap();
        origem.excluir_paginas(&[2]).unwrap();
        assert_eq!(larguras(&origem), [101, 103, 104]);
        let mut destino = quatro();
        assert_eq!(destino.inserir_pdf(copia.clone(), 5).unwrap(), 1);
        assert_eq!(larguras(&destino), [101, 102, 103, 104, 102]);
        assert_eq!(origem.inserir_pdf(copia, 1).unwrap(), 1);
        assert_eq!(larguras(&origem), [102, 101, 103, 104]);
    }

    #[test]
    fn numeracao_aparece_no_texto() {
        let mut pdf = quatro();
        pdf.numerar(Numeracao {
            formato: FormatoNumero::DeTotal, posicao: PosicaoNumero::RodapeCentro,
            a_partir_de: 2, primeiro: 1, tamanho: 10.0,
        }).unwrap();
        let relido = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert!(!relido.texto_pagina(1).unwrap().contains(" de "));
        assert!(relido.texto_pagina(2).unwrap().contains("1 de 3"));
        assert!(relido.texto_pagina(4).unwrap().contains("3 de 3"));
        assert_eq!(literal_pdf("Página (1)"), "P\\341gina \\(1\\)");
    }

    #[test]
    fn padronizar_respeita_orientacao() {
        let mut pdf = quatro();
        let deitada = pdf.achatar().unwrap()[3];
        pdf.documento.get_dictionary_mut(deitada).unwrap()
            .set("MediaBox", vec![0.into(), 0.into(), 300.into(), 100.into()]);
        assert_eq!(pdf.padronizar_tamanho(A4.0, A4.1).unwrap(), 4);
        let ids = pdf.achatar().unwrap();
        assert_eq!(pdf.caixa(ids[0]), (0.0, 0.0, 595.0, 842.0));
        assert_eq!(pdf.caixa(ids[3]), (0.0, 0.0, 842.0, 595.0));
        assert_eq!(pdf.padronizar_tamanho(A4.0, A4.1).unwrap(), 0);
    }

    #[test]
    fn intervalos() {
        assert_eq!(ler_intervalos("1-3, 5; 8-", 10).unwrap(), [(1, 3), (5, 5), (8, 10)]);
        assert_eq!(ler_intervalos("-2", 10).unwrap(), [(1, 2)]);
        assert!(ler_intervalos("4-2", 10).is_err());
        assert!(ler_intervalos("9-11", 10).is_err());
        assert!(ler_intervalos("a", 10).is_err());
        assert!(ler_intervalos(" , ", 10).is_err());
    }

    #[test]
    fn herdados_valem_antes_de_achatar() {
        let mut pdf = pdf_teste();
        let raiz = pdf.raiz_paginas().unwrap();
        let id = pdf.id_pagina(1).unwrap();
        // Carta e recursos no /Pages, nada na página: o que geradores antigos gravam.
        let fonte = pdf.documento.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let p = pdf.documento.get_dictionary_mut(id).unwrap();
        p.remove(b"MediaBox");
        p.remove(b"Resources");
        pdf.documento.get_dictionary_mut(raiz).unwrap().set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
        pdf.documento.get_dictionary_mut(raiz).unwrap().set("Resources", dictionary! { "Font" => dictionary! { "F1" => fonte } });
        assert_eq!(pdf.geometria(1).unwrap().0, [0.0, 0.0, 612.0, 792.0]);
        let nova = pdf.documento.add_object(dictionary! {});
        pdf.acrescentar(1, String::new(), &[("Font", "FPt0".into(), nova)]).unwrap();
        let recursos = pdf.documento.get_page_fonts(id).unwrap();
        assert!(recursos.contains_key(b"F1".as_slice()) && recursos.contains_key(b"FPt0".as_slice()), "{:?}", recursos.keys());
    }

    #[test]
    fn arvore_com_ciclo_nao_duplica_paginas() {
        let mut pdf = pdf_teste();
        let raiz = pdf.raiz_paginas().unwrap();
        let d = pdf.documento.get_dictionary_mut(raiz).unwrap();
        let mut kids = d.get(b"Kids").unwrap().as_array().unwrap().clone();
        kids.push(Object::Reference(raiz));
        d.set("Kids", kids);
        assert_eq!(pdf.total_paginas(), 1);
        pdf.girar_paginas(&[1], 90).unwrap();
        let bytes = pdf.serializar().unwrap();
        assert_eq!(DocumentoPdf::de_bytes(&bytes, None).unwrap().total_paginas(), 1);
    }

    #[test]
    fn juntar_leva_os_campos_do_outro() {
        let mut a = pdf_teste();
        let mut b = pdf_teste();
        b.criar_campo(1, [100.0, 700.0, 250.0, 720.0], &crate::CampoNovo::Texto, "Nome").unwrap();
        a.inserir_pdf(b, 2).unwrap();
        let campos = a.campos();
        assert_eq!(campos.len(), 1);
        assert_eq!(campos[0].pagina, 2);
        // O mesmo formulário de duas pessoas: o "Nome" de quem chega vira "Nome 2".
        let mut c = pdf_teste();
        c.criar_campo(1, [100.0, 700.0, 250.0, 720.0], &crate::CampoNovo::Texto, "Nome").unwrap();
        a.inserir_pdf(c, 3).unwrap();
        let mut nomes: Vec<String> = a.campos().into_iter().map(|c| c.nome).collect();
        nomes.sort();
        assert_eq!(nomes, ["Nome", "Nome 2"]);
    }

    #[test]
    fn herda_atributos_ao_achatar() {
        let mut pdf = pdf_teste();
        let raiz = pdf.raiz_paginas().unwrap();
        let id = pdf.achatar().unwrap()[0];
        let caixa = pdf.documento.get_dictionary_mut(id).unwrap().remove(b"MediaBox").unwrap();
        pdf.documento.get_dictionary_mut(raiz).unwrap().set("MediaBox", caixa);
        pdf.achatar().unwrap();
        assert!(pdf.documento.get_dictionary(id).unwrap().has(b"MediaBox"));
    }
}
