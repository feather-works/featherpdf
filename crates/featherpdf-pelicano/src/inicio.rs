//! Página inicial: começar (abrir, PDF em branco…), recentes e atalhos da suíte.
//! Desenho de referência: canvas "FeatherPDF — telas", quadro "Página inicial".

use feather_ui::{cromo, icones, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{recentes, App, Msg};

pub const VERMELHO: Color = Color { r: 0xc5 as f32 / 255.0, g: 0x35 as f32 / 255.0, b: 0x2b as f32 / 255.0, a: 1.0 };

/// Fundo rosado dos itens ativos (no escuro, o vermelho transparente).
pub fn fundo_vermelho() -> Color {
    if tema::escuro() { Color { a: 0.22, ..VERMELHO } } else { Color::rgb8(0xf9, 0xeb, 0xea) }
}

pub fn painel(borda_direita: bool) -> impl Fn(&Theme) -> Style {
    move |_| Style {
        background: Some(Background::Color(tema::COR_PAINEL())),
        border: Border { color: if borda_direita { tema::COR_BORDA() } else { Color::TRANSPARENT }, width: 0.0, radius: [0.0; 4] },
        ..Default::default()
    }
}

pub fn linha_vertical<'a>() -> Element<'a, Msg> {
    container(Space::with_width(1)).height(Length::FILL).style(tema::estilo_linha).into()
}

pub fn linha_horizontal<'a>() -> Element<'a, Msg> {
    container(Space::with_height(1)).width(Length::FILL).style(tema::estilo_linha).into()
}

fn cartao(borda: Color, fundo: Color, tracejado: bool) -> impl Fn(&Theme, Status) -> Style {
    move |_, status| {
        let hover = matches!(status, Status::Hovered | Status::Pressed);
        Style {
            background: Some(Background::Color(if hover && !tracejado { tema::COR_HOVER() } else { fundo })),
            text_color: Some(tema::COR_TEXTO()),
            border: Border { color: if hover { VERMELHO } else { borda }, width: if tracejado { 2.0 } else { 1.0 }, radius: [10.0; 4] },
            ..Default::default()
        }
    }
}

fn item_lateral<'a>(rotulo: &'a str, ativo: bool, msg: Option<Msg>) -> Element<'a, Msg> {
    button(text(rotulo).size(13).font(if ativo { Font::SEMIBOLD } else { Font::DEFAULT }))
        .width(Length::FILL).padding([8, 10])
        .style(move |_, status| Style {
            background: if ativo { Some(Background::Color(fundo_vermelho())) }
                else if matches!(status, Status::Hovered) { Some(Background::Color(tema::COR_HOVER())) }
                else { None },
            text_color: Some(if ativo { VERMELHO } else { tema::COR_TEXTO() }),
            border: Border { radius: [6.0; 4], ..Default::default() },
            ..Default::default()
        })
        .on_press_maybe(msg).into()
}

/// Cartão de "Começar": ícone em quadrado colorido e rótulo.
fn comecar<'a>(icone: &str, cor_icone: Color, fundo_icone: Color, rotulo: &'a str, msg: Option<Msg>) -> Element<'a, Msg> {
    let ativo = msg.is_some();
    let quadro = container(icones::icone_traco(icone, 18.).color(cor_icone))
        .width(32).height(32).align_x(Align::Center).align_y(Align::Center)
        .style(move |_| Style {
            background: Some(Background::Color(fundo_icone)),
            border: Border { radius: [8.0; 4], ..Default::default() },
            ..Default::default()
        });
    let conteudo = column![quadro, text(rotulo).size(13)]
        .spacing(8);
    let b = button(container(conteudo).height(Length::FILL).align_y(Align::Center))
        .width(Length::FILL).height(110).padding(14)
        .style(cartao(tema::COR_BORDA(), tema::COR_PAINEL(), false))
        .on_press_maybe(msg);
    if ativo { b.into() } else { em_breve(b.into()) }
}

