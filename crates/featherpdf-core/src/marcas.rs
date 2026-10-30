//! Marca d'água de texto e cabeçalho/rodapé com campos ({pagina}, {total},
//! {data}, {arquivo}). Tudo vira conteúdo da página (como a numeração), em
//! Helvetica, num fluxo no fim do conteúdo com o que existia embrulhado em q…Q.

use lopdf::dictionary;

use crate::anotacoes::hexa;
use crate::inserir::cor;
use crate::DocumentoPdf;
use feather_draw::{largura_texto, FontePdf};

#[derive(Debug, Clone, PartialEq)]
pub struct MarcaDagua {
    pub texto: String,
    pub tamanho: f32,
    pub cor: [f32; 3],
    /// 0 (invisível) a 1 (opaca).
    pub opacidade: f32,
    /// Graus no sentido anti-horário; 45 é a diagonal clássica.
    pub angulo: f32,
    /// Páginas que recebem a marca; vazio = todas.
    pub paginas: Vec<u32>,
    /// Numa camada própria ("Marca d'água"), que o leitor pode esconder.
    pub em_camada: bool,
}

/// Cabeçalho e rodapé: esquerda, centro e direita de cada um. Os campos são
/// trocados página a página (`{pagina}`, `{total}`, `{data}`, `{arquivo}`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CabecalhoRodape {
    pub cabecalho: [String; 3],
    pub rodape: [String; 3],
    pub tamanho: f32,
    /// Distância da borda, em pontos.
    pub margem: f32,
    pub cor: [f32; 3],
    /// Páginas que recebem; vazio = todas.
    pub paginas: Vec<u32>,
    /// Nome do arquivo e data já prontos (o core não lê o relógio da pessoa).
    pub arquivo: String,
    pub data: String,
}

/// Troca os campos do texto pelos valores da página.
pub fn expandir_campos(texto: &str, pagina: u32, total: u32, data: &str, arquivo: &str) -> String {
    texto.replace("{pagina}", &pagina.to_string()).replace("{total}", &total.to_string())
        .replace("{data}", data).replace("{arquivo}", arquivo)
}

impl DocumentoPdf {
    /// Ids das páginas pedidas (vazio = todas), recusando número fora do documento.
    fn escolhidas(&mut self, paginas: &[u32]) -> Result<Vec<(u32, lopdf::ObjectId)>, String> {
        let ids = self.achatar()?;
        let total = ids.len() as u32;
        if let Some(p) = paginas.iter().find(|&&p| p == 0 || p > total) { return Err(format!("Página {p} inexistente.")); }
        Ok(ids.into_iter().enumerate().map(|(i, id)| (i as u32 + 1, id))
            .filter(|(n, _)| paginas.is_empty() || paginas.contains(n)).collect())
    }

    /// Escreve a marca d'água centrada em cada página, girada e translúcida, por cima do conteúdo.
    /// shortcut: o ângulo é na página sem giro; numa página com /Rotate a marca sai girada junto.
    pub fn marca_dagua(&mut self, m: &MarcaDagua) -> Result<u32, String> {
        let texto = m.texto.trim();
        if texto.is_empty() { return Err("Escreva o texto da marca d'água.".into()); }
        if !(4.0..=400.0).contains(&m.tamanho) { return Err("Tamanho da letra entre 4 e 400 pt.".into()); }
        let paginas = self.escolhidas(&m.paginas)?;
        let fonte = self.documento.add_object(Self::fonte_helvetica(true));
        let estado = self.documento.add_object(dictionary! {
            "Type" => "ExtGState", "ca" => m.opacidade.clamp(0.0, 1.0), "CA" => m.opacidade.clamp(0.0, 1.0),
        });
        let largura = largura_texto(texto, FontePdf::Negrito, m.tamanho);
        let (s, c) = m.angulo.to_radians().sin_cos();
        let camada = if m.em_camada { Some(self.criar_camada("Marca d'água")?) } else { None };
        let (bdc, emc) = if camada.is_some() { ("/OC /FPmdC BDC ", " EMC") } else { ("", "") };
        let abre = self.fluxo("q\n".into());
        for &(_, id) in &paginas {
            let (x0, y0, x1, y1) = self.caixa(id);
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let fecha = self.fluxo(format!(
                "Q\n{bdc}q /FPmdG gs BT /FPmdF {t} Tf {} rg {c:.4} {s:.4} {:.4} {c:.4} {cx:.2} {cy:.2} Tm {:.2} {:.2} Td {} Tj ET Q{emc}\n",
                cor(m.cor), -s, -largura / 2.0, -m.tamanho * 0.35, hexa(texto), t = m.tamanho));
            self.envolver(id, abre, fecha)?;
            self.por_recurso(id, "Font", "FPmdF", fonte)?;
            self.por_recurso(id, "ExtGState", "FPmdG", estado)?;
            if let Some(c) = camada { self.por_recurso(id, "Properties", "FPmdC", c)?; }
        }
        self.modificado = true;
        Ok(paginas.len() as u32)
    }

