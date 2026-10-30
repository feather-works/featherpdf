//! Inspetor interno (F12): a janela auxiliar que mostra o PDF por dentro (objetos,
//! fontes, imagens, tags, problemas) ao lado do desempenho e do console do
//! `feather_ui::monitor`. A leitura do documento é do core (`inspecionar`,
//! `arvore_objetos`, `detalhe_objeto`); aqui só o estado da janela e o desenho.
use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse;
use crate::compat::canvas::{self, Frame, Geometry, Path, Stroke};
use std::collections::HashSet;
use std::time::Instant;

use super::{App, Msg};
use feather_ui::monitor::{Evento, Nivel};
use feather_ui::ribbon::AbaRibbon;
use feather_ui::{cromo, marca, tema};
use featherpdf_core::{tamanho, Cor, DetalheObjeto, DocumentoPdf, Gravidade, Inspecao, LinhaArvore, Problema};

type Id = (u32, u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbaInsp { #[default] Objetos, Fontes, Imagens, Estrutura, Problemas, Desempenho, Console }

#[derive(Debug, Clone)]
pub enum Ins {
    Aba(AbaInsp),
    Filtro(String),
    /// Abre ou fecha um nó da árvore pelo caminho.
    Alternar(String),
    /// Clique numa linha da árvore.
    Escolher(Id),
    /// Clique numa referência ("20 0 R"), num problema ou numa linha de tabela: abre a árvore até ele.
    Ir(Id),
    MostrarPagina,
    Copiar,
    LiberarCache,
}

#[derive(Default)]
pub struct Inspetor {
    pub aba: AbaInsp,
    filtro: String,
    abertos: HashSet<String>,
    selecionado: Option<Id>,
    /// O documento mudou: reler na próxima mensagem (se a janela estiver aberta).
    pub sujo: bool,
    leitura: Option<Inspecao>,
    arvore: Vec<LinhaArvore>,
    detalhe: Option<DetalheObjeto>,
}

impl Inspetor {
    pub fn tem_leitura(&self) -> bool { self.leitura.is_some() }

    /// Documento novo: os números de objeto não valem mais.
    pub fn novo_documento(&mut self) {
        self.selecionado = None;
        self.abertos.clear();
        self.filtro.clear();
        self.sujo = true;
    }

    fn refazer(&mut self, pdf: &DocumentoPdf) {
        self.arvore = pdf.arvore_objetos(&self.abertos, &self.filtro);
        self.detalhe = self.selecionado.and_then(|id| pdf.detalhe_objeto(id));
    }

    fn ir(&mut self, pdf: &DocumentoPdf, id: Id) {
        self.filtro.clear();
        self.abertos.extend(pdf.caminho_ate(id).unwrap_or_default());
        self.selecionado = Some(id);
    }
}

fn roxo() -> Color { if tema::escuro() { Color::rgb8(189, 155, 255) } else { Color::rgb8(111, 66, 193) } }
fn verde() -> Color { if tema::escuro() { Color::rgb8(134, 224, 160) } else { Color::rgb8(22, 120, 60) } }
fn ambar() -> Color { if tema::escuro() { Color::rgb8(245, 193, 97) } else { Color::rgb8(176, 109, 0) } }
/// Cor `b` sobre `a` na proporção `t` (o `tema` tem uma igual, privada).
fn mistura(a: Color, b: Color, t: f32) -> Color {
    Color::rgb(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t)
}
fn mono(t: impl Into<std::borrow::Cow<'static, str>>) -> Text<'static> { text(t).font(Font::MONO).size(12) }

fn painel(_: &Theme) -> Style {
    Style {
        background: Some(Background::Color(tema::COR_PAINEL())),
        border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [8.0; 4] },
        text_color: Some(tema::COR_TEXTO()),
        ..Default::default()
    }
}

fn vazio<'a>(titulo: &'a str, detalhe: &'a str) -> Element<'a, Msg> {
    container(column![text(titulo).size(16), text(detalhe).size(12).color(tema::COR_TEXTO_MUTED())].spacing(6).align_x(Align::Center))
        .width(Length::FILL).height(Length::FILL).center().into()
}

fn msg(i: Ins) -> Msg { Msg::Insp(i) }

/// "1–3, 5, 8" (no máximo uns 40 caracteres).
fn intervalos(paginas: &[u32]) -> String {
    let mut partes: Vec<String> = Vec::new();
    let mut i = 0;
    while i < paginas.len() {
        let mut j = i;
        while j + 1 < paginas.len() && paginas[j + 1] == paginas[j] + 1 { j += 1; }
        partes.push(if j > i { format!("{}–{}", paginas[i], paginas[j]) } else { paginas[i].to_string() });
        i = j + 1;
    }
    let t = partes.join(", ");
    if t.chars().count() > 40 { format!("{}…", t.chars().take(40).collect::<String>()) } else if t.is_empty() { "—".into() } else { t }
}

/// Gráfico de linha pequeno do quadro Recursos.
struct Linha { valores: Vec<f32>, maximo: f32, cor: Color }

impl canvas::Program<Msg> for Linha {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rect, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if self.valores.len() >= 2 {
            let passo = bounds.width / (self.valores.len() - 1) as f32;
            let y = |v: f32| bounds.height - 2.0 - (v / self.maximo.max(f32::EPSILON)).clamp(0.0, 1.0) * (bounds.height - 4.0);
            let caminho = Path::new(|p| {
                p.move_to(Point::new(0.0, y(self.valores[0])));
                for (i, v) in self.valores.iter().enumerate().skip(1) { p.line_to(Point::new(i as f32 * passo, y(*v))); }
            });
            frame.stroke(&caminho, Stroke::default().with_color(self.cor).with_width(1.5));
        }
        vec![frame.into_geometry()]
    }
}

fn grafico<'a>(valores: Vec<f32>, maximo: f32, cor: Color) -> Element<'a, Msg> {
    container(canvas::Canvas::new(Linha { valores, maximo, cor }).width(Length::FILL).height(34))
        .padding([2, 4]).style(painel).into()
}

fn cartao_problema(p: &Problema) -> Element<'_, Msg> {
    let cor = if p.gravidade == Gravidade::Erro { tema::COR_ERRO() } else { ambar() };
    let corpo = column![text(&p.titulo).size(12).color(cor), text(&p.detalhe).size(11).color(tema::COR_TEXTO_MUTED())].spacing(3);
    button(corpo).width(Length::FILL).padding([7, 9])
        .style(move |_, estado| Style {
            background: Some(Background::Color(mistura(tema::COR_PAINEL(), cor,
                if estado == Status::Hovered { 0.16 } else { 0.08 }))),
            border: Border { color: cor, width: 1.0, radius: [6.0; 4] },
            text_color: Some(tema::COR_TEXTO()),
            ..Default::default()
        })
        .on_press_maybe(p.objeto.map(|id| msg(Ins::Ir(id)))).into()
}

