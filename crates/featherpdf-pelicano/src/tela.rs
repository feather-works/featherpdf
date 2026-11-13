//! O desenho da janela: barra do documento, abas, faixa, painéis, mesa, trilho e rodapé.
//! As ferramentas são as da versão Iced (cada módulo desenha a sua parte); aqui fica a moldura.

use feather_ui::componentes::botao_icone;
use feather_ui::icones::{icone, icone_colorido, icone_traco};
use feather_ui::marca;
use feather_ui::tema::*;
use pelicano::column;
use pelicano::*;

use crate::compat::*;
use crate::comentar::Anotador as A;
use crate::contexto::{Alvo, Ct};
use crate::formularios::Fm;
use crate::inserir::{Ins, Insercao, Menu};
use crate::medir::{Med, Medidor};
use crate::organizar::{Dialogos, Pag, Tamanho};
use crate::tarja::Tj;
use crate::{Aba, App, Auxiliar, Disposicao, Ferramenta, Msg, Painel, Suspenso, ID_MESA, VAO_PAGINAS, ZOOM_MAX, ZOOM_MIN};
use feather_draw::{Alinhar, Eixo};
use featherpdf_core::{FormatoNumero, Modelo, PosicaoNumero, TipoMedida};

const VERDE: &str = "#21865c";
const AZUL: &str = "#2179a7";
const LARANJA: &str = "#d68126";
const ROXO: &str = "#7d5cc6";
const VERMELHO: &str = "#ca5555";
const RAIO: f32 = 10.0;
const ALTURA_FAIXA: f32 = 118.0;

const ABAS: [(Aba, &str); 9] = [
    (Aba::Inicio, "Página Inicial"), (Aba::Paginas, "Páginas"), (Aba::Editar, "Editar"), (Aba::Comentar, "Comentar"),
    (Aba::Formularios, "Formulários"), (Aba::Medir, "Medir"), (Aba::Proteger, "Proteger"), (Aba::Converter, "Converter"), (Aba::Exibir, "Exibir"),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NivelZoom(u32);
impl std::fmt::Display for NivelZoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}%", self.0) }
}
const NIVEIS: [NivelZoom; 11] = [NivelZoom(25), NivelZoom(50), NivelZoom(75), NivelZoom(100), NivelZoom(125), NivelZoom(150), NivelZoom(200), NivelZoom(300), NivelZoom(400), NivelZoom(600), NivelZoom(800)];

/// Cartão branco que flutua sobre o fundo cinza da janela.
fn cartao(_: &Theme) -> Style {
    Style::default().background(COR_PAINEL()).border(Border::rounded(RAIO).color(COR_BORDA_SUAVE(), 1.0))
        .shadow(Shadow { color: Color { a: 0.04, ..Color::BLACK }, offset: Point::new(0.0, 1.0), blur: 3.0 })
}

/// Caixa com borda fina: campo de página, grupo do zoom.
fn caixa(_: &Theme) -> Style {
    Style::default().background(COR_PAINEL()).border(Border::rounded(7.0).color(COR_BORDA(), 1.0))
}

fn divisoria<'a>(altura: f32) -> Element<'a, Msg> {
    container(Space::new(1.0, altura)).style(|_| Style::default().background(COR_BORDA_SUAVE())).into()
}

pub fn janela(app: &App) -> Element<'_, Msg> {
    if app.aberto.is_none() { return app.view_inicio(); }
    let corpo: Element<'_, Msg> = if app.apresentacao.is_some() {
        mesa(app)
    } else if app.leitura {
        let sair = button(text("Sair da leitura (Esc)").size(12)).padding([5u16, 12]).style(estilo_botao_secundario).on_press(Msg::Leitura);
        column![barra_documento(app), stack![mesa(app), container(sair).width(Length::FILL).align_x(Align::End).padding(12u16)]].into()
    } else {
        let mut meio = row![].spacing(10).height(Length::FILL);
        if app.ferramentas_abertas { meio = meio.push(painel_ferramentas(app)); }
        meio = meio.push(mesa(app));
        if let Some(p) = app.painel {
            // Borda arrastável: alarga ou estreita o painel, como no Acrobat.
            meio = meio.push(mouse_area(Space::new(4.0, Length::FILL)).cursor_icon(CursorIcon::EwResize).on_press(Msg::LarguraPainel(Some(0.0))))
                .push(painel_direito(app, p));
        }
        meio = meio.push(trilho(app));
        let mut corpo = column![barra_documento(app), barra_abas(app)];
        if app.faixa_aberta { corpo = corpo.push(container(faixa(app)).padding([0u16, 12])); }
        corpo.push(container(meio).padding([10u16, 12]).height(Length::FILL))
            .push(container(rodape(app)).padding(Padding { top: 0.0, right: 12.0, bottom: 10.0, left: 12.0 }))
            .into()
    };
    let tela = app.com_pergunta_fechar(app.com_dialogo_campo(app.com_dialogo_comentario(app.com_protecao(app.com_dialogo_escala(
        app.com_menu_ins(app.com_menu_contexto(app.com_dialogo_link(corpo))))))));
    let mut camadas = stack![app.com_pedido_senha(app.com_propriedades(tela)), alcas(Msg::Puxar)];
    if app.menu { camadas = camadas.push(menu_arquivo(app)); }
    if !app.busca_comandos.trim().is_empty() { camadas = camadas.push(resultados_comandos(app)); }
    if let Some((qual, canto)) = app.suspenso { camadas = camadas.push(menu_suspenso(qual, canto)); }
    container(camadas).width(Length::FILL).height(Length::FILL).clip(true)
        .style(|_| Style::default().background(COR_FUNDO_JANELA()).border(Border::default().color(COR_BORDA(), 1.0)).text_color(COR_TEXTO()))
        .into()
}

/// Nome comprido cortado com "…" no fim (o `ellipsis` sozinho empurrava o "salvo" e o × para fora da aba).
fn encurtar(nome: &str, max: usize) -> String {
    if nome.chars().count() <= max { nome.to_string() } else { nome.chars().take(max - 1).collect::<String>() + "…" }
}

/// Linha de cima: pena, abas dos documentos, "+" e os botões da janela; o resto arrasta.
pub(crate) fn barra_documento(app: &App) -> Element<'_, Msg> {
    // shortcut: abas demais passam da largura da janela (cada uma encolhe só até o nome com "…"); rolar a faixa de abas se alguém abrir dezenas.
    let mut abas = row![].spacing(2).height(Length::FILL).align(Align::End);
    for i in 0..app.guias.len() {
        let ativa = i == app.ativa;
        let fechar = button(icone_traco("x", 13.)).padding(4u16)
            .style(|_, s| Style::default().border(Border::rounded(5.0)).text_color(COR_TEXTO_MUTED())
                .background(if matches!(s, Status::Hovered | Status::Pressed) { COR_HOVER() } else { Color::TRANSPARENT }))
            .on_press(Msg::FecharGuia(i));
        // Sem documento (página inicial): a aba é o "Início", sem × nem "salvo".
        let inicio = ativa && app.aberto.is_none();
        let nome = if inicio { "Início".to_string() } else { encurtar(&app.nome_da_aba(i), if ativa { 34 } else { 24 }) };
        let mut conteudo = row![text(nome).size(13.5).font(if ativa { Font::SEMIBOLD } else { Font::DEFAULT }).no_wrap().ellipsis()]
            .spacing(8).align(Align::Center);
        if ativa && !inicio {
            let alterado = app.aberto.as_ref().is_some_and(|a| a.pdf.modificado() || a.pdf.origem().is_none());
            conteudo = conteudo.push(text(if alterado { "• não salvo" } else { "salvo" }).size(11.5).color(COR_TEXTO_MUTED()).no_wrap());
        }
        if !inicio { conteudo = conteudo.push(Element::from(fechar).access_label("Fechar aba")); }
        let aba = container(conteudo)
            .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 18.0 }).height(36.0).max_width(if ativa { 400.0 } else { 240.0 }).align_y(Align::Center)
            .style(move |_| {
                let s = Style::default().border(Border { radius: [10.0, 10.0, 0.0, 0.0], ..Default::default() });
                if ativa { s.background(COR_PAINEL()) } else { s }
            });
        abas = abas.push(mouse_area(aba).on_press(Msg::Guia(i)).cursor_icon(CursorIcon::Default));
    }
    let novo = Element::from(button(icone_traco("plus", 16.)).padding([7u16, 9])
        .style(|_, s| Style::default().background(if s == Status::Hovered { COR_HOVER() } else { COR_PAINEL() }).border(Border::rounded(8.0)).text_color(COR_TEXTO()))
        .on_press(Msg::NovoA4)).access_label("Novo PDF A4");
    let esquerda = container(row![
        container(marca::pena_atual(26.)).padding([0u16, 10]),
        abas,
        novo,
    ].spacing(8).align(Align::Center).height(Length::FILL)).padding(Padding { top: 6.0, left: 6.0, ..Padding::ZERO });
    // A área de arrastar fica por baixo de tudo: um `mouse_area` em volta de conteúdo elástico
    // ocupava a linha inteira e empurrava os botões da janela para fora.
    container(stack![
        mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::Arrastar(WindowId::MAIN)).on_double_click(Msg::Maximizar(WindowId::MAIN)),
        row![esquerda, horizontal_space(),
            // Na página inicial não há a faixa (que tem a engrenagem): ela fica aqui em cima.
            Element::from(if app.aberto.is_none() {
                container(Element::from(button(icone_traco("settings", 18.)).padding([6u16, 8])
                    .style(|_, s| Style::default().border(Border::rounded(8.0)).text_color(COR_TEXTO_MUTED())
                        .background(if matches!(s, Status::Hovered | Status::Pressed) { COR_HOVER() } else { Color::TRANSPARENT }))
                    .on_press(Msg::AbrirConfiguracoes)).access_label("Configurações")).padding([0u16, 6])
            } else { container(Space::new(0.0, 0.0)) }),
            feather_ui::cromo::controles_janela(App::acoes_janela(WindowId::MAIN))].height(Length::FILL).align(Align::Center),
    ]).height(44.0).width(Length::FILL).style(|_| Style::default().background(COR_CROMO())).into()
}

