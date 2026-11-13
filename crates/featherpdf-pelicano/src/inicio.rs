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

/// Ferramenta da faixa: ícone na cor dele, título e uma linha do que faz; sem moldura até o mouse passar.
fn acao<'a>(icone: &str, cor: Color, titulo: &'a str, detalhe: &'a str, msg: Option<Msg>) -> Element<'a, Msg> {
    button(row![
        icones::icone_traco(icone, 20.).color(texto_escuro(cor)),
        column![text(titulo).size(13), text(detalhe).size(11.5).color(tema::COR_TEXTO_MUTED()).no_wrap().ellipsis()].spacing(1),
        Space::with_width(Length::FILL),
    ].spacing(12).align_y(Align::Center))
        .width(Length::FILL).padding([10, 12])
        .style(move |_, status| Style {
            background: matches!(status, Status::Hovered | Status::Pressed).then(|| Background::Color(tema::COR_HOVER())),
            text_color: Some(tema::COR_TEXTO()),
            border: Border { radius: [8.0; 4], ..Default::default() },
            ..Default::default()
        })
        .on_press_maybe(msg).into()
}

impl App {
    pub fn view_inicio(&self) -> Element<'_, Msg> {
        // A mesma barra das abas dos documentos: a pena, a aba "Início", o + e a engrenagem.
        let topo = crate::tela::barra_documento(self);
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;

        // Cabeçalho como o do FeatherDocs: marca, frase e Abrir à direita (soltar arquivo vale na janela toda).
        let cabecalho = row![
            feather_ui::marca::icone_atual(36.),
            column![
                text("FeatherPDF").size(20).font(negrito),
                text("PDFs que ficam na sua máquina. Solte um arquivo em qualquer lugar da janela para abrir.").size(12.5).color(muted),
            ].spacing(1).width(Length::FILL),
            button(row![icones::icone_traco("file-plus", 16.), text("PDF em branco").size(13)].spacing(7).align_y(Align::Center))
                .padding([8, 14]).style(tema::estilo_botao_secundario).on_press(Msg::NovoA4),
            button(row![icones::icone_traco("folder-open", 16.), text("Abrir…").size(13)].spacing(7).align_y(Align::Center))
                .padding([8, 16]).style(tema::estilo_botao_secundario).on_press(Msg::Abrir),
        ].spacing(12).align_y(Align::Center);