impl App {
    pub(crate) fn reler_inspetor(&mut self) {
        let insp = &mut self.monitor.insp;
        insp.sujo = false;
        let Some(a) = &self.aberto else {
            *insp = Inspetor { aba: insp.aba, ..Default::default() };
            return;
        };
        let inicio = Instant::now();
        insp.leitura = Some(a.pdf.inspecionar());
        if insp.selecionado.is_none() {
            if let Some(id) = a.pdf.objeto_da_pagina(self.pagina) { insp.ir(&a.pdf, id); }
        }
        insp.refazer(&a.pdf);
        // O objeto escolhido sumiu numa edição: volta para a página atual.
        if insp.detalhe.is_none() {
            if let Some(id) = a.pdf.objeto_da_pagina(self.pagina) { insp.ir(&a.pdf, id); insp.refazer(&a.pdf); }
        }
        let ms = inicio.elapsed().as_millis();
        if ms >= 100 { self.monitor.tela.registrar(Nivel::Info, "Inspetor", &format!("[inspetor] documento lido em {ms} ms")); }
    }

    pub(crate) fn update_inspetor(&mut self, m: Ins) -> Task<Msg> {
        let insp = &mut self.monitor.insp;
        let pdf = self.aberto.as_ref().map(|a| &a.pdf);
        match m {
            Ins::Aba(aba) => {
                insp.aba = aba;
                // O console do monitor só acompanha o fim quando está na aba dele.
                return self.monitor.tela.atualizar(Evento::Aba(if aba == AbaInsp::Console { 1 } else { 0 }));
            }
            Ins::Filtro(t) => insp.filtro = t,
            Ins::Alternar(c) => if !insp.abertos.remove(&c) { insp.abertos.insert(c); },
            Ins::Escolher(id) => insp.selecionado = Some(id),
            Ins::Ir(id) => {
                insp.aba = AbaInsp::Objetos;
                if let Some(pdf) = pdf { insp.ir(pdf, id); }
            }
            Ins::MostrarPagina => {
                let Some(p) = insp.detalhe.as_ref().and_then(|d| d.pagina) else { return Task::none() };
                return self.update_interno(Msg::IrPagina(p));
            }
            Ins::Copiar => {
                let Some(d) = &insp.detalhe else { return Task::none() };
                return Task::write_clipboard(d.texto());
            }
            Ins::LiberarCache => {
                let bytes: usize = self.desenhadas.values().map(|d| d.largura as usize * d.altura as usize * 4).sum();
                self.desenhadas.clear();
                self.monitor.tela.registrar(Nivel::Info, "Memória", &format!("[memória] cache de páginas liberado: {}", tamanho(bytes)));
                // As páginas à vista são pedidas de novo.
                return self.invalidar();
            }
        }
        if let Some(pdf) = pdf { insp.refazer(pdf); }
        Task::none()
    }

