//! Adaptador do FeatherPDF sobre os motores compartilhados.
 //! `featherpdf-core` cuida da estrutura; `feather-io` cuida do
 //! formato de exportacao e da gravacao atomica. Hayro rasteriza PDFs reais.

use std::path::{Path, PathBuf};

use featherpdf_core::DocumentoPdf;
use feather_draw::Cena;
use feather_io::seguranca::{
    gravar_bytes_atomico, ler_arquivo_limitado, PoliticaArquivos,
};
use feather_io::{Documento, Registro};

/// Exportador vetorial oficial da suíte, sem implementar um segundo encoder.
/// Perfil sRGB (CC0, do colord) para a intenção de saída do PDF/A.
pub const PERFIL_SRGB: &[u8] = include_bytes!("../perfis/sRGB.icc");

pub fn criar_a4() -> Result<DocumentoPdf, String> {
    let cena = Cena::nova(595.0, 842.0);
    let bytes = Registro::padrao()
        .gravar("pdf", &Documento::Desenho(cena))
        .map_err(|e| format!("Falha ao criar PDF pelo FeatherIO: {e:?}"))?;
    DocumentoPdf::de_bytes(&bytes, None)
}

/// Leitura limitada pelo mesmo modulo de seguranca usado pelos demais produtos.
pub fn abrir(caminho: &Path) -> Result<DocumentoPdf, String> { abrir_com_senha(caminho, None) }

/// [`abrir`] com a senha que a pessoa digitou (erro `featherpdf_core::PRECISA_SENHA` quando ela falta).
pub fn abrir_com_senha(caminho: &Path, senha: Option<&str>) -> Result<DocumentoPdf, String> {
    let bytes = ler_arquivo_limitado(caminho, &PoliticaArquivos::default())
        .map_err(|e| format!("Erro ao ler PDF: {e}"))?;
    if !bytes.starts_with(b"%PDF-") {
        return Err("O arquivo selecionado nao possui cabecalho PDF valido.".into());
    }
    DocumentoPdf::de_bytes_com_senha(&bytes, Some(caminho.to_path_buf()), senha)
}

/// Bytes crus de um arquivo (o PDF como está no disco, para assinar/validar; o .pfx).
pub fn ler_bytes(caminho: &Path) -> Result<Vec<u8>, String> {
    ler_arquivo_limitado(caminho, &PoliticaArquivos::default())
        .map_err(|e| format!("Erro ao ler {}: {e}", caminho.display()))
}

/// Salva em arquivo temporario e renomeia apenas em caso de sucesso.
pub fn salvar_como(pdf: &mut DocumentoPdf, destino: &Path) -> Result<(), String> {
    gravar_pdf(destino, &pdf.bytes_para_gravar()?)?;
    pdf.confirmar_salvo(PathBuf::from(destino));
    Ok(())
}

mod fluxo;
mod fontes;
mod pptx;
pub use fluxo::{executar_fluxo, Bloco, NIVEIS_COMPRESSAO, PAGINAS_GIRAR, TAMANHOS_PAGINA};
pub use fontes::embutir_fontes_do_sistema;
pub use pptx::gravar_pptx;

/// Teto para gravar: um PDF juntado passa fácil dos 128 MiB que limitam a leitura.
fn politica_gravacao() -> PoliticaArquivos {
    PoliticaArquivos { max_bytes_arquivo: 2 << 30, ..Default::default() }
}

/// Gravação atômica do FeatherIO que respeita o arquivo que já existia: um link
/// simbólico continua link (grava no alvo) e as permissões ficam as mesmas (0600 não vira 0644).
pub fn gravar_atomico(destino: &Path, bytes: &[u8]) -> Result<(), String> {
    let destino = std::fs::canonicalize(destino).unwrap_or_else(|_| destino.to_path_buf());
    let permissoes = std::fs::metadata(&destino).ok().map(|m| m.permissions());
    gravar_bytes_atomico(&destino, bytes, &politica_gravacao()).map_err(|e| e.to_string())?;
    // shortcut: o temporário nasce com a permissão padrão e só ganha a antiga depois do rename;
    // passar a permissão para o feather-io quando ele aceitar uma.
    if let Some(p) = permissoes { std::fs::set_permissions(&destino, p).map_err(|e| e.to_string())?; }
    Ok(())
}

/// Grava bytes de PDF já prontos (páginas extraídas, partes de uma divisão).
pub fn gravar_pdf(destino: &Path, bytes: &[u8]) -> Result<(), String> {
    if !destino.extension().and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err("O arquivo de destino deve ter extensao .pdf.".into());
    }
    gravar_atomico(destino, bytes)
        .map_err(|e| format!("Erro ao salvar PDF: {e}"))
}

/// Página desenhada (RGBA) em WebP sem perda (codificador em Rust puro).
pub fn gravar_webp(destino: &Path, largura: u32, altura: u32, rgba: Vec<u8>) -> Result<(), String> {
    let img = image::RgbaImage::from_raw(largura, altura, rgba).ok_or("Página com tamanho incoerente.")?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::WebP).map_err(|e| format!("{}: {e}", destino.display()))?;
    gravar_atomico(destino, bytes.get_ref()).map_err(|e| format!("Erro ao salvar a imagem: {e}"))
}

/// Reamostrador do Comprimir: a imagem cabe em `lado` px (sem aumentar) e volta como JPEG
/// na `qualidade`, com o mesmo número de canais do espaço de cor.
pub fn reamostrar_jpeg(img: featherpdf_core::ImagemCrua<'_>, lado: u32, qualidade: u8) -> Option<(Vec<u8>, u32, u32)> {
    use image::{DynamicImage, GrayImage, RgbImage};
    let original = if img.jpeg {
        image::load_from_memory_with_format(img.dados, image::ImageFormat::Jpeg).ok()?
    } else if img.componentes == 3 {
        DynamicImage::ImageRgb8(RgbImage::from_raw(img.largura, img.altura, img.dados.to_vec())?)
    } else {
        DynamicImage::ImageLuma8(GrayImage::from_raw(img.largura, img.altura, img.dados.to_vec())?)
    };
    let menor = if lado < original.width().max(original.height()) {
        original.resize(lado, lado, image::imageops::FilterType::CatmullRom)
    } else { original };
    let menor = if img.componentes == 1 { DynamicImage::ImageLuma8(menor.to_luma8()) } else { DynamicImage::ImageRgb8(menor.to_rgb8()) };
    // Tabelas de Huffman feitas para a imagem: 5–10% menor que as tabelas padrão, mesma qualidade.
    let mut jpeg = Vec::new();
    let mut codificador = jpeg_encoder::Encoder::new(&mut jpeg, qualidade.clamp(1, 100));
    codificador.set_optimized_huffman_tables(true);
    let (lado_x, lado_y) = (u16::try_from(menor.width()).ok()?, u16::try_from(menor.height()).ok()?);
    let cor = if img.componentes == 1 { jpeg_encoder::ColorType::Luma } else { jpeg_encoder::ColorType::Rgb };
    codificador.encode(menor.as_bytes(), lado_x, lado_y, cor).ok()?;
    Some((jpeg, menor.width(), menor.height()))
}

