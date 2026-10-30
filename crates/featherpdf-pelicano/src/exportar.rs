//! Janela Converter e exportar: o PDF aberto para o FeatherSheet ou o Excel (as tabelas
//! achadas, com o tipo de cada coluna, ou a página inteira como está), o FeatherDocs, o
//! Word, imagens (uma por página), texto puro ou HTML. O arquivo aberto não muda.

use std::path::PathBuf;

use feather_ui::dialogo as dlg;
use feather_ui::{cromo, icones, tema};
use featherpdf_core::{ler_valor, TabelaPdf, TipoColuna};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Auxiliar, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formato { Sheet, Docs, Excel, Word, Slides, Imagens, Texto, PdfA }

impl Formato {
    fn nome(self) -> &'static str {
        match self {
            Formato::Sheet => "FeatherSheet", Formato::Docs => "FeatherDocs", Formato::Excel => "Excel (.xlsx) · CSV",
            Formato::Word => "Word (.docx) · ODT", Formato::Slides => "Apresentação (.pptx)", Formato::Imagens => "Imagens (PNG, JPG, WebP, TIFF, SVG)", Formato::Texto => "Texto · HTML · Markdown", Formato::PdfA => "PDF/A (arquivamento)",
        }
    }
    /// Cor da bolinha dos apps da suíte; ícone dos outros.
    fn marca(self) -> Element<'static, Msg> {
        let cor = match self { Formato::Sheet => Some((0x2e, 0x7d, 0x4f)), Formato::Docs => Some((0x23, 0x6a, 0xc7)), _ => None };
        match cor {
            Some((r, g, b)) => container(Space::new(10, 10)).style(move |_| Style {
                background: Some(Background::Color(Color::rgb8(r, g, b))), border: Border { radius: [2.0; 4], ..Border::default() }, ..Default::default()
            }).into(),
            None => icones::icone(match self { Formato::Excel => "table", Formato::Word => "file-code", Formato::Slides => "layout-columns", Formato::Imagens => "photo", Formato::PdfA => "lock", _ => "list" }, 15.).into(),
        }
    }
    fn planilha(self) -> bool { matches!(self, Formato::Sheet | Formato::Excel) }
}

/// Como as tabelas vão para a planilha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoPlanilha { PorTabela, Tudo, Pagina }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoImagem { Png, Jpg, Webp, Tiff, Svg }

#[derive(Debug, Clone)]
pub enum Ex {
    Abrir(Formato),
    Formato(Formato),
    Imagem(TipoImagem),
    Dpi(u32),
    SoAtual(bool),
    Html(bool),
    Markdown(bool),
    Odt(bool),
    Tabelas(u64, Result<Vec<TabelaPdf>, String>),
    Escolher(usize),
    Tipo(usize, usize, TipoColuna),
    Modo(ModoPlanilha),
    Juntar(bool),
    OcrTabelas(bool),
    AbrirSheet,
    Exportar,
    /// Id do documento que pediu (`Aberto::id`) e o destino.
    Destino(u64, Option<PathBuf>),
    Pronto(Result<String, String>),
}

#[derive(Debug, Clone)]
pub struct Exportar {
    pub formato: Formato,
    imagem: TipoImagem,
    dpi: u32,
    so_atual: bool,
    html: bool,
    markdown: bool,
    odt: bool,
    /// Verificação do PDF/A, refeita ao escolher o formato.
    pdfa: Vec<featherpdf_core::ItemPdfa>,
    pub(crate) trabalhando: bool,
    resultado: Option<Result<String, String>>,
    /// Tabelas sem juntar (como saíram de cada página) e o que a pessoa vê, já juntadas e tipadas.
    brutas: Option<Result<Vec<TabelaPdf>, String>>,
    tabelas: Vec<TabelaPdf>,
    escolhida: usize,
    modo: ModoPlanilha,
    juntar: bool,
    ocr_tabelas: bool,
    /// Muda a cada leitura: resultado antigo é jogado fora.
    leitura: u64,
}

impl Default for Exportar {
    fn default() -> Self {
        Self { formato: Formato::Sheet, imagem: TipoImagem::Png, dpi: 150, so_atual: false, html: false, markdown: false, odt: false, pdfa: vec![], trabalhando: false, resultado: None,
            brutas: None, tabelas: vec![], escolhida: 0, modo: ModoPlanilha::PorTabela, juntar: true, ocr_tabelas: false, leitura: 0 }
    }
}

impl Exportar {
    /// Documento trocado: as tabelas lidas eram do outro.
    pub fn esquecer(&mut self) { self.brutas = None; self.tabelas.clear(); self.leitura += 1; }

    fn refazer(&mut self) {
        let brutas = match &self.brutas { Some(Ok(v)) => v.clone(), _ => vec![] };
        self.tabelas = if self.juntar { featherpdf_core::juntar_continuacoes(brutas) } else { brutas };
        self.escolhida = self.escolhida.min(self.tabelas.len().saturating_sub(1));
    }

    fn continuacoes(&self) -> Vec<String> {
        self.tabelas.iter().filter(|t| t.ate_pagina > t.pagina).map(|t| format!("p. {}–{}", t.pagina, t.ate_pagina)).collect()
    }
}

