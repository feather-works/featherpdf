//! Janela Fluxos em blocos: escolher a entrada (pasta ou o PDF aberto), encadear blocos
//! (girar → OCR → tarjar padrões → comprimir…), testar com o primeiro arquivo e rodar em
//! todos. Os originais ficam; os prontos vão para a pasta de saída. Os blocos de PDF estão em
//! `featherpdf_io::executar_fluxo`; o OCR usa o mesmo leitor da janela de OCR.

use std::path::{Path as Caminho, PathBuf};

use feather_ui::{cromo, tema};
use featherpdf_core::{DocumentoPdf, Padrao};
use featherpdf_io::{Bloco, Visualizador, NIVEIS_COMPRESSAO, PAGINAS_GIRAR, TAMANHOS_PAGINA};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::Cursor;
use crate::compat::canvas::{self, Frame, Geometry, Path, Stroke};
use crate::{App, Auxiliar, Msg};

const VERMELHO: Color = Color::rgb(0.77, 0.21, 0.17);
/// Onde o bloco arrastado vai entrar (o destaque do tema do PDF é vermelho, igual ao "escolhido").
const AZUL: Color = Color::rgb(0.10, 0.45, 0.91);
const VERDE: Color = Color::rgb(0.157, 0.47, 0.275);
/// Cartão dos blocos no meio: largura, altura e o passo da grade de duas colunas.
const CARTAO: (f32, f32) = (180.0, 62.0);
const PASSO: (f32, f32) = (250.0, 130.0);
const MARGEM: (f32, f32) = (50.0, 40.0);

#[derive(Clone, PartialEq)]
pub enum Passo {
    Bloco(Bloco),
    Ocr { so_sem_texto: bool },
    /// Senha para abrir (AES-256). Só na memória: nunca vai a disco nem a log.
    Senha(String),
    /// Assinatura digital com certificado A1 (.pfx/.p12) e a senha dele, só na memória.
    Assinar { certificado: Option<PathBuf>, senha: String },
}

// As senhas nunca aparecem no Debug (que pode ir para log).
impl std::fmt::Debug for Passo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Passo::Bloco(b) => write!(f, "Bloco({b:?})"),
            Passo::Ocr { so_sem_texto } => write!(f, "Ocr({so_sem_texto})"),
            Passo::Senha(_) => write!(f, "Senha(***)"),
            Passo::Assinar { certificado, .. } => write!(f, "Assinar({certificado:?}, ***)"),
        }
    }
}

impl Passo {
    /// Senha e assinatura mexem no arquivo inteiro: vão sempre no fim, nessa ordem.
    fn ordem_final(&self) -> u8 { match self { Passo::Senha(_) => 1, Passo::Assinar { .. } => 2, _ => 0 } }
}

/// Como os PDFs saem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo { PorArquivo, Juntar, Dividir, Png, Word, Planilha }

impl Modo {
    const TODOS: [Modo; 6] = [Modo::PorArquivo, Modo::Juntar, Modo::Dividir, Modo::Png, Modo::Word, Modo::Planilha];
    fn nome(self) -> &'static str {
        match self {
            Modo::PorArquivo => "Um PDF por arquivo (mesmo nome)",
            Modo::Juntar => "Juntar tudo num PDF só (ordem do nome)",
            Modo::Dividir => "Dividir: uma página por PDF",
            Modo::Png => "Imagens PNG, uma por página",
            Modo::Word => "Documento Word (.docx)",
            Modo::Planilha => "Planilha (.xlsx)",
        }
    }
    fn pdf(self) -> bool { matches!(self, Modo::PorArquivo | Modo::Juntar | Modo::Dividir) }
}

/// O cartão escolhido: entrada, um dos passos ou a saída.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sel { Entrada, Passo(usize), Saida }

#[derive(Debug, Clone)]
pub enum Fx {
    Abrir,
    UsarAberto,
    EscolherPasta,
    Pasta(Option<PathBuf>),
    EscolherSaida,
    Saida(Option<PathBuf>),
    Acrescentar(Passo),
    Selecionar(Sel),
    Mudar(usize, Passo),
    Mover(usize, bool),
    Tirar(usize),
    /// Começa a arrastar um bloco da paleta ou um cartão do meio.
    Pegar(Pego),
    Cursor(Point),
    /// O cartão debaixo do cursor ao arrastar (posição na tela: 0 é a entrada); o bloco entra antes dele.
    Alvo(Option<usize>),
    SobreMesa(bool),
    Largar,
    /// Arquivo ou pasta solto na janela, vindo do gerenciador de arquivos.
    Solto(PathBuf),
    Modo(Modo),
    Modelo(usize),
    EscolherCertificado(usize),
    Certificado(usize, Option<PathBuf>),
    Testar,
    Testado(Result<(String, Vec<String>), String>),
    Rodar,
    Feito(Result<String, String>),
    /// Menu do botão direito num bloco: qual e onde (na janela); `None` fecha.
    Menu(Option<(usize, Point)>),
    Duplicar(usize),
    /// Delete na janela: tira o bloco escolhido.
    TirarEscolhido,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pego { Novo(Passo), Cartao(usize), Pasta, Aberto, Saida }

impl Pego {
    fn nome(&self, passos: &[Passo]) -> String {
        match self {
            Pego::Novo(p) => p.nome().into(),
            Pego::Cartao(i) => passos.get(*i).map_or("Bloco", |p| p.nome()).into(),
            Pego::Pasta => "Pasta de arquivos".into(),
            Pego::Aberto => "Arquivo aberto".into(),
            Pego::Saida => "Salvar PDF em pasta".into(),
        }
    }
}

/// `pos`: onde o cursor está, para o fantasma do bloco ir junto.
struct Arrasto { pego: Pego, inicio: Option<Point>, pos: Option<Point>, moveu: bool, alvo: Option<usize>, sobre_mesa: bool }

pub struct Fluxos {
    /// `None`: o PDF aberto.
    pasta: Option<PathBuf>,
    arquivos: Vec<PathBuf>,
    /// `None`: a subpasta "processados" ao lado da entrada.
    saida: Option<PathBuf>,
    passos: Vec<Passo>,
    modo: Modo,
    sel: Sel,
    arrasto: Option<Arrasto>,
    menu: Option<(usize, Point)>,
    pub rodando: bool,
    previa: Option<Result<(String, Vec<String>), String>>,
    resultado: Option<Result<String, String>>,
}

impl Default for Fluxos {
    fn default() -> Self {
        Self { pasta: None, arquivos: vec![], saida: None, passos: vec![], modo: Modo::PorArquivo,
            sel: Sel::Entrada, arrasto: None, menu: None, rodando: false, previa: None, resultado: None }
    }
}

/// Fluxos prontos: nome, blocos e como saem.
fn modelos() -> Vec<(&'static str, Vec<Passo>, Modo)> {
    use Bloco::*;
    vec![
        ("Comprimir vários PDFs", vec![Passo::Bloco(Comprimir(1))], Modo::PorArquivo),
        ("Comprimir para e-mail", vec![Passo::Bloco(LimparMetadados), Passo::Bloco(Comprimir(2))], Modo::PorArquivo),
        ("Fotos → um PDF só", vec![Passo::Bloco(Comprimir(1))], Modo::Juntar),
        ("Limpar scans e fazer OCR", vec![Passo::Bloco(TirarBrancas), Passo::Bloco(ClarearFundo), Passo::Ocr { so_sem_texto: true }], Modo::PorArquivo),
        ("Tirar dados pessoais", vec![Passo::Bloco(TarjarPadroes([true; 6])), Passo::Bloco(LimparMetadados)], Modo::PorArquivo),
        ("Arquivo de contratos", vec![Passo::Bloco(TarjarPadroes([true; 6])), Passo::Bloco(LimparMetadados), Passo::Bloco(PdfA)], Modo::PorArquivo),
        ("Plantas para cliente", vec![Passo::Bloco(Normalizar(1)), Passo::Bloco(MarcaDagua("PRELIMINAR".into())), Passo::Bloco(Linearizar)], Modo::PorArquivo),
    ]
}

/// A paleta da esquerda: grupo e os passos que ele acrescenta.
fn paleta() -> [(&'static str, Vec<Passo>); 3] {
    use Bloco::*;
    [
        ("Páginas", vec![Passo::Bloco(Girar { graus: 90, quais: 0 }), Passo::Bloco(Numerar), Passo::Bloco(TirarBrancas), Passo::Bloco(Normalizar(0)),
            Passo::Bloco(Cabecalho { cabecalho: "{arquivo}".into(), rodape: "Página {pagina} de {total}".into() })]),
        ("Conteúdo", vec![Passo::Ocr { so_sem_texto: true }, Passo::Bloco(TarjarPadroes([true; 6])),
            Passo::Bloco(MarcaDagua("CONFIDENCIAL".into())), Passo::Bloco(ClarearFundo), Passo::Bloco(LimparCaderno),
            Passo::Bloco(LimparMetadados), Passo::Bloco(EmbutirFontes), Passo::Bloco(Comprimir(1)), Passo::Bloco(PdfA), Passo::Bloco(Linearizar)]),
        ("Proteger", vec![Passo::Senha(String::new()), Passo::Assinar { certificado: None, senha: String::new() }]),
    ]
}

impl Passo {
    fn nome(&self) -> &'static str {
        match self { Passo::Bloco(b) => b.nome(), Passo::Ocr { .. } => "OCR", Passo::Senha(_) => "Proteger com senha", Passo::Assinar { .. } => "Assinar digitalmente" }
    }
    fn grupo(&self) -> &'static str {
        match self {
            Passo::Bloco(Bloco::Girar { .. } | Bloco::Numerar | Bloco::TirarBrancas | Bloco::Normalizar(_) | Bloco::Cabecalho { .. }) => "Páginas",
            Passo::Senha(_) | Passo::Assinar { .. } => "Proteger",
            _ => "Conteúdo",
        }
    }
    /// As duas linhas do cartão.
    fn resumo(&self) -> String {
        match self.clone() {
            Passo::Bloco(Bloco::Girar { graus, quais }) => format!("{graus}° no sentido horário\n{}",
                PAGINAS_GIRAR[(quais as usize).min(PAGINAS_GIRAR.len() - 1)].to_lowercase().replace("todas", "todas as páginas")),
            Passo::Bloco(Bloco::Numerar) => "“Página N de T”\nrodapé, centro".into(),
            Passo::Bloco(Bloco::TirarBrancas) => "páginas sem nada\nsaem do arquivo".into(),
            Passo::Bloco(Bloco::TarjarPadroes(q)) => {
                let n: Vec<&str> = Padrao::TODOS.iter().zip(q).filter(|(_, q)| *q).map(|(p, _)| p.nome()).collect();
                if n.is_empty() { "nenhum padrão".into() } else { format!("{}\napaga o texto embaixo", n.join(", ")) }
            }
            Passo::Bloco(Bloco::Comprimir(n)) => {
                let (nome, dpi, q) = NIVEIS_COMPRESSAO[(n as usize).min(2)];
                format!("{nome}: fotos a {dpi:.0} dpi\nqualidade {q}")
            }
            Passo::Bloco(Bloco::Normalizar(t)) => format!("todas as páginas em {}\nconteúdo ajustado", TAMANHOS_PAGINA[(t as usize).min(3)].0),
            Passo::Bloco(Bloco::PdfA) => "PDF/A-2b, para arquivar\nembute as fontes".into(),
            Passo::Bloco(Bloco::MarcaDagua(t)) => format!("“{t}”\nna diagonal, translúcida"),
            Passo::Bloco(Bloco::Cabecalho { cabecalho, rodape }) => format!("alto: {cabecalho}\npé: {rodape}"),
            Passo::Senha(s) => format!("{}\nAES-256 · vai no fim", if s.is_empty() { "falta a senha" } else { "senha definida" }),
            Passo::Assinar { certificado, .. } => format!("{}\nvai sempre por último",
                certificado.as_deref().map_or_else(|| "escolha o certificado".into(), nome)),
            Passo::Ocr { so_sem_texto } => format!("{}\nportuguês",
                if so_sem_texto { "Só páginas sem texto" } else { "Todas as páginas" }),
            Passo::Bloco(b) => format!("{}\n", b.nome()),
        }
    }
    fn cor_cabeca(&self) -> Color {
        match self.grupo() { "Páginas" => Color::rgb8(0xe8, 0xf0, 0xfe), "Proteger" => Color::rgb8(0xfc, 0xe8, 0xe6), _ => Color::rgb8(0xfb, 0xf3, 0xdc) }
    }
}

