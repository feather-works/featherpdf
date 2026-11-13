//! Ferramentas de área: arrastar um retângulo na página e
//! - **OCR da área**: ler só aquele pedaço (o carimbo de uma prancha, uma nota no canto), copiar
//!   ou deixar pesquisável;
//! - **Tabela do scan**: ler a tabela escaneada da área e levar para planilha (.xlsx);
//! - **Gráfico para dados**: tirar os pontos de um gráfico vetorial para planilha;
//! - **Copiar como imagem**: o pedaço da página vai para a área de transferência (WhatsApp, Word).

use std::path::PathBuf;

use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::TabelaPdf;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Ferramenta, Msg, Painel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uso { Ocr, Tabela, Grafico, Imagem, /** Zoom por área: o retângulo enche a tela. */ Zoom }

#[derive(Debug, Clone)]
pub enum Rr {
    Ferramenta(Uso),
    Area { pagina: u32, a: Point, b: Point },
    /// Página, e as linhas com quanto a imagem girou para a leitura ficar em pé.
    Lida(u32, Result<(Vec<(String, [f32; 4])>, u32), String>),
    Tabelas(Result<Vec<TabelaPdf>, String>),
    Salvar(Vec<TabelaPdf>, Option<PathBuf>),
    Copiar,
    Pesquisavel,
    /// A área foi para a área de transferência: largura × altura em pixels.
    ImagemCopiada(Result<(u32, u32), String>),
}

/// O texto lido da última área: página, linhas (caixa em fração da página vista) e o giro da leitura.
#[derive(Default)]
pub struct Regiao {
    pub lida: Option<(u32, Vec<(String, [f32; 4])>, u32)>,
    pub lendo: bool,
    /// A leitura já virou texto no PDF: um segundo clique somaria outra camada igual (Ctrl+F acharia em dobro).
    pub pesquisavel: bool,
}

impl App {
    pub fn regiao(&self) -> Option<Uso> { match self.ferramenta { Ferramenta::Regiao(u) => Some(u), _ => None } }

    pub fn camada_regiao(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        self.regiao().map(|_| crate::inserir::camada_arrastar(n, |pagina, a, b| Msg::Regiao(Rr::Area { pagina, a, b }), l, a))
    }

