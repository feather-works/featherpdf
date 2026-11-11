//! Imprimir (Ctrl+P): janela própria, como a do Acrobat, que manda o documento (com as
//! edições ainda não salvas) para o CUPS pelo `lp` ou, no Windows, pelo GDI
//! (`imprimir_windows.rs`). Impressora, cópias, páginas, frente e verso, preto e branco,
//! ajustar à folha, páginas por folha e ordem inversa.

use feather_ui::dialogo as dlg;
use feather_ui::{cromo, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Auxiliar, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paginas { Todas, Atual, Intervalo, Impares, Pares }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lados { Um, BordaLonga, BordaCurta }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientacao { Automatica, Retrato, Paisagem }

/// Tamanho do papel: o da impressora ou um escolhido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel { Impressora, A4, Carta, Oficio, A3, A5 }

impl Papel {
    pub const TODOS: [Papel; 6] = [Papel::Impressora, Papel::A4, Papel::Carta, Papel::Oficio, Papel::A3, Papel::A5];
    /// Nome do tamanho no CUPS (`-o media=`).
    #[cfg_attr(windows, allow(dead_code))]
    fn cups(self) -> Option<&'static str> {
        match self { Papel::Impressora => None, Papel::A4 => Some("A4"), Papel::Carta => Some("Letter"), Papel::Oficio => Some("Legal"), Papel::A3 => Some("A3"), Papel::A5 => Some("A5") }
    }
    /// Código DMPAPER_* do Windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn windows(self) -> Option<i16> {
        match self { Papel::Impressora => None, Papel::A4 => Some(9), Papel::Carta => Some(1), Papel::Oficio => Some(5), Papel::A3 => Some(8), Papel::A5 => Some(11) }
    }
}

impl std::fmt::Display for Papel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Papel::Impressora => "O da impressora", Papel::A4 => "A4 (210 × 297 mm)", Papel::Carta => "Carta (216 × 279 mm)",
            Papel::Oficio => "Ofício / Legal (216 × 356 mm)", Papel::A3 => "A3 (297 × 420 mm)", Papel::A5 => "A5 (148 × 210 mm)",
        })
    }
}

#[derive(Debug, Clone)]
pub enum Im {
    Abrir,
    Impressoras(Vec<String>, Option<String>),
    Impressora(String),
    Copias(i32),
    Agrupar(bool),
    Paginas(Paginas),
    Intervalo(String),
    Lados(Lados),
    Orientacao(Orientacao),
    Papel(Papel),
    /// Com ou sem os comentários no papel.
    Comentarios(bool),
    Cinza(bool),
    Ajustar(bool),
    PorFolha(u32),
    Inversa(bool),
    Previa(i32),
    PreviaPronta(u32, bool, Option<Quadro>),
    Imprimir,
    Pronto(Result<String, String>),
}

#[derive(Clone)]
pub struct Impressao {
    impressoras: Vec<String>,
    impressora: Option<String>,
    procurando: bool,
    copias: u32,
    agrupar: bool,
    paginas: Paginas,
    intervalo: String,
    lados: Lados,
    orientacao: Orientacao,
    papel: Papel,
    comentarios: bool,
    cinza: bool,
    ajustar: bool,
    por_folha: u32,
    inversa: bool,
    /// Posição, entre as páginas escolhidas, da que aparece na prévia.
    previa: usize,
    /// Página desenhada para a prévia (e se saiu em cinza).
    imagem: Option<(u32, bool, Image)>,
    enviando: bool,
    resultado: Option<Result<String, String>>,
}

impl Default for Impressao {
    fn default() -> Self {
        Self { impressoras: vec![], impressora: None, procurando: false, copias: 1, agrupar: true, paginas: Paginas::Todas, intervalo: String::new(),
            lados: Lados::Um, orientacao: Orientacao::Automatica, papel: Papel::Impressora, comentarios: true, cinza: false, ajustar: true, por_folha: 1, inversa: false, previa: 0, imagem: None, enviando: false, resultado: None }
    }
}