    pub(crate) fn view_monitor(&self, id: WindowId) -> Element<'_, Msg> {
        let insp = &self.monitor.insp;
        let muted = tema::COR_TEXTO_MUTED();
        let resumo = insp.leitura.as_ref().map(|l| {
            let r = &l.resumo;
            format!("PDF {} · {} páginas · {} objetos · {}", r.versao, r.paginas, r.objetos,
                if r.xref_comprimido { "xref comprimido" } else { "tabela xref" })
        }).unwrap_or_default();
        let titulo = self.nome_documento().map_or("Inspetor".into(), |n| format!("Inspetor — {n}"));
        let cabecalho = row![marca::icone_atual(18.0), text(titulo).size(15), Space::with_width(Length::FILL), text(resumo).size(12).color(muted)]
            .spacing(9).align_y(Align::Center).padding([8, 14]);
        let problemas = insp.leitura.as_ref().map_or(0, |l| l.problemas.len());
        let mut abas = row![].spacing(3).align_y(Align::Center);
        for (aba, rotulo) in [
            (AbaInsp::Objetos, "Objetos".to_string()), (AbaInsp::Fontes, "Fontes".into()), (AbaInsp::Imagens, "Imagens".into()),
            (AbaInsp::Estrutura, "Estrutura (tags)".into()), (AbaInsp::Problemas, format!("Problemas {problemas}")),
            (AbaInsp::Desempenho, "Desempenho".into()), (AbaInsp::Console, "Console".into()),
        ] {
            abas = abas.push(feather_ui::ribbon::aba(&AbaRibbon::nova(aba, rotulo), &insp.aba, |a| msg(Ins::Aba(a))));
        }
        let barra = container(abas).padding([4, 10]).width(Length::FILL).height(44).style(tema::estilo_barra_abas);
        let sem_documento = || vazio("Nenhum PDF aberto", "Abra um documento para inspecionar objetos, fontes e imagens.");
        let corpo: Element<'_, Msg> = match (insp.aba, &insp.leitura) {
            (AbaInsp::Desempenho, _) => row![
                container(self.monitor.tela.view_memoria(Msg::MonitorEvento)).width(Length::Fill(3)),
                container(self.monitor.tela.view_metricas(self.metricas_monitor())).width(Length::Fill(2)),
            ].spacing(14).into(),
            (AbaInsp::Console, _) => self.monitor.tela.view_console(Msg::MonitorEvento),
            (_, None) => sem_documento(),
            (AbaInsp::Objetos, Some(l)) => row![
                container(self.coluna_arvore()).width(300),
                container(self.coluna_objeto()).width(Length::FILL),
                container(self.coluna_recursos(l)).width(270),
            ].spacing(10).into(),
            (AbaInsp::Fontes, Some(l)) => tabela_fontes(l),
            (AbaInsp::Imagens, Some(l)) => tabela_imagens(l),
            (AbaInsp::Estrutura, Some(l)) => estrutura(l),
            (AbaInsp::Problemas, Some(l)) if l.problemas.is_empty() => vazio("Nenhum problema encontrado", "Fontes, imagens e referências parecem em ordem."),
            (AbaInsp::Problemas, Some(l)) => scrollable(column(l.problemas.iter().map(cartao_problema)).spacing(6)).height(Length::FILL).into(),
        };
        let rodape = row![Space::with_width(Length::FILL), text("F12 abre e fecha. Só leitura.").size(11).color(muted)].padding([4, 14]);
        let tela = column![
            cabecalho,
            barra,
            container(corpo).padding([10, 12]).width(Length::FILL).height(Length::FILL),
            container(Space::with_height(1)).width(Length::FILL).style(tema::estilo_linha),
            rodape,
        ];
        cromo::janela_com_barra("Inspetor interno (F12)", tela.into(), Self::acoes_janela(id))
    }

    fn coluna_arvore(&self) -> Element<'_, Msg> {
        let insp = &self.monitor.insp;
        let muted = tema::COR_TEXTO_MUTED();
        let filtro = text_input("Filtrar (ex.: /Type /Font)", &insp.filtro)
            .on_input(|t| msg(Ins::Filtro(t))).size(12).padding([6, 8]).style(tema::estilo_entrada_suspensa);
        let mut linhas = column![].spacing(0);
        for l in &insp.arvore {
            let seta = if !l.expansivel { " " } else if l.aberto { "▾" } else { "▸" };
            let alternar = button(mono(seta).color(muted)).padding([2, 4]).style(botao_texto)
                .on_press_maybe(l.expansivel.then(|| msg(Ins::Alternar(l.caminho.clone()))));
            let mut conteudo = row![].spacing(6).align_y(Align::Center);
            if !l.chave.is_empty() { conteudo = conteudo.push(mono(l.chave.clone()).color(roxo())); }
            if let Some((n, g)) = l.referencia { conteudo = conteudo.push(mono(format!("{n} {g} R")).color(tema::COR_ERRO())); }
            conteudo = conteudo.push(text(l.dica.clone()).size(11).color(muted)).push(Space::with_width(Length::FILL));
            let escolhido = l.referencia.is_some() && l.referencia == insp.selecionado;
            let acao = l.referencia.map(|r| msg(Ins::Escolher(r)))
                .or_else(|| l.expansivel.then(|| msg(Ins::Alternar(l.caminho.clone()))));
            let linha = button(conteudo).width(Length::FILL).padding([2, 4])
                .style(move |_, estado| Style {
                    background: (escolhido || estado == Status::Hovered).then(|| Background::Color(
                        mistura(tema::COR_PAINEL(), tema::COR_AZUL_ACENTO(), if escolhido { 0.14 } else { 0.06 }))),
                    text_color: Some(tema::COR_TEXTO()),
                    border: Border { radius: [4.0; 4], ..Default::default() },
                    ..Default::default()
                })
                .on_press_maybe(acao);
            linhas = linhas.push(row![Space::with_width(l.nivel as f32 * 14.0), alternar, linha].align_y(Align::Center));
        }
        if insp.arvore.is_empty() {
            linhas = linhas.push(text("Nenhum objeto bate com o filtro.").size(12).color(muted));
        }
        column![filtro, container(scrollable(linhas.padding(6)).height(Length::FILL)).style(painel).height(Length::FILL)]
            .spacing(8).into()
    }

    fn coluna_objeto(&self) -> Element<'_, Msg> {
        let insp = &self.monitor.insp;
        let muted = tema::COR_TEXTO_MUTED();
        let Some(d) = &insp.detalhe else {
            return vazio("Nenhum objeto escolhido", "Clique num objeto da árvore.");
        };
        let total = self.total_paginas();
        let subtitulo = match d.pagina { Some(p) => format!("{} · página {p} de {total}", d.tipo), None => d.tipo.clone() };
        let cabecalho = row![
            text(format!("{} {} obj", d.id.0, d.id.1)).font(Font::MONO).size(14),
            text(subtitulo).size(12).color(muted),
            Space::with_width(Length::FILL),
            button(text("Mostrar na página").size(12)).padding([5, 10]).style(tema::estilo_botao_secundario)
                .on_press_maybe(d.pagina.map(|_| msg(Ins::MostrarPagina))),
            button(text("Copiar").size(12)).padding([5, 10]).style(tema::estilo_botao_secundario).on_press(msg(Ins::Copiar)),
        ].spacing(10).align_y(Align::Center);
        let mut linhas = column![].spacing(1);
        for l in &d.linhas {
            let pedacos: Vec<Span<'static, Msg>> = l.iter().map(|p| {
                let s = span(p.texto.clone());
                match p.cor {
                    Cor::Nome => s.color(roxo()),
                    Cor::Texto => s.color(verde()),
                    Cor::Comentario => s.color(muted),
                    Cor::Referencia(id) => s.color(tema::COR_ERRO()).underline().link(msg(Ins::Ir(id))),
                    Cor::Numero | Cor::Pontuacao => s.color(tema::COR_TEXTO()),
                }
            }).collect();
            linhas = linhas.push(rich_text(pedacos).font(Font::MONO).size(12));
        }
        let mut coluna = column![
            cabecalho,
            container(scrollable(linhas.padding(10)).height(Length::FILL)).style(painel).width(Length::FILL).height(Length::Fill(3)),
        ].spacing(8);
        if let Some(((n, g), fluxo)) = &d.fluxo {
            coluna = coluna
                .push(text(format!("Fluxo de conteúdo ({n} {g} R) — decodificado")).size(12).color(muted))
                .push(container(scrollable(mono(fluxo.clone()).size(11)).height(Length::FILL)).padding(10)
                    .style(painel).width(Length::FILL).height(Length::Fill(2)));
        }
        coluna.into()
    }

    fn coluna_recursos<'a>(&'a self, l: &'a Inspecao) -> Element<'a, Msg> {
        let m = &self.monitor;
        let muted = tema::COR_TEXTO_MUTED();
        let mib = |kb: u64| format!("{:.0} MB", kb as f64 / 1024.0);
        let item = |rotulo: String, valor: String| row![text(rotulo).size(11).color(muted), Space::with_width(Length::FILL), text(valor).size(11)].spacing(6);
        let ram = m.ram.last().map_or("—".into(), |&kb| mib(kb));
        let pico = m.sistema.pico_kb.or_else(|| m.ram.iter().max().copied()).map_or("—".into(), mib);
        let nucleos = std::thread::available_parallelism().map_or(0, |n| n.get());
        let cpu = m.cpu.last().map_or("—".into(), |c| format!("{c:.0}%"));
        let threads = m.sistema.threads.map_or(String::new(), |t| format!(" · {t} threads"));
        let maior_ram = m.ram.iter().max().copied().unwrap_or(1) as f32 * 1.1;
        let cache: usize = self.desenhadas.values().map(|d| d.largura as usize * d.altura as usize * 4).sum();
        let mut c = column![
            text("Recursos").size(14),
            item("Memória RAM".into(), format!("{ram} · pico {pico}")),
            grafico(m.ram.iter().map(|&v| v as f32).collect(), maior_ram, verde()),
            item("CPU".into(), format!("{cpu} · {nucleos} núcleos{threads}")),
            grafico(m.cpu.clone(), 100.0, roxo()),
            item("Páginas desenhadas".into(), format!("{} · {}", self.desenhadas.len(), tamanho(cache))),
        ].spacing(6);
        if let Some((ms, p)) = m.ultimo { c = c.push(item(format!("Desenhar p. {p}"), format!("{ms} ms"))); }
        if let Some(t) = m.abrir { c = c.push(item("Abrir o arquivo".into(), crate::monitor::tempo(t))); }
        if let Some((lido, gravado)) = m.sistema.disco {
            c = c.push(item("Disco lido / gravado".into(), format!("{} / {}", tamanho(lido as usize), tamanho(gravado as usize))));
        }
        if !self.desenhadas.is_empty() {
            c = c.push(button(text("Liberar cache de páginas").size(11).color(tema::COR_AZUL_ACENTO()))
                .padding(0).style(botao_texto).on_press(msg(Ins::LiberarCache)));
        }
        c = c.push(Space::with_height(6)).push(text(format!("Problemas ({})", l.problemas.len())).size(14));
        if l.problemas.is_empty() { c = c.push(text("Nenhum encontrado.").size(11).color(muted)); }
        for p in l.problemas.iter().take(4) { c = c.push(cartao_problema(p)); }
        if l.problemas.len() > 4 {
            c = c.push(button(text(format!("Ver todos ({})", l.problemas.len())).size(11).color(tema::COR_AZUL_ACENTO()))
                .padding(0).style(botao_texto).on_press(msg(Ins::Aba(AbaInsp::Problemas))));
        }
        scrollable(c.padding([0, 6])).height(Length::FILL).into()
    }
}