    pub fn update_regiao(&mut self, m: Rr) -> Task<Msg> {
        match m {
            Rr::Ferramenta(u) => {
                let ligar = self.regiao() != Some(u);
                let f = if ligar { Ferramenta::Regiao(u) } else { Ferramenta::Selecionar };
                if ligar {
                    self.status = match u {
                        Uso::Ocr => "Arraste um retângulo em volta do texto escaneado que quer ler.",
                        Uso::Tabela => "Arraste um retângulo em volta da tabela escaneada.",
                        Uso::Grafico => "Arraste um retângulo em volta do gráfico, incluindo os números dos eixos.",
                        Uso::Imagem => "Arraste um retângulo em volta do que quer copiar como imagem.",
                        Uso::Zoom => "Arraste um retângulo: ele passa a encher a tela. Clique de novo em Área para sair.",
                    }.into();
                }
                return self.update_interno(Msg::Ferramenta(f));
            }
            Rr::Area { pagina, a, b } => {
                let (Some(uso), Some(aberto), Some(caixa)) = (self.regiao(), &self.aberto, self.caixa_na_pagina(pagina, a, b)) else { return Task::none() };
                if caixa[2] - caixa[0] < 4.0 || caixa[3] - caixa[1] < 4.0 { return Task::none(); }
                if uso == Uso::Grafico {
                    return self.update_regiao(Rr::Tabelas(aberto.pdf.dados_do_grafico(pagina, caixa)));
                }
                // Fração da página vista (y para baixo), para desenhar só o trecho e voltar aos pontos.
                let (pl, pa) = aberto.tamanhos[pagina as usize - 1];
                let (l, h) = (pl * self.zoom, pa * self.zoom);
                let f = [a.x.min(b.x) / l, a.y.min(b.y) / h, a.x.max(b.x) / l, a.y.max(b.y) / h].map(|v| v.clamp(0.0, 1.0));
                if uso == Uso::Zoom {
                    let (alvo, ancora) = zoom_da_area(((f[2] - f[0]) * pl, (f[3] - f[1]) * pa), self.tamanho_mesa.get());
                    self.zoom_animado = Some(crate::ZoomAnimado { alvo, ancora: (pagina, (f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0, ancora) });
                    return Task::none();
                }
                if uso == Uso::Imagem {
                    let vis = aberto.vis.clone();
                    let escala = escala_da_copia(((f[2] - f[0]) * pl, (f[3] - f[1]) * pa), self.zoom);
                    self.status = "Copiando a área…".into();
                    return Task::perform(async move {
                        tokio::task::spawn_blocking(move || copiar_area(&vis, pagina, (pl, pa), f, escala))
                            .await.unwrap_or_else(|e| Err(e.to_string()))
                    }, |r| Msg::Regiao(Rr::ImagemCopiada(r)));
                }
                let Ok((geometria, giro)) = aberto.pdf.geometria(pagina) else { return Task::none() };
                let vis = aberto.vis.clone();
                self.regiao.lendo = true;
                self.status = "Lendo a área…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || ler_area(&vis, pagina, (pl, pa), f, (geometria, giro), uso == Uso::Tabela))
                        .await.unwrap_or_else(|e| Err(e.to_string()))
                }, move |r| Msg::Regiao(if uso == Uso::Tabela {
                    Rr::Tabelas(r.and_then(|(palavras, _)| {
                        let t = featherpdf_core::tabelas_da_pagina(pagina, &palavras);
                        if t.is_empty() { Err("Não achei linhas e colunas alinhadas nessa área. Inclua o cabeçalho e pelo menos duas linhas.".into()) } else { Ok(t) }
                    }))
                } else { Rr::Lida(pagina, r) }));
            }
            Rr::Lida(pagina, r) => {
                self.regiao.lendo = false;
                match r {
                    Ok((v, _)) if v.is_empty() => self.status = "Nenhum texto na área.".into(),
                    Ok((v, giro)) => {
                        self.status = format!("{} linha(s) lidas na página {pagina}{}.", v.len(), if giro == 0 { "" } else { " (texto deitado)" });
                        self.regiao.lida = Some((pagina, v, giro));
                        self.regiao.pesquisavel = false;
                        self.painel = Some(Painel::Regiao);
                    }
                    Err(e) => self.status = e,
                }
            }
            Rr::Tabelas(r) => {
                self.regiao.lendo = false;
                match r {
                    Ok(t) => {
                        let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
                        let base = std::path::Path::new(&nome).file_stem().map_or("documento".into(), |n| n.to_string_lossy().into_owned());
                        let sugestao = format!("{base} (dados).xlsx");
                        return Task::perform(async move {
                            let destino = rfd::AsyncFileDialog::new().add_filter("Planilha", &["xlsx"]).set_file_name(sugestao).save_file().await.map(|h| h.path().to_path_buf());
                            (t, destino)
                        }, |(t, d)| Msg::Regiao(Rr::Salvar(t, d)));
                    }
                    Err(e) => self.status = e,
                }
            }
            Rr::Salvar(_, None) => {}
            Rr::Salvar(t, Some(destino)) => {
                let linhas: usize = t.iter().map(|x| x.linhas.len()).sum();
                match featherpdf_io::gravar_tabelas_xlsx(&t, false, &destino) {
                    Ok(()) => self.avisar_salvo(format!("{} tabela(s), {linhas} linha(s), salvas em {} — abre no FeatherSheet.", t.len(), destino.display()), destino),
                    Err(e) => self.status = e,
                }
            }
            Rr::Copiar => if let Some((_, v, _)) = &self.regiao.lida {
                self.status = "Texto da área copiado.".into();
                return Task::write_clipboard(v.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join("\n"));
            },
            Rr::ImagemCopiada(r) => self.status = match r {
                Ok((l, a)) => format!("Área copiada como imagem ({l} × {a} pixels): cole com Ctrl+V."),
                Err(e) => e,
            },
            Rr::Pesquisavel => if let Some((pagina, v, giro)) = self.regiao.lida.clone().filter(|_| !self.regiao.pesquisavel) {
                let tarefa = self.editar(move |pdf| {
                    let n = pdf.inserir_camada_ocr_girada(pagina, &v, giro)?;
                    Ok(format!("{n} linha(s) da área agora podem ser buscadas e copiadas no PDF (Ctrl+Z desfaz)."))
                });
                self.regiao.pesquisavel = self.ultima_edicao_ok;
                return tarefa;
            },
        }
        Task::none()
    }

    pub fn painel_regiao(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let Some((pagina, linhas, _)) = &self.regiao.lida else {
            return text(if self.regiao.lendo { "Lendo…" } else { "Use Converter → OCR da área e arraste em volta do texto." }).size(13).color(muted).into();
        };
        let mut texto = column![].spacing(2);
        for (t, _) in linhas { texto = texto.push(text(t.clone()).size(13)); }
        let corpo = column![
            text(format!("Página {pagina}, {} linha(s)", linhas.len())).size(12).color(muted),
            container(texto).padding([8, 10]).width(Length::FILL).style(dlg::estilo_bloco),
            row![
                button(text("Copiar").size(12)).padding([4, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Regiao(Rr::Copiar)),
                Space::with_width(Length::FILL),
                button(text(if self.regiao.pesquisavel { "Já pesquisável" } else { "Tornar pesquisável" }).size(12)).padding([4, 10]).style(tema::estilo_botao_pequeno(false))
                    .on_press_maybe((!self.regiao.pesquisavel).then_some(Msg::Regiao(Rr::Pesquisavel))),
            ],
            text("Tornar pesquisável põe o texto lido, invisível, por cima da área: a busca e o copiar passam a achar.").size(11.5).color(muted),
        ].spacing(10);
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}

/// Pixels por ponto da cópia: o dobro do zoom da tela (nítida colada em outro lugar), de 2 a 8,
/// e no máximo ~16 MP (uma área enorme não estoura a memória nem a área de transferência).
fn escala_da_copia((l, a): (f32, f32), zoom: f32) -> f32 {
    let teto = (16_000_000.0 / (l * a).max(1.0)).sqrt();
    (zoom * 2.0).clamp(2.0, 8.0).min(teto)
}

/// Bloqueante: desenha o trecho `f` (fração da página vista) e põe na área de transferência.
fn copiar_area(vis: &featherpdf_io::Visualizador, pagina: u32, (pl, pa): (f32, f32), f: [f32; 4], escala: f32) -> Result<(u32, u32), String> {
    let (dl, dh) = (pl * escala, pa * escala);
    let t = [(f[0] * dl) as u32, (f[1] * dh) as u32, ((f[2] - f[0]) * dl).ceil().max(1.0) as u32, ((f[3] - f[1]) * dh).ceil().max(1.0) as u32];
    let r = vis.desenhar_trecho(pagina, escala, t)?;
    // No Linux o conteúdo some quando o `Clipboard` é solto: este fica vivo até a próxima cópia.
    static AREA: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let mut area = AREA.lock().unwrap_or_else(|e| e.into_inner());
    if area.is_none() { *area = Some(arboard::Clipboard::new().map_err(|e| format!("Área de transferência indisponível: {e}"))?); }
    let imagem = arboard::ImageData { width: r.largura as usize, height: r.altura as usize, bytes: r.rgba.into() };
    area.as_mut().expect("criado acima").set_image(imagem).map_err(|e| format!("Não consegui copiar a imagem: {e}"))?;
    Ok((r.largura, r.altura))
}

/// Lê o texto do trecho `f` (fração da página vista) com o OCR. Para tabela, cada palavra com a
/// caixa em pontos do PDF; para texto, cada linha com a caixa em fração da página vista (0..1,
/// y para baixo), que é o que `inserir_camada_ocr` espera. Trecho mais alto que largo, ou sem
/// nada lido em pé, também é lido girado 90° para cada lado (texto deitado de planta, carimbo):
/// fica a leitura de mais confiança, e o giro dela vai junto.
pub(crate) fn ler_area(vis: &featherpdf_io::Visualizador, pagina: u32, (pl, pa): (f32, f32), f: [f32; 4],
    (geometria, giro): ([f32; 4], i64), tabela: bool) -> Result<(Vec<(String, [f32; 4])>, u32), String> {
    // ~216 dpi: letra pequena de carimbo ainda sai legível.
    let escala = 3.0;
    let (dl, dh) = (pl * escala, pa * escala);
    let t = [(f[0] * dl) as u32, (f[1] * dh) as u32, ((f[2] - f[0]) * dl).ceil().max(1.0) as u32, ((f[3] - f[1]) * dh).ceil().max(1.0) as u32];
    let r = vis.desenhar_trecho(pagina, escala, t)?;
    let rgb: Vec<u8> = r.rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let motor = crate::ocr::motor()?;
    let original = featherpdf_ocr::Imagem { largura: r.largura, altura: r.altura, rgb: &rgb };
    let (w, h) = (r.largura as f32, r.altura as f32);
    let ler = |g: u32| -> Result<(Vec<featherpdf_ocr::Linha>, f32), String> {
        let mut linhas = if g == 0 { motor.reconhecer(&original)? } else {
            let (gw, gh, girada) = featherpdf_ocr::girar(&original, g);
            motor.reconhecer(&featherpdf_ocr::Imagem { largura: gw, altura: gh, rgb: &girada })?
        };
        // Caixa na imagem girada → caixa no trecho como está na página.
        let volta = |[x0, y0, x1, y1]: [f32; 4]| match g { 90 => [y0, h - x1, y1, h - x0], 270 => [w - y1, x0, w - y0, x1], _ => [x0, y0, x1, y1] };
        for l in &mut linhas { l.caixa = volta(l.caixa); for p in &mut l.palavras { p.caixa = volta(p.caixa); } }
        let nota = linhas.iter().map(|l| l.confianca * l.texto.chars().filter(|c| c.is_alphanumeric()).count() as f32).sum();
        Ok((linhas, nota))
    };
    let (mut linhas, mut nota) = ler(0)?;
    let mut giro_lido = 0;
    if linhas.is_empty() || h > w * 1.2 {
        for g in [90, 270] {
            let (outras, n) = ler(g)?;
            if n > nota * 1.2 { (linhas, nota, giro_lido) = (outras, n, g); }
        }
    }
    // Pixel do trecho → fração da página vista.
    let fracao = |c: [f32; 4]| [(t[0] as f32 + c[0]) / dl, (t[1] as f32 + c[1]) / dh, (t[0] as f32 + c[2]) / dl, (t[1] as f32 + c[3]) / dh];
    Ok((if tabela {
        linhas.into_iter().flat_map(|x| x.palavras).map(|p| (p.texto, crate::ocr::em_pontos(fracao(p.caixa), geometria, giro))).collect()
    } else {
        linhas.into_iter().map(|x| (x.texto, fracao(x.caixa))).collect()
    }, giro_lido))
}

/// Zoom que faz a área (em pontos) caber na mesa com folga, e o centro da mesa (onde o centro
/// da área vai parar).
fn zoom_da_area((l, a): (f32, f32), mesa: Size) -> (f32, Point) {
    let folga = 2.0 * crate::VAO_PAGINAS;
    let zoom = ((mesa.width - folga) / l.max(1.0)).min((mesa.height - folga) / a.max(1.0));
    (zoom.clamp(crate::ZOOM_MIN, crate::ZOOM_MAX), Point::new(mesa.width / 2.0, mesa.height / 2.0))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn zoom_da_area_enche_a_mesa_e_respeita_o_teto() {
        let mesa = Size::new(1032.0, 632.0);
        let (z, centro) = zoom_da_area((250.0, 100.0), mesa);
        assert!((z - 4.0).abs() < 1e-3 && centro == Point::new(516.0, 316.0), "{z}");
        assert_eq!(zoom_da_area((1.0, 1.0), mesa).0, crate::ZOOM_MAX);
        assert_eq!(zoom_da_area((10_000.0, 10_000.0), mesa).0, crate::ZOOM_MIN);
    }

    #[test]
    fn copia_nitida_mas_com_teto_de_pixels() {
        assert_eq!(escala_da_copia((100.0, 100.0), 0.5), 2.0);
        assert_eq!(escala_da_copia((100.0, 100.0), 8.0), 8.0);
        // Folha A1 inteira (2384 × 1684 pt): cai para caber em ~16 MP.
        let e = escala_da_copia((2384.0, 1684.0), 2.0);
        assert!(2384.0 * e * 1684.0 * e <= 16_000_001.0 && e > 1.0, "{e}");
    }

    #[test]
    fn area_lida_fica_dentro_da_pagina_ao_tornar_pesquisavel() {
        let mut pdf = featherpdf_io::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "Relatorio mensal", 28.0, false, [0.0; 3]).unwrap();
        let vis = featherpdf_io::Visualizador::novo(pdf.serializar().unwrap()).unwrap();
        let (geometria, giro) = pdf.geometria(1).unwrap();
        let (lidas, girada) = ler_area(&vis, 1, (595.0, 842.0), [0.05, 0.1, 0.8, 0.25], (geometria, giro), false).unwrap();
        assert_eq!(girada, 0);
        assert!(!lidas.is_empty());
        assert!(lidas.iter().all(|(_, c)| c.iter().all(|v| (0.0..=1.0).contains(v))), "{lidas:?}");
        let mut limpo = featherpdf_io::criar_a4().unwrap();
        limpo.inserir_camada_ocr(1, &lidas).unwrap();
        let texto = limpo.texto_pagina(1).unwrap().to_lowercase();
        assert!(texto.contains("mensal"), "{texto:?}");
        // A camada fica onde a frase estava, no alto da página, e não fora dela.
        let caixa = limpo.objetos(1).unwrap()[0].caixa;
        assert!(caixa[1] > 600.0 && caixa[3] < 842.0, "{caixa:?}");
    }

    #[test]
    fn texto_deitado_e_lido_girado_e_volta_deitado() {
        // A página girada 90° mostra a frase na vertical, como os rótulos de uma planta.
        let mut pdf = featherpdf_io::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "Relatorio mensal", 28.0, false, [0.0; 3]).unwrap();
        pdf.girar_paginas(&[1], 90).unwrap();
        let vis = featherpdf_io::Visualizador::novo(pdf.serializar().unwrap()).unwrap();
        let (pl, pa) = vis.tamanhos()[0];
        assert!(pl > pa, "a vista fica deitada");
        let (geometria, giro) = pdf.geometria(1).unwrap();
        let (lidas, girada) = ler_area(&vis, 1, (pl, pa), [0.75, 0.05, 0.95, 0.6], (geometria, giro), false).unwrap();
        assert!(girada == 90 || girada == 270, "giro {girada}: {lidas:?}");
        let junto = lidas.iter().map(|(t, _)| t.to_lowercase()).collect::<Vec<_>>().join(" ");
        assert!(junto.contains("mensal"), "{junto:?}");
        let mut limpo = featherpdf_io::criar_a4().unwrap();
        limpo.girar_paginas(&[1], 90).unwrap();
        limpo.inserir_camada_ocr_girada(1, &lidas, girada).unwrap();
        assert!(limpo.texto_pagina(1).unwrap().to_lowercase().contains("mensal"));
        // A camada cai em cima da frase original (perto de y = 700 na página em pé).
        let caixa = limpo.objetos(1).unwrap()[0].caixa;
        assert!(caixa[1] > 600.0 && caixa[3] < 842.0 && caixa[0] > 40.0 && caixa[2] < 400.0, "{caixa:?}");
    }
}