/// "1-3, 5" → páginas 1, 2, 3 e 5, todas dentro de 1..=total; `None` se algo não se lê.
pub(crate) fn ler_intervalo(texto: &str, total: u32) -> Option<Vec<u32>> {
    let mut v = Vec::new();
    for parte in texto.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match parte.split_once('-') {
            Some((a, b)) => (a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?),
            None => { let n = parte.parse::<u32>().ok()?; (n, n) }
        };
        if a == 0 || a > b || b > total { return None; }
        v.extend(a..=b);
    }
    (!v.is_empty()).then_some(v)
}

#[cfg(windows)]
use crate::imprimir_windows::impressoras;

/// Impressoras do CUPS e a padrão (`lpstat`).
#[cfg(not(windows))]
fn impressoras() -> (Vec<String>, Option<String>) {
    let saida = |args: &[&str]| std::process::Command::new("lpstat").args(args).env("LC_ALL", "C").output().ok()
        .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let lista: Vec<String> = saida(&["-e"]).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    let padrao = saida(&["-d"]).split(':').nth(1).map(|s| s.trim().to_string()).filter(|s| lista.contains(s));
    (lista, padrao)
}

impl Impressao {
    fn escolhidas(&self, total: u32, atual: u32) -> Option<Vec<u32>> {
        match self.paginas {
            Paginas::Todas => Some((1..=total).collect()),
            Paginas::Atual => Some(vec![atual]),
            Paginas::Intervalo => ler_intervalo(&self.intervalo, total),
            Paginas::Impares => Some((1..=total).step_by(2).collect()),
            Paginas::Pares => Some((2..=total).step_by(2).collect()).filter(|v: &Vec<u32>| !v.is_empty()),
        }
    }

    /// Argumentos do `lp` (sem o arquivo).
    #[cfg_attr(windows, allow(dead_code))]
    fn argumentos(&self, impressora: &str, atual: u32) -> Vec<String> {
        let mut a = vec!["-d".into(), impressora.into(), "-n".into(), self.copias.to_string(), "-t".into(), "FeatherPDF".into()];
        let mut o = |op: String| { a.push("-o".into()); a.push(op); };
        match self.paginas {
            Paginas::Todas => {}
            Paginas::Atual => o(format!("page-ranges={atual}")),
            Paginas::Intervalo => o(format!("page-ranges={}", self.intervalo.replace(' ', "").replace(';', ","))),
            Paginas::Impares => o("page-set=odd".into()),
            Paginas::Pares => o("page-set=even".into()),
        }
        o(match self.lados { Lados::Um => "sides=one-sided", Lados::BordaLonga => "sides=two-sided-long-edge", Lados::BordaCurta => "sides=two-sided-short-edge" }.into());
        match self.orientacao {
            Orientacao::Automatica => {}
            Orientacao::Retrato => o("orientation-requested=3".into()),
            Orientacao::Paisagem => o("orientation-requested=4".into()),
        }
        if let Some(m) = self.papel.cups() { o(format!("media={m}")); }
        if self.cinza { o("print-color-mode=monochrome".into()); }
        if self.ajustar { o("fit-to-page".into()); }
        if self.por_folha > 1 { o(format!("number-up={}", self.por_folha)); }
        if self.inversa { o("outputorder=reverse".into()); }
        if self.copias > 1 && self.agrupar { o("collate=true".into()); }
        a
    }
}

impl App {
    /// Desenha, fora da tela, a página que a prévia mostra.
    fn pedir_previa(&mut self) -> Task<Msg> {
        let p = &self.impressao;
        let Some(n) = p.escolhidas(self.total_paginas(), self.pagina).and_then(|v| v.get(p.previa).copied()) else { return Task::none() };
        let cinza = p.cinza;
        if p.imagem.as_ref().is_some_and(|i| i.0 == n && i.1 == cinza) { return Task::none(); }
        let Some(a) = &self.aberto else { return Task::none() };
        let vis = a.vis.clone();
        Task::perform(async move {
            tokio::task::spawn_blocking(move || vis.desenhar(n, 0.6).ok().map(|mut r| {
                if cinza {
                    for px in r.rgba.chunks_exact_mut(4) {
                        let y = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as u8;
                        px[..3].fill(y);
                    }
                }
                imagem_rgba(r.largura, r.altura, r.rgba)
            })).await.ok().flatten()
        }, move |h| Msg::Imp(Im::PreviaPronta(n, cinza, h.map(Quadro))))
    }

