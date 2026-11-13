//! Motor do FeatherPDF. O parser/editor de objetos PDF e especifico deste produto;
//! conversao/arquivos ficam no FeatherPDF-IO e nos motores comuns da suíte FeatherWorks.
//! Este crate nao depende de GUI/Iced.

use std::path::{Path, PathBuf};
use lopdf::{Document, Object, ObjectId};

mod achatamento;
mod anotacoes;
mod caixas;
mod comparar;
mod caderno;
mod latex;
mod tabela_viva;
mod refluxo;
mod tipografia;
mod fontes;
mod reconhecer;
mod camadas;
pub mod assinatura;
mod dados_form;
mod divisao;
mod fluxo;
mod formularios;
mod grafico;
mod imagens;
mod inserir;
mod inspecao;
mod linearizar;
mod marcas;
mod markdown;
mod medida;
mod modelos;
mod modo_escuro;
mod navegacao;
mod objetos;
mod ordem;
mod paragrafos;
mod otimizar;
mod paginas;
mod revisao;
mod rotulos;
pub use rotulos::{EstiloRotulo, TrechoRotulo};
mod sanitizar;
mod tabela;
mod tabelas;
mod pdfa;
mod propriedades;
pub use propriedades::Propriedades;
mod regras;
mod preflight;
mod tarja;
mod versoes;
mod editar_imagem;
pub use modelos::Modelo;
pub use caderno::limpar_caderno;
pub use tabela_viva::GradeTabela;
pub use tipografia::EstiloTexto;
pub use fontes::{caractere_win_ansi, FonteNova};
pub use reconhecer::{rubricas_faltando, tinta, LocalAssinatura, TINTA_ASSINADO};
pub use camadas::Visibilidade;
pub use comparar::{comparar_texto, sobrepor, Mudanca, Trecho};
pub use modo_escuro::escurecer;
pub use navegacao::{Anexo, LinkPdf, Marcador, MoverMarcador};
pub use anotacoes::{cor_do_carimbo, hoje, Anotacao, AnotacaoNova, Camada, Marcacao};
pub use formularios::{obrigatorios_vazios_de, Campo, CampoNovo, TipoCampo};
pub use fluxo::{BlocoFluxo, CelulaFluxo, PaginaFluxo};
pub use inserir::{DesenhoNovo, Destino, ImagemNova};
pub use inspecao::{tamanho, Cor, DetalheObjeto, FonteInfo, Gravidade, ImagemInfo, Inspecao, LinhaArvore, NoEstrutura, Pedaco, Problema, Resumo};
pub use medida::{comprimento, medir, numero, Escala, TipoMedida, Unidade};
pub use objetos::{achar, achar_sem_caixa, EstiloDesenho, ObjetoPdf, OpcoesBusca, TipoObjeto};
pub use otimizar::{ImagemCrua, ImagemExtraida, OpcoesCompressao, Permissoes, Peso, Protecao, Reamostrar};
pub use marcas::{expandir_campos, paginas_do_texto, CabecalhoRodape, MarcaDagua};
pub use revisao::{comentarios_csv, data_legivel, estado_atual, nome_estado, ESTADOS};
pub use imagens::{caixa_da_imagem, AjusteImagem};
pub use divisao::Parte;
pub use sanitizar::OpcoesLimpeza;
pub use markdown::markdown;
pub use caixas::CaixaPagina;
pub use dados_form::{ler_csv_formulario, ler_json_formulario};
pub use paginas::{ler_intervalos, FormatoNumero, Numeracao, PosicaoNumero};
pub use tarja::{Achado, ApagarPixels, Padrao};
pub use tabela::{CelulaGrade, GradePagina, ImagemGrade};
pub use pdfa::{ItemPdfa, Situacao as SituacaoPdfa};
pub use preflight::{Destino as DestinoPreflight, ItemPreflight, NivelPreflight};
pub use regras::{escrever_numero, ler_numero, Calculo, OpCalculo, Regra};
pub use versoes::{versoes, Versao};
pub use tabelas::{ler_valor, tabelas_da_pagina, juntar_continuacoes, tipo_provavel, TabelaPdf, TipoColuna, Valor};