/// Aviso de ordem para o passo `i` (âmbar, não impede rodar).
fn aviso_ordem(passos: &[Passo], i: usize) -> Option<&'static str> {
    let depois = &passos[i + 1..];
    match &passos[i] {
        Passo::Bloco(Bloco::Comprimir(_)) if depois.iter().any(|p| matches!(p, Passo::Ocr { .. } | Passo::Bloco(Bloco::TarjarPadroes(_)))) =>
            Some("antes do OCR/tarja piora a leitura"),
        Passo::Bloco(Bloco::Linearizar) if depois.iter().any(|p| p.ordem_final() == 0) => Some("blocos depois desfazem o linearizar"),
        Passo::Senha(_) | Passo::Assinar { .. } if passos.iter().any(|p| matches!(p, Passo::Senha(_))) && passos.iter().any(|p| matches!(p, Passo::Assinar { .. })) =>
            Some("senha e assinatura juntas não dá"),
        _ => None,
    }
}

/// O que impede testar ou rodar (senha faltando, certificado faltando…).
fn impedimento(passos: &[Passo], modo: Modo) -> Option<String> {
    let senha = passos.iter().find_map(|p| match p { Passo::Senha(s) => Some(s), _ => None });
    let assinar = passos.iter().find_map(|p| match p { Passo::Assinar { certificado, senha } => Some((certificado, senha)), _ => None });
    if senha.is_some() && assinar.is_some() { return Some("Senha e assinatura juntas não dá: o PDF com senha não pode ser assinado. Tire um dos dois.".into()); }
    if senha.is_some_and(|s| s.is_empty()) { return Some("Digite a senha do bloco “Proteger com senha”.".into()); }
    if let Some((c, s)) = assinar {
        if c.is_none() { return Some("Escolha o certificado (.pfx/.p12): sem certificado a assinatura não tem validade.".into()); }
        if s.is_empty() { return Some("Digite a senha do certificado.".into()); }
    }
    if (senha.is_some() || assinar.is_some()) && !modo.pdf() { return Some("Senha e assinatura só valem com saída em PDF.".into()); }
    None
}

/// Põe senha e assinatura no fim (nessa ordem), sem mexer no resto. Diz se mudou algo.
fn finais_no_fim(passos: &mut [Passo]) -> bool {
    let antes = passos.to_vec();
    passos.sort_by_key(Passo::ordem_final);
    passos != antes.as_slice()
}

impl Fluxos {
    pub fn arrastando(&self) -> bool { self.arrasto.is_some() }
}

/// PDF ou imagem (a imagem vira um PDF de uma página).
fn serve(p: &Caminho) -> bool {
    p.is_file() && (p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) || crate::juntar::e_imagem(p))
}

fn arquivos_da_pasta(pasta: &Caminho) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(pasta).map(|l| l.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| serve(p)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn nome(p: &Caminho) -> String { p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()) }