/// Menu, abas da faixa, pesquisa de comandos, desfazer, tema e configurações.
fn barra_abas(app: &App) -> Element<'_, Msg> {
    let menu = button(row![icone("menu-2", 19.), text("Menu").size(13.5)].spacing(10).align(Align::Center))
        .padding([8u16, 12]).style(estilo_botao_pequeno(app.menu)).on_press(Msg::Menu);
    let mut abas = row![].spacing(2).align(Align::End);
    for (aba, rotulo) in ABAS {
        let ativa = app.aba == aba;
        let b = button(text(rotulo).size(13.5).no_wrap().font(if ativa { Font::SEMIBOLD } else { Font::DEFAULT }))
            .padding([8u16, 11])
            .style(move |_, status| {
                let s = Style::default().border(Border { radius: [8.0, 8.0, 0.0, 0.0], ..Default::default() });
                if ativa {
                    // O sublinhado é o fim do degradê: uma linha num `column` alargava a aba.
                    let (fundo, linha) = (COR_AZUL_FUNDO(), COR_AZUL_ACENTO());
                    s.background(Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, fundo), (0.93, fundo), (0.93, linha), (1.0, linha)] })
                        .text_color(COR_ACENTO_TEXTO(fundo))
                } else if status == Status::Hovered {
                    s.background(COR_HOVER()).text_color(COR_TEXTO())
                } else {
                    s.text_color(COR_TEXTO())
                }
            })
            .on_press(Msg::MudarAba(aba));
        abas = abas.push(b);
    }
    let pesquisa = container(row![
        icone("search", 17.),
        { let primeiro = comandos_achados(app).into_iter().next().map(|(_, _, m)| Msg::Comando(Box::new(m)));
          let t = text_input("Pesquisar comandos…", app.busca_comandos.as_str()).on_input(Msg::BuscaComandos).on_submit_maybe(primeiro);
          if app.foco_pesquisa > 0 { t.autofocus(app.foco_pesquisa) } else { t } }
            .size(12.5).padding([5u16, 2]).width(Length::FILL)
            .style(|_, _| Style::default().text_color(COR_TEXTO())).placeholder_color(COR_TEXTO_MUTED()),
        text("Ctrl+K").size(11.5).color(COR_TEXTO_MUTED()).no_wrap(),
    ].spacing(8).align(Align::Center))
        .padding([3u16, 12]).width(236.0)
        .style(|_| Style::default().background(COR_PAINEL()).border(Border::rounded(8.0).color(COR_BORDA(), 1.0)));
    let b = |nome: &str, dica: &str, msg: Option<Msg>| botao_icone(icone(nome, 20.), dica, msg, false);
    container(row![
        // Janela estreita: as abas rolam de lado e os botões da direita continuam à vista.
        scrollable(row![menu, abas].spacing(6).align(Align::Center)).direction(Direction::Horizontal).scrollbar_width(3.0).width(Length::FILL).height(Length::Shrink),
        pesquisa, Space::new(8.0, 0.0),
        b("arrow-back-up", "Desfazer (Ctrl+Z)", app.pode_desfazer().then_some(Msg::Desfazer)),
        b("arrow-forward-up", "Refazer (Ctrl+Y)", app.pode_refazer().then_some(Msg::Refazer)),
        b("device-floppy", "Salvar (Ctrl+S)", Some(Msg::Salvar)),
        b(if app.tema_escuro { "sun" } else { "moon" }, if app.tema_escuro { "Tema claro" } else { "Tema escuro" }, Some(Msg::AlternarTema)),
        b("help", "Ajuda (F1)", Some(Msg::AbrirAjuda)),
        b("settings", "Configurações", Some(Msg::AbrirConfiguracoes)),
        b(if app.faixa_aberta { "chevron-up" } else { "chevron-down" }, if app.faixa_aberta { "Recolher faixa" } else { "Expandir faixa" }, Some(Msg::AlternarFaixa)),
    ].spacing(6).align(Align::Center))
        .padding(Padding { top: 6.0, right: 14.0, bottom: 6.0, left: 14.0 }).width(Length::FILL).style(|_| Style::default().background(COR_CROMO())).into()
}

fn desenho(nome: &str, cor: Option<&str>, tamanho: f32) -> SvgView {
    cor.map_or_else(|| icone(nome, tamanho), |c| icone_colorido(nome, tamanho, c))
}

/// Botão alto da faixa: ícone grande em cima, rótulo embaixo. Sem mensagem, fica apagado.
fn grande<'a>(nome: &str, cor: Option<&str>, rotulo: &'a str, msg: Option<Msg>, ativo: bool) -> Element<'a, Msg> {
    button(column![desenho(nome, cor, 26.), text(rotulo).size(12.5).no_wrap()].spacing(7).align(Align::Center))
        .padding([10u16, 10]).style(move |t, s| estilo_botao_grande(ativo)(t, s).border(Border::rounded(9.0)))
        .on_press_maybe(msg).into()
}

fn pequeno<'a>(nome: &str, cor: Option<&str>, rotulo: &'a str, msg: Option<Msg>, ativo: bool) -> Element<'a, Msg> {
    button(row![desenho(nome, cor, 19.), text(rotulo).size(12.5).no_wrap()].spacing(10).align(Align::Center))
        .padding([5u16, 10]).style(move |t, s| estilo_botao_pequeno(ativo)(t, s).border(Border::rounded(7.0)))
        .on_press_maybe(msg).into()
}

/// A lista suspensa com cara de botão da faixa (sem borda, fundo só no hover).
fn estilo_suspenso(_: &Theme, status: Status) -> Style {
    let s = Style::default().border(Border::rounded(6.0)).text_color(COR_TEXTO());
    if matches!(status, Status::Hovered | Status::Pressed) { s.background(COR_HOVER()) } else { s }
}

/// Botão pequeno com ▾ que abre o menu suspenso `qual` embaixo dele.
fn suspenso<'a>(app: &App, nome: &str, cor: Option<&str>, rotulo: &'static str, qual: Suspenso) -> Element<'a, Msg> {
    let aberto = app.suspenso.is_some_and(|s| s.0 == qual);
    button(row![desenho(nome, cor, 19.), text(rotulo).size(12.5).no_wrap(), icone_traco(if aberto { "chevron-up" } else { "chevron-down" }, 13.)]
        .spacing(8).align(Align::Center))
        .padding([5u16, 10]).style(move |t, s| estilo_botao_pequeno(aberto)(t, s).border(Border::rounded(7.0)))
        .on_press(Msg::Suspenso(Some(qual))).into()
}

fn nome_modelo(m: Modelo) -> (&'static str, &'static str) {
    match m {
        Modelo::Cadastro => ("user", "Cadastro"), Modelo::Recibo => ("currency-dollar", "Recibo"),
        Modelo::Declaracao => ("signature", "Declaração"), Modelo::Presenca => ("list-details", "Presença"),
        Modelo::Pesquisa => ("checks", "Pesquisa"),
    }
}

pub(crate) fn dividir() -> Msg {
    Msg::Pag(Pag::Abrir(Dialogos::Dividir { modo: crate::organizar::ModoDivisao::ACada, texto: "1".into() }))
}

/// As opções de cada menu suspenso: (rótulo, ação).
fn opcoes(qual: Suspenso) -> Vec<(&'static str, Msg)> {
    match qual {
        Suspenso::Girar => vec![("Girar à direita", Msg::Girar(90)), ("Girar à esquerda", Msg::Girar(-90)), ("Girar 180°", Msg::Girar(180))],
        Suspenso::Excluir => vec![("Excluir as páginas escolhidas", Msg::Excluir), ("Excluir as outras", Msg::ExcluirOutras)],
        Suspenso::Inserir => vec![("Página em branco", Msg::Pag(Pag::InserirBranco)), ("Duplicar esta página", Msg::Pag(Pag::Duplicar)),
            ("Outro PDF…", Msg::Pag(Pag::Juntar))],
        Suspenso::Extrair => vec![("Páginas escolhidas…", Msg::Pag(Pag::Extrair)), ("Dividir o documento…", dividir())],
        Suspenso::Modelos => Modelo::TODOS.iter().map(|&m| (nome_modelo(m).1, Msg::Form(Fm::Modelo(m)))).collect(),
    }
}

/// O menu suspenso aberto, flutuando sobre a janela; clicar fora fecha.
fn menu_suspenso<'a>(qual: Suspenso, canto: Point) -> Element<'a, Msg> {
    let opcoes = opcoes(qual);
    // Largura pelo rótulo mais longo (13 px de letra dão ~7 px por caractere).
    let largura = opcoes.iter().map(|(r, _)| r.chars().count()).max().unwrap_or(10) as f32 * 7.2 + 40.0;
    let mut itens = column![].spacing(0).width(largura);
    for (rotulo, msg) in opcoes {
        itens = itens.push(button(row![text(rotulo).size(13).no_wrap(), horizontal_space()].width(Length::FILL)).width(Length::FILL).padding([7u16, 12])
            .style(estilo_item_menu(false, true)).on_press(Msg::DoSuspenso(Box::new(msg))));
    }
    let menu = container(itens).padding(4u16).width(largura + 8.0).style(|_| Style::default().background(COR_PAINEL())
        .border(Border::rounded(8.0).color(COR_BORDA_SUAVE(), 1.0))
        .shadow(Shadow { color: COR_SOMBRA(), offset: Point::new(0.0, 4.0), blur: 12.0 }));
    stack![
        mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::Suspenso(None)),
        container(opaque(menu)).padding(Padding { top: canto.y.max(0.0), left: canto.x.max(0.0), ..Padding::ZERO }),
    ].into()
}

