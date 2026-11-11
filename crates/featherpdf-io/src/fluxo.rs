//! Fluxos em blocos: uma sequência de passos que roda igual em cada PDF de uma pasta
//! (tirar páginas em branco → limpar o fundo → numerar → comprimir…). Cada bloco é uma
//! função que já existe no app; aqui só se encadeia.

use featherpdf_core::{CabecalhoRodape, DocumentoPdf, FormatoNumero, MarcaDagua, Numeracao, OpcoesCompressao, Padrao, PosicaoNumero};

/// Níveis do bloco Comprimir: nome, dpi das fotos e qualidade JPEG.
pub const NIVEIS_COMPRESSAO: [(&str, f32, u8); 3] = [("Leve", 200.0, 85), ("Padrão", 150.0, 75), ("Forte", 100.0, 60)];
/// Quais páginas o bloco Girar gira (a orientação é a que aparece na tela).
pub const PAGINAS_GIRAR: [&str; 5] = ["Todas", "Só as em pé", "Só as deitadas", "Ímpares", "Pares"];
/// Tamanhos do bloco Normalizar: nome e largura × altura em pontos (em pé).
pub const TAMANHOS_PAGINA: [(&str, f32, f32); 4] = [("A4", 595.0, 842.0), ("A3", 842.0, 1191.0), ("A1", 1684.0, 2384.0), ("Carta", 612.0, 792.0)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bloco {
    TirarBrancas, ClarearFundo, LimparCaderno, Numerar, LimparMetadados, EmbutirFontes, Linearizar, PdfA,
    /// Índice em [`NIVEIS_COMPRESSAO`].
    Comprimir(u8),
    /// Índice em [`TAMANHOS_PAGINA`].
    Normalizar(u8),
    /// Texto na diagonal, cinza e translúcido, numa camada própria.
    MarcaDagua(String),
    /// Texto centrado no alto e no pé de cada página ({pagina}, {total}, {data}, {arquivo}).
    Cabecalho { cabecalho: String, rodape: String },
    /// Graus no sentido horário (90, 180 ou 270), nas páginas escolhidas (índice em [`PAGINAS_GIRAR`]).
    Girar { graus: i64, quais: u8 },
    /// Quais de [`Padrao::TODOS`] tarjar, na mesma ordem.
    TarjarPadroes([bool; 6]),
}

impl Bloco {
    pub fn nome(&self) -> &'static str {
        match self {
            Bloco::TirarBrancas => "Tirar páginas em branco",
            Bloco::ClarearFundo => "Clarear fundo do scan",
            Bloco::LimparCaderno => "Limpar foto de caderno",
            Bloco::Numerar => "Numerar páginas",
            Bloco::LimparMetadados => "Apagar metadados",
            Bloco::EmbutirFontes => "Embutir fontes",
            Bloco::Comprimir(_) => "Comprimir",
            Bloco::Normalizar(_) => "Normalizar tamanho",
            Bloco::PdfA => "Converter para PDF/A",
            Bloco::MarcaDagua(_) => "Marca d'água",
            Bloco::Cabecalho { .. } => "Cabeçalho e rodapé",
            Bloco::Linearizar => "Linearizar (web)",
            Bloco::Girar { .. } => "Girar páginas",
            Bloco::TarjarPadroes(_) => "Tarjar padrões",
        }
    }
}