/// Erro de [`DocumentoPdf::de_bytes_com_senha`] quando o PDF pede senha para abrir.
pub const PRECISA_SENHA: &str = "Este PDF pede senha para abrir.";
pub const SENHA_ERRADA: &str = "Senha incorreta.";

const MAX_TEXTO_PAGINA: usize = 32 * 1024 * 1024;
/// Teto para descomprimir um fluxo do arquivo: um fluxo de 50 KB pode virar 8 GB (bomba de compressão).
pub(crate) const TETO_FLUXO: usize = 256 << 20;

#[derive(Debug, Clone)]
pub struct DocumentoPdf {
    documento: Document,
    origem: Option<PathBuf>,
    modificado: bool,
}

impl DocumentoPdf {
    /// Construtor sem acesso ao sistema de arquivos: o FeatherPDF-IO recebe os bytes.
    pub fn de_bytes(bytes: &[u8], origem: Option<PathBuf>) -> Result<Self, String> {
        Self::de_bytes_com_senha(bytes, origem, None)
    }

    /// Abre com a senha de abrir (ou a do dono). PDF que pede senha e não a recebeu devolve
    /// [`PRECISA_SENHA`]; senha errada, [`SENHA_ERRADA`]. A proteção volta ao salvar
    /// ([`Self::bytes_para_gravar`]) até [`Self::tirar_senha`].
    pub fn de_bytes_com_senha(bytes: &[u8], origem: Option<PathBuf>, senha: Option<&str>) -> Result<Self, String> {
        let opcoes = senha.map_or_else(lopdf::LoadOptions::default, lopdf::LoadOptions::with_password);
        let documento = Document::load_mem_with_options(bytes, opcoes).map_err(|e| match e {
            lopdf::Error::InvalidPassword => SENHA_ERRADA.to_string(),
            e => format!("Nao foi possivel ler o PDF: {e}"),
        })?;
        // Sem a senha o lopdf devolve o documento ainda cifrado (com o /Encrypt no trailer).
        if documento.is_encrypted() {
            return Err(if senha.is_some() { SENHA_ERRADA } else { PRECISA_SENHA }.into());
        }
        if documento.get_pages().is_empty() {
            return Err("O PDF nao possui paginas.".into());
        }
        let modificado = origem.is_none();
        let mut documento = documento;
        normalizar_cmaps(&mut documento);
        Ok(Self { documento, origem, modificado })
    }

    pub fn origem(&self) -> Option<&Path> { self.origem.as_deref() }

    /// Veio com senha ou restrições (e elas voltam ao salvar).
    pub fn tem_senha(&self) -> bool { self.documento.encryption_state.is_some() }

    /// Como o arquivo aberto está protegido (cifra e o que ele não deixa fazer); `None` sem proteção.
    pub fn seguranca(&self) -> Option<String> {
        use lopdf::Permissions as P;
        let e = self.documento.encryption_state.as_ref()?;
        let cifra = match e.version() { 5 => "AES-256", 4 => "128 bits (AES ou RC4)", _ => "RC4 antigo (fraco)" };
        let p = e.permissions();
        let negadas: Vec<&str> = [(P::PRINTABLE, "imprimir"), (P::COPYABLE, "copiar"), (P::MODIFIABLE, "alterar"),
            (P::ANNOTABLE, "comentar"), (P::FILLABLE, "preencher campos"), (P::ASSEMBLABLE, "mexer nas páginas")]
            .into_iter().filter(|(f, _)| !p.contains(*f)).map(|(_, n)| n).collect();
        Some(if negadas.is_empty() { format!("Cifrado com {cifra}, sem restrições.") }
            else { format!("Cifrado com {cifra}; não deixa {}.", negadas.join(", ")) })
    }

    /// A cópia salva sai sem senha nem restrições (quem abriu com a senha pode).
    pub fn tirar_senha(&mut self) {
        if self.documento.encryption_state.take().is_some() { self.modificado = true; }
    }