impl App {
    pub fn update_fluxos(&mut self, m: Fx) -> Task<Msg> {
        let f = &mut self.fluxos;
        // Qualquer ação fecha o menu do botão direito (só o mouse andando não).
        if !matches!(m, Fx::Menu(_) | Fx::Cursor(_) | Fx::Alvo(_) | Fx::SobreMesa(_)) { f.menu = None; }
        match m {
            Fx::Abrir => return self.alternar_auxiliar(Auxiliar::Fluxos),
            Fx::Menu(m) => f.menu = m,
            Fx::Duplicar(i) => if let Some(p) = f.passos.get(i).cloned() {
                f.passos.insert(i + 1, p);
                finais_no_fim(&mut f.passos);
                f.sel = Sel::Passo((i + 1).min(f.passos.len() - 1));
                f.previa = None;
            },
            Fx::TirarEscolhido => if let Sel::Passo(i) = f.sel { return self.update_fluxos(Fx::Tirar(i)) },
            Fx::UsarAberto => { f.pasta = None; f.arquivos.clear(); f.sel = Sel::Entrada; f.previa = None; }
            Fx::EscolherPasta => return Task::perform(async {
                rfd::AsyncFileDialog::new().set_title("Pasta com os PDFs").pick_folder().await.map(|h| h.path().to_path_buf())
            }, |p| Msg::Fluxos(Fx::Pasta(p))),
            Fx::Pasta(Some(p)) => { f.arquivos = arquivos_da_pasta(&p); f.pasta = Some(p); f.sel = Sel::Entrada; f.previa = None; }
            Fx::Solto(p) => if p.is_dir() { return self.update_fluxos(Fx::Pasta(Some(p))) } else if serve(&p) {
                // Arquivos soltos um a um formam a entrada; um de outra pasta recomeça a lista.
                if f.pasta.as_deref() != p.parent() { f.pasta = p.parent().map(Caminho::to_path_buf); f.arquivos.clear(); }
                if !f.arquivos.contains(&p) { f.arquivos.push(p); f.arquivos.sort(); }
                f.sel = Sel::Entrada;
                f.previa = None;
            },
            Fx::Modo(m) => { f.modo = m; f.previa = None; }
            Fx::EscolherCertificado(i) => return Task::perform(async {
                rfd::AsyncFileDialog::new().set_title("Certificado A1").add_filter("Certificado", &["pfx", "p12"]).pick_file().await
                    .map(|h| h.path().to_path_buf())
            }, move |c| Msg::Fluxos(Fx::Certificado(i, c))),
            Fx::Certificado(i, Some(c)) => if let Some(Passo::Assinar { certificado, .. }) = f.passos.get_mut(i) { *certificado = Some(c); f.previa = None; },
            Fx::Certificado(_, None) => {}
            Fx::Modelo(i) => if let Some((_, passos, modo)) = modelos().into_iter().nth(i) {
                f.passos = passos;
                f.modo = modo;
                f.sel = Sel::Entrada;
                f.previa = None;
            },
            Fx::Pegar(pego) => f.arrasto = Some(Arrasto { pego, inicio: None, pos: None, moveu: false, alvo: None, sobre_mesa: false }),
            Fx::Cursor(p) => if let Some(a) = &mut f.arrasto {
                a.pos = Some(p);
                // Uns pixels de tremida no clique não contam como arrastar.
                match a.inicio { None => a.inicio = Some(p), Some(i) => if (p.x - i.x).abs() + (p.y - i.y).abs() > 6.0 { a.moveu = true } }
            },
            Fx::Alvo(alvo) => if let Some(a) = &mut f.arrasto { a.alvo = alvo },
            Fx::SobreMesa(v) => if let Some(a) = &mut f.arrasto { a.sobre_mesa = v },
            Fx::Largar => if let Some(a) = f.arrasto.take() {
                let fim = f.passos.len();
                // Cartão k da tela → vaga na lista de passos (a entrada e o 1º passo dão 0; a saída, o fim).
                let a = Arrasto { alvo: a.alvo.map(|k| k.saturating_sub(1).min(fim)), ..a };
                // Entrada e saída: soltas no meio (ou só clicadas) fazem o mesmo que o botão.
                let vale = a.sobre_mesa || a.alvo.is_some() || !a.moveu;
                match a.pego {
                    Pego::Pasta => if vale { return self.update_fluxos(Fx::EscolherPasta) },
                    Pego::Aberto => if vale { return self.update_fluxos(Fx::UsarAberto) },
                    Pego::Saida => if vale { return self.update_fluxos(Fx::EscolherSaida) },
                    // Clique sem arrastar também acrescenta no fim.
                    Pego::Novo(p) => if let Some(i) = a.alvo.filter(|_| a.moveu).or((a.sobre_mesa || !a.moveu).then_some(fim)) {
                        f.passos.insert(i.min(fim), p.clone());
                        f.sel = Sel::Passo(i.min(fim));
                        f.previa = None;
                        if finais_no_fim(&mut f.passos) {
                            f.sel = Sel::Passo(f.passos.iter().position(|x| *x == p).unwrap_or(0));
                            f.resultado = Some(Ok("Senha e assinatura vão sempre no fim: o resto entra antes.".into()));
                        }
                    },
                    Pego::Cartao(i) => match a.alvo.filter(|_| a.moveu) {
                        Some(j) if i < fim && j != i && j != i + 1 => {
                            let p = f.passos.remove(i);
                            let j = if j > i { j - 1 } else { j };
                            f.passos.insert(j, p.clone());
                            f.sel = Sel::Passo(j);
                            f.previa = None;
                            if finais_no_fim(&mut f.passos) {
                                f.sel = Sel::Passo(f.passos.iter().position(|x| *x == p).unwrap_or(0));
                                f.resultado = Some(Ok("Senha e assinatura vão sempre no fim.".into()));
                            }
                        }
                        // Solto fora do meio (na paleta, por exemplo): sai do fluxo.
                        None if a.moveu && !a.sobre_mesa => return self.update_fluxos(Fx::Tirar(i)),
                        _ => f.sel = Sel::Passo(i),
                    },
                }
            },
            Fx::EscolherSaida => return Task::perform(async {
                rfd::AsyncFileDialog::new().set_title("Onde salvar os PDFs prontos").pick_folder().await.map(|h| h.path().to_path_buf())
            }, |p| Msg::Fluxos(Fx::Saida(p))),
            Fx::Saida(Some(p)) => { f.saida = Some(p); f.sel = Sel::Saida; }
            Fx::Pasta(None) | Fx::Saida(None) => {}
            Fx::Acrescentar(p) => {
                f.passos.push(p.clone());
                finais_no_fim(&mut f.passos);
                f.sel = Sel::Passo(f.passos.iter().rposition(|x| *x == p).unwrap_or(0));
                f.previa = None;
            }
            Fx::Selecionar(s) => f.sel = s,
            Fx::Mudar(i, p) => if let Some(x) = f.passos.get_mut(i) { *x = p; f.previa = None; },
            Fx::Mover(i, cima) => {
                let j = if cima { i.checked_sub(1) } else { Some(i + 1).filter(|j| *j < f.passos.len()) };
                // Senha e assinatura não saem do fim.
                if let Some(j) = j.filter(|&j| f.passos[i].ordem_final() == 0 && f.passos[j].ordem_final() == 0) {
                    f.passos.swap(i, j); f.sel = Sel::Passo(j); f.previa = None;
                }
            }
            Fx::Tirar(i) => if i < f.passos.len() {
                f.passos.remove(i);
                f.sel = if f.passos.is_empty() { Sel::Entrada } else { Sel::Passo(i.min(f.passos.len() - 1)) };
                f.previa = None;
            },
            Fx::Testar => {
                if impedimento(&self.fluxos.passos, self.fluxos.modo).is_some() { return Task::none(); }
                let Some((rotulo, abrir)) = self.primeiro() else { return Task::none() };
                let passos = self.fluxos.passos.clone();
                self.fluxos.rodando = true;
                self.fluxos.previa = None;
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        let finais = Finais::de(&passos)?;
                        let (bytes, mut relato) = processar(abrir()?, &passos, &rotulo)?;
                        let paginas = DocumentoPdf::de_bytes(&bytes, None)?.total_paginas();
                        let bytes = finais.aplicar(bytes, &mut relato)?;
                        Ok((format!("{rotulo} → {paginas} página(s), {}", featherpdf_core::tamanho(bytes.len())), relato))
                    }).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Fluxos(Fx::Testado(r)));
            }
            Fx::Testado(r) => { f.rodando = false; f.previa = Some(r); }
            Fx::Rodar => {
                if impedimento(&self.fluxos.passos, self.fluxos.modo).is_some() { return Task::none(); }
                let passos = self.fluxos.passos.clone();
                let modo = self.fluxos.modo;
                let Some(saida) = self.pasta_saida() else {
                    self.fluxos.resultado = Some(Err("Escolha a pasta de saída: o PDF aberto ainda não foi salvo.".into()));
                    return Task::none();
                };
                let trabalho: Vec<(String, Fonte)> = match &self.fluxos.pasta {
                    Some(p) if modo == Modo::Juntar => if self.fluxos.arquivos.is_empty() { vec![] }
                        else { vec![(format!("{}.pdf", nome(p)), Fonte::Juntar(self.fluxos.arquivos.clone()))] },
                    Some(_) => self.fluxos.arquivos.iter().map(|p| (nome(p), Fonte::Arquivo(p.clone()))).collect(),
                    None => match &self.aberto {
                        Some(a) => vec![(self.nome_documento().unwrap_or_else(|| "documento.pdf".into()), Fonte::Pdf(Box::new(a.pdf.clone())))],
                        None => vec![],
                    },
                };
                if trabalho.is_empty() { self.fluxos.resultado = Some(Err("Nenhum PDF ou imagem na entrada.".into())); return Task::none(); }
                self.fluxos.rodando = true;
                self.fluxos.resultado = None;
                return Task::perform(async move {
                    tokio::task::spawn_blocking(move || rodar(trabalho, &passos, modo, &saida)).await.unwrap_or_else(|e| Err(e.to_string()))
                }, |r| Msg::Fluxos(Fx::Feito(r)));
            }
            Fx::Feito(r) => { f.rodando = false; f.resultado = Some(r); }
        }
        Task::none()
    }

    /// O primeiro PDF da entrada, para o "Testar com 1".
    #[allow(clippy::type_complexity)]
    fn primeiro(&self) -> Option<(String, Box<dyn FnOnce() -> Result<DocumentoPdf, String> + Send>)> {
        match &self.fluxos.pasta {
            Some(_) if self.fluxos.modo == Modo::Juntar => {
                let todos = self.fluxos.arquivos.clone();
                if todos.is_empty() { return None; }
                Some((format!("{} arquivo(s) juntados", todos.len()), Box::new(move || crate::juntar::juntar_arquivos(&todos))))
            }
            Some(_) => {
                let p = self.fluxos.arquivos.first()?.clone();
                Some((nome(&p), Box::new(move || crate::juntar::documento_de_arquivo(&p))))
            }
            None => {
                let pdf = self.aberto.as_ref()?.pdf.clone();
                Some((self.nome_documento().unwrap_or_else(|| "documento.pdf".into()), Box::new(move || Ok(pdf))))
            }
        }
    }

    /// A pasta de saída escolhida ou "processados" ao lado da entrada.
    fn pasta_saida(&self) -> Option<PathBuf> {
        if let Some(s) = &self.fluxos.saida { return Some(s.clone()); }
        let base = match &self.fluxos.pasta {
            Some(p) => p.clone(),
            None => self.aberto.as_ref()?.pdf.origem()?.parent()?.to_path_buf(),
        };
        Some(base.join("processados"))
    }

    pub fn view_fluxos(&self, id: WindowId) -> Element<'_, Msg> {
        let f = &self.fluxos;
        let muted = tema::COR_TEXTO_MUTED();
        let negrito = Font::SEMIBOLD;

        // Esquerda: os blocos que dá para usar.
        let titulo_grupo = |t: &'static str| container(text(t.to_uppercase()).size(11).font(negrito).color(muted)).padding([6, 2]);
        let item = |rotulo: String, msg: Msg, verde: bool| -> Element<'_, Msg> {
            button(text(rotulo).size(12.5).width(Length::FILL)).width(Length::FILL).padding([6, 8]).on_press(msg)
                .style(move |_, status| {
                    let hover = matches!(status, Status::Hovered | Status::Pressed);
                    Style::default().background(if hover { tema::COR_HOVER() } else { tema::COR_PAINEL() }).text_color(tema::COR_TEXTO())
                        .border(Border::rounded(6.0).color(if verde { VERDE } else { tema::COR_BORDA_SUAVE() }, 1.0))
                }).into()
        };
        // Bloco da paleta: arrasta para o meio (ou clica, e vai para o fim).
        let pegavel_de = |rotulo: String, pego: Pego, verde: bool| -> Element<'_, Msg> {
            mouse_area(container(text(rotulo).size(12.5).width(Length::FILL)).width(Length::FILL).padding([6, 8])
                .style(move |_| Style::default().background(tema::COR_PAINEL()).border(Border::rounded(6.0).color(if verde { VERDE } else { tema::COR_BORDA_SUAVE() }, 1.0))))
                .on_press(Msg::Fluxos(Fx::Pegar(pego))).cursor_icon(CursorIcon::Grab).into()
        };
        let pegavel = |p: Passo| pegavel_de(p.nome().into(), Pego::Novo(p), false);
        let mut esquerda = column![text("Modelos prontos").size(13).font(negrito)].spacing(4);
        for (i, (nome, _, _)) in modelos().into_iter().enumerate() { esquerda = esquerda.push(item(nome.into(), Msg::Fluxos(Fx::Modelo(i)), false)); }
        esquerda = esquerda.push(Space::with_height(8)).push(text("Blocos").size(13).font(negrito)).push(titulo_grupo("Entrada"))
            .push(pegavel_de("Pasta de arquivos".into(), Pego::Pasta, false))
            .push(pegavel_de("Arquivo aberto".into(), Pego::Aberto, false));
        for (grupo, passos) in paleta() {
            if grupo.is_empty() { continue; }
            esquerda = esquerda.push(titulo_grupo(grupo));
            for p in passos { esquerda = esquerda.push(pegavel(p)); }
        }
        esquerda = esquerda.push(titulo_grupo("Saída")).push(pegavel_de("Salvar PDF em pasta".into(), Pego::Saida, true));
        let esquerda = container(scrollable(esquerda).height(Length::FILL)).width(232).height(Length::FILL).padding(12)
            .style(|_| Style::default().background(tema::COR_PAINEL()));

        // Meio: os cartões ligados em duas colunas, sobre a mesa pontilhada.
        let mut cartoes: Vec<(Sel, String, String, Option<&str>, Color)> = vec![(Sel::Entrada, "Entrada".into(), self.resumo_entrada(), None, Color::rgb8(0xf1, 0xf3, 0xf4))];
        for (i, p) in f.passos.iter().enumerate() { cartoes.push((Sel::Passo(i), p.nome().into(), p.resumo(), aviso_ordem(&f.passos, i), p.cor_cabeca())); }
        cartoes.push((Sel::Saida, "Saída".into(), self.resumo_saida(), None, Color::rgb8(0xe6, 0xf1, 0xea)));
        let linhas = cartoes.len().div_ceil(2) as f32;
        let altura = MARGEM.1 * 2.0 + (linhas - 1.0) * PASSO.1 + CARTAO.1;
        let mut pilha = Stack::new().width(Length::FILL).height(altura)
            .push(canvas::Canvas::new(Ligacoes { n: cartoes.len() }).width(Length::FILL).height(altura));
        let alvo = f.arrasto.as_ref().filter(|a| a.moveu).and_then(|a| a.alvo);
        for (k, (sel, cabeca, corpo, aviso, fundo)) in cartoes.into_iter().enumerate() {
            let (x, y) = posicao(k);
            // Soltar em cima: entra antes deste cartão (na entrada, logo depois dela).
            let mirado = alvo == Some(k);
            let escolhido = f.sel == sel && !mirado;
            let titulo = text(format!("{} · {cabeca}", k + 1)).size(12).font(negrito)
                .color(if escolhido { VERMELHO } else if sel == Sel::Saida { Color::rgb8(0x1e, 0x5c, 0x35) } else { Color::rgb8(0x20, 0x21, 0x24) });
            let titulo: Element<'_, Msg> = match sel {
                Sel::Passo(i) => row![titulo, Space::with_width(Length::FILL),
                    button(text("×").size(14).color(Color::rgb8(0x5f, 0x63, 0x68))).padding([0, 4]).style(tema::estilo_botao_pequeno(false))
                        .on_press(Msg::Fluxos(Fx::Tirar(i)))].align_y(Align::Center).into(),
                _ => titulo.into(),
            };
            let cabeca = container(titulo)
                .width(Length::FILL).padding([6, 10])
                .style(move |_| Style::default().background(if escolhido { Color::rgb8(0xf9, 0xeb, 0xea) } else { fundo }));
            // Com aviso de ordem, a segunda linha do resumo dá lugar a ele, em âmbar.
            let corpo: Element<'_, Msg> = match aviso {
                Some(a) => column![text(corpo.lines().next().unwrap_or_default().to_string()).size(12).color(muted),
                    text(format!("⚠ {a}")).size(12).color(Color::rgb8(0xb0, 0x7d, 0x0b))].into(),
                None => text(corpo).size(12).color(muted).into(),
            };
            let cartao = column![cabeca, container(corpo).padding([6, 10])];
            let borda = if mirado { AZUL } else if escolhido { VERMELHO } else { tema::COR_BORDA() };
            let caixa = container(cartao).width(CARTAO.0).clip(true)
                .style(move |_| Style::default().background(tema::COR_PAINEL()).border(Border::rounded(8.0).color(borda, if mirado || escolhido { 2.0 } else { 1.0 })));
            let pegar = match sel { Sel::Passo(i) => Fx::Pegar(Pego::Cartao(i)), s => Fx::Selecionar(s) };
            let mut cartao = mouse_area(caixa).on_press(Msg::Fluxos(pegar))
                .on_enter(Msg::Fluxos(Fx::Alvo(Some(k)))).on_exit(Msg::Fluxos(Fx::Alvo(None)));
            if let Sel::Passo(i) = sel { cartao = cartao.on_right_press_at(move |p| Msg::Fluxos(Fx::Menu(Some((i, p))))); }
            pilha = pilha.push(container(cartao).padding(Padding { top: y, left: x, ..Padding::ZERO }));
        }
        if f.passos.is_empty() {
            pilha = pilha.push(container(text("Arraste blocos da esquerda para cá — ou escolha um modelo pronto.").size(12.5).color(muted))
                .padding(Padding { top: altura - 22.0, left: MARGEM.0, ..Padding::ZERO }));
        }
        let mesa = mouse_area(scrollable(container(pilha).width(Length::FILL)).height(Length::FILL))
            .on_enter(Msg::Fluxos(Fx::SobreMesa(true))).on_exit(Msg::Fluxos(Fx::SobreMesa(false)));
        let previa = self.caixa_previa();
        let meio = container(column![mesa, previa].spacing(12)).width(Length::FILL).height(Length::FILL).padding(Padding { right: 20.0, bottom: 20.0, left: 20.0, ..Padding::ZERO })
            .style(|_| Style::default().background(tema::COR_MESA()));

        // Direita: o cartão escolhido, e testar ou rodar.
        let direita = container(self.config_escolhido()).width(280).height(Length::FILL).padding(14)
            .style(|_| Style::default().background(tema::COR_PAINEL()));

        let total = match &f.pasta { Some(_) => f.arquivos.len(), None => self.aberto.is_some() as usize };
        let barra = row![
            text("Fluxo em blocos").size(13).font(negrito),
            text(format!("{} bloco(s) · {}", f.passos.len(), if total == 1 { "1 arquivo".to_string() } else { format!("{total} arquivos") })).size(12).color(muted),
            text(match f.arrasto.as_ref().filter(|a| a.moveu) {
                Some(a) => match (&a.pego, a.alvo) {
                    (Pego::Novo(p), Some(_)) => format!("Solte para pôr “{}” no ponto destacado em azul", p.nome()),
                    (Pego::Novo(p), None) => format!("Solte no meio para pôr “{}” no fim", p.nome()),
                    (Pego::Cartao(_), _) => "Solte em cima de outro bloco para mudar a ordem — ou fora do meio para tirar".into(),
                    (Pego::Pasta | Pego::Aberto, _) => "Solte no meio para usar como entrada".into(),
                    (Pego::Saida, _) => "Solte no meio para escolher onde salvar".into(),
                },
                None => String::new(),
            }).size(12).color(AZUL),
        ].spacing(12).align_y(Align::Center);
        let corpo = column![
            container(barra).padding([10, 16]),
            crate::inicio::linha_horizontal(),
            row![esquerda, crate::inicio::linha_vertical(), meio, crate::inicio::linha_vertical(), direita].height(Length::FILL),
        ];
        let janela = cromo::janela_com_barra("Fluxos em blocos", corpo.into(), Self::acoes_janela(id));
        if let Some((i, p)) = f.menu {
            let n = f.passos.len();
            let fixo = f.passos.get(i).is_some_and(|p| p.ordem_final() != 0);
            let opcao = |rotulo: &'static str, msg: Option<Fx>| button(text(rotulo).size(12.5).width(Length::FILL)).width(Length::FILL).padding([6, 12])
                .style(tema::estilo_botao_pequeno(false)).on_press_maybe(msg.map(Msg::Fluxos));
            let menu = container(column![
                opcao("Subir", (i > 0 && !fixo).then_some(Fx::Mover(i, true))),
                opcao("Descer", (i + 1 < n && !fixo).then_some(Fx::Mover(i, false))),
                opcao("Duplicar", Some(Fx::Duplicar(i))),
                opcao("Remover", Some(Fx::Tirar(i))),
            ].spacing(2)).width(160).padding(4).style(|_| Style::default().background(tema::COR_PAINEL())
                .border(Border::rounded(6.0).color(tema::COR_BORDA(), 1.0))
                .shadow(Shadow { color: Color { a: 0.2, ..Color::BLACK }, offset: Point::new(0.0, 4.0), blur: 12.0 }));
            // Clicar fora fecha o menu.
            return Stack::new().push(janela)
                .push(mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::Fluxos(Fx::Menu(None))).on_right_press(Msg::Fluxos(Fx::Menu(None))))
                .push(container(menu).padding(Padding { top: p.y, left: p.x, ..Padding::ZERO })).into();
        }
        // O fantasma do bloco segue o cursor enquanto arrasta (a janela inteira começa em 0,0).
        match f.arrasto.as_ref().filter(|a| a.moveu).and_then(|a| a.pos.map(|p| (p, a.pego.nome(&f.passos)))) {
            Some((p, nome)) => Stack::new().push(janela).push(
                container(container(text(nome).size(12.5).font(negrito)).padding([6, 10]).style(|_| Style::default()
                    .background(Color { a: 0.92, ..tema::COR_PAINEL() }).border(Border::rounded(6.0).color(AZUL, 2.0))
                    .shadow(Shadow { color: Color { a: 0.25, ..Color::BLACK }, offset: Point::new(0.0, 4.0), blur: 12.0 })))
                .padding(Padding { top: p.y + 10.0, left: p.x + 12.0, ..Padding::ZERO })).into(),
            None => janela,
        }
    }

    fn resumo_entrada(&self) -> String {
        match &self.fluxos.pasta {
            Some(p) => format!("{}\nPDFs e fotos · {} arquivo(s)", nome(p), self.fluxos.arquivos.len()),
            None => format!("Arquivo aberto\n{}", self.nome_documento().unwrap_or_else(|| "nenhum".into())),
        }
    }

    fn resumo_saida(&self) -> String {
        let modo = match self.fluxos.modo {
            Modo::Juntar if self.fluxos.pasta.is_some() => "tudo num PDF só",
            Modo::Juntar | Modo::PorArquivo => "um PDF por arquivo",
            Modo::Dividir => "uma página por PDF",
            Modo::Png => "PNG por página",
            Modo::Word => "Word (.docx)",
            Modo::Planilha => "planilha (.xlsx)",
        };
        match (&self.fluxos.saida, self.pasta_saida()) {
            (Some(p), _) => format!("{}\n{modo}", nome(p)),
            (None, Some(p)) => format!("…/{}\n{modo}", nome(&p)),
            (None, None) => "escolha a pasta\no PDF não foi salvo".into(),
        }
    }

    fn caixa_previa(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let primeiro = self.primeiro().map(|(n, _)| n).unwrap_or_default();
        let mut c = column![row![text("Prévia do resultado").size(13).font(Font::SEMIBOLD),
            text(if primeiro.is_empty() { String::new() } else { format!("1º arquivo · {primeiro}") }).size(12).color(muted)].spacing(12)].spacing(6);
        c = match &self.fluxos.previa {
            None if self.fluxos.rodando => c.push(text("Rodando…").size(12).color(muted)),
            None => c.push(text("“Testar com 1” roda o fluxo no primeiro arquivo sem gravar nada.").size(12).color(muted)),
            Some(Err(e)) => c.push(text(e.clone()).size(12).color(tema::COR_ERRO())),
            Some(Ok((resumo, relato))) => {
                let mut c = c.push(text(resumo.clone()).size(12.5));
                for (i, r) in relato.iter().enumerate() { c = c.push(text(format!("{}. {r}", i + 1)).size(12).color(muted)); }
                c
            }
        };
        container(scrollable(c).height(Length::FILL)).width(Length::FILL).height(140).padding([8, 12])
            .style(|_| Style::default().background(tema::COR_PAINEL()).border(Border::rounded(8.0).color(tema::COR_BORDA_SUAVE(), 1.0))).into()
    }

    fn config_escolhido(&self) -> Element<'_, Msg> {
        let f = &self.fluxos;
        let muted = tema::COR_TEXTO_MUTED();
        let titulo = |t: String| text(t).size(15).font(Font::SEMIBOLD);
        let botao = |t: &'static str, msg: Option<Msg>| button(text(t).size(12.5)).padding([5, 10]).style(tema::estilo_botao_pequeno(false)).on_press_maybe(msg);
        let mut c = column![].spacing(10);
        match f.sel {
            Sel::Entrada => {
                c = c.push(titulo("1 · Entrada".into()));
                c = c.push(text(match &f.pasta { Some(p) => p.display().to_string(), None => "O PDF aberto agora.".into() }).size(12.5).color(muted));
                if f.pasta.is_some() { c = c.push(text(format!("{} PDF(s) e foto(s) na pasta", f.arquivos.len())).size(12.5)); }
                c = c.push(row![botao("Escolher pasta…", Some(Msg::Fluxos(Fx::EscolherPasta))),
                    botao("Usar o aberto", f.pasta.is_some().then_some(Msg::Fluxos(Fx::UsarAberto)))].spacing(6));
            }
            Sel::Saida => {
                c = c.push(titulo(format!("{} · Saída", f.passos.len() + 2)));
                c = c.push(text(self.pasta_saida().map_or_else(|| "Escolha onde salvar.".into(), |p| p.display().to_string())).size(12.5).color(muted));
                // Juntar só faz sentido com uma pasta de entrada.
                for m in Modo::TODOS.into_iter().filter(|&m| m != Modo::Juntar || f.pasta.is_some()) {
                    c = c.push(radio(m.nome(), m, Some(f.modo), |v| Msg::Fluxos(Fx::Modo(v))).size(15).text_size(12.5));
                }
                if f.modo == Modo::Juntar && f.pasta.is_some() {
                    c = c.push(text(format!("Fotos entram uma por página. Sai como “{}.pdf”.", f.pasta.as_deref().map(nome).unwrap_or_default())).size(12).color(muted));
                }
                c = c.push(text("Os originais não mudam.").size(12).color(muted));
                c = c.push(botao("Escolher pasta…", Some(Msg::Fluxos(Fx::EscolherSaida))));
            }
            Sel::Passo(i) => if let Some(p) = f.passos.get(i).cloned() {
                let p_movel = p.ordem_final() == 0;
                c = c.push(titulo(format!("{} · {}", i + 2, p.nome())));
                let mudar = move |novo: Passo| Msg::Fluxos(Fx::Mudar(i, novo));
                match p {
                    Passo::Bloco(Bloco::Girar { graus: g, quais }) => {
                        for graus in [90, 180, 270] {
                            c = c.push(radio(format!("{graus}° no sentido horário"), graus, Some(g), move |v| mudar(Passo::Bloco(Bloco::Girar { graus: v, quais }))).size(15).text_size(12.5));
                        }
                        c = c.push(text("Quais páginas").size(12.5).color(muted));
                        for (k, rotulo) in PAGINAS_GIRAR.iter().enumerate() {
                            c = c.push(radio(*rotulo, k as u8, Some(quais), move |v| mudar(Passo::Bloco(Bloco::Girar { graus: g, quais: v }))).size(15).text_size(12.5));
                        }
                        c = c.push(text("Em pé e deitada: como a página aparece na tela.").size(12).color(muted));
                    }
                    Passo::Bloco(Bloco::TarjarPadroes(q)) => {
                        for (k, padrao) in Padrao::TODOS.iter().enumerate() {
                            c = c.push(checkbox(padrao.nome(), q[k]).on_toggle(move |v| { let mut q = q; q[k] = v; mudar(Passo::Bloco(Bloco::TarjarPadroes(q))) }).size(15).text_size(12.5));
                        }
                        c = c.push(text("Apaga o texto e a imagem embaixo, não só cobre.").size(12).color(muted));
                    }
                    Passo::Ocr { so_sem_texto } => {
                        c = c.push(checkbox("Só páginas sem texto", so_sem_texto).on_toggle(move |v| mudar(Passo::Ocr { so_sem_texto: v })).size(15).text_size(12.5));
                        c = c.push(text("Português, no computador. Põe o texto invisível atrás da imagem.").size(12).color(muted));
                    }
                    Passo::Bloco(Bloco::Comprimir(n)) => for (k, (nome, dpi, q)) in NIVEIS_COMPRESSAO.iter().enumerate() {
                        c = c.push(radio(format!("{nome} — fotos a {dpi:.0} dpi, qualidade {q}"), k as u8, Some(n), move |v| mudar(Passo::Bloco(Bloco::Comprimir(v)))).size(15).text_size(12.5));
                    },
                    Passo::Bloco(Bloco::Normalizar(t)) => for (k, (nome, l, a)) in TAMANHOS_PAGINA.iter().enumerate() {
                        c = c.push(radio(format!("{nome} ({:.0} × {:.0} mm)", l / 72.0 * 25.4, a / 72.0 * 25.4), k as u8, Some(t), move |v| mudar(Passo::Bloco(Bloco::Normalizar(v)))).size(15).text_size(12.5));
                    },
                    Passo::Bloco(Bloco::MarcaDagua(t)) => {
                        c = c.push(text_input("Texto da marca d'água", t).on_input(move |v| mudar(Passo::Bloco(Bloco::MarcaDagua(v)))).size(13).padding([5, 8]));
                        c = c.push(text("Cinza, na diagonal, numa camada que o leitor pode esconder. {arquivo} vira o nome do arquivo.").size(12).color(muted));
                    }
                    Passo::Bloco(Bloco::Cabecalho { cabecalho, rodape }) => {
                        let (r, cab) = (rodape.clone(), cabecalho.clone());
                        c = c.push(text_input("Cabeçalho", cabecalho).on_input(move |v| mudar(Passo::Bloco(Bloco::Cabecalho { cabecalho: v, rodape: r.clone() }))).size(13).padding([5, 8]));
                        c = c.push(text_input("Rodapé", rodape).on_input(move |v| mudar(Passo::Bloco(Bloco::Cabecalho { cabecalho: cab.clone(), rodape: v }))).size(13).padding([5, 8]));
                        c = c.push(text("Use {pagina}, {total}, {data} e {arquivo}.").size(12).color(muted));
                    }
                    Passo::Senha(senha) => {
                        c = c.push(text_input("Senha para abrir", senha).secure(true).on_input(move |v| mudar(Passo::Senha(v))).size(13).padding([5, 8]));
                        c = c.push(text("AES-256. A senha fica só na memória: não é gravada em lugar nenhum. Vai sempre no fim do fluxo.").size(12).color(muted));
                    }
                    Passo::Assinar { certificado, senha } => {
                        let cert = certificado.clone();
                        c = c.push(text(certificado.as_deref().map_or_else(|| "Nenhum certificado escolhido.".into(), |p| p.display().to_string())).size(12.5)
                            .color(if certificado.is_some() { tema::COR_TEXTO() } else { tema::COR_ERRO() }));
                        c = c.push(botao("Escolher certificado…", Some(Msg::Fluxos(Fx::EscolherCertificado(i)))));
                        c = c.push(text_input("Senha do certificado", senha).secure(true)
                            .on_input(move |v| mudar(Passo::Assinar { certificado: cert.clone(), senha: v })).size(13).padding([5, 8]));
                        c = c.push(text("Só com certificado A1 (.pfx/.p12): sem ele a assinatura não tem validade. Assina por último, sem desfazer as assinaturas anteriores; a senha fica só na memória.").size(12).color(muted));
                    }
                    p => c = c.push(text(p.resumo().replace('\n', " · ")).size(12.5).color(muted)),
                }
                if let Some(a) = aviso_ordem(&f.passos, i) { c = c.push(text(format!("⚠ {a}")).size(12).color(Color::rgb8(0xb0, 0x7d, 0x0b))); }
                let movel = |j: usize| f.passos.get(j).is_some_and(|x| x.ordem_final() == 0) && p_movel;
                c = c.push(row![botao("↑", (i > 0 && movel(i - 1)).then_some(Msg::Fluxos(Fx::Mover(i, true)))),
                    botao("↓", movel(i + 1).then_some(Msg::Fluxos(Fx::Mover(i, false)))),
                    Space::with_width(Length::FILL), botao("Tirar do fluxo", Some(Msg::Fluxos(Fx::Tirar(i))))].spacing(6));
            },
        }
        match &f.resultado {
            Some(Ok(t)) => c = c.push(text(t.clone()).size(12).color(VERDE)),
            Some(Err(e)) => c = c.push(text(e.clone()).size(12).color(tema::COR_ERRO())),
            None => {}
        }
        let total = match &f.pasta { Some(_) => f.arquivos.len(), None => self.aberto.is_some() as usize };
        let impede = impedimento(&f.passos, f.modo);
        if let Some(e) = &impede { c = c.push(text(e.clone()).size(12).color(tema::COR_ERRO())); }
        let pode = total > 0 && !f.rodando && impede.is_none();
        let rodar = if f.rodando { "Rodando…".to_string() } else if f.pasta.is_some() { format!("Rodar nos {total}") } else { "Rodar".into() };
        c.push(Space::with_height(Length::FILL))
            .push(row![
                button(text("Testar com 1").size(13).width(Length::FILL).align_x(Align::Center)).width(Length::FILL).padding([8, 10])
                    .style(tema::estilo_botao_pequeno(false)).on_press_maybe(pode.then_some(Msg::Fluxos(Fx::Testar))),
                button(text(rodar).size(13).width(Length::FILL).align_x(Align::Center)).width(Length::FILL).padding([8, 10])
                    .style(tema::estilo_botao_primario).on_press_maybe(pode.then_some(Msg::Fluxos(Fx::Rodar))),
            ].spacing(6))
            .into()
    }
}

