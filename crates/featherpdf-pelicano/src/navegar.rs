//! Painéis Marcadores (o sumário do PDF: clicar leva à página; criar, renomear e excluir) e Anexos (salvar o arquivo anexado),
//! e os links do próprio PDF clicáveis na página.

use std::path::PathBuf;

use feather_ui::{icones, tema};
use featherpdf_core::{numero, Anexo, Destino, Marcador, MoverMarcador};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

#[derive(Debug, Clone)]
pub enum Nv {
    Salvar(usize),
    /// Abre a escolha do arquivo a anexar.
    Anexar,
    /// Id do documento e arquivo escolhido.
    Anexado(u64, Option<PathBuf>),
    /// Tira o anexo da lista do PDF.
    RemoverAnexo(usize),
    /// Id do documento (`Aberto::id`), anexo e destino.
    Destino(u64, usize, Option<PathBuf>),
    /// Link de site do PDF (já filtrado para http/https/mailto em `DocumentoPdf::links`).
    Site(String),
    /// Clique num marcador: escolhe e vai à página dele.
    Escolher(usize),
    /// Abre o campo de nome para um marcador novo (na página atual, depois do escolhido).
    Novo,
    /// Abre o campo de nome para um marcador novo dentro do escolhido.
    NovoDentro,
    /// Sobe, desce, entra no de cima ou sai do pai (com os de dentro).
    Mover(MoverMarcador),
    /// O escolhido passa a levar à página atual.
    PaginaAtual,
    /// Abre o campo de nome para renomear o escolhido.
    Renomear,
    Nome(String),
    Confirmar,
    Cancelar,
    Excluir,
    /// Texto que filtra a lista de marcadores.
    Filtro(String),
    /// Painel Links: abre o campo do destino do link `indice` da página atual.
    EditarLink(usize, String),
    TextoLink(String),
    ConfirmarLink,
    CancelarLink,
    ApagarLink(usize),
}

/// O campo de nome aberto no painel Marcadores.
#[derive(Debug, Clone)]
pub enum EdicaoMarcador {
    Novo(String),
    /// Dentro do marcador de índice `usize`.
    Dentro(usize, String),
    Renomear(usize, String),
}

/// O que os dois painéis mostram, lido quando o painel abre.
#[derive(Debug, Default)]
pub struct Navegacao {
    pub marcadores: Vec<Marcador>,
    pub anexos: Vec<Anexo>,
    /// Marcador escolhido (índice em `marcadores`), alvo de Renomear e Excluir.
    pub escolhido: Option<usize>,
    pub edicao: Option<EdicaoMarcador>,
    /// Procurar nos marcadores: só os que têm esse texto no título aparecem.
    pub filtro: String,
    /// Link sendo editado: página, índice em /Annots e o destino digitado.
    pub link: Option<(u32, usize, String)>,
    /// Links da página já lidos: (página, versão do documento, links). O painel desenha a cada
    /// movimento do mouse, e ler percorria a árvore de páginas inteira.
    links_lidos: std::cell::RefCell<Option<(u32, u64, std::rc::Rc<Vec<featherpdf_core::LinkPdf>>)>>,
}

/// Como o destino aparece no campo: o endereço ou o número da página.
fn destino_em_texto(d: &Option<Destino>) -> String {
    match d { Some(Destino::Endereco(u)) => u.clone(), Some(Destino::Pagina(p)) => p.to_string(), None => String::new() }
}

/// O que se digitou no campo: número é página; o resto, endereço (o motor põe https:// ou mailto:).
fn destino_do_texto(t: &str) -> Destino {
    match t.trim().parse::<u32>() { Ok(n) => Destino::Pagina(n), Err(_) => Destino::Endereco(t.trim().to_string()) }
}

impl App {
    pub fn ler_navegacao(&mut self) {
        let (escolhido, filtro, link) = (self.navegacao.escolhido, std::mem::take(&mut self.navegacao.filtro), self.navegacao.link.take());
        self.navegacao = self.aberto.as_ref()
            .map(|a| Navegacao { marcadores: a.pdf.marcadores(), anexos: a.pdf.anexos(), ..Default::default() })
            .unwrap_or_default();
        self.navegacao.escolhido = escolhido.filter(|&i| i < self.navegacao.marcadores.len());
        self.navegacao.filtro = filtro;
        self.navegacao.link = link;
    }