    /// Bytes para o arquivo da pessoa: cifrados de novo com a mesma senha e permissões do original.
    /// O [`Self::serializar`] fica sempre aberto, para o desenho, as cópias e o PDF/A.
    pub fn bytes_para_gravar(&self) -> Result<Vec<u8>, String> {
        let Some(estado) = &self.documento.encryption_state else { return self.serializar() };
        let mut documento = self.documento.clone();
        podar(&mut documento);
        documento.encryption_state = None;
        documento.encrypt(estado).map_err(|e| format!("Falha ao manter a senha do PDF: {e}"))?;
        let mut bytes = Vec::new();
        documento.save_to(&mut bytes).map_err(|e| format!("Falha ao serializar PDF: {e}"))?;
        Ok(bytes)
    }
    pub fn modificado(&self) -> bool { self.modificado }
    /// Desfazer para um estado anterior ao último salvamento: o disco já tem outra coisa.
    pub fn marcar_modificado(&mut self) { self.modificado = true; }
    pub fn total_paginas(&self) -> u32 { self.ids_paginas().len() as u32 }

    /// Páginas em ordem, sem repetir: numa árvore com ciclo o lopdf lista a mesma página várias vezes.
    /// As páginas em ordem, pelo objeto de cada uma: muda quando páginas saem, entram ou trocam de lugar.
    pub fn ordem_paginas(&self) -> Vec<(u32, u16)> { self.ids_paginas() }

    pub(crate) fn ids_paginas(&self) -> Vec<lopdf::ObjectId> {
        let mut vistas = std::collections::HashSet::new();
        self.documento.get_pages().into_values().filter(|id| vistas.insert(*id)).collect()
    }

    fn id_pagina(&self, pagina: u32) -> Result<lopdf::ObjectId, String> {
        pagina.checked_sub(1).and_then(|i| self.ids_paginas().get(i as usize).copied())
            .ok_or_else(|| format!("Pagina {pagina} inexistente."))
    }

    /// Extracao textual auxiliar. Nao substitui a renderizacao fiel das paginas.
    /// Impoe limite de memoria na descompressao de streams.
    pub fn texto_pagina(&self, pagina: u32) -> Result<String, String> {
        self.id_pagina(pagina)?;
        self.documento.extract_text_with_limit(&[pagina], MAX_TEXTO_PAGINA)
            .map_err(|e| format!("Nao foi possivel extrair texto desta pagina: {e}"))
    }

    /// Serializa o documento alterado sem realizar IO ou converter paginas em imagem.
    pub fn serializar(&self) -> Result<Vec<u8>, String> {
        let mut documento = self.documento.clone();
        // Páginas excluídas e o catálogo de um PDF juntado ficam soltos até aqui.
        podar(&mut documento);
        let mut bytes = Vec::new();
        documento.save_to(&mut bytes)
            .map_err(|e| format!("Falha ao serializar PDF: {e}"))?;
        Ok(bytes)
    }

    /// Gravação em segundo plano começou: daqui em diante só uma edição nova marca de novo
    /// (se a gravação falhar, quem chama usa `marcar_modificado`).
    pub fn comecar_a_salvar(&mut self) { self.modificado = false; }
    /// A gravação em segundo plano terminou em `destino`: o arquivo passa a ser esse, sem
    /// mexer em `modificado` (uma edição feita durante a gravação continua por salvar).
    pub fn gravado_em(&mut self, destino: PathBuf) { self.origem = Some(destino); }

    /// Apenas o adaptador de IO confirma que a gravacao atomica terminou.
    pub fn confirmar_salvo(&mut self, destino: PathBuf) {
        self.origem = Some(destino);
        self.modificado = false;
    }
}

/// Tira os objetos que nada alcança a partir do trailer. Faz o mesmo que o `prune_objects`
/// do lopdf, que é quadrático (procura em `Vec`) e trava documentos com dezenas de milhares de objetos.
pub(crate) fn podar(documento: &mut Document) {
    fn refs(o: &lopdf::Object, fila: &mut Vec<lopdf::ObjectId>) {
        match o {
            lopdf::Object::Reference(id) => fila.push(*id),
            lopdf::Object::Array(v) => v.iter().for_each(|o| refs(o, fila)),
            lopdf::Object::Dictionary(d) => d.iter().for_each(|(_, o)| refs(o, fila)),
            lopdf::Object::Stream(s) => s.dict.iter().for_each(|(_, o)| refs(o, fila)),
            _ => {}
        }
    }
    let mut fila = Vec::new();
    documento.trailer.iter().for_each(|(_, o)| refs(o, &mut fila));
    let mut vistos = std::collections::HashSet::new();
    while let Some(id) = fila.pop() {
        if vistos.insert(id) { if let Some(o) = documento.objects.get(&id) { refs(o, &mut fila); } }
    }
    documento.objects.retain(|id, _| vistos.contains(id));
}