/// Canto de cima à esquerda do cartão `k`: duas colunas em zigue-zague (a linha ímpar volta da direita).
fn posicao(k: usize) -> (f32, f32) {
    let coluna = if (k / 2) % 2 == 0 { k % 2 } else { 1 - k % 2 };
    (MARGEM.0 + coluna as f32 * PASSO.0, MARGEM.1 + (k / 2) as f32 * PASSO.1)
}

/// Pontos da mesa e as ligações entre um cartão e o seguinte (a última, para a saída, em verde).
struct Ligacoes { n: usize }

impl canvas::Program<Msg> for Ligacoes {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        let ponto = if tema::escuro() { Color::rgb8(0x3d, 0x41, 0x47) } else { Color::rgb8(0xcf, 0xd3, 0xd8) };
        let mut y = 8.0;
        while y < limites.height { let mut x = 8.0; while x < limites.width { f.fill(&Path::circle(Point::new(x, y), 1.0), ponto); x += 16.0; } y += 16.0; }
        let meio = CARTAO.1 / 2.0;
        for k in 0..self.n.saturating_sub(1) {
            let ((x0, y0), (x1, y1)) = (posicao(k), posicao(k + 1));
            // Mesma linha: de lado a lado; linha seguinte: desce na mesma coluna.
            let caminho = if y0 != y1 { Path::line(Point::new(x0 + CARTAO.0 / 2.0, y0 + CARTAO.1), Point::new(x1 + CARTAO.0 / 2.0, y1)) }
                else if x0 < x1 { Path::line(Point::new(x0 + CARTAO.0, y0 + meio), Point::new(x1, y1 + meio)) }
                else { Path::line(Point::new(x0, y0 + meio), Point::new(x1 + CARTAO.0, y1 + meio)) };
            let cor = if k + 2 == self.n { VERDE } else { Color::rgb8(0x9a, 0xa0, 0xa6) };
            f.stroke(&caminho, Stroke::default().with_color(cor).with_width(2.0));
        }
        vec![f.into_geometry()]
    }
}

