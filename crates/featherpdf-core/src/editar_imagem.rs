//! Editar uma imagem da página: redimensionar (caixa nova), cortar, trocar por outra e
//! ajustar brilho e contraste. Tudo sem recomprimir os pixels: o tamanho e o corte entram
//! como `cm` e recorte em volta do `Do`, e o brilho/contraste pelo /Decode da imagem.

use lopdf::content::Operation;
use lopdf::Object;

use crate::inserir::ImagemNova;
use crate::objetos::{bloco_do_recorte, ctm_em, inversa, mult, Analise};
use crate::{DocumentoPdf, TipoObjeto};

impl DocumentoPdf {
    fn imagem_da_pagina(&self, pagina: u32, objeto: usize) -> Result<Analise, String> {
        let analise = self.analisar_com(pagina, false)?;
        match analise.objetos.get(objeto).map(|o| &o.tipo) {
            Some(TipoObjeto::Imagem { .. }) => Ok(analise),
            Some(_) => Err("O objeto escolhido não é uma imagem.".into()),
            None => Err(format!("O objeto {objeto} não existe mais nesta página.")),
        }
    }

    /// Leva a imagem para a caixa `nova` (pontos da página). Com o recorte em volta, ele vai junto.
    pub fn redimensionar_imagem(&mut self, pagina: u32, objeto: usize, nova: [f32; 4]) -> Result<(), String> {
        let mut analise = self.imagem_da_pagina(pagina, objeto)?;
        let velha = analise.objetos[objeto].caixa;
        let (lv, av) = (velha[2] - velha[0], velha[3] - velha[1]);
        let (ln, an) = (nova[2] - nova[0], nova[3] - nova[1]);
        if lv <= 0.0 || av <= 0.0 || ln < 1.0 || an < 1.0 { return Err("Tamanho pequeno demais.".into()); }
        let (sx, sy) = (ln / lv, an / av);
        // Afim da página: escala a partir do canto da caixa velha e leva ao canto da nova.
        let afim = [sx, 0.0, 0.0, sy, nova[0] - velha[0] * sx, nova[1] - velha[1] * sy];
        let lugar = &analise.lugares[objeto];
        let (inicio, fim, ctm) = match bloco_do_recorte(&analise.ops, lugar.insercao, lugar.fim) {
            Some((i, f)) => (i, f, ctm_em(&analise.ops, i)),
            None => (lugar.insercao, lugar.fim, lugar.matriz),
        };
        // Mesma conta do mover_objetos, com a afim no lugar do deslocamento.
        let m = mult(mult(ctm, afim), inversa(ctm).ok_or("A imagem tem tamanho zero.")?);
        analise.ops.insert(fim, Operation::new("Q", vec![]));
        analise.ops.splice(inicio..inicio, [Operation::new("q", vec![]), Operation::new("cm", m.iter().map(|&v| Object::Real(v)).collect())]);
        self.gravar_conteudo(pagina, &analise.ops)
    }

    /// Corta a imagem: `corte` = quanto sai de cada lado (esquerda, direita, topo, base),
    /// em fração da imagem (0 a 1). O que sai fica escondido, não apagado.
    pub fn cortar_imagem(&mut self, pagina: u32, objeto: usize, corte: [f32; 4]) -> Result<(), String> {
        let [e, d, t, b] = corte.map(|v| v.clamp(0.0, 1.0));
        if e + d >= 0.98 || t + b >= 0.98 { return Err("O corte não deixa sobrar imagem.".into()); }
        let mut analise = self.imagem_da_pagina(pagina, objeto)?;
        let lugar = &analise.lugares[objeto];
        let (i, f) = (lugar.insercao, lugar.fim);
        let r = |v: f32| Object::Real(v);
        // No ponto do `Do` o espaço é o da imagem (0..1), então o retângulo é a fração que fica.
        analise.ops.insert(f, Operation::new("Q", vec![]));
        analise.ops.splice(i..i, [
            Operation::new("q", vec![]),
            Operation::new("re", vec![r(e), r(b), r(1.0 - e - d), r(1.0 - t - b)]),
            Operation::new("W", vec![]), Operation::new("n", vec![]),
        ]);
        self.gravar_conteudo(pagina, &analise.ops)
    }

    fn id_da_imagem(&self, pagina: u32, analise: &Analise, objeto: usize) -> Result<lopdf::ObjectId, String> {
        let op = &analise.ops[analise.lugares[objeto].insercao];
        let nome = op.operands.first().and_then(|o| o.as_name().ok()).ok_or("Imagem sem nome no conteúdo.")?;
        let (_, xobjetos) = self.recursos(self.id_pagina(pagina)?);
        xobjetos.get(nome).copied().ok_or_else(|| "A imagem não está nos recursos da página.".into())
    }

    /// Troca a imagem por outra de `l × a` pixels, centrada na mesma caixa sem distorcer.
    /// Se a mesma imagem aparece em outras páginas, troca lá também.
    pub fn trocar_imagem(&mut self, pagina: u32, objeto: usize, imagem: ImagemNova, l: u32, a: u32) -> Result<(), String> {
        let analise = self.imagem_da_pagina(pagina, objeto)?;
        let xid = self.id_da_imagem(pagina, &analise, objeto)?;
        let c = analise.objetos[objeto].caixa;
        let novo = self.objeto_imagem(imagem)?;
        let fluxo = self.documento.objects.remove(&novo).ok_or("Imagem nova não gravada.")?;
        self.documento.objects.insert(xid, fluxo);
        let (cl, ca) = (c[2] - c[0], c[3] - c[1]);
        let s = (cl / l.max(1) as f32).min(ca / a.max(1) as f32);
        let (w, h) = (l as f32 * s, a as f32 * s);
        let (x, y) = (c[0] + (cl - w) / 2.0, c[1] + (ca - h) / 2.0);
        if (w - cl).abs() < 0.5 && (h - ca).abs() < 0.5 { self.modificado = true; return Ok(()); }
        self.redimensionar_imagem(pagina, objeto, [x, y, x + w, y + h])
    }