/// Grupo da faixa: os botões no meio, a legenda embaixo e a divisória à direita.
fn grupo<'a>(itens: impl Into<Element<'a, Msg>>, rotulo: &'static str) -> Element<'a, Msg> {
    row![
        column![
            container(itens).height(Length::FILL).align_y(Align::Center),
            text(rotulo).size(12).no_wrap().color(COR_TEXTO_MUTED()),
        ].align(Align::Center).spacing(4).padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 12.0 }).height(Length::FILL),
        container(divisoria(84.0)).height(Length::FILL).align_y(Align::Center),
    ].height(Length::FILL).into()
}

fn grupo_zoom(app: &App) -> Element<'_, Msg> {
    let botao = |nome: &str, rotulo: &str, msg: Msg| botao_icone(icone(nome, 19.), rotulo, Some(msg), false);
    let caixa_zoom = container(row![
        botao("zoom-out", "Diminuir zoom (Ctrl+-)", Msg::ZoomMenos),
        divisoria(26.0),
        // Digitar o percentual (Enter aplica); fora da edição mostra o zoom atual.
        container(text_input("", app.campo_zoom.clone().unwrap_or_else(|| format!("{:.0}", app.zoom * 100.0)))
            .on_input(Msg::CampoZoom).on_submit(Msg::ConfirmarZoom).size(13.5).padding([4u16, 4]).width(44.0)
            .style(|_, _| Style::default().text_color(COR_TEXTO()))).width(50.0).align_x(Align::Center),
        text("%").size(13.5),
        divisoria(26.0),
        botao_icone(icone_traco("plus", 19.), "Aumentar zoom (Ctrl++)", Some(Msg::ZoomMais), false),
    ].align(Align::Center)).padding(2u16).width(Length::Shrink).style(caixa);
    let niveis = pick_list(&NIVEIS[..], None, |n: NivelZoom| Msg::Zoom(n.0 as f32 / 100.0)).placeholder("").width(34.0).padding([6u16, 6])
        .style(estilo_suspenso);
    grupo(column![
        row![caixa_zoom, niveis].spacing(4).align(Align::Center),
        row![
            pequeno("arrows-horizontal", None, "Largura", Some(Msg::AjustarLargura), false),
            pequeno("arrows-vertical", None, "Altura", Some(Msg::AjustarAltura), false),
            pequeno("arrows-maximize", None, "Inteira", Some(Msg::AjustarPagina), false),
            pequeno("zoom-in", None, "Área", Some(Msg::Regiao(crate::regiao::Rr::Ferramenta(crate::regiao::Uso::Zoom))),
                app.regiao() == Some(crate::regiao::Uso::Zoom)),
        ],
    ].spacing(4), "Zoom")
}

fn grupo_exibicao(app: &App) -> Element<'_, Msg> {
    let modo = |nome: &str, rotulo: &'static str, d: Disposicao| pequeno(nome, None, rotulo, Some(Msg::Disposicao(d)), app.disposicao == d);
    grupo(column![
        modo("layout-sidebar", "Uma página", Disposicao::UmaPagina),
        modo("book-2", "Duas páginas", Disposicao::DuasPaginas),
        pequeno("eye", None, "Leitura", Some(Msg::Leitura), false),
    ].spacing(0), "Exibição")
}

fn faixa(app: &App) -> Element<'_, Msg> {
    let grupos: Vec<Element<'_, Msg>> = match app.aba {
        Aba::Inicio => faixa_inicio(app),
        Aba::Paginas => faixa_paginas(app),
        Aba::Editar => faixa_editar(app),
        Aba::Comentar => faixa_comentar(app),
        Aba::Formularios => faixa_formularios(app),
        Aba::Medir => faixa_medir(app),
        Aba::Proteger => faixa_proteger(app),
        Aba::Converter => faixa_converter(app),
        Aba::Exibir => faixa_exibir(app),
    };
    let linha = grupos.into_iter().fold(row![].height(Length::FILL), |r, g| r.push(g));
    // Rolagem só de reserva (janela estreita): fina e discreta, sem a barra clara no tema escuro.
    let mut rolagem = scrollable(linha).direction(Direction::Horizontal).scrollbar_width(3.0);
    if escuro() { rolagem = rolagem.scrollbar_color(Color::rgb8(0x45, 0x48, 0x4e)); }
    container(rolagem).height(ALTURA_FAIXA).width(Length::FILL).padding([0u16, 8]).clip(true).style(cartao).into()
}

/// Clicar de novo na ferramenta ativa volta a Selecionar.
fn alternar(app: &App, f: Ferramenta) -> (Option<Msg>, bool) {
    let ativa = app.ferramenta == f;
    (Some(Msg::Ferramenta(if ativa { Ferramenta::Selecionar } else { f })), ativa)
}

fn faixa_inicio(app: &App) -> Vec<Element<'_, Msg>> {
    let f = app.ferramenta;
    let comentar = |nome, cor, rotulo, a: A| { let (m, at) = alternar(app, Ferramenta::Comentar(a)); grande(nome, cor, rotulo, m, at) };
    vec![
        grupo(row![
            grande("pointer", None, "Selecionar", Some(Msg::Ferramenta(Ferramenta::Selecionar)), f == Ferramenta::Selecionar),
            grande("hand-stop", None, "Mão", Some(Msg::Ferramenta(Ferramenta::Mao)), f == Ferramenta::Mao),
            grande("text-size", None, "Texto", Some(Msg::Ins(Ins::Ferramenta(Insercao::Texto))), app.insercao() == Some(Insercao::Texto)),
        ].spacing(4), "Ferramentas"),
        grupo(row![
            grande("printer", None, "Imprimir", Some(Msg::Imp(crate::imprimir::Im::Abrir)), app.janelas.aberta(Auxiliar::Imprimir)),
            comentar("highlight", None, "Destacar", A::Destacar),
            comentar("message-2", Some(LARANJA), "Nota", A::Nota),
            comentar("pencil", None, "Desenhar", A::Desenhar),
        ].spacing(4), "Anotações"),
        grupo(row![
            column![
                suspenso(app, "rotate-clockwise", Some(VERDE), "Girar", Suspenso::Girar),
                suspenso(app, "trash", Some(VERMELHO), "Excluir", Suspenso::Excluir),
            ].spacing(6),
            column![
                suspenso(app, "file-plus", None, "Inserir", Suspenso::Inserir),
                suspenso(app, "crop", None, "Extrair", Suspenso::Extrair),
            ].spacing(6),
        ].spacing(14), "Páginas"),
        grupo_zoom(app),
        grupo_exibicao(app),
    ]
}

fn faixa_paginas(app: &App) -> Vec<Element<'_, Msg>> {
    let alvo = app.alvo().len();
    let total = app.total_paginas();
    let dialogo = |d: fn(&Dialogos) -> bool| app.org.dialogo.as_ref().is_some_and(d) && app.painel == Some(Painel::Paginas);
    let p = |d: Dialogos| Some(Msg::Pag(Pag::Abrir(d)));
    vec![
        grupo(row![
            grande("arrow-back-up", Some(VERDE), "Girar ←", Some(Msg::GirarEsquerda), false),
            grande("rotate-clockwise", Some(VERDE), "Girar →", Some(Msg::GirarDireita), false),
            grande("trash", Some(VERMELHO), "Excluir", (alvo < total as usize).then_some(Msg::Excluir), false),
            grande("copy", None, "Duplicar", Some(Msg::Pag(Pag::Duplicar)), false),
            column![
                pequeno("copy-plus", None, "Copiar páginas", Some(Msg::Pag(Pag::Copiar)), false),
                pequeno("cut", None, "Recortar páginas", (alvo < total as usize).then_some(Msg::Pag(Pag::Recortar)), false),
                pequeno("clipboard", None, "Colar páginas", app.paginas_copiadas.is_some().then_some(Msg::Pag(Pag::Colar)), false),
            ],
        ].spacing(4), "Seleção"),
        grupo(row![
            grande("file-plus", None, "Inserir", Some(Msg::Pag(Pag::InserirBranco)), false),
            grande("files", None, "Juntar", Some(Msg::Pag(Pag::Juntar)), false),
            grande("file-export", None, "Extrair", Some(Msg::Pag(Pag::Extrair)), false),
            grande("cut", None, "Dividir", (total > 1).then(dividir), false),
        ].spacing(4), "Documento"),
        grupo(row![
            column![
                pequeno("square", None, "Caixas da página…", p(Dialogos::Margens { mm: Default::default(), todas: true, caixa: featherpdf_core::CaixaPagina::Corte }),
                    dialogo(|d| matches!(d, Dialogos::Margens { .. }))),
                pequeno("list-numbers", None, "Números de página…", p(Dialogos::Numeros {
                    formato: FormatoNumero::DeTotal, posicao: PosicaoNumero::RodapeCentro, desde: "1".into(), primeiro: "1".into() }),
                    dialogo(|d| matches!(d, Dialogos::Numeros { .. }))),
                pequeno("pointer", None, "Selecionar tudo", Some(Msg::Pag(Pag::SelecionarTudo)), false),
            ],
            column![
                pequeno("arrows-maximize", None, "Padronizar tamanho…", p(Dialogos::Padronizar(Tamanho::A4)), dialogo(|d| matches!(d, Dialogos::Padronizar(_)))),
                pequeno("copy", None, "Frente e verso…", p(Dialogos::FrenteVerso { invertidos: true }), dialogo(|d| matches!(d, Dialogos::FrenteVerso { .. }))),
                pequeno("tag", None, "Numeração lógica…", p(Dialogos::Rotulos { de: "1".into(), ate: total.max(1).to_string(),
                    estilo: featherpdf_core::EstiloRotulo::RomanoMinusculo, prefixo: String::new(), primeiro: "1".into() }),
                    dialogo(|d| matches!(d, Dialogos::Rotulos { .. }))),
            ],
        ].spacing(4), "Formato"),
        grupo(column![
            pequeno("list-numbers", None, "Numeração Bates…", p(Dialogos::Bates { prefixo: String::new(), primeiro: "1".into(), digitos: 6, posicao: PosicaoNumero::RodapeDireita }),
                dialogo(|d| matches!(d, Dialogos::Bates { .. }))),
            pequeno("columns", None, "Dividir colunas…", p(Dialogos::Colunas { colunas: 2, todas: true }), dialogo(|d| matches!(d, Dialogos::Colunas { .. }))),
        ], "Lote e leitura"),
        grupo(row![
            grande("wordart", Some(AZUL), "Marca d'água", Some(Msg::Marca(crate::marcas::Mc::Abrir(crate::marcas::Modo::Marca))),
                app.painel == Some(Painel::Marcas) && app.marcas.modo == crate::marcas::Modo::Marca),
            grande("header-footer", None, "Cabeçalho e rodapé", Some(Msg::Marca(crate::marcas::Mc::Abrir(crate::marcas::Modo::Cabecalho))),
                app.painel == Some(Painel::Marcas) && app.marcas.modo == crate::marcas::Modo::Cabecalho),
        ].spacing(4), "Marcas"),
    ]
}