/// Ferramenta ainda sem função: aparece no lugar, apagada, com a dica "Em breve".
pub fn em_breve<'a>(conteudo: Element<'a, Msg>) -> Element<'a, Msg> {
    tooltip(
        container(conteudo).style(|_| Style { text_color: Some(tema::COR_TEXTO_MUTED()), ..Default::default() }),
        "Em breve",
        TooltipPosition::Bottom,
    ).style(tema::estilo_dica).into()
}

impl App {
    pub fn view_inicio(&self, id: WindowId) -> Element<'_, Msg> {
        let topo = row![
            cromo::emblema(),
            text("FeatherPDF").size(14).font(Font::SEMIBOLD),
            Space::with_width(Length::FILL),
            text(format!("FeatherWorks · v{}", crate::VERSAO)).size(12).color(tema::COR_TEXTO_MUTED()),
            cromo::acao_icone(icones::icone_traco("settings", 19.), "Configurações", Some(Msg::AbrirConfiguracoes)),
        ].spacing(12).padding([0, 12]).align_y(Align::Center).height(cromo::BARRA_ALTURA);
        let topo = cromo::barra_integrada(container(topo).width(Length::FILL).style(tema::estilo_container_topo).into(),
            App::acoes_janela(id));

        let lateral = container(column![
            item_lateral("Início", true, None),
            item_lateral("Abrir…", false, Some(Msg::Abrir)),
            item_lateral("PDF em branco", false, Some(Msg::NovoA4)),
            Space::with_height(Length::FILL),
            item_lateral("Configurações", false, Some(Msg::AbrirConfiguracoes)),
        ].spacing(2)).width(220).height(Length::FILL).padding([14, 10]).style(painel(true));

        let soltar = button(container(column![
            icones::icone_traco("folder-open", 26.).color(VERMELHO),
            text("Abrir ou soltar um PDF").size(13).color(VERMELHO)
                .font(Font::SEMIBOLD),
        ].spacing(6).align_x(Align::Center)).width(Length::FILL).height(Length::FILL).center())
            .width(Length::FILL).height(110).padding(8)
            .style(cartao(VERMELHO, if tema::escuro() { fundo_vermelho() } else { Color::rgb8(0xfd, 0xf6, 0xf5) }, true))
            .on_press(Msg::Abrir);
        let cartoes = row![
            container(soltar).width(Length::Fill(1)),
            container(comecar("file-plus", Color::rgb8(0x8e, 0x44, 0xad), Color::rgb8(0xf3, 0xe9, 0xf7), "PDF em branco", Some(Msg::NovoA4))).width(Length::Fill(1)),
            container(comecar("copy-plus", VERMELHO, fundo_vermelho(), "Juntar arquivos", Some(Msg::Juntar(crate::juntar::Jt::Escolher(false))))).width(Length::Fill(1)),
            container(comecar("photo", Color::rgb8(0x23, 0x6a, 0xc7), Color::rgb8(0xe8, 0xf0, 0xfb), "Imagens para PDF", Some(Msg::Juntar(crate::juntar::Jt::Escolher(true))))).width(Length::Fill(1)),
            container(comecar("screenshot", Color::rgb8(0xb0, 0x7d, 0x0a), Color::rgb8(0xfb, 0xf3, 0xdc), "Scan com OCR", None)).width(Length::Fill(1)),
        ].spacing(12);

        let negrito = Font::SEMIBOLD;
        let titulo_recentes = row![
            text("Recentes").size(16).font(negrito),
            Space::with_width(Length::FILL),
            text_input("Buscar nos recentes", &self.busca_recentes)
                .on_input(Msg::BuscaRecentes).width(240).padding([6, 10]).size(13),
        ].align_y(Align::Center);

        let corpo = container(column![
            text("Começar").size(20).font(negrito),
            cartoes,
            Space::with_height(10),
            titulo_recentes,
            self.tabela_recentes(),
            self.status_inicio(),
        ].spacing(12)).padding(Padding { top: 28.0, right: 36.0, bottom: 20.0, left: 36.0 })
            .width(Length::FILL).height(Length::FILL);