    /// Brilho (-1 a 1, 0 = como está) e contraste (0 a 3, 1 = como está) pelo /Decode da
    /// imagem: o leitor aplica na hora de desenhar, os pixels não mudam e (0, 1) volta ao original.
    pub fn ajustar_imagem(&mut self, pagina: u32, objeto: usize, brilho: f32, contraste: f32) -> Result<(), String> {
        let analise = self.imagem_da_pagina(pagina, objeto)?;
        let xid = self.id_da_imagem(pagina, &analise, objeto)?;
        let dict = &mut self.documento.get_object_mut(xid).and_then(Object::as_stream_mut).map_err(|e| e.to_string())?.dict;
        let espaco = dict.get(b"ColorSpace").ok().and_then(|o| o.as_name().ok()).unwrap_or(b"");
        let canais = match espaco { b"DeviceGray" | b"CalGray" => 1, b"DeviceRGB" | b"CalRGB" => 3,
            _ => return Err("Brilho e contraste valem para imagens em cinza ou RGB.".into()) };
        if dict.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false) { return Err("Máscara não tem brilho nem contraste.".into()); }
        if brilho.abs() < 1e-3 && (contraste - 1.0).abs() < 1e-3 { dict.remove(b"Decode"); } else {
            let (d0, d1) = (0.5 - 0.5 * contraste + brilho, 0.5 + 0.5 * contraste + brilho);
            dict.set("Decode", (0..canais).flat_map(|_| [Object::Real(d0), Object::Real(d1)]).collect::<Vec<_>>());
        }
        self.modificado = true;
        Ok(())
    }

    /// Brilho e contraste atuais (lidos do /Decode), para o painel começar deles.
    pub fn ajuste_da_imagem(&self, pagina: u32, objeto: usize) -> (f32, f32) {
        let Ok(analise) = self.imagem_da_pagina(pagina, objeto) else { return (0.0, 1.0) };
        let Ok(xid) = self.id_da_imagem(pagina, &analise, objeto) else { return (0.0, 1.0) };
        let d: Vec<f32> = self.documento.get_object(xid).and_then(Object::as_stream).ok()
            .and_then(|s| s.dict.get(b"Decode").ok()?.as_array().ok().cloned())
            .map(|v| v.iter().filter_map(|o| o.as_float().ok()).collect()).unwrap_or_default();
        if d.len() < 2 { return (0.0, 1.0); }
        let contraste = d[1] - d[0];
        ((d[0] + d[1]) / 2.0 - 0.5, contraste)
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;
    use crate::inserir::ImagemNova;

    fn caixa(pdf: &crate::DocumentoPdf) -> [f32; 4] { pdf.objetos(1).unwrap()[0].caixa }
    fn perto(a: [f32; 4], b: [f32; 4]) -> bool { a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.05) }

    #[test]
    fn redimensiona_corta_troca_e_ajusta() {
        let mut pdf = pdf_com("q 50 0 0 40 300 500 cm /Im1 Do Q");
        assert!(perto(caixa(&pdf), [300.0, 500.0, 350.0, 540.0]));
        pdf.redimensionar_imagem(1, 0, [100.0, 100.0, 200.0, 180.0]).unwrap();
        assert!(perto(caixa(&pdf), [100.0, 100.0, 200.0, 180.0]), "{:?}", caixa(&pdf));
        // Cortar e depois mover: o recorte anda junto.
        pdf.cortar_imagem(1, 0, [0.5, 0.0, 0.0, 0.0]).unwrap();
        pdf.mover_objetos(1, &[0], 10.0, 0.0).unwrap();
        pdf.redimensionar_imagem(1, 0, [0.0, 0.0, 50.0, 40.0]).unwrap();
        assert!(perto(caixa(&pdf), [0.0, 0.0, 50.0, 40.0]), "{:?}", caixa(&pdf));
        // Brilho/contraste vão e voltam.
        pdf.ajustar_imagem(1, 0, 0.2, 1.5).unwrap();
        let (b, c) = pdf.ajuste_da_imagem(1, 0);
        assert!((b - 0.2).abs() < 1e-4 && (c - 1.5).abs() < 1e-4);
        pdf.ajustar_imagem(1, 0, 0.0, 1.0).unwrap();
        assert_eq!(pdf.ajuste_da_imagem(1, 0), (0.0, 1.0));
        // Imagem 4×1 numa caixa 50×40: fica 50×12,5 centrada.
        pdf.trocar_imagem(1, 0, ImagemNova::Pixels { largura: 4, altura: 1, rgb: vec![9; 12], alfa: None }, 4, 1).unwrap();
        let pdf = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert!(perto(caixa(&pdf), [0.0, 13.75, 50.0, 26.25]), "{:?}", caixa(&pdf));
        assert!(matches!(pdf.objetos(1).unwrap()[0].tipo, crate::TipoObjeto::Imagem { largura: 4, altura: 1 }));
    }
}
