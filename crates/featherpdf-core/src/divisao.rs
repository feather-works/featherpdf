//! Dividir avançado: onde cortar o documento por marcador (sumário), por página em
//! branco, por texto encontrado ou pelo tamanho máximo de cada arquivo. Cada parte é
//! (primeira, última, nome sugerido) — o nome vem do marcador; vazio nos outros modos.

use crate::DocumentoPdf;

pub type Parte = (u32, u32, String);

impl DocumentoPdf {
    /// Uma parte por marcador de primeiro nível, do começo dele até antes do seguinte.
    /// As páginas antes do primeiro marcador viram uma parte sem nome.
    pub fn partes_por_marcadores(&self) -> Result<Vec<Parte>, String> {
        let total = self.total_paginas();
        let mut inicios: Vec<(u32, String)> = self.marcadores().into_iter().filter(|m| m.nivel == 1 && (1..=total).contains(&m.pagina))
            .map(|m| (m.pagina, m.titulo)).collect();
        inicios.sort_by_key(|m| m.0);
        inicios.dedup_by_key(|m| m.0);
        if inicios.is_empty() { return Err("Este PDF não tem marcadores (sumário) para dividir.".into()); }
        if inicios[0].0 > 1 { inicios.insert(0, (1, String::new())); }
        Ok(inicios.iter().enumerate().map(|(i, (a, nome))| {
            let b = inicios.get(i + 1).map_or(total, |p| p.0 - 1);
            (*a, b, nome.clone())
        }).collect())
    }

    /// As páginas em branco separam as partes e não entram em nenhuma.
    /// shortcut: "em branco" é página sem texto, imagem nem desenho; folha escaneada em
    /// branco (é uma imagem) não conta. Medir a tinta da imagem se pedirem.
    pub fn partes_por_branco(&self) -> Result<Vec<Parte>, String> {
        let total = self.total_paginas();
        let mut partes = Vec::new();
        let mut inicio = None;
        for p in 1..=total {
            if self.objetos(p)?.is_empty() {
                if let Some(a) = inicio.take() { partes.push((a, p - 1, String::new())); }
            } else if inicio.is_none() { inicio = Some(p); }
        }
        if let Some(a) = inicio { partes.push((a, total, String::new())); }
        if partes.len() < 2 { return Err("Nenhuma página em branco entre páginas com conteúdo.".into()); }
        Ok(partes)
    }

    /// Cada página que contém `termo` (sem diferenciar maiúsculas) começa uma parte nova.
    pub fn partes_por_texto(&self, termo: &str) -> Result<Vec<Parte>, String> {
        let termo = termo.trim().to_lowercase();
        if termo.is_empty() { return Err("Escreva o texto que marca o começo de cada parte.".into()); }
        let total = self.total_paginas();
        let mut inicios = vec![1];
        for p in 2..=total {
            if self.texto_pagina(p).unwrap_or_default().to_lowercase().contains(&termo) { inicios.push(p); }
        }
        if inicios.len() < 2 && !self.texto_pagina(1).unwrap_or_default().to_lowercase().contains(&termo) {
            return Err(format!("\"{}\" não aparece no documento.", termo));
        }
        Ok(inicios.iter().enumerate().map(|(i, &a)| (a, inicios.get(i + 1).map_or(total, |b| b - 1), String::new())).collect())
    }

    /// Partes que cabem em `limite` bytes cada (uma página maior que o limite fica sozinha).
    /// Busca a maior parte que cabe dobrando e depois por bisseção: poucas gravações por parte.
    /// shortcut: cada medida grava a parte de verdade (~2·log2(páginas) gravações por parte);
    /// estimar sem gravar erraria a compressão. Trocar se PDF de milhares de páginas ficar lento.
    pub fn partes_por_tamanho(&self, limite: usize) -> Result<Vec<Parte>, String> {
        let total = self.total_paginas();
        let tamanho = |a: u32, b: u32| self.extrair(&(a..=b).collect::<Vec<_>>()).map(|v| v.len());
        let mut partes = Vec::new();
        let mut a = 1;
        while a <= total {
            // `bom` cabe (ou é uma página só); `ruim` não cabe.
            let (mut bom, mut ruim) = (a, None);
            let mut passo = 1;
            while ruim.is_none() && bom < total {
                let teste = (bom + passo).min(total);
                if tamanho(a, teste)? <= limite { bom = teste; passo *= 2; } else { ruim = Some(teste); }
            }
            if let Some(mut r) = ruim {
                while r - bom > 1 {
                    let meio = (bom + r) / 2;
                    if tamanho(a, meio)? <= limite { bom = meio } else { r = meio }
                }
            }
            partes.push((a, bom, String::new()));
            a = bom + 1;
        }
        Ok(partes)
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;

    #[test]
    fn divide_por_branco_texto_e_tamanho() {
        // Páginas: 1 texto, 2 branca, 3 "Capítulo", 4 texto.
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Abertura) Tj ET");
        pdf.inserir_em_branco(2).unwrap();
        pdf.inserir_em_branco(3).unwrap();
        pdf.inserir_texto(3, 72.0, 100.0, "Capítulo 2", 12.0, false, [0.0; 3]).unwrap();
        pdf.inserir_em_branco(4).unwrap();
        pdf.inserir_texto(4, 72.0, 100.0, "Fim", 12.0, false, [0.0; 3]).unwrap();
        let so = |v: Vec<super::Parte>| v.into_iter().map(|p| (p.0, p.1)).collect::<Vec<_>>();
        assert_eq!(so(pdf.partes_por_branco().unwrap()), [(1, 1), (3, 4)]);
        assert_eq!(so(pdf.partes_por_texto("capítulo").unwrap()), [(1, 2), (3, 4)]);
        assert!(pdf.partes_por_texto("inexistente").is_err());
        assert!(pdf.partes_por_marcadores().is_err());
        assert_eq!(so(pdf.partes_por_tamanho(usize::MAX).unwrap()), [(1, 4)]);
        assert_eq!(so(pdf.partes_por_tamanho(1).unwrap()), [(1, 1), (2, 2), (3, 3), (4, 4)]);
        let uma = pdf.extrair(&[1, 2]).unwrap().len();
        let partes = pdf.partes_por_tamanho(uma).unwrap();
        assert!(partes.len() >= 2 && partes.iter().all(|p| pdf.extrair(&(p.0..=p.1).collect::<Vec<_>>()).unwrap().len() <= uma || p.0 == p.1));
    }
}
