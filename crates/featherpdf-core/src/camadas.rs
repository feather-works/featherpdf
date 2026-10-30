//! Camadas (OCG) além de ligar e desligar: renomear, travar, criar uma camada nova e dizer
//! quando ela aparece (só na tela, só na impressão, numa faixa de zoom) pelo /Usage + /AS.

use lopdf::{dictionary, Dictionary, Object, ObjectId};

use crate::DocumentoPdf;

/// Quando a camada aparece.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibilidade {
    Sempre,
    SoTela,
    SoImpressao,
    /// Zoom em porcentagem: aparece de `min` até `max` (0 = sem limite).
    Zoom { min: f32, max: f32 },
}

/// Lê o /Usage da camada.
pub(crate) fn uso_da_camada(pdf: &DocumentoPdf, id: ObjectId) -> Visibilidade {
    let Some(uso) = pdf.documento.get_dictionary(id).ok().and_then(|g| g.get(b"Usage").ok()).and_then(|u| pdf.dict(u)) else { return Visibilidade::Sempre };
    let estado = |chave: &[u8], campo: &[u8]| uso.get(chave).ok().and_then(|o| pdf.dict(o)).and_then(|d| d.get(campo).ok().and_then(|n| n.as_name().ok().map(|n| n == b"ON")));
    if let Some(z) = uso.get(b"Zoom").ok().and_then(|o| pdf.dict(o)) {
        let ler = |k: &[u8]| z.get(k).ok().and_then(|o| o.as_float().ok()).unwrap_or(0.0);
        let max = ler(b"max");
        return Visibilidade::Zoom { min: ler(b"min") * 100.0, max: if max > 1e6 { 0.0 } else { max * 100.0 } };
    }
    match (estado(b"View", b"ViewState"), estado(b"Print", b"PrintState")) {
        (Some(true), Some(false)) => Visibilidade::SoTela,
        (Some(false), Some(true)) => Visibilidade::SoImpressao,
        _ => Visibilidade::Sempre,
    }
}

impl DocumentoPdf {
    /// Diz quando a camada aparece. Grava o /Usage na camada e refaz o /AS da configuração
    /// (os eventos de tela, impressão e zoom que o leitor aplica sozinho). "Só na impressão" também
    /// começa desligada, para leitor que não conhece o /AS não mostrar na tela.
    pub fn visibilidade_camada(&mut self, id: ObjectId, v: Visibilidade) -> Result<(), String> {
        let g = self.documento.get_dictionary_mut(id).map_err(|_| "Essa camada não existe mais.".to_string())?;
        let estado = |ligada: bool| Object::Name(if ligada { b"ON".to_vec() } else { b"OFF".to_vec() });
        match v {
            Visibilidade::Sempre => { g.remove(b"Usage"); }
            Visibilidade::SoTela | Visibilidade::SoImpressao => {
                let tela = v == Visibilidade::SoTela;
                g.set("Usage", dictionary! { "View" => dictionary! { "ViewState" => estado(tela) }, "Print" => dictionary! { "PrintState" => estado(!tela) } });
            }
            Visibilidade::Zoom { min, max } => {
                let max = if max <= 0.0 { 1e9 } else { max / 100.0 };
                g.set("Usage", dictionary! { "Zoom" => dictionary! { "min" => min.max(0.0) / 100.0, "max" => max } });
            }
        }
        let todas: Vec<(ObjectId, Visibilidade)> = self.camadas().into_iter().map(|c| (c.id, if c.id == id { v } else { c.uso })).collect();
        self.mudar_camadas(|_, d| {
            let lista = |f: &dyn Fn(Visibilidade) -> bool| -> Vec<Object> { todas.iter().filter(|(_, u)| f(*u)).map(|(i, _)| Object::Reference(*i)).collect() };
            let tela = lista(&|u| matches!(u, Visibilidade::SoTela | Visibilidade::SoImpressao));
            let zoom = lista(&|u| matches!(u, Visibilidade::Zoom { .. }));
            let mut eventos = vec![];
            if !tela.is_empty() {
                eventos.push(Object::Dictionary(dictionary! { "Event" => "View", "OCGs" => tela.clone(), "Category" => vec![Object::Name(b"View".to_vec())] }));
                eventos.push(Object::Dictionary(dictionary! { "Event" => "Print", "OCGs" => tela, "Category" => vec![Object::Name(b"Print".to_vec())] }));
            }
            if !zoom.is_empty() { eventos.push(Object::Dictionary(dictionary! { "Event" => "View", "OCGs" => zoom, "Category" => vec![Object::Name(b"Zoom".to_vec())] })); }
            if eventos.is_empty() { d.remove(b"AS"); } else { d.set("AS", eventos); }
            // Estado inicial coerente com a tela.
            let tirar = |d: &mut Dictionary, k: &[u8]| {
                let w: Vec<Object> = d.get(k).and_then(Object::as_array).cloned().unwrap_or_default().into_iter().filter(|o| o.as_reference().ok() != Some(id)).collect();
                d.set(k.to_vec(), w);
            };
            tirar(d, b"ON");
            tirar(d, b"OFF");
            let chave: &[u8] = if v == Visibilidade::SoImpressao { b"OFF" } else { b"ON" };
            let mut w = d.get(chave).and_then(Object::as_array).cloned().unwrap_or_default();
            w.push(Object::Reference(id));
            d.set(chave.to_vec(), w);
        })
    }