/// De onde vem o PDF de cada item do lote.
enum Fonte { Arquivo(PathBuf), Pdf(Box<DocumentoPdf>), Juntar(Vec<PathBuf>) }

/// Senha e assinatura, aplicadas em cada PDF que sai, depois de todo o resto.
struct Finais { senha: Option<String>, certificado: Option<featherpdf_core::assinatura::Certificado> }

impl Finais {
    fn de(passos: &[Passo]) -> Result<Self, String> {
        let mut f = Finais { senha: None, certificado: None };
        for p in passos {
            match p {
                Passo::Senha(s) => f.senha = Some(s.clone()),
                Passo::Assinar { certificado: Some(c), senha } => {
                    let bytes = featherpdf_io::ler_bytes(c)?;
                    f.certificado = Some(featherpdf_core::assinatura::ler_certificado(&bytes, senha)?);
                }
                Passo::Assinar { certificado: None, .. } => return Err("Escolha o certificado: sem ele a assinatura não tem validade.".into()),
                _ => {}
            }
        }
        Ok(f)
    }

    fn aplicar(&self, mut bytes: Vec<u8>, relato: &mut Vec<String>) -> Result<Vec<u8>, String> {
        if let Some(s) = &self.senha {
            let p = featherpdf_core::Protecao { senha_abrir: s.clone(), limitar: None, aes256: true };
            bytes = DocumentoPdf::de_bytes(&bytes, None)?.protegido(&p)?;
            relato.push("protegido com senha (AES-256)".into());
        }
        if let Some(c) = &self.certificado {
            bytes = featherpdf_core::assinatura::assinar(&bytes, c, &featherpdf_core::assinatura::OpcoesAssinatura::default())?;
            relato.push(format!("assinado por {}", c.titular()));
        }
        Ok(bytes)
    }
}

