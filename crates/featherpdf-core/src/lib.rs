//! Motor do FeatherPDF. O parser/editor de objetos PDF e especifico deste produto;
//! conversao/arquivos ficam no FeatherPDF-IO e nos motores comuns da suíte FeatherWorks.
//! Este crate nao depende de GUI/Iced.

use std::path::{Path, PathBuf};
use lopdf::Document;

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
mod sanitizar;
mod tabela;
mod tabelas;
mod pdfa;
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
pub use navegacao::{Anexo, Marcador};
pub use anotacoes::{cor_do_carimbo, hoje, Anotacao, AnotacaoNova, Camada, Marcacao};
pub use formularios::{Campo, CampoNovo, TipoCampo};
pub use fluxo::{BlocoFluxo, CelulaFluxo, PaginaFluxo};
pub use inserir::{DesenhoNovo, Destino, ImagemNova};
pub use inspecao::{tamanho, Cor, DetalheObjeto, FonteInfo, Gravidade, ImagemInfo, Inspecao, LinhaArvore, NoEstrutura, Pedaco, Problema, Resumo};
pub use medida::{comprimento, medir, numero, Escala, TipoMedida, Unidade};
pub use objetos::{achar_sem_caixa, EstiloDesenho, ObjetoPdf, TipoObjeto};
pub use otimizar::{ImagemCrua, OpcoesCompressao, Permissoes, Peso, Protecao, Reamostrar};
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
        Ok(Self { documento, origem, modificado })
    }

    pub fn origem(&self) -> Option<&Path> { self.origem.as_deref() }

    /// Veio com senha ou restrições (e elas voltam ao salvar).
    pub fn tem_senha(&self) -> bool { self.documento.encryption_state.is_some() }

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

#[cfg(test)]
mod testes {
    use super::*;
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
