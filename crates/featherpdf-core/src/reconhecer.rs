//! Reconhecer assinaturas: acha onde o documento pede assinatura (a linha `______`, o rótulo
//! "Assinatura", "Rubrica", o campo de assinatura digital) e, pela página desenhada, diz se
//! tem tinta ali ou se ficou em branco. Também aponta a página sem rubrica quando as outras têm.
//! Página escaneada precisa do OCR antes (os rótulos vêm da camada de texto).

use crate::{DocumentoPdf, TipoCampo};

#[derive(Debug, Clone, PartialEq)]
pub struct LocalAssinatura {
    pub pagina: u32,
    /// O que fez achar o lugar ("Contratante", "Rubrica", "linha", "campo Sig1").
    pub rotulo: String,
    /// Onde a assinatura deve estar, em pontos do PDF.
    pub caixa: [f32; 4],
    /// Texto impresso dentro da área (não conta como tinta).
    pub ignorar: Vec<[f32; 4]>,
    /// Campo de assinatura digital: já se sabe se está assinado, sem olhar a tinta.
    pub digital: Option<bool>,
}

const PALAVRAS: [&str; 6] = ["assinatura", "assinado", "rubrica", "signature", "testemunha", "visto"];
/// Altura da área acima da linha (pt): uma assinatura costuma passar um pouco da linha.
const ALTURA: f32 = 34.0;

impl DocumentoPdf {
    pub fn locais_de_assinatura(&self) -> Result<Vec<LocalAssinatura>, String> {
        let mut locais: Vec<LocalAssinatura> = Vec::new();
        for p in 1..=self.total_paginas() {
            let (itens, _, segmentos) = self.conteudo(p)?;
            let mut daqui: Vec<(String, [f32; 4])> = Vec::new();
            for it in &itens {
                let minusculo = it.texto.to_lowercase();
                let [x0, y0, x1, y1] = it.caixa;
                if let Some(i) = it.texto.find("________") {
                    // Só a parte sublinhada: "Contratante: ______" pega a área em cima dos traços.
                    let fim = it.texto.rfind('_').unwrap_or(i) + 1;
                    let n = it.texto.chars().count().max(1) as f32;
                    let (a, b) = (it.texto[..i].chars().count() as f32 / n, it.texto[..fim].chars().count() as f32 / n);
                    let rotulo = it.texto[..i].trim().trim_end_matches(':').trim();
                    let caixa = [x0 + (x1 - x0) * a, y0 + 1.0, x0 + (x1 - x0) * b, y0 + 1.0 + ALTURA];
                    daqui.push((if rotulo.is_empty() { "linha".into() } else { rotulo.to_string() }, caixa));
                } else if PALAVRAS.iter().any(|w| minusculo.contains(w)) && it.texto.chars().count() <= 60 {
                    // Rótulo logo embaixo de uma linha desenhada: a área fica em cima da linha.
                    let linha = segmentos.iter().filter(|s| (s[3] - s[1]).abs() < 1.0 && s[2] - s[0] >= 60.0
                        && s[1] >= y1 - 2.0 && s[1] <= y1 + 20.0 && s[0] < x1 && s[2] > x0)
                        .min_by(|a, b| a[1].total_cmp(&b[1]));
                    let caixa = match linha {
                        Some(s) => [s[0], s[1] + 1.0, s[2], s[1] + 1.0 + ALTURA],
                        // Sem linha: em cima do rótulo (o nome vem embaixo da assinatura).
                        None => [x0, y1 + 1.0, x0 + (x1 - x0).max(150.0), y1 + 1.0 + ALTURA],
                    };
                    daqui.push((it.texto.clone(), caixa));
                }
            }
            // A linha e o rótulo dela apontam a mesma área: fica uma, com o nome do rótulo.
            for (rotulo, caixa) in daqui {
                if let Some(l) = locais.iter_mut().find(|l| l.pagina == p && sobrepoe(l.caixa, caixa) > 0.5) {
                    if l.rotulo == "linha" { l.rotulo = rotulo; }
                    continue;
                }
                let ignorar = itens.iter().filter(|i| sobrepoe(caixa, i.caixa) > 0.3).map(|i| i.caixa).collect();
                locais.push(LocalAssinatura { pagina: p, rotulo, caixa, ignorar, digital: None });
            }
        }
        for c in self.campos().into_iter().filter(|c| c.tipo == TipoCampo::Assinatura) {
            locais.push(LocalAssinatura { pagina: c.pagina, rotulo: format!("campo {}", c.nome), caixa: c.caixa, ignorar: vec![], digital: Some(c.valor == "assinado") });
        }
        Ok(locais)
    }

    /// A caixa em pontos como fração da página vista (0..1, y para baixo), com o /Rotate.
    pub fn fracao_na_pagina(&self, pagina: u32, c: [f32; 4]) -> Result<[f32; 4], String> {
        let ([x0, y0, x1, y1], giro) = self.geometria(pagina)?;
        let (l, h) = if giro.rem_euclid(180) == 90 { (y1 - y0, x1 - x0) } else { (x1 - x0, y1 - y0) };
        let uv = |x: f32, y: f32| match giro.rem_euclid(360) {
            90 => ((y - y0) / l, (x - x0) / h),
            180 => ((x1 - x) / l, (y - y0) / h),
            270 => ((y1 - y) / l, (x1 - x) / h),
            _ => ((x - x0) / l, (y1 - y) / h),
        };
        let (a, b) = (uv(c[0], c[1]), uv(c[2], c[3]));
        Ok([a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)].map(|v| v.clamp(0.0, 1.0)))
    }
}