/// Roda os passos num PDF: os blocos seguidos vão juntos ao `executar_fluxo`; o OCR desenha
/// cada página e põe o texto lido como camada invisível. Senha e assinatura ficam para [`Finais`].
/// `nome` é o do arquivo, para o {arquivo} da marca d'água e do cabeçalho.
fn processar(mut pdf: DocumentoPdf, passos: &[Passo], nome: &str) -> Result<(Vec<u8>, Vec<String>), String> {
    let base = Caminho::new(nome).file_stem().map_or_else(|| nome.to_string(), |s| s.to_string_lossy().into_owned());
    let mut relato = Vec::new();
    let mut blocos = Vec::new();
    for p in passos {
        match p.clone() {
            Passo::Bloco(Bloco::MarcaDagua(t)) => blocos.push(Bloco::MarcaDagua(t.replace("{arquivo}", &base))),
            Passo::Bloco(Bloco::Cabecalho { cabecalho, rodape }) =>
                blocos.push(Bloco::Cabecalho { cabecalho: cabecalho.replace("{arquivo}", &base), rodape: rodape.replace("{arquivo}", &base) }),
            Passo::Bloco(b) => blocos.push(b),
            Passo::Senha(_) | Passo::Assinar { .. } => {}
            Passo::Ocr { so_sem_texto } => {
                if !blocos.is_empty() {
                    let (bytes, r) = featherpdf_io::executar_fluxo(pdf, &std::mem::take(&mut blocos))?;
                    pdf = DocumentoPdf::de_bytes(&bytes, None)?;
                    relato.extend(r);
                }
                let vis = Visualizador::novo(pdf.serializar()?)?;
                let mut lidas = 0;
                for n in 1..=pdf.total_paginas() {
                    if so_sem_texto && !pdf.texto_pagina(n).unwrap_or_default().trim().is_empty() { continue; }
                    let ([x0, y0, x1, y1], _) = pdf.geometria(n)?;
                    let escala = (crate::ocr::LADO / (x1 - x0).max(y1 - y0)).clamp(1.0, 4.0);
                    let (linhas, _) = crate::ocr::ler(&vis, n, escala)?;
                    let linhas: Vec<(String, [f32; 4])> = linhas.iter().map(|l| (
                        l.palavras.iter().map(|p| p.texto.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" "), l.caixa,
                    )).filter(|(t, _)| !t.is_empty()).collect();
                    if pdf.inserir_camada_ocr(n, &linhas)? > 0 { lidas += 1; }
                }
                relato.push(format!("OCR em {lidas} página(s)"));
            }
        }
    }
    let (bytes, r) = featherpdf_io::executar_fluxo(pdf, &blocos)?;
    relato.extend(r);
    Ok((bytes, relato))
}