/// Para a tarja: pinta de preto os retângulos (pixels, origem no alto à
/// esquerda) e regrava em JPEG de qualidade 90.
pub fn apagar_pixels_jpeg(img: featherpdf_core::ImagemCrua<'_>, retangulos: &[[u32; 4]]) -> Option<(Vec<u8>, u32, u32)> {
    use image::DynamicImage;
    let imagem = match decodificar(img)? {
        DynamicImage::ImageLuma8(mut i) => { for r in retangulos { pintar(&mut i, *r, image::Luma([0])); } DynamicImage::ImageLuma8(i) }
        DynamicImage::ImageRgb8(mut i) => { for r in retangulos { pintar(&mut i, *r, image::Rgb([0, 0, 0])); } DynamicImage::ImageRgb8(i) }
        _ => return None,
    };
    em_jpeg(imagem)
}

/// Página escaneada com o papel acinzentado: estica os níveis para o papel virar branco
/// e a tinta preta (ponto branco no percentil 90 do brilho, ponto preto no 2).
/// `None` quando a imagem não tem contraste para isso (foto escura, imagem lisa).
pub fn clarear_fundo_jpeg(img: featherpdf_core::ImagemCrua<'_>) -> Option<(Vec<u8>, u32, u32)> {
    let mut imagem = decodificar(img)?;
    let brilho = imagem.to_luma8();
    let (preto, branco) = niveis_do_papel(brilho.as_raw())?;
    let mapa: Vec<u8> = (0..=255u32).map(|v| {
        let v = v as f32;
        (((v - preto) / (branco - preto)).clamp(0.0, 1.0) * 255.0).round() as u8
    }).collect();
    match &mut imagem {
        image::DynamicImage::ImageLuma8(i) => i.iter_mut().for_each(|v| *v = mapa[*v as usize]),
        image::DynamicImage::ImageRgb8(i) => i.iter_mut().for_each(|v| *v = mapa[*v as usize]),
        _ => return None,
    }
    em_jpeg(imagem)
}

/// Foto de caderno: tira sombra, pauta e quadriculado e deixa só a tinta (`featherpdf_core::limpar_caderno`).
pub fn limpar_caderno_jpeg(img: featherpdf_core::ImagemCrua<'_>) -> Option<(Vec<u8>, u32, u32)> {
    let mut rgb = decodificar(img)?.to_rgb8();
    let (l, h) = rgb.dimensions();
    featherpdf_core::limpar_caderno(&mut rgb, l, h);
    em_jpeg(image::DynamicImage::ImageRgb8(rgb))
}

/// Pontos preto e branco do papel pelo histograma do brilho.
fn niveis_do_papel(brilho: &[u8]) -> Option<(f32, f32)> {
    let mut hist = [0usize; 256];
    for &v in brilho { hist[v as usize] += 1; }
    let total = brilho.len();
    let percentil = |p: f32| {
        let alvo = (total as f32 * p) as usize;
        let mut soma = 0;
        hist.iter().position(|&n| { soma += n; soma > alvo }).unwrap_or(255) as f32
    };
    let (preto, branco) = (percentil(0.02), percentil(0.90));
    (branco - preto >= 40.0).then_some((preto, branco))
}

/// Um TIFF de várias páginas (Deflate, RGB), uma por chamada de `desenhar`, com o `dpi` gravado
/// para imprimir no tamanho certo. Desenha uma de cada vez; o TIFF comprimido fica na memória até a gravação atômica.
pub fn gravar_tiff(destino: &Path, paginas: &[u32], dpi: u32, mut desenhar: impl FnMut(u32) -> Result<(u32, u32, Vec<u8>), String>) -> Result<(), String> {
    use tiff::encoder::{colortype::RGB8, compression::DeflateLevel, Compression, Rational, TiffEncoder};
    let erro = |e: tiff::TiffError| format!("{}: {e}", destino.display());
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut tiff = TiffEncoder::new(&mut bytes).map_err(erro)?.with_compression(Compression::Deflate(DeflateLevel::Balanced));
    for &n in paginas {
        let (l, a, rgba) = desenhar(n)?;
        let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut img = tiff.new_image::<RGB8>(l, a).map_err(erro)?;
        img.resolution(tiff::tags::ResolutionUnit::Inch, Rational { n: dpi, d: 1 });
        img.write_data(&rgb).map_err(erro)?;
    }
    drop(tiff);
    gravar_atomico(destino, bytes.get_ref()).map_err(|e| format!("Erro ao salvar a imagem: {e}"))
}

fn decodificar(img: featherpdf_core::ImagemCrua<'_>) -> Option<image::DynamicImage> {
    use image::{DynamicImage, GrayImage, RgbImage};
    Some(if img.jpeg {
        let d = image::load_from_memory_with_format(img.dados, image::ImageFormat::Jpeg).ok()?;
        if img.componentes == 1 { DynamicImage::ImageLuma8(d.to_luma8()) } else { DynamicImage::ImageRgb8(d.to_rgb8()) }
    } else if img.componentes == 3 {
        DynamicImage::ImageRgb8(RgbImage::from_raw(img.largura, img.altura, img.dados.to_vec())?)
    } else {
        DynamicImage::ImageLuma8(GrayImage::from_raw(img.largura, img.altura, img.dados.to_vec())?)
    })
}

fn em_jpeg(imagem: image::DynamicImage) -> Option<(Vec<u8>, u32, u32)> {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&imagem).ok()?;
    Some((jpeg, imagem.width(), imagem.height()))
}

fn pintar<P: image::Pixel>(i: &mut image::ImageBuffer<P, Vec<P::Subpixel>>, [x0, y0, x1, y1]: [u32; 4], cor: P) {
    for y in y0..y1.min(i.height()) { for x in x0..x1.min(i.width()) { i.put_pixel(x, y, cor); } }
}

/// PDF já interpretado pelo Hayro, para desenhar as páginas sem reler o
/// arquivo a cada uma. Clonar é barato (compartilha o documento entre threads).
#[derive(Clone)]
pub struct Visualizador {
    pdf: std::sync::Arc<hayro::hayro_syntax::Pdf>,
}

impl std::fmt::Debug for Visualizador {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Visualizador({} páginas)", self.total_paginas())
    }
}

/// Página desenhada em RGBA (fundo branco, opaco), pronta para a tela.
#[derive(Debug, Clone)]
pub struct PaginaRaster {
    pub largura: u32,
    pub altura: u32,
    pub rgba: Vec<u8>,
}

impl Visualizador {
    pub fn novo(bytes: Vec<u8>) -> Result<Self, String> {
        let pdf = hayro::hayro_syntax::Pdf::new(bytes)
            .map_err(|e| format!("Nao foi possivel interpretar o PDF para renderizacao: {e:?}"))?;
        Ok(Self { pdf: std::sync::Arc::new(pdf) })
    }

    /// Lê o arquivo direto do disco, mapeado na memória: sem a segunda cópia de ~50 MB que
    /// `novo(pdf.serializar())` fazia ao abrir. `None` se o hayro não ler o arquivo como está
    /// (cifrado, quebrado) ou contar outras páginas: aí vale a cópia regravada pelo lopdf.
    // shortcut: outro programa truncando o arquivo enquanto está aberto derruba o app (SIGBUS); salvar
    // por cima com rename (como o FeatherPDF faz) é seguro. Copiar para a memória se isso aparecer.
    pub fn do_arquivo(caminho: &Path, paginas: u32) -> Option<Self> {
        let arquivo = std::fs::File::open(caminho).ok()?;
        // SAFETY: só leitura; o risco é o arquivo mudar de tamanho por fora (ver shortcut acima).
        let mapa = unsafe { memmap2::Mmap::map(&arquivo) }.ok()?;
        let pdf = hayro::hayro_syntax::Pdf::new(std::sync::Arc::new(mapa)).ok()?;
        (pdf.pages().len() as u32 == paginas).then(|| Self { pdf: std::sync::Arc::new(pdf) })
    }