/// Quanto de `b` está dentro de `a` (0..1).
fn sobrepoe(a: [f32; 4], b: [f32; 4]) -> f32 {
    let (l, h) = ((a[2].min(b[2]) - a[0].max(b[0])).max(0.0), (a[3].min(b[3]) - a[1].max(b[1])).max(0.0));
    l * h / ((b[2] - b[0]) * (b[3] - b[1])).max(1e-3)
}

/// Fração de pixels escuros (tinta) na área `caixa` (fração da página, y para baixo) do RGBA,
/// fora das áreas `ignorar` (texto impresso, na mesma medida).
pub fn tinta(rgba: &[u8], largura: u32, altura: u32, caixa: [f32; 4], ignorar: &[[f32; 4]]) -> f32 {
    let (l, h) = (largura as f32, altura as f32);
    let px = |c: [f32; 4]| [(c[0] * l) as u32, (c[1] * h) as u32, ((c[2] * l).ceil() as u32).min(largura), ((c[3] * h).ceil() as u32).min(altura)];
    let [x0, y0, x1, y1] = px(caixa);
    let fora: Vec<[u32; 4]> = ignorar.iter().map(|&c| px(c)).collect();
    let (mut escuros, mut total) = (0u32, 0u32);
    for y in y0..y1 {
        for x in x0..x1 {
            if fora.iter().any(|f| x >= f[0] && x < f[2] && y >= f[1] && y < f[3]) { continue; }
            total += 1;
            let i = ((y * largura + x) * 4) as usize;
            let lum = 0.299 * rgba[i] as f32 + 0.587 * rgba[i + 1] as f32 + 0.114 * rgba[i + 2] as f32;
            if lum < 140.0 { escuros += 1; }
        }
    }
    if total == 0 { 0.0 } else { escuros as f32 / total as f32 }
}

/// Tinta acima disto conta como assinado.
pub const TINTA_ASSINADO: f32 = 0.01;

/// Índices das páginas sem rubrica quando a maioria tem (tinta no canto de baixo de cada página).
pub fn rubricas_faltando(tinta_no_canto: &[f32]) -> Vec<usize> {
    let com = tinta_no_canto.iter().filter(|&&t| t > TINTA_ASSINADO).count();
    if tinta_no_canto.len() < 2 || com * 2 < tinta_no_canto.len() { return vec![]; }
    tinta_no_canto.iter().enumerate().filter(|(_, &t)| t <= TINTA_ASSINADO * 0.3).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn acha_linha_e_rotulo_e_mede_a_tinta() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.inserir_texto(1, 72.0, 400.0, "Contratante: ______________________", 11.0, false, [0.0; 3]).unwrap();
        pdf.inserir_texto(1, 330.0, 200.0, "Rubrica", 11.0, false, [0.0; 3]).unwrap();
        let locais = pdf.locais_de_assinatura().unwrap();
        assert_eq!(locais.len(), 2, "{locais:?}");
        let linha = &locais[0];
        assert_eq!(linha.rotulo, "Contratante");
        assert!(linha.caixa[0] > 110.0 && linha.caixa[1] > 385.0 && linha.caixa[3] > 420.0, "área sobre os traços: {:?}", linha.caixa);
        assert_eq!(locais[1].rotulo, "Rubrica");
        assert!(locais[1].caixa[1] >= 199.0, "área acima do rótulo: {:?}", locais[1].caixa);
        assert_eq!(linha.ignorar.len(), 1, "os traços impressos não contam como tinta");
        assert!(locais[1].ignorar.is_empty(), "o rótulo fica fora da área");
        // A4 sem giro: y para baixo.
        let f = pdf.fracao_na_pagina(1, [0.0, 0.0, 595.0, 421.0]).unwrap();
        assert!((f[1] - 0.5).abs() < 0.01 && (f[3] - 1.0).abs() < 0.01, "{f:?}");

        // Tinta: 100×100 branco, um rabisco de 10×10 no canto.
        let mut img = vec![255u8; 100 * 100 * 4];
        for y in 10..20 { for x in 10..20 { img[(y * 100 + x) * 4..][..3].copy_from_slice(&[20, 20, 90]); } }
        assert!(tinta(&img, 100, 100, [0.0, 0.0, 0.5, 0.5], &[]) > TINTA_ASSINADO);
        assert_eq!(tinta(&img, 100, 100, [0.0, 0.0, 0.5, 0.5], &[[0.05, 0.05, 0.25, 0.25]]), 0.0, "texto impresso não conta");
        assert_eq!(tinta(&img, 100, 100, [0.5, 0.5, 1.0, 1.0], &[]), 0.0);
        assert_eq!(rubricas_faltando(&[0.03, 0.02, 0.0, 0.04]), vec![2]);
        assert!(rubricas_faltando(&[0.0, 0.0, 0.03]).is_empty(), "a maioria sem rubrica: o documento não usa rubrica");
    }
}