/// Grava o resultado de um arquivo no formato da saída; devolve quantos arquivos saíram.
fn gravar_saida(modo: Modo, nome: &str, bytes: Vec<u8>, saida: &Caminho, finais: &Finais) -> Result<usize, String> {
    let base = Caminho::new(nome).file_stem().map_or_else(|| nome.to_string(), |s| s.to_string_lossy().into_owned());
    let mut relato = Vec::new();
    match modo {
        Modo::PorArquivo | Modo::Juntar => {
            featherpdf_io::gravar_pdf(&saida.join(format!("{base}.pdf")), &finais.aplicar(bytes, &mut relato)?)?;
            Ok(1)
        }
        Modo::Dividir => {
            let pdf = DocumentoPdf::de_bytes(&bytes, None)?;
            for n in 1..=pdf.total_paginas() {
                featherpdf_io::gravar_pdf(&saida.join(format!("{base} - página {n}.pdf")), &finais.aplicar(pdf.extrair_para_gravar(&[n])?, &mut relato)?)?;
            }
            Ok(pdf.total_paginas() as usize)
        }
        Modo::Png => {
            let vis = Visualizador::novo(bytes)?;
            let total = vis.tamanhos().len() as u32;
            for n in 1..=total {
                // ~150 dpi: legível na tela sem arquivo enorme.
                let r = vis.desenhar(n, 150.0 / 72.0)?;
                let img = image::RgbaImage::from_raw(r.largura, r.altura, r.rgba).ok_or("Página com tamanho incoerente.")?;
                let mut png = std::io::Cursor::new(Vec::new());
                img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
                featherpdf_io::gravar_atomico(&saida.join(format!("{base} - página {n}.png")), png.get_ref())?;
            }
            Ok(total as usize)
        }
        Modo::Word => {
            let pdf = DocumentoPdf::de_bytes(&bytes, None)?;
            featherpdf_io::gravar_docx(&pdf, &pdf.paginas_em_fluxo()?, &saida.join(format!("{base}.docx")))?;
            Ok(1)
        }
        Modo::Planilha => {
            let pdf = DocumentoPdf::de_bytes(&bytes, None)?;
            let grade = pdf.paginas_em_grade()?;
            if grade.is_empty() { return Err("sem texto para a planilha (é imagem? ponha um bloco de OCR antes)".into()); }
            featherpdf_io::gravar_xlsx(&pdf, &grade, &saida.join(format!("{base}.xlsx")))?;
            Ok(1)
        }
    }
}