    pub fn total_paginas(&self) -> u32 { self.pdf.pages().len() as u32 }

    /// Tamanho de cada página em pontos, já com o giro aplicado.
    pub fn tamanhos(&self) -> Vec<(f32, f32)> {
        self.pdf.pages().iter().map(|p| p.render_dimensions()).collect()
    }

    /// Desenha a página (começando em 1). Rodar fora da thread da interface.
    pub fn desenhar(&self, pagina: u32, escala: f32) -> Result<PaginaRaster, String> {
        Self::raster(self.pixmap(pagina, escala, None)?)
    }

    /// Como `desenhar`, mas em pedaços colados: exportar uma A1 a 300 dpi (9933×7016) passa do
    /// limite de um desenho só. Teto de 150 MP (~600 MB) para não derrubar o app.
    pub fn desenhar_grande(&self, pagina: u32, escala: f32) -> Result<PaginaRaster, String> {
        let (pl, pa) = *self.tamanhos().get(pagina.saturating_sub(1) as usize).ok_or("Pagina solicitada nao existe.")?;
        let (l, a) = ((pl * escala) as u32, (pa * escala) as u32);
        if l as u64 * a as u64 <= 20_000_000 && l <= 8000 && a <= 8000 { return self.desenhar(pagina, escala); }
        if l as u64 * a as u64 > 150_000_000 { return Err("Pagina grande demais para esta resolucao; escolha menos dpi.".into()); }
        const LADO: u32 = 4000;
        let mut rgba = vec![0u8; l as usize * a as usize * 4];
        for y0 in (0..a).step_by(LADO as usize) {
            for x0 in (0..l).step_by(LADO as usize) {
                let p = self.desenhar_trecho(pagina, escala, [x0, y0, LADO.min(l - x0), LADO.min(a - y0)])?;
                for y in 0..p.altura.min(a - y0) {
                    let n = p.largura.min(l - x0) as usize * 4;
                    let de = (y * p.largura) as usize * 4;
                    let para = (((y0 + y) * l + x0) as usize) * 4;
                    rgba[para..para + n].copy_from_slice(&p.rgba[de..de + n]);
                }
            }
        }
        Ok(PaginaRaster { largura: l, altura: a, rgba })
    }

    /// A página (começando em 1) em SVG vetorial: texto, traços e imagens como no PDF.
    pub fn svg(&self, pagina: u32) -> Result<String, String> {
        let p = pagina.checked_sub(1).and_then(|i| self.pdf.pages().get(i as usize))
            .ok_or_else(|| "Pagina solicitada nao existe.".to_string())?;
        let cache = hayro_svg::RenderCache::new();
        Ok(hayro_svg::convert(p, &cache, &hayro::hayro_interpret::InterpreterSettings::default(), &hayro_svg::SvgRenderSettings::default()))
    }

    /// Desenha só o trecho `[x, y, largura, altura]`, em pixels da página inteira
    /// já na escala: zoom alto numa página grande não cabe num desenho só.
    pub fn desenhar_trecho(&self, pagina: u32, escala: f32, trecho: [u32; 4]) -> Result<PaginaRaster, String> {
        Self::raster(self.pixmap(pagina, escala, Some(trecho))?)
    }

    fn raster(raster: hayro::vello_cpu::Pixmap) -> Result<PaginaRaster, String> {
        let (largura, altura) = (raster.width() as u32, raster.height() as u32);
        // Fundo branco opaco: o RGBA pré-multiplicado é igual ao comum, e os pixels saem sem cópia.
        Ok(PaginaRaster { largura, altura, rgba: raster.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::AlphaPremultiplied) })
    }

    fn pixmap(&self, pagina: u32, escala: f32, trecho: Option<[u32; 4]>) -> Result<hayro::vello_cpu::Pixmap, String> {
        use hayro::hayro_interpret::util::TransformExt;
        use hayro::hayro_interpret::InterpreterSettings;
        use hayro::kurbo::Affine;
        use hayro::vello_cpu::color::palette::css::WHITE;
        use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
        use hayro::{render_into, RenderCache, RenderSettings};

        if !escala.is_finite() || !(0.1..=16.0).contains(&escala) {
            return Err("Zoom deve estar entre 10% e 1600%.".into());
        }
        let indice = pagina.checked_sub(1)
            .ok_or_else(|| "Pagina deve ser maior que zero.".to_string())? as usize;
        let pagina = self.pdf.pages().get(indice)
            .ok_or_else(|| "Pagina solicitada nao existe.".to_string())?;

        let (largura, altura) = pagina.render_dimensions();
        let [x, y, l, a] = trecho.unwrap_or([0, 0, (largura * escala) as u32, (altura * escala) as u32]);
        // Defesa contra paginas com dimensoes absurdas ou consumo exagerado de memoria.
        if l < 1 || a < 1 || l > 8000 || a > 8000 || l as u64 * a as u64 > 20_000_000 {
            return Err("Pagina muito grande para renderizar neste zoom.".into());
        }

        let cache = RenderCache::new();
        // Trecho grande (zoom alto) com 4 threads; desenho pequeno (miniatura) não compensa o custo de dividir.
        let threads = if l as u64 * a as u64 >= 1_000_000 { 4 } else { 0 };
        let mut ctx = RenderContext::new_with(l as u16, a as u16, hayro::vello_cpu::RenderSettings { num_threads: threads, ..Default::default() });
        let transform = Affine::translate((-(x as f64), -(y as f64)))
            * Affine::scale(escala as f64) * pagina.initial_transform(true).to_kurbo();
        render_into(pagina, &cache, &InterpreterSettings::default(), &RenderSettings::default(), &mut ctx, transform);
        ctx.flush();
        let mut pixmap = Pixmap::new(ctx.width(), ctx.height());
        ctx.render_with(&mut pixmap, &mut Resources::default(),
            RasterizerSettings { target_init: TargetInit::Clear(WHITE), ..Default::default() });
        Ok(pixmap)
    }
}