        column![
            topo,
            row![lateral, linha_vertical(), corpo].height(Length::FILL),
        ].into()
    }

    fn status_inicio(&self) -> Element<'_, Msg> {
        if self.status.is_empty() { return Space::with_height(0).into(); }
        text(self.status.clone()).size(12).color(tema::COR_TEXTO_MUTED()).into()
    }

    fn tabela_recentes(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let cab = |t: &'static str, w: Length, direita: bool| -> Element<'_, Msg> {
            let c = container(text(t).size(13).color(muted)).width(w).padding([9, 14]);
            if direita { c.align_x(Align::End).into() } else { c.into() }
        };
        let cabecalho = container(row![
            cab("Nome", Length::Fill(5), false),
            cab("Pasta", Length::Fill(3), false),
            cab("Páginas", Length::Fixed(90.0), true),
            cab("Aberto", Length::Fixed(130.0), false),
        ]).style(|_| Style { background: Some(Background::Color(tema::COR_PAINEL_FAIXA())), ..Default::default() });

        let busca = self.busca_recentes.to_lowercase();
        let agora = recentes::agora();
        let mut linhas = column![];
        let mut quantos = 0;
        for r in self.recentes.itens.iter().filter(|r| busca.is_empty()
            || r.caminho.to_string_lossy().to_lowercase().contains(&busca))
        {
            quantos += 1;
            let nome = r.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let pasta = r.caminho.parent().map(encurtar_pasta).unwrap_or_default();
            let folha = container(Space::new(16, 20)).style(|_| Style {
                background: Some(Background::Color(Color::WHITE)),
                border: Border { color: VERMELHO, width: 1.0, radius: [1.0; 4] },
                ..Default::default()
            });
            let linha = row![
                container(row![folha, text(nome).size(13)].spacing(10).align_y(Align::Center))
                    .width(Length::Fill(5)).padding([8, 14]),
                container(text(pasta).size(13).color(muted)).width(Length::Fill(3)).padding([8, 14]),
                container(text(r.paginas.to_string()).size(13)).width(Length::Fixed(90.0)).padding([8, 14]).align_x(Align::End),
                container(text(recentes::quando(r.aberto_em, agora)).size(13).color(muted)).width(Length::Fixed(130.0)).padding([8, 14]),
            ].align_y(Align::Center);
            linhas = linhas.push(linha_horizontal()).push(
                button(linha).padding(0).width(Length::FILL)
                    .style(|_, status| Style {
                        background: matches!(status, Status::Hovered).then(|| Background::Color(tema::COR_HOVER())),
                        text_color: Some(tema::COR_TEXTO()),
                        ..Default::default()
                    })
                    .on_press(Msg::AbrirCaminho(r.caminho.clone())));
        }
        let vazio: Element<'_, Msg> = if quantos == 0 {
            container(text(if self.recentes.itens.is_empty() {
                "Os PDFs que você abrir aparecem aqui."
            } else { "Nenhum recente com esse nome." }).size(13).color(muted)).padding([14, 14]).into()
        } else { Space::with_height(0).into() };

        container(column![cabecalho, scrollable(linhas).height(Length::Shrink), vazio])
            .width(Length::FILL)
            .style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL())),
                border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [8.0; 4] },
                ..Default::default()
            }).into()
    }
}

/// "~/Documentos/Manutenção" no lugar de "/home/fulano/Documentos/Manutenção".
fn encurtar_pasta(pasta: &std::path::Path) -> String {
    let texto = pasta.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && texto.starts_with(&home) => format!("~{}", &texto[home.len()..]),
        _ => texto,
    }
}

/// A base sozinha, mas com a mesma forma (Stack com a base primeiro) que os diálogos e menus
/// sobrepostos usam: trocar a forma da árvore zera o estado dos widgets e a rolagem da página volta ao topo.
pub fn sem_camada<'a>(base: Element<'a, Msg>) -> Element<'a, Msg> {
    Stack::new().push(base).width(Length::FILL).height(Length::FILL).into()
}
