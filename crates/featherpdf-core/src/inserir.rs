//! Inserir na página: texto (Helvetica), desenho vetorial (forma, ícone), imagem e link.
//! Tudo entra num fluxo novo no fim do conteúdo, depois de um `q…Q` em volta
//! do que já existia, sem reescrever a página. Forma e ícone viram Form
//! XObject e a imagem vira Image XObject: a ferramenta Editar os acha como
//! objetos (mover, apagar). O texto vira uma linha BT…ET, editável como as
//! outras. A caixa é dada no espaço da página; numa página com /Rotate o
//! objeto é desenhado em pé na tela.

use feather_draw::{largura_texto, Caminho, Comando, FontePdf, Ponto};
use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use super::DocumentoPdf;

#[derive(Debug, Clone)]
pub enum ImagemNova {
    /// O JPEG entra como está (DCTDecode), sem decodificar e recomprimir.
    Jpeg { bytes: Vec<u8>, largura: u32, altura: u32, cinza: bool },
    Pixels { largura: u32, altura: u32, rgb: Vec<u8>, alfa: Option<Vec<u8>> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Destino { Endereco(String), Pagina(u32) }

/// Desenho vetorial (forma do catálogo, ícone) na caixa 0..l × 0..a com y para
/// baixo, como o `feather-shapes` entrega: regiões preenchidas e traços soltos.
#[derive(Debug, Clone)]
pub struct DesenhoNovo {
    pub preenchida: Caminho,
    pub tracos: Caminho,
    /// RGB de 0 a 1.
    pub preenchimento: Option<[f32; 3]>,
    pub contorno: Option<[f32; 3]>,
    /// Espessura do contorno em pontos.
    pub espessura: f32,
}

/// WinAnsi de 0x80 a 0x9F (o resto coincide com o Latin-1).
const WIN_ANSI: [(char, u8); 27] = [
    ('€', 0x80), ('‚', 0x82), ('ƒ', 0x83), ('„', 0x84), ('…', 0x85), ('†', 0x86), ('‡', 0x87),
    ('ˆ', 0x88), ('‰', 0x89), ('Š', 0x8A), ('‹', 0x8B), ('Œ', 0x8C), ('Ž', 0x8E), ('‘', 0x91),
    ('’', 0x92), ('“', 0x93), ('”', 0x94), ('•', 0x95), ('–', 0x96), ('—', 0x97), ('˜', 0x98),
    ('™', 0x99), ('š', 0x9A), ('›', 0x9B), ('œ', 0x9C), ('ž', 0x9E), ('Ÿ', 0x9F),
];

pub(crate) fn win_ansi(c: char) -> Option<u8> {
    match c as u32 {
        0x20..=0x7E | 0xA0..=0xFF => Some(c as u8),
        _ => WIN_ANSI.iter().find(|(k, _)| *k == c).map(|(_, b)| *b),
    }
}

pub(crate) fn de_win_ansi(b: u8) -> Option<char> {
    match b {
        0x20..=0x7E | 0xA0..=0xFF => Some(b as char),
        _ => WIN_ANSI.iter().find(|(_, k)| *k == b).map(|(c, _)| *c),
    }
}

/// Matriz que leva a caixa "em pé na tela" (0..l × 0..a, y para cima) para a
/// página com o giro dado; devolve a matriz e o tamanho l × a.
pub(crate) fn em_pe(caixa: [f32; 4], giro: i64) -> ([f32; 6], f32, f32) {
    let [x0, y0, x1, y1] = caixa;
    let (w, h) = (x1 - x0, y1 - y0);
    match giro {
        90 => ([0.0, 1.0, -1.0, 0.0, x1, y0], h, w),
        180 => ([-1.0, 0.0, 0.0, -1.0, x1, y1], w, h),
        270 => ([0.0, -1.0, 1.0, 0.0, x0, y1], h, w),
        _ => ([1.0, 0.0, 0.0, 1.0, x0, y0], w, h),
    }
}

pub(crate) fn cm(m: [f32; 6]) -> String {
    format!("{:.4} {:.4} {:.4} {:.4} {:.3} {:.3} cm", m[0], m[1], m[2], m[3], m[4], m[5])
}

pub(crate) fn cor(c: [f32; 3]) -> String {
    format!("{:.3} {:.3} {:.3}", c[0], c[1], c[2])
}

/// Caminho do `feather-draw` em operadores de caminho do PDF.
fn caminho_pdf(c: &Caminho) -> String {
    let mut s = String::new();
    let mut atual = Ponto { x: 0.0, y: 0.0 };
    for cmd in &c.comandos {
        match *cmd {
            Comando::Mover(p) => { s += &format!("{:.3} {:.3} m\n", p.x, p.y); atual = p; }
            Comando::Linha(p) => { s += &format!("{:.3} {:.3} l\n", p.x, p.y); atual = p; }
            Comando::Quadratica { controle: q, fim } => {
                let c1 = (atual.x + 2.0 / 3.0 * (q.x - atual.x), atual.y + 2.0 / 3.0 * (q.y - atual.y));
                let c2 = (fim.x + 2.0 / 3.0 * (q.x - fim.x), fim.y + 2.0 / 3.0 * (q.y - fim.y));
                s += &format!("{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n", c1.0, c1.1, c2.0, c2.1, fim.x, fim.y);
                atual = fim;
            }
            Comando::Cubica { controle1: a, controle2: b, fim } => {
                s += &format!("{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n", a.x, a.y, b.x, b.y, fim.x, fim.y);
                atual = fim;
            }
            Comando::Fechar => s += "h\n",
        }
    }
    s
}

impl DocumentoPdf {
    /// Acrescenta `conteudo` no fim da página, com o que existia embrulhado em q…Q.
    pub(crate) fn acrescentar(&mut self, pagina: u32, conteudo: String, recursos: &[(&str, String, ObjectId)]) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let abre = self.fluxo("q\n".into());
        let fecha = self.fluxo(format!("Q\n{conteudo}\n"));
        self.envolver(id, abre, fecha)?;
        for (categoria, nome, alvo) in recursos { self.por_recurso(id, categoria, nome, *alvo)?; }
        self.modificado = true;
        Ok(())
    }

    /// Form XObject com `conteudo` desenhado em 0..l × 0..a, chamado com a matriz `m`.
    pub(crate) fn por_grupo(&mut self, pagina: u32, m: [f32; 6], l: f32, a: f32, conteudo: String) -> Result<(), String> {
        let grupo = self.documento.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), l.into(), a.into()],
            "Resources" => dictionary! {},
        }, conteudo.into_bytes()));
        let nome = format!("FPg{}", grupo.0);
        self.acrescentar(pagina, format!("q {} /{nome} Do Q", cm(m)), &[("XObject", nome, grupo)])
    }

    /// Escreve uma linha em Helvetica com o canto de cima à esquerda (como
    /// aparece na tela) no ponto `(x, y)` da página. Letra fora do WinAnsi é
    /// recusada com o nome dela.
    pub fn inserir_texto(&mut self, pagina: u32, x: f32, y: f32, texto: &str, tamanho: f32, negrito: bool, c: [f32; 3])
        -> Result<(), String>
    {
        let texto = texto.trim_end();
        if texto.is_empty() { return Err("Texto vazio.".into()); }
        let faltam: String = texto.chars().filter(|&c| win_ansi(c).is_none())
            .collect::<std::collections::BTreeSet<char>>().into_iter().collect();
        if !faltam.is_empty() { return Err(format!("A Helvetica do PDF não tem: {faltam}")); }
        let (_, giro) = self.geometria(pagina)?;
        let (m, _, _) = em_pe([x, y, x, y], giro);
        let f = self.fonte_com_larguras(negrito);
        let nome = format!("FPt{}", f.0);
        let hexa: String = texto.chars().filter_map(win_ansi).map(|b| format!("{b:02X}")).collect();
        // O topo da letra fica no ponto; a linha de base, 0,75 em abaixo (ascendente da Helvetica).
        let conteudo = format!("q {} BT /{nome} {tamanho:.2} Tf {} rg 0 {:.3} Td {} Tj ET Q",
            cm([m[0], m[1], m[2], m[3], x, y]), cor(c), -0.75 * tamanho, format!("<{hexa}>"));
        self.acrescentar(pagina, conteudo, &[("Font", nome, f)])
    }

    /// Helvetica com as larguras: sem elas a caixa da linha (e a seleção) sairia errada.
    pub(crate) fn fonte_com_larguras(&mut self, negrito: bool) -> ObjectId {
        let (fonte, base) = if negrito { (FontePdf::Negrito, "Helvetica-Bold") } else { (FontePdf::Helvetica, "Helvetica") };
        let larguras: Vec<Object> = (32u8..=255).map(|b| {
            de_win_ansi(b).map_or(0.0, |c| largura_texto(&c.to_string(), fonte, 1000.0)).round().into()
        }).collect();
        self.documento.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base, "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 32, "LastChar" => 255, "Widths" => larguras,
        })
    }

    /// Camada de texto invisível do OCR (modo 3: não pinta, mas seleciona, copia e busca).
    /// Cada linha vem com a caixa em fração da página como aparece na tela (0..1, y para baixo);
    /// a letra é esticada na horizontal para a seleção cobrir a palavra da imagem.
    /// Letra fora do WinAnsi vira "?". Devolve quantas linhas entraram.
    pub fn inserir_camada_ocr(&mut self, pagina: u32, linhas: &[(String, [f32; 4])]) -> Result<usize, String> {
        self.camada_de_texto(pagina, linhas, false, 0)
    }

    /// [`inserir_camada_ocr`](Self::inserir_camada_ocr) no lugar da camada que um OCR anterior desta
    /// página deixou: ler de novo não duplica o texto (o Ctrl+F achava cada palavra duas vezes).
    pub fn trocar_camada_ocr(&mut self, pagina: u32, linhas: &[(String, [f32; 4])]) -> Result<usize, String> {
        let id = self.id_pagina(pagina)?;
        for s in self.documento.get_page_contents(id) {
            let Ok(fluxo) = self.documento.get_object_mut(s).and_then(|o| o.as_stream_mut()) else { continue };
            let bytes = fluxo.decompressed_content_with_limit(crate::TETO_FLUXO).unwrap_or_else(|_| fluxo.content.clone());
            // O fim que `acrescentar` grava ("Q" e a camada) com o texto invisível (3 Tr) da camada: volta a só "Q".
            if bytes.starts_with(b"Q\nq ") && bytes.windows(13).any(|w| w == b" BT 3 Tr 0 g\n") { fluxo.set_plain_content(b"Q\n".to_vec()); }
        }
        self.camada_de_texto(pagina, linhas, false, 0)
    }

    /// [`inserir_camada_ocr`](Self::inserir_camada_ocr) para texto deitado na página: `giro` é quanto
    /// a imagem precisou girar no sentido horário para a leitura ficar em pé (90: o texto sobe de
    /// baixo para cima; 270: desce de cima para baixo). A camada sai na mesma direção do desenho.
    pub fn inserir_camada_ocr_girada(&mut self, pagina: u32, linhas: &[(String, [f32; 4])], giro: u32) -> Result<usize, String> {
        self.camada_de_texto(pagina, linhas, false, giro)
    }

    /// Troca o que a página desenha (a imagem escaneada) pelo texto lido, visível e editável,
    /// cada linha no lugar e na largura em que estava. Só para escrita latina (Helvetica).
    pub fn trocar_por_texto(&mut self, pagina: u32, linhas: &[(String, [f32; 4])]) -> Result<usize, String> {
        if let Some(ch) = linhas.iter().flat_map(|(t, _)| t.chars()).find(|&ch| win_ansi(ch).is_none()) {
            return Err(format!("A Helvetica não tem “{ch}”: deixe o texto invisível por trás da imagem."));
        }
        let id = self.id_pagina(pagina)?;
        let vazio = self.fluxo(String::new());
        self.documento.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Contents", vazio);
        self.modificado = true;
        self.camada_de_texto(pagina, linhas, true, 0)
    }

    fn camada_de_texto(&mut self, pagina: u32, linhas: &[(String, [f32; 4])], visivel: bool, giro: u32) -> Result<usize, String> {
        let (caixa, giro_pagina) = self.geometria(pagina)?;
        let (m, l, a) = em_pe(caixa, giro_pagina);
        // Escrita fora do Windows-1252 (grego, cirílico, árabe, CJK…): fonte Unicode sem desenho.
        let latina = linhas.iter().all(|(t, _)| t.chars().all(|ch| win_ansi(ch).is_some()));
        let (f, unicode) = if latina { (self.fonte_com_larguras(false), Vec::new()) } else {
            let mut letras: Vec<char> = linhas.iter().flat_map(|(t, _)| t.trim().chars()).collect();
            letras.sort_unstable();
            letras.dedup();
            (self.fonte_unicode_invisivel(&letras), letras)
        };
        let nome = format!("FPo{}", f.0);
        let mut c = format!("q {} BT {} Tr 0 g
", cm(m), if visivel { 0 } else { 3 });
        let mut n = 0;
        for (texto, [x0, y0, x1, y1]) in linhas {
            let texto = texto.trim();
            // Deitado, o comprimento da linha corre na vertical e a altura da letra na horizontal.
            let deitado = giro % 180 == 90;
            let (largura, altura) = if deitado { ((y1 - y0) * a, (x1 - x0) * l) } else { ((x1 - x0) * l, (y1 - y0) * a) };
            if texto.is_empty() || largura < 1.0 || altura < 1.0 { continue; }
            let tamanho = altura * 0.8;
            let (natural, hexa): (f32, String) = if latina {
                (largura_texto(texto, FontePdf::Helvetica, tamanho), texto.chars().map(|ch| win_ansi(ch).unwrap_or(b'?')).map(|b| format!("{b:02X}")).collect())
            } else {
                // Código de 2 bytes = posição da letra na lista (+1); toda letra tem 500/1000 de largura.
                (texto.chars().count() as f32 * 0.5 * tamanho,
                 texto.chars().filter_map(|ch| unicode.binary_search(&ch).ok()).map(|i| format!("{:04X}", i + 1)).collect())
            };
            let natural = natural.max(0.1);
            // Matriz do texto: em pé; subindo (base na direita da caixa); descendo (base na esquerda).
            let (matriz, x, y) = match giro % 360 {
                90 => ("0 1 -1 0", x1 * l - altura * 0.2, (1.0 - y1) * a),
                270 => ("0 -1 1 0", x0 * l + altura * 0.2, (1.0 - y0) * a),
                _ => ("1 0 0 1", x0 * l, (1.0 - y1) * a + altura * 0.2),
            };
            c += &format!("/{nome} {tamanho:.2} Tf {:.1} Tz {matriz} {x:.2} {y:.2} Tm <{hexa}> Tj
",
                100.0 * largura / natural);
            n += 1;
        }
        c += "ET Q";
        if n > 0 { self.acrescentar(pagina, c, &[("Font", nome, f)])?; }
        Ok(n)
    }

    /// Fonte Type0 (Identity-H) sem programa de desenho, para texto invisível: o código
    /// `i + 1` é a letra `letras[i]` pelo /ToUnicode (o mesmo truque da camada do Tesseract).
    fn fonte_unicode_invisivel(&mut self, letras: &[char]) -> ObjectId {
        let mut cmap = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
            /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /FeatherOCR-UCS def /CMapType 2 def\n\
            1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
        for bloco in letras.chunks(100).enumerate() {
            let (b, grupo) = bloco;
            cmap += &format!("{} beginbfchar\n", grupo.len());
            for (j, ch) in grupo.iter().enumerate() {
                let mut u = [0u16; 2];
                let utf16: String = ch.encode_utf16(&mut u).iter().map(|x| format!("{x:04X}")).collect();
                cmap += &format!("<{:04X}> <{utf16}>\n", b * 100 + j + 1);
            }
            cmap += "endbfchar\n";
        }
        cmap += "endcmap CMapName currentdict /CMap defineresource pop end end";
        let to_unicode = self.documento.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
        let descritor = self.documento.add_object(dictionary! {
            "Type" => "FontDescriptor", "FontName" => "FeatherOCR", "Flags" => 4,
            "FontBBox" => vec![0.into(), 0.into(), 500.into(), 1000.into()], "ItalicAngle" => 0,
            "Ascent" => 1000, "Descent" => 0, "CapHeight" => 1000, "StemV" => 80,
        });
        let cid = self.documento.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "FeatherOCR",
            "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
            "FontDescriptor" => descritor, "DW" => 500, "CIDToGIDMap" => "Identity",
        });
        self.documento.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "FeatherOCR", "Encoding" => "Identity-H",
            "DescendantFonts" => vec![cid.into()], "ToUnicode" => to_unicode,
        })
    }

    /// Desenho vetorial na caixa da página, em pé na tela (forma ou ícone).
    pub fn inserir_desenho(&mut self, pagina: u32, caixa: [f32; 4], d: &DesenhoNovo) -> Result<(), String> {
        let (_, giro) = self.geometria(pagina)?;
        let (m, l, a) = em_pe(caixa, giro);
        if l < 1.0 || a < 1.0 { return Err("O desenho ficou pequeno demais.".into()); }
        let w = d.espessura.max(0.1);
        // O desenho vem com y para baixo: a primeira matriz vira o eixo.
        let mut c = format!("1 0 0 -1 0 {a:.3} cm 1 J 1 j {w:.2} w\n");
        if let Some(p) = d.preenchimento { c += &format!("{} rg\n", cor(p)); }
        if let Some(t) = d.contorno { c += &format!("{} RG\n", cor(t)); }
        if !d.preenchida.comandos.is_empty() {
            let pinta = match (d.preenchimento.is_some(), d.contorno.is_some()) {
                (true, true) => "B", (true, false) => "f", (false, true) => "S", (false, false) => "n",
            };
            c += &format!("{}{pinta}\n", caminho_pdf(&d.preenchida));
        }
        // Traços soltos (linha, detalhe, ícone): na cor do contorno, ou do preenchimento sem ele.
        if let (false, Some(t)) = (d.tracos.comandos.is_empty(), d.contorno.or(d.preenchimento)) {
            c += &format!("{} RG\n{}S\n", cor(t), caminho_pdf(&d.tracos));
        }
        self.por_grupo(pagina, m, l, a, c)
    }

    /// Imagem esticada na caixa (a tela já mandou a caixa na proporção da imagem).
    pub fn inserir_imagem(&mut self, pagina: u32, caixa: [f32; 4], imagem: ImagemNova) -> Result<(), String> {
        let (_, giro) = self.geometria(pagina)?;
        let (m, l, a) = em_pe(caixa, giro);
        let img = self.objeto_imagem(imagem)?;
        let nome = format!("FPi{}", img.0);
        self.acrescentar(pagina, format!("q {} {l:.3} 0 0 {a:.3} 0 0 cm /{nome} Do Q", cm(m)), &[("XObject", nome, img)])
    }

    /// Image XObject da imagem (com máscara de transparência quando tem), já no documento.
    pub(crate) fn objeto_imagem(&mut self, imagem: ImagemNova) -> Result<ObjectId, String> {
        let fluxo = match imagem {
            ImagemNova::Jpeg { bytes, largura, altura, cinza } => Stream::new(dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => largura, "Height" => altura,
                "ColorSpace" => if cinza { "DeviceGray" } else { "DeviceRGB" }, "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            }, bytes),
            ImagemNova::Pixels { largura, altura, rgb, alfa } => {
                if rgb.len() != (largura * altura * 3) as usize { return Err("Imagem com tamanho incoerente.".into()); }
                let mut d = dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => largura, "Height" => altura,
                    "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
                };
                if let Some(alfa) = alfa.filter(|a| a.iter().any(|&v| v < 255)) {
                    let mut mascara = Stream::new(dictionary! {
                        "Type" => "XObject", "Subtype" => "Image", "Width" => largura, "Height" => altura,
                        "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                    }, alfa);
                    let _ = mascara.compress();
                    d.set("SMask", self.documento.add_object(mascara));
                }
                let mut s = Stream::new(d, rgb);
                let _ = s.compress();
                s
            }
        };
        Ok(self.documento.add_object(fluxo))
    }

    /// Área clicável na caixa: abre o endereço ou vai para a página.
    pub fn inserir_link(&mut self, pagina: u32, caixa: [f32; 4], destino: Destino) -> Result<(), String> {
        let id = self.id_pagina(pagina)?;
        let acao = self.acao_do_link(destino)?;
        let anotacao = self.documento.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Link",
            "Rect" => caixa.iter().map(|&v| v.into()).collect::<Vec<Object>>(),
            "Border" => vec![0.into(), 0.into(), 0.into()],
            "A" => acao,
        });
        self.por_anotacao(id, anotacao)
    }

    /// A ação (/A) de um link: abrir o endereço ou ir para a página.
    pub(crate) fn acao_do_link(&self, destino: Destino) -> Result<Dictionary, String> {
        Ok(match destino {
            Destino::Endereco(url) => {
                let url = url.trim();
                if url.is_empty() { return Err("Endereço vazio.".into()); }
                // Sem esquema, o leitor trataria como arquivo local.
                // Só o e-mail ("fulano@site.com") vira mailto:, não https://fulano@….
                let email = url.contains('@') && !url.contains(['/', ':', ' ']);
                let url = if url.contains("://") || url.starts_with("mailto:") { url.to_string() }
                    else if email { format!("mailto:{url}") } else { format!("https://{url}") };
                dictionary! { "S" => "URI", "URI" => Object::string_literal(url) }
            }
            Destino::Pagina(n) => {
                let alvo = self.id_pagina(n)?;
                dictionary! { "S" => "GoTo", "D" => vec![alvo.into(), "Fit".into()] }
            }
        })
    }

    /// Lista de anotações da página (resolvida, se vier por referência).
    pub(crate) fn lista_anotacoes(&self, pagina: ObjectId) -> Vec<Object> {
        match self.documento.get_dictionary(pagina).and_then(|p| p.get(b"Annots")) {
            Ok(Object::Array(v)) => v.clone(),
            Ok(Object::Reference(r)) => self.documento.get_object(*r).ok().and_then(|o| o.as_array().ok().cloned()).unwrap_or_default(),
            _ => vec![],
        }
    }

    /// Acrescenta a anotação `anotacao` no fim das anotações da página.
    pub(crate) fn por_anotacao(&mut self, pagina: ObjectId, anotacao: ObjectId) -> Result<(), String> {
        let mut lista = self.lista_anotacoes(pagina);
        lista.push(anotacao.into());
        self.documento.get_dictionary_mut(pagina).map_err(|e| e.to_string())?.set("Annots", lista);
        self.modificado = true;
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes::pdf_teste;
    use crate::TipoObjeto;

    #[test]
    fn camada_ocr_invisivel_e_achavel() {
        let mut pdf = pdf_teste();
        let n = pdf.inserir_camada_ocr(1, &[("Certidão negativa".into(), [0.1, 0.1, 0.6, 0.13]), ("".into(), [0.0; 4])]).unwrap();
        assert_eq!(n, 1);
        let conteudo = String::from_utf8_lossy(&pdf.documento.get_page_content(pdf.id_pagina(1).unwrap())).into_owned();
        assert!(conteudo.contains("3 Tr") && conteudo.contains(" Tz "), "{conteudo}");
        assert!(pdf.texto_pagina(1).unwrap().contains("Certid"));
        // Grego e russo vão pela fonte Unicode e voltam inteiros na extração.
        assert_eq!(pdf.inserir_camada_ocr(1, &[("Αθήνα Москва".into(), [0.1, 0.3, 0.6, 0.33])]).unwrap(), 1);
        // Ler a página de novo troca a camada em vez de somar outra.
        let mut de_novo = pdf.clone();
        de_novo.trocar_camada_ocr(1, &[("Certidão negativa".into(), [0.1, 0.1, 0.6, 0.13])]).unwrap();
        assert_eq!(de_novo.texto_pagina(1).unwrap().matches("Certid").count(), 1, "{}", de_novo.texto_pagina(1).unwrap());
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        assert!(pdf.texto_pagina(1).unwrap().contains("Αθήνα Москва"), "{}", pdf.texto_pagina(1).unwrap());
        // Trocar por texto: a página perde o desenho antigo e fica só com o texto, visível.
        let mut pdf = pdf;
        assert_eq!(pdf.trocar_por_texto(1, &[("Laudo 12".into(), [0.1, 0.1, 0.5, 0.13])]).unwrap(), 1);
        let conteudo = String::from_utf8_lossy(&pdf.documento.get_page_content(pdf.id_pagina(1).unwrap())).into_owned();
        assert!(conteudo.contains("0 Tr") && !conteudo.contains("3 Tr"), "{conteudo}");
        assert!(pdf.trocar_por_texto(1, &[("Αθήνα".into(), [0.1, 0.1, 0.5, 0.13])]).is_err());
    }

    #[test]
    fn camada_deitada_em_pagina_sem_giro() {
        // Rótulo vertical numa página em pé: a camada sobe (90) ou desce (270) junto com o desenho.
        for (giro, matriz) in [(90, "0 1 -1 0"), (270, "0 -1 1 0")] {
            let mut pdf = pdf_teste();
            assert_eq!(pdf.inserir_camada_ocr_girada(1, &[("PORTA EXISTENTE".into(), [0.5, 0.2, 0.53, 0.6])], giro).unwrap(), 1);
            let conteudo = String::from_utf8_lossy(&pdf.documento.get_page_content(pdf.id_pagina(1).unwrap())).into_owned();
            assert!(conteudo.contains(&format!("{matriz} ")), "{giro}: {conteudo}");
            assert!(pdf.texto_pagina(1).unwrap().contains("PORTA EXISTENTE"));
        }
    }

    #[test]
    fn inserir_aparece_como_objetos() {
        let mut pdf = pdf_teste();
        pdf.inserir_texto(1, 100.0, 700.0, "Olá, ação", 12.0, false, [0.0; 3]).unwrap();
        let mut quadrado = Caminho::novo();
        for (i, (x, y)) in [(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0)].into_iter().enumerate() {
            let p = Ponto { x, y };
            quadrado.adicionar(if i == 0 { Comando::Mover(p) } else { Comando::Linha(p) });
        }
        quadrado.adicionar(Comando::Fechar);
        let d = DesenhoNovo { preenchida: quadrado, tracos: Caminho::novo(), preenchimento: Some([1.0, 0.0, 0.0]), contorno: Some([0.0; 3]), espessura: 1.0 };
        pdf.inserir_desenho(1, [100.0, 500.0, 200.0, 560.0], &d).unwrap();
        pdf.inserir_imagem(1, [50.0, 50.0, 150.0, 100.0], ImagemNova::Pixels {
            largura: 2, altura: 1, rgb: vec![255, 0, 0, 0, 0, 255], alfa: Some(vec![255, 128]),
        }).unwrap();
        pdf.inserir_link(1, [50.0, 50.0, 150.0, 100.0], Destino::Endereco("gov.br".into())).unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let obj = pdf.objetos(1).unwrap();
        assert_eq!(obj.len(), 3, "{obj:?}");
        let TipoObjeto::Texto { texto, .. } = &obj[0].tipo else { panic!("{obj:?}") };
        assert_eq!(texto.trim_end(), "Olá, ação");
        // Topo da linha no ponto pedido, largura medida pela Helvetica.
        let c = obj[0].caixa;
        assert!((c[3] - 700.0).abs() < 3.0 && c[0] == 100.0 && (c[2] - 100.0 - largura_texto("Olá, ação", FontePdf::Helvetica, 12.0)).abs() < 1.0, "{c:?}");
        assert_eq!(obj[1].caixa.map(f32::round), [100.0, 500.0, 200.0, 560.0]);
        assert!(matches!(obj[2].tipo, TipoObjeto::Imagem { largura: 2, altura: 1 }));
        assert!(pdf.texto_pagina(1).unwrap().contains("Olá, ação"));
        let mut pdf = pdf;
        assert!(pdf.inserir_texto(1, 0.0, 0.0, "π ✓", 12.0, false, [0.0; 3]).unwrap_err().contains('π'));
        // Só o e-mail vira mailto:, o resto ganha https://.
        pdf.inserir_link(1, [200.0, 50.0, 300.0, 100.0], Destino::Endereco("fulano@site.com".into())).unwrap();
        let enderecos: Vec<String> = pdf.links(1).into_iter().filter_map(|(_, d)| match d { Destino::Endereco(u) => Some(u), _ => None }).collect();
        assert!(enderecos.contains(&"mailto:fulano@site.com".to_string()) && enderecos.contains(&"https://gov.br".to_string()), "{enderecos:?}");
    }

    #[test]
    fn em_pe_na_pagina_girada() {
        // Página girada 90°: a largura na tela é a altura da caixa na página.
        let (m, l, a) = em_pe([10.0, 20.0, 50.0, 120.0], 90);
        assert_eq!((l, a), (100.0, 40.0));
        // O canto de baixo à esquerda da tela é (x1, y0) da página.
        assert_eq!((m[4], m[5]), (50.0, 20.0));
    }
}