/// Grava as tabelas achadas como `.xlsx`: cada coluna no tipo escolhido (data, número,
/// moeda); a célula que não se lê no tipo fica como texto, com fundo vermelho claro.
/// `uma_aba`: todas empilhadas numa aba só, cada uma com o título por cima.
pub fn gravar_tabelas_xlsx(tabelas: &[featherpdf_core::TabelaPdf], uma_aba: bool, destino: &Path) -> Result<(), String> {
    use featherpdf_core::{ler_valor, Valor};
    use rust_xlsxwriter::{Color, ExcelDateTime, Format, Workbook};
    let erro = |e: rust_xlsxwriter::XlsxError| format!("Não consegui montar a planilha: {e}");
    let (negrito, data, numero, moeda) = (Format::new().set_bold(), Format::new().set_num_format("dd/mm/yyyy"),
        Format::new().set_num_format("#,##0.##########"), Format::new().set_num_format("\"R$\" #,##0.00"));
    let ruim = Format::new().set_background_color(Color::RGB(0xFDE2E2));
    let mut livro = Workbook::new();
    let mut nomes: Vec<String> = Vec::new();
    let mut linha_atual = 0u32;
    for (i, t) in tabelas.iter().enumerate() {
        if !uma_aba || i == 0 {
            let aba = livro.add_worksheet();
            // Nome da aba: o título (sem os caracteres que o Excel recusa), único, até 31 letras.
            let base: String = if uma_aba { "Tabelas".into() } else if t.titulo.trim().is_empty() { format!("Tabela {}", i + 1) } else {
                t.titulo.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(28).collect::<String>().trim().to_string()
            };
            let mut nome = if base.is_empty() { format!("Tabela {}", i + 1) } else { base };
            let mut n = 2;
            while nomes.iter().any(|x| x.eq_ignore_ascii_case(&nome)) { nome = format!("{} {n}", nome.chars().take(27).collect::<String>()); n += 1; }
            aba.set_name(&nome).map_err(erro)?;
            nomes.push(nome);
            linha_atual = 0;
        }
        let aba = livro.worksheets_mut().last_mut().ok_or("Planilha sem aba.")?;
        if uma_aba {
            let titulo = if t.titulo.is_empty() { format!("Tabela {} (p. {})", i + 1, t.pagina) } else { t.titulo.clone() };
            aba.write_string_with_format(linha_atual, 0, titulo, &negrito).map_err(erro)?;
            linha_atual += 1;
        }
        for (c, h) in t.cabecalho.iter().enumerate() {
            aba.write_string_with_format(linha_atual, c as u16, h, &negrito).map_err(erro)?;
            aba.set_column_width(c as u16, (h.chars().count().max(10) + 2) as f64).map_err(erro)?;
        }
        linha_atual += 1;
        for l in &t.linhas {
            for (c, v) in l.iter().enumerate() {
                let (lin, col) = (linha_atual, c as u16);
                if v.trim().is_empty() { continue; }
                let tipo = t.tipos.get(c).copied().unwrap_or(featherpdf_core::TipoColuna::Texto);
                match ler_valor(v, tipo) {
                    Some(Valor::Numero(x)) => { aba.write_number_with_format(lin, col, x, &numero).map_err(erro)?; }
                    Some(Valor::Moeda(x)) => { aba.write_number_with_format(lin, col, x, &moeda).map_err(erro)?; }
                    Some(Valor::Data(a, m, d)) => {
                        // Data impossível (31/02) sai como o texto lido, sem derrubar a planilha.
                        match ExcelDateTime::from_ymd(a as u16, m as u8, d as u8) {
                            Ok(dt) => { aba.write_datetime_with_format(lin, col, &dt, &data).map_err(erro)?; }
                            Err(_) => { aba.write_string(lin, col, v).map_err(erro)?; }
                        }
                    }
                    Some(Valor::Texto(_)) => { aba.write_string(lin, col, v).map_err(erro)?; }
                    None => { aba.write_string_with_format(lin, col, v, &ruim).map_err(erro)?; }
                }
            }
            linha_atual += 1;
        }
        linha_atual += 1;
    }
    if tabelas.is_empty() { return Err("Nenhuma tabela para gravar.".into()); }
    let bytes = livro.save_to_buffer().map_err(erro)?;
    gravar_atomico(destino, &bytes).map_err(|e| format!("Erro ao salvar a planilha: {e}"))
}