/// As células que não se leem no tipo da coluna: (endereço, texto).
fn ruins(t: &TabelaPdf) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for (l, linha) in t.linhas.iter().enumerate() {
        for (c, x) in linha.iter().enumerate() {
            let tipo = t.tipos.get(c).copied().unwrap_or(TipoColuna::Texto);
            if !x.trim().is_empty() && ler_valor(x, tipo).is_none() { v.push((format!("{}{}", letra(c), l + 2), x.clone())); }
        }
    }
    v
}

fn letra(c: usize) -> String {
    let mut n = c + 1;
    let mut s = String::new();
    while n > 0 { s.insert(0, (b'A' + ((n - 1) % 26) as u8) as char); n = (n - 1) / 26; }
    s
}

impl App {
    pub fn update_exportar(&mut self, m: Ex) -> Task<Msg> {
        if matches!(m, Ex::Exportar) && self.exportar.formato == Formato::Docs {
            self.exportar.resultado = Some(self.para_featherdocs());
            return Task::none();
        }
        if let Ex::Abrir(Formato::PdfA) | Ex::Formato(Formato::PdfA) = m {
            self.exportar.pdfa = self.aberto.as_ref().map(|a| a.pdf.verificar_pdfa()).unwrap_or_default();
        }
        let (base, total, atual, doc) = (self.base_nome(), self.total_paginas(), self.pagina, self.id_aberto().unwrap_or(0));
        let e = &mut self.exportar;
        match m {
            Ex::Abrir(f) => {
                e.formato = f;
                e.resultado = None;
                let ler = self.ler_tabelas();
                if self.janelas.aberta(Auxiliar::Converter) { return ler; }
                return Task::batch([self.alternar_auxiliar(Auxiliar::Converter), ler]);
            }
            Ex::Formato(f) => { e.formato = f; e.resultado = None; return self.ler_tabelas(); }
            Ex::Imagem(v) => e.imagem = v,
            Ex::Dpi(d) => e.dpi = d,
            Ex::SoAtual(v) => e.so_atual = v,
            Ex::Html(v) => { e.html = v; e.markdown = false; }
            Ex::Markdown(v) => { e.markdown = v; e.html = false; }
            Ex::Odt(v) => e.odt = v,
            Ex::Tabelas(leitura, r) => if leitura == e.leitura {
                e.trabalhando = false;
                e.brutas = Some(r);
                e.refazer();
                if e.tabelas.is_empty() && matches!(e.brutas, Some(Ok(_))) { e.modo = ModoPlanilha::Pagina; }
            },
            Ex::Escolher(i) => e.escolhida = i,
            Ex::Tipo(t, c, tipo) => if let Some(x) = e.tabelas.get_mut(t).and_then(|t| t.tipos.get_mut(c)) { *x = tipo },
            Ex::Modo(m) => e.modo = m,
            Ex::Juntar(v) => { e.juntar = v; e.refazer(); }
            Ex::OcrTabelas(v) => { e.ocr_tabelas = v; e.brutas = None; return self.ler_tabelas(); }
            Ex::AbrirSheet => {
                let destino = std::env::temp_dir().join("featherpdf").join(format!("{base} — tabelas.xlsx"));
                return self.update_exportar(Ex::Destino(doc, Some(destino)));
            }
            Ex::Exportar => {
                let pedir = match e.formato {
                    // TIFF: um arquivo só com todas as páginas (o que arquivo e fax esperam).
                    Formato::Imagens if e.imagem == TipoImagem::Tiff => rfd::AsyncFileDialog::new().add_filter("TIFF", &["tif", "tiff"]).set_file_name(format!("{base}.tif")),
                    Formato::Imagens => rfd::AsyncFileDialog::new().set_title("Pasta para as imagens"),
                    Formato::PdfA => rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{base} (PDF-A).pdf")),
                    Formato::Slides => rfd::AsyncFileDialog::new().add_filter("PowerPoint", &["pptx"]).set_file_name(format!("{base}.pptx")),
                    Formato::Word if e.odt => rfd::AsyncFileDialog::new().add_filter("OpenDocument", &["odt"]).set_file_name(format!("{base}.odt")),
                    Formato::Word => rfd::AsyncFileDialog::new().add_filter("Word", &["docx"]).set_file_name(format!("{base}.docx")),
                    Formato::Sheet | Formato::Excel if e.modo != ModoPlanilha::Pagina => rfd::AsyncFileDialog::new()
                        .add_filter("Excel", &["xlsx"]).add_filter("CSV (separado por ;)", &["csv"]).set_file_name(format!("{base}.xlsx")),
                    Formato::Sheet | Formato::Excel => rfd::AsyncFileDialog::new().add_filter("Excel", &["xlsx"]).set_file_name(format!("{base}.xlsx")),
                    _ if e.markdown => rfd::AsyncFileDialog::new().add_filter("Markdown", &["md"]).set_file_name(format!("{base}.md")),
                    _ if e.html => rfd::AsyncFileDialog::new().add_filter("HTML", &["html"]).set_file_name(format!("{base}.html")),
                    _ => rfd::AsyncFileDialog::new().add_filter("Texto", &["txt"]).set_file_name(format!("{base}.txt")),
                };
                let pasta = e.formato == Formato::Imagens && e.imagem != TipoImagem::Tiff;
                return Task::perform(async move {
                    if pasta { pedir.pick_folder().await } else { pedir.save_file().await }.map(|h| h.path().to_path_buf())
                }, move |d| Msg::Exp(Ex::Destino(doc, d)));
            }
            Ex::Destino(_, None) => {}
            // Outro documento veio para a frente com o diálogo aberto: não exporta o errado.
            Ex::Destino(pedido, Some(_)) if pedido != doc => self.status = "O documento mudou: nada foi exportado. Exporte de novo.".into(),
            Ex::Destino(_, Some(destino)) => {
                let Some(a) = &self.aberto else { return Task::none() };
                let (pdf, vis) = (a.pdf.clone(), a.vis.clone());
                let e = &mut self.exportar;
                let paginas: Vec<u32> = if e.so_atual { vec![atual] } else { (1..=total).collect() };
                let (formato, tipo, dpi, html, markdown, odt, modo) = (e.formato, e.imagem, e.dpi, e.html, e.markdown, e.odt, e.modo);
                let tabelas = e.tabelas.clone();
                // "Abrir no FeatherSheet" grava num temporário e abre lá; o resto vai para onde a pessoa escolheu.
                let no_sheet = destino.starts_with(std::env::temp_dir().join("featherpdf"));
                e.trabalhando = true;
                e.resultado = None;
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || -> Result<String, String> {
                        let salvo = format!("Salvo em {}", destino.display());
                        match formato {
                            Formato::PdfA => {
                                let mut pdf = pdf;
                                pdf.converter_pdfa(featherpdf_io::PERFIL_SRGB)?;
                                featherpdf_io::gravar_pdf(&destino, &pdf.serializar()?).map(|_| salvo)
                            }
                            Formato::Slides => {
                                // ~150 dpi: nítido no projetor sem a apresentação pesar demais.
                                let tamanhos = vis.tamanhos();
                                featherpdf_io::gravar_pptx(&destino, &paginas, |n| vis.desenhar(n, 150.0 / 72.0).map(|r| (r.largura, r.altura, r.rgba, tamanhos[n as usize - 1])))?;
                                Ok(format!("{} slide(s) em {}", paginas.len(), destino.display()))
                            }
                            Formato::Word => {
                                let fluxo = pdf.paginas_em_fluxo()?;
                                if odt { featherpdf_io::gravar_odt(&pdf, &fluxo, &destino) } else { featherpdf_io::gravar_docx(&pdf, &fluxo, &destino) }.map(|_| salvo)
                            }
                            Formato::Sheet | Formato::Excel => {
                                if let Some(p) = destino.parent() { std::fs::create_dir_all(p).map_err(|e| e.to_string())?; }
                                if modo == ModoPlanilha::Pagina {
                                    let grade = pdf.paginas_em_grade()?;
                                    if grade.is_empty() { return Err("Este PDF não tem texto para a planilha (é imagem? marque “Ler tabela de página escaneada com OCR”).".into()); }
                                    featherpdf_io::gravar_xlsx(&pdf, &grade, &destino)?;
                                } else if destino.extension().is_some_and(|x| x.eq_ignore_ascii_case("csv")) {
                                    featherpdf_io::gravar_atomico(&destino, featherpdf_io::tabelas_csv(&tabelas).as_bytes())?;
                                } else {
                                    featherpdf_io::gravar_tabelas_xlsx(&tabelas, modo == ModoPlanilha::Tudo, &destino)?;
                                }
                                if no_sheet { crate::abrir_no("feathersheet", "FeatherSheet", &destino).map(|_| "Aberto no FeatherSheet.".to_string()) } else { Ok(salvo) }
                            }
                            Formato::Texto if markdown => {
                                let mut fluxo = pdf.paginas_em_fluxo()?;
                                fluxo.retain(|p| paginas.contains(&p.pagina));
                                featherpdf_io::gravar_atomico(&destino, featherpdf_core::markdown(&fluxo).as_bytes())?;
                                Ok(salvo)
                            }
                            Formato::Texto => {
                                let textos: Vec<(u32, String)> = paginas.iter().map(|&n| pdf.texto_pagina(n).map(|t| (n, t.trim_end().to_string()))).collect::<Result<_, _>>()?;
                                let conteudo = if html { html_das_paginas(&textos) } else {
                                    textos.iter().map(|(n, t)| format!("--- Página {n} ---\n{t}")).collect::<Vec<_>>().join("\n\n")
                                };
                                featherpdf_io::gravar_atomico(&destino, conteudo.as_bytes())?;
                                Ok(salvo)
                            }
                            _ if tipo == TipoImagem::Tiff => {
                                featherpdf_io::gravar_tiff(&destino, &paginas, dpi, |n| vis.desenhar(n, dpi as f32 / 72.0).map(|r| (r.largura, r.altura, r.rgba)))?;
                                Ok(format!("{} página(s) em {}", paginas.len(), destino.display()))
                            }
                            _ => {
                                let extensao = match tipo { TipoImagem::Jpg => "jpg", TipoImagem::Webp => "webp", TipoImagem::Svg => "svg", _ => "png" };
                                let arquivo = |n: u32| destino.join(format!("{base} - página {n}.{extensao}"));
                                // A pasta foi escolhida sem diálogo de "substituir?": não apaga uma exportação anterior.
                                if let Some(existe) = paginas.iter().map(|&n| arquivo(n)).find(|a| a.exists()) {
                                    return Err(format!("{} já existe: escolha outra pasta ou apague as imagens antigas.", existe.display()));
                                }
                                for &n in &paginas {
                                    let arquivo = arquivo(n);
                                    if tipo == TipoImagem::Svg {
                                        featherpdf_io::gravar_atomico(&arquivo, vis.svg(n)?.as_bytes()).map_err(|e| format!("{}: {e}", arquivo.display()))?;
                                        continue;
                                    }
                                    let r = vis.desenhar(n, dpi as f32 / 72.0)?;
                                    if tipo == TipoImagem::Webp { featherpdf_io::gravar_webp(&arquivo, r.largura, r.altura, r.rgba)?; continue; }
                                    let img = image::RgbaImage::from_raw(r.largura, r.altura, r.rgba).ok_or("Página com tamanho incoerente.")?;
                                    let mut bytes = std::io::Cursor::new(Vec::new());
                                    let gravado = if tipo == TipoImagem::Jpg { image::DynamicImage::ImageRgba8(img).to_rgb8().write_to(&mut bytes, image::ImageFormat::Jpeg) }
                                        else { img.write_to(&mut bytes, image::ImageFormat::Png) };
                                    gravado.map_err(|e| e.to_string()).and_then(|_| featherpdf_io::gravar_atomico(&arquivo, bytes.get_ref()))
                                        .map_err(|e| format!("{}: {e}", arquivo.display()))?;
                                }
                                Ok(format!("{} imagem(ns) em {}", paginas.len(), destino.display()))
                            }
                        }
                    }).await.unwrap_or_else(|e| Err(format!("Falha ao exportar: {e}")))
                }, |r| Msg::Exp(Ex::Pronto(r)));
            }
            Ex::Pronto(r) => {
                let e = &mut self.exportar;
                e.trabalhando = false;
                if let Ok(t) = &r { self.status = t.clone(); }
                e.resultado = Some(r);
            }
        }
        Task::none()
    }

    /// Lê as tabelas (uma vez por documento) quando a escolha é planilha; com o OCR ligado,
    /// as páginas sem texto são lidas pelo OCR antes.
    pub(crate) fn ler_tabelas(&mut self) -> Task<Msg> {
        let e = &mut self.exportar;
        if !e.formato.planilha() || e.brutas.is_some() { return Task::none(); }
        let Some(a) = &self.aberto else { return Task::none() };
        let (pdf, vis, ocr) = (a.pdf.clone(), a.vis.clone(), e.ocr_tabelas);
        e.leitura += 1;
        let leitura = e.leitura;
        e.brutas = Some(Ok(vec![]));
        e.tabelas.clear();
        e.trabalhando = true;
        Task::perform(async move {
            tokio::task::spawn_blocking(move || -> Result<Vec<TabelaPdf>, String> {
                let mut v = pdf.tabelas(false)?;
                if ocr {
                    for p in 1..=pdf.total_paginas() {
                        if pdf.texto_pagina(p).is_ok_and(|t| t.trim().chars().count() > crate::ocr::MINIMO_TEXTO) { continue; }
                        let (pagina_pt, giro) = pdf.geometria(p)?;
                        let (l, h) = (pagina_pt[2] - pagina_pt[0], pagina_pt[3] - pagina_pt[1]);
                        let (linhas, _) = crate::ocr::ler(&vis, p, (crate::ocr::LADO / l.max(h)).clamp(1.0, 4.0), 0, false)?;
                        let palavras: Vec<(String, [f32; 4])> = linhas.iter().flat_map(|x| &x.palavras)
                            .map(|w| (w.texto.clone(), crate::ocr::em_pontos(w.caixa, pagina_pt, giro))).collect();
                        v.extend(featherpdf_core::tabelas_da_pagina(p, &palavras));
                    }
                    v.sort_by_key(|t| t.pagina);
                }
                Ok(v)
            }).await.unwrap_or_else(|e| Err(e.to_string()))
        }, move |r| Msg::Exp(Ex::Tabelas(leitura, r)))
    }

    fn base_nome(&self) -> String {
        let nome = self.nome_documento().unwrap_or_default();
        std::path::Path::new(&nome).file_stem().map_or("documento".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn view_exportar(&self, id: WindowId) -> Element<'_, Msg> {
        let e = &self.exportar;
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;
        let item = |f: Formato| button(row![f.marca(), text(f.nome()).size(13)].spacing(10).align_y(Align::Center))
            .width(Length::FILL).padding([7, 10]).style(tema::estilo_botao_pequeno(e.formato == f)).on_press(Msg::Exp(Ex::Formato(f)));
        let titulo = |t: &'static str| container(text(t).size(10.5).font(negrito).color(muted)).padding([8, 4]);
        let menu = container(column![
            titulo("PARA A SUÍTE"), item(Formato::Sheet), item(Formato::Docs),
            titulo("OUTROS"), item(Formato::Excel), item(Formato::Word), item(Formato::Slides), item(Formato::Imagens), item(Formato::Texto), item(Formato::PdfA),
        ].spacing(2)).width(210).height(Length::FILL).padding(12);
        let opcao = |rotulo: &'static str, ativo: bool, msg: Ex| button(text(rotulo).size(12.5)).padding([5, 10])
            .style(tema::estilo_botao_pequeno(ativo)).on_press(Msg::Exp(msg));
        let paginas = row![text("Páginas").size(13).width(90),
            opcao("Todas", !e.so_atual, Ex::SoAtual(false)), opcao("Só a atual", e.so_atual, Ex::SoAtual(true))].spacing(6).align_y(Align::Center);
        let (mut corpo, botao): (Column<'_, Msg>, &str) = match e.formato {
            Formato::Sheet | Formato::Excel => (self.corpo_planilha(), if e.formato == Formato::Sheet { "Abrir no FeatherSheet" } else { "Salvar como…" }),
            Formato::Docs => (column![text("Texto corrido com títulos, parágrafos e imagens, aberto direto no FeatherDocs para editar.").size(13).color(muted)], "Abrir no FeatherDocs"),
            Formato::Word => (column![
                text("O mesmo texto corrido do FeatherDocs, com títulos, tabelas e imagens, para editar em outro programa.").size(13).color(muted),
                row![text("Formato").size(13).width(90), opcao("Word (.docx)", !e.odt, Ex::Odt(false)), opcao("LibreOffice (.odt)", e.odt, Ex::Odt(true))].spacing(6).align_y(Align::Center),
            ].spacing(10), "Salvar como…"),
            Formato::Slides => (column![
                text("Cada página vira um slide, no tamanho da primeira página, para apresentar no PowerPoint, no Impress ou no FeatherSlide.").size(13).color(muted),
                paginas,
            ].spacing(10), "Salvar como…"),
            Formato::Imagens => (column![
                text(match e.imagem {
                    TipoImagem::Tiff => "Um arquivo .tif com todas as páginas, na resolução escolhida (o que arquivo digital e fax pedem).",
                    TipoImagem::Svg => "Uma página por arquivo .svg, numa pasta: texto e desenhos continuam vetoriais, com nitidez em qualquer tamanho.",
                    _ => "Uma imagem por página, na resolução escolhida, numa pasta.",
                }).size(13).color(muted),
                row![text("Formato").size(13).width(90), opcao("PNG (nítido)", e.imagem == TipoImagem::Png, Ex::Imagem(TipoImagem::Png)),
                    opcao("JPG (menor)", e.imagem == TipoImagem::Jpg, Ex::Imagem(TipoImagem::Jpg)),
                    opcao("WebP (sem perda)", e.imagem == TipoImagem::Webp, Ex::Imagem(TipoImagem::Webp)),
                    opcao("TIFF (várias páginas)", e.imagem == TipoImagem::Tiff, Ex::Imagem(TipoImagem::Tiff)),
                    opcao("SVG (vetorial)", e.imagem == TipoImagem::Svg, Ex::Imagem(TipoImagem::Svg))].spacing(6).align_y(Align::Center),
            ].spacing(10).push_maybe((e.imagem != TipoImagem::Svg).then(|| row![text("Resolução").size(13).width(90), opcao("72 dpi (tela)", e.dpi == 72, Ex::Dpi(72)), opcao("150 dpi", e.dpi == 150, Ex::Dpi(150)), opcao("300 dpi (impressão)", e.dpi == 300, Ex::Dpi(300))].spacing(6).align_y(Align::Center)))
                .push(paginas), if e.imagem == TipoImagem::Tiff { "Salvar como…" } else { "Escolher pasta…" }),
            Formato::PdfA => {
                use featherpdf_core::SituacaoPdfa as S;
                let cores = |s: S| match s {
                    S::Ok => ("circle-check", Color::rgb8(0x28, 0x78, 0x46), Color::rgb8(0x1e, 0x5c, 0x35), Color::rgb8(0xe6, 0xf1, 0xea)),
                    S::Corrige => ("refresh", Color::rgb8(0xa3, 0x6a, 0x00), Color::rgb8(0x6b, 0x4e, 0x05), Color::rgb8(0xfb, 0xf3, 0xdc)),
                    S::Impede => ("alert-triangle", Color::rgb8(0xb3, 0x26, 0x1e), Color::rgb8(0x8f, 0x22, 0x19), Color::rgb8(0xfd, 0xec, 0xeb)),
                };
                let icone = |s: S| { let (n, c, ..) = cores(s); icones::icone_traco(n, 18.).color(c) };
                let conta = |s: S| e.pdfa.iter().filter(|i| i.situacao == s).count();
                let cartao = |s: S, rotulo: &'static str| { let (_, _, escura, fundo) = cores(s);
                    container(column![text(conta(s).to_string()).size(22).font(negrito).color(escura), text(rotulo).size(13).color(escura)])
                        .width(Length::FILL).padding([12, 14]).style(move |_| Style { background: Some(Background::Color(fundo)), border: Border { radius: [8.0; 4], ..Border::default() }, ..Default::default() }) };
                let separador = || container(Space::with_height(1)).width(Length::FILL).style(|_| Style { background: Some(Background::Color(tema::COR_BORDA_SUAVE())), ..Default::default() });
                let mut itens = column![];
                for i in e.pdfa.iter().filter(|i| i.situacao == S::Impede) {
                    let (_, _, escura, _) = cores(S::Impede);
                    let mut t = column![text(i.regra).size(13).font(negrito)].spacing(3);
                    if !i.detalhe.is_empty() { t = t.push(text(i.detalhe.clone()).size(13).color(escura)); }
                    itens = itens.push(container(row![icone(S::Impede), t].spacing(12)).width(Length::FILL).padding([12, 14])
                        .style(|_| Style { background: Some(Background::Color(Color::rgb8(0xff, 0xfa, 0xf9))), ..Default::default() })).push(separador());
                }
                for i in e.pdfa.iter().filter(|i| i.situacao == S::Corrige) {
                    let (_, _, escura, _) = cores(S::Corrige);
                    itens = itens.push(container(row![icone(S::Corrige), text(i.regra).size(13).width(Length::FILL), text(i.detalhe.clone()).size(12).color(escura)]
                        .spacing(12).align_y(Align::Center)).padding([10, 14])).push(separador());
                }
                let certos: Vec<&str> = e.pdfa.iter().filter(|i| i.situacao == S::Ok).map(|i| i.regra).collect();
                if !certos.is_empty() {
                    itens = itens.push(container(row![icone(S::Ok), text(certos.join(" · ")).size(13).color(muted)].spacing(12).align_y(Align::Center)).padding([10, 14]));
                }
                (column![
                    row![text("PDF/A-2b").size(18).font(negrito), text("o padrão de arquivamento que tribunais, órgãos públicos e arquivos pedem").size(13).color(muted)]
                        .spacing(10).align_y(Align::End),
                    row![cartao(S::Ok, "já estão certos"), cartao(S::Corrige, "o FeatherPDF corrige ao salvar"), cartao(S::Impede, "impede a conversão")].spacing(10),
                    container(itens).width(Length::FILL).clip(true).style(|_| Style {
                        border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [8.0; 4] }, ..Default::default() }),
                    text(format!("A cópia sai como “{} (PDF-A).pdf”; o arquivo aberto fica como está. Para entregar a quem confere com rigor, valide também no veraPDF.", self.base_nome())).size(12).color(muted),
                ].spacing(14), "Salvar cópia PDF/A")
            }
            Formato::Texto => (column![
                text(if e.markdown { "Texto corrido com títulos, negrito e tabelas, em Markdown. PDF escaneado precisa do OCR antes." }
                    else { "Só o texto, página por página. PDF escaneado precisa do OCR antes." }).size(13).color(muted),
                row![text("Formato").size(13).width(90), opcao("Texto (.txt)", !e.html && !e.markdown, Ex::Html(false)), opcao("Página web (.html)", e.html, Ex::Html(true)),
                    opcao("Markdown (.md)", e.markdown, Ex::Markdown(true))].spacing(6).align_y(Align::Center),
                paginas,
            ].spacing(10), "Salvar como…"),
        };
        if !e.formato.planilha() && e.formato != Formato::PdfA { corpo = column![text(e.formato.nome()).size(18), corpo].spacing(14); }
        if e.trabalhando { corpo = corpo.push(text(if e.formato.planilha() && e.tabelas.is_empty() { "Procurando tabelas…" } else { "Exportando…" }).size(13).color(tema::COR_AZUL_ACENTO())); }
        match &e.resultado {
            Some(Ok(t)) => corpo = corpo.push(dlg::aviso("circle-check", t.clone())),
            Some(Err(t)) => corpo = corpo.push(dlg::aviso("alert-triangle", t.clone())),
            None => {}
        }
        let pode = self.aberto.is_some() && !e.trabalhando && (!e.formato.planilha() || e.modo == ModoPlanilha::Pagina || !e.tabelas.is_empty())
            && (e.formato != Formato::PdfA || !e.pdfa.iter().any(|i| i.situacao == featherpdf_core::SituacaoPdfa::Impede));
        let resumo = if e.formato.planilha() && e.modo != ModoPlanilha::Pagina && !e.tabelas.is_empty() {
            let linhas: usize = e.tabelas.iter().map(|t| t.linhas.len()).sum();
            let abas = if e.modo == ModoPlanilha::Tudo { 1 } else { e.tabelas.len() };
            format!("{abas} aba{} · {linhas} linhas", if abas == 1 { "" } else { "s" })
        } else {
            self.nome_documento().map_or("Nenhum PDF aberto".into(), |n| format!("{n} · {} páginas", self.total_paginas()))
        };
        let impede = e.formato == Formato::PdfA && e.pdfa.iter().any(|i| i.situacao == featherpdf_core::SituacaoPdfa::Impede);
        let mut rodape = row![if impede { text("Resolva o item em vermelho para salvar").size(13).color(Color::rgb8(0x8f, 0x22, 0x19)) } else { text(resumo).size(12).color(muted) },
            Space::with_width(Length::FILL)].spacing(8).align_y(Align::Center);
        if e.formato == Formato::PdfA { rodape = rodape.push(dlg::botao_secundario("Verificar de novo", Some(Msg::Exp(Ex::Formato(Formato::PdfA))))); }
        if e.formato == Formato::Sheet {
            rodape = rodape.push(button(text("Salvar como…").size(13)).padding([6, 12]).style(tema::estilo_botao_pequeno(false))
                .on_press_maybe(pode.then_some(Msg::Exp(Ex::Exportar))));
        }
        rodape = rodape.push(dlg::botao_primario(botao, pode.then_some(Msg::Exp(if e.formato == Formato::Sheet { Ex::AbrirSheet } else { Ex::Exportar }))));
        let direita = column![
            container(scrollable(container(corpo).padding([20, 24])).height(Length::FILL)).height(Length::FILL),
            container(rodape).padding([12, 24]),
        ].width(Length::FILL);
        cromo::janela_com_barra("Converter e exportar", row![menu, crate::inicio::linha_vertical(), direita].into(), Self::acoes_janela(id))
    }

    /// FeatherSheet e Excel: as tabelas achadas, a prévia com o tipo de cada coluna e as opções.
    fn corpo_planilha(&self) -> Column<'_, Msg> {
        let e = &self.exportar;
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;
        let modo = |rotulo: &'static str, m: ModoPlanilha| radio(rotulo, m, Some(e.modo), |m| Msg::Exp(Ex::Modo(m))).size(15).text_size(12.5);
        let mut topo = row![text(format!("Tabelas encontradas: {}", e.tabelas.len())).size(13).font(negrito), Space::with_width(Length::FILL)].spacing(14).align_y(Align::Center);
        if !e.tabelas.is_empty() { topo = topo.push(modo("Uma aba por tabela", ModoPlanilha::PorTabela)).push(modo("Tudo numa aba", ModoPlanilha::Tudo)); }
        topo = topo.push(modo("A página inteira, como está", ModoPlanilha::Pagina));
        let mut c = column![topo].spacing(12);
        if e.modo == ModoPlanilha::Pagina {
            c = c.push(text("Cada página vira uma aba, com o mesmo desenho: caixas viram células mescladas com borda, e o resto do texto vai para as células.").size(12.5).color(muted));
        } else if e.tabelas.is_empty() {
            if matches!(e.brutas, Some(Ok(_))) && !e.trabalhando {
                c = c.push(dlg::aviso_vazio("Não achei tabelas (linhas com várias colunas alinhadas). Use “A página inteira, como está”."));
            }
        } else {
            let t = &e.tabelas[e.escolhida];
            let miniaturas = column(e.tabelas.iter().enumerate().map(|(i, t)| self.miniatura(i, t))).spacing(8).width(96);
            let mut previa = column![text(format!("Prévia da aba {} · confira o tipo de cada coluna", if t.titulo.is_empty() { format!("Tabela {}", e.escolhida + 1) } else { t.titulo.clone() })).size(12).color(muted)].spacing(6);
            previa = previa.push(scrollable(self.grade_previa(e.escolhida, t)).direction(Direction::Horizontal));
            let ruins = ruins(t);
            if let Some((end, txt)) = ruins.first() {
                previa = previa.push(text(format!("{} célula{} não {} no tipo ({end}: “{txt}”{}). Fica{} como texto e marcada{} em vermelho na planilha.",
                    ruins.len(), if ruins.len() == 1 { "" } else { "s" }, if ruins.len() == 1 { "se lê" } else { "se leem" },
                    if ruins.len() > 1 { "…" } else { "" }, if ruins.len() == 1 { "" } else { "m" }, if ruins.len() == 1 { "" } else { "s" }))
                    .size(12).color(tema::COR_ERRO()));
            }
            c = c.push(row![miniaturas, previa.width(Length::FILL)].spacing(14));
            let continuam = e.continuacoes();
            let juntaveis = matches!(&e.brutas, Some(Ok(b)) if featherpdf_core::juntar_continuacoes(b.clone()).len() != b.len());
            if juntaveis {
                c = c.push(checkbox(format!("Juntar tabela que continua na página seguinte{}", if continuam.is_empty() { String::new() } else { format!(" ({})", continuam.join(", ")) }), e.juntar)
                    .on_toggle(|v| Msg::Exp(Ex::Juntar(v))).size(15).text_size(12.5));
            }
        }
        let tem_escaneada = self.aberto.as_ref().is_some_and(|a| (1..=a.tamanhos.len() as u32).any(|p| a.pdf.texto_pagina(p).is_ok_and(|t| t.trim().chars().count() <= crate::ocr::MINIMO_TEXTO)));
        if e.modo != ModoPlanilha::Pagina && (tem_escaneada || e.ocr_tabelas) {
            c = c.push(checkbox("Ler tabela de página escaneada com OCR", e.ocr_tabelas).on_toggle(|v| Msg::Exp(Ex::OcrTabelas(v))).size(15).text_size(12.5));
        }
        c
    }

    /// Página em miniatura com a tabela marcada; clicar escolhe a tabela.
    fn miniatura<'a>(&'a self, i: usize, t: &'a TabelaPdf) -> Element<'a, Msg> {
        let e = &self.exportar;
        let (pagina_pt, _) = self.aberto.as_ref().and_then(|a| a.pdf.geometria(t.pagina).ok()).unwrap_or(([0.0, 0.0, 595.0, 842.0], 0));
        let (l, h) = (pagina_pt[2] - pagina_pt[0], pagina_pt[3] - pagina_pt[1]);
        let (w, a) = (86.0, 86.0 * h / l.max(1.0));
        let x = ((t.caixa[0] - pagina_pt[0]) / l * w).clamp(0.0, w - 4.0);
        let y = ((pagina_pt[3] - t.caixa[3]) / h * a).clamp(0.0, a - 4.0);
        let (lw, la) = (((t.caixa[2] - t.caixa[0]) / l * w).clamp(4.0, w - x), ((t.caixa[3] - t.caixa[1]) / h * a).clamp(4.0, a - y));
        let ativa = e.escolhida == i;
        let verde = Color::rgb8(0x2e, 0x7d, 0x4f);
        let pagina = Stack::new().width(w).height(a)
            .push(container(Space::new(w, a)).style(|_| Style { background: Some(Background::Color(Color::WHITE)), ..Default::default() }))
            .push(column![Space::with_height(y), row![Space::with_width(x), container(Space::new(lw, la)).style(move |_| Style {
                background: Some(Background::Color(Color { a: 0.18, ..verde })), border: Border { color: verde, width: 1.0, radius: [1.0; 4] }, ..Default::default()
            })]]);
        let paginas = if t.ate_pagina > t.pagina { format!("p. {}–{}", t.pagina, t.ate_pagina) } else { format!("p. {}", t.pagina) };
        let legenda = if t.titulo.is_empty() { paginas } else { format!("{paginas} · {}", t.titulo) };
        button(column![pagina, text(legenda).size(10.5).color(tema::COR_TEXTO_MUTED())].spacing(3)).padding(4).on_press(Msg::Exp(Ex::Escolher(i)))
            .style(move |_, _| Style {
                background: Some(Background::Color(Color::rgb8(0xf3, 0xf4, 0xf6))),
                border: Border { color: if ativa { verde } else { Color::rgb8(0xd0, 0xd4, 0xda) }, width: if ativa { 2.0 } else { 1.0 }, radius: [4.0; 4] },
                ..Default::default()
            }).into()
    }

    /// A prévia em grade: letras, tipo de cada coluna, cabeçalho e as primeiras linhas.
    fn grade_previa<'a>(&'a self, i: usize, t: &'a TabelaPdf) -> Element<'a, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;
        let larg = 132.0;
        let borda = Color::rgb8(0xdd, 0xe0, 0xe5);
        let celula = move |conteudo: Element<'a, Msg>, fundo: Color| container(conteudo).width(larg).padding([4, 6])
            .style(move |_| Style { background: Some(Background::Color(fundo)), border: Border { color: borda, width: 0.5, radius: [0.0; 4] }, ..Default::default() });
        let num = move |s: String| container(text(s).size(11).color(muted)).width(28).padding([4, 4])
            .style(move |_| Style { background: Some(Background::Color(Color::rgb8(0xf3, 0xf4, 0xf6))), border: Border { color: borda, width: 0.5, radius: [0.0; 4] }, ..Default::default() });
        let cinza = Color::rgb8(0xf3, 0xf4, 0xf6);
        let mut g = column![].spacing(0);
        g = g.push(row(std::iter::once(num(String::new()).into()).chain((0..t.cabecalho.len()).map(|c|
            celula(text(letra(c)).size(11).color(muted).width(Length::FILL).align_x(Align::Center).into(), cinza).into()))));
        g = g.push(row(std::iter::once(num(String::new()).into()).chain(t.tipos.iter().enumerate().map(|(c, &tipo)|
            celula(suspensa(TipoColuna::TODOS, Some(tipo), move |x| Msg::Exp(Ex::Tipo(i, c, x))).text_size(11).padding([2, 4]).width(Length::FILL).into(),
                Color::rgb8(0xfa, 0xfb, 0xfc)).into()))));
        g = g.push(row(std::iter::once(num("1".into()).into()).chain(t.cabecalho.iter().map(|h|
            celula(text(h.clone()).size(12).font(negrito).into(), Color::WHITE).into()))));
        for (l, linha) in t.linhas.iter().take(8).enumerate() {
            g = g.push(row(std::iter::once(num((l + 2).to_string()).into()).chain(linha.iter().enumerate().map(|(c, v)| {
                let tipo = t.tipos.get(c).copied().unwrap_or(TipoColuna::Texto);
                let ruim = !v.trim().is_empty() && ler_valor(v, tipo).is_none();
                let alinhar = if matches!(tipo, TipoColuna::Numero | TipoColuna::Moeda | TipoColuna::Data) && !ruim { Align::End } else { Align::Start };
                celula(text(v.clone()).size(12).width(Length::FILL).align_x(alinhar).into(), if ruim { Color::rgb8(0xfd, 0xe2, 0xe2) } else { Color::WHITE }).into()
            }))));
        }
        if t.linhas.len() > 8 { g = g.push(text(format!("… e mais {} linhas", t.linhas.len() - 8)).size(11).color(muted)); }
        g.into()
    }
}

/// As páginas como uma página web simples (um título por página, parágrafos pelas linhas em branco).
fn html_das_paginas(textos: &[(u32, String)]) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut h = String::from("<!doctype html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Documento</title>\
        <style>body{font-family:sans-serif;max-width:46em;margin:2em auto;line-height:1.5}h2{color:#666;font-size:1em;border-top:1px solid #ddd;padding-top:1em}</style></head><body>\n");
    for (n, t) in textos {
        h += &format!("<h2>Página {n}</h2>\n");
        for p in t.split("\n\n").filter(|p| !p.trim().is_empty()) { h += &format!("<p>{}</p>\n", esc(p.trim()).replace('\n', "<br>")); }
    }
    h + "</body></html>\n"
}

#[cfg(test)]
mod testes {
    #[test]
    fn letras_das_colunas_e_html() {
        assert_eq!((super::letra(0), super::letra(25), super::letra(26)), ("A".into(), "Z".into(), "AA".into()));
        let h = super::html_das_paginas(&[(1, "a < b\nlinha 2\n\noutro".into())]);
        assert!(h.contains("<p>a &lt; b<br>linha 2</p>") && h.contains("<p>outro</p>"), "{h}");
    }
}