fn rodar(trabalho: Vec<(String, Fonte)>, passos: &[Passo], modo: Modo, saida: &Caminho) -> Result<String, String> {
    let finais = Finais::de(passos)?;
    std::fs::create_dir_all(saida).map_err(|e| e.to_string())?;
    let mut arquivos = 0;
    let (mut ok, mut falhas) = (0, Vec::new());
    let total = trabalho.len();
    for (nome, fonte) in trabalho {
        // A foto sai como PDF com o mesmo nome.
        let destino = saida.join(Caminho::new(&nome).with_extension("pdf"));
        let feito = match &fonte { Fonte::Arquivo(p) => Ok(p.clone()), _ => Err(()) };
        // Nunca grava por cima da própria entrada.
        if feito.as_ref().is_ok_and(|p| std::fs::canonicalize(p).ok() == std::fs::canonicalize(&destino).ok()) {
            falhas.push(format!("{nome}: a saída é a mesma pasta da entrada"));
            continue;
        }
        let pdf = match fonte {
            Fonte::Arquivo(p) => crate::juntar::documento_de_arquivo(&p),
            Fonte::Pdf(pdf) => Ok(*pdf),
            Fonte::Juntar(v) => crate::juntar::juntar_arquivos(&v),
        };
        match pdf.and_then(|pdf| processar(pdf, passos, &nome)).and_then(|(bytes, _)| gravar_saida(modo, &nome, bytes, saida, &finais)) {
            Ok(n) => { ok += 1; arquivos += n; }
            Err(e) => falhas.push(format!("{nome}: {e}")),
        }
    }
    let mut r = format!("{ok} de {total} prontos ({arquivos} arquivo(s)) em {}.", saida.display());
    if !falhas.is_empty() { r += &format!(" Falharam: {}", falhas.join("; ")); }
    Ok(r)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn lote_grava_na_saida_e_nao_toca_o_original() {
        let pasta = std::env::temp_dir().join(format!("featherpdf-fluxo-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let mut pdf = featherpdf_io::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "CPF 123.456.789-09", 14.0, false, [0.0; 3]).unwrap();
        let original = pasta.join("a.pdf");
        std::fs::write(&original, pdf.serializar().unwrap()).unwrap();
        let antes = std::fs::read(&original).unwrap();
        let passos = [Passo::Bloco(Bloco::Girar { graus: 90, quais: 0 }), Passo::Bloco(Bloco::TarjarPadroes([true; 6]))];
        let r = rodar(vec![("a.pdf".into(), Fonte::Arquivo(original.clone()))], &passos, Modo::PorArquivo, &pasta.join("processados")).unwrap();
        assert!(r.starts_with("1 de 1"), "{r}");
        assert_eq!(std::fs::read(&original).unwrap(), antes);
        let pronto = featherpdf_io::abrir(&pasta.join("processados/a.pdf")).unwrap();
        assert!(!pronto.texto_pagina(1).unwrap().contains("123"));
        // Saída igual à entrada: recusa em vez de gravar por cima.
        let r = rodar(vec![("a.pdf".into(), Fonte::Arquivo(original.clone()))], &passos, Modo::PorArquivo, &pasta).unwrap();
        assert!(r.starts_with("0 de 1"), "{r}");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// Pasta temporária com um PDF de duas páginas e um CPF.
    fn lote(nome: &str) -> (PathBuf, PathBuf) {
        let pasta = std::env::temp_dir().join(format!("featherpdf-fluxo-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut pdf = featherpdf_io::criar_a4().unwrap();
        pdf.inserir_texto(1, 72.0, 700.0, "Contrato de obra", 14.0, false, [0.0; 3]).unwrap();
        pdf.inserir_em_branco(2).unwrap();
        pdf.inserir_texto(2, 72.0, 700.0, "Pagina dois", 14.0, false, [0.0; 3]).unwrap();
        let original = pasta.join("contrato.pdf");
        std::fs::write(&original, pdf.serializar().unwrap()).unwrap();
        (pasta, original)
    }

    #[test]
    fn senha_exige_senha_para_abrir() {
        let (pasta, original) = lote("senha");
        let passos = [Passo::Senha("s3gredo".into())];
        let r = rodar(vec![("contrato.pdf".into(), Fonte::Arquivo(original))], &passos, Modo::PorArquivo, &pasta.join("ok")).unwrap();
        assert!(r.starts_with("1 de 1"), "{r}");
        let pronto = pasta.join("ok/contrato.pdf");
        assert!(featherpdf_io::abrir(&pronto).is_err(), "abriu sem senha");
        assert!(featherpdf_io::abrir_com_senha(&pronto, Some("s3gredo")).is_ok());
        assert!(!format!("{passos:?}").contains("s3gredo"), "a senha apareceu no Debug");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn assina_com_certificado_por_ultimo() {
        let (pasta, original) = lote("assinar");
        let cert = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../featherpdf-core/testes/certificado-teste.p12");
        let mut passos = vec![Passo::Assinar { certificado: Some(cert), senha: "teste".into() }, Passo::Bloco(Bloco::Comprimir(1))];
        assert!(finais_no_fim(&mut passos), "a assinatura devia ir para o fim");
        assert!(matches!(passos.last(), Some(Passo::Assinar { .. })));
        let r = rodar(vec![("contrato.pdf".into(), Fonte::Arquivo(original))], &passos, Modo::PorArquivo, &pasta.join("ok")).unwrap();
        assert!(r.starts_with("1 de 1"), "{r}");
        let lidas = featherpdf_core::assinatura::verificar_assinaturas(&std::fs::read(pasta.join("ok/contrato.pdf")).unwrap()).unwrap();
        assert!(lidas.len() == 1 && lidas[0].valida() && lidas[0].cobre_tudo, "{lidas:?}");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn sem_certificado_ou_com_senha_junto_nao_roda() {
        assert!(impedimento(&[Passo::Assinar { certificado: None, senha: String::new() }], Modo::PorArquivo).is_some_and(|e| e.contains("certificado")));
        let ambos = [Passo::Senha("x".into()), Passo::Assinar { certificado: Some("c.pfx".into()), senha: "y".into() }];
        assert!(impedimento(&ambos, Modo::PorArquivo).is_some_and(|e| e.contains("juntas")));
        assert!(impedimento(&[Passo::Senha("x".into())], Modo::Png).is_some());
        assert!(impedimento(&[Passo::Bloco(Bloco::Comprimir(1))], Modo::Png).is_none());
    }

    #[test]
    fn avisa_comprimir_antes_do_ocr_e_linearizar_no_meio() {
        let passos = [Passo::Bloco(Bloco::Comprimir(1)), Passo::Ocr { so_sem_texto: true }, Passo::Bloco(Bloco::Linearizar), Passo::Bloco(Bloco::Numerar)];
        assert!(aviso_ordem(&passos, 0).is_some());
        assert!(aviso_ordem(&passos, 1).is_none());
        assert!(aviso_ordem(&passos, 2).is_some());
    }

    #[test]
    fn divide_e_exporta_png() {
        let (pasta, original) = lote("dividir");
        let um = || vec![("contrato.pdf".to_string(), Fonte::Arquivo(original.clone()))];
        let r = rodar(um(), &[], Modo::Dividir, &pasta.join("partes")).unwrap();
        assert!(r.contains("(2 arquivo(s))"), "{r}");
        assert!(pasta.join("partes/contrato - página 1.pdf").exists() && pasta.join("partes/contrato - página 2.pdf").exists());
        let r = rodar(um(), &[], Modo::Png, &pasta.join("png")).unwrap();
        assert!(r.contains("(2 arquivo(s))"), "{r}");
        let png = std::fs::read(pasta.join("png/contrato - página 2.png")).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