fn faixa_editar(app: &App) -> Vec<Element<'_, Msg>> {
    let f = app.ferramenta;
    let ins = |i: Insercao| Some(Msg::Ins(Ins::Ferramenta(i)));
    let tipografia = |nome, rotulo, e: featherpdf_core::EstiloTexto| pequeno(nome, None, rotulo, app.tem_texto_escolhido().then_some(Msg::Obj(crate::edicao::Obj::Tipografia(e))), false);
    let estilo = |escala: Option<f32>, letras: Option<f32>| featherpdf_core::EstiloTexto { escala, letras, ..Default::default() };
    let encaixe = |rotulo: &'static str, i: usize| checkbox(rotulo, app.ins.encaixe[i]).on_toggle(move |_| Msg::Ins(Ins::Encaixe(i))).size(14).text_size(12.5);
    let rotulos: Vec<&'static str> = crate::edicao::PASSOS_GRADE.iter().map(|p| p.0).collect();
    let passo = crate::edicao::PASSOS_GRADE.iter().find(|p| (p.1 - app.ins.passo_grade).abs() < 0.01).map(|p| p.0);
    let grade = suspensa(rotulos.clone(), passo, move |r| Msg::Ins(Ins::PassoGrade(rotulos.iter().position(|x| *x == r).unwrap_or(1))))
        .text_size(12).padding([1, 6]);
    vec![
        grupo(row![
            grande("pointer", None, "Selecionar", Some(Msg::Ferramenta(Ferramenta::Selecionar)), f == Ferramenta::Selecionar),
            grande("edit", Some(VERDE), "Editar texto", Some(Msg::Ferramenta(Ferramenta::Editar)), f == Ferramenta::Editar),
        ].spacing(4), "Ferramenta"),
        grupo(row![
            grande("text-size", None, "Texto", ins(Insercao::Texto), app.insercao() == Some(Insercao::Texto)),
            grande("photo", Some(AZUL), "Imagem", ins(Insercao::Imagem), app.insercao() == Some(Insercao::Imagem)),
            column![
                pequeno("table", None, "Tabela", Some(Msg::Tabelas(crate::tabelas_vivas::Tv::Abrir)), app.painel == Some(Painel::Tabelas)),
                pequeno("math-function", None, "Equação", Some(Msg::Equacao(crate::equacao::Eq::Abrir)), app.painel == Some(Painel::Equacao)),
                pequeno("link", None, "Link", ins(Insercao::Link), app.insercao() == Some(Insercao::Link)),
            ],
            column![
                pequeno("shapes", None, "Formas", Some(Msg::Ins(Ins::AbrirMenu(Menu::Formas))), matches!(app.insercao(), Some(Insercao::Forma(_)))),
                pequeno("icons", None, "Ícones", Some(Msg::Ins(Ins::AbrirMenu(Menu::Icones))), matches!(app.insercao(), Some(Insercao::Icone(_)))),
                pequeno("signature", None, "Assinatura", Some(Msg::Assin(crate::assinar::As::Abrir)), app.painel == Some(Painel::Assinar)),
            ],
        ].spacing(4), "Inserir"),
        grupo(app.grupo_estilo(), "Estilo"),
        grupo(grande("photo-effects", None, "Editar imagem", app.imagem_escolhida().map(|_| Msg::Imagem(crate::imagem::Im::Abrir)), app.painel == Some(Painel::Imagem)), "Imagem"),
        grupo(column![
            pequeno("layout-align-top", None, "Para a frente", app.escolhidos.is_some().then_some(Msg::Obj(crate::edicao::Obj::Ordem(true))), false),
            pequeno("layout-align-bottom", None, "Para trás", app.escolhidos.is_some().then_some(Msg::Obj(crate::edicao::Obj::Ordem(false))), false),
        ], "Ordem"),
        grupo(row![
            column![tipografia("font-increase", "Letra maior", estilo(Some(1.1), None)), tipografia("font-decrease", "Letra menor", estilo(Some(1.0 / 1.1), None))],
            column![tipografia("text-wrap", "Mais espaço", estilo(None, Some(0.5))), tipografia("line-height", "Menos espaço", estilo(None, Some(-0.3)))],
        ].spacing(4), "Texto"),
        grupo(column![
            row![app.botao_alinhar("objeto_alinhar_esquerda", "Alinhar à esquerda", Ins::Alinhar(Alinhar::Esquerda)),
                app.botao_alinhar("objeto_alinhar_centro", "Centralizar na horizontal", Ins::Alinhar(Alinhar::CentroHorizontal)),
                app.botao_alinhar("objeto_alinhar_direita", "Alinhar à direita", Ins::Alinhar(Alinhar::Direita)),
                app.botao_alinhar("distribuir_horizontal", "Distribuir na horizontal (3 ou mais)", Ins::Distribuir(Eixo::Horizontal))].spacing(2),
            row![app.botao_alinhar("objeto_alinhar_topo", "Alinhar ao topo", Ins::Alinhar(Alinhar::Topo)),
                app.botao_alinhar("objeto_alinhar_meio", "Centralizar na vertical", Ins::Alinhar(Alinhar::CentroVertical)),
                app.botao_alinhar("objeto_alinhar_base", "Alinhar à base", Ins::Alinhar(Alinhar::Base)),
                app.botao_alinhar("distribuir_vertical", "Distribuir na vertical (3 ou mais)", Ins::Distribuir(Eixo::Vertical))].spacing(2),
            row![
                pequeno("file", None, "À página", Some(Msg::Ins(Ins::AlinharPagina(true))), app.ins.alinhar_pagina),
                pequeno("pointer", None, "À seleção", Some(Msg::Ins(Ins::AlinharPagina(false))), !app.ins.alinhar_pagina),
            ],
        ].spacing(4), "Alinhar"),
        grupo(column![
            encaixe("Encaixar em objetos", 0),
            encaixe("Encaixar na margem", 1),
            row![encaixe("Grade", 2), grade].spacing(6).align(Align::Center),
        ].spacing(5), "Encaixe"),
        grupo(column![
            checkbox("Réguas", app.ins.reguas).on_toggle(|_| Msg::Ins(Ins::Reguas)).size(14),
            text(if app.ins.reguas { "Arraste da régua para criar guia" } else { "" }).size(11).color(COR_TEXTO_MUTED()),
            pequeno("trash", None, "Limpar guias", (!app.ins.guias.is_empty()).then_some(Msg::Ins(Ins::LimparGuias)), false),
        ].spacing(4), "Guias"),
    ]
}

fn faixa_comentar(app: &App) -> Vec<Element<'_, Msg>> {
    let botao = |nome, cor, rotulo, a: A| { let (m, at) = alternar(app, Ferramenta::Comentar(a)); grande(nome, cor, rotulo, m, at) };
    let fino = |nome, rotulo, a: A| { let (m, at) = alternar(app, Ferramenta::Comentar(a)); pequeno(nome, None, rotulo, m, at) };
    vec![
        grupo(row![
            botao("highlight", None, "Destacar", A::Destacar),
            column![fino("underline", "Sublinhar", A::Sublinhar), fino("strikethrough", "Riscar", A::Riscar), fino("underline-double", "Ondulado", A::Ondular)],
            botao("edit", None, "Substituir", A::Substituir),
            botao("message-2", Some(LARANJA), "Nota", A::Nota),
            botao("text-box", None, "Caixa", A::Caixa),
            botao("pencil", None, "Desenhar", A::Desenhar),
            botao("tag", Some(ROXO), "Carimbo", A::Carimbo),
        ].spacing(4), "Anotar"),
        grupo(row![
            grande("list-details", None, "Lista", Some(Msg::AlternarPainel(Painel::Comentarios)), app.painel == Some(Painel::Comentarios)),
            grande("columns", None, "Comparar", Some(Msg::Comparar(crate::comparar::Cmp::Abrir)), app.painel == Some(Painel::Comparar)),
        ].spacing(4), "Comentários"),
    ]
}