    pub fn update_imprimir(&mut self, m: Im) -> Task<Msg> {
        let (total, atual) = (self.total_paginas(), self.pagina);
        let p = &mut self.impressao;
        match m {
            Im::Abrir => {
                if self.aberto.is_none() { return Task::none(); }
                p.resultado = None;
                p.previa = 0;
                p.procurando = true;
                let abrir = if self.janelas.aberta(Auxiliar::Imprimir) { Task::none() } else { self.alternar_auxiliar(Auxiliar::Imprimir) };
                let procurar = Task::perform(async { tokio::task::spawn_blocking(impressoras).await.unwrap_or_default() }, |(l, d)| Msg::Imp(Im::Impressoras(l, d)));
                return Task::batch([abrir, procurar, self.pedir_previa()]);
            }
            Im::Impressoras(lista, padrao) => {
                p.procurando = false;
                if !p.impressora.as_ref().is_some_and(|i| lista.contains(i)) { p.impressora = padrao.or_else(|| lista.first().cloned()); }
                p.impressoras = lista;
            }
            Im::Impressora(i) => p.impressora = Some(i),
            Im::Copias(d) => p.copias = (p.copias as i32 + d).clamp(1, 999) as u32,
            Im::Paginas(v) => { p.paginas = v; p.previa = 0; return self.pedir_previa(); }
            Im::Intervalo(t) => { p.intervalo = t; p.paginas = Paginas::Intervalo; p.previa = 0; return self.pedir_previa(); }
            Im::Lados(v) => p.lados = v,
            Im::Orientacao(v) => p.orientacao = v,
            Im::Papel(v) => p.papel = v,
            Im::Comentarios(v) => p.comentarios = v,
            Im::Cinza(v) => { p.cinza = v; return self.pedir_previa(); }
            Im::Agrupar(v) => p.agrupar = v,
            Im::Ajustar(v) => p.ajustar = v,
            Im::PorFolha(v) => p.por_folha = v,
            Im::Inversa(v) => p.inversa = v,
            Im::Previa(d) => {
                let n = p.escolhidas(total, atual).map_or(1, |v| v.len());
                p.previa = (p.previa as i32 + d).clamp(0, n as i32 - 1) as usize;
                return self.pedir_previa();
            }
            Im::PreviaPronta(n, cinza, h) => p.imagem = h.map(|h| (n, cinza, h.0)),
            Im::Imprimir => {
                let (Some(a), Some(impressora)) = (&self.aberto, p.impressora.clone()) else { return Task::none() };
                if p.escolhidas(total, atual).is_none() { return Task::none(); }
                p.enviando = true;
                p.resultado = None;
                #[cfg(windows)]
                {
                    let pedido = crate::imprimir_windows::Pedido { impressora, paginas: p.escolhidas(total, atual).unwrap_or_default(), copias: p.copias,
                        agrupar: p.agrupar, lados: p.lados, orientacao: p.orientacao, papel: p.papel, cinza: p.cinza, ajustar: p.ajustar, por_folha: p.por_folha, inversa: p.inversa };
                    let (pdf, tamanhos, comentarios) = (a.pdf.clone(), a.tamanhos.clone(), p.comentarios);
                    return Task::perform(async move {
                        tokio::task::spawn_blocking(move || -> Result<String, String> {
                            // A impressão sai de uma cópia com (ou sem) os comentários, desenhada de novo.
                            let mut pdf = pdf;
                            pdf.comentarios_na_impressao(comentarios)?;
                            let vis = featherpdf_io::Visualizador::novo(pdf.serializar()?)?;
                            crate::imprimir_windows::imprimir(&vis, &tamanhos, pedido)
                        }).await.unwrap_or_else(|e| Err(e.to_string()))
                    }, |r| Msg::Imp(Im::Pronto(r)));
                }
                #[cfg(not(windows))]
                let args = p.argumentos(&impressora, atual);
                #[cfg(not(windows))]
                let (pdf, comentarios) = (a.pdf.clone(), p.comentarios);
                #[cfg(not(windows))]
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || -> Result<String, String> {
                        // O documento como está na tela, com as edições ainda não salvas.
                        let pasta = crate::recentes::pasta_temporaria().map_err(|e| e.to_string())?;
                        let arquivo = pasta.join(format!("imprimir-{}.pdf", std::process::id()));
                        let mut pdf = pdf;
                        pdf.comentarios_na_impressao(comentarios)?;
                        std::fs::write(&arquivo, pdf.serializar()?).map_err(|e| e.to_string())?;
                        let saida = std::process::Command::new("lp").args(&args).arg(&arquivo).output();
                        // O lp já copiou o arquivo para a fila: a cópia (com o que não foi salvo nem tarjado) não fica no /tmp.
                        let _ = std::fs::remove_file(&arquivo);
                        let saida = saida.map_err(|e| format!("Não consegui chamar o lp (CUPS): {e}"))?;
                        if !saida.status.success() { return Err(String::from_utf8_lossy(&saida.stderr).trim().to_string()); }
                        Ok(format!("Enviado para {impressora}. {}", String::from_utf8_lossy(&saida.stdout).trim()))
                    }).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Imp(Im::Pronto(r)));
            }
            Im::Pronto(r) => {
                p.enviando = false;
                if let Ok(t) = &r { self.status = t.clone(); }
                p.resultado = Some(r);
            }
        }
        Task::none()
    }

    pub fn view_imprimir(&self, id: WindowId) -> Element<'_, Msg> {
        let p = &self.impressao;
        let muted = tema::COR_TEXTO_MUTED();
        let (total, atual) = (self.total_paginas(), self.pagina);
        let escolhidas = p.escolhidas(total, atual);
        let lista = escolhidas.clone().unwrap_or_default();
        let folhas = (lista.len() as u32).div_ceil(p.por_folha);
        let papeis = if p.lados == Lados::Um { folhas } else { folhas.div_ceil(2) };

        // Impressora: caixa com ícone, a lista e o estado.
        let escolha_impressora: Element<'_, Msg> = if p.procurando {
            text("Procurando impressoras…").size(13).color(muted).into()
        } else if p.impressoras.is_empty() {
            text(if cfg!(windows) { "Nenhuma impressora instalada. Instale uma em Configurações → Impressoras." }
                else { "Nenhuma impressora instalada (CUPS). Instale uma nas configurações do sistema." }).size(12.5).color(muted).into()
        } else {
            suspensa(p.impressoras.clone(), p.impressora.clone(), |i| Msg::Imp(Im::Impressora(i))).text_size(13).padding([8, 4])
                .width(Length::FILL).into()
        };
        let pronta = p.impressora.is_some().then(|| row![
            container(Space::new(8, 8)).style(|_| Style { background: Some(Background::Color(VERDE)), border: Border { radius: [4.0; 4], ..Border::default() }, ..Default::default() }),
            text("Pronta").size(12).color(VERDE),
        ].spacing(6).align_y(Align::Center));
        let impressora = container(row![icone("printer", 20.0, muted), escolha_impressora].push_maybe(pronta).spacing(10).align_y(Align::Center))
            .padding([0, 12]).height(44).align_y(Align::Center).style(caixa);

        // Cópias: − número + e o agrupar.
        let passo = |rotulo: &'static str, msg: Option<Msg>| button(text(rotulo).size(16).center()).width(44).height(42)
            .style(|_, s| Style { background: Some(Background::Color(if s == Status::Hovered { tema::COR_HOVER() } else { tema::COR_PAINEL_FAIXA() })),
                text_color: Some(tema::COR_TEXTO()), ..Default::default() }).on_press_maybe(msg);
        let copias = container(row![
            passo("−", (p.copias > 1).then_some(Msg::Imp(Im::Copias(-1)))),
            text(p.copias.to_string()).size(14).width(48).center(),
            passo("+", Some(Msg::Imp(Im::Copias(1)))),
        ].align_y(Align::Center)).height(44).style(caixa);
        let copias = column![
            secao("Cópias"),
            row![copias, checkbox("Agrupar cópias (1, 2, 3… 1, 2, 3)", p.agrupar).on_toggle_maybe((p.copias > 1).then_some(|v| Msg::Imp(Im::Agrupar(v)))).size(15).text_size(13)]
                .spacing(16).align_y(Align::Center),
        ].spacing(6);

        // Páginas: controle segmentado, intervalo e o que vai sair.
        let segmento = |rotulo: String, v: Paginas| button(text(rotulo).size(13).center().font(if p.paginas == v { NEGRITO } else { Font::DEFAULT }))
            .width(Length::FILL).height(36).style(move |_, s| Style {
                background: Some(Background::Color(if p.paginas == v { tema::COR_AZUL_FUNDO() } else if s == Status::Hovered { tema::COR_HOVER() } else { tema::COR_PAINEL() })),
                text_color: Some(if p.paginas == v { tema::COR_AZUL_ACENTO() } else { tema::COR_TEXTO() }), ..Default::default()
            }).on_press(Msg::Imp(Im::Paginas(v)));
        let segmentos = container(row![
            segmento(format!("Todas ({total})"), Paginas::Todas), segmento("Atual".into(), Paginas::Atual), segmento("Intervalo".into(), Paginas::Intervalo),
            segmento("Ímpares".into(), Paginas::Impares), segmento("Pares".into(), Paginas::Pares),
        ]).padding(1).style(caixa).clip(true);
        let ruim = p.paginas == Paginas::Intervalo && escolhidas.is_none() && !p.intervalo.trim().is_empty();
        let intervalo = text_input("Intervalo, ex.: 1-3, 7", &p.intervalo).on_input(|t| Msg::Imp(Im::Intervalo(t))).padding([8, 12]).size(13)
            .style(move |t, s| { let mut e = campo_padrao(t, s);
                e.border = Border { color: if ruim { tema::COR_ERRO() } else if p.paginas == Paginas::Intervalo { tema::COR_AZUL_ACENTO() } else { tema::COR_BORDA() },
                    width: if p.paginas == Paginas::Intervalo { 2.0 } else { 1.0 }, radius: [8.0; 4] }; e });
        let resumo_paginas = if ruim { text(format!("Use números de 1 a {total}, com hífen e vírgula.")).size(12).color(tema::COR_ERRO()) }
            else { text(descrever(&lista)).size(12).color(muted) };
        let paginas = column![secao("Páginas"), segmentos, intervalo, resumo_paginas].spacing(8);

        // Ajustes em grade: rótulo à esquerda, escolhas em botões.
        let ajuste = |rotulo: &'static str, conteudo: Element<'static, Msg>| row![text(rotulo).size(13).color(muted).width(130), conteudo].align_y(Align::Center);
        let lados = suspensa([Lados::Um, Lados::BordaLonga, Lados::BordaCurta], Some(p.lados), |v| Msg::Imp(Im::Lados(v)))
            .text_size(13).padding([8, 10]).width(Length::FILL)
            ;
        let ajustes = column![
            ajuste("Frente e verso", lados.into()),
            ajuste("Orientação", row![escolha("Automática".into(), 0.0, p.orientacao == Orientacao::Automatica, Im::Orientacao(Orientacao::Automatica)),
                escolha("Retrato".into(), 0.0, p.orientacao == Orientacao::Retrato, Im::Orientacao(Orientacao::Retrato)),
                escolha("Paisagem".into(), 0.0, p.orientacao == Orientacao::Paisagem, Im::Orientacao(Orientacao::Paisagem))].spacing(6).into()),
            ajuste("Papel", suspensa(Papel::TODOS, Some(p.papel), |v| Msg::Imp(Im::Papel(v))).text_size(13).padding([8, 10]).width(Length::FILL).into()),
            ajuste("Páginas por folha", [1u32, 2, 4, 6, 9].into_iter().fold(row![].spacing(6), |r, n| r.push(escolha(n.to_string(), 44.0, p.por_folha == n, Im::PorFolha(n)))).into()),
            ajuste("Tamanho", row![escolha("Ajustar à folha".into(), 0.0, p.ajustar, Im::Ajustar(true)), escolha("Tamanho real".into(), 0.0, !p.ajustar, Im::Ajustar(false))].spacing(6).into()),
            ajuste("Comentários", row![escolha("Imprimir".into(), 0.0, p.comentarios, Im::Comentarios(true)),
                escolha("Só o documento".into(), 0.0, !p.comentarios, Im::Comentarios(false))].spacing(6).into()),
            ajuste("Cor", row![escolha("Colorido".into(), 0.0, !p.cinza, Im::Cinza(false)), escolha("Preto e branco".into(), 0.0, p.cinza, Im::Cinza(true))].spacing(6).into()),
        ].spacing(12);
        let mut form = column![
            column![secao("Impressora"), impressora].spacing(6), copias, paginas, ajustes,
            checkbox("Ordem inversa (a última sai primeiro)", p.inversa).on_toggle(|v| Msg::Imp(Im::Inversa(v))).size(15).text_size(13),
        ].spacing(16);
        if let Some(Err(t)) = &p.resultado { form = form.push(dlg::aviso("alert-triangle", t.clone())); }

        // Prévia: a página como vai sair, sobre a mesa cinza.
        let pagina = lista.get(p.previa.min(lista.len().saturating_sub(1))).copied();
        let deitada = match p.orientacao {
            Orientacao::Paisagem => true,
            Orientacao::Retrato => false,
            Orientacao::Automatica => pagina.and_then(|n| self.aberto.as_ref()?.tamanhos.get(n as usize - 1).copied()).is_some_and(|(l, a)| l > a),
        };
        let papel: Element<'_, Msg> = match p.imagem.as_ref().filter(|i| Some(i.0) == pagina && i.1 == p.cinza) {
            Some((_, _, h)) => image(h.clone()).content_fit(ContentFit::Contain).into(),
            None => container(text(pagina.map_or("Nenhuma página".into(), |n| format!("Página {n}"))).size(13).color(muted)).width(Length::FILL).height(Length::FILL).center().into(),
        };
        let seta = |rotulo: &'static str, msg: Option<Msg>| button(text(rotulo).size(16).center()).width(36).height(36)
            .style(|t, s| { let mut e = tema::estilo_botao_secundario(t, s); e.border.radius = [8.0; 4]; e }).on_press_maybe(msg);
        let previa = container(column![
            text(match pagina { Some(n) => format!("Página {n} · como vai sair"), None => String::new() }).size(12).color(muted),
            container(papel).width(if deitada { 424 } else { 300 }).height(if deitada { 300 } else { 424 }).padding(14).style(|_| Style {
                background: Some(Background::Color(Color::WHITE)),
                shadow: Some(Shadow { color: Color { a: 0.2, ..Color::BLACK }, offset: Point::new(0.0, 2.0), blur: 8.0 }), ..Default::default()
            }),
            row![
                seta("‹", (p.previa > 0).then_some(Msg::Imp(Im::Previa(-1)))),
                text(if lista.is_empty() { String::new() } else { format!("{} / {}", p.previa.min(lista.len() - 1) + 1, lista.len()) }).size(13).width(60).center(),
                seta("›", (p.previa + 1 < lista.len()).then_some(Msg::Imp(Im::Previa(1)))),
            ].spacing(8).align_y(Align::Center),
        ].spacing(14).align_x(Align::Center)).width(Length::FILL).height(Length::FILL).center().padding(18)
            .style(|_| Style { background: Some(Background::Color(tema::COR_MESA())), ..Default::default() });

        let resumo = match (&p.resultado, p.enviando) {
            (_, true) => "Enviando…".to_string(),
            (Some(Ok(t)), _) => t.clone(),
            _ if lista.is_empty() => String::new(),
            _ => format!("{} → {} · {} no total{}", plural(lista.len() as u32, "página", "páginas"), plural(folhas, "folha por cópia", "folhas por cópia"),
                plural(papeis * p.copias, "papel", "papéis"), if p.lados == Lados::Um { "" } else { ", frente e verso" }),
        };
        let pode = !p.enviando && p.impressora.is_some() && escolhidas.is_some();
        let rodape = container(row![
            text(resumo).size(13).color(muted), Space::with_width(Length::FILL),
            dlg::botao_secundario("Cancelar", Some(Msg::FecharJanela(id))),
            dlg::botao_primario(if p.copias > 1 { format!("Imprimir {} cópias", p.copias) } else { "Imprimir".into() }, pode.then_some(Msg::Imp(Im::Imprimir))),
        ].spacing(8).align_y(Align::Center)).padding([12, 18]).style(|_| Style {
            background: Some(Background::Color(tema::COR_PAINEL_FAIXA())),
            border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [0.0; 4] }, ..Default::default()
        });
        let corpo = column![
            row![scrollable(container(form).padding([18, 22])).width(470), previa].height(Length::FILL),
            rodape,
        ];
        cromo::janela_com_barra("Imprimir", corpo.into(), Self::acoes_janela(id))
    }
}