/// As tabelas em CSV (separador `;`, como o Excel em português): uma depois da outra,
/// com o título e uma linha em branco entre elas.
pub fn tabelas_csv(tabelas: &[featherpdf_core::TabelaPdf]) -> String {
    let campo = |s: &str| if s.contains([';', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
    let linha = |v: &[String]| v.iter().map(|x| campo(x)).collect::<Vec<_>>().join(";") + "\n";
    let mut saida = String::new();
    for (i, t) in tabelas.iter().enumerate() {
        if tabelas.len() > 1 {
            if i > 0 { saida.push('\n'); }
            saida += &(campo(if t.titulo.is_empty() { "Tabela" } else { &t.titulo }) + "\n");
        }
        saida += &linha(&t.cabecalho);
        for l in &t.linhas { saida += &linha(l); }
    }
    saida
}

/// Grava as páginas em grade como `.xlsx`: uma aba por página, com as larguras,
/// alturas, mesclas, bordas e tamanhos de letra do PDF. As imagens vão como o
/// recorte da página desenhado a ~216 dpi, no mesmo lugar e tamanho.
pub fn gravar_xlsx(pdf: &DocumentoPdf, paginas: &[featherpdf_core::GradePagina], destino: &Path) -> Result<(), String> {
    use rust_xlsxwriter::{Format, FormatBorder, Image, Workbook};
    let erro = |e: rust_xlsxwriter::XlsxError| format!("Não consegui montar a planilha: {e}");
    let px = |pt: f32| (pt * 96.0 / 72.0).round().max(1.0);
    let vis = if paginas.iter().any(|g| !g.imagens.is_empty()) { Some(Visualizador::novo(pdf.serializar()?)?) } else { None };
    let mut livro = Workbook::new();
    for (i, g) in paginas.iter().enumerate() {
        let aba = livro.add_worksheet();
        aba.set_name(format!("Página {}", i + 1)).map_err(erro)?;
        for (c, &l) in g.larguras.iter().enumerate() {
            aba.set_column_width_pixels(c as u16, (l * 96.0 / 72.0).round().clamp(2.0, 4000.0) as u16).map_err(erro)?;
        }
        for (r, &a) in g.alturas.iter().enumerate() {
            aba.set_row_height(r as u32, a.clamp(1.0, 409.0)).map_err(erro)?;
        }
        let bordas: std::collections::HashMap<(usize, usize), [bool; 4]> = g.bordas.iter().map(|&(l, c, b)| ((l, c), b)).collect();
        let com_bordas = |mut f: Format, b: [bool; 4]| {
            let traco = |sim: bool| if sim { FormatBorder::Thin } else { FormatBorder::None };
            f = f.set_border_top(traco(b[0])).set_border_bottom(traco(b[1]));
            f.set_border_left(traco(b[2])).set_border_right(traco(b[3]))
        };
        for (&(l, c), &b) in &bordas {
            aba.write_blank(l as u32, c as u16, &com_bordas(Format::new(), b)).map_err(erro)?;
        }
        for cel in &g.celulas {
            let primeira = bordas.get(&(cel.linha, cel.coluna)).copied().unwrap_or_default();
            let ultima = bordas.get(&(cel.linha, cel.ate_coluna)).copied().unwrap_or_default();
            let mut f = com_bordas(Format::new(), [primeira[0], primeira[1], primeira[2], ultima[3]])
                .set_font_size(cel.tamanho.clamp(4.0, 72.0));
            if cel.negrito { f = f.set_bold(); }
            if cel.ate_coluna > cel.coluna {
                aba.merge_range(cel.linha as u32, cel.coluna as u16, cel.linha as u32, cel.ate_coluna as u16, &cel.texto, &f).map_err(erro)?;
            } else {
                aba.write_string_with_format(cel.linha as u32, cel.coluna as u16, &cel.texto, &f).map_err(erro)?;
            }
        }
        let Some(vis) = &vis else { continue };
        for im in &g.imagens {
            // Imagem que não desenha (filtro desconhecido) fica de fora; o resto da planilha sai.
            let Some(png) = recorte_png(vis, pdf, g.pagina, im.caixa) else { continue };
            let imagem = Image::new_from_buffer(&png).map_err(erro)?.set_scale_to_size(px(im.largura), px(im.altura), false);
            aba.insert_image_with_offset(im.linha as u32, im.coluna as u16, &imagem, px(im.dx) as u32, px(im.dy) as u32).map_err(erro)?;
        }
    }
    let bytes = livro.save_to_buffer().map_err(erro)?;
    gravar_atomico(destino, &bytes).map_err(|e| format!("Erro ao salvar a planilha: {e}"))
}

/// Recorte da página (caixa no espaço dela) desenhado a ~216 dpi, em PNG.
fn recorte_png(vis: &Visualizador, pdf: &DocumentoPdf, pagina: u32, caixa: [f32; 4]) -> Option<Vec<u8>> {
    let ([x0, _, _, y1], _) = pdf.geometria(pagina).ok()?;
    let [cx0, cy0, cx1, cy1] = caixa;
    let escala = (3.0f32).min(4000.0 / (cx1 - cx0).max(cy1 - cy0));
    let trecho = [((cx0 - x0) * escala).max(0.0) as u32, ((y1 - cy1) * escala).max(0.0) as u32,
        ((cx1 - cx0) * escala).max(1.0) as u32, ((cy1 - cy0) * escala).max(1.0) as u32];
    vis.pixmap(pagina, escala, Some(trecho)).ok()?.into_png().ok()
}

/// Texto para dentro do XML: escapa `& < > "` e tira os caracteres de controle
/// (fonte sem ToUnicode dá `\u{3}` e afins), que o XML não aceita.
fn xml(t: &str) -> String {
    t.chars().filter(|c| *c >= ' ' || matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Grava as páginas em texto corrido como `.docx` (o que o FeatherDocs e o Word
/// abrem): parágrafos com negrito, tamanho, centro e recuo, as imagens no
/// lugar, uma quebra de página entre as páginas do PDF.
pub fn gravar_docx(pdf: &DocumentoPdf, paginas: &[featherpdf_core::PaginaFluxo], destino: &Path) -> Result<(), String> {
    use featherpdf_core::BlocoFluxo;
    use std::fmt::Write as _;
    use std::io::Write as _;
    let twip = |pt: f32| (pt * 20.0).round() as i64;
    let emu = |pt: f32| (pt * 12700.0).round() as i64;
    let vis = if paginas.iter().any(|p| p.blocos.iter().any(|b| matches!(b, BlocoFluxo::Imagem { .. }))) {
        Some(Visualizador::novo(pdf.serializar()?)?)
    } else { None };
    let mut corpo = String::new();
    let mut imagens: Vec<Vec<u8>> = Vec::new();
    for (i, p) in paginas.iter().enumerate() {
        if i > 0 { corpo.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#); }
        for b in &p.blocos {
            match b {
                BlocoFluxo::Paragrafo { trechos, tamanho, centro, recuo, primeira } => {
                    let _ = write!(corpo, r#"<w:p><w:pPr><w:spacing w:after="{}"/>"#, twip(tamanho * 0.5));
                    if *centro { corpo.push_str(r#"<w:jc w:val="center"/>"#); }
                    else if *recuo > 1.0 || primeira.abs() > 1.0 {
                        let primeira = if *primeira >= 0.0 { format!(r#" w:firstLine="{}""#, twip(*primeira)) } else { format!(r#" w:hanging="{}""#, twip(-primeira)) };
                        let _ = write!(corpo, r#"<w:ind w:left="{}"{primeira}/>"#, twip(recuo.max(0.0)));
                    }
                    corpo.push_str("</w:pPr>");
                    for (t, negrito) in trechos {
                        let _ = write!(corpo, r#"<w:r><w:rPr>{}<w:sz w:val="{}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                            if *negrito { "<w:b/>" } else { "" }, (tamanho * 2.0).round() as i64, xml(t));
                    }
                    corpo.push_str("</w:p>");
                }
                BlocoFluxo::Tabela { recuo, celulas } => {
                    let total: i64 = celulas.iter().map(|c| twip(c.largura)).sum();
                    let linha = r#"w:val="single" w:sz="4" w:space="0" w:color="000000""#;
                    let _ = write!(corpo, r#"<w:tbl><w:tblPr><w:tblW w:w="{total}" w:type="dxa"/><w:tblInd w:w="{}" w:type="dxa"/><w:tblBorders><w:top {linha}/><w:left {linha}/><w:bottom {linha}/><w:right {linha}/><w:insideV {linha}/></w:tblBorders><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="40" w:type="dxa"/><w:right w:w="40" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid>"#,
                        twip(recuo.max(0.0)));
                    for c in celulas { let _ = write!(corpo, r#"<w:gridCol w:w="{}"/>"#, twip(c.largura)); }
                    corpo.push_str("</w:tblGrid><w:tr>");
                    for c in celulas {
                        let _ = write!(corpo, r#"<w:tc><w:tcPr><w:tcW w:w="{}" w:type="dxa"/></w:tcPr>"#, twip(c.largura));
                        // Toda célula precisa de pelo menos um parágrafo.
                        if c.paragrafos.is_empty() { corpo.push_str("<w:p/>"); }
                        for (trechos, tamanho) in &c.paragrafos {
                            corpo.push_str(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr>"#);
                            for (t, negrito) in trechos {
                                let _ = write!(corpo, r#"<w:r><w:rPr>{}<w:sz w:val="{}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                                    if *negrito { "<w:b/>" } else { "" }, (tamanho * 2.0).round().max(8.0) as i64, xml(t));
                            }
                            corpo.push_str("</w:p>");
                        }
                        corpo.push_str("</w:tc>");
                    }
                    corpo.push_str("</w:tr></w:tbl>");
                }
                BlocoFluxo::Imagem { caixa, recuo } => {
                    let Some(png) = vis.as_ref().and_then(|v| recorte_png(v, pdf, p.pagina, *caixa)) else { continue };
                    imagens.push(png);
                    let n = imagens.len();
                    let (cx, cy) = (emu(caixa[2] - caixa[0]), emu(caixa[3] - caixa[1]));
                    let _ = write!(corpo, concat!(
                        r#"<w:p><w:pPr><w:ind w:left="{}"/></w:pPr><w:r><w:drawing><wp:inline><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{n}" name="Imagem {n}"/>"#,
                        r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>"#,
                        r#"<pic:nvPicPr><pic:cNvPr id="{n}" name="imagem{n}.png"/><pic:cNvPicPr/></pic:nvPicPr>"#,
                        r#"<pic:blipFill><a:blip r:embed="rImg{n}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
                        r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
                        r#"</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#),
                        twip(recuo.max(0.0)), cx = cx, cy = cy, n = n);
                }
            }
        }
    }
    let primeira = paginas.first().cloned().unwrap_or_default();
    let [me, mc, md, mb] = primeira.margens;
    let _ = write!(corpo, r#"<w:sectPr><w:pgSz w:w="{}" w:h="{}"/><w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr>"#,
        twip(primeira.largura.max(72.0)), twip(primeira.altura.max(72.0)), twip(mc), twip(md), twip(mb), twip(me));
    let documento = format!(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
        r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
        r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
        r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
        r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{}</w:body></w:document>"#), corpo);
    let mut relacoes = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for n in 1..=imagens.len() {
        let _ = write!(relacoes, r#"<Relationship Id="rImg{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/imagem{n}.png"/>"#);
    }
    relacoes.push_str("</Relationships>");
    let arquivos: Vec<(String, Vec<u8>)> = [
        ("[Content_Types].xml".to_string(), concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
            r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
            r#"<Default Extension="png" ContentType="image/png"/>"#,
            r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).as_bytes().to_vec()),
        ("_rels/.rels".to_string(), concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
            r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#).as_bytes().to_vec()),
        ("word/document.xml".to_string(), documento.into_bytes()),
        ("word/_rels/document.xml.rels".to_string(), relacoes.into_bytes()),
    ].into_iter().chain(imagens.into_iter().enumerate().map(|(i, png)| (format!("word/media/imagem{}.png", i + 1), png))).collect();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opcoes = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (nome, bytes) in &arquivos {
        zip.start_file(nome.as_str(), opcoes).and_then(|_| zip.write_all(bytes).map_err(Into::into))
            .map_err(|e| format!("Não consegui montar o documento: {e}"))?;
    }
    let bytes = zip.finish().map_err(|e| format!("Não consegui montar o documento: {e}"))?.into_inner();
    gravar_atomico(destino, &bytes).map_err(|e| format!("Erro ao salvar o documento: {e}"))
}

/// O mesmo texto corrido do .docx em OpenDocument (.odt), para LibreOffice e afins.
pub fn gravar_odt(pdf: &DocumentoPdf, paginas: &[featherpdf_core::PaginaFluxo], destino: &Path) -> Result<(), String> {
    use featherpdf_core::BlocoFluxo;
    use std::fmt::Write as _;
    use std::io::Write as _;
    let vis = if paginas.iter().any(|p| p.blocos.iter().any(|b| matches!(b, BlocoFluxo::Imagem { .. }))) {
        Some(Visualizador::novo(pdf.serializar()?)?)
    } else { None };
    let trechos_em = |saida: &mut String, trechos: &[(String, bool)]| for (t, negrito) in trechos {
        if *negrito { let _ = write!(saida, r#"<text:span text:style-name="Negrito">{}</text:span>"#, xml(t)); }
        else { saida.push_str(&xml(t)); }
    };
    // Um estilo automático por parágrafo, coluna e tabela (o LibreOffice junta os iguais ao salvar).
    let (mut estilos, mut corpo) = (String::new(), String::new());
    let mut imagens: Vec<Vec<u8>> = Vec::new();
    let mut k = 0;
    for (i, p) in paginas.iter().enumerate() {
        if i > 0 { corpo.push_str(r#"<text:p text:style-name="Quebra"/>"#); }
        for b in &p.blocos {
            k += 1;
            match b {
                BlocoFluxo::Paragrafo { trechos, tamanho, centro, recuo, primeira } => {
                    let alinhamento = if *centro { r#" fo:text-align="center""# } else { "" };
                    let (recuo, primeira) = if *centro { (0.0, 0.0) } else { (recuo.max(0.0), *primeira) };
                    let _ = write!(estilos, r#"<style:style style:name="P{k}" style:family="paragraph"><style:paragraph-properties fo:margin-left="{recuo:.1}pt" fo:text-indent="{primeira:.1}pt" fo:margin-bottom="{:.1}pt"{alinhamento}/><style:text-properties fo:font-size="{tamanho:.1}pt"/></style:style>"#, tamanho * 0.5);
                    let _ = write!(corpo, r#"<text:p text:style-name="P{k}">"#);
                    trechos_em(&mut corpo, trechos);
                    corpo.push_str("</text:p>");
                }
                BlocoFluxo::Tabela { recuo, celulas } => {
                    let total: f32 = celulas.iter().map(|c| c.largura).sum();
                    let _ = write!(estilos, r#"<style:style style:name="Tab{k}" style:family="table"><style:table-properties style:width="{total:.1}pt" fo:margin-left="{:.1}pt" table:align="left"/></style:style>"#, recuo.max(0.0));
                    let _ = write!(corpo, r#"<table:table table:style-name="Tab{k}">"#);
                    for (j, c) in celulas.iter().enumerate() {
                        let _ = write!(estilos, r#"<style:style style:name="Col{k}_{j}" style:family="table-column"><style:table-column-properties style:column-width="{:.1}pt"/></style:style>"#, c.largura);
                        let _ = write!(corpo, r#"<table:table-column table:style-name="Col{k}_{j}"/>"#);
                    }
                    corpo.push_str("<table:table-row>");
                    for c in celulas {
                        corpo.push_str(r#"<table:table-cell table:style-name="Celula" office:value-type="string">"#);
                        if c.paragrafos.is_empty() { corpo.push_str("<text:p/>"); }
                        for (trechos, tamanho) in &c.paragrafos {
                            k += 1;
                            let _ = write!(estilos, r#"<style:style style:name="P{k}" style:family="paragraph"><style:text-properties fo:font-size="{:.1}pt"/></style:style>"#, tamanho.max(4.0));
                            let _ = write!(corpo, r#"<text:p text:style-name="P{k}">"#);
                            trechos_em(&mut corpo, trechos);
                            corpo.push_str("</text:p>");
                        }
                        corpo.push_str("</table:table-cell>");
                    }
                    corpo.push_str("</table:table-row></table:table>");
                }
                BlocoFluxo::Imagem { caixa, recuo } => {
                    let Some(png) = vis.as_ref().and_then(|v| recorte_png(v, pdf, p.pagina, *caixa)) else { continue };
                    imagens.push(png);
                    let n = imagens.len();
                    let _ = write!(estilos, r#"<style:style style:name="P{k}" style:family="paragraph"><style:paragraph-properties fo:margin-left="{:.1}pt"/></style:style>"#, recuo.max(0.0));
                    let _ = write!(corpo, r#"<text:p text:style-name="P{k}"><draw:frame draw:name="Imagem {n}" text:anchor-type="as-char" svg:width="{:.1}pt" svg:height="{:.1}pt"><draw:image xlink:href="Pictures/imagem{n}.png" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame></text:p>"#,
                        caixa[2] - caixa[0], caixa[3] - caixa[1]);
                }
            }
        }
    }
    let espacos = concat!(
        r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
        r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" "#,
        r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
        r#"xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.3""#);
    let conteudo = format!(concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {espacos}><office:automatic-styles>"#,
        r#"<style:style style:name="Quebra" style:family="paragraph"><style:paragraph-properties fo:break-before="page"/></style:style>"#,
        r#"<style:style style:name="Negrito" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>"#,
        r#"<style:style style:name="Celula" style:family="table-cell"><style:table-cell-properties fo:padding="2pt" fo:border="0.5pt solid #000000"/></style:style>"#,
        r#"{estilos}</office:automatic-styles><office:body><office:text>{corpo}</office:text></office:body></office:document-content>"#),
        espacos = espacos, estilos = estilos, corpo = corpo);
    let primeira = paginas.first().cloned().unwrap_or_default();
    let [me, mc, md, mb] = primeira.margens;
    let estilos_doc = format!(concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles {espacos}><office:automatic-styles>"#,
        r#"<style:page-layout style:name="Pagina"><style:page-layout-properties fo:page-width="{l:.1}pt" fo:page-height="{a:.1}pt" "#,
        r#"fo:margin-top="{mc:.1}pt" fo:margin-right="{md:.1}pt" fo:margin-bottom="{mb:.1}pt" fo:margin-left="{me:.1}pt"/></style:page-layout></office:automatic-styles>"#,
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="Pagina"/></office:master-styles></office:document-styles>"#),
        espacos = espacos, l = primeira.largura.max(72.0), a = primeira.altura.max(72.0), mc = mc, md = md, mb = mb, me = me);
    let mut manifesto = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">"#,
        r#"<manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>"#,
        r#"<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>"#));
    for n in 1..=imagens.len() { let _ = write!(manifesto, r#"<manifest:file-entry manifest:full-path="Pictures/imagem{n}.png" manifest:media-type="image/png"/>"#); }
    manifesto.push_str("</manifest:manifest>");
    let erro = |e: zip::result::ZipError| format!("Não consegui montar o documento: {e}");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    // O `mimetype` vem primeiro e sem compressão: é como os programas reconhecem o ODF.
    zip.start_file("mimetype", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).map_err(erro)?;
    zip.write_all(b"application/vnd.oasis.opendocument.text").map_err(|e| erro(e.into()))?;
    let opcoes = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let arquivos = [("content.xml".to_string(), conteudo.into_bytes()), ("styles.xml".into(), estilos_doc.into_bytes()), ("META-INF/manifest.xml".into(), manifesto.into_bytes())]
        .into_iter().chain(imagens.into_iter().enumerate().map(|(i, png)| (format!("Pictures/imagem{}.png", i + 1), png)));
    for (nome, bytes) in arquivos {
        zip.start_file(nome.as_str(), opcoes).map_err(erro)?;
        zip.write_all(&bytes).map_err(|e| erro(e.into()))?;
    }
    let bytes = zip.finish().map_err(erro)?.into_inner();
    gravar_atomico(destino, &bytes).map_err(|e| format!("Erro ao salvar o documento: {e}"))
}

/// "IP … · MAC … · PC …" para gravar na assinatura, só quando a pessoa pede.
/// O IP público é perguntado a api.ipify.org (HTTP simples, 3 s de prazo); o resto é local.
// shortcut: o MAC só é lido no Linux (/sys/class/net); no Windows a parte do MAC fica de fora.
pub fn dados_do_computador() -> String {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let ip = (|| {
        let endereco = ("api.ipify.org", 80).to_socket_addrs().ok()?.next()?;
        let mut s = TcpStream::connect_timeout(&endereco, Duration::from_secs(3)).ok()?;
        s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
        s.write_all(b"GET / HTTP/1.0\r\nHost: api.ipify.org\r\n\r\n").ok()?;
        let mut r = String::new();
        s.read_to_string(&mut r).ok()?;
        let corpo = r.split("\r\n\r\n").nth(1)?.trim().to_string();
        corpo.parse::<std::net::IpAddr>().ok().map(|_| corpo)
    })();
    let mac = std::fs::read_dir("/sys/class/net").ok().and_then(|d| {
        let mut nomes: Vec<_> = d.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n != "lo")).collect();
        nomes.sort();
        nomes.iter().filter_map(|p| std::fs::read_to_string(p.join("address")).ok())
            .map(|m| m.trim().to_string()).find(|m| !m.is_empty() && m != "00:00:00:00:00:00")
    });
    let pc = std::env::var("COMPUTERNAME").ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok()).map(|h| h.trim().to_string()).filter(|h| !h.is_empty());
    [ip.map(|v| format!("IP {v}")), mac.map(|v| format!("MAC {v}")), pc.map(|v| format!("PC {v}"))]
        .into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pagina_grande_sai_em_pedacos_colados() {
        let v = Visualizador::novo(criar_a4().unwrap().serializar().unwrap()).unwrap();
        // A4 a 8× = 4763×6736: passa dos 20 MP de um desenho só.
        let r = v.desenhar_grande(1, 8.0).unwrap();
        assert_eq!((r.largura, r.altura), ((595.0f32 * 8.0) as u32, (842.0f32 * 8.0) as u32));
        assert_eq!(r.rgba.len(), r.largura as usize * r.altura as usize * 4);
        assert!(r.rgba.chunks_exact(4).all(|p| p == [255, 255, 255, 255]), "página em branco sai branca, sem buraco");
    }

    #[test]
    fn desenha_lendo_o_arquivo_do_disco() {
        let destino = std::env::temp_dir().join(format!("featherpdf-teste-mapa-{}.pdf", std::process::id()));
        std::fs::write(&destino, criar_a4().unwrap().serializar().unwrap()).unwrap();
        let v = Visualizador::do_arquivo(&destino, 1).expect("lê o PDF mapeado");
        assert!(v.desenhar(1, 0.5).is_ok());
        assert!(Visualizador::do_arquivo(&destino, 2).is_none(), "outra contagem de páginas cai na cópia regravada");
        let _ = std::fs::remove_file(&destino);
    }

    #[test]
    fn imagem_grande_lida_em_pedacos_do_disco_sai_igual() {
        // 3000×3000 (36 MB pronta) não cabe na RAM do cache: vai para o arquivo e volta em pedaços.
        let (l, a) = (3000u32, 3000u32);
        let rgb = (0..l * a).flat_map(|i| { let (x, y) = (i % l, i / l); [(x * 255 / l) as u8, (y * 255 / a) as u8, ((x + y) * 255 / (l + a)) as u8] }).collect();
        let mut pdf = criar_a4().unwrap();
        pdf.so_imagens(vec![(featherpdf_core::ImagemNova::Pixels { largura: l, altura: a, rgb, alfa: None }, l, a)],
            featherpdf_core::AjusteImagem::A4).unwrap();
        let v = Visualizador::novo(pdf.serializar().unwrap()).unwrap();
        let inteira = v.desenhar(1, 6.0).unwrap();
        for t in [[500u32, 1500, 900, 700], [2400, 2600, 800, 600], [0, 1200, 600, 500]] {
            let p = v.desenhar_trecho(1, 6.0, t).unwrap();
            let mut ruins = 0;
            for y in 0..t[3] { for x in 0..t[2] {
                let i = (((y + t[1]) * inteira.largura + x + t[0]) * 4) as usize;
                let j = ((y * p.largura + x) * 4) as usize;
                if (0..3).any(|c| inteira.rgba[i + c].abs_diff(p.rgba[j + c]) > 3) { ruins += 1; }
            }}
            assert_eq!(ruins, 0, "trecho {t:?}");
        }
    }

    #[test]
    fn trecho_com_imagem_sai_igual_a_pagina_inteira() {
        // O hayro em vendor/ pula o que cai fora do trecho e guarda a imagem pronta entre um desenho e outro.
        let (l, a) = (400u32, 300u32);
        let rgb = (0..l * a).flat_map(|i| [(i % l) as u8, (i / l) as u8, ((i * 7) % 251) as u8]).collect();
        let mut pdf = criar_a4().unwrap();
        pdf.so_imagens(vec![(featherpdf_core::ImagemNova::Pixels { largura: l, altura: a, rgb, alfa: None }, l, a)],
            featherpdf_core::AjusteImagem::A4).unwrap();
        let v = Visualizador::novo(pdf.serializar().unwrap()).unwrap();
        let inteira = v.desenhar(1, 1.0).unwrap();
        // O segundo trecho já pega a imagem do cache; o terceiro fica numa borda.
        for t in [[100u32, 200, 300, 250], [150, 250, 300, 250], [0, 0, 80, 60]] {
            let p = v.desenhar_trecho(1, 1.0, t).unwrap();
            for y in 0..t[3] { for x in 0..t[2] {
                let i = (((y + t[1]) * inteira.largura + x + t[0]) * 4) as usize;
                let j = ((y * p.largura + x) * 4) as usize;
                assert!((0..3).all(|c| inteira.rgba[i + c].abs_diff(p.rgba[j + c]) <= 2), "trecho {t:?} pixel {x},{y}");
            }}
        }
    }

    #[test]
    fn webp_sem_perda_volta_igual() {
        let destino = std::env::temp_dir().join(format!("featherpdf-teste-{}.webp", std::process::id()));
        let rgba = vec![10, 20, 30, 255, 200, 100, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        gravar_webp(&destino, 2, 2, rgba.clone()).unwrap();
        let lido = image::open(&destino).unwrap().to_rgba8().into_raw();
        let _ = std::fs::remove_file(&destino);
        assert_eq!(lido, rgba);
    }

    #[test]
    fn odt_abre_como_zip_com_mimetype_primeiro() {
        use featherpdf_core::{BlocoFluxo, CelulaFluxo, PaginaFluxo};
        let pdf = criar_a4().unwrap();
        let pagina = PaginaFluxo { pagina: 1, largura: 595.0, altura: 842.0, margens: [56.0; 4], blocos: vec![
            BlocoFluxo::Paragrafo { trechos: vec![("Título & <só>".into(), true)], tamanho: 16.0, centro: true, recuo: 0.0, primeira: 0.0 },
            BlocoFluxo::Tabela { recuo: 0.0, celulas: vec![CelulaFluxo { largura: 100.0, paragrafos: vec![(vec![("Nome".into(), false)], 10.0)] }] },
        ] };
        let destino = std::env::temp_dir().join(format!("featherpdf-teste-{}.odt", std::process::id()));
        gravar_odt(&pdf, &[pagina.clone(), pagina], &destino).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&destino).unwrap()).unwrap();
        assert_eq!(z.by_index(0).unwrap().name(), "mimetype");
        let mut conteudo = String::new();
        std::io::Read::read_to_string(&mut z.by_name("content.xml").unwrap(), &mut conteudo).unwrap();
        assert!(conteudo.contains("Título &amp; &lt;só&gt;") && conteudo.contains("Quebra") && conteudo.contains("<table:table "));
        std::fs::remove_file(&destino).ok();
    }

    #[test]
    fn tiff_com_duas_paginas_e_svg() {
        let pdf = criar_a4().unwrap();
        let bytes = pdf.serializar().unwrap();
        let vis = Visualizador::novo(bytes).unwrap();
        assert!(vis.svg(1).unwrap().starts_with("<svg"));
        let destino = std::env::temp_dir().join(format!("featherpdf-teste-{}.tif", std::process::id()));
        gravar_tiff(&destino, &[1, 1], 20, |n| vis.desenhar(n, 20.0 / 72.0).map(|r| (r.largura, r.altura, r.rgba))).unwrap();
        let mut d = tiff::decoder::Decoder::new(std::fs::File::open(&destino).unwrap()).unwrap();
        assert!(d.more_images());
        d.next_image().unwrap();
        assert!(!d.more_images());
        std::fs::remove_file(&destino).ok();
    }

    #[test]
    fn papel_cinza_vira_branco() {
        // Papel em 180, tinta em 60: o papel sai branco e a tinta preta; imagem lisa não mexe.
        let mut px = vec![180u8; 100 * 100];
        for v in px.iter_mut().take(1500) { *v = 60; }
        let crua = |d: &[u8]| clarear_fundo_jpeg(featherpdf_core::ImagemCrua { largura: 100, altura: 100, componentes: 1, jpeg: false, dados: d });
        let (jpeg, _, _) = crua(&px).unwrap();
        let d = image::load_from_memory(&jpeg).unwrap().to_luma8();
        assert!(d.get_pixel(50, 90).0[0] > 245 && d.get_pixel(0, 0).0[0] < 10);
        assert!(crua(&[180u8; 100 * 100]).is_none());
    }

    #[test]
    fn a4_do_exportador_compartilhado_e_serializavel() {
        let mut pdf = criar_a4().unwrap();
        assert_eq!(pdf.total_paginas(), 1);
        assert!(pdf.modificado());
        pdf.girar_paginas(&[1], 90).unwrap();
        let bytes = pdf.serializar().unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(DocumentoPdf::de_bytes(&bytes, None).unwrap().total_paginas(), 1);
    }

    #[test]
    fn a4_produz_imagem_png_de_visualizacao() {
        let pdf = criar_a4().unwrap();
        let r = Visualizador::novo(pdf.serializar().unwrap()).unwrap().desenhar(1, 1.0).unwrap();
        assert!(r.largura > 0 && r.altura > 0 && r.rgba.len() == (r.largura * r.altura * 4) as usize);
    }

    #[test]
    fn entrada_de_renderizacao_invalida_rejeitada() {
        let pdf = criar_a4().unwrap();
        let bytes = pdf.serializar().unwrap();
        let vis = Visualizador::novo(bytes.clone()).unwrap();
        assert!(vis.desenhar(0, 1.0).is_err());
        assert!(vis.desenhar(2, 1.0).is_err());
        assert!(vis.desenhar(1, 100.0).is_err());
        assert_eq!(vis.total_paginas(), 1);
        let (l, a) = vis.tamanhos()[0];
        assert!((l - 595.0).abs() < 1.0 && (a - 842.0).abs() < 1.0);
        let raster = vis.desenhar(1, 0.5).unwrap();
        assert_eq!(raster.rgba.len(), (raster.largura * raster.altura * 4) as usize);
        assert!(Visualizador::novo(b"nao e pdf".to_vec()).is_err());
    }

    /// Pasta temporária só deste teste; apagada ao sair.
    struct Pasta(PathBuf);
    impl Pasta {
        fn nova(nome: &str) -> Self {
            let p = std::env::temp_dir().join(format!("featherpdf-teste-{nome}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Pasta(p)
        }
    }
    impl Drop for Pasta { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    #[test]
    fn salvar_como_grava_e_confirma_origem() {
        let pasta = Pasta::nova("salvar");
        let destino = pasta.0.join("novo.pdf");
        let mut pdf = criar_a4().unwrap();
        salvar_como(&mut pdf, &destino).unwrap();
        assert!(destino.exists());
        assert!(!pdf.modificado());
        assert_eq!(pdf.origem(), Some(destino.as_path()));
    }

    #[test]
    fn salvar_por_cima_substitui_o_arquivo() {
        let pasta = Pasta::nova("sobrescrever");
        let destino = pasta.0.join("doc.pdf");
        std::fs::write(&destino, b"conteudo antigo").unwrap();
        salvar_como(&mut criar_a4().unwrap(), &destino).unwrap();
        assert!(std::fs::read(&destino).unwrap().starts_with(b"%PDF-"));
    }

    #[cfg(unix)]
    #[test]
    fn salvar_mantem_permissao_e_link() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = Pasta::nova("permissao");
        let real = pasta.0.join("real.pdf");
        let link = pasta.0.join("link.pdf");
        std::fs::write(&real, b"antigo").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        salvar_como(&mut criar_a4().unwrap(), &link).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(std::fs::read(&real).unwrap().starts_with(b"%PDF-"));
        assert_eq!(std::fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn abrir_recusa_arquivo_sem_cabecalho_pdf() {
        let pasta = Pasta::nova("cabecalho");
        let falso = pasta.0.join("falso.pdf");
        std::fs::write(&falso, b"isto nao e um pdf").unwrap();
        assert!(abrir(&falso).is_err());
    }

    #[test]
    fn salvar_e_abrir_mantem_as_paginas() {
        let pasta = Pasta::nova("ida-e-volta");
        let destino = pasta.0.join("tres.pdf");
        let mut pdf = criar_a4().unwrap();
        pdf.duplicar_paginas(&[1]).unwrap();
        pdf.duplicar_paginas(&[1]).unwrap();
        let total = pdf.total_paginas();
        salvar_como(&mut pdf, &destino).unwrap();
        assert_eq!(abrir(&destino).unwrap().total_paginas(), total);
    }

    #[test]
    fn xml_tira_caracteres_de_controle() {
        assert_eq!(xml("a\u{3}b\u{0}<\t\n"), "ab&lt;\t\n");
    }
}