fn faixa_formularios(app: &App) -> Vec<Element<'_, Msg>> {
    let tem_campos = app.form.campos.as_ref().is_some_and(|c| !c.is_empty());
    let atual = app.novo_campo();
    let mut campos = row![].spacing(4);
    for (nome, rotulo, n) in app.faixa_campos() {
        campos = campos.push(grande(nome, None, rotulo,
            Some(Msg::Ferramenta(if atual == Some(n) { Ferramenta::Selecionar } else { Ferramenta::Campo(n) })), atual == Some(n)));
    }
    vec![
        grupo(column![
            suspenso(app, "forms", None, "Modelos", Suspenso::Modelos),
            pequeno("wand", Some(ROXO), "Detectar campos", Some(Msg::Form(Fm::Detectar)), false),
        ].spacing(6), "Criar"),
        grupo(campos, "Campos"),
        grupo(grande("math-function", None, "Regras e Tab", tem_campos.then_some(Msg::Regras(crate::regras_campos::Rg::Abrir)), app.painel == Some(Painel::RegrasCampos)), "Validar e calcular"),
        grupo(row![
            grande("table", Some(VERDE), "Para o FeatherSheet", tem_campos.then_some(Msg::Form(Fm::ParaSheet)), false),
            column![
                pequeno("folder", None, "Pasta de fichas", Some(Msg::Form(Fm::Pasta)), false),
                pequeno("arrow-down", None, "Importar dados", tem_campos.then_some(Msg::FormDados(crate::form_dados::Fd::Importar)), false),
                pequeno("file-export", None, "Exportar dados", tem_campos.then_some(Msg::FormDados(crate::form_dados::Fd::Exportar)), false),
            ],
            grande("eraser", None, "Limpar", tem_campos.then_some(Msg::Form(Fm::Limpar)), false),
        ].spacing(4), "Dados"),
    ]
}

fn faixa_medir(app: &App) -> Vec<Element<'_, Msg>> {
    let m = app.medidor();
    let ferramenta = |nome, rotulo, f: Medidor| grande(nome, None, rotulo, Some(Msg::Medir(Med::Ferramenta(f))), m == Some(f));
    vec![
        grupo(column![
            row![
                ferramenta("wand", "Calibrar", Medidor::Calibrar),
                grande("percentage", None, "Escala…", Some(Msg::Medir(Med::AbrirEscala)), false),
            ].spacing(4),
            text(app.medidas.escala.rotulo()).size(11).color(COR_TEXTO_MUTED()),
        ].align(Align::Center), "Escala"),
        grupo(row![
            ferramenta("ruler-measure", "Distância", Medidor::Medir(TipoMedida::Distancia)),
            ferramenta("shapes", "Perímetro", Medidor::Medir(TipoMedida::Perimetro)),
            ferramenta("square", "Área", Medidor::Medir(TipoMedida::Area)),
            ferramenta("arrow-up-right", "Ângulo", Medidor::Medir(TipoMedida::Angulo)),
            grande("list", None, "Medidas", Some(Msg::AlternarPainel(Painel::Medidas)), app.painel == Some(Painel::Medidas)),
        ].spacing(4), "Medir"),
        grupo(row![
            ferramenta("list-numbers", "Contar", Medidor::Medir(TipoMedida::Contagem)),
            ferramenta("circle", "Nuvem", Medidor::Nuvem),
        ].spacing(4), "Revisão"),
    ]
}

fn faixa_proteger(app: &App) -> Vec<Element<'_, Msg>> {
    use crate::proteger::Pr;
    let (tarja, tarja_ativa) = alternar(app, Ferramenta::Tarja);
    vec![
        grupo(row![
            grande("writing-sign", Some(AZUL), "Assinar", Some(Msg::Assin(crate::assinar::As::Abrir)), app.painel == Some(Painel::Assinar)),
            column![
                pequeno("circle-check", None, "Validar", Some(Msg::Assin(crate::assinar::As::Validar)), app.painel == Some(Painel::Assinaturas)),
                pequeno("checks", None, "Conferir à mão", Some(Msg::Reconhecer(crate::reconhecer::Rc::Abrir)), app.painel == Some(Painel::Reconhecer)),
            ],
        ].spacing(4), "Assinaturas"),
        grupo(row![
            grande("lock", None, "Senha", Some(Msg::Prot(Pr::AbrirSenha)), false),
            column![
                pequeno("sparkles", None, "Limpar e sanitizar", Some(Msg::Sanitizar(crate::sanitizar::Sz::Abrir)), app.painel == Some(Painel::Sanitizar)),
                pequeno("layout-align-bottom", None, "Achatar", Some(Msg::Achatar), false),
            ],
        ].spacing(4), "Proteger"),
        grupo(row![
            grande("square", Some(VERMELHO), "Marcar área", tarja, tarja_ativa),
            column![
                pequeno("search", None, "Buscar e tarjar", Some(Msg::Tarja(Tj::Buscar)), false),
                pequeno("list", None, "Padrões", Some(Msg::Tarja(Tj::Padroes)), false),
                pequeno("eye-off", None, "Marcadas", Some(Msg::AlternarPainel(Painel::Tarja)), app.painel == Some(Painel::Tarja)),
            ],
        ].spacing(4), "Tarja definitiva"),
    ]
}

fn faixa_converter(app: &App) -> Vec<Element<'_, Msg>> {
    use crate::exportar::{Ex, Formato};
    use crate::regiao::{Rr, Uso};
    let regiao = |nome, rotulo, u: Uso| pequeno(nome, None, rotulo, Some(Msg::Regiao(Rr::Ferramenta(u))), app.regiao() == Some(u));
    vec![
        grupo(row![
            grande("file-text", Some(AZUL), "FeatherDocs", Some(Msg::ParaFeatherDocs), false),
            column![
                pequeno("photo", None, "Imagens", Some(Msg::Exp(Ex::Abrir(Formato::Imagens))), false),
                regiao("crop", "Copiar área", Uso::Imagem),
                regiao("table", "Tabela do scan", Uso::Tabela),
            ],
            column![
                pequeno("file-export", None, "Word, Excel, texto…", Some(Msg::Exp(Ex::Abrir(Formato::Word))), app.janelas.aberta(Auxiliar::Converter)),
                pequeno("eye", None, "Ver texto", Some(Msg::AlternarPainel(Painel::Texto)), app.painel == Some(Painel::Texto)),
                // Quarto item na primeira coluna passava da altura da faixa e cobria o nome do grupo.
                regiao("chart-bar", "Gráfico para dados", Uso::Grafico),
            ],
        ].spacing(4), "Exportar"),
        grupo(row![
            grande("file-zip", Some(ROXO), "Comprimir", Some(Msg::Prot(crate::proteger::Pr::AbrirComprimir)), false),
            column![
                pequeno("text-scan-2", None, "OCR", Some(Msg::Ocr(crate::ocr::Oc::Abrir)), app.janelas.aberta(Auxiliar::Ocr)),
                regiao("scan-eye", "OCR da área", Uso::Ocr),
                pequeno("checks", None, "Preflight", Some(Msg::Preflight(crate::preflight::Pf::Abrir)), app.painel == Some(Painel::Preflight)),
            ],
            column![
                pequeno("text-size", None, "Embutir fontes", Some(Msg::EmbutirFontes), false),
                pequeno("brush", None, "Limpar caderno", Some(Msg::LimparCaderno), false),
            ],
        ].spacing(4), "Otimizar"),
        grupo(row![
            grande("device-floppy", None, "Salvar como", Some(Msg::SalvarComo), false),
            column![
                pequeno("history", None, "Versões", Some(Msg::Versoes(crate::versoes::Vs::Abrir)), app.painel == Some(Painel::Versoes)),
                pequeno("terminal", None, "Script", Some(Msg::Scripts(crate::scripts::Sc::Abrir)), app.painel == Some(Painel::Scripts)),
                pequeno("list", None, "Fluxos em lote", Some(Msg::Fluxos(crate::fluxos::Fx::Abrir)), app.janelas.aberta(Auxiliar::Fluxos)),
            ],
            pequeno("world", None, "Para a web", Some(Msg::Copia(crate::copias::Cp::Pedir(crate::copias::Copia::Web))), false),
        ].spacing(4), "Arquivo"),
    ]
}

fn faixa_exibir(app: &App) -> Vec<Element<'_, Msg>> {
    vec![
        grupo_zoom(app),
        grupo(row![
            grande("layout-sidebar", None, "Uma página", Some(Msg::Disposicao(Disposicao::UmaPagina)), app.disposicao == Disposicao::UmaPagina),
            grande("book-2", None, "Duas páginas", Some(Msg::Disposicao(Disposicao::DuasPaginas)), app.disposicao == Disposicao::DuasPaginas),
            grande("eye", None, "Leitura", Some(Msg::Leitura), false),
            grande("player-play", None, "Apresentar (F5)", Some(Msg::Apresentar), false),
            grande("palette", None, "Página escura", Some(Msg::ModoEscuro), app.modo_escuro),
        ].spacing(4), "Disposição"),
        grupo(row![
            grande("layout-sidebar", None, "Ferramentas", Some(Msg::AlternarFerramentas), app.ferramentas_abertas),
            grande("layout-grid", None, "Miniaturas", Some(Msg::AlternarPainel(Painel::Miniaturas)), app.painel == Some(Painel::Miniaturas)),
            grande("message-circle", None, "Comentários", Some(Msg::AlternarPainel(Painel::Comentarios)), app.painel == Some(Painel::Comentarios)),
        ].spacing(4), "Painéis"),
        grupo(row![
            grande(if app.tema_escuro { "sun" } else { "moon" }, None, if app.tema_escuro { "Tema claro" } else { "Tema escuro" }, Some(Msg::AlternarTema), false),
            grande("maximize", None, "Tela cheia", Some(Msg::TelaCheia), false),
            grande("help", None, "Ajuda (F1)", Some(Msg::AbrirAjuda), false),
            grande("device-desktop", None, "Monitor (F12)", Some(Msg::AbrirMonitor), false),
        ].spacing(4), "Janela"),
    ]
}