const VERDE: Color = Color::rgb(0x28 as f32 / 255.0, 0x78 as f32 / 255.0, 0x46 as f32 / 255.0);
const NEGRITO: Font = Font::SEMIBOLD;

impl std::fmt::Display for Lados {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self { Lados::Um => "Só a frente", Lados::BordaLonga => "Virar na borda longa", Lados::BordaCurta => "Virar na borda curta" })
    }
}

fn plural(n: u32, um: &str, varios: &str) -> String { format!("{n} {}", if n == 1 { um } else { varios }) }

/// "4 páginas: 1, 2, 3 e 7" (as primeiras, para não virar um parágrafo).
fn descrever(lista: &[u32]) -> String {
    match lista {
        [] => String::new(),
        [n] => format!("1 página: {n}"),
        _ if lista.len() > 8 => format!("{} páginas", lista.len()),
        [inicio @ .., ultima] => format!("{} páginas: {} e {ultima}", lista.len(), inicio.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")),
    }
}

fn secao(texto: &str) -> Element<'static, Msg> {
    text(texto.to_uppercase()).size(11).font(NEGRITO).color(tema::COR_TEXTO_MUTED()).into()
}

fn icone(nome: &str, tamanho: f32, cor: Color) -> Element<'static, Msg> {
    feather_ui::icones::icone_traco(nome, tamanho).color(cor).into()
}