    /// Escreve cabeçalho e rodapé nas páginas escolhidas; devolve quantas receberam.
    /// shortcut: posição na página sem giro, como a numeração.
    pub fn cabecalho_rodape(&mut self, op: &CabecalhoRodape) -> Result<u32, String> {
        if op.cabecalho.iter().chain(&op.rodape).all(|t| t.trim().is_empty()) {
            return Err("Escreva o texto de pelo menos um lugar do cabeçalho ou do rodapé.".into());
        }
        if !(4.0..=72.0).contains(&op.tamanho) { return Err("Tamanho da letra entre 4 e 72 pt.".into()); }
        let total = self.total_paginas();
        let paginas = self.escolhidas(&op.paginas)?;
        let fonte = self.documento.add_object(Self::fonte_helvetica(false));
        let abre = self.fluxo("q\n".into());
        for &(n, id) in &paginas {
            let (x0, y0, x1, y1) = self.caixa(id);
            let mut conteudo = format!("Q\nq BT /FPcrF {} Tf {} rg\n", op.tamanho, cor(op.cor));
            for (textos, y) in [(&op.cabecalho, y1 - op.margem - op.tamanho * 0.75), (&op.rodape, y0 + op.margem)] {
                for (lado, t) in textos.iter().enumerate() {
                    let t = expandir_campos(t.trim(), n, total, &op.data, &op.arquivo);
                    if t.is_empty() { continue; }
                    let l = largura_texto(&t, FontePdf::Helvetica, op.tamanho);
                    let x = match lado { 0 => x0 + op.margem, 1 => (x0 + x1 - l) / 2.0, _ => x1 - op.margem - l };
                    conteudo += &format!("1 0 0 1 {x:.2} {y:.2} Tm {} Tj\n", hexa(&t));
                }
            }
            conteudo += "ET Q\n";
            let fecha = self.fluxo(conteudo);
            self.envolver(id, abre, fecha)?;
            self.por_recurso(id, "Font", "FPcrF", fonte)?;
        }
        self.modificado = true;
        Ok(paginas.len() as u32)
    }
}

/// Lê "1-3, 5" como a lista de páginas; vazio = todas (lista vazia).
pub fn paginas_do_texto(texto: &str, total: u32) -> Result<Vec<u32>, String> {
    if texto.trim().is_empty() { return Ok(Vec::new()); }
    Ok(crate::ler_intervalos(texto, total)?.into_iter().flat_map(|(a, b)| a..=b).collect())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes::pdf_teste;

    #[test]
    fn campos_sao_trocados() {
        assert_eq!(expandir_campos("{arquivo} · {pagina}/{total} · {data}", 2, 9, "19/10/2026", "a.pdf"), "a.pdf · 2/9 · 19/10/2026");
        assert_eq!(paginas_do_texto("1-2, 4", 5).unwrap(), [1, 2, 4]);
        assert!(paginas_do_texto("", 5).unwrap().is_empty());
        assert!(paginas_do_texto("7", 5).is_err());
    }

    #[test]
    fn marca_e_cabecalho_viram_texto_da_pagina() {
        let mut pdf = pdf_teste();
        pdf.inserir_em_branco(2).unwrap();
        let m = MarcaDagua { texto: "CONFIDENCIAL".into(), tamanho: 60.0, cor: [0.8, 0.1, 0.1], opacidade: 0.3, angulo: 45.0, paginas: vec![2], em_camada: true };
        assert_eq!(pdf.marca_dagua(&m).unwrap(), 1);
        let op = CabecalhoRodape { cabecalho: ["{arquivo}".into(), String::new(), String::new()],
            rodape: [String::new(), "Página {pagina} de {total}".into(), String::new()],
            tamanho: 9.0, margem: 28.0, cor: [0.0; 3], paginas: vec![], arquivo: "relatorio.pdf".into(), data: String::new() };
        assert_eq!(pdf.cabecalho_rodape(&op).unwrap(), 2);
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert!(pdf.texto_pagina(1).unwrap().contains("Página 1 de 2"));
        let t2 = pdf.texto_pagina(2).unwrap();
        assert!(t2.contains("CONFIDENCIAL") && t2.contains("relatorio.pdf"), "{t2}");
        assert!(!pdf.texto_pagina(1).unwrap().contains("CONFIDENCIAL"));
        assert_eq!(pdf.camadas()[0].nome, "Marca d'água");
        assert!(pdf.clone().marca_dagua(&MarcaDagua { texto: " ".into(), ..m }).is_err());
    }
}