/// Separa os tokens colados de um CMap (`/CMapName/Adobe-Identity-UCS`, `/Registry(Adobe)`,
/// `>>def`), como o AutoCAD e outros grava: o parser do lopdf exige o espaço e, sem ele, perde o
/// /ToUnicode inteiro, e o texto da página some na busca, na cópia e na edição.
pub(crate) fn espacar_cmap(c: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(c.len() + 64);
    for (i, &b) in c.iter().enumerate() {
        let colado = o.last().is_some_and(|a: &u8| !a.is_ascii_whitespace() && !matches!(a, b'<' | b'[' | b'(' | b'{'));
        if matches!(b, b'/' | b'(' | b'<' | b'[') && colado { o.push(b' '); }
        o.push(b);
        if b == b'>' && c.get(i + 1).is_some_and(u8::is_ascii_alphanumeric) { o.push(b' '); }
    }
    o
}

/// Aplica [`espacar_cmap`] aos /ToUnicode das fontes (só quando muda algo).
fn normalizar_cmaps(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.values().filter_map(|o| o.as_dict().ok())
        .filter_map(|d| d.get(b"ToUnicode").and_then(Object::as_reference).ok()).collect();
    for id in ids {
        let Ok(s) = doc.get_object_mut(id).and_then(Object::as_stream_mut) else { continue };
        let Ok(plano) = s.decompressed_content_with_limit(crate::TETO_FLUXO).or_else(|_| Ok::<_, lopdf::Error>(s.content.clone())) else { continue };
        if !plano.windows(9).any(|w| w == b"begincmap") { continue; }
        let novo = espacar_cmap(&plano);
        if novo != plano { s.set_plain_content(novo); }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cmap_colado_do_cad_volta_a_ler() {
        // O /ToUnicode como o AutoCAD grava: nomes e valores sem espaço entre eles.
        let cmap = b"/CIDInit/ProcSet findresource begin\n12 dict begin begincmap\n/CMapName/Adobe-Identity-UCS def\n\
/CIDSystemInfo<</Registry(Adobe)/Ordering(UCS)/Supplement 0>>def\n/CMapType 2 def\n1 begincodespacerange\n<0000><FFFF>\n\
endcodespacerange\n2 beginbfchar\n<0001><0034>\n<0002><0032>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop end end";
        let mut doc = lopdf::Document::with_version("1.7");
        let tu = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), cmap.to_vec()));
        let fonte = doc.add_object(lopdf::dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Verdana", "ToUnicode" => tu });
        let ler = |doc: &lopdf::Document| {
            let d = doc.get_dictionary(fonte).unwrap();
            lopdf::Document::decode_text(&d.get_font_encoding(doc).unwrap(), &[0, 1, 0, 2, 0, 2]).unwrap()
        };
        assert_eq!(ler(&doc), "", "o lopdf sozinho perde o mapa");
        normalizar_cmaps(&mut doc);
        assert_eq!(ler(&doc), "422");
        assert_eq!(espacar_cmap(b"/CMapName/Adobe def <0001><0034> /Registry(A)>>def"), b"/CMapName /Adobe def <0001> <0034> /Registry (A)>> def".to_vec());
    }
    use feather_draw::Cena;
    use feather_io::{Documento, Registro};

    pub(crate) fn pdf_teste() -> DocumentoPdf {
        let bytes = Registro::padrao()
            .gravar("pdf", &Documento::Desenho(Cena::nova(595.0, 842.0))).unwrap();
        DocumentoPdf::de_bytes(&bytes, None).unwrap()
    }

    #[test]
    fn cria_pdf_por_motor_compartilhado_e_gira_pagina() {
        let mut pdf = pdf_teste();
        assert_eq!(pdf.total_paginas(), 1);
        pdf.girar_paginas(&[1], 90).unwrap();
        let id = pdf.id_pagina(1).unwrap();
        let grau = pdf.documento.get_dictionary(id).unwrap().get(b"Rotate").unwrap().as_i64().unwrap();
        assert_eq!(grau, 90);
        pdf.girar_paginas(&[1], 270).unwrap();
        let grau = pdf.documento.get_dictionary(id).unwrap().get(b"Rotate").unwrap().as_i64().unwrap();
        assert_eq!(grau, 0);
    }

    #[test]
    fn rejeita_apagar_ultima_pagina_e_operar_indice_invalido() {
        let mut pdf = pdf_teste();
        assert!(pdf.excluir_paginas(&[1]).is_err());
        assert!(pdf.excluir_paginas(&[2]).is_err());
        assert!(pdf.girar_paginas(&[2], 90).is_err());
        assert_eq!(pdf.total_paginas(), 1);
    }

    #[test]
    fn serializacao_preserva_giro_e_estado() {
        let mut pdf = pdf_teste();
        pdf.girar_paginas(&[1], 90).unwrap();
        let bytes = pdf.serializar().unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let recarregado = DocumentoPdf::de_bytes(&bytes, Some(PathBuf::from("teste.pdf"))).unwrap();
        assert_eq!(recarregado.total_paginas(), 1);
        let id = recarregado.id_pagina(1).unwrap();
        let graus = recarregado.documento.get_dictionary(id).unwrap()
            .get(b"Rotate").unwrap().as_i64().unwrap();
        assert_eq!(graus, 90);
        assert!(!recarregado.modificado());
        pdf.confirmar_salvo(PathBuf::from("teste.pdf"));
        assert!(!pdf.modificado());
    }
}

