//! Página inicial: começar (abrir, PDF em branco…), recentes e atalhos da suíte.
//! Desenho de referência: canvas "FeatherPDF — telas", quadro "Página inicial".

use feather_ui::{icones, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{recentes, App, Msg};

pub const VERMELHO: Color = Color { r: 0xc5 as f32 / 255.0, g: 0x35 as f32 / 255.0, b: 0x2b as f32 / 255.0, a: 1.0 };
/// Verde de "deu certo" (assinatura íntegra, impressão, fluxo concluído).
pub const VERDE: Color = Color { r: 0x28 as f32 / 255.0, g: 0x78 as f32 / 255.0, b: 0x46 as f32 / 255.0, a: 1.0 };

/// Texto vermelho: no escuro, um vermelho claro (o escuro some sobre o fundo vermelho translúcido).
pub fn vermelho_texto() -> Color {
    if tema::escuro() { Color::rgb8(0xff, 0x8a, 0x80) } else { VERMELHO }
}

/// Fundo claro fixo (etiqueta, cartão de situação) no tema escuro: a mesma cor translúcida,
/// em vez de um bloco claro no meio da janela escura.
pub fn fundo_claro(c: Color) -> Color { if tema::escuro() { Color { a: 0.12, ..c } } else { c } }

/// Texto escuro fixo (verde, vermelho, âmbar das situações) no tema escuro: a mesma cor, clareada.
pub fn texto_escuro(c: Color) -> Color {
    if !tema::escuro() { return c; }
    let claro = |x: f32| x + (1.0 - x) * 0.55;
    Color { r: claro(c.r), g: claro(c.g), b: claro(c.b), a: c.a }
}

/// Fundo rosado dos itens ativos (no escuro, o vermelho transparente).
pub fn fundo_vermelho() -> Color {
    if tema::escuro() { Color { a: 0.22, ..VERMELHO } } else { Color::rgb8(0xf9, 0xeb, 0xea) }
}

pub fn linha_vertical<'a>() -> Element<'a, Msg> {
    container(Space::with_width(1)).height(Length::FILL).style(tema::estilo_linha).into()
}

pub fn linha_horizontal<'a>() -> Element<'a, Msg> {
    container(Space::with_height(1)).width(Length::FILL).style(tema::estilo_linha).into()
}

fn item_lateral<'a>(icone: &str, rotulo: &'a str, ativo: bool, msg: Option<Msg>) -> Element<'a, Msg> {
    // Ativo: fundo neutro e só o ícone na cor da marca, como no FeatherDocs (sem bloco vermelho).
    let cor_icone = if ativo { vermelho_texto() } else { tema::COR_TEXTO_MUTED() };
    button(row![icones::icone_traco(icone, 17.).color(cor_icone), text(rotulo).size(13).font(if ativo { Font::SEMIBOLD } else { Font::DEFAULT }), Space::with_width(Length::FILL)]
        .spacing(11).align_y(Align::Center))
        .width(Length::FILL).padding([8, 12])
        .style(move |_, status| Style {
            background: (ativo || matches!(status, Status::Hovered)).then(|| Background::Color(tema::COR_HOVER())),
            text_color: Some(tema::COR_TEXTO()),
            border: Border { radius: [8.0; 4], ..Default::default() },
            ..Default::default()
        })
        .on_press_maybe(msg).into()
}

/// Ação rápida: ícone em quadrado translúcido da cor dele, título e uma linha do que faz.
fn acao<'a>(icone: &str, cor: Color, titulo: &'a str, detalhe: &'a str, msg: Option<Msg>) -> Element<'a, Msg> {
    let quadro = container(icones::icone_traco(icone, 18.).color(texto_escuro(cor))).width(34).height(34).align_x(Align::Center).align_y(Align::Center)
        .style(move |_| Style { background: Some(Background::Color(Color { a: if tema::escuro() { 0.16 } else { 0.12 }, ..cor })), border: Border { radius: [9.0; 4], ..Default::default() }, ..Default::default() });
    button(container(row![quadro, column![text(titulo).size(13.5), text(detalhe).size(12).color(tema::COR_TEXTO_MUTED()).no_wrap().ellipsis()].spacing(2)]
        .spacing(12).align_y(Align::Center)).width(Length::FILL))
        .width(Length::FILL).padding(14)
        .style(move |_, status| Style {
            background: Some(Background::Color(tema::COR_PAINEL())),
            text_color: Some(tema::COR_TEXTO()),
            border: Border { color: if matches!(status, Status::Hovered | Status::Pressed) { tema::COR_TEXTO_MUTED() } else { tema::COR_BORDA() }, width: 1.0, radius: [10.0; 4] },
            ..Default::default()
        })
        .on_press_maybe(msg).into()
}