fn caixa(_: &Theme) -> Style {
    Style { background: Some(Background::Color(tema::COR_PAINEL())), border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [8.0; 4] }, ..Default::default() }
}

/// Botão de escolha: a ativa com borda e texto na cor de destaque, como nos modelos.
fn escolha(rotulo: String, largura: f32, ativo: bool, msg: Im) -> Element<'static, Msg> {
    let b = button(text(rotulo).size(13).center().font(if ativo { NEGRITO } else { Font::DEFAULT })).height(36).padding([0, 12])
        .style(move |_, s| Style {
            background: Some(Background::Color(if ativo { tema::COR_AZUL_FUNDO() } else if s == Status::Hovered { tema::COR_HOVER() } else { tema::COR_PAINEL() })),
            text_color: Some(if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_TEXTO() }),
            border: Border { color: if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_BORDA() }, width: if ativo { 2.0 } else { 1.0 }, radius: [8.0; 4] },
            ..Default::default()
        }).on_press(Msg::Imp(msg));
    if largura > 0.0 { b.width(largura).padding(0).into() } else { b.into() }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn intervalo_e_argumentos_do_lp() {
        assert_eq!(ler_intervalo("1-3, 5", 10), Some(vec![1, 2, 3, 5]));
        assert_eq!(ler_intervalo("3-1", 10), None);
        assert_eq!(ler_intervalo("0", 10), None);
        assert_eq!(ler_intervalo("11", 10), None);
        assert_eq!(ler_intervalo("a", 10), None);
        let i = Impressao { paginas: Paginas::Intervalo, intervalo: "1-3; 5".into(), lados: Lados::BordaLonga, cinza: true, por_folha: 2, copias: 2, ..Default::default() };
        assert!(Impressao { papel: Papel::Oficio, ..Default::default() }.argumentos("HP", 1).join(" ").contains("-o media=Legal"));
        assert!(!i.argumentos("HP", 1).iter().any(|a| a.starts_with("media=")), "sem escolha, o papel é o da impressora");
        let a = i.argumentos("HP", 1).join(" ");
        assert_eq!(a, "-d HP -n 2 -t FeatherPDF -o page-ranges=1-3,5 -o sides=two-sided-long-edge -o print-color-mode=monochrome -o fit-to-page -o number-up=2 -o collate=true");
        assert_eq!(Impressao { paginas: Paginas::Pares, ..Default::default() }.escolhidas(1, 1), None);
    }
}
