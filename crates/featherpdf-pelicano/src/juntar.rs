//! Juntar arquivos num PDF novo (PDFs e imagens PNG/JPG/TIFF, cada imagem vira uma página; o TIFF de
//! várias páginas do scanner vira várias),
//! criar PDF a partir de imagens e intercalar frente e verso de dois PDFs escaneados.

use std::path::{Path, PathBuf};

use featherpdf_core::{AjusteImagem, DocumentoPdf};
use featherpdf_io::Visualizador;

use pelicano::*;
use crate::{App, Msg};

pub const EXTENSOES_IMAGEM: [&str; 6] = ["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"];
/// O que abre como PDF novo: as imagens e o TIFF (que não serve para carimbo, só para virar páginas).
pub const EXTENSOES_PAGINAS: [&str; 10] = ["png", "jpg", "jpeg", "tif", "tiff", "PNG", "JPG", "JPEG", "TIF", "TIFF"];

#[derive(Debug, Clone)]
pub enum Jt {
    /// Escolher arquivos para um PDF novo (`true`: só imagens).
    Escolher(bool),
    Escolhidos(Option<Vec<PathBuf>>),
    /// Frente e verso em dois arquivos: escolhe as frentes, depois os versos.
    FrenteVerso { invertidos: bool },
    Frentes(bool, Option<PathBuf>),
    Versos(bool, PathBuf, Option<PathBuf>),
    Pronto(Result<(DocumentoPdf, Visualizador), String>),
}

/// Imagem ou texto: abre (e junta) como um PDF novo.
pub fn vira_pdf(caminho: &Path) -> bool {
    caminho.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSOES_PAGINAS.contains(&e) || e.eq_ignore_ascii_case("txt"))
}

fn e_texto(caminho: &Path) -> bool {
    caminho.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("txt"))
}

/// Teto do `.txt` aberto como PDF: 20 MB já dão milhares de páginas.
const TETO_TEXTO: u64 = 20 << 20;

/// Um `.txt` vira PDF A4 com o texto corrido. UTF-8; o que não for UTF-8 é lido como Windows-1252
/// (o Bloco de Notas antigo grava assim).
fn texto_para_pdf(caminho: &Path) -> Result<DocumentoPdf, String> {
    let tamanho = std::fs::metadata(caminho).map_err(|e| e.to_string())?.len();
    if tamanho > TETO_TEXTO { return Err("Arquivo de texto grande demais para virar PDF (mais de 20 MB).".into()); }
    let bytes = featherpdf_io::ler_bytes(caminho)?;
    let texto = texto_do_arquivo(&bytes);
    let mut pdf = featherpdf_io::criar_a4()?;
    pdf.escrever_texto_corrido(&texto)?;
    Ok(pdf)
}

/// O texto de um `.txt`: UTF-8 (com ou sem BOM), UTF-16 com BOM (o "Unicode" do Bloco de Notas e o `>`
/// do PowerShell 5) e, no resto, Windows-1252.
fn texto_do_arquivo(bytes: &[u8]) -> String {
    let utf16 = |pares: &[u8], de: fn([u8; 2]) -> u16| String::from_utf16_lossy(&pares.chunks_exact(2).map(|p| de([p[0], p[1]])).collect::<Vec<_>>());
    if let Some(r) = bytes.strip_prefix(&[0xFF, 0xFE][..]) { return utf16(r, u16::from_le_bytes); }
    if let Some(r) = bytes.strip_prefix(&[0xFE, 0xFF][..]) { return utf16(r, u16::from_be_bytes); }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(t) => t.to_string(),
        Err(_) => bytes.iter().map(|&b| featherpdf_core::de_win_ansi(b).unwrap_or(b as char)).collect(),
    }
}

fn e_tiff(caminho: &Path) -> bool {
    caminho.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("tif") || e.eq_ignore_ascii_case("tiff"))
}

/// Abre um PDF, ou faz um PDF de uma página com a imagem (bloqueante).
pub fn documento_de_arquivo(caminho: &Path) -> Result<DocumentoPdf, String> {
    if !vira_pdf(caminho) { return featherpdf_io::abrir(caminho); }
    let mut pdf = convertido(caminho)?;
    // relatorio.txt vira "relatorio.pdf" na aba e no "Salvar como".
    if let Some(base) = caminho.file_stem() { pdf.sugerir_nome(format!("{}.pdf", base.to_string_lossy())); }
    Ok(pdf)
}