/// Tabela simples: cabeçalho e linhas clicáveis (levam ao objeto na aba Objetos).
fn tabela<'a>(colunas: &[(&'a str, f32)], linhas: Vec<(Id, Vec<String>)>) -> Element<'a, Msg> {
    let largura = |w: f32| if w > 0.0 { Length::Fixed(w) } else { Length::FILL };
    let cabecalho = row(colunas.iter().map(|&(t, w)| text(t).size(11).color(tema::COR_TEXTO_MUTED()).width(largura(w)).into())).spacing(10);
    let mut corpo = column![].spacing(0);
    for (id, valores) in linhas {
        let celulas = row(valores.into_iter().zip(colunas).map(|(v, &(_, w))| mono(v).width(largura(w)).into())).spacing(10);
        corpo = corpo.push(button(celulas).width(Length::FILL).padding([4, 0]).style(botao_texto).on_press(msg(Ins::Ir(id))));
    }
    column![
        container(cabecalho).padding([0, 10]),
        container(scrollable(corpo.padding([0, 10])).height(Length::FILL)).style(painel).height(Length::FILL),
    ].spacing(6).into()
}

fn sim(b: bool) -> String { if b { "sim".into() } else { "não".into() } }

fn tabela_fontes(l: &Inspecao) -> Element<'_, Msg> {
    if l.fontes.is_empty() { return vazio("Nenhuma fonte", "Este PDF não tem fontes (é só imagem ou desenho?)."); }
    let linhas = l.fontes.iter().map(|f| (f.id, vec![
        if f.recurso.is_empty() { format!("{} {} R", f.id.0, f.id.1) } else { f.recurso.clone() },
        f.nome.clone(), f.tipo.clone(), sim(f.embutida), sim(f.subconjunto), sim(f.to_unicode), intervalos(&f.paginas),
    ])).collect();
    tabela(&[("Recurso", 70.0), ("Nome", 0.0), ("Tipo", 90.0), ("Embutida", 70.0), ("Subconjunto", 90.0), ("ToUnicode", 80.0), ("Páginas", 140.0)], linhas)
}

