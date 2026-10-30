//! Imagens como páginas: juntar PNG/JPG a um PDF ou criar um PDF só de imagens.

use lopdf::{dictionary, Object};

use crate::inserir::ImagemNova;
use crate::DocumentoPdf;

const A4: (f32, f32) = (595.0, 842.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AjusteImagem {
    /// A página fica do tamanho da imagem, a 96 dpi (o tamanho dela na tela).
    TamanhoDaImagem,
    /// Página A4 (deitada se a imagem for larga), com a imagem centrada e margem de 1 cm.
    A4,
}

/// Tamanho da página e caixa da imagem, em pontos, para uma imagem de `l × a` pixels.
pub fn caixa_da_imagem(l: u32, a: u32, ajuste: AjusteImagem) -> ((f32, f32), [f32; 4]) {
    let (l, a) = (l.max(1) as f32, a.max(1) as f32);
    match ajuste {
        AjusteImagem::TamanhoDaImagem => {
            let (w, h) = (l * 0.75, a * 0.75);
            ((w, h), [0.0, 0.0, w, h])
        }
        AjusteImagem::A4 => {
            let (pw, ph) = if l > a { (A4.1, A4.0) } else { A4 };
            let m = 28.35;
            let s = ((pw - 2.0 * m) / l).min((ph - 2.0 * m) / a);
            let (w, h) = (l * s, a * s);
            let (x, y) = ((pw - w) / 2.0, (ph - h) / 2.0);
            ((pw, ph), [x, y, x + w, y + h])
        }
    }
}

impl DocumentoPdf {
    /// Página nova (a de número `posicao`) só com a imagem de `l × a` pixels.
    pub fn inserir_pagina_imagem(&mut self, posicao: u32, imagem: ImagemNova, l: u32, a: u32, ajuste: AjusteImagem) -> Result<(), String> {
        self.inserir_em_branco(posicao)?;
        let ((w, h), caixa) = caixa_da_imagem(l, a, ajuste);
        let id = self.id_pagina(posicao)?;
        self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?
            .set("MediaBox", vec![Object::from(0), 0.into(), w.into(), h.into()]);
        self.inserir_imagem(posicao, caixa, imagem)
    }

    /// Carimbo (anotação /Stamp) com a imagem, centrada na caixa sem distorcer.
    /// `nome` vai no texto do comentário (o painel mostra) e `autor` no /T.
    pub fn carimbo_imagem(&mut self, pagina: u32, caixa: [f32; 4], imagem: ImagemNova, l: u32, a: u32, nome: &str, autor: &str) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let (cl, ca) = (caixa[2] - caixa[0], caixa[3] - caixa[1]);
        let s = (cl / l.max(1) as f32).min(ca / a.max(1) as f32);
        let (w, h) = (l as f32 * s, a as f32 * s);
        let c = [caixa[0] + (cl - w) / 2.0, caixa[1] + (ca - h) / 2.0, caixa[0] + (cl + w) / 2.0, caixa[1] + (ca + h) / 2.0];
        let img = self.objeto_imagem(imagem)?;
        let ap = self.documento.add_object(lopdf::Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => crate::anotacoes::retangulo(c),
            "Resources" => dictionary! { "XObject" => dictionary! { "Im" => Object::Reference(img) } },
        }, format!("q {w:.3} 0 0 {h:.3} {:.3} {:.3} cm /Im Do Q", c[0], c[1]).into_bytes()));
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Stamp", "Name" => "Imagem", "Rect" => crate::anotacoes::retangulo(c),
            "P" => Object::Reference(id), "F" => 4, "Contents" => lopdf::text_string(nome),
            "AP" => dictionary! { "N" => Object::Reference(ap) },
        };
        if !autor.is_empty() { d.set("T", lopdf::text_string(autor)); }
        let anotacao = self.documento.add_object(d);
        self.por_anotacao(id, anotacao)
    }

    /// Troca as páginas pelas imagens, em ordem (para criar um PDF a partir de um em branco).
    pub fn so_imagens(&mut self, imagens: Vec<(ImagemNova, u32, u32)>, ajuste: AjusteImagem) -> Result<(), String> {
        if imagens.is_empty() { return Err("Nenhuma imagem escolhida.".into()); }
        let antigas: Vec<u32> = (1..=self.total_paginas()).collect();
        for (imagem, l, a) in imagens {
            let fim = self.total_paginas() + 1;
            self.inserir_pagina_imagem(fim, imagem, l, a, ajuste)?;
        }
        self.excluir_paginas(&antigas)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes::pdf_teste;

    fn pixel(l: u32, a: u32) -> (ImagemNova, u32, u32) {
        (ImagemNova::Pixels { largura: l, altura: a, rgb: vec![200; (l * a * 3) as usize], alfa: None }, l, a)
    }

    #[test]
    fn imagens_viram_paginas() {
        assert_eq!(caixa_da_imagem(800, 400, AjusteImagem::TamanhoDaImagem), ((600.0, 300.0), [0.0, 0.0, 600.0, 300.0]));
        let ((pw, ph), c) = caixa_da_imagem(1000, 500, AjusteImagem::A4);
        assert_eq!((pw, ph), (842.0, 595.0));
        assert!((c[2] - c[0]) / (c[3] - c[1]) - 2.0 < 0.01 && c[0] >= 28.0);
        let mut pdf = pdf_teste();
        pdf.so_imagens(vec![pixel(40, 20), pixel(10, 30)], AjusteImagem::TamanhoDaImagem).unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert_eq!(pdf.total_paginas(), 2);
        let id = pdf.id_pagina(2).unwrap();
        assert_eq!(pdf.caixa(id), (0.0, 0.0, 7.5, 22.5));
        assert!(matches!(pdf.objetos(1).unwrap()[0].tipo, crate::TipoObjeto::Imagem { largura: 40, altura: 20 }));
    }

    #[test]
    fn carimbo_de_imagem_fica_na_proporcao() {
        let mut pdf = pdf_teste();
        let (img, l, a) = pixel(40, 20);
        pdf.carimbo_imagem(1, [100.0, 100.0, 300.0, 300.0], img, l, a, "assinatura.png", "Ana").unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let c = &pdf.anotacoes()[0];
        assert_eq!((c.tipo, c.texto.as_str(), c.caixa), ("Carimbo", "assinatura.png", [100.0, 150.0, 300.0, 250.0]));
    }
}