/// Uma ferramenta da lista lateral: (seção, ícone, cor própria, rótulo, mensagem, acesa).
type Item = (&'static str, &'static str, Option<&'static str>, &'static str, Msg, bool);

fn itens_ferramentas(app: &App) -> Vec<Item> {
    use crate::proteger::{Janela, Pr};
    let aba = |a: Aba| (Msg::MudarAba(a), app.aba == a);
    let tarja = app.ferramenta == Ferramenta::Tarja || app.painel == Some(Painel::Tarja);
    let item = |s, i, c, r, (m, a): (Msg, bool)| (s, i, c, r, m, a);
    vec![
        item("PÁGINAS", "layout-grid", None, "Organizar páginas", aba(Aba::Paginas)),
        item("PÁGINAS", "files", None, "Juntar arquivos", (Msg::Pag(Pag::Juntar), false)),
        item("EDITAR E REVISAR", "pencil", Some(VERDE), "Editar PDF", aba(Aba::Editar)),
        item("EDITAR E REVISAR", "message-2", Some(LARANJA), "Comentar", aba(Aba::Comentar)),
        item("EDITAR E REVISAR", "writing-sign", None, "Preencher e assinar", (Msg::Assin(crate::assinar::As::Abrir), app.painel == Some(Painel::Assinar))),
        item("EDITAR E REVISAR", "forms", None, "Formulários", aba(Aba::Formularios)),
        item("EDITAR E REVISAR", "ruler-measure", None, "Medir e contar", aba(Aba::Medir)),
        item("PROTEGER", "lock", None, "Senha e permissões", (Msg::Prot(Pr::AbrirSenha), matches!(app.protecao, Some(Janela::Senha(_))))),
        item("PROTEGER", "eraser", Some(VERMELHO), "Tarja definitiva", (Msg::MudarAba(Aba::Proteger), tarja)),
        item("CONVERTER E OTIMIZAR", "file-export", None, "Converter e exportar",
            (Msg::Exp(crate::exportar::Ex::Abrir(crate::exportar::Formato::Docs)), app.janelas.aberta(Auxiliar::Converter))),
        item("CONVERTER E OTIMIZAR", "scan", Some(AZUL), "OCR", (Msg::Ocr(crate::ocr::Oc::Abrir), app.janelas.aberta(Auxiliar::Ocr))),
        item("CONVERTER E OTIMIZAR", "file-zip", Some(ROXO), "Comprimir", (Msg::Prot(Pr::AbrirComprimir), matches!(app.protecao, Some(Janela::Comprimir(_))))),
    ]
}

fn cabecalho_painel<'a>(titulo: &'a str, fechar: Msg) -> Element<'a, Msg> {
    row![text(titulo).size(16).font(Font::SEMIBOLD), horizontal_space(),
        botao_icone(icone("x", 17.), "Fechar painel", Some(fechar), false)].align(Align::Center).into()
}

fn painel_ferramentas(app: &App) -> Element<'_, Msg> {
    let filtro = container(row![
        icone("search", 16.),
        text_input("Filtrar ferramentas…", app.filtro_ferramentas.as_str()).on_input(Msg::FiltroFerramentas).size(12.5).padding([6u16, 2]).width(Length::FILL)
            .style(|_, _| Style::default().text_color(COR_TEXTO())).placeholder_color(COR_TEXTO_MUTED()),
    ].spacing(8).align(Align::Center)).padding([2u16, 10])
        .style(|_| Style::default().border(Border::rounded(7.0).color(COR_BORDA(), 1.0)));
    let procura = app.filtro_ferramentas.trim().to_lowercase();
    let mut lista = column![].spacing(0);
    let mut secao_atual = "";
    for (secao, nome, cor, rotulo, msg, ativo) in itens_ferramentas(app) {
        if !rotulo.to_lowercase().contains(&procura) { continue; }
        if secao != secao_atual {
            if !secao_atual.is_empty() {
                lista = lista.push(container(container(Space::new(Length::FILL, 1.0)).width(Length::FILL).style(|_| Style::default().background(COR_BORDA_SUAVE())))
                    .padding(Padding { top: 8.0, bottom: 4.0, left: 8.0, right: 8.0 }));
            }
            secao_atual = secao;
            lista = lista.push(container(text(secao).size(12).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED())).padding(Padding { top: 8.0, bottom: 6.0, left: 10.0, right: 0.0 }));
        }
        lista = lista.push(button(row![desenho(nome, cor, 22.), text(rotulo).size(13.5), horizontal_space()].spacing(16).align(Align::Center).width(Length::FILL))
            .width(Length::FILL).padding([5u16, 10]).style(estilo_item_menu(ativo, true)).on_press(msg));
    }
    container(column![
        cabecalho_painel("Ferramentas", Msg::AlternarFerramentas),
        filtro,
        scrollable(lista).scrollbar_width(4.0).height(Length::FILL),
    ].spacing(12)).padding(Padding { top: 16.0, right: 12.0, bottom: 10.0, left: 14.0 }).width(270.0).height(Length::FILL).style(cartao).into()
}

fn painel_direito(app: &App, p: Painel) -> Element<'_, Msg> {
    let (titulo, corpo): (&str, Element<'_, Msg>) = match p {
        Painel::Comentarios => ("Comentários", app.painel_comentarios()),
        Painel::Miniaturas => ("Miniaturas", miniaturas(app)),
        Painel::Marcadores => ("Marcadores", app.painel_marcadores()),
        Painel::Links => ("Links da página", app.painel_links()),
        Painel::Camadas => ("Camadas", app.painel_camadas()),
        Painel::Anexos => ("Anexos", app.painel_anexos()),
        Painel::Medidas => ("Medidas", app.painel_medidas()),
        Painel::Tarja => ("Marcadas para tarjar", app.painel_tarja()),
        Painel::Assinar => ("Preencher e assinar", app.painel_assinar()),
        Painel::Assinaturas => ("Assinaturas do documento", app.painel_assinaturas()),
        Painel::Paginas => app.painel_paginas(),
        Painel::Preflight => ("Preflight", app.painel_preflight()),
        Painel::RegrasCampos => ("Regras dos campos", app.painel_regras()),
        Painel::Versoes => ("Versões no arquivo", app.painel_versoes()),
        Painel::Reconhecer => ("Conferir assinaturas", app.painel_reconhecer()),
        Painel::Scripts => ("Script", app.painel_scripts()),
        Painel::Equacao => ("Equação", app.painel_equacao()),
        Painel::Tabelas => ("Tabelas da página", app.painel_tabelas_vivas()),
        Painel::Comparar => ("Comparar versões", app.painel_comparar()),
        Painel::Regiao => ("Texto da área", app.painel_regiao()),
        Painel::Sanitizar => ("Limpar e sanitizar", app.painel_sanitizar()),
        Painel::Marcas => ("Cabeçalho, rodapé e marca d'água", app.painel_marcas()),
        Painel::Imagem => ("Imagem", app.painel_imagem()),
        Painel::Texto => ("Texto da página", column![
            row![text(format!("Página {}", app.texto_da_pagina.0)).size(12).color(COR_TEXTO_MUTED()), horizontal_space(),
                button(text("Copiar texto").size(12)).padding([4u16, 10]).style(estilo_botao_pequeno(false)).on_press(Msg::CopiarTexto)]
                .align(Align::Center),
            scrollable(text(app.texto_da_pagina.1.as_str()).size(13)).height(Length::FILL),
        ].spacing(8).into()),
    };
    container(column![cabecalho_painel(titulo, Msg::AlternarPainel(p)), corpo].spacing(12))
        .padding(Padding { top: 16.0, right: 12.0, bottom: 10.0, left: 14.0 }).width(app.largura_painel).height(Length::FILL).style(cartao).into()
}

fn miniaturas(app: &App) -> Element<'_, Msg> {
    let Some(aberto) = &app.aberto else { return Space::new(0.0, 0.0).into() };
    let mut lista = column![].spacing(10).align(Align::Center).width(Length::FILL);
    for (i, (l, a)) in aberto.tamanhos.iter().enumerate() {
        let n = i as u32 + 1;
        let largura = 150.0_f32.min(150.0 * l / a);
        let altura = largura * a / l;
        // A miniatura (pedida em volta da página atual) ou, enquanto não chega, o desenho da mesa.
        let figura: Element<'_, Msg> = match (app.org.miniaturas.get(&n), app.desenhadas.get(&n)) {
            (Some(m), _) => image(m.clone()).width(largura).height(altura).into(),
            (None, Some(d)) if d.trecho.is_none() => image(d.imagem.clone()).width(largura).height(altura).into(),
            _ => Space::new(largura, altura).into(),
        };
        let ativa = n == app.pagina;
        lista = lista.push(mouse_area(button(column![
            container(figura).padding(1u16).style(move |_| Style::default().background(Color::WHITE)
                .shadow(Shadow { color: Color { a: 0.12, ..Color::BLACK }, offset: Point::new(0.0, 1.0), blur: 4.0 })
                .border(Border::rounded(2.0).color(if ativa { COR_AZUL_ACENTO() } else { COR_BORDA() }, if ativa { 2.0 } else { 1.0 }))),
            text(n.to_string()).size(12).color(if ativa { COR_AZUL_ACENTO() } else { COR_TEXTO_MUTED() }),
        ].spacing(4).align(Align::Center)).padding(4u16).style(estilo_item_menu(false, true)).on_press(Msg::IrPagina(n)))
            .on_right_press(Msg::Ctx(Ct::Abrir(Alvo::Pagina(n)))));
    }
    scrollable(lista).scrollbar_width(4.0).height(Length::FILL).into()
}