fn tabela_imagens(l: &Inspecao) -> Element<'_, Msg> {
    if l.imagens.is_empty() { return vazio("Nenhuma imagem", "Este PDF não tem imagens."); }
    let linhas = l.imagens.iter().map(|i| (i.id, vec![
        format!("{} {} R", i.id.0, i.id.1), format!("{}×{}", i.largura, i.altura), i.formato.clone(), tamanho(i.bytes),
        intervalos(&i.paginas), i.dpi.map_or("—".into(), |d| format!("{d:.0}")),
    ])).collect();
    tabela(&[("Objeto", 90.0), ("Tamanho (px)", 120.0), ("Formato", 100.0), ("Bytes", 90.0), ("Páginas", 0.0), ("DPI", 70.0)], linhas)
}

fn estrutura(l: &Inspecao) -> Element<'_, Msg> {
    let Some(nos) = &l.estrutura else {
        return vazio("Este PDF não tem marcação de estrutura (tags)", "Leitores de tela leem o texto na ordem em que foi desenhado.");
    };
    if nos.is_empty() { return vazio("A árvore de estrutura está vazia", "O PDF tem /StructTreeRoot, mas sem elementos."); }
    let linhas = column(nos.iter().map(|n| row![
        Space::with_width(n.nivel as f32 * 16.0),
        mono(n.tipo.clone()).color(roxo()),
        text(n.detalhe.clone()).size(11).color(tema::COR_TEXTO_MUTED()),
    ].spacing(8).align_y(Align::Center).into())).spacing(2);
    container(scrollable(linhas.padding(10)).height(Length::FILL)).style(painel).height(Length::FILL).into()
}