    pub fn update_nav(&mut self, m: Nv) -> Task<Msg> {
        match m {
            Nv::Salvar(i) => {
                let nome = self.navegacao.anexos.get(i).map(|a| a.nome.clone()).unwrap_or_default();
                let Some(doc) = self.id_aberto() else { return Task::none() };
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().set_file_name(nome).save_file().await.map(|h| h.path().to_path_buf())
                }, move |d| Msg::Nav(Nv::Destino(doc, i, d)));
            }
            Nv::Anexar => {
                let Some(doc) = self.id_aberto() else { return Task::none() };
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().set_title("Anexar arquivo ao PDF").pick_file().await.map(|h| h.path().to_path_buf())
                }, move |c| Msg::Nav(Nv::Anexado(doc, c)));
            }
            Nv::Anexado(_, None) => {}
            Nv::Anexado(doc, Some(_)) if Some(doc) != self.id_aberto() => self.status = "O documento mudou: nada foi anexado.".into(),
            Nv::Anexado(_, Some(caminho)) => {
                // Teto: o anexo vai inteiro para a memória e para o arquivo salvo.
                const TETO: u64 = 256 << 20;
                match std::fs::metadata(&caminho) {
                    Ok(m) if m.len() > TETO => self.status = "Arquivo grande demais para anexar (mais de 256 MB).".into(),
                    Err(e) => self.status = format!("Não consegui ler o arquivo: {e}"),
                    Ok(_) => match std::fs::read(&caminho) {
                        Err(e) => self.status = format!("Não consegui ler o arquivo: {e}"),
                        Ok(bytes) => {
                            let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "anexo".into());
                            let tarefa = self.editar(move |pdf| pdf.anexar(&nome, bytes).map(|n| format!("\"{n}\" anexado ao PDF. Ctrl+Z desfaz.")));
                            self.ler_navegacao();
                            return tarefa;
                        }
                    },
                }
            }
            Nv::RemoverAnexo(i) => {
                let tarefa = self.editar(move |pdf| pdf.remover_anexo(i).map(|()| "Anexo tirado do PDF. Ctrl+Z desfaz.".to_string()));
                self.ler_navegacao();
                return tarefa;
            }
            Nv::Escolher(i) => {
                self.navegacao.escolhido = Some(i);
                self.navegacao.edicao = None;
                if let Some(p) = self.navegacao.marcadores.get(i).map(|m| m.pagina).filter(|&p| p > 0) {
                    return self.update_interno(Msg::IrPagina(p));
                }
                self.status = "Este marcador não aponta para uma página do documento.".into();
            }
            Nv::Novo => self.navegacao.edicao = Some(EdicaoMarcador::Novo(format!("Página {}", self.pagina))),
            Nv::NovoDentro => if let Some(i) = self.navegacao.escolhido {
                self.navegacao.edicao = Some(EdicaoMarcador::Dentro(i, format!("Página {}", self.pagina)));
            },
            Nv::Mover(para) => if let Some(i) = self.navegacao.escolhido {
                self.navegacao.edicao = None;
                let mut novo = None;
                let tarefa = self.editar(|pdf| pdf.mover_marcador(i, para).map(|n| { novo = Some(n); "Marcador movido. Ctrl+Z desfaz.".to_string() }));
                if novo.is_some() { self.navegacao.escolhido = novo; }
                return tarefa;
            },
            Nv::PaginaAtual => if let Some(i) = self.navegacao.escolhido {
                let pagina = self.pagina;
                return self.editar(move |pdf| pdf.trocar_pagina_marcador(i, pagina)
                    .map(|()| format!("O marcador agora leva à página {pagina}. Ctrl+Z desfaz.")));
            },
            Nv::Renomear => if let Some(i) = self.navegacao.escolhido {
                let nome = self.navegacao.marcadores.get(i).map(|m| m.titulo.clone()).unwrap_or_default();
                self.navegacao.edicao = Some(EdicaoMarcador::Renomear(i, nome));
            },
            Nv::Nome(t) => match &mut self.navegacao.edicao {
                Some(EdicaoMarcador::Novo(n) | EdicaoMarcador::Dentro(_, n) | EdicaoMarcador::Renomear(_, n)) => *n = t,
                None => {}
            },
            Nv::Cancelar => self.navegacao.edicao = None,
            Nv::Filtro(t) => self.navegacao.filtro = t,
            Nv::EditarLink(i, atual) => self.navegacao.link = Some((self.pagina, i, atual)),
            Nv::TextoLink(t) => if let Some((_, _, x)) = &mut self.navegacao.link { *x = t },
            Nv::CancelarLink => self.navegacao.link = None,
            Nv::ConfirmarLink => if let Some((pagina, i, t)) = self.navegacao.link.take() {
                let destino = destino_do_texto(&t);
                return self.editar(move |pdf| pdf.trocar_destino_link(pagina, i, destino).map(|()| "Destino do link trocado. Ctrl+Z desfaz.".to_string()));
            },
            Nv::ApagarLink(i) => {
                self.navegacao.link = None;
                let pagina = self.pagina;
                return self.editar(move |pdf| pdf.apagar_link(pagina, i).map(|()| "Link apagado. Ctrl+Z desfaz.".to_string()));
            }
            Nv::Confirmar => {
                let Some(edicao) = self.navegacao.edicao.take() else { return Task::none() };
                return match edicao {
                    EdicaoMarcador::Novo(nome) => {
                        let (pagina, depois_de) = (self.pagina, self.navegacao.escolhido);
                        let mut criado = None;
                        let tarefa = self.editar(|pdf| pdf.criar_marcador(&nome, pagina, depois_de)
                            .map(|i| { criado = Some(i); format!("Marcador \"{}\" criado na página {pagina}. Ctrl+Z desfaz.", nome.trim()) }));
                        if criado.is_some() { self.navegacao.escolhido = criado; }
                        tarefa
                    }
                    EdicaoMarcador::Dentro(pai, nome) => {
                        let pagina = self.pagina;
                        let mut criado = None;
                        let tarefa = self.editar(|pdf| pdf.criar_marcador_dentro(&nome, pagina, pai)
                            .map(|i| { criado = Some(i); format!("Marcador \"{}\" criado na página {pagina}, dentro do escolhido. Ctrl+Z desfaz.", nome.trim()) }));
                        if criado.is_some() { self.navegacao.escolhido = criado; }
                        tarefa
                    }
                    EdicaoMarcador::Renomear(i, nome) => self.editar(move |pdf| pdf.renomear_marcador(i, &nome)
                        .map(|()| "Marcador renomeado. Ctrl+Z desfaz.".to_string())),
                };
            }
            Nv::Excluir => if let Some(i) = self.navegacao.escolhido {
                self.navegacao.edicao = None;
                let tarefa = self.editar(move |pdf| pdf.excluir_marcador(i).map(|()| "Marcador excluído (com os de dentro). Ctrl+Z desfaz.".to_string()));
                self.navegacao.escolhido = None;
                return tarefa;
            },
            Nv::Destino(_, _, None) => {}
            Nv::Destino(doc, _, Some(_)) if Some(doc) != self.id_aberto() => self.status = "O documento mudou: o anexo não foi salvo.".into(),
            Nv::Site(url) if !self.config.abrir_sites => self.status = format!("Link para {url} (abrir sites está desligado nas Configurações)."),
            Nv::Site(url) => self.status = match abrir_site(&url) {
                Ok(()) => format!("Abrindo {url} no navegador."),
                Err(e) => format!("Não consegui abrir {url}: {e}"),
            },
            Nv::Destino(_, i, Some(destino)) => if let Some(a) = &self.aberto {
                match a.pdf.anexo(i).and_then(|(_, b)| featherpdf_io::gravar_atomico(&destino, &b)) {
                    Ok(()) => self.avisar_salvo(format!("Anexo salvo em {}", destino.display()), destino),
                    Err(e) => self.status = format!("Não consegui salvar o anexo: {e}"),
                }
            },
        }
        Task::none()
    }

    /// Áreas clicáveis por cima dos links da página `n` (tamanho na tela `l` × `a`), na ferramenta Selecionar.
    // shortcut: relê os links do lopdf a cada desenho da tela (poucas anotações por página); guardar por página se pesar.
    pub fn camada_links(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        if self.ferramenta != crate::Ferramenta::Selecionar { return None; }
        let ab = self.aberto.as_ref()?;
        let links = ab.pdf.links(n);
        if links.is_empty() { return None; }
        let (pagina, giro) = ab.pdf.geometria(n).ok()?;
        let mut pilha = Stack::new().width(l).height(a);
        for (caixa, destino) in links {
            let r = crate::edicao::na_tela(caixa, pagina, giro, self.zoom);
            let (msg, dica) = match destino {
                Destino::Pagina(p) => (Msg::IrPagina(p), format!("Ir para a página {p}")),
                Destino::Endereco(url) => (Msg::Nav(Nv::Site(url.clone())), url),
            };
            let area = mouse_area(Space::new(r.width.max(4.0), r.height.max(4.0))).on_press(msg).cursor_icon(CursorIcon::Pointer);
            pilha = pilha.push(column![
                Space::with_height(r.y.max(0.0)),
                row![Space::with_width(r.x.max(0.0)), tooltip(area, dica, TooltipPosition::Bottom).style(tema::estilo_dica)],
            ]);
        }
        Some(pilha.into())
    }

    pub fn painel_marcadores(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let nv = &self.navegacao;
        let tem_doc = self.aberto.is_some();
        let acao = |icone: &'static str, dica: &'static str, msg: Option<Msg>| -> Element<'_, Msg> {
            let b = button(icones::icone(icone, 16.)).padding([4, 6]).style(tema::estilo_botao_pequeno(false));
            tooltip(match msg { Some(m) => b.on_press(m), None => b }, dica, TooltipPosition::Bottom).style(tema::estilo_dica).into()
        };
        let com_escolha = |msg: Msg| nv.escolhido.map(|_| msg);
        let barra = row![
            acao("plus", "Novo marcador na página atual", tem_doc.then_some(Msg::Nav(Nv::Novo))),
            acao("copy-plus", "Novo marcador dentro do escolhido", com_escolha(Msg::Nav(Nv::NovoDentro))),
            acao("pencil", "Renomear o marcador escolhido", com_escolha(Msg::Nav(Nv::Renomear))),
            acao("link", "O escolhido passa a levar à página atual", com_escolha(Msg::Nav(Nv::PaginaAtual))),
            acao("trash", "Excluir o marcador escolhido (e os de dentro)", com_escolha(Msg::Nav(Nv::Excluir))),
        ].spacing(4);
        let mover = row![
            acao("chevron-up", "Subir", com_escolha(Msg::Nav(Nv::Mover(MoverMarcador::Subir)))),
            acao("chevron-down", "Descer", com_escolha(Msg::Nav(Nv::Mover(MoverMarcador::Descer)))),
            acao("indent-increase", "Pôr dentro do marcador de cima", com_escolha(Msg::Nav(Nv::Mover(MoverMarcador::Dentro)))),
            acao("indent-decrease", "Tirar de dentro (sobe um nível)", com_escolha(Msg::Nav(Nv::Mover(MoverMarcador::Fora)))),
        ].spacing(4);
        let mut corpo = column![barra, mover].spacing(8);
        if let Some(edicao) = &nv.edicao {
            let (rotulo, nome) = match edicao {
                EdicaoMarcador::Novo(n) => (format!("Marcador novo na página {}", self.pagina), n),
                EdicaoMarcador::Dentro(_, n) => (format!("Marcador novo na página {}, dentro do escolhido", self.pagina), n),
                EdicaoMarcador::Renomear(_, n) => ("Novo nome".to_string(), n),
            };
            corpo = corpo.push(column![
                text(rotulo).size(12).color(muted),
                text_input("Nome do marcador", nome).on_input(|t| Msg::Nav(Nv::Nome(t))).on_submit(Msg::Nav(Nv::Confirmar)).padding([5, 8]).size(13),
                row![
                    Space::with_width(Length::FILL),
                    feather_ui::dialogo::botao_secundario("Cancelar", Some(Msg::Nav(Nv::Cancelar))),
                    feather_ui::dialogo::botao_primario("OK", (!nome.trim().is_empty()).then_some(Msg::Nav(Nv::Confirmar))),
                ].spacing(6),
            ].spacing(4));
        }
        if nv.marcadores.is_empty() {
            return corpo.push(text("Este PDF não tem marcadores (sumário). Use + para criar um na página atual.").size(13).color(muted)).into();
        }
        // Destaque: o escolhido; sem escolha, o último marcador que começa até a página atual.
        let atual_i = nv.escolhido.or_else(|| nv.marcadores.iter().enumerate().filter(|(_, m)| m.pagina > 0 && m.pagina <= self.pagina)
            .max_by_key(|(i, m)| (m.pagina, *i)).map(|(i, _)| i));
        corpo = corpo.push(text_input("Procurar nos marcadores", &nv.filtro).on_input(|t| Msg::Nav(Nv::Filtro(t))).padding([5, 8]).size(13));
        let mut lista = column![].spacing(1);
        let mut mostrados = 0;
        for (i, m) in nv.marcadores.iter().enumerate() {
            if !nv.filtro.trim().is_empty() && featherpdf_core::achar_sem_caixa(&m.titulo, nv.filtro.trim()).is_empty() { continue; }
            mostrados += 1;
            let atual = Some(i) == atual_i;
            lista = lista.push(button(row![
                Space::with_width(((m.nivel - 1).min(6) * 14) as f32),
                text(m.titulo.clone()).size(13).width(Length::FILL),
                text(if m.pagina > 0 { m.pagina.to_string() } else { String::new() }).size(12).color(muted),
            ].spacing(6).align_y(Align::Center)).width(Length::FILL).padding([4, 6])
                .style(tema::estilo_botao_pequeno(atual)).on_press(Msg::Nav(Nv::Escolher(i))));
        }
        if mostrados == 0 { lista = lista.push(text(format!("Nenhum marcador com “{}”.", nv.filtro.trim())).size(13).color(muted)); }
        corpo.push(scrollable(lista).height(Length::FILL)).into()
    }

    /// Links da página atual: para onde cada um vai, trocar o destino e apagar.
    pub fn painel_links(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let Some(a) = &self.aberto else { return text("").into() };
        let links = {
            let mut lidos = self.navegacao.links_lidos.borrow_mut();
            match &*lidos {
                Some((p, g, l)) if *p == self.pagina && *g == self.geracao_fundos => l.clone(),
                _ => {
                    let l = std::rc::Rc::new(a.pdf.links_da_pagina(self.pagina));
                    *lidos = Some((self.pagina, self.geracao_fundos, l.clone()));
                    l
                }
            }
        };
        if links.is_empty() {
            return text(format!("A página {} não tem links. Crie um na aba Editar (Link).", self.pagina)).size(13).color(muted).into();
        }
        let botao = |nome: &'static str, msg: Option<Msg>| button(icones::icone(nome, 14.)).padding(4).style(tema::estilo_botao_pequeno(false)).on_press_maybe(msg);
        let mut lista = column![text(format!("{} link(s) na página {}. Número leva à página; o resto vira endereço de site ou e-mail.", links.len(), self.pagina)).size(12).color(muted)].spacing(8);
        for l in links.iter() {
            match &self.navegacao.link {
                Some((p, i, t)) if *p == self.pagina && *i == l.indice => {
                    lista = lista.push(row![
                        text_input("Página ou endereço", t).on_input(|t| Msg::Nav(Nv::TextoLink(t))).on_submit(Msg::Nav(Nv::ConfirmarLink)).padding([4, 8]).size(13),
                        botao("check", (!t.trim().is_empty()).then_some(Msg::Nav(Nv::ConfirmarLink))),
                        botao("x", Some(Msg::Nav(Nv::CancelarLink))),
                    ].spacing(4).align_y(Align::Center));
                }
                _ => {
                    let rotulo = match &l.destino {
                        Some(Destino::Endereco(u)) => u.clone(),
                        Some(Destino::Pagina(p)) => format!("Página {p}"),
                        None => "Outra ação (arquivo, programa ou script): não abre aqui".into(),
                    };
                    lista = lista.push(row![
                        icones::icone("link", 15.),
                        text(rotulo).size(13).width(Length::FILL),
                        botao("pencil", Some(Msg::Nav(Nv::EditarLink(l.indice, destino_em_texto(&l.destino))))),
                        botao("trash", Some(Msg::Nav(Nv::ApagarLink(l.indice)))),
                    ].spacing(6).align_y(Align::Center));
                }
            }
        }
        scrollable(lista).height(Length::FILL).into()
    }

    pub fn painel_anexos(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let anexar = button(row![icones::icone("paperclip", 14.), text("Anexar arquivo…").size(12.5)].spacing(6).align_y(Align::Center))
            .padding([5, 10]).style(tema::estilo_botao_pequeno(false)).on_press_maybe(self.aberto.is_some().then_some(Msg::Nav(Nv::Anexar)));
        if self.navegacao.anexos.is_empty() {
            return column![anexar, text("Este PDF não tem arquivos anexados.").size(13).color(muted)].spacing(10).into();
        }
        let mut lista = column![anexar].spacing(8);
        for (i, a) in self.navegacao.anexos.iter().enumerate() {
            // `usize::MAX`: sem /Size e grande demais para medir (não é descomprimido só para isso).
            let tamanho = if a.tamanho == usize::MAX { "tamanho não informado".to_string() } else if a.tamanho >= 1024 * 1024 { format!("{} MB", numero(a.tamanho as f64 / 1048576.0, 1)) }
                else { format!("{} KB", numero((a.tamanho as f64 / 1024.0).ceil(), 0)) };
            lista = lista.push(row![
                icones::icone("file-export", 16.),
                column![text(a.nome.clone()).size(13), text(tamanho).size(11.5).color(muted)].spacing(2).width(Length::FILL),
                button(text("Salvar…").size(12)).padding([4, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Nav(Nv::Salvar(i))),
                button(icones::icone("trash", 14.)).padding([4, 6]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Nav(Nv::RemoverAnexo(i))),
            ].spacing(8).align_y(Align::Center));
        }
        scrollable(lista).height(Length::FILL).into()
    }
}