fn mesa(app: &App) -> Element<'_, Msg> {
    let conteudo: Element<'_, Msg> = if app.aba == Aba::Paginas && !app.leitura {
        app.organizador()
    } else {
        let paginas = responsive(move |tamanho| { app.tamanho_mesa.set(tamanho); paginas(app, tamanho.width) });
        if app.busca.aberta {
            stack![paginas, container(app.barra_busca()).width(Length::FILL).align_x(Align::End).padding([10u16, 24])].into()
        } else {
            paginas.into()
        }
    };
    let apresentando = app.apresentacao.is_some();
    container(conteudo).width(Length::FILL).height(Length::FILL).clip(true)
        .style(move |_| if apresentando { Style::default().background(Color::BLACK) }
            else { Style::default().background(COR_MESA()).border(Border::rounded(RAIO)) }).into()
}

/// As páginas da mesa, no arranjo de `App::posicoes` (vão em volta, linhas centradas).
fn paginas(app: &App, largura_visivel: f32) -> Element<'_, Msg> {
    let Some(aberto) = &app.aberto else { return Space::new(Length::FILL, Length::FILL).into() };
    let mut mais_larga: f32 = 0.0;
    let mut coluna = column![].spacing(VAO_PAGINAS).align(Align::Center).padding(VAO_PAGINAS);
    // As camadas (links, medidas, busca, campos) só nas páginas perto da tela: montar todas a cada
    // quadro percorria a árvore de páginas duas vezes por página (500 páginas: ~1000 voltas por quadro).
    let perto: std::collections::HashSet<u32> = app.perto_da_tela().into_iter().map(|(p, _)| p).collect();
    for (numeros, _) in app.linhas() {
        let mut linha = row![].spacing(VAO_PAGINAS).align(Align::Start);
        let mut largura_linha = VAO_PAGINAS * (numeros.len() as f32 - 1.0);
        for n in numeros {
            let (l, a) = aberto.tamanhos[n as usize - 1];
            let (l, a) = ((l * app.zoom).round(), (a * app.zoom).round());
            largura_linha += l;
            let conteudo: Element<'_, Msg> = match app.desenhadas.get(&n) {
                Some(d) => {
                    // Desenho antigo (outro zoom) fica esticado até o novo chegar.
                    let f = app.zoom / d.escala;
                    let img = image(d.imagem.clone()).width(d.largura as f32 * f).height(d.altura as f32 * f).content_fit(ContentFit::Fill);
                    match d.trecho {
                        None => img.into(),
                        Some([x, y, ..]) => {
                            let trecho = column![Space::new(0.0, y as f32 * f), row![Space::new(x as f32 * f, 0.0), img]];
                            match app.fundos.get(&n) {
                                Some(fundo) => stack![image(fundo.clone()).width(l).height(a).content_fit(ContentFit::Fill), trecho].into(),
                                None => trecho.into(),
                            }
                        }
                    }
                }
                None => match app.fundos.get(&n) {
                    Some(fundo) => image(fundo.clone()).width(l).height(a).content_fit(ContentFit::Fill).into(),
                    None => Space::new(l, a).into(),
                },
            };
            let folha = container(conteudo).width(l).height(a).clip(true).style(|_| Style::default().background(Color::WHITE)
                .border(Border::default().color(COR_BORDA_SUAVE(), 1.0))
                .shadow(Shadow { color: Color { a: 0.12, ..Color::BLACK }, offset: Point::new(0.0, 1.0), blur: 6.0 }));
            let pagina = if perto.contains(&n) { app.sobre_pagina(n, folha.into(), l, a) } else { folha.into() };
            linha = linha.push(mouse_area(pagina)
                .on_right_press(Msg::Ctx(Ct::Abrir(Alvo::Pagina(n)))));
        }
        mais_larga = mais_larga.max(largura_linha);
        coluna = coluna.push(linha);
    }
    let largura = (mais_larga + 2.0 * VAO_PAGINAS).max(largura_visivel);
    // Ctrl+roda é zoom: a área pega a roda antes do scrollable, senão ele também rola e a tela treme.
    let mut dentro = mouse_area(coluna.width(largura));
    if app.modificadores.control_key() { dentro = dentro.on_scroll(|_, dy, _| Msg::Roda(dy.signum())); }
    let mut rolagem = scrollable(dentro).id(ID_MESA).direction(Direction::Both).on_scroll(Msg::Rolou)
        .width(Length::FILL).height(Length::FILL);
    // O padrão (cor do texto translúcida) fica claro demais sobre a mesa escura.
    if escuro() { rolagem = rolagem.scrollbar_color(Color::rgb8(0x45, 0x48, 0x4e)); }
    let area = mouse_area(rolagem);
    if app.ferramenta == Ferramenta::Mao {
        area.on_press(Msg::MaoApertou).on_move(Msg::MaoMoveu).on_release(Msg::MaoSoltou).cursor_icon(CursorIcon::Grab).into()
    } else {
        area.into()
    }
}

fn trilho(app: &App) -> Element<'_, Msg> {
    let item = |nome: &str, rotulo: &'static str, p: Painel| {
        let ativo = app.painel == Some(p);
        botao_icone(desenho(nome, ativo.then_some(VERMELHO), 20.), rotulo, Some(Msg::AlternarPainel(p)), ativo)
    };
    let mut coluna = column![
        item("message-circle", "Comentários", Painel::Comentarios),
        item("layout-grid", "Miniaturas das páginas", Painel::Miniaturas),
        item("list-details", "Marcadores", Painel::Marcadores),
        item("link", "Links da página", Painel::Links),
        item("shapes", "Camadas", Painel::Camadas),
        item("paperclip", "Anexos", Painel::Anexos),
    ].spacing(22).align(Align::Center);
    if !app.ferramentas_abertas {
        coluna = coluna.push(botao_icone(icone("layout-sidebar", 20.), "Ferramentas", Some(Msg::AlternarFerramentas), false));
    }
    container(coluna).padding([20u16, 9]).height(Length::FILL).style(cartao).into()
}

/// Nome do papel pelo tamanho em milímetros (em pé ou deitado).
pub fn papel(l: f32, a: f32) -> String {
    let (l, a) = (l * 25.4 / 72.0, a * 25.4 / 72.0);
    let (menor, maior) = (l.min(a), l.max(a));
    let nome = [("A3", 297.0, 420.0), ("A4", 210.0, 297.0), ("A5", 148.0, 210.0), ("Carta", 215.9, 279.4), ("Ofício", 215.9, 355.6)]
        .into_iter().find(|(_, x, y)| (menor - x).abs() < 2.0 && (maior - y).abs() < 2.0).map(|(n, ..)| n);
    let medida = format!("{:.0} × {:.0} mm", l, a);
    nome.map_or(medida.clone(), |n| format!("{n}  •  {medida}"))
}

fn rodape(app: &App) -> Element<'_, Msg> {
    let atual = app.pagina;
    let total = app.total_paginas();
    let (l, a) = app.aberto.as_ref().and_then(|ab| ab.tamanhos.get(atual as usize - 1)).copied().unwrap_or((595.0, 842.0));
    let nav = |nome: &str, dica_: &str, msg: Msg, pode: bool| botao_icone(icone(nome, 18.), dica_, pode.then_some(msg), false);
    let pagina = app.campo_pagina.clone().unwrap_or_else(|| atual.to_string());
    let campo = container(text_input("", pagina).on_input(Msg::CampoPagina).on_submit(Msg::ConfirmarPagina)
        .size(13).padding([4u16, 8]).width(40.0).style(|_, _| Style::default().text_color(COR_TEXTO()))).style(caixa);
    let modo = |nome: &str, rotulo: &str, d: Disposicao| {
        let ativo = app.disposicao == d;
        button(icone(nome, 18.)).padding([5u16, 8])
            .style(move |_, s| {
                let st = Style::default().border(Border::rounded(6.0));
                if ativo || s == Status::Hovered { st.background(COR_HOVER()) } else { st }
            })
            .on_press(Msg::Disposicao(d)).into_dica(rotulo)
    };
    let zoom = (app.zoom * 100.0).round() as u32;
    container(row![
        nav("chevron-left-pipe", "Primeira página", Msg::IrPagina(1), atual > 1),
        nav("chevron-left", "Página anterior (PageUp)", Msg::Anterior, atual > 1),
        campo,
        text(match app.aberto.as_ref().and_then(|ab| ab.pdf.rotulo_da_pagina(atual)) {
            Some(r) => format!("/ {total}  ({r})"),
            None => format!("/ {total}"),
        }).size(13).color(COR_TEXTO_MUTED()).no_wrap(),
        nav("chevron-right", "Próxima página (PageDown)", Msg::Proxima, atual < total),
        nav("chevron-right-pipe", "Última página", Msg::IrPagina(total), atual < total),
        container(divisoria(20.0)).padding([0u16, 10]),
        text(papel(l, a)).size(13).color(COR_TEXTO_MUTED()).no_wrap(),
        container(text(app.status.as_str()).size(12).color(COR_TEXTO_MUTED()).ellipsis()).width(Length::FILL).padding([0u16, 16]),
        modo("layout-sidebar", "Uma página", Disposicao::UmaPagina),
        modo("book-2", "Duas páginas", Disposicao::DuasPaginas),
        Space::new(10.0, 0.0),
        pick_list(&NIVEIS[..], Some(NivelZoom(zoom)), |n: NivelZoom| Msg::Zoom(n.0 as f32 / 100.0)).size(13).width(82.0),
        Space::new(8.0, 0.0),
        slider(ZOOM_MIN..=ZOOM_MAX, app.zoom, Msg::Zoom).width(140.0),
        Space::new(8.0, 0.0),
        botao_icone(icone("arrows-horizontal", 18.), "Ajustar à largura", Some(Msg::AjustarLargura), false),
        botao_icone(icone("maximize", 18.), "Tela cheia (F11)", Some(Msg::TelaCheia), false),
    ].spacing(4).align(Align::Center)).padding([6u16, 12]).width(Length::FILL).style(cartao).into()
}