/// "Bom dia", "Boa tarde" ou "Boa noite" pela hora local.
fn saudacao() -> &'static str {
    match hora_local() { 5..=11 => "Bom dia", 12..=17 => "Boa tarde", _ => "Boa noite" }
}

#[cfg(unix)]
fn hora_local() -> i32 {
    #[repr(C)]
    struct Tm { seg: i32, min: i32, hora: i32, dia: i32, mes: i32, ano: i32, sem: i32, ano_dia: i32, verao: i32, gmtoff: i64, zona: *const i8 }
    extern "C" { fn localtime_r(t: *const i64, saida: *mut Tm) -> *mut Tm; }
    let t = recentes::agora() as i64;
    let mut tm = Tm { seg: 0, min: 0, hora: 12, dia: 0, mes: 0, ano: 0, sem: 0, ano_dia: 0, verao: 0, gmtoff: 0, zona: std::ptr::null() };
    // SAFETY: localtime_r só lê `t` e escreve em `tm`, os dois válidos aqui.
    unsafe { localtime_r(&t, &mut tm); }
    tm.hora
}
#[cfg(not(unix))]
fn hora_local() -> i32 { 12 }

impl App {
    pub fn view_inicio(&self) -> Element<'_, Msg> {
        // A mesma barra das abas dos documentos: a pena, a aba "Início", o + e a engrenagem.
        let topo = crate::tela::barra_documento(self);
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;

        let agora = recentes::agora();
        let hoje = self.recentes.itens.iter().filter(|r| recentes::quando(r.aberto_em, agora) == "hoje").count();
        let n = self.recentes.itens.len();
        let resumo = match (n, hoje) {
            (0, _) => "Nenhum PDF aberto ainda".to_string(),
            (n, 0) => format!("{n} PDF{} recente{}", if n == 1 { "" } else { "s" }, if n == 1 { "" } else { "s" }),
            (n, h) => format!("{n} PDF{} recente{} · {h} aberto{} hoje", if n == 1 { "" } else { "s" }, if n == 1 { "" } else { "s" }, if h == 1 { "" } else { "s" }),
        };
        let saudacao = row![
            text(saudacao()).size(24).font(negrito),
            Space::with_width(Length::FILL),
            text(resumo).size(13).color(muted),
        ].align_y(Align::Center);

