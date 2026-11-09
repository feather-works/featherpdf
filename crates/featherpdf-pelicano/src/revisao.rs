//! Painel Comentários: a lista em conversas (comentário e respostas, com autor e data),
//! responder, marcar estado de revisão (aceito, rejeitado, concluído), filtrar e levar a
//! lista para o FeatherSheet (CSV). O motor está em `featherpdf_core::revisao`.

use std::path::PathBuf;

use feather_ui::{icones as ic, tema};
use featherpdf_core::{comentarios_csv, estado_atual, nome_estado, Anotacao, ESTADOS};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

/// Filtro por estado: todos, só os sem estado (abertos) ou um estado do PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiltroEstado { #[default] Todos, Abertos, Estado(&'static str) }

#[derive(Debug, Clone, Default)]
pub struct Revisao {
    pub filtro: String,
    pub estado: FiltroEstado,
    /// Comentário (página, índice) com a caixa de resposta aberta, e o texto.
    pub respondendo: Option<((u32, usize), String)>,
    /// A caixa aberta edita o texto do próprio comentário em vez de responder.
    pub editando: bool,
}

#[derive(Debug, Clone)]
pub enum Rv {
    Filtro(String),
    Estado(FiltroEstado),
    Responder(u32, usize),
    /// Editar o texto do comentário (página, índice, texto atual).
    Editar(u32, usize, String),
    Texto(String),
    Enviar,
    Cancelar,
    Marcar(u32, usize, &'static str),
    Exportar,
    ExportarPara(Option<PathBuf>),
    ParaSheet,
}

pub fn id_resposta() -> &'static str { "comentario-resposta" }

fn autor() -> String {
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default()
}

/// O comentário (ou alguma resposta dele) contém o texto do filtro.
fn combina(lista: &[Anotacao], a: &Anotacao, filtro: &str) -> bool {
    let tem = |x: &Anotacao| [x.tipo, &x.autor, &x.texto].iter().any(|t| t.to_lowercase().contains(filtro));
    filtro.is_empty() || tem(a) || lista.iter().any(|r| r.pagina == a.pagina && r.responde_a == Some(a.indice) && tem(r))
}

impl App {
    pub fn update_revisao(&mut self, m: Rv) -> Task<Msg> {
        let r = &mut self.revisao;
        match m {
            Rv::Filtro(t) => r.filtro = t,
            Rv::Estado(e) => r.estado = e,
            Rv::Responder(p, i) => { r.respondendo = Some(((p, i), String::new())); r.editando = false; return crate::focar(id_resposta()); }
            Rv::Editar(p, i, atual) => { r.respondendo = Some(((p, i), atual)); r.editando = true; return crate::focar(id_resposta()); }
            Rv::Texto(t) => if let Some((_, v)) = &mut r.respondendo { *v = t },
            Rv::Cancelar => { r.respondendo = None; r.editando = false; }
            Rv::Enviar => {
                let Some(((p, i), texto)) = r.respondendo.take() else { return Task::none() };
                if std::mem::take(&mut r.editando) {
                    return self.editar(move |pdf| pdf.editar_comentario(p, i, &texto).map(|_| "Comentário alterado. Ctrl+Z desfaz.".into()));
                }
                let quem = autor();
                return self.editar(move |pdf| pdf.responder(p, i, &texto, &quem).map(|_| "Resposta gravada no comentário. Ctrl+Z desfaz.".into()));
            }
            Rv::Marcar(p, i, estado) => {
                let quem = autor();
                return self.editar(move |pdf| pdf.marcar_estado(p, i, estado, &quem)
                    .map(|_| if estado == "None" { "Estado removido.".into() } else { format!("Marcado como {}.", nome_estado(estado).to_lowercase()) }));
            }
            Rv::Exportar => {
                let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
                let sugestao = format!("{} - comentários.csv", nome.strip_suffix(".pdf").unwrap_or(&nome));
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().add_filter("CSV", &["csv"]).set_file_name(sugestao).save_file().await.map(|h| h.path().to_path_buf())
                }, |d| Msg::Rev(Rv::ExportarPara(d)));
            }
            Rv::ExportarPara(None) => {}
            Rv::ExportarPara(Some(destino)) => {
                let csv = self.aberto.as_ref().map(|a| comentarios_csv(&a.pdf.anotacoes())).unwrap_or_default();
                self.status = match featherpdf_io::gravar_atomico(&destino, csv.as_bytes()) {
                    Ok(()) => format!("Comentários salvos em {}", destino.display()),
                    Err(e) => format!("Não consegui gravar a lista: {e}"),
                };
            }
            Rv::ParaSheet => {
                let csv = self.aberto.as_ref().map(|a| comentarios_csv(&a.pdf.anotacoes())).unwrap_or_default();
                let destino = crate::recentes::pasta_temporaria().map(|p| p.join("comentarios.csv"));
                self.status = destino.and_then(|destino| std::fs::write(&destino, csv).map(|_| destino)).map_err(|e| format!("Não consegui gravar a lista: {e}"))
                    .and_then(|destino| crate::abrir_no("feathersheet", "FeatherSheet", &destino))
                    .map_or_else(|e| e, |_| "Lista de comentários aberta no FeatherSheet.".into());
            }
        }
        Task::none()
    }

    pub fn painel_comentarios(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let r = &self.revisao;
        // shortcut: relê as anotações a cada desenho do painel; guardar a lista se PDF com milhares de comentários pesar.
        let lista = self.aberto.as_ref().map(|a| a.pdf.anotacoes()).unwrap_or_default();
        let filtro = r.filtro.to_lowercase();
        let principais: Vec<&Anotacao> = lista.iter().filter(|a| a.responde_a.is_none()).filter(|a| combina(&lista, a, &filtro))
            .filter(|a| match r.estado {
                FiltroEstado::Todos => true,
                FiltroEstado::Abertos => estado_atual(&lista, a).is_none(),
                FiltroEstado::Estado(e) => estado_atual(&lista, a) == Some(e),
            }).collect();

        let chip = |rotulo: &'static str, f: FiltroEstado| button(text(rotulo).size(11.5)).padding([3, 8])
            .style(tema::estilo_botao_pequeno(r.estado == f)).on_press(Msg::Rev(Rv::Estado(f)));
        let mut chips = row![chip("Todos", FiltroEstado::Todos), chip("Abertos", FiltroEstado::Abertos)].spacing(4);
        for (k, v) in ESTADOS.iter().take(3) { chips = chips.push(chip(v, FiltroEstado::Estado(k))); }
        let topo = column![
            text_input("Filtrar por texto, autor ou tipo", &r.filtro).on_input(|t| Msg::Rev(Rv::Filtro(t))).padding([5, 8]).size(13),
            scrollable(chips).direction(Direction::Horizontal),
            row![
                text(format!("{} de {}", principais.len(), lista.iter().filter(|a| a.responde_a.is_none()).count())).size(12).color(muted),
                Space::with_width(Length::FILL),
                button(text("CSV…").size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno(false))
                    .on_press_maybe((!lista.is_empty()).then_some(Msg::Rev(Rv::Exportar))),
                button(text("Para o FeatherSheet").size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno(false))
                    .on_press_maybe((!lista.is_empty()).then_some(Msg::Rev(Rv::ParaSheet))),
            ].spacing(4).align_y(Align::Center),
        ].spacing(6);

        let mut col = column![].spacing(6);
        for a in &principais {
            let (pagina, indice) = (a.pagina, a.indice);
            let estado = estado_atual(&lista, a);
            let mut cab = row![text(a.tipo).size(12).color(muted)].spacing(6).align_y(Align::Center);
            if let Some(e) = estado {
                cab = cab.push(container(text(nome_estado(e).to_string()).size(10.5).color(tema::COR_AZUL_ACENTO())).padding([1, 6]).style(|_| Style {
                    border: Border { color: tema::COR_AZUL_ACENTO(), width: 1.0, radius: [8.0; 4] }, ..Default::default() }));
            }
            cab = cab.push(Space::with_width(Length::FILL)).push(text(format!("p. {pagina}")).size(11).color(muted));
            let mut info = column![cab].spacing(2);
            if !a.texto.is_empty() { info = info.push(text(a.texto.clone()).size(13)); }
            let assinatura = [a.autor.as_str(), a.data.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" · ");
            if !assinatura.is_empty() { info = info.push(text(assinatura).size(11).color(muted)); }
            let mut cartao = column![row![
                button(info).padding(0).width(Length::FILL).style(tema::estilo_botao_pequeno(false)).on_press(Msg::IrPagina(pagina)),
                button(ic::icone("trash", 13.0)).padding(3).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Com(crate::comentar::Cm::Apagar(pagina, indice))),
            ].spacing(6).align_y(Align::Start)].spacing(6);
            // Respostas, recuadas, na ordem em que foram gravadas.
            for resp in lista.iter().filter(|x| x.pagina == pagina && x.responde_a == Some(indice) && x.estado.is_none()) {
                let assinatura = [resp.autor.as_str(), resp.data.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" · ");
                cartao = cartao.push(row![Space::with_width(10), container(column![
                    text(resp.texto.clone()).size(12.5),
                    text(assinatura).size(10.5).color(muted),
                ].spacing(1)).padding([4, 8]).width(Length::FILL).style(|_| Style {
                    border: Border { color: tema::COR_BORDA(), width: 0.0, radius: [4.0; 4] },
                    background: Some(Background::Color(tema::COR_HOVER())), ..Default::default() })]);
            }
            match &r.respondendo {
                Some(((p, i), texto)) if (*p, *i) == (pagina, indice) => {
                    let (dica, rotulo) = if r.editando { ("Texto do comentário (Enter grava)", "Gravar") } else { ("Escreva a resposta e tecle Enter", "Responder") };
                    // Editar pode esvaziar a nota (some o texto, fica o ícone); responder pede texto.
                    let pode = r.editando || !texto.trim().is_empty();
                    cartao = cartao.push(text_input(dica, texto).id(id_resposta())
                        .on_input(|t| Msg::Rev(Rv::Texto(t))).on_submit(Msg::Rev(Rv::Enviar)).padding([5, 8]).size(12.5))
                        .push(row![Space::with_width(Length::FILL),
                            button(text("Cancelar").size(11.5)).padding([3, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Rev(Rv::Cancelar)),
                            button(text(rotulo).size(11.5)).padding([3, 8]).style(tema::estilo_botao_pequeno(true))
                                .on_press_maybe(pode.then_some(Msg::Rev(Rv::Enviar))),
                        ].spacing(4));
                }
                _ => {
                    let mut acoes = row![
                        button(text("Responder").size(11.5)).padding([3, 8]).style(tema::estilo_botao_pequeno(false))
                            .on_press(Msg::Rev(Rv::Responder(pagina, indice))),
                        button(text("Editar").size(11.5)).padding([3, 8]).style(tema::estilo_botao_pequeno(false))
                            .on_press(Msg::Rev(Rv::Editar(pagina, indice, a.texto.clone()))),
                    ].spacing(3);
                    for (k, v) in ESTADOS.iter().take(3) {
                        let ativo = estado == Some(*k);
                        acoes = acoes.push(button(text(*v).size(11.5)).padding([3, 6]).style(tema::estilo_botao_pequeno(ativo))
                            .on_press(Msg::Rev(Rv::Marcar(pagina, indice, if ativo { "None" } else { k }))));
                    }
                    cartao = cartao.push(scrollable(acoes).direction(Direction::Horizontal));
                }
            }
            col = col.push(container(cartao).padding([6, 8]).style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL())),
                border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [6.0; 4] }, ..Default::default()
            }));
        }
        if lista.is_empty() {
            col = col.push(text("Nenhum comentário. Use Destacar, Nota, Caixa de texto, Desenhar ou Carimbo na aba Comentar.").size(13).color(muted));
        } else if principais.is_empty() {
            col = col.push(text("Nenhum comentário com esse filtro.").size(13).color(muted));
        }
        column![topo, scrollable(col).height(Length::FILL)].spacing(10).into()
    }
}