/// Roda os blocos em ordem e devolve os bytes do PDF pronto e o que cada bloco fez.
pub fn executar_fluxo(mut pdf: DocumentoPdf, blocos: &[Bloco]) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut relato = Vec::new();
    let mut pronto: Option<Vec<u8>> = None;
    for b in blocos {
        // Comprimir e linearizar devolvem bytes: o bloco seguinte trabalha sobre eles.
        let mut tamanho_anterior = None;
        if let Some(bytes) = pronto.take() { pdf = DocumentoPdf::de_bytes(&bytes, None)?; tamanho_anterior = Some(bytes.len()); }
        let todas: Vec<u32> = (1..=pdf.total_paginas()).collect();
        relato.push(match b {
            Bloco::TirarBrancas => {
                let brancas: Vec<u32> = todas.iter().copied().filter(|&p| pdf.objetos(p).is_ok_and(|o| o.is_empty())).collect();
                if !brancas.is_empty() && brancas.len() < todas.len() { pdf.excluir_paginas(&brancas)?; }
                format!("{} página(s) em branco tiradas", if brancas.len() < todas.len() { brancas.len() } else { 0 })
            }
            Bloco::ClarearFundo => format!("{} foto(s) clareadas", pdf.limpar_fundo(&todas, &crate::clarear_fundo_jpeg)),
            Bloco::LimparCaderno => format!("{} foto(s) de caderno limpas", pdf.limpar_fundo(&todas, &crate::limpar_caderno_jpeg)),
            Bloco::Numerar => {
                pdf.numerar(Numeracao { formato: FormatoNumero::PaginaDeTotal, posicao: PosicaoNumero::RodapeCentro, a_partir_de: 1, primeiro: 1, tamanho: 9.0 })?;
                format!("{} página(s) numeradas", todas.len())
            }
            Bloco::LimparMetadados => { pdf.limpar_metadados(); "metadados apagados".into() }
            Bloco::EmbutirFontes => format!("{} fonte(s) embutidas", crate::embutir_fontes_do_sistema(&mut pdf)?.0.len()),
            Bloco::Comprimir(nivel) => {
                let (_, dpi, qualidade) = NIVEIS_COMPRESSAO[(*nivel as usize).min(2)];
                let bytes = pdf.comprimido(OpcoesCompressao { dpi: Some(dpi), qualidade, limpar: true, anexos: false, zopfli: false }, &crate::reamostrar_jpeg)?;
                // O tamanho de antes só quando um bloco anterior já gravou: medir gravando de novo custa uma gravação inteira.
                let depois = featherpdf_core::tamanho(bytes.len());
                let r = match tamanho_anterior { Some(antes) => format!("comprimido de {} para {depois}", featherpdf_core::tamanho(antes)), None => format!("comprimido para {depois}") };
                pronto = Some(bytes);
                r
            }
            Bloco::Linearizar => { pronto = Some(pdf.linearizado()?); "linearizado".into() }
            Bloco::Girar { graus, quais } => {
                let deitada = |p: u32| pdf.geometria(p).is_ok_and(|([x0, y0, x1, y1], g)| {
                    let (l, a) = if g.rem_euclid(180) == 90 { (y1 - y0, x1 - x0) } else { (x1 - x0, y1 - y0) };
                    l > a
                });
                let escolhidas: Vec<u32> = todas.iter().copied().filter(|&p| match quais {
                    1 => !deitada(p), 2 => deitada(p), 3 => p % 2 == 1, 4 => p % 2 == 0, _ => true,
                }).collect();
                if !escolhidas.is_empty() { pdf.girar_paginas(&escolhidas, *graus)?; }
                format!("{} de {} página(s) giradas {graus}°", escolhidas.len(), todas.len())
            }
            Bloco::Normalizar(t) => {
                let (nome, l, a) = TAMANHOS_PAGINA[(*t as usize).min(TAMANHOS_PAGINA.len() - 1)];
                format!("{} página(s) em {nome}", pdf.padronizar_tamanho(l, a)?)
            }
            Bloco::PdfA => {
                // PDF/A exige fonte embutida: as do sistema entram antes; uma que falte vira o erro da conversão.
                let _ = crate::embutir_fontes_do_sistema(&mut pdf);
                pdf.converter_pdfa(crate::PERFIL_SRGB)?;
                "convertido para PDF/A-2b".into()
            }
            Bloco::MarcaDagua(texto) => {
                let n = pdf.marca_dagua(&MarcaDagua { texto: texto.clone(), tamanho: 60.0, cor: [0.5; 3], opacidade: 0.25, angulo: 45.0, paginas: vec![], em_camada: true })?;
                format!("marca d'água em {n} página(s)")
            }
            Bloco::Cabecalho { cabecalho, rodape } => {
                let n = pdf.cabecalho_rodape(&CabecalhoRodape { cabecalho: [String::new(), cabecalho.clone(), String::new()],
                    rodape: [String::new(), rodape.clone(), String::new()], tamanho: 9.0, margem: 24.0, cor: [0.2; 3], paginas: vec![],
                    arquivo: String::new(), data: featherpdf_core::hoje() })?;
                format!("cabeçalho e rodapé em {n} página(s)")
            }
            Bloco::TarjarPadroes(quais) => {
                let padroes: Vec<Padrao> = Padrao::TODOS.into_iter().zip(*quais).filter(|(_, q)| *q).map(|(p, _)| p).collect();
                let marcas: Vec<(u32, [f32; 4])> = if padroes.is_empty() { vec![] }
                    else { pdf.achar_para_tarja("", &padroes).into_iter().map(|a| (a.pagina, a.caixa)).collect() };
                if !marcas.is_empty() { pdf.tarjar(&marcas, "", &crate::apagar_pixels_jpeg)?; }
                format!("{} trecho(s) tarjados", marcas.len())
            }
        });
    }
    Ok((match pronto { Some(b) => b, None => pdf.serializar()? }, relato))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn brancas_saem_numera_e_comprime() {
        let mut pdf = crate::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "Capa", 20.0, false, [0.0; 3]).unwrap();
        pdf.inserir_em_branco(2).unwrap();
        pdf.inserir_em_branco(3).unwrap();
        pdf.inserir_texto(3, 72.0, 700.0, "Fim", 20.0, false, [0.0; 3]).unwrap();
        assert_eq!(pdf.total_paginas(), 3);
        let (bytes, relato) = executar_fluxo(pdf, &[Bloco::TirarBrancas, Bloco::Numerar, Bloco::Comprimir(1), Bloco::LimparMetadados]).unwrap();
        let pronto = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        assert_eq!(pronto.total_paginas(), 2, "{relato:?}");
        assert!(pronto.texto_pagina(2).unwrap().contains("2"), "numerada: {:?}", pronto.texto_pagina(2));
        assert_eq!(relato.len(), 4);
    }

    #[test]
    fn gira_so_as_paginas_escolhidas() {
        // Página 1 em pé, página 2 deitada (já girada 90°).
        let mut pdf = crate::criar_a4().unwrap();
        pdf.inserir_em_branco(2).unwrap();
        pdf.girar_paginas(&[2], 90).unwrap();
        let giros = |b: &[u8]| { let p = DocumentoPdf::de_bytes(b, None).unwrap(); (p.geometria(1).unwrap().1, p.geometria(2).unwrap().1) };
        let (b, _) = executar_fluxo(pdf.clone(), &[Bloco::Girar { graus: 90, quais: 2 }]).unwrap();
        assert_eq!(giros(&b), (0, 180), "só a deitada gira");
        let (b, _) = executar_fluxo(pdf.clone(), &[Bloco::Girar { graus: 90, quais: 1 }]).unwrap();
        assert_eq!(giros(&b), (90, 90), "só a em pé gira");
        let (b, _) = executar_fluxo(pdf, &[Bloco::Girar { graus: 180, quais: 4 }]).unwrap();
        assert_eq!(giros(&b), (0, 270), "só a par gira");
    }

    #[test]
    fn gira_e_tarja_padroes() {
        let mut pdf = crate::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "CPF 123.456.789-09", 14.0, false, [0.0; 3]).unwrap();
        let (bytes, relato) = executar_fluxo(pdf, &[Bloco::Girar { graus: 90, quais: 0 }, Bloco::TarjarPadroes([true; 6])]).unwrap();
        let pronto = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        assert!(!pronto.texto_pagina(1).unwrap().contains("123"), "{relato:?}");
        assert_eq!(pronto.geometria(1).unwrap().1, 90);
    }

    #[test]
    fn comprime_em_niveis_e_forte_fica_menor() {
        // Foto com ruído, para a qualidade JPEG fazer diferença.
        let (l, a) = (1200u16, 1200u16);
        let rgb: Vec<u8> = (0..l as u32 * a as u32 * 3).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8 / 2 + (i / 3 % 1200 / 10) as u8).collect();
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 95).encode(&rgb, l, a, jpeg_encoder::ColorType::Rgb).unwrap();
        let mut pdf = crate::criar_a4().unwrap();
        pdf.inserir_imagem(1, [72.0, 300.0, 472.0, 700.0], featherpdf_core::ImagemNova::Jpeg { bytes: jpeg, largura: l as u32, altura: a as u32, cinza: false }).unwrap();
        let leve = executar_fluxo(pdf.clone(), &[Bloco::Comprimir(0)]).unwrap().0.len();
        let forte = executar_fluxo(pdf, &[Bloco::Comprimir(2)]).unwrap().0.len();
        assert!(forte < leve, "forte {forte} >= leve {leve}");
    }

    #[test]
    fn normaliza_marca_e_cabecalho() {
        let pdf = crate::criar_a4().unwrap();
        let blocos = [Bloco::Normalizar(1), Bloco::MarcaDagua("PRELIMINAR".into()),
            Bloco::Cabecalho { cabecalho: "Obra Loft".into(), rodape: "Pág {pagina} de {total}".into() }];
        let (bytes, relato) = executar_fluxo(pdf, &blocos).unwrap();
        let pronto = DocumentoPdf::de_bytes(&bytes, None).unwrap();
        let ([x0, y0, x1, y1], _) = pronto.geometria(1).unwrap();
        assert_eq!(((x1 - x0).round(), (y1 - y0).round()), (842.0, 1191.0), "{relato:?}");
        let texto = pronto.texto_pagina(1).unwrap();
        assert!(texto.contains("PRELIMINAR") && texto.contains("Obra Loft") && texto.contains("Pág 1 de 1"), "{texto:?}");
    }

    #[test]
    fn converte_para_pdfa() {
        let (bytes, _) = executar_fluxo(crate::criar_a4().unwrap(), &[Bloco::PdfA]).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("pdfaid"), "sem o XMP de PDF/A");
    }
}