/// Imagem ou texto → PDF novo (bloqueante).
fn convertido(caminho: &Path) -> Result<DocumentoPdf, String> {
    if e_texto(caminho) { return texto_para_pdf(caminho); }
    let bytes = featherpdf_io::ler_bytes(caminho)?;
    let imagens = if e_tiff(caminho) {
        featherpdf_io::paginas_tiff(&bytes)?.into_iter()
            .map(|(l, a, rgb)| (featherpdf_core::ImagemNova::Pixels { largura: l, altura: a, rgb, alfa: None }, l, a)).collect()
    } else {
        let (imagem, l, a) = crate::inserir::ler_imagem(bytes)?;
        vec![(std::sync::Arc::unwrap_or_clone(imagem), l, a)]
    };
    let mut pdf = featherpdf_io::criar_a4()?;
    pdf.so_imagens(imagens, AjusteImagem::A4)?;
    Ok(pdf)
}

/// Um PDF novo com todos os arquivos, na ordem dada (bloqueante).
pub fn juntar_arquivos(caminhos: &[PathBuf]) -> Result<DocumentoPdf, String> {
    let (primeiro, resto) = caminhos.split_first().ok_or("Nenhum arquivo escolhido.")?;
    let ler = |c: &PathBuf| documento_de_arquivo(c).map_err(|e| format!("{}: {e}", c.display()));
    let mut pdf = ler(primeiro)?;
    for c in resto {
        let fim = pdf.total_paginas() + 1;
        pdf.inserir_pdf(ler(c)?, fim)?;
    }
    Ok(pdf)
}

/// O resultado nasce sem origem (e modificado): Ctrl+S pede um nome em vez de gravar por cima do primeiro arquivo.
fn com_vista(pdf: DocumentoPdf) -> Result<(DocumentoPdf, Visualizador), String> {
    let bytes = pdf.serializar()?;
    Ok((DocumentoPdf::de_bytes(&bytes, None)?, Visualizador::novo(bytes)?))
}

