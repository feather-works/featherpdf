//! Painéis Marcadores (o sumário do PDF, clicar leva à página) e Anexos (salvar o arquivo anexado),
//! e os links do próprio PDF clicáveis na página.

use std::path::PathBuf;

use feather_ui::{icones, tema};
use featherpdf_core::{numero, Anexo, Destino, Marcador};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

#[derive(Debug, Clone)]
pub enum Nv {
    Salvar(usize),
    /// Id do documento (`Aberto::id`), anexo e destino.
    Destino(u64, usize, Option<PathBuf>),
    /// Link de site do PDF (já filtrado para http/https/mailto em `DocumentoPdf::links`).
    Site(String),
}

/// O que os dois painéis mostram, lido quando o painel abre.
#[derive(Debug, Default)]
pub struct Navegacao {
    pub marcadores: Vec<Marcador>,
    pub anexos: Vec<Anexo>,
}

impl App {
    pub fn ler_navegacao(&mut self) {
        self.navegacao = self.aberto.as_ref()
            .map(|a| Navegacao { marcadores: a.pdf.marcadores(), anexos: a.pdf.anexos() })
            .unwrap_or_default();
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
            Nv::Destino(_, _, None) => {}
            Nv::Destino(doc, _, Some(_)) if Some(doc) != self.id_aberto() => self.status = "O documento mudou: o anexo não foi salvo.".into(),
            Nv::Site(url) => self.status = match abrir_site(&url) {
                Ok(()) => format!("Abrindo {url} no navegador."),
                Err(e) => format!("Não consegui abrir {url}: {e}"),
            },
            Nv::Destino(_, i, Some(destino)) => if let Some(a) = &self.aberto {
                self.status = match a.pdf.anexo(i).and_then(|(_, b)| featherpdf_io::gravar_atomico(&destino, &b)) {
                    Ok(()) => format!("Anexo salvo em {}", destino.display()),
                    Err(e) => format!("Não consegui salvar o anexo: {e}"),
                };
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
        if self.navegacao.marcadores.is_empty() {
            return text("Este PDF não tem marcadores (sumário).").size(13).color(muted).into();
        }
        // Só um destaque: o último marcador que começa até a página atual (a seção em que se está).
        let atual_i = self.navegacao.marcadores.iter().enumerate().filter(|(_, m)| m.pagina <= self.pagina)
            .max_by_key(|(i, m)| (m.pagina, *i)).map(|(i, _)| i);
        let mut lista = column![].spacing(1);
        for (i, m) in self.navegacao.marcadores.iter().enumerate() {
            let atual = Some(i) == atual_i;
            lista = lista.push(button(row![
                Space::with_width(((m.nivel - 1).min(6) * 14) as f32),
                text(m.titulo.clone()).size(13).width(Length::FILL),
                text(m.pagina.to_string()).size(12).color(muted),
            ].spacing(6).align_y(Align::Center)).width(Length::FILL).padding([4, 6])
                .style(tema::estilo_botao_pequeno(atual)).on_press(Msg::IrPagina(m.pagina)));
        }
        scrollable(lista).height(Length::FILL).into()
    }

    pub fn painel_anexos(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        if self.navegacao.anexos.is_empty() {
            return text("Este PDF não tem arquivos anexados.").size(13).color(muted).into();
        }
        let mut lista = column![].spacing(8);
        for (i, a) in self.navegacao.anexos.iter().enumerate() {
            let tamanho = if a.tamanho >= 1024 * 1024 { format!("{} MB", numero(a.tamanho as f64 / 1048576.0, 1)) }
                else { format!("{} KB", numero((a.tamanho as f64 / 1024.0).ceil(), 0)) };
            lista = lista.push(row![
                icones::icone("file-export", 16.),
                column![text(a.nome.clone()).size(13), text(tamanho).size(11.5).color(muted)].spacing(2).width(Length::FILL),
                button(text("Salvar…").size(12)).padding([4, 8]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Nav(Nv::Salvar(i))),
            ].spacing(8).align_y(Align::Center));
        }
        scrollable(lista).height(Length::FILL).into()
    }
}

/// Abre o endereço no navegador padrão. Só http, https e mailto: link de PDF não abre arquivo nem programa.
fn abrir_site(url: &str) -> Result<(), String> {
    let minusculo = url.to_ascii_lowercase();
    if !["http://", "https://", "mailto:"].iter().any(|e| minusculo.starts_with(e)) { return Err("endereço não é de site".into()); }
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

#[cfg(test)]
mod testes_site {
    #[test]
    fn site_recusa_o_que_nao_e_endereco() {
        assert!(super::abrir_site("file:///c:/windows/calc.exe").is_err());
        assert!(super::abrir_site("javascript:alert(1)").is_err());
    }
}