        let soltar = button(row![
            container(icones::icone_traco("folder-open", 24.).color(vermelho_texto())).width(48).height(48).align_x(Align::Center).align_y(Align::Center)
                .style(|_| Style { background: Some(Background::Color(fundo_vermelho())), border: Border { radius: [12.0; 4], ..Default::default() }, ..Default::default() }),
            column![
                text("Solte um PDF aqui ou clique para abrir").size(16).font(negrito),
                text("Imagens (PNG, JPG) viram PDF de uma página. Vários arquivos abrem em abas.").size(13).color(muted),
            ].spacing(4).width(Length::FILL),
            container(text("Ctrl+O").size(12).font(Font::MONO).color(muted)).padding([3, 8])
                .style(|_| Style { border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [6.0; 4] }, ..Default::default() }),
        ].spacing(18).align_y(Align::Center))
            .width(Length::FILL).padding([22, 24])
            .style(|_, status| {
                let sobre = matches!(status, Status::Hovered | Status::Pressed);
                Style {
                background: Some(Background::Color(if sobre { tema::COR_HOVER() } else { tema::COR_PAINEL() })),
                text_color: Some(tema::COR_TEXTO()),
                border: Border { color: if sobre { Color { a: 0.7, ..vermelho_texto() } } else { tema::COR_BORDA() }, width: 1.0, radius: [12.0; 4] },
                ..Default::default()
            }})
            .on_press(Msg::Abrir);

        // As ferramentas que precisam de um PDF pedem o arquivo e abrem nele (Msg::AbrirPara).
        let para = |m: Msg| Some(Msg::AbrirPara(Box::new(m)));
        let acoes: [(&str, Color, &str, &str, Option<Msg>); 14] = [
            ("file-plus", Color::rgb8(0x8e, 0x44, 0xad), "PDF em branco", "A4, uma página", Some(Msg::NovoA4)),
            ("copy-plus", VERMELHO, "Juntar arquivos", "PDFs e imagens", Some(Msg::Juntar(crate::juntar::Jt::Escolher(false)))),
            ("cut", Color::rgb8(0xc2, 0x41, 0x5d), "Dividir", "Um PDF a cada N páginas", para(crate::tela::dividir())),
            ("photo", Color::rgb8(0x23, 0x6a, 0xc7), "Imagens para PDF", "Uma por página", Some(Msg::Juntar(crate::juntar::Jt::Escolher(true)))),
            ("list", Color::rgb8(0xb0, 0x7d, 0x0a), "Fluxos em lote", "Pasta inteira", Some(Msg::Fluxos(crate::fluxos::Fx::Abrir))),
            ("scan", Color::rgb8(0x1f, 0x8a, 0x5b), "OCR", "Scan vira texto pesquisável", para(Msg::Ocr(crate::ocr::Oc::Abrir))),
            ("file-export", Color::rgb8(0x2f, 0x7f, 0xb8), "Converter", "Word, Excel, imagens", para(Msg::Exp(crate::exportar::Ex::Abrir(crate::exportar::Formato::Word)))),
            ("file-zip", Color::rgb8(0x7a, 0x5c, 0xc4), "Comprimir", "Arquivo menor para enviar", para(Msg::Prot(crate::proteger::Pr::AbrirComprimir))),
            ("writing-sign", Color::rgb8(0x3d, 0x6f, 0x8f), "Assinar", "Com certificado digital", para(Msg::Assin(crate::assinar::As::Abrir))),
            ("lock", Color::rgb8(0xb8, 0x86, 0x0b), "Senha", "Proteger e permissões", para(Msg::Prot(crate::proteger::Pr::AbrirSenha))),
            ("eraser", Color::rgb8(0x8a, 0x92, 0x9c), "Tarja definitiva", "Apagar CPF e nomes", para(Msg::MudarAba(crate::Aba::Proteger))),
            ("columns", Color::rgb8(0xc0, 0x60, 0x14), "Comparar", "Diferença entre versões", para(Msg::Comparar(crate::comparar::Cmp::Abrir))),
            ("forms", Color::rgb8(0x1a, 0x7f, 0x86), "Formulário", "Preencher ou criar campos", para(Msg::MudarAba(crate::Aba::Formularios))),
            ("file-export", Color::rgb8(0x5b, 0x6b, 0x7c), "Extrair páginas", "Escolha e salve à parte", para(Msg::MudarAba(crate::Aba::Paginas))),
        ];
        let mut grade = column![].spacing(10);
        let mut linha = row![].spacing(10);
        let mostrar = if self.inicio_mais { acoes.len() } else { 4 };
        for (k, (icone, cor, titulo, detalhe, msg)) in acoes.into_iter().take(mostrar).enumerate() {
            linha = linha.push(container(acao(icone, cor, titulo, detalhe, msg)).width(Length::Fill(1)));
            if k % 4 == 3 { grade = grade.push(linha); linha = row![].spacing(10); }
        }
        // A última fileira incompleta fica com a largura das outras.
        let sobra = mostrar % 4;
        if sobra > 0 {
            for _ in sobra..4 { linha = linha.push(Space::with_width(Length::Fill(1))); }
            grade = grade.push(linha);
        }

        let titulo_recentes = row![
            text("Recentes").size(16).font(negrito),
            Space::with_width(Length::FILL),
            text_input("Buscar nos recentes", &self.busca_recentes)
                .on_input(Msg::BuscaRecentes).width(260).padding([6, 10]).size(13),
        ].align_y(Align::Center);

        let corpo = column![
            saudacao,
            soltar,
            grade,
            row![
                text("As ferramentas que precisam de um PDF pedem o arquivo e abrem nele.").size(12).color(muted),
                Space::with_width(Length::FILL),
                button(text(if self.inicio_mais { "Ver menos" } else { "Ver mais ferramentas" }).size(13).color(vermelho_texto()))
                    .style(|_, _| Style::default()).padding([4, 8]).on_press(Msg::InicioMais),
            ].align_y(Align::Center),
            Space::with_height(6),
            titulo_recentes,
            self.tabela_recentes(),
            self.status_inicio(),
        ].spacing(16);

        let lateral = container(column![
            item_lateral("layout-sidebar", "Início", true, None),
            item_lateral("folder-open", "Abrir…", false, Some(Msg::Abrir)),
            item_lateral("file-plus", "PDF em branco", false, Some(Msg::NovoA4)),
            Space::with_height(Length::FILL),
            item_lateral("settings", "Configurações", false, Some(Msg::AbrirConfiguracoes)),
        ].spacing(3)).width(210).height(Length::FILL).padding([14, 10])
            .style(|_| Style { background: Some(Background::Color(tema::COR_PAINEL_FAIXA())), ..Default::default() });

        column![
            topo,
            row![
                lateral,
                linha_vertical(),
                scrollable(container(container(corpo).max_width(1000.0)).width(Length::FILL).align_x(Align::Center).padding([32, 28])).height(Length::FILL),
            ].height(Length::FILL),
        ].into()
    }

    fn status_inicio(&self) -> Element<'_, Msg> {
        if self.status.is_empty() { return Space::with_height(0).into(); }
        text(self.status.clone()).size(12).color(tema::COR_TEXTO_MUTED()).into()
    }

    fn tabela_recentes(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let busca = self.busca_recentes.to_lowercase();
        let agora = recentes::agora();
        let mut linhas = column![];
        let mut quantos = 0;
        for r in self.recentes.itens.iter().filter(|r| busca.is_empty()
            || r.caminho.to_string_lossy().to_lowercase().contains(&busca))
        {
            quantos += 1;
            let nome = cortar(&r.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), 60, false);
            let pasta = cortar(&r.caminho.parent().map(encurtar_pasta).unwrap_or_default(), 40, true);
            let folha = icones::icone_traco("file-type-pdf", 22.).color(vermelho_texto());
            let linha = row![
                container(row![folha, column![text(nome).size(13.5).no_wrap().ellipsis(), text(pasta).size(12).color(muted).no_wrap().ellipsis()].spacing(1)]
                    .spacing(12).align_y(Align::Center)).width(Length::FILL).padding([9, 16]),
                container(text(format!("{} pág.", r.paginas)).size(13).color(muted)).width(Length::Fixed(90.0)).padding([9, 8]).align_x(Align::End),
                container(text(recentes::quando(r.aberto_em, agora)).size(13).color(muted)).width(Length::Fixed(110.0)).padding([9, 16]).align_x(Align::End),
            ].align_y(Align::Center);
            if quantos > 1 { linhas = linhas.push(linha_horizontal()); }
            linhas = linhas.push(
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

        container(column![linhas, vazio])
            .width(Length::FILL)
            .style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL())),
                border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [12.0; 4] },
                ..Default::default()
            }).into()
    }
}

