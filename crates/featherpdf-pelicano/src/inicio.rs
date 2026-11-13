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

/// Quantos recentes ganham capa na página inicial (dois por linha de cartões a mais pesam pouco).
const CAPAS: usize = 12;

/// Página 1 do arquivo, com `altura` pixels, para a capa do recente. Rodar fora da janela.
pub fn capa(caminho: &std::path::Path, paginas: u32, altura: f32) -> Result<featherpdf_io::PaginaRaster, String> {
    let vis = match featherpdf_io::Visualizador::do_arquivo(caminho, paginas) {
        Some(v) => v,
        None => featherpdf_io::Visualizador::novo(std::fs::read(caminho).map_err(|e| e.to_string())?)?,
    };
    let (_, a) = vis.tamanhos().first().copied().ok_or("PDF sem páginas")?;
    vis.desenhar(1, (altura / a).clamp(0.1, 4.0))
}

/// Botão de cor cheia (vermelho da marca, texto branco).
fn estilo_principal(_: &Theme, status: Status) -> Style {
    let sobre = matches!(status, Status::Hovered | Status::Pressed);
    Style {
        background: Some(Background::Color(if sobre { Color::rgb8(0xa9, 0x2c, 0x23) } else { VERMELHO })),
        text_color: Some(Color::WHITE),
        border: Border { radius: [8.0; 4], ..Default::default() },
        ..Default::default()
    }
}

/// Cartão neutro: painel com borda suave; o mouse em cima clareia.
fn estilo_cartao(_: &Theme, status: Status) -> Style {
    let sobre = matches!(status, Status::Hovered | Status::Pressed);
    Style {
        background: Some(Background::Color(if sobre { tema::COR_HOVER() } else { tema::COR_PAINEL() })),
        text_color: Some(tema::COR_TEXTO()),
        border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [10.0; 4] },
        ..Default::default()
    }
}

fn tecla<'a>(t: &'a str) -> Element<'a, Msg> {
    container(text(t).size(11).font(Font::MONO).color(tema::COR_TEXTO_MUTED())).padding([2, 6])
        .style(|_| Style { border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [5.0; 4] }, ..Default::default() }).into()
}

/// A capa desenhada ou, enquanto não chega (ou o arquivo não abre), uma folha com o ícone.
fn folha<'a>(capa: Option<&Image>, l: f32, a: f32) -> Element<'a, Msg> {
    let dentro: Element<'a, Msg> = match capa {
        Some(img) => image(img.clone()).width(l).height(a).content_fit(ContentFit::Contain).into(),
        None => container(icones::icone_traco("file-type-pdf", 26.).color(tema::COR_TEXTO_MUTED())).width(l).height(a).align_x(Align::Center).align_y(Align::Center).into(),
    };
    let tem = capa.is_some();
    container(dentro).style(move |_| Style {
        background: Some(Background::Color(if tem { Color::TRANSPARENT } else { tema::COR_FUNDO_JANELA() })),
        border: Border { color: tema::COR_BORDA_SUAVE(), width: if tem { 0.0 } else { 1.0 }, radius: [3.0; 4] },
        ..Default::default()
    }).into()
}

impl App {
    /// Pede as capas dos primeiros recentes que ainda não têm (ou que foram abertos de novo).
    pub fn pedir_capas(&mut self) -> Task<Msg> {
        let vivos: Vec<(std::path::PathBuf, u64)> = self.recentes.itens.iter().take(CAPAS).map(|r| (r.caminho.clone(), r.aberto_em)).collect();
        self.capas.retain(|c, (q, _)| vivos.iter().any(|(v, w)| v == c && w == q));
        let altura = 150.0 * self.escala_tela;
        let mut tarefas = Vec::new();
        for r in self.recentes.itens.iter().take(CAPAS) {
            if !self.capas_pedidas.insert((r.caminho.clone(), r.aberto_em)) { continue; }
            let (caminho, paginas, quando) = (r.caminho.clone(), r.paginas, r.aberto_em);
            let destino = caminho.clone();
            tarefas.push(Task::perform(async move {
                let _vez = crate::DESENHOS.acquire().await;
                tokio::task::spawn_blocking(move || { crate::alocador::prioridade_baixa(); capa(&caminho, paginas, altura) }).await
                    .unwrap_or_else(|e| Err(e.to_string()))
            }, move |r| Msg::Capa(destino.clone(), quando, r)));
        }
        Task::batch(tarefas)
    }