trait ComDica<'a> { fn into_dica(self, texto: &str) -> Element<'a, Msg>; }
impl<'a> ComDica<'a> for Button<'a, Msg> {
    fn into_dica(self, texto: &str) -> Element<'a, Msg> {
        Element::from(tooltip(self, texto.to_owned(), TooltipPosition::Top).padding([4, 8]).style(estilo_dica)).access_label(texto.to_owned())
    }
}

/// Os comandos do menu Arquivo: (ícone, rótulo, atalho, mensagem).
fn itens_arquivo() -> [(&'static str, &'static str, &'static str, Msg); 9] {
    [
        ("file-plus", "Novo PDF A4", "Ctrl+N", Msg::NovoA4),
        ("folder-open", "Abrir…", "Ctrl+O", Msg::Abrir),
        ("device-floppy", "Salvar", "Ctrl+S", Msg::Salvar),
        ("device-floppy", "Salvar como…", "Ctrl+Shift+S", Msg::SalvarComo),
        ("printer", "Imprimir…", "Ctrl+P", Msg::Imp(crate::imprimir::Im::Abrir)),
        ("files", "Juntar outro PDF…", "", Msg::Pag(Pag::Juntar)),
        ("file-export", "Exportar…", "", Msg::Exp(crate::exportar::Ex::Abrir(crate::exportar::Formato::Word))),
        ("info-circle", "Propriedades do documento…", "Ctrl+D", Msg::Prop(crate::propriedades::Pp::Abrir)),
        ("x", "Fechar documento", "Ctrl+W", Msg::IrInicio),
    ]
}

/// O que bate com a pesquisa de comandos: menu Arquivo, ferramentas e abas da faixa.
fn comandos_achados(app: &App) -> Vec<(&'static str, String, Msg)> {
    let procura = app.busca_comandos.trim().to_lowercase();
    if procura.is_empty() { return vec![]; }
    itens_arquivo().into_iter().map(|(i, r, _, m)| (i, r.to_string(), m))
        .chain(itens_ferramentas(app).into_iter().map(|(_, i, _, r, m, _)| (i, r.to_string(), m)))
        .chain(ABAS.iter().map(|(a, r)| ("layout-columns", format!("Aba {r}"), Msg::MudarAba(*a))))
        .filter(|(_, r, _)| r.to_lowercase().contains(&procura)).take(8).collect()
}

/// A lista embaixo da pesquisa de comandos; clicar fora limpa a pesquisa.
fn resultados_comandos(app: &App) -> Element<'_, Msg> {
    let achados = comandos_achados(app);
    let mut lista = column![].spacing(2);
    if achados.is_empty() { lista = lista.push(container(text("Nenhum comando encontrado").size(12.5).color(COR_TEXTO_MUTED())).padding([8u16, 10])); }
    for (i, r, m) in achados {
        lista = lista.push(button(row![icone(i, 17.), text(r).size(13).ellipsis(), horizontal_space()].spacing(12).align(Align::Center).width(Length::FILL))
            .width(Length::FILL).padding([7u16, 10]).style(estilo_item_menu(false, true)).on_press(Msg::Comando(Box::new(m))));
    }
    let caixa = container(lista).padding(8u16).width(300.0)
        .style(|_| Style::default().background(COR_PAINEL()).border(Border::rounded(RAIO).color(COR_BORDA_SUAVE(), 1.0)).shadow(Shadow { color: COR_SOMBRA(), offset: Point::new(0.0, 6.0), blur: 18.0 }));
    stack![
        mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::BuscaComandos(String::new())),
        // shortcut: a distância da direita é a dos 7 botões depois da pesquisa; muda se a barra mudar.
        container(opaque(caixa)).width(Length::FILL).align_x(Align::End).padding(Padding { top: 90.0, right: 287.0, ..Padding::ZERO }),
    ].into()
}

/// O menu Arquivo: abrir, salvar, imprimir e os recentes.
fn menu_arquivo(app: &App) -> Element<'_, Msg> {
    let item = |nome: &str, rotulo: String, atalho: &'static str, msg: Msg| button(row![icone(nome, 17.), text(rotulo).size(13).ellipsis(), horizontal_space(),
            text(atalho).size(11.5).color(COR_TEXTO_MUTED())].spacing(12).align(Align::Center).width(Length::FILL))
        .width(Length::FILL).padding([8u16, 10]).style(estilo_item_menu(false, true)).on_press(msg);
    let mut itens = column![
        text("FeatherPDF").size(18).font(Font::SEMIBOLD),
        text("Arquivos e ferramentas").size(12).color(COR_TEXTO_MUTED()),
        Space::new(0.0, 6.0),
    ].spacing(4);
    for (nome, rotulo, atalho, msg) in itens_arquivo() {
        itens = itens.push(item(nome, rotulo.into(), atalho, msg));
    }
    if !app.recentes.itens.is_empty() {
        itens = itens.push(container(text("RECENTES").size(11.5).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED())).padding(Padding { top: 10.0, bottom: 4.0, left: 10.0, right: 0.0 }));
        for r in app.recentes.itens.iter().take(6) {
            let nome = r.caminho.file_name().map_or_else(|| r.caminho.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
            itens = itens.push(item("file", nome, "", Msg::AbrirCaminho(r.caminho.clone())));
        }
    }
    let menu = container(itens).padding(14u16).width(320.0).style(|_| Style::default().background(COR_PAINEL()).border(Border::rounded(RAIO).color(COR_BORDA_SUAVE(), 1.0)));
    stack![
        mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::Menu),
        container(opaque(container(menu).style(|_| Style::default().border(Border::rounded(RAIO)).shadow(Shadow { color: COR_SOMBRA(), offset: Point::new(0.0, 6.0), blur: 18.0 }))))
            .padding(Padding { top: 92.0, left: 14.0, ..Padding::ZERO }),
    ].into()
}

/// Bordas finas para redimensionar a janela sem moldura (a principal e as auxiliares).
pub(crate) fn alcas<'a>(puxar: impl Fn(ResizeDirection) -> Msg) -> Element<'a, Msg> {
    let alca = |w: Length, h: Length, d: ResizeDirection, i: CursorIcon| mouse_area(Space::new(w, h)).on_press(puxar(d)).cursor_icon(i);
    stack![
        container(alca(Length::Fixed(5.0), Length::FILL, ResizeDirection::East, CursorIcon::EResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End),
        container(alca(Length::Fixed(5.0), Length::FILL, ResizeDirection::West, CursorIcon::WResize)).width(Length::FILL).height(Length::FILL),
        container(alca(Length::FILL, Length::Fixed(5.0), ResizeDirection::South, CursorIcon::SResize)).width(Length::FILL).height(Length::FILL).align_y(Align::End),
        container(alca(Length::FILL, Length::Fixed(4.0), ResizeDirection::North, CursorIcon::NResize)).width(Length::FILL).height(Length::FILL),
        container(alca(Length::Fixed(14.0), Length::Fixed(14.0), ResizeDirection::SouthEast, CursorIcon::SeResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End),
    ].into()
}

#[cfg(test)]
mod testes {
    use super::{papel, comandos_achados, App, Msg};

    /// Nome de ícone errado desenha o quadrado vazio sem avisar: confere os da faixa e dos painéis.
    #[test]
    fn icones_existem() {
        let fonte = include_str!("tela.rs");
        let chamadas = ["grande(", "pequeno(", "icone(", "icone_traco(", "desenho(", "botao_alinhar(", "suspenso(app, ", "regiao(", "ferramenta(",
            "nav(", "modo(", "fino(", "botao(", "comentar(", "b(", "tipografia(", "item("];
        let mut faltam = Vec::new();
        for c in chamadas {
            for pedaco in fonte.split(c).skip(1) {
                let Some(resto) = pedaco.strip_prefix('"') else { continue };
                let nome = &resto[..resto.find('"').unwrap()];
                if nome.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_') && !feather_ui::icones::existe(nome) {
                    faltam.push(nome);
                }
            }
        }
        assert!(faltam.is_empty(), "ícones que não existem: {faltam:?}");
    }

    #[test]
    fn nome_do_papel() {
        assert_eq!(papel(595.0, 842.0), "A4  •  210 × 297 mm");
        assert_eq!(papel(842.0, 595.0), "A4  •  297 × 210 mm");
        assert_eq!(papel(612.0, 792.0), "Carta  •  216 × 279 mm");
        assert_eq!(papel(500.0, 500.0), "176 × 176 mm");
    }

    #[test]
    fn pesquisa_de_comandos_acha_menu_ferramentas_e_abas() {
        let (mut app, _) = App::novo();
        app.busca_comandos = "ocr".into();
        assert!(comandos_achados(&app).iter().any(|(_, r, _)| r == "OCR"));
        app.busca_comandos = "salvar".into();
        assert_eq!(comandos_achados(&app).len(), 2);
        app.busca_comandos = "exibir".into();
        assert!(comandos_achados(&app).iter().any(|(_, r, m)| r == "Aba Exibir" && matches!(m, Msg::MudarAba(_))));
        app.busca_comandos = "  ".into();
        assert!(comandos_achados(&app).is_empty());
    }
}
