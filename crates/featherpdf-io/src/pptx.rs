//! PDF para apresentação: cada página vira um slide (.pptx) com a página desenhada como imagem,
//! centralizada no slide. O slide tem o tamanho da primeira página; página de outro tamanho
//! cabe inteira, com margem branca.

use std::io::Write;
use std::path::Path;

/// EMU (unidade do Office) por ponto do PDF.
const EMU_PT: f64 = 12700.0;

/// `desenhar(n)` devolve a página `n` em RGBA e o tamanho dela em pontos.
pub fn gravar_pptx(destino: &Path, paginas: &[u32], mut desenhar: impl FnMut(u32) -> Result<(u32, u32, Vec<u8>, (f32, f32)), String>) -> Result<(), String> {
    let erro = |e: zip::result::ZipError| format!("Não consegui montar a apresentação: {e}");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opcoes = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let por = |nome: &str, bytes: &[u8], zip: &mut zip::ZipWriter<_>| -> Result<(), String> {
        zip.start_file(nome, opcoes).map_err(erro)?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    let mut slide = (0i64, 0i64);
    for (i, &n) in paginas.iter().enumerate() {
        let (l, a, rgba, (pl, pa)) = desenhar(n)?;
        let (pl, pa) = (pl as f64 * EMU_PT, pa as f64 * EMU_PT);
        if i == 0 { slide = (pl.round() as i64, pa.round() as i64); }
        let escala = (slide.0 as f64 / pl).min(slide.1 as f64 / pa);
        let (w, h) = ((pl * escala) as i64, (pa * escala) as i64);
        let (x, y) = ((slide.0 - w) / 2, (slide.1 - h) / 2);
        let img = image::RgbaImage::from_raw(l, a, rgba).ok_or("Página com tamanho incoerente.")?;
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88).encode_image(&image::DynamicImage::ImageRgba8(img).to_rgb8()).map_err(|e| e.to_string())?;
        let k = i + 1;
        por(&format!("ppt/media/pagina{k}.jpg"), &jpeg, &mut zip)?;
        por(&format!("ppt/slides/slide{k}.xml"), format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:pic><p:nvPicPr><p:cNvPr id="2" name="Página {n}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic></p:spTree></p:cSld></p:sld>"#).as_bytes(), &mut zip)?;
        por(&format!("ppt/slides/_rels/slide{k}.xml.rels"), format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{T}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{T}/image" Target="../media/pagina{k}.jpg"/></Relationships>"#).as_bytes(), &mut zip)?;
    }
    if paginas.is_empty() { return Err("Nenhuma página para a apresentação.".into()); }
    let ids: String = (1..=paginas.len()).map(|k| format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + k, k + 1)).collect();
    let rels: String = (1..=paginas.len()).map(|k| format!(r#"<Relationship Id="rId{}" Type="{T}/slide" Target="slides/slide{k}.xml"/>"#, k + 1)).collect();
    let tipos: String = (1..=paginas.len()).map(|k| format!(r#"<Override PartName="/ppt/slides/slide{k}.xml" ContentType="{CT}.slide+xml"/>"#)).collect();
    let (cx, cy) = slide;
    let fixos = [
        ("[Content_Types].xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="jpg" ContentType="image/jpeg"/><Override PartName="/ppt/presentation.xml" ContentType="{CT}.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="{CT}.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="{CT}.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>{tipos}</Types>"#)),
        ("_rels/.rels", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{T}/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#)),
        ("ppt/presentation.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="{cx}" cy="{cy}"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#)),
        ("ppt/_rels/presentation.xml.rels", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{T}/slideMaster" Target="slideMasters/slideMaster1.xml"/>{rels}<Relationship Id="rId{}" Type="{T}/theme" Target="theme/theme1.xml"/></Relationships>"#, paginas.len() + 2)),
        ("ppt/slideMasters/slideMaster1.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster {NS}><p:cSld>{ARVORE}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#)),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{T}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{T}/theme" Target="../theme/theme1.xml"/></Relationships>"#)),
        ("ppt/slideLayouts/slideLayout1.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout {NS} type="blank"><p:cSld name="Em branco">{ARVORE}</p:cSld></p:sldLayout>"#)),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{T}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#)),
        ("ppt/theme/theme1.xml", TEMA.to_string()),
    ];
    for (nome, xml) in fixos { por(nome, xml.as_bytes(), &mut zip)?; }
    let bytes = zip.finish().map_err(erro)?.into_inner();
    crate::gravar_atomico(destino, &bytes).map_err(|e| format!("Erro ao salvar a apresentação: {e}"))
}

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const T: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.presentationml";
const ARVORE: &str = r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree>"#;
/// O mínimo de tema que o PowerPoint exige (cores, fontes e estilos de preenchimento/linha/efeito).
const TEMA: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="FeatherPDF"><a:themeElements>"#,
    r#"<a:clrScheme name="FeatherPDF"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F3F4F6"/></a:lt2>"#,
    r#"<a:accent1><a:srgbClr val="236AC7"/></a:accent1><a:accent2><a:srgbClr val="2E7D4F"/></a:accent2><a:accent3><a:srgbClr val="C0392B"/></a:accent3><a:accent4><a:srgbClr val="8E44AD"/></a:accent4><a:accent5><a:srgbClr val="D68910"/></a:accent5><a:accent6><a:srgbClr val="16A085"/></a:accent6>"#,
    r#"<a:hlink><a:srgbClr val="236AC7"/></a:hlink><a:folHlink><a:srgbClr val="8E44AD"/></a:folHlink></a:clrScheme>"#,
    r#"<a:fontScheme name="FeatherPDF"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>"#,
    r#"<a:fmtScheme name="FeatherPDF"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>"#,
    r#"<a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="25400"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="38100"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>"#,
    r#"<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>"#,
    r#"<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>"#,
    r#"</a:themeElements></a:theme>"#);

#[cfg(test)]
mod testes {
    #[test]
    fn duas_paginas_viram_dois_slides() {
        let destino = std::env::temp_dir().join(format!("featherpdf-teste-{}.pptx", std::process::id()));
        super::gravar_pptx(&destino, &[1, 2], |n| {
            let (l, a) = if n == 1 { (60, 80) } else { (80, 60) };
            Ok((l, a, vec![200; (l * a * 4) as usize], (l as f32 * 10.0, a as f32 * 10.0)))
        }).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&destino).unwrap()).unwrap();
        let mut ler = |nome: &str| { let mut s = String::new(); std::io::Read::read_to_string(&mut zip.by_name(nome).unwrap(), &mut s).unwrap(); s };
        let p = ler("ppt/presentation.xml");
        assert!(p.contains(r#"<p:sldSz cx="7620000" cy="10160000"/>"#), "slide no tamanho da 1ª página: {p}");
        assert_eq!(p.matches("<p:sldId ").count(), 2);
        // A 2ª, deitada, cabe inteira na largura e fica centralizada na altura.
        assert!(ler("ppt/slides/slide2.xml").contains(r#"<a:off x="0" y="2222500"/><a:ext cx="7620000" cy="5715000"/>"#));
        assert!(ler("[Content_Types].xml").contains("/ppt/slides/slide2.xml"));
        // FEATHERPDF_PPTX_SAIDA=arquivo.pptx guarda uma cópia para abrir no Impress/PowerPoint.
        if let Ok(saida) = std::env::var("FEATHERPDF_PPTX_SAIDA") { let _ = std::fs::copy(&destino, saida); }
        let _ = std::fs::remove_file(destino);
    }
}