impl App {
    pub fn update_juntar(&mut self, m: Jt) -> Task<Msg> {
        match m {
            Jt::Escolher(so_imagens) => return Task::perform(async move {
                let d = rfd::AsyncFileDialog::new();
                let d = if so_imagens { d.add_filter("Imagens", &EXTENSOES_PAGINAS) }
                    else { d.add_filter("PDF e imagens", &["pdf", "PDF", "png", "jpg", "jpeg", "tif", "tiff", "PNG", "JPG", "JPEG", "TIF", "TIFF"]) };
                d.pick_files().await.map(|v| v.into_iter().map(|h| h.path().to_path_buf()).collect())
            }, |v| Msg::Juntar(Jt::Escolhidos(v))),
            Jt::Escolhidos(None) | Jt::Frentes(_, None) | Jt::Versos(_, _, None) => {}
            Jt::Escolhidos(Some(mut caminhos)) => {
                // O seletor devolve em qualquer ordem: em ordem de nome, como foto_01, foto_02…
                caminhos.sort();
                self.status = format!("Juntando {} arquivo(s)…", caminhos.len());
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || juntar_arquivos(&caminhos).and_then(com_vista)).await
                        .unwrap_or_else(|e| Err(format!("Falha ao juntar: {e}")))
                }, |r| Msg::Juntar(Jt::Pronto(r)));
            }
            Jt::FrenteVerso { invertidos } => {
                self.status = "Escolha o PDF das frentes.".into();
                return Task::perform(async {
                    rfd::AsyncFileDialog::new().set_title("PDF com as frentes").add_filter("PDF", &["pdf", "PDF"]).pick_file().await.map(|h| h.path().to_path_buf())
                }, move |c| Msg::Juntar(Jt::Frentes(invertidos, c)));
            }
            Jt::Frentes(invertidos, Some(frentes)) => {
                self.status = "Agora o PDF dos versos.".into();
                return Task::perform(async {
                    rfd::AsyncFileDialog::new().set_title("PDF com os versos").add_filter("PDF", &["pdf", "PDF"]).pick_file().await.map(|h| h.path().to_path_buf())
                }, move |c| Msg::Juntar(Jt::Versos(invertidos, frentes.clone(), c)));
            }
            Jt::Versos(invertidos, frentes, Some(versos)) => {
                self.status = "Intercalando frentes e versos…".into();
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let mut pdf = featherpdf_io::abrir(&frentes)?;
                        let outro = featherpdf_io::abrir(&versos)?;
                        if outro.total_paginas() != pdf.total_paginas() {
                            return Err(format!("As frentes têm {} página(s) e os versos {}: precisam ser iguais.", pdf.total_paginas(), outro.total_paginas()));
                        }
                        let fim = pdf.total_paginas() + 1;
                        pdf.inserir_pdf(outro, fim)?;
                        pdf.intercalar_frente_verso(invertidos)?;
                        com_vista(pdf)
                    }).await.unwrap_or_else(|e| Err(format!("Falha ao intercalar: {e}")))
                }, |r| Msg::Juntar(Jt::Pronto(r)));
            }
            Jt::Pronto(Ok((pdf, vis))) => {
                let tarefa = self.entrar(pdf, vis);
                self.status = format!("PDF novo com {} página(s). Salve para escolher o nome.", self.total_paginas());
                return tarefa;
            }
            Jt::Pronto(Err(e)) => self.status = e,
        }
        Task::none()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn juntado_nasce_sem_origem() {
        let bytes = featherpdf_io::criar_a4().unwrap().serializar().unwrap();
        let pdf = DocumentoPdf::de_bytes(&bytes, Some(PathBuf::from("/tmp/primeiro.pdf"))).unwrap();
        let (novo, _) = com_vista(pdf).unwrap();
        assert!(novo.origem().is_none());
        assert!(novo.modificado());
    }

    #[test]
    fn tiff_de_varias_paginas_vira_um_pdf_com_todas() {
        use tiff::encoder::{colortype::Gray8, TiffEncoder};
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut t = TiffEncoder::new(&mut bytes).unwrap();
        for _ in 0..3 { t.write_image::<Gray8>(40, 60, &[200; 2400]).unwrap(); }
        drop(t);
        let caminho = std::env::temp_dir().join(format!("featherpdf-scan-{}.TIF", std::process::id()));
        std::fs::write(&caminho, bytes.get_ref()).unwrap();
        assert!(vira_pdf(&caminho));
        let pdf = documento_de_arquivo(&caminho).unwrap();
        std::fs::remove_file(&caminho).ok();
        assert_eq!(pdf.total_paginas(), 3);
    }

    #[test]
    fn txt_vira_pdf_em_utf8_e_windows_1252() {
        let pasta = std::env::temp_dir();
        let utf8 = pasta.join(format!("featherpdf-texto-{}.txt", std::process::id()));
        std::fs::write(&utf8, "\u{FEFF}Relatório de ação\nlinha 2").unwrap();
        let antigo = pasta.join(format!("featherpdf-texto-{}-1252.TXT", std::process::id()));
        std::fs::write(&antigo, b"Fun\xe7\xe3o").unwrap();
        assert!(vira_pdf(&utf8) && vira_pdf(&antigo));
        let a = documento_de_arquivo(&utf8).unwrap();
        let b = documento_de_arquivo(&antigo).unwrap();
        std::fs::remove_file(&utf8).ok();
        std::fs::remove_file(&antigo).ok();
        assert!(a.texto_pagina(1).unwrap().contains("Relatório de ação"));
        assert_eq!(a.nome_sugerido(), Some(format!("featherpdf-texto-{}.pdf", std::process::id()).as_str()));
        assert!(b.texto_pagina(1).unwrap().contains("Função"));
    }

    #[test]
    fn txt_em_utf16_do_bloco_de_notas() {
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain("Relatório".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain("Relatório".encode_utf16().flat_map(u16::to_be_bytes)).collect();
        assert_eq!(texto_do_arquivo(&le), "Relatório");
        assert_eq!(texto_do_arquivo(&be), "Relatório");
    }
}