/// Descomprime um fluxo zlib (FlateDecode) parando em `teto` bytes: `None` se passa do teto
/// ou se o fluxo está quebrado. Protege contra o fluxo pequeno que infla para gigabytes.
pub(crate) fn inflar_com_teto(dados: &[u8], teto: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut saida = Vec::new();
    flate2::read::ZlibDecoder::new(dados).take(teto + 1).read_to_end(&mut saida).ok()?;
    (saida.len() as u64 <= teto).then_some(saida)
}

#[cfg(test)]
mod testes_teto {
    use lopdf::{dictionary, Object, Stream};

    fn zlib(dados: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        e.write_all(dados).unwrap();
        e.finish().unwrap()
    }

    /// Audit: 10 KB comprimidos que viram 10 MB param no teto; imagem com medidas absurdas não
    /// estoura a conta; anexo grande não é lido pela busca.
    #[test]
    fn fluxo_que_explode_para_no_teto() {
        let bomba = zlib(&vec![0u8; 10 << 20]);
        assert!(bomba.len() < 64 << 10);
        assert!(super::inflar_com_teto(&bomba, 1 << 20).is_none());
        assert_eq!(super::inflar_com_teto(&bomba, 10 << 20).map(|v| v.len()), Some(10 << 20));

        let mut pdf = crate::testes::pdf_teste();
        let imagem = pdf.documento.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image",
            "Width" => 4_000_000_000i64, "Height" => 4_000_000_000i64, "BitsPerComponent" => 8, "ColorSpace" => "DeviceRGB",
            "Filter" => "FlateDecode" }, bomba.clone()));
        assert!(crate::otimizar::ler_imagem(&pdf.documento, imagem).is_none());

        let arquivo = pdf.documento.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile", "Filter" => "FlateDecode" }, bomba));
        let spec = pdf.documento.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("grande.txt"), "EF" => dictionary! { "F" => arquivo } });
        let nomes = pdf.documento.add_object(dictionary! { "Names" => vec![Object::string_literal("grande.txt"), Object::Reference(spec)] });
        pdf.documento.catalog_mut().unwrap().set("Names", dictionary! { "EmbeddedFiles" => nomes });
        assert!(pdf.anexo_ate(0, 4 << 20).is_none(), "a busca não lê anexo acima do teto");
        assert_eq!(pdf.anexos()[0].tamanho, 10 << 20, "medido descomprimindo até 64 MB");
    }
}