    /// Muda a configuração das camadas (OCProperties e o /D dela, criados se faltarem) e
    /// grava de volta onde cada um estava: objeto próprio ou dentro do pai.
    fn mudar_camadas(&mut self, f: impl FnOnce(&mut Dictionary, &mut Dictionary)) -> Result<(), String> {
        let catalogo = self.documento.catalog().map_err(|e| e.to_string())?;
        let props_ref = catalogo.get(b"OCProperties").ok().and_then(|o| o.as_reference().ok());
        let mut props = match catalogo.get(b"OCProperties") {
            Ok(Object::Reference(r)) => self.documento.get_dictionary(*r).cloned().unwrap_or_default(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => dictionary! { "OCGs" => Vec::<Object>::new() },
        };
        let d_ref = props.get(b"D").ok().and_then(|o| o.as_reference().ok());
        let mut d = match props.get(b"D") {
            Ok(Object::Reference(r)) => self.documento.get_dictionary(*r).cloned().unwrap_or_default(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        f(&mut props, &mut d);
        match d_ref {
            Some(r) => { self.documento.objects.insert(r, Object::Dictionary(d)); }
            None => props.set("D", d),
        }
        match props_ref {
            Some(r) => { self.documento.objects.insert(r, Object::Dictionary(props)); }
            None => self.documento.catalog_mut().map_err(|e| e.to_string())?.set("OCProperties", props),
        }
        self.modificado = true;
        Ok(())
    }

    pub fn renomear_camada(&mut self, id: ObjectId, nome: &str) -> Result<(), String> {
        let nome = nome.trim();
        if nome.is_empty() { return Err("Escreva o nome da camada.".into()); }
        self.documento.get_dictionary_mut(id).map_err(|_| "Essa camada não existe mais.".to_string())?.set("Name", lopdf::text_string(nome));
        self.modificado = true;
        Ok(())
    }

    /// Trava (o leitor não deixa ligar ou desligar) ou destrava a camada.
    pub fn travar_camada(&mut self, id: ObjectId, travada: bool) -> Result<(), String> {
        self.mudar_camadas(|_, d| {
            let mut v: Vec<Object> = d.get(b"Locked").and_then(Object::as_array).cloned().unwrap_or_default()
                .into_iter().filter(|o| o.as_reference().ok() != Some(id)).collect();
            if travada { v.push(Object::Reference(id)); }
            d.set("Locked", v);
        })
    }

    /// Camada nova, visível, no fim da lista. Devolve o id dela.
    pub fn criar_camada(&mut self, nome: &str) -> Result<ObjectId, String> {
        let nome = nome.trim();
        if nome.is_empty() { return Err("Escreva o nome da camada.".into()); }
        let id = self.documento.add_object(dictionary! { "Type" => "OCG", "Name" => lopdf::text_string(nome) });
        self.mudar_camadas(|props, d| {
            let somar = |alvo: &mut Dictionary, k: &[u8]| {
                let mut v = alvo.get(k).and_then(Object::as_array).cloned().unwrap_or_default();
                v.push(Object::Reference(id));
                alvo.set(k.to_vec(), v);
            };
            somar(props, b"OCGs");
            somar(d, b"Order");
            somar(d, b"ON");
        })?;
        Ok(id)
    }
}

#[cfg(test)]
mod testes {
    use crate::testes::pdf_teste;

    #[test]
    fn cria_renomeia_e_trava() {
        let mut pdf = pdf_teste();
        let id = pdf.criar_camada("Notas").unwrap();
        pdf.criar_camada("Cotas").unwrap();
        pdf.renomear_camada(id, "Revisão").unwrap();
        pdf.travar_camada(id, true).unwrap();
        let pdf = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let c = pdf.camadas();
        assert_eq!(c.iter().map(|c| (c.nome.as_str(), c.visivel, c.travada)).collect::<Vec<_>>(), [("Revisão", true, true), ("Cotas", true, false)]);
        let mut pdf = pdf;
        pdf.travar_camada(c[0].id, false).unwrap();
        assert!(!pdf.camadas()[0].travada);
        assert!(pdf.criar_camada("  ").is_err());
    }

    #[test]
    fn so_impressao_e_zoom_voltam_ao_reler() {
        use crate::Visibilidade as V;
        let mut pdf = pdf_teste();
        let a = pdf.criar_camada("Carimbo de cópia").unwrap();
        let b = pdf.criar_camada("Detalhes").unwrap();
        pdf.visibilidade_camada(a, V::SoImpressao).unwrap();
        pdf.visibilidade_camada(b, V::Zoom { min: 150.0, max: 0.0 }).unwrap();
        let pdf = crate::DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let c = pdf.camadas();
        assert_eq!((c[0].uso, c[0].visivel), (V::SoImpressao, false));
        assert_eq!(c[1].uso, V::Zoom { min: 150.0, max: 0.0 });
        let mut pdf = pdf;
        pdf.visibilidade_camada(a, V::Sempre).unwrap();
        assert_eq!(pdf.camadas()[0].uso, V::Sempre);
        assert!(pdf.camadas()[0].visivel);
    }
}