    pub fn view_inicio(&self) -> Element<'_, Msg> {
        // A mesma barra das abas dos documentos: a pena, a aba "Início", o + e a engrenagem.
        let topo = crate::tela::barra_documento(self);
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;
        let agora = recentes::agora();
        let capa_de = |c: &std::path::Path| self.capas.get(c).and_then(|(_, i)| i.as_ref());

        // Continuar de onde parou: o recente mais novo, com a página onde a pessoa saiu.
        let destaque: Element<'_, Msg> = match self.recentes.itens.first() {
            Some(r) => {
                let nome = r.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let pasta = cortar(&r.caminho.parent().map(encurtar_pasta).unwrap_or_default(), 40, true);
                let pagina = r.pagina.clamp(1, r.paginas.max(1));
                let feito = if r.paginas > 1 { (pagina - 1) as f32 / (r.paginas - 1) as f32 } else { 1.0 };
                let barra = container(container(Space::new(Length::Fixed(320.0 * feito), 4.0))
                    .style(|_| Style { background: Some(Background::Color(VERMELHO)), border: Border { radius: [2.0; 4], ..Default::default() }, ..Default::default() }))
                    .width(320).height(4)
                    .style(|_| Style { background: Some(Background::Color(tema::COR_BORDA())), border: Border { radius: [2.0; 4], ..Default::default() }, ..Default::default() });
                let mut botoes = row![
                    button(text(if pagina > 1 { "Continuar lendo" } else { "Abrir" }).size(13).font(negrito)).padding([9, 16]).style(estilo_principal)
                        .on_press(Msg::AbrirCaminho(r.caminho.clone())),
                ].spacing(8);
                if pagina > 1 {
                    botoes = botoes.push(button(text("Abrir na página 1").size(13)).padding([9, 14]).style(tema::estilo_botao_secundario)
                        .on_press(Msg::AbrirDoComeco(r.caminho.clone())));
                }
                row![
                    folha(capa_de(&r.caminho), 112., 148.),
                    column![
                        text("CONTINUAR DE ONDE PAROU").size(11).font(negrito).color(muted),
                        text(nome).size(20).font(negrito).no_wrap().ellipsis(),
                        text(format!("{pasta} · página {pagina} de {} · aberto {}", r.paginas, recentes::quando(r.aberto_em, agora))).size(12.5).color(muted).no_wrap().ellipsis(),
                        Space::with_height(6),
                        barra,
                        Space::with_height(Length::FILL),
                        botoes,
                    ].spacing(4).width(Length::FILL).height(148),
                ].spacing(22).align_y(Align::Center).into()
            }
            None => row![
                container(icones::icone_traco("folder-open", 30.).color(vermelho_texto())).width(64).height(64).align_x(Align::Center).align_y(Align::Center)
                    .style(|_| Style { background: Some(Background::Color(fundo_vermelho())), border: Border { radius: [14.0; 4], ..Default::default() }, ..Default::default() }),
                column![
                    text("Abra um PDF para começar").size(20).font(negrito),
                    text("Solte um arquivo em qualquer lugar da janela, ou use Abrir. Imagens viram PDF de uma página.").size(12.5).color(muted),
                    Space::with_height(8),
                    button(text("Abrir PDF…").size(13).font(negrito)).padding([9, 16]).style(estilo_principal).on_press(Msg::Abrir),
                ].spacing(4).width(Length::FILL),
            ].spacing(22).align_y(Align::Center).height(148).into(),
        };
        let destaque = container(destaque).padding(20).width(Length::Fill(3))
            .style(|_| Style {
                background: Some(Background::Color(tema::COR_PAINEL())),
                border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [14.0; 4] },
                ..Default::default()
            });

        let atalho = |icone: &'static str, rotulo: &'static str, t: &'static str, msg: Msg| -> Element<'_, Msg> {
            button(row![icones::icone_traco(icone, 18.).color(vermelho_texto()), text(rotulo).size(13), Space::with_width(Length::FILL), tecla(t)]
                .spacing(12).align_y(Align::Center))
                .width(Length::FILL).height(54).padding([0, 14]).style(estilo_cartao).on_press(msg).into()
        };
        let atalhos = column![
            atalho("folder-open", "Abrir PDF…", "Ctrl+O", Msg::Abrir),
            atalho("file-plus", "PDF em branco", "Ctrl+N", Msg::NovoA4),
            atalho("copy-plus", "Juntar arquivos", "Ctrl+J", Msg::Juntar(crate::juntar::Jt::Escolher(false))),
        ].spacing(8).width(Length::Fill(1));

        // As ferramentas que precisam de um PDF pedem o arquivo e abrem nele (Msg::AbrirPara).
        let para = |m: Msg| Some(Msg::AbrirPara(Box::new(m)));
        let acoes: [(&str, &str, Option<Msg>); 12] = [
            ("cut", "Dividir", para(crate::tela::dividir())),
            ("photo", "Imagens para PDF", Some(Msg::Juntar(crate::juntar::Jt::Escolher(true)))),
            ("file-export", "Converter", para(Msg::Exp(crate::exportar::Ex::Abrir(crate::exportar::Formato::Word)))),
            ("scan", "OCR", para(Msg::Ocr(crate::ocr::Oc::Abrir))),
            ("file-zip", "Comprimir", para(Msg::Prot(crate::proteger::Pr::AbrirComprimir))),
            ("writing-sign", "Assinar", para(Msg::Assin(crate::assinar::As::Abrir))),
            ("lock", "Senha", para(Msg::Prot(crate::proteger::Pr::AbrirSenha))),
            ("file-export", "Extrair páginas", para(Msg::MudarAba(crate::Aba::Paginas))),
            ("list", "Fluxos em lote", Some(Msg::Fluxos(crate::fluxos::Fx::Abrir))),
            ("eraser", "Tarja definitiva", para(Msg::MudarAba(crate::Aba::Proteger))),
            ("columns", "Comparar", para(Msg::Comparar(crate::comparar::Cmp::Abrir))),
            ("forms", "Formulário", para(Msg::MudarAba(crate::Aba::Formularios))),
        ];
        let por_linha = 8;
        let mostrar = if self.inicio_mais { acoes.len() } else { por_linha };
        let mut grade = column![].spacing(8);
        let mut linha = row![].spacing(8);
        for (k, (icone, titulo, msg)) in acoes.into_iter().take(mostrar).enumerate() {
            let miolo = container(column![icones::icone_traco(icone, 22.).color(muted), text(titulo).size(12.5).no_wrap()].spacing(8).align_x(Align::Center))
                .width(Length::FILL).height(Length::FILL).align_x(Align::Center).align_y(Align::Center);
            linha = linha.push(button(miolo).width(Length::Fill(1)).height(84).padding(4).style(estilo_cartao).on_press_maybe(msg));
            if k % por_linha == por_linha - 1 { grade = grade.push(linha); linha = row![].spacing(8); }
        }
        let sobra = mostrar % por_linha;
        if sobra > 0 {
            for _ in sobra..por_linha { linha = linha.push(Space::with_width(Length::Fill(1))); }
            grade = grade.push(linha);
        }
        let ferramentas = column![
            row![
                text("Ferramentas").size(15).font(negrito),
                text("Pedem o PDF e abrem nele").size(12).color(muted),
                Space::with_width(Length::FILL),
                button(text(if self.inicio_mais { "Menos ferramentas" } else { "Todas as ferramentas" }).size(12.5).color(vermelho_texto()))
                    .style(|_, _| Style::default()).padding([4, 4]).on_press(Msg::InicioMais),
            ].spacing(12).align_y(Align::Center),
            grade,
        ].spacing(10);

        let titulo_recentes = row![
            text("Recentes").size(15).font(negrito),
            Space::with_width(Length::FILL),
            container(row![
                icones::icone_traco("search", 15.).color(muted),
                text_input("Buscar nos recentes", &self.busca_recentes).on_input(Msg::BuscaRecentes).size(13).padding([5, 4])
                    .style(|tema_, estado| { let mut e = pelicano::input_default(tema_, estado); e.background = Some(Background::Color(tema::COR_PAINEL())); e.border = Border::default(); e }),
            ].spacing(6).align_y(Align::Center))
                .padding([1, 10]).width(260)
                .style(|_| Style {
                    background: Some(Background::Color(tema::COR_PAINEL())),
                    border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [8.0; 4] },
                    ..Default::default()
                }),
        ].align_y(Align::Center);

        let corpo = column![
            row![destaque, atalhos].spacing(16),
            ferramentas,
            column![titulo_recentes, self.cartoes_recentes()].spacing(12),
            self.status_inicio(),
        ].spacing(28);

        column![
            topo,
            scrollable(container(container(corpo).max_width(1120.0)).width(Length::FILL).align_x(Align::Center).padding([32, 40])).height(Length::FILL),
        ].into()
    }

    fn status_inicio(&self) -> Element<'_, Msg> {
        if self.status.is_empty() { return Space::with_height(0).into(); }
        text(self.status.clone()).size(12).color(tema::COR_TEXTO_MUTED()).into()
    }

    /// Recentes em cartões com a capa (página 1), seis por linha; a busca mostra todos os que batem.
    fn cartoes_recentes(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let busca = self.busca_recentes.to_lowercase();
        let agora = recentes::agora();
        let achados: Vec<_> = self.recentes.itens.iter()
            .filter(|r| busca.is_empty() || r.caminho.to_string_lossy().to_lowercase().contains(&busca))
            .take(if busca.is_empty() { CAPAS } else { usize::MAX })
            .collect();
        if achados.is_empty() {
            return text(if self.recentes.itens.is_empty() { "Os PDFs que você abrir aparecem aqui." } else { "Nenhum recente com esse nome." })
                .size(13).color(muted).into();
        }
        let por_linha = 6;
        let mut grade = column![].spacing(14);
        for fila in achados.chunks(por_linha) {
            let mut linha = row![].spacing(14);
            for r in fila {
                let nome = r.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let capa = self.capas.get(&r.caminho).and_then(|(_, i)| i.as_ref());
                let moldura = container(folha(capa, 96., 128.)).width(Length::FILL).height(150).align_x(Align::Center).align_y(Align::Center)
                    .style(|_| Style { background: Some(Background::Color(tema::COR_PAINEL())), border: Border { radius: [8.0; 4], ..Default::default() }, ..Default::default() });
                let cartao = column![
                    moldura,
                    text(nome).size(13).no_wrap().ellipsis(),
                    text(format!("{} pág. · {}", r.paginas, recentes::quando(r.aberto_em, agora))).size(12).color(muted),
                ].spacing(6);
                linha = linha.push(button(cartao).width(Length::Fill(1)).padding(8)
                    .style(|_, status| Style {
                        background: matches!(status, Status::Hovered | Status::Pressed).then(|| Background::Color(tema::COR_HOVER())),
                        text_color: Some(tema::COR_TEXTO()),
                        border: Border { radius: [12.0; 4], ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Msg::AbrirCaminho(r.caminho.clone())));
            }
            for _ in fila.len()..por_linha { linha = linha.push(Space::with_width(Length::Fill(1))); }
            grade = grade.push(linha);
        }
        grade.into()
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