        // As ferramentas que precisam de um PDF pedem o arquivo e abrem nele (Msg::AbrirPara).
        let para = |m: Msg| Some(Msg::AbrirPara(Box::new(m)));
        let acoes: [(&str, Color, &str, &str, Option<Msg>); 13] = [
            ("copy-plus", VERMELHO, "Juntar arquivos", "PDFs e imagens", Some(Msg::Juntar(crate::juntar::Jt::Escolher(false)))),
            ("cut", Color::rgb8(0xc2, 0x41, 0x5d), "Dividir", "Um PDF a cada N páginas", para(crate::tela::dividir())),
            ("photo", Color::rgb8(0x23, 0x6a, 0xc7), "Imagens para PDF", "Uma por página", Some(Msg::Juntar(crate::juntar::Jt::Escolher(true)))),
            ("file-export", Color::rgb8(0x2f, 0x7f, 0xb8), "Converter", "Word, Excel, imagens", para(Msg::Exp(crate::exportar::Ex::Abrir(crate::exportar::Formato::Word)))),
            ("scan", Color::rgb8(0x1f, 0x8a, 0x5b), "OCR", "Scan vira texto pesquisável", para(Msg::Ocr(crate::ocr::Oc::Abrir))),
            ("file-zip", Color::rgb8(0x7a, 0x5c, 0xc4), "Comprimir", "Arquivo menor para enviar", para(Msg::Prot(crate::proteger::Pr::AbrirComprimir))),
            ("writing-sign", Color::rgb8(0x3d, 0x6f, 0x8f), "Assinar", "Com certificado digital", para(Msg::Assin(crate::assinar::As::Abrir))),
            ("lock", Color::rgb8(0xb8, 0x86, 0x0b), "Senha", "Proteger e permissões", para(Msg::Prot(crate::proteger::Pr::AbrirSenha))),
            ("list", Color::rgb8(0xb0, 0x7d, 0x0a), "Fluxos em lote", "Pasta inteira", Some(Msg::Fluxos(crate::fluxos::Fx::Abrir))),
            ("eraser", Color::rgb8(0x8a, 0x92, 0x9c), "Tarja definitiva", "Apagar CPF e nomes", para(Msg::MudarAba(crate::Aba::Proteger))),
            ("columns", Color::rgb8(0xc0, 0x60, 0x14), "Comparar", "Diferença entre versões", para(Msg::Comparar(crate::comparar::Cmp::Abrir))),
            ("forms", Color::rgb8(0x1a, 0x7f, 0x86), "Formulário", "Preencher ou criar campos", para(Msg::MudarAba(crate::Aba::Formularios))),
            ("file-export", Color::rgb8(0x5b, 0x6b, 0x7c), "Extrair páginas", "Escolha e salve à parte", para(Msg::MudarAba(crate::Aba::Paginas))),
        ];
        let mut grade = column![].spacing(2);
        let mut linha = row![].spacing(6);
        let mostrar = if self.inicio_mais { acoes.len() } else { 8 };
        for (k, (icone, cor, titulo, detalhe, msg)) in acoes.into_iter().take(mostrar).enumerate() {
            linha = linha.push(container(acao(icone, cor, titulo, detalhe, msg)).width(Length::Fill(1)));
            if k % 4 == 3 { grade = grade.push(linha); linha = row![].spacing(6); }
        }
        // A última fileira incompleta fica com a largura das outras.
        let sobra = mostrar % 4;
        if sobra > 0 {
            for _ in sobra..4 { linha = linha.push(Space::with_width(Length::Fill(1))); }
            grade = grade.push(linha);
        }
        let ferramentas = container(column![
            row![
                text("Ferramentas").size(16).font(negrito),
                Space::with_width(Length::FILL),
                text("Pedem o PDF e abrem nele").size(12).color(muted),
                button(row![text(if self.inicio_mais { "Ver menos" } else { "Ver todas" }).size(12.5), icones::icone_traco(if self.inicio_mais { "chevron-up" } else { "chevron-right" }, 14.).color(tema::COR_AZUL_ACENTO())].spacing(3).align_y(Align::Center))
                    .style(|_, _| Style { text_color: Some(tema::COR_AZUL_ACENTO()), ..Default::default() }).padding([4, 8]).on_press(Msg::InicioMais),
            ].spacing(12).align_y(Align::Center),
            grade,
        ].spacing(10))
            .padding([14, 14]).width(Length::FILL)
            .style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL_FAIXA())),
                border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [10.0; 4] },
                ..Default::default()
            });

        let titulo_recentes = row![
            text("Recentes").size(16).font(negrito),
            Space::with_width(Length::FILL),
            container(row![
                icones::icone_traco("search", 15.).color(muted),
                text_input("Buscar nos recentes", &self.busca_recentes).on_input(Msg::BuscaRecentes).size(13).padding([5, 4])
                    .style(|tema_, estado| { let mut e = pelicano::input_default(tema_, estado); e.background = Some(Background::Color(tema::COR_PAINEL())); e.border = Border::default(); e }),
            ].spacing(6).align_y(Align::Center))
                .padding([1, 10]).width(280)
                .style(|_| Style {
                    background: Some(Background::Color(tema::COR_PAINEL())),
                    border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [8.0; 4] },
                    ..Default::default()
                }),
        ].align_y(Align::Center);

        let corpo = column![
            cabecalho,
            ferramentas,
            column![titulo_recentes, self.tabela_recentes()].spacing(10),
            self.status_inicio(),
        ].spacing(18);

        column![
            topo,
            scrollable(container(container(corpo).max_width(1180.0)).width(Length::FILL).align_x(Align::Center).padding([24, 44])).height(Length::FILL),
        ].into()
    }

    fn status_inicio(&self) -> Element<'_, Msg> {
        if self.status.is_empty() { return Space::with_height(0).into(); }
        text(self.status.clone()).size(12).color(tema::COR_TEXTO_MUTED()).into()
    }

    /// Recentes em tabela sem moldura, como no FeatherDocs: etiqueta PDF, nome, pasta, páginas e quando.
    fn tabela_recentes(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let rotulo = |s: &'static str| text(s).size(11).color(muted);
        let mut linhas = column![
            container(row![
                Space::with_width(Length::Fixed(44.0)),
                container(rotulo("Nome")).width(Length::Fill(5)),
                container(rotulo("Pasta")).width(Length::Fill(4)),
                container(rotulo("Páginas")).width(Length::Fixed(70.0)).align_x(Align::End),
                container(rotulo("Aberto")).width(Length::Fixed(100.0)).align_x(Align::End),
            ].spacing(10)).padding([0, 10]),
            linha_horizontal(),
        ].spacing(4);
        let busca = self.busca_recentes.to_lowercase();
        let agora = recentes::agora();
        let mut quantos = 0;
        for r in self.recentes.itens.iter().filter(|r| busca.is_empty()
            || r.caminho.to_string_lossy().to_lowercase().contains(&busca))
        {
            quantos += 1;
            let nome = r.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let pasta = cortar(&r.caminho.parent().map(encurtar_pasta).unwrap_or_default(), 50, true);
            let etiqueta = container(text("PDF").size(9.5).font(Font::SEMIBOLD).color(Color::WHITE)).padding([1, 5])
                .style(|_| Style { background: Some(Background::Color(Color { a: if tema::escuro() { 0.85 } else { 1.0 }, ..VERMELHO })), border: Border { radius: [3.0; 4], ..Default::default() }, ..Default::default() });
            let linha = row![
                container(etiqueta).width(Length::Fixed(44.0)).align_x(Align::Center),
                container(text(nome).size(13).no_wrap().ellipsis()).width(Length::Fill(5)),
                container(text(pasta).size(12.5).color(muted).no_wrap().ellipsis()).width(Length::Fill(4)),
                container(text(r.paginas.to_string()).size(12.5).color(muted)).width(Length::Fixed(70.0)).align_x(Align::End),
                container(text(recentes::quando(r.aberto_em, agora)).size(12.5).color(muted)).width(Length::Fixed(100.0)).align_x(Align::End),
            ].spacing(10).align_y(Align::Center);
            linhas = linhas.push(
                button(linha).padding([8, 10]).width(Length::FILL)
                    .style(|_, status| Style {
                        background: matches!(status, Status::Hovered).then(|| Background::Color(tema::COR_HOVER())),
                        text_color: Some(tema::COR_TEXTO()),
                        border: Border { radius: [7.0; 4], ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Msg::AbrirCaminho(r.caminho.clone())));
        }
        if quantos == 0 {
            linhas = linhas.push(container(text(if self.recentes.itens.is_empty() {
                "Os PDFs que você abrir aparecem aqui."
            } else { "Nenhum recente com esse nome." }).size(13).color(muted)).padding([14, 10]));
        }
        linhas.into()
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