/// Abre o endereço no navegador padrão. Só http, https e mailto: link de PDF não abre arquivo nem programa.
fn abrir_site(url: &str) -> Result<(), String> {
    let minusculo = url.to_ascii_lowercase();
    if !["http://", "https://", "mailto:"].iter().any(|e| minusculo.starts_with(e)) { return Err("endereço não é de site".into()); }
    let url = &so_assunto_e_texto(url);
    // O rundll32 recebe o endereço inteiro como um argumento (o `cmd /c start` quebraria no `&`).
    let mut comando = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    comando.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// `mailto:` só com assunto e texto: `?attach=/home/…/.ssh/id_rsa` (de um PDF de fora) anexaria um
/// arquivo seu no rascunho dos programas de e-mail que aceitam `attach`. Outro endereço passa igual.
fn so_assunto_e_texto(url: &str) -> String {
    if !url.to_ascii_lowercase().starts_with("mailto:") { return url.to_string(); }
    let Some((para, consulta)) = url.split_once('?') else { return url.to_string() };
    let ficam: Vec<&str> = consulta.split('&')
        .filter(|p| p.split('=').next().is_some_and(|k| k.eq_ignore_ascii_case("subject") || k.eq_ignore_ascii_case("body"))).collect();
    if ficam.is_empty() { para.to_string() } else { format!("{para}?{}", ficam.join("&")) }
}

#[cfg(test)]
mod testes_site {
    #[test]
    fn mailto_sem_anexo() {
        use super::so_assunto_e_texto as f;
        assert_eq!(f("mailto:a@b.com?subject=Oi&attach=/home/u/.ssh/id_rsa&body=x"), "mailto:a@b.com?subject=Oi&body=x");
        assert_eq!(f("MAILTO:a@b.com?Attach=%2Fetc%2Fpasswd"), "MAILTO:a@b.com");
        assert_eq!(f("https://x.com/?attach=1"), "https://x.com/?attach=1");
    }
    #[test]
    fn site_recusa_o_que_nao_e_endereco() {
        assert!(super::abrir_site("file:///c:/windows/calc.exe").is_err());
        assert!(super::abrir_site("javascript:alert(1)").is_err());
    }
}

#[cfg(test)]
mod testes_link {
    use super::*;

    #[test]
    fn texto_do_campo_vira_destino() {
        assert_eq!(destino_do_texto(" 3 "), Destino::Pagina(3));
        assert_eq!(destino_do_texto("exemplo.com"), Destino::Endereco("exemplo.com".into()));
        assert_eq!(destino_em_texto(&Some(Destino::Pagina(7))), "7");
    }
}