/// Uma linha só: nome longo perde o fim, pasta longa perde o começo (o fim é o que identifica).
fn cortar(texto: &str, max: usize, do_comeco: bool) -> String {
    let n = texto.chars().count();
    if n <= max { return texto.to_string(); }
    if do_comeco { format!("…{}", texto.chars().skip(n - max + 1).collect::<String>()) }
    else { format!("{}…", texto.chars().take(max - 1).collect::<String>()) }
}

/// "~/Documentos/Manutenção" no lugar de "/home/fulano/Documentos/Manutenção".
fn encurtar_pasta(pasta: &std::path::Path) -> String {
    encurtar_em(pasta, std::env::var_os("HOME").as_deref().map(std::path::Path::new))
}

/// Compara pedaço por pedaço do caminho: com HOME `/home/ana`, `/home/anabela` não vira `~bela`.
fn encurtar_em(pasta: &std::path::Path, home: Option<&std::path::Path>) -> String {
    match home.filter(|h| !h.as_os_str().is_empty()).and_then(|h| pasta.strip_prefix(h).ok()) {
        Some(resto) if resto.as_os_str().is_empty() => "~".into(),
        Some(resto) => format!("~{}{}", std::path::MAIN_SEPARATOR, resto.display()),
        None => pasta.display().to_string(),
    }
}

/// A base sozinha, mas com a mesma forma (Stack com a base primeiro) que os diálogos e menus
/// sobrepostos usam: trocar a forma da árvore zera o estado dos widgets e a rolagem da página volta ao topo.
pub fn sem_camada<'a>(base: Element<'a, Msg>) -> Element<'a, Msg> {
    Stack::new().push(base).width(Length::FILL).height(Length::FILL).into()
}

#[cfg(test)]
mod testes {
    #[test]
    fn cortar_mantem_uma_linha() {
        assert_eq!(super::cortar("curto.pdf", 60, false), "curto.pdf");
        assert_eq!(super::cortar("abcdef", 4, false), "abc…");
        assert_eq!(super::cortar("/a/b/c/d", 4, true), "…c/d");
    }

    #[test]
    fn encurtar_so_dentro_da_pasta_pessoal() {
        use std::path::Path;
        let home = Some(Path::new("/home/ana"));
        assert_eq!(super::encurtar_em(Path::new("/home/ana/Documentos"), home), "~/Documentos");
        assert_eq!(super::encurtar_em(Path::new("/home/ana"), home), "~");
        assert_eq!(super::encurtar_em(Path::new("/home/anabela/x"), home), "/home/anabela/x");
        assert_eq!(super::encurtar_em(Path::new("/tmp"), None), "/tmp");
    }
}
