//! FeatherPDF no pelicano. A manipulação do PDF pertence ao core, o desenho das páginas ao
//! featherpdf-io (Hayro) e tema, marca e diálogos ao feather-ui. A janela (barra com as abas
//! de documento, faixa, painel de ferramentas, mesa, trilho e rodapé) fica em `tela.rs`; cada
//! ferramenta mora no seu módulo, com o estado, as mensagens e as camadas dela.

mod ajuda_pdf;
mod assinar;
mod busca;
mod camadas;
mod comentar;
mod configuracoes;
mod comparar;
mod compat;
mod contexto;
mod copias;
mod desbloquear;
mod edicao;
mod equacao;
mod exportar;
mod fluxos;
mod form_dados;
mod formularios;
mod imagem;
mod imprimir;
mod inicio;
mod inserir;
mod inspetor;
mod juntar;
mod marcas;
mod medir;
mod monitor;
mod navegar;
mod ocr;
mod ocr_modelos;
mod organizar;
mod preflight;
mod proteger;
mod reconhecer;
mod recentes;
mod regiao;
mod regras_campos;
mod revisao;
mod rubrica;
mod sanitizar;
mod scripts;
mod selecao;
mod tabelas_vivas;
mod tarja;
mod tela;
mod versoes;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use feather_ui::{atalhos, cromo, janelas::Janelas, marca, tema};
use featherpdf_core::DocumentoPdf;
use featherpdf_io::Visualizador;
use pelicano::*;

pub const VERSAO: &str = env!("CARGO_PKG_VERSION");
/// Índice de "Vermelho" em `tema::ACENTOS`: a cor do FeatherPDF.
pub(crate) const ACENTO_PDF: usize = 4;
pub const ZOOM_MIN: f32 = 0.25;
pub const ZOOM_MAX: f32 = 4.0;
/// Espaço entre páginas e margem da mesa, em pixels lógicos.
pub const VAO_PAGINAS: f32 = 16.0;
/// Passos de Ctrl+Z guardados.
const LIMITE_DESFAZER: usize = 20;
pub const ID_MESA: &str = "featherpdf.mesa";

pub fn id_mesa() -> &'static str { ID_MESA }

/// Desenho de uma página inteira, ou só do trecho à vista quando a página
/// inteira passaria de [`LIMITE_INTEIRA`] pixels (zoom alto numa planta A1).
pub struct Desenho {
    pub imagem: Image,
    /// Os mesmos pixels em RGBA, para a edição pegar a cor da letra e do papel.
    pub pixels: std::sync::Arc<Vec<u8>>,
    pub largura: u32,
    pub altura: u32,
    pub geracao: u64,
    /// Zoom × escala da tela em que foi desenhado.
    pub escala: f32,
    /// Trecho `[x, y, largura, altura]` em pixels da página nessa escala; `None` = inteira.
    pub trecho: Option<[u32; 4]>,
}

const LIMITE_INTEIRA: f32 = 12_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auxiliar { Ajuda, Monitor, Configuracoes, Converter, Ocr, Imprimir, Fluxos }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aba { Inicio, Paginas, Editar, Comentar, Formularios, Medir, Proteger, Converter, Exibir }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Painel { Comentarios, Miniaturas, Marcadores, Camadas, Anexos, Texto, Medidas, Tarja, Assinar, Assinaturas, Paginas, Marcas, Sanitizar, Preflight, Imagem, RegrasCampos, Versoes, Comparar, Regiao, Reconhecer, Scripts, Equacao, Tabelas }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ferramenta { Selecionar, Mao, Texto, Editar, Tarja, Comentar(comentar::Anotador), Campo(formularios::Novo), Inserir(inserir::Insercao), Medir(medir::Medidor), Regiao(regiao::Uso) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposicao { UmaPagina, DuasPaginas }

/// Menu suspenso da faixa (aberto embaixo do botão, na largura do texto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suspenso { Girar, Excluir, Inserir, Extrair, Modelos }

#[derive(Debug, Clone)]
pub enum Msg {
    // Arquivo
    NovoA4,
    Abrir,
    AbrirCaminho(PathBuf),
    CaminhoEscolhido(Option<Vec<PathBuf>>),
    Aberto(Result<(DocumentoPdf, Visualizador), String>),
    Salvar,
    SalvarComo,
    ParaFeatherDocs,
    Medir(medir::Med),
    Prot(proteger::Pr),
    Assin(assinar::As),
    Rub(rubrica::Rb),
    Nav(navegar::Nv),
    Exp(exportar::Ex),
    Tarja(tarja::Tj),
    Com(comentar::Cm),
    Form(formularios::Fm),
    Marca(marcas::Mc),
    Rev(revisao::Rv),
    Juntar(juntar::Jt),
    Sanitizar(sanitizar::Sz),
    Imagem(imagem::Im),
    Cam(camadas::Ca),
    FormDados(form_dados::Fd),
    Ocr(ocr::Oc),
    /// Destino do "Salvar como", com a aba que pediu.
    /// Id do documento (`Aberto::id`) que abriu o diálogo e o destino escolhido.
    DestinoEscolhido(u64, Option<PathBuf>),
    /// Fecha o documento da frente (Ctrl+W).
    IrInicio,
    BuscaRecentes(String),
    // Abas de documento
    Guia(usize),
    FecharGuia(usize),
    ProximaGuia,
    // Páginas e zoom
    Desenhada { geracao: u64, pagina: u32, escala: f32, trecho: Option<[u32; 4]>, resultado: Result<featherpdf_io::PaginaRaster, String> },
    Rolou(Viewport),
    IrPagina(u32),
    Anterior,
    Proxima,
    ZoomMais,
    ZoomMenos,
    Zoom(f32),
    AjustarLargura,
    AjustarPagina,
    /// Página digitada no rodapé (texto enquanto edita; Enter vai).
    CampoPagina(String),
    ConfirmarPagina,
    Disposicao(Disposicao),
    Pag(organizar::Pag),
    Obj(edicao::Obj),
    Ctx(contexto::Ct),
    Senha(desbloquear::Sb),
    /// Comentários, carimbos e campos viram desenho da página.
    Achatar,
    Preflight(preflight::Pf),
    Copia(copias::Cp),
    ModoEscuro,
    LimparCaderno,
    EmbutirFontes,
    Regras(regras_campos::Rg),
    Versoes(versoes::Vs),
    Reconhecer(reconhecer::Rc),
    Fluxos(fluxos::Fx),
    Scripts(scripts::Sc),
    Equacao(equacao::Eq),
    Tabelas(tabelas_vivas::Tv),
    Comparar(comparar::Cmp),
    Regiao(regiao::Rr),
    Selecao(selecao::Sel),
    Imp(imprimir::Im),
    Ins(inserir::Ins),
    Desfazer,
    Refazer,
    Modificadores(Modifiers),
    /// Roda do mouse com Ctrl: zoom (passos, positivo aproxima).
    Roda(f32),
    /// Arquivo solto na janela: abre, ou junta na aba Páginas.
    Soltou(PathBuf),
    GirarEsquerda,
    GirarDireita,
    /// Gira a página atual (ou as escolhidas) por estes graus.
    Girar(i64),
    Excluir,
    /// Exclui todas as páginas menos a atual.
    ExcluirOutras,
    Busca(busca::Busca),
    // Interface
    MudarAba(Aba),
    AlternarFaixa,
    /// Modo leitura: só a barra de título e as páginas.
    Leitura,
    AlternarFerramentas,
    FiltroFerramentas(String),
    BuscaComandos(String),
    /// Ctrl+K: foco na pesquisa de comandos.
    Pesquisar,
    AlternarPainel(Painel),
    /// Arrasto da borda do painel lateral: `Some(x)` do cursor, `None` soltou.
    LarguraPainel(Option<f32>),
    /// Copia o texto da página atual (painel "Texto da página").
    CopiarTexto,
    Ferramenta(Ferramenta),
    /// Menu Arquivo (botão Menu).
    Menu,
    /// Abre (ou fecha, se já aberto) o menu suspenso, embaixo do ponteiro.
    Suspenso(Option<Suspenso>),
    /// Item do menu suspenso: fecha o menu e faz a ação.
    DoSuspenso(Box<Msg>),
    /// Um resultado da pesquisa de comandos: limpa o campo e executa.
    Comando(Box<Msg>),
    Escape,
    Status(String),
    /// Arrasto da ferramenta Mão na mesa.
    MaoApertou,
    MaoMoveu(Point),
    MaoSoltou,
    AbrirAjuda,
    AbrirMonitor,
    AbrirConfiguracoes,
    AlternarTema,
    SecaoConfig(usize),
    Config(configuracoes::Cf),
    AjudaEvento(feather_ui::ajuda::Evento),
    // Janelas
    EscalaTela(f32),
    Arrastar(WindowId),
    Puxar(ResizeDirection),
    Minimizar(WindowId),
    Maximizar(WindowId),
    TelaCheia,
    FecharJanela(WindowId),
    Teclado(WindowId, Key, Modifiers),
    MonitorTick,
    MonitorEvento(feather_ui::monitor::Evento),
    Insp(inspetor::Ins),
    /// O foco pedido por `focar`, já com o campo na tela.
    Focar(String),
}

/// Documento aberto: a estrutura editável (lopdf) e a vista desenhável (Hayro).
pub struct Aberto {
    /// Único por documento aberto: a resposta de um diálogo só vale para o documento que o abriu.
    pub id: u64,
    pub pdf: DocumentoPdf,
    pub vis: Visualizador,
    /// Tamanho de cada página em pontos, com o giro aplicado.
    pub tamanhos: Vec<(f32, f32)>,
}

/// Documento de uma aba que não está na frente: volta para a tela como estava.
pub struct Guia {
    aberto: Aberto,
    historico: Vec<DocumentoPdf>,
    refeitos: Vec<DocumentoPdf>,
    zoom: f32,
    disposicao: Disposicao,
    pagina: u32,
    rolagem: Point,
}

pub struct App {
    /// Objetos lidos de cada página, para a ferramenta Editar texto.
    pub objetos: HashMap<u32, edicao::ObjetosDaPagina>,
    pub escolhidos: Option<(u32, std::collections::BTreeSet<usize>)>,
    pub edicao: Option<edicao::Edicao>,
    /// A última chamada de `editar` deu certo (a edição de texto reabre a caixa se não).
    pub ultima_edicao_ok: bool,
    pub org: organizar::Organizador,
    pub modificadores: Modifiers,
    /// Estado anterior a cada mudança de estrutura (Ctrl+Z) e o que foi desfeito (Ctrl+Y).
    historico: Vec<DocumentoPdf>,
    refeitos: Vec<DocumentoPdf>,
    /// O documento da frente. Sempre há um: fechar o último deixa um A4 em branco.
    pub aberto: Option<Aberto>,
    /// As abas de documento; `None` no lugar da que está na frente (ela vive nos campos).
    pub guias: Vec<Option<Guia>>,
    pub ativa: usize,
    /// Aba com alterações à espera do segundo clique no ×; `usize::MAX`: fechar a janela.
    fechar_pendente: Option<usize>,
    pub pagina: u32,
    pub zoom: f32,
    pub disposicao: Disposicao,
    /// Páginas desenhadas. Um desenho de outra `geracao` (zoom ou conteúdo
    /// antigos) fica na tela, esticado, até o novo chegar: sem isso o zoom piscava.
    pub desenhadas: HashMap<u32, Desenho>,
    pedidas: HashSet<u32>,
    geracao: u64,
    pub escala_tela: f32,
    pub viewport: Option<(f32, f32, f32)>, // deslocamento y, largura e altura visíveis
    pub rolagem_x: f32,
    pub status: String,
    pub aba: Aba,
    pub faixa_aberta: bool,
    pub leitura: bool,
    pub ferramentas_abertas: bool,
    pub painel: Option<Painel>,
    pub ferramenta: Ferramenta,
    pub filtro_ferramentas: String,
    pub busca_comandos: String,
    pub foco_pesquisa: u64,
    pub busca_recentes: String,
    pub busca: busca::EstadoBusca,
    pub ins: inserir::Insercoes,
    pub medidas: medir::Medidas,
    /// Janela Comprimir ou Senha aberta, e os bytes esperando o destino.
    pub protecao: Option<proteger::Janela>,
    /// Painel Assinar (o formulário) e o resultado do Validar.
    pub assinatura: Option<assinar::Assinar>,
    pub rubrica: rubrica::Rubrica,
    pub contexto: Option<(contexto::Alvo, (f32, f32))>,
    /// Submenu "Página N ›" aberto no menu da seleção.
    pub contexto_sub: bool,
    pedido_senha: Option<desbloquear::PedidoSenha>,
    preflight: preflight::Preflight,
    regras: regras_campos::RegrasUi,
    versoes: versoes::Versoes,
    reconhecer: reconhecer::Conferencia,
    fluxos: fluxos::Fluxos,
    scripts: scripts::Scripts,
    equacao: equacao::Equacao,
    tabelas_vivas: tabelas_vivas::TabelasVivas,
    comparar: comparar::Comparacao,
    regiao: regiao::Regiao,
    pub largura_painel: f32,
    arrastando_painel: bool,
    pub impressao: imprimir::Impressao,
    pub campo_pagina: Option<String>,
    pub selecao_texto: Option<(u32, selecao::Posicao, selecao::Posicao)>,
    pub navegacao: navegar::Navegacao,
    pub exportar: exportar::Exportar,
    pub validacao: Option<Result<Vec<featherpdf_core::assinatura::AssinaturaLida>, String>>,
    pub saida_pendente: Option<Vec<u8>>,
    pub tarjas: tarja::Tarjas,
    pub comentarios: comentar::Comentarios,
    pub form: formularios::Formulario,
    pub marcas: marcas::Marcas,
    pub revisao: revisao::Revisao,
    pub sanitizar: sanitizar::Sanitizar,
    pub imagem: imagem::EditarImagem,
    pub camadas: camadas::EstadoCamadas,
    pub ocr: ocr::EstadoOcr,
    /// Texto extraído da página mostrada no painel "Texto da página" (página, texto).
    pub texto_da_pagina: (u32, String),
    pub recentes: recentes::Recentes,
    pub tema_escuro: bool,
    pub config: configuracoes::Config,
    /// Página escura (modo escuro inteligente), separado do tema da janela.
    pub modo_escuro: bool,
    config_secao: usize,
    janelas: Janelas<Auxiliar>,
    ajuda: feather_ui::ajuda::Estado,
    inicio_render: HashMap<u32, Instant>,
    monitor: monitor::EstadoMonitor,
    /// Menu Arquivo aberto.
    pub menu: bool,
    /// Menu suspenso aberto e o canto de cima à esquerda dele.
    pub suspenso: Option<(Suspenso, Point)>,
    /// Arrasto da Mão: onde começou (tela) e a rolagem de então.
    mao: Option<(Option<Point>, Point)>,
}

/// O foco num campo que acaba de aparecer: o `Task::focus` do pelicano só acha campo já desenhado.
/// shortcut: espera 30 ms em vez do próximo quadro; trocar se o pelicano ganhar um foco adiado.
pub fn focar(id: impl Into<String>) -> Task<Msg> {
    let id = id.into();
    Task::perform(async { tokio::time::sleep(Duration::from_millis(30)).await }, move |_| Msg::Focar(id))
}

fn novo_id() -> u64 {
    static PROXIMO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    PROXIMO.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn documento(pdf: DocumentoPdf) -> Result<Aberto, String> {
    let vis = Visualizador::novo(pdf.serializar()?)?;
    Ok(Aberto { id: novo_id(), tamanhos: vis.tamanhos(), pdf, vis })
}

fn nome_do(pdf: &DocumentoPdf) -> String {
    pdf.origem().and_then(|p| p.file_name()).map_or("Documento sem título.pdf".into(), |n| n.to_string_lossy().into_owned())
}

impl App {
    fn novo() -> (Self, Task<Msg>) {
        let config = configuracoes::Config::carregar();
        let escuro = config.escuro();
        tema::definir_tema(escuro, ACENTO_PDF);
        marca::definir_marca(marca::Produto::FeatherPDF, false);
        let mut app = Self {
            objetos: HashMap::new(),
            escolhidos: None,
            edicao: None,
            ultima_edicao_ok: true,
            org: organizar::Organizador::default(),
            modificadores: Modifiers::default(),
            historico: Vec::new(),
            refeitos: Vec::new(),
            aberto: None,
            guias: vec![None],
            ativa: 0,
            fechar_pendente: None,
            pagina: 1,
            zoom: 1.0,
            disposicao: Disposicao::UmaPagina,
            desenhadas: HashMap::new(),
            pedidas: HashSet::new(),
            geracao: 0,
            escala_tela: 1.0,
            viewport: None,
            rolagem_x: 0.0,
            status: String::new(),
            aba: Aba::Inicio,
            faixa_aberta: config.faixa,
            leitura: false,
            ferramentas_abertas: config.ferramentas,
            painel: None,
            ferramenta: Ferramenta::Selecionar,
            filtro_ferramentas: String::new(),
            busca_comandos: String::new(),
            foco_pesquisa: 0,
            busca_recentes: String::new(),
            busca: busca::EstadoBusca::default(),
            ins: inserir::Insercoes::default(),
            medidas: medir::Medidas::default(),
            protecao: None,
            assinatura: None,
            rubrica: Default::default(),
            contexto: None,
            contexto_sub: false,
            pedido_senha: None,
            preflight: Default::default(),
            regras: Default::default(),
            versoes: Default::default(),
            reconhecer: Default::default(),
            fluxos: Default::default(),
            scripts: Default::default(),
            equacao: Default::default(),
            tabelas_vivas: Default::default(),
            comparar: Default::default(),
            regiao: Default::default(),
            largura_painel: 300.0,
            arrastando_painel: false,
            selecao_texto: None,
            impressao: Default::default(),
            campo_pagina: None,
            navegacao: navegar::Navegacao::default(),
            exportar: exportar::Exportar::default(),
            validacao: None,
            saida_pendente: None,
            tarjas: tarja::Tarjas::default(),
            comentarios: comentar::Comentarios::default(),
            form: formularios::Formulario::default(),
            marcas: Default::default(),
            revisao: Default::default(),
            sanitizar: Default::default(),
            imagem: Default::default(),
            camadas: Default::default(),
            ocr: ocr::EstadoOcr::default(),
            texto_da_pagina: (0, String::new()),
            recentes: recentes::Recentes::carregar(),
            tema_escuro: escuro,
            modo_escuro: false,
            config_secao: 0,
            janelas: Janelas::default(),
            ajuda: feather_ui::ajuda::Estado::default(),
            inicio_render: HashMap::new(),
            monitor: monitor::EstadoMonitor::default(),
            menu: false,
            suspenso: None,
            mao: None,
            config,
        };
        app.ocr.lingua = app.config.lingua_ocr;
        app.janelas.principal = Some(WindowId::MAIN);
        let mut tarefas = vec![match featherpdf_io::criar_a4().and_then(|pdf| Ok((Visualizador::novo(pdf.serializar()?)?, pdf))) {
            Ok((vis, pdf)) => app.entrar(pdf, vis),
            Err(e) => { app.status = e; Task::none() }
        }];
        // Arquivo pela linha de comando (abrir com…).
        if let Some(caminho) = std::env::args_os().nth(1).map(PathBuf::from) {
            tarefas.push(Task::done(Msg::AbrirCaminho(caminho)));
        }
        (app, Task::batch(tarefas))
    }

    pub fn geracao_atual(&self) -> u64 { self.geracao }
    pub fn pode_desfazer(&self) -> bool { !self.historico.is_empty() }
    pub fn pode_refazer(&self) -> bool { !self.refeitos.is_empty() }

    /// Páginas do PDF em texto corrido, gravadas num `.docx` temporário e abertas no FeatherDocs.
    pub(crate) fn para_featherdocs(&self) -> Result<String, String> {
        let a = self.aberto.as_ref().ok_or_else(String::new)?;
        let fluxo = a.pdf.paginas_em_fluxo()?;
        if fluxo.iter().all(|p| p.blocos.is_empty()) { return Err("Este PDF não tem texto nem imagem para levar ao documento.".into()); }
        let nome = self.nome_documento().unwrap_or_default();
        let base = std::path::Path::new(&nome).file_stem().map_or("PDF".into(), |n| n.to_string_lossy().into_owned());
        let pasta = std::env::temp_dir().join("featherpdf");
        let destino = pasta.join(format!("{base}.docx"));
        std::fs::create_dir_all(&pasta).map_err(|e| format!("Não consegui gravar o documento: {e}"))?;
        featherpdf_io::gravar_docx(&a.pdf, &fluxo, &destino)?;
        abrir_no("featherdocs", "FeatherDocs", &destino).map(|_| format!("{} página(s) abertas no FeatherDocs.", fluxo.len()))
    }

    pub fn nome_documento(&self) -> Option<String> {
        Some(nome_do(&self.aberto.as_ref()?.pdf))
    }

    pub fn total_paginas(&self) -> u32 {
        self.aberto.as_ref().map_or(0, |a| a.tamanhos.len() as u32)
    }

    // ---------- Abas de documento ----------

    pub fn nome_da_aba(&self, i: usize) -> String {
        if i == self.ativa { self.nome_documento().unwrap_or_default() } else { self.guias[i].as_ref().map_or_else(String::new, |g| nome_do(&g.aberto.pdf)) }
    }

    fn modificada(&self, i: usize) -> bool {
        if i == self.ativa { self.aberto.as_ref().is_some_and(|a| a.pdf.modificado()) }
        else { self.guias[i].as_ref().is_some_and(|g| g.aberto.pdf.modificado()) }
    }

    /// O documento da frente vai para a gaveta com zoom, rolagem e Ctrl+Z dele.
    fn guardar_guia(&mut self) -> Option<Guia> {
        Some(Guia {
            aberto: self.aberto.take()?,
            historico: std::mem::take(&mut self.historico),
            refeitos: std::mem::take(&mut self.refeitos),
            zoom: self.zoom,
            disposicao: self.disposicao,
            pagina: self.pagina,
            rolagem: Point::new(self.rolagem_x, self.viewport.map_or(0.0, |v| v.0)),
        })
    }

    /// Põe a aba `i` na frente.
    fn ir_para_aba(&mut self, i: usize) -> Task<Msg> {
        if i == self.ativa || i >= self.guias.len() { return Task::none(); }
        let Some(g) = self.guias[i].take() else { return Task::none() };
        self.guias[self.ativa] = self.guardar_guia();
        self.ativa = i;
        self.fechar_pendente = None;
        self.trazer(g)
    }

    /// Documento da gaveta para a frente: a tela volta como estava (as ferramentas recomeçam).
    fn trazer(&mut self, g: Guia) -> Task<Msg> {
        let (pdf, vis, id) = (g.aberto.pdf, g.aberto.vis, g.aberto.id);
        let tarefa = self.entrar_sem_registro(pdf, vis);
        // É o mesmo documento de antes: um diálogo aberto para ele continua valendo.
        if let Some(a) = &mut self.aberto { a.id = id; }
        self.historico = g.historico;
        self.refeitos = g.refeitos;
        self.zoom = g.zoom;
        self.disposicao = g.disposicao;
        self.pagina = g.pagina;
        Task::batch([tarefa, Task::scroll_to(ID_MESA, g.rolagem)])
    }

    /// Fecha a aba `i`; com alterações não salvas, só no segundo clique.
    fn fechar_aba(&mut self, i: usize) -> Task<Msg> {
        if i >= self.guias.len() { return Task::none(); }
        if self.modificada(i) && self.fechar_pendente != Some(i) {
            self.fechar_pendente = Some(i);
            self.status = format!("“{}” tem alterações não salvas. Clique de novo no × para fechar sem salvar.", self.nome_da_aba(i));
            return Task::none();
        }
        self.fechar_pendente = None;
        if self.guias.len() == 1 {
            return match featherpdf_io::criar_a4().and_then(documento) {
                Ok(a) => self.entrar_sem_registro(a.pdf, a.vis),
                Err(e) => { self.status = e; Task::none() }
            };
        }
        let mut tarefa = Task::none();
        if i == self.ativa { tarefa = self.ir_para_aba(if i + 1 < self.guias.len() { i + 1 } else { i - 1 }); }
        self.guias.remove(i);
        if i < self.ativa { self.ativa -= 1; }
        tarefa
    }

    // ---------- Desenho das páginas ----------

    /// Joga fora o que foi desenhado (zoom, giro ou documento mudaram).
    fn invalidar(&mut self) -> Task<Msg> {
        self.geracao = self.geracao.wrapping_add(1);
        // Fica só o desenho em volta da página atual, para cobrir a tela enquanto o novo vem.
        let atual = self.pagina;
        self.desenhadas.retain(|&p, _| p.abs_diff(atual) <= 3);
        self.pedidas.clear();
        self.org.limpar_miniaturas();
        Task::batch([self.pedir_visiveis(), self.pedir_miniaturas()])
    }

    /// Guarda o estado anterior a uma mudança feita aos poucos (digitar num campo).
    fn guardar_desfazer(&mut self, antes: DocumentoPdf) {
        self.historico.push(antes);
        if self.historico.len() > LIMITE_DESFAZER { self.historico.remove(0); }
        self.refeitos.clear();
        self.scripts.rodada += 1;
        // Alteração nova: o "clique de novo para fechar sem salvar" volta a avisar.
        self.fechar_pendente = None;
    }

    /// Mudança de estrutura com um passo de Ctrl+Z; refaz a vista se deu certo.
    /// shortcut: guarda o documento inteiro por passo (20 cópias de um PDF de 200 MB
    /// pesam); guardar só os objetos mudados se o Monitor mostrar o pico.
    fn editar(&mut self, f: impl FnOnce(&mut DocumentoPdf) -> Result<String, String>) -> Task<Msg> {
        let Some(a) = &mut self.aberto else { return Task::none() };
        let antes = a.pdf.clone();
        match f(&mut a.pdf) {
            Ok(aviso) => {
                self.ultima_edicao_ok = true;
                self.objetos.clear();
                self.selecao_texto = None;
                self.form.campos = None;
                self.guardar_desfazer(antes);
                // O OCR guarda números de página que a edição pode ter mudado.
                self.ocr.cancelar();
                self.status = aviso;
                self.texto_da_pagina.0 = 0;
                self.refazer_vista()
            }
            Err(e) => {
                a.pdf = antes;
                self.ultima_edicao_ok = false;
                self.status = e;
                Task::none()
            }
        }
    }

    fn voltar(&mut self, desfazer: bool) -> Task<Msg> {
        let Some(a) = &mut self.aberto else { return Task::none() };
        let (de, para) = if desfazer { (&mut self.historico, &mut self.refeitos) } else { (&mut self.refeitos, &mut self.historico) };
        let Some(pdf) = de.pop() else {
            self.status = if desfazer { "Nada para desfazer.".into() } else { "Nada para refazer.".into() };
            return Task::none();
        };
        para.push(std::mem::replace(&mut a.pdf, pdf));
        self.scripts.rodada += 1;
        // O passo guardado traz o `modificado` de quando foi guardado (talvez antes de salvar).
        a.pdf.marcar_modificado();
        self.ocr.cancelar();
        self.objetos.clear();
        self.selecao_texto = None;
        self.form.campos = None;
        self.edicao = None;
        self.status = if desfazer { "Desfeito.".into() } else { "Refeito.".into() };
        self.texto_da_pagina.0 = 0;
        let total = a.pdf.total_paginas();
        self.org.selecao.retain(|&p| p <= total);
        self.refazer_vista()
    }

    /// Altura de cada "linha" da mesa (uma página, ou duas lado a lado).
    pub fn linhas(&self) -> Vec<(Vec<u32>, f32)> {
        let Some(aberto) = &self.aberto else { return Vec::new() };
        let por_linha = if self.disposicao == Disposicao::DuasPaginas { 2 } else { 1 };
        aberto.tamanhos.chunks(por_linha).enumerate().map(|(i, grupo)| {
            let paginas = (0..grupo.len()).map(|j| (i * por_linha + j + 1) as u32).collect();
            let altura = grupo.iter().map(|t| (t.1 * self.zoom).round()).fold(0.0, f32::max);
            (paginas, altura)
        }).collect()
    }

    /// Retângulo de cada página no conteúdo da mesa (o mesmo arranjo de `tela::paginas`).
    pub fn posicoes(&self) -> Vec<(u32, Rect)> {
        let Some(aberto) = &self.aberto else { return Vec::new() };
        let tamanho = |n: u32| {
            let (l, a) = aberto.tamanhos[n as usize - 1];
            ((l * self.zoom).round(), (a * self.zoom).round())
        };
        let largura = |ns: &[u32]| ns.iter().map(|&n| tamanho(n).0).sum::<f32>() + VAO_PAGINAS * (ns.len() as f32 - 1.0);
        let linhas = self.linhas();
        let mesa = (linhas.iter().map(|(ns, _)| largura(ns)).fold(0.0, f32::max) + 2.0 * VAO_PAGINAS)
            .max(self.viewport.map_or(0.0, |v| v.1));
        let (mut y, mut lista) = (VAO_PAGINAS, Vec::new());
        for (ns, h) in &linhas {
            let mut x = VAO_PAGINAS + (mesa - 2.0 * VAO_PAGINAS - largura(ns)) / 2.0;
            for &n in ns {
                let (l, a) = tamanho(n);
                lista.push((n, Rect { x, y, width: l, height: a }));
                x += l + VAO_PAGINAS;
            }
            y += h + VAO_PAGINAS;
        }
        lista
    }

    /// Pede as páginas na tela e uma de cada lado; as outras ficam como papel em branco.
    /// Página grande demais para um desenho só (zoom alto) é pedida só no trecho à vista.
    fn pedir_visiveis(&mut self) -> Task<Msg> {
        let (topo, largura, altura) = self.viewport.unwrap_or((0.0, 0.0, 900.0));
        let visiveis: Vec<(u32, Rect)> = self.posicoes().into_iter()
            .filter(|(_, r)| r.y + r.height >= topo - r.height && r.y <= topo + altura + r.height).collect();
        self.ler_objetos(&visiveis.iter().map(|v| v.0).collect::<Vec<_>>());
        let Some(aberto) = &self.aberto else { return Task::none() };
        let escala = (self.zoom * self.escala_tela).clamp(0.1, 8.0);
        let tela = Rect { x: self.rolagem_x, y: topo, width: largura.max(1.0), height: altura };
        // Folga de meia tela em volta, para rolar um pouco sem esperar desenho novo.
        let folga = Rect { x: tela.x - tela.width / 2.0, y: tela.y - tela.height / 2.0, width: tela.width * 2.0, height: tela.height * 2.0 };
        let mut tarefas = Vec::new();
        for (pagina, r) in visiveis {
            let (pl, pa) = aberto.tamanhos[pagina as usize - 1];
            let (pl, pa) = (pl * escala, pa * escala);
            // Pedaço da página, em pixels do desenho.
            let em_pixels = |v: Rect| -> [u32; 4] {
                let f = escala / self.zoom;
                let x0 = ((v.x - r.x) * f).floor().clamp(0.0, pl) as u32;
                let y0 = ((v.y - r.y) * f).floor().clamp(0.0, pa) as u32;
                let x1 = ((v.x + v.width - r.x) * f).ceil().clamp(0.0, pl) as u32;
                let y1 = ((v.y + v.height - r.y) * f).ceil().clamp(0.0, pa) as u32;
                [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
            };
            let trecho = if pl * pa <= LIMITE_INTEIRA { None } else {
                let Some(pedido) = interseccao(r, folga) else { continue };
                let Some(vista) = interseccao(r, tela) else { continue };
                let precisa = em_pixels(vista);
                let cobre = |t: [u32; 4]| t[0] <= precisa[0] && t[1] <= precisa[1]
                    && t[0] + t[2] >= precisa[0] + precisa[2] && t[1] + t[3] >= precisa[1] + precisa[3];
                if self.desenhadas.get(&pagina).is_some_and(|d| d.geracao == self.geracao && d.trecho.is_some_and(cobre)) { continue; }
                let mut t = em_pixels(pedido);
                // O desenhador recusa mais de 8000 px de lado.
                t[2] = t[2].min(8000);
                t[3] = t[3].min(8000);
                Some(t)
            };
            if trecho.is_none() && self.desenhadas.get(&pagina).is_some_and(|d| d.geracao == self.geracao && d.trecho.is_none()) { continue; }
            if !self.pedidas.insert(pagina) { continue; }
            let vis = aberto.vis.clone();
            let geracao = self.geracao;
            self.inicio_render.insert(pagina, Instant::now());
            // Modo escuro: fotos, gráficos e carimbos (imagens e grupos) ficam com as cores de verdade.
            let fotos: Option<Vec<[u32; 4]>> = self.modo_escuro.then(|| self.objetos.get(&pagina).filter(|o| o.giro == 0).map(|o| {
                let (dx, dy) = trecho.map_or((0.0, 0.0), |t| (t[0] as f32, t[1] as f32));
                o.lista.iter().filter(|x| matches!(x.tipo, featherpdf_core::TipoObjeto::Imagem { .. } | featherpdf_core::TipoObjeto::Grupo)).map(|x| {
                    let x0 = ((x.caixa[0] - o.caixa[0]) * escala - dx).max(0.0);
                    let y0 = ((o.caixa[3] - x.caixa[3]) * escala - dy).max(0.0);
                    [x0 as u32, y0 as u32, ((x.caixa[2] - x.caixa[0]) * escala).ceil() as u32, ((x.caixa[3] - x.caixa[1]) * escala).ceil() as u32]
                }).collect()
            }).unwrap_or_default());
            tarefas.push(Task::perform(async move {
                tokio::task::spawn_blocking(move || {
                    let mut r = match trecho {
                        Some(t) => vis.desenhar_trecho(pagina, escala, t),
                        None => vis.desenhar(pagina, escala),
                    }?;
                    if let Some(f) = fotos { featherpdf_core::escurecer(&mut r.rgba, r.largura, &f); }
                    Ok(r)
                }).await.unwrap_or_else(|e| Err(format!("Falha na tarefa de desenho: {e}")))
            }, move |resultado| Msg::Desenhada { geracao, pagina, escala, trecho, resultado }));
        }
        Task::batch(tarefas)
    }

    /// Página no terço de cima da área visível.
    fn pagina_no_meio(&self) -> u32 {
        let Some((topo, _, altura)) = self.viewport else { return self.pagina };
        let meio = topo + altura / 3.0;
        let mut y = VAO_PAGINAS;
        for (paginas, h) in self.linhas() {
            if meio <= y + h + VAO_PAGINAS { return paginas[0]; }
            y += h + VAO_PAGINAS;
        }
        self.total_paginas().max(1)
    }

    fn rolar_para(&self, pagina: u32) -> Task<Msg> {
        let mut y = 0.0;
        for (paginas, h) in self.linhas() {
            if paginas.contains(&pagina) { break; }
            y += h + VAO_PAGINAS;
        }
        Task::scroll_to(ID_MESA, Point::new(self.rolagem_x, y))
    }

    fn mudar_zoom(&mut self, zoom: f32) -> Task<Msg> {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        if (zoom - self.zoom).abs() < 0.001 { return Task::none(); }
        self.zoom = zoom;
        let rolar = self.rolar_para(self.pagina);
        Task::batch([self.invalidar(), rolar])
    }

    // ---------- Arquivo ----------

    fn abrir_caminho(caminho: PathBuf, senha: Option<String>) -> Task<Msg> {
        let pedir = caminho.clone();
        Task::perform(async move {
            tokio::task::spawn_blocking(move || {
                // As etapas vão ao Console do F12, como o "[abrir]" do FeatherSheet.
                let inicio = Instant::now();
                // Uma imagem PNG/JPG abre como PDF novo de uma página.
                let pdf = if juntar::e_imagem(&caminho) { juntar::documento_de_arquivo(&caminho)? }
                    else { featherpdf_io::abrir_com_senha(&caminho, senha.as_deref())? };
                let ler = inicio.elapsed();
                let bytes = pdf.serializar()?;
                let serializar = inicio.elapsed() - ler;
                let vis = Visualizador::novo(bytes)?;
                monitor::anotar(format!("[abrir] {} páginas em {} · ler a estrutura {} · serializar {} · preparar o desenho {}",
                    pdf.total_paginas(), monitor::tempo(inicio.elapsed()), monitor::tempo(ler),
                    monitor::tempo(serializar), monitor::tempo(inicio.elapsed() - ler - serializar)));
                Ok((pdf, vis))
            }).await.unwrap_or_else(|e| Err(format!("Falha ao abrir: {e}")))
        }, move |r| match r {
            Err(e) if e == featherpdf_core::PRECISA_SENHA => Msg::Senha(desbloquear::Sb::Pedir(pedir.clone(), None)),
            Err(e) if e == featherpdf_core::SENHA_ERRADA => Msg::Senha(desbloquear::Sb::Pedir(pedir.clone(), Some(e))),
            r => Msg::Aberto(r),
        })
    }

    /// Documento aberto do disco ou criado: numa aba nova, a não ser que a da frente seja o
    /// "sem título" intocado do começo.
    pub(crate) fn entrar(&mut self, pdf: DocumentoPdf, vis: Visualizador) -> Task<Msg> {
        if let Some(caminho) = pdf.origem() {
            self.recentes.registrar(caminho, pdf.total_paginas());
        }
        // Sem origem o PDF sempre conta como modificado: o que diz "intocado" é não ter Ctrl+Z.
        let intocado = self.aberto.as_ref().is_none_or(|a| a.pdf.origem().is_none() && self.historico.is_empty() && a.tamanhos.len() == 1);
        if !intocado {
            self.guias[self.ativa] = self.guardar_guia();
            self.guias.push(None);
            self.ativa = self.guias.len() - 1;
        }
        // O A4 em branco do início não tem arquivo: o zoom escolhido vale para PDF aberto.
        let do_disco = pdf.origem().is_some();
        let entrar = self.entrar_sem_registro(pdf, vis);
        if !do_disco { return entrar; }
        Task::batch([entrar, self.zoom_ao_abrir()])
    }

    /// Põe o documento na frente e recomeça as ferramentas, sem mexer nas abas.
    fn entrar_sem_registro(&mut self, pdf: DocumentoPdf, vis: Visualizador) -> Task<Msg> {
        let tamanhos = vis.tamanhos();
        self.status = format!("{} página(s)", tamanhos.len());
        self.aberto = Some(Aberto { id: novo_id(), pdf, vis, tamanhos });
        self.monitor.insp.novo_documento();
        self.validacao = None;
        self.assinatura = None;
        self.ocr.cancelar();
        self.exportar.esquecer();
        self.scripts.rodada += 1;
        self.ler_navegacao();
        self.pagina = 1;
        self.historico.clear();
        self.refeitos.clear();
        self.objetos.clear();
        self.selecao_texto = None;
        self.form = Default::default();
        self.escolhidos = None;
        self.edicao = None;
        self.medidas.feitas.clear();
        self.medidas.atual = None;
        self.org.selecao = std::collections::BTreeSet::from([1]);
        self.org.dialogo = None;
        self.campo_pagina = None;
        // Os desenhos são do documento anterior.
        self.desenhadas.clear();
        let rolar = Task::scroll_to(ID_MESA, Point::new(0.0, 0.0));
        let desenhar = self.invalidar();
        Task::batch([desenhar, rolar])
    }

    /// Depois de mexer na estrutura (girar, excluir), a vista é refeita do PDF editado.
    fn refazer_vista(&mut self) -> Task<Msg> {
        let Some(aberto) = &mut self.aberto else { return Task::none() };
        match aberto.pdf.serializar().and_then(Visualizador::novo) {
            Ok(vis) => {
                aberto.tamanhos = vis.tamanhos();
                aberto.vis = vis;
                self.monitor.insp.sujo = true;
                // As tabelas lidas para o Converter eram do documento antes da mudança.
                self.exportar.esquecer();
                self.pagina = self.pagina.min(aberto.tamanhos.len() as u32).max(1);
                let tabelas = if self.janelas.aberta(Auxiliar::Converter) { self.ler_tabelas() } else { Task::none() };
                Task::batch([self.invalidar(), tabelas])
            }
            Err(e) => { self.status = e; Task::none() }
        }
    }

    /// O documento na frente, para conferir na volta de um diálogo.
    pub fn id_aberto(&self) -> Option<u64> { self.aberto.as_ref().map(|a| a.id) }

    fn escolher_destino(&self) -> Task<Msg> {
        let Some(id) = self.id_aberto() else { return Task::none() };
        let nome = self.nome_documento().unwrap_or_else(|| "documento.pdf".into());
        Task::perform(async move {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF", &["pdf"]).set_file_name(nome).save_file().await
                .map(|h| h.path().to_path_buf())
        }, move |d| Msg::DestinoEscolhido(id, d))
    }

    fn atualizar(&mut self, msg: Msg) -> Task<Msg> {
        let (status_antes, trabalho_antes, inicio) = (self.status.clone(), self.trabalho_em_curso(), Instant::now());
        let nome = monitor::nome_curto(&msg);
        // Só a chamada de fora anota: as de dentro (Salvar → Destino) já contam no tempo dela.
        self.monitor.profundidade += 1;
        let tarefa = self.update_interno(msg);
        self.monitor.profundidade -= 1;
        let tarefa = if self.monitor.profundidade > 0 { tarefa }
            else { Task::batch([tarefa, self.observar(&status_antes, trabalho_antes, inicio.elapsed(), &nome)]) };
        self.atualizar_texto();
        self.reler_campos();
        // Uma edição esvazia os objetos lidos: com a busca aberta, os achados são refeitos.
        if self.busca.aberta && self.aberto.is_some() && self.objetos.is_empty() {
            let atual = self.busca.atual;
            let total = self.total_paginas();
            self.ler_objetos_sempre(&(1..=total).collect::<Vec<_>>());
            self.update_busca_refazer(atual);
        }
        tarefa
    }

    /// Os módulos chamam `update` como no Iced.
    pub(crate) fn update(&mut self, msg: Msg) -> Task<Msg> { self.atualizar(msg) }

    /// Lê o texto da página atual só quando o painel "Texto da página" está aberto.
    fn atualizar_texto(&mut self) {
        if self.painel != Some(Painel::Texto) || self.texto_da_pagina.0 == self.pagina { return; }
        let Some(a) = &self.aberto else { return };
        let texto = match a.pdf.texto_pagina(self.pagina) {
            Ok(t) if t.trim().is_empty() => "Esta página não tem texto: pode ser imagem ou texto em curvas.".into(),
            Ok(t) => t,
            Err(e) => e,
        };
        self.texto_da_pagina = (self.pagina, texto);
    }

    fn update_interno(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::NovoA4 => {
                self.menu = false;
                match featherpdf_io::criar_a4().and_then(|pdf| Ok((Visualizador::novo(pdf.serializar()?)?, pdf))) {
                    Ok((vis, pdf)) => {
                        // Novo sempre ganha aba própria.
                        self.guias[self.ativa] = self.guardar_guia();
                        self.guias.push(None);
                        self.ativa = self.guias.len() - 1;
                        let t = self.entrar_sem_registro(pdf, vis);
                        self.status = "PDF A4 em branco.".into();
                        return t;
                    }
                    Err(e) => self.status = e,
                }
            }
            Msg::Abrir => {
                self.menu = false;
                return Task::perform(async {
                    rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf", "PDF"]).add_filter("Imagem (vira PDF)", &juntar::EXTENSOES_IMAGEM).pick_files().await
                        .map(|hs| hs.iter().map(|h| h.path().to_path_buf()).collect())
                }, Msg::CaminhoEscolhido);
            }
            Msg::CaminhoEscolhido(Some(caminhos)) => return Task::batch(caminhos.into_iter().map(|c| Self::abrir_caminho(c, None))),
            Msg::AbrirCaminho(caminho) => {
                self.status = format!("Abrindo {}…", caminho.display());
                return Self::abrir_caminho(caminho, None);
            }
            Msg::CaminhoEscolhido(None) | Msg::DestinoEscolhido(_, None) => {}
            Msg::Aberto(Ok((pdf, vis))) => return self.entrar(pdf, vis),
            Msg::Aberto(Err(e)) => self.status = e,
            Msg::Salvar => {
                self.menu = false;
                let Some(a) = &self.aberto else { return Task::none() };
                if !a.pdf.modificado() { self.status = "Nada para salvar.".into(); return Task::none(); }
                let Some(d) = a.pdf.origem().map(PathBuf::from) else { return self.escolher_destino() };
                // Regravar o arquivo inteiro invalida as assinaturas: com elas, só "Salvar como".
                // Todo dicionário de assinatura tem /ByteRange; procurar é linear, sem ler as versões.
                // Sem conseguir ler o original, não dá para saber: na dúvida, "Salvar como".
                // Original apagado: não há assinatura a proteger, grava no lugar.
                match if d.exists() { featherpdf_io::ler_bytes(&d) } else { Ok(vec![]) } {
                    Ok(b) if !b.windows(10).any(|w| w == b"/ByteRange") => {}
                    Ok(_) => {
                        self.status = "Este PDF é assinado: gravar por cima invalidaria as assinaturas. Salve com outro nome.".into();
                        return self.escolher_destino();
                    }
                    Err(e) => {
                        self.status = format!("Não consegui conferir se o original é assinado ({e}). Salve com outro nome.");
                        return self.escolher_destino();
                    }
                }
                return self.update_interno(Msg::DestinoEscolhido(a.id, Some(d)));
            }
            Msg::SalvarComo => { self.menu = false; return self.escolher_destino(); }
            Msg::Medir(m) => return self.update_medir(m),
            Msg::Prot(m) => return self.update_protecao(m),
            Msg::Assin(m) => return self.update_assinatura(m),
            Msg::Rub(m) => return self.update_rubrica(m),
            Msg::Tarja(m) => return self.update_tarja(m),
            Msg::Com(m) => return self.update_comentar(m),
            Msg::Form(m) => return self.update_formulario(m),
            Msg::Marca(m) => return self.update_marcas(m),
            Msg::Rev(m) => return self.update_revisao(m),
            Msg::Juntar(m) => return self.update_juntar(m),
            Msg::Sanitizar(m) => return self.update_sanitizar(m),
            Msg::Imagem(m) => return self.update_imagem(m),
            Msg::Cam(m) => return self.update_camadas(m),
            Msg::FormDados(m) => return self.update_form_dados(m),
            Msg::Ocr(m) => return self.update_ocr(m),
            Msg::Ctx(m) => return self.update_contexto(m),
            Msg::Senha(m) => return self.update_desbloquear(m),
            Msg::Preflight(m) => self.update_preflight(m),
            Msg::Copia(m) => return self.update_copias(m),
            Msg::Regras(m) => return self.update_regras(m),
            Msg::Versoes(m) => return self.update_versoes(m),
            Msg::Reconhecer(m) => return self.update_reconhecer(m),
            Msg::Fluxos(m) => return self.update_fluxos(m),
            Msg::Scripts(m) => return self.update_scripts(m),
            Msg::Equacao(m) => return self.update_equacao(m),
            Msg::Tabelas(m) => return self.update_tabelas_vivas(m),
            Msg::Comparar(m) => return self.update_comparar(m),
            Msg::Regiao(m) => return self.update_regiao(m),
            Msg::Achatar => return self.editar(|pdf| match pdf.achatar_anotacoes(true)? {
                0 => Err("Não há comentários nem campos com aparência para achatar.".into()),
                n => Ok(format!("{n} comentário(s) e campo(s) fundidos na página; não se editam mais. Ctrl+Z desfaz.")),
            }),
            Msg::Selecao(m) => return self.update_selecao(m),
            Msg::Imp(m) => return self.update_imprimir(m),
            Msg::ParaFeatherDocs => self.status = self.para_featherdocs().unwrap_or_else(|e| e),
            // O documento da frente mudou enquanto o diálogo estava aberto: não grava o documento errado.
            Msg::DestinoEscolhido(id, Some(_)) if Some(id) != self.id_aberto() => self.status = "O documento mudou: nada foi salvo. Salve de novo.".into(),
            Msg::DestinoEscolhido(_, Some(destino)) => if let Some(a) = &mut self.aberto {
                let assinado = a.pdf.modificado() && assinar::tem_assinatura(&a.pdf);
                self.status = match featherpdf_io::salvar_como(&mut a.pdf, &destino) {
                    Ok(()) => {
                        self.recentes.registrar(&destino, a.pdf.total_paginas());
                        if assinado { format!("Salvo em {}. As assinaturas que o PDF tinha não valem nesta versão alterada.", destino.display()) }
                        else { format!("Salvo em {}", destino.display()) }
                    }
                    Err(e) => e,
                };
            },
            Msg::IrInicio => return self.fechar_aba(self.ativa),
            Msg::Guia(i) => return self.ir_para_aba(i),
            Msg::FecharGuia(i) => return self.fechar_aba(i),
            Msg::ProximaGuia => return self.ir_para_aba((self.ativa + 1) % self.guias.len()),
            Msg::Soltou(caminho) => {
                let pdf = caminho.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) || juntar::e_imagem(&caminho);
                if self.aba == Aba::Paginas && self.aberto.is_some() && pdf {
                    let depois = self.alvo().last().copied().unwrap_or(self.total_paginas());
                    return self.juntar(vec![caminho], depois + 1);
                }
                return self.update_interno(Msg::AbrirCaminho(caminho));
            }
            Msg::Pag(m) => return self.update_paginas(m),
            Msg::Nav(m) => return self.update_nav(m),
            Msg::Exp(m) => return self.update_exportar(m),
            Msg::Obj(m) => return self.update_objetos(m),
            Msg::Ins(m) => return self.update_ins(m),
            Msg::Busca(m) => return self.update_busca(m),
            Msg::Desfazer => return self.voltar(true),
            Msg::Refazer => return self.voltar(false),
            Msg::Modificadores(m) => self.modificadores = m,
            // Um "clique" da roda = 10%.
            Msg::Roda(passos) => if self.aba != Aba::Paginas {
                return self.mudar_zoom((self.zoom * 1.1f32.powf(passos) * 100.0).round() / 100.0);
            },
            Msg::BuscaRecentes(t) => self.busca_recentes = t,

            Msg::Desenhada { geracao, pagina, escala, trecho, resultado } => {
                if geracao != self.geracao { return Task::none(); }
                self.pedidas.remove(&pagina);
                if let Some(t) = self.inicio_render.remove(&pagina) {
                    self.monitor.desenhou(pagina, t.elapsed().as_millis());
                }
                match resultado.and_then(|r| {
                    let (l, a) = (r.largura, r.altura);
                    let pixels = std::sync::Arc::new(r.rgba.clone());
                    Image::from_rgba(l, a, r.rgba).map(|i| (i, pixels, l, a)).ok_or_else(|| "imagem inválida".to_string())
                }) {
                    Ok((imagem, pixels, largura, altura)) => {
                        self.desenhadas.insert(pagina, Desenho { imagem, pixels, largura, altura, geracao, escala, trecho });
                    }
                    Err(e) => self.status = format!("Página {pagina}: {e}"),
                }
                // A tela pode ter rolado enquanto o trecho era desenhado.
                if trecho.is_some() { return self.pedir_visiveis(); }
            }
            Msg::Rolou(vp) => {
                self.viewport = Some((vp.offset.y, vp.bounds.width, vp.bounds.height));
                self.rolagem_x = vp.offset.x;
                self.pagina = self.pagina_no_meio();
                // Só o desenho em volta da página atual fica na memória.
                let atual = self.pagina;
                self.desenhadas.retain(|&p, _| p.abs_diff(atual) <= 6);
                return self.pedir_visiveis();
            }
            Msg::IrPagina(p) => if (1..=self.total_paginas()).contains(&p) {
                self.pagina = p;
                self.campo_pagina = None;
                return self.rolar_para(p);
            },
            Msg::Anterior => return self.update_interno(Msg::IrPagina(self.pagina.saturating_sub(1))),
            Msg::Proxima => return self.update_interno(Msg::IrPagina(self.pagina + 1)),
            Msg::CampoPagina(t) => self.campo_pagina = Some(t.chars().filter(char::is_ascii_digit).take(6).collect()),
            Msg::ConfirmarPagina => {
                let p = self.campo_pagina.take().and_then(|t| t.parse().ok()).unwrap_or(self.pagina);
                return self.update_interno(Msg::IrPagina(p.clamp(1, self.total_paginas().max(1))));
            }
            Msg::ZoomMais => return self.mudar_zoom((self.zoom * 1.25 * 20.0).round() / 20.0),
            Msg::ZoomMenos => return self.mudar_zoom((self.zoom / 1.25 * 20.0).round() / 20.0),
            Msg::Zoom(z) => return self.mudar_zoom(z),
            Msg::AjustarLargura => {
                let largura_pagina = self.aberto.as_ref()
                    .and_then(|a| a.tamanhos.get(self.pagina as usize - 1)).map_or(595.0, |t| t.0);
                let visivel = self.viewport.map_or(800.0, |v| v.1);
                let por_linha = if self.disposicao == Disposicao::DuasPaginas { 2.0 } else { 1.0 };
                return self.mudar_zoom((visivel - 3.0 * VAO_PAGINAS - 12.0) / (largura_pagina * por_linha));
            }
            Msg::AjustarPagina => {
                let (l, a) = self.aberto.as_ref().and_then(|x| x.tamanhos.get(self.pagina as usize - 1)).copied().unwrap_or((595.0, 842.0));
                let (_, largura, altura) = self.viewport.unwrap_or((0.0, 800.0, 600.0));
                let por_linha = if self.disposicao == Disposicao::DuasPaginas { 2.0 } else { 1.0 };
                let zoom = ((largura - 3.0 * VAO_PAGINAS) / (l * por_linha)).min((altura - 2.0 * VAO_PAGINAS) / a);
                return self.mudar_zoom(zoom);
            }
            Msg::Disposicao(d) => { self.disposicao = d; return self.invalidar(); }
            Msg::GirarEsquerda | Msg::GirarDireita | Msg::Girar(_) => {
                let graus = match msg { Msg::GirarDireita => 90, Msg::GirarEsquerda => -90, Msg::Girar(g) => g, _ => 0 };
                let alvo = self.alvo();
                return self.editar(|pdf| {
                    pdf.girar_paginas(&alvo, graus)?;
                    Ok(format!("{} página(s) girada(s). Ctrl+Z desfaz.", alvo.len()))
                });
            }
            Msg::Excluir => {
                let alvo = self.alvo();
                if alvo.len() >= self.total_paginas() as usize {
                    self.status = "O PDF precisa de ao menos uma página.".into();
                    return Task::none();
                }
                let tarefa = self.editar(|pdf| {
                    pdf.excluir_paginas(&alvo)?;
                    Ok(format!("{} página(s) excluída(s). Ctrl+Z desfaz.", alvo.len()))
                });
                let primeira = alvo[0].min(self.total_paginas()).max(1);
                self.org.selecao = std::collections::BTreeSet::from([primeira]);
                self.pagina = primeira;
                return tarefa;
            }
            Msg::ExcluirOutras => {
                let p = self.pagina;
                let outras: Vec<u32> = (1..=self.total_paginas()).filter(|&q| q != p).collect();
                if outras.is_empty() { return Task::none(); }
                let tarefa = self.editar(move |pdf| pdf.excluir_paginas(&outras).map(|_| format!("Só a página {p} ficou. Ctrl+Z desfaz.")));
                self.org.selecao = std::collections::BTreeSet::from([1]);
                self.pagina = 1;
                return tarefa;
            }

            Msg::MudarAba(aba) => {
                self.aba = aba;
                self.menu = false;
                self.faixa_aberta = true;
                if aba == Aba::Paginas {
                    if self.org.selecao.is_empty() { self.org.selecao.insert(self.pagina); }
                    return self.pedir_miniaturas();
                }
                return self.rolar_para(self.pagina);
            }
            Msg::AlternarFaixa => { self.faixa_aberta = !self.faixa_aberta; self.config.faixa = self.faixa_aberta; self.config.gravar(); }
            Msg::Leitura => self.leitura = !self.leitura,
            Msg::AlternarFerramentas => { self.ferramentas_abertas = !self.ferramentas_abertas; self.config.ferramentas = self.ferramentas_abertas; self.config.gravar(); }
            Msg::FiltroFerramentas(t) => { self.filtro_ferramentas = t; self.ferramentas_abertas = true; }
            Msg::BuscaComandos(t) => self.busca_comandos = t,
            Msg::Focar(id) => return Task::focus(id),
            Msg::Pesquisar => self.foco_pesquisa += 1,
            Msg::CopiarTexto => {
                self.status = format!("Texto da página {} copiado.", self.texto_da_pagina.0);
                return Task::write_clipboard(self.texto_da_pagina.1.clone());
            }
            Msg::LarguraPainel(Some(x)) => {
                // O painel encosta no trilho: a largura é o que sobra à direita do cursor.
                let (jl, _) = inserir::janela();
                if self.arrastando_painel && jl > 0.0 { self.largura_painel = (jl - 74.0 - x).clamp(240.0, (jl * 0.6).max(240.0)); }
                self.arrastando_painel = true;
            }
            Msg::LarguraPainel(None) => self.arrastando_painel = false,
            Msg::AlternarPainel(p) => {
                self.painel = if self.painel == Some(p) { None } else { Some(p) };
                if matches!(self.painel, Some(Painel::Marcadores | Painel::Anexos)) { self.ler_navegacao(); }
                if self.painel == Some(Painel::Miniaturas) { return self.pedir_miniaturas(); }
            }
            Msg::Ferramenta(f) => {
                let concluir = self.concluir_medida();
                self.ferramenta = f;
                self.ins.texto = None;
                self.mao = None;
                if f == Ferramenta::Editar { return Task::batch([concluir, self.pedir_visiveis()]); }
                self.escolhidos = None;
                self.edicao = None;
                return concluir;
            }
            Msg::Menu => { self.menu = !self.menu; self.suspenso = None; }
            Msg::DoSuspenso(m) => { self.suspenso = None; return self.update_interno(*m); }
            Msg::Comando(m) => { self.busca_comandos.clear(); return self.update_interno(*m); }
            Msg::Suspenso(qual) => self.suspenso = match qual {
                Some(q) if self.suspenso.map(|s| s.0) != Some(q) => {
                    let (x, y) = inserir::ponteiro();
                    Some((q, Point::new(x - 40.0, y + 16.0)))
                }
                _ => None,
            },
            Msg::Escape => { self.menu = false; self.suspenso = None; self.mao = None; }
            Msg::Status(t) => if !t.is_empty() { self.status = t },
            Msg::MaoApertou => if self.ferramenta == Ferramenta::Mao { self.mao = Some((None, Point::new(self.rolagem_x, self.viewport.map_or(0.0, |v| v.0)))) },
            Msg::MaoMoveu(p) => match &mut self.mao {
                Some((inicio @ None, _)) => *inicio = Some(p),
                Some((Some(inicio), rolagem)) => {
                    let alvo = Point::new((rolagem.x - (p.x - inicio.x)).max(0.0), (rolagem.y - (p.y - inicio.y)).max(0.0));
                    return Task::scroll_to(ID_MESA, alvo);
                }
                None => {}
            },
            Msg::MaoSoltou => self.mao = None,
            Msg::AbrirAjuda => return self.alternar_auxiliar(Auxiliar::Ajuda),
            Msg::AbrirConfiguracoes => return self.alternar_auxiliar(Auxiliar::Configuracoes),
            Msg::AbrirMonitor => {
                self.monitor.amostrar();
                self.monitor.insp.sujo = true;
                return self.alternar_auxiliar(Auxiliar::Monitor);
            }
            Msg::EmbutirFontes => return self.editar(|pdf| {
                let (feitas, faltaram) = featherpdf_io::embutir_fontes_do_sistema(pdf)?;
                if feitas.is_empty() && faltaram.is_empty() { return Err("Todas as fontes já estão no arquivo (ou são de um tipo que não precisa).".into()); }
                let trocas: Vec<String> = feitas.iter().map(|(a, b)| if a.contains(b.split('-').next().unwrap_or(b)) { a.clone() } else { format!("{a} → {b}") }).collect();
                let mut aviso = format!("{} fonte(s) embutida(s): {}.", feitas.len(), trocas.join(", "));
                if !faltaram.is_empty() { aviso += &format!(" Sem nenhuma parecida no sistema: {}.", faltaram.join(", ")); }
                Ok(aviso + " Ctrl+Z desfaz.")
            }),
            Msg::LimparCaderno => return self.editar(|pdf| {
                let paginas: Vec<u32> = (1..=pdf.total_paginas()).collect();
                match pdf.limpar_fundo(&paginas, &featherpdf_io::limpar_caderno_jpeg) {
                    0 => Err("Nenhuma foto de página para limpar (só funciona em páginas escaneadas ou fotografadas).".into()),
                    n => Ok(format!("{n} foto(s) limpas: sem sombra, pauta e quadriculado (Ctrl+Z desfaz).")),
                }
            }),
            Msg::ModoEscuro => { self.modo_escuro = !self.modo_escuro; return self.invalidar(); }
            Msg::AlternarTema => {
                let t = if self.tema_escuro { configuracoes::Tema::Claro } else { configuracoes::Tema::Escuro };
                return self.update_config(configuracoes::Cf::Tema(t));
            }
            Msg::Config(m) => return self.update_config(m),
            Msg::SecaoConfig(i) => self.config_secao = i,
            Msg::AjudaEvento(e) => self.ajuda.atualizar(e, ajuda_pdf::modelo()),

            Msg::EscalaTela(f) => if (f - self.escala_tela).abs() > 0.01 {
                self.escala_tela = f;
                return self.invalidar();
            },
            Msg::Arrastar(id) => return Task::window_of(id, WindowCmd::Drag),
            Msg::Puxar(d) => return Task::window(WindowCmd::DragResize(d)),
            Msg::Minimizar(id) => return Task::window_of(id, WindowCmd::Minimize),
            Msg::Maximizar(id) => return Task::window_of(id, WindowCmd::ToggleMaximize),
            Msg::TelaCheia => return Task::window(WindowCmd::ToggleFullscreen),
            Msg::FecharJanela(id) => {
                if id != WindowId::MAIN {
                    self.janelas.retirar(id);
                    return Task::window_of(id, WindowCmd::Close);
                }
                let alteradas = (0..self.guias.len()).filter(|&i| self.modificada(i)).count();
                if alteradas > 0 && self.fechar_pendente != Some(usize::MAX) {
                    self.fechar_pendente = Some(usize::MAX);
                    self.status = format!("{alteradas} documento(s) com alterações não salvas. Clique de novo em fechar para sair sem salvar.");
                    return Task::none();
                }
                return Task::window(WindowCmd::Close);
            }
            Msg::Teclado(id, key, mods) => return self.tratar_tecla(id, key, mods),
            Msg::MonitorTick => self.monitor.amostrar(),
            Msg::MonitorEvento(e) => return self.monitor.tela.atualizar(e),
            Msg::Insp(m) => return self.update_inspetor(m),
        }
        Task::none()
    }

    pub(crate) fn alternar_auxiliar(&mut self, tipo: Auxiliar) -> Task<Msg> {
        let tamanho = match tipo {
            Auxiliar::Ajuda => Size::new(970.0, 700.0),
            Auxiliar::Monitor => Size::new(1000.0, 640.0),
            Auxiliar::Configuracoes => Size::new(920.0, 640.0),
            Auxiliar::Converter => Size::new(820.0, 520.0),
            Auxiliar::Ocr => Size::new(1160.0, 740.0),
            Auxiliar::Imprimir => Size::new(940.0, 700.0),
            Auxiliar::Fluxos => Size::new(1280.0, 800.0),
        };
        // Teste com o mouse: `FEATHERPDF_ROTEIRO_OCR=roteiro.txt` (o nome do tipo em maiúsculas) faz o cursor
        // do pelicano andar na janela que abriu.
        let roteiro = std::env::var(format!("FEATHERPDF_ROTEIRO_{}", format!("{tipo:?}").to_uppercase())).ok()
            .and_then(|arq| std::fs::read_to_string(arq).ok()).filter(|_| !self.janelas.aberta(tipo));
        let abrir = self.janelas.alternar(tipo, tamanho, true);
        match (roteiro, self.janelas.id(tipo)) {
            (Some(r), Some(id)) => Task::batch([abrir, Task::window_of(id, WindowCmd::Roteiro(r))]),
            _ => abrir,
        }
    }

    fn tratar_tecla(&mut self, id: WindowId, key: Key, mods: Modifiers) -> Task<Msg> {
        if let Some(tipo) = self.janelas.tipo(id) {
            if tipo == Auxiliar::Fluxos && matches!(key, Key::Named(NamedKey::Delete)) { return self.update(Msg::Fluxos(fluxos::Fx::TirarEscolhido)); }
            let painel = match tipo {
                Auxiliar::Ajuda => atalhos::PainelGlobal::Ajuda,
                Auxiliar::Monitor => atalhos::PainelGlobal::Monitor,
                Auxiliar::Configuracoes | Auxiliar::Converter | Auxiliar::Ocr | Auxiliar::Imprimir | Auxiliar::Fluxos => atalhos::PainelGlobal::Configuracoes,
            };
            if atalhos::fechar_painel(painel, &key, mods) { return self.update(Msg::FecharJanela(id)); }
            return Task::none();
        }
        if let Some(acao) = atalhos::acao_global(&key, mods) {
            return match acao {
                atalhos::AcaoGlobal::Ajuda => self.update(Msg::AbrirAjuda),
                atalhos::AcaoGlobal::Monitor => self.update(Msg::AbrirMonitor),
                atalhos::AcaoGlobal::AlternarFaixa => self.update(Msg::AlternarFaixa),
            };
        }
        let ctrl = mods.control_key();
        let shift = mods.shift_key();
        let organizando = self.aba == Aba::Paginas && self.aberto.is_some();
        let letra = match key.as_ref() { Key::Character(c) => c.to_lowercase(), _ => String::new() };
        let c = |k: &str| letra == k;
        let n = |k: NamedKey| key == Key::Named(k);
        use NamedKey as N;
        if n(N::Escape) {
            return if self.menu || self.suspenso.is_some() { self.update(Msg::Escape) }
            else if self.leitura { self.update(Msg::Leitura) }
            else if self.protecao.is_some() { self.update(Msg::Prot(proteger::Pr::Fechar)) }
            else if self.assinatura.as_ref().is_some_and(|s| s.marcando()) { self.update(Msg::Assin(assinar::As::Marcar(false))) }
            else if matches!(self.painel, Some(Painel::Assinar | Painel::Assinaturas)) { self.update(Msg::Assin(assinar::As::Fechar)) }
            else if self.pedido_senha.is_some() { self.update(Msg::Senha(desbloquear::Sb::Fechar)) }
            else if self.contexto.is_some() { self.update(Msg::Ctx(contexto::Ct::Fechar)) }
            else if self.org.dialogo.is_some() { self.update(Msg::Pag(organizar::Pag::Fechar)) }
            else if self.ins.link.is_some() || self.ins.texto.is_some() || self.ins.menu.is_some() { self.update(Msg::Ins(inserir::Ins::Fechar)) }
            else if self.medidas.dialogo.is_some() { self.update(Msg::Medir(medir::Med::FecharEscala)) }
            else if self.medidas.atual.is_some() { self.update(Msg::Medir(medir::Med::Cancelar)) }
            else if self.form.criando.is_some() { self.update(Msg::Form(formularios::Fm::Fechar)) }
            else if self.comentarios.pendente.is_some() { self.update(Msg::Com(comentar::Cm::Fechar)) }
            else if self.busca.aberta { self.update(Msg::Busca(busca::Busca::Fechar)) }
            else if self.escolhidos.is_some() || self.edicao.is_some() { self.update(Msg::Obj(edicao::Obj::Cancelar)) }
            else if self.ferramenta != Ferramenta::Selecionar { self.update(Msg::Ferramenta(Ferramenta::Selecionar)) }
            else { Task::none() };
        }
        if self.medidas.atual.is_some() {
            if n(N::Enter) { return self.update(Msg::Medir(medir::Med::Terminar)); }
            if n(N::Backspace) { return self.update(Msg::Medir(medir::Med::DesfazerPonto)); }
        }
        if ctrl && c("f") { return self.update(Msg::Busca(busca::Busca::Abrir(false))); }
        if ctrl && c("h") { return self.update(Msg::Busca(busca::Busca::Abrir(true))); }
        if (n(N::Delete) || n(N::Backspace)) && self.escolhidos.is_some() && self.editando_objetos() { return self.update(Msg::Obj(edicao::Obj::Excluir)); }
        if self.escolhidos.is_some() && self.editando_objetos() {
            let passo = if shift { 10.0 } else { 1.0 };
            let seta = if n(N::ArrowLeft) { Some((-passo, 0.0)) } else if n(N::ArrowRight) { Some((passo, 0.0)) }
                else if n(N::ArrowUp) { Some((0.0, -passo)) } else if n(N::ArrowDown) { Some((0.0, passo)) } else { None };
            if let Some((x, y)) = seta { return self.update(Msg::Obj(edicao::Obj::Seta(x, y))); }
        }
        let msg = if ctrl && shift && c("z") { Msg::Refazer }
            else if ctrl && c("z") { Msg::Desfazer }
            else if ctrl && c("y") { Msg::Refazer }
            else if n(N::Delete) && organizando { Msg::Excluir }
            else if ctrl && c("a") && organizando { Msg::Pag(organizar::Pag::SelecionarTudo) }
            else if n(N::PageDown) { Msg::Proxima }
            else if n(N::PageUp) { Msg::Anterior }
            else if ctrl && c("c") && self.selecao_texto.is_some() { Msg::Selecao(selecao::Sel::Copiar) }
            else if ctrl && c("v") && !organizando { Msg::Ins(inserir::Ins::Colar) }
            else if ctrl && c("p") { Msg::Imp(imprimir::Im::Abrir) }
            else if ctrl && c("o") { Msg::Abrir }
            else if ctrl && shift && c("s") { Msg::SalvarComo }
            else if ctrl && c("s") { Msg::Salvar }
            else if ctrl && c("n") { Msg::NovoA4 }
            else if ctrl && c("k") { Msg::Pesquisar }
            else if ctrl && (c("=") || c("+")) { Msg::ZoomMais }
            else if ctrl && c("-") { Msg::ZoomMenos }
            else if ctrl && c("0") { Msg::Zoom(1.0) }
            else if ctrl && c("w") { Msg::IrInicio }
            else if ctrl && n(N::Tab) { Msg::ProximaGuia }
            else if n(N::F11) { Msg::TelaCheia }
            else { return Task::none() };
        self.update(msg)
    }

    pub fn acoes_janela(id: WindowId) -> cromo::AcoesJanela<Msg> {
        cromo::AcoesJanela {
            arrastar: Msg::Arrastar(id),
            minimizar: Msg::Minimizar(id),
            maximizar: Msg::Maximizar(id),
            fechar: Msg::FecharJanela(id),
        }
    }
}

fn interseccao(a: Rect, b: Rect) -> Option<Rect> {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = ((a.x + a.width).min(b.x + b.width), (a.y + a.height).min(b.y + b.height));
    (x1 > x0 && y1 > y0).then(|| Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
}

/// Abre o arquivo no app da suíte; sem ele instalado, diz onde o arquivo ficou.
pub(crate) fn abrir_no(comando: &str, nome: &str, arquivo: &std::path::Path) -> Result<(), String> {
    match std::process::Command::new(comando).arg(arquivo).spawn() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound =>
            Err(format!("{nome} não está instalado. O arquivo ficou em {}.", arquivo.display())),
        Err(e) => Err(format!("Não consegui abrir o {nome}: {e}")),
    }
}

impl pelicano::App for App {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> { self.atualizar(msg) }

    fn view(&self) -> Element<'_, Msg> { tela::janela(self) }

    fn view_window(&self, id: WindowId) -> Element<'_, Msg> {
        match self.janelas.tipo(id) {
            Some(Auxiliar::Ajuda) => cromo::painel_utilitario(
                "Ajuda · FeatherPDF",
                ajuda_pdf::modelo().view(&self.ajuda).map(Msg::AjudaEvento),
                Msg::FecharJanela(id),
            ),
            Some(Auxiliar::Monitor) => self.view_monitor(id),
            Some(Auxiliar::Configuracoes) => self.view_configuracoes(id),
            Some(Auxiliar::Converter) => self.view_exportar(id),
            Some(Auxiliar::Ocr) => self.view_ocr(id),
            Some(Auxiliar::Imprimir) => self.view_imprimir(id),
            Some(Auxiliar::Fluxos) => self.view_fluxos(id),
            None => tela::janela(self),
        }
    }

    fn theme(&self) -> Theme { tema::tema() }

    fn subscription(&self) -> Subscription<Msg> {
        let mut subs = Vec::new();
        if self.janelas.aberta(Auxiliar::Monitor) {
            subs.push(Subscription::every(Duration::from_secs(1)).map(|_| Msg::MonitorTick));
        }
        if self.comprimindo() {
            subs.push(Subscription::every(Duration::from_millis(50)).map(|_| Msg::Prot(proteger::Pr::Tique)));
        }
        if self.ocr.rodando() {
            subs.push(Subscription::every(Duration::from_secs(1)).map(|_| Msg::Ocr(ocr::Oc::Tique)));
        }
        Subscription::batch(subs)
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        inserir::observar(ev);
        match ev {
            Event::FileDropped(caminho) => Some(Msg::Soltou(caminho.clone())),
            Event::ScaleChanged(e) => Some(Msg::EscalaTela(*e)),
            Event::ModifiersChanged(m) => Some(Msg::Modificadores(*m)),
            Event::CursorMoved(p) if self.arrastando_painel => Some(Msg::LarguraPainel(Some(p.x))),
            Event::MouseReleased(MouseButton::Left) if self.arrastando_painel => Some(Msg::LarguraPainel(None)),
            Event::Wheel { dy, modifiers, .. } if modifiers.control_key() => Some(Msg::Roda(dy.signum())),
            Event::CloseRequested => Some(Msg::FecharJanela(WindowId::MAIN)),
            _ => None,
        }
    }

    fn on_window_event(&self, id: WindowId, ev: &Event) -> Option<Msg> {
        if id == WindowId::MAIN { return self.on_event(ev); }
        // Fluxos: arrastar blocos dentro da janela e soltar pasta ou arquivos vindos de fora.
        if self.janelas.tipo(id) == Some(Auxiliar::Fluxos) {
            match ev {
                Event::FileDropped(p) => return Some(Msg::Fluxos(fluxos::Fx::Solto(p.clone()))),
                Event::CursorMoved(p) if self.fluxos.arrastando() => return Some(Msg::Fluxos(fluxos::Fx::Cursor(*p))),
                Event::MouseReleased(MouseButton::Left) if self.fluxos.arrastando() => return Some(Msg::Fluxos(fluxos::Fx::Largar)),
                _ => {}
            }
        }
        match ev {
            Event::CloseRequested => Some(Msg::FecharJanela(id)),
            _ => None,
        }
    }

    fn on_unhandled_event(&self, id: WindowId, ev: &Event) -> Option<Msg> {
        match ev {
            Event::Key { key, modifiers, pressed: true, .. } => Some(Msg::Teclado(id, key.clone(), *modifiers)),
            _ => None,
        }
    }
}

fn main() -> Result<(), Error> {
    // A marca antes da janela: o app_id ("featherpdf") e o ícone saem dela; depois seria o padrão (FeatherSheet).
    marca::definir_marca(marca::Produto::FeatherPDF, false);
    pelicano::run(
        marca::preparar_janela(Settings {
            title: "FeatherPDF".into(),
            size: Size::new(1280.0, 820.0),
            min_size: Some(Size::new(760.0, 480.0)),
            decorations: false,
            close_on_request: false,
            ..Default::default()
        }),
        App::novo,
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn abas_abrem_trocam_e_pedem_confirmacao_para_fechar() {
        let (mut app, _) = App::novo();
        let _ = app.update(Msg::NovoA4);
        assert_eq!((app.guias.len(), app.ativa), (2, 1));
        // A aba nova ganha uma página a mais; a primeira continua com uma.
        let _ = app.update(Msg::Pag(organizar::Pag::InserirBranco));
        assert_eq!(app.total_paginas(), 2);
        let _ = app.update(Msg::Guia(0));
        assert_eq!((app.ativa, app.total_paginas()), (0, 1));
        let _ = app.update(Msg::Guia(1));
        assert_eq!(app.total_paginas(), 2);
        // O Ctrl+Z da aba volta com ela.
        assert!(app.pode_desfazer());
        // Com alterações, o primeiro × só avisa; o segundo fecha e a outra aba vem para a frente.
        let _ = app.update(Msg::FecharGuia(1));
        assert_eq!(app.guias.len(), 2);
        let _ = app.update(Msg::FecharGuia(1));
        assert_eq!((app.guias.len(), app.ativa, app.total_paginas()), (1, 0, 1));
        // A última aba não some: vira um PDF em branco.
        let _ = app.update(Msg::FecharGuia(0));
        assert_eq!(app.guias.len(), 1);
    }

    #[test]
    fn abrir_no_comeco_usa_a_aba_em_branco() {
        let (mut app, _) = App::novo();
        let bytes = featherpdf_io::criar_a4().unwrap().serializar().unwrap();
        let pdf = DocumentoPdf::de_bytes(&bytes, Some(PathBuf::from("/tmp/x.pdf"))).unwrap();
        let vis = Visualizador::novo(bytes).unwrap();
        let _ = app.update(Msg::Aberto(Ok((pdf, vis))));
        assert_eq!(app.guias.len(), 1);
    }

    #[test]
    fn desfazer_e_refazer_voltam_a_estrutura() {
        let (mut app, _) = App::novo();
        let _ = app.update(Msg::Pag(organizar::Pag::Duplicar));
        assert_eq!(app.total_paginas(), 2);
        let _ = app.update(Msg::Desfazer);
        assert_eq!(app.total_paginas(), 1);
        let _ = app.update(Msg::Refazer);
        assert_eq!(app.total_paginas(), 2);
    }

    fn abrir_de(app: &mut App, origem: &std::path::Path) {
        let bytes = featherpdf_io::criar_a4().unwrap().serializar().unwrap();
        let pdf = DocumentoPdf::de_bytes(&bytes, Some(origem.to_path_buf())).unwrap();
        let _ = app.update(Msg::Aberto(Ok((pdf, Visualizador::novo(bytes).unwrap()))));
    }

    fn temporario(nome: &str) -> PathBuf {
        let c = std::env::temp_dir().join(format!("featherpdf-teste-{}-{nome}.pdf", std::process::id()));
        let _ = std::fs::remove_file(&c);
        c
    }

    #[test]
    fn salvar_sem_alteracao_nao_grava() {
        let (mut app, _) = App::novo();
        let c = temporario("intocado");
        abrir_de(&mut app, &c);
        let _ = app.update(Msg::Salvar);
        assert!(!c.exists());
        assert_eq!(app.status, "Nada para salvar.");
    }

    #[test]
    fn desfazer_depois_de_salvar_continua_modificado() {
        let (mut app, _) = App::novo();
        let c = temporario("desfazer");
        abrir_de(&mut app, &c);
        let _ = app.update(Msg::Pag(organizar::Pag::Duplicar));
        let _ = app.update(Msg::Salvar);
        assert!(c.exists());
        assert!(!app.aberto.as_ref().unwrap().pdf.modificado());
        let _ = app.update(Msg::Desfazer);
        assert!(app.aberto.as_ref().unwrap().pdf.modificado());
        let _ = std::fs::remove_file(&c);
    }

    #[test]
    fn destino_de_outra_aba_e_ignorado() {
        let (mut app, _) = App::novo();
        let primeiro = app.id_aberto().unwrap();
        let _ = app.update(Msg::NovoA4);
        let c = temporario("outra-aba");
        let _ = app.update(Msg::DestinoEscolhido(primeiro, Some(c.clone())));
        assert!(!c.exists());
        let _ = app.update(Msg::DestinoEscolhido(app.id_aberto().unwrap(), Some(c.clone())));
        assert!(c.exists());
        let _ = std::fs::remove_file(&c);
    }

    #[test]
    fn alteracao_nova_rearma_o_aviso_de_fechar() {
        let (mut app, _) = App::novo();
        let _ = app.update(Msg::NovoA4);
        let _ = app.update(Msg::Pag(organizar::Pag::InserirBranco));
        let _ = app.update(Msg::FecharGuia(1));
        let _ = app.update(Msg::Pag(organizar::Pag::InserirBranco));
        // Depois da alteração, o segundo × volta a só avisar.
        let _ = app.update(Msg::FecharGuia(1));
        assert_eq!(app.guias.len(), 2);
    }

    /// `FEATHERPDF_CAPTURA=tela.png cargo test -p featherpdf-pelicano captura -- --ignored`: a janela em PNG, sem abrir janela.
    /// `FEATHERPDF_CAPTURA_ABAS=1` também grava uma imagem por aba da faixa (tela-1.png, tela-2.png…).
    #[test]
    #[ignore]
    fn captura() {
        use pelicano::App as _;
        let destino = std::env::var("FEATHERPDF_CAPTURA").expect("FEATHERPDF_CAPTURA=caminho.png");
        let (mut app, _) = App::novo();
        app.viewport = Some((0.0, 1000.0, 640.0));
        let resultado = app.aberto.as_ref().unwrap().vis.desenhar(1, 1.0);
        let geracao = app.geracao;
        let _ = app.update(Msg::Desenhada { geracao, pagina: 1, escala: 1.0, trecho: None, resultado });
        let foto = |app: &App| capture(&app.view(), Size::new(1672.0, 941.0), 1.0, &app.theme(), &mut Renderer::new(), None);
        std::fs::write(&destino, foto(&app)).unwrap();
        if std::env::var_os("FEATHERPDF_CAPTURA_ABAS").is_some() {
            for (i, aba) in [Aba::Paginas, Aba::Editar, Aba::Comentar, Aba::Formularios, Aba::Medir, Aba::Proteger, Aba::Converter, Aba::Exibir].into_iter().enumerate() {
                let _ = app.update(Msg::MudarAba(aba));
                std::fs::write(destino.replace(".png", &format!("-{}.png", i + 1)), foto(&app)).unwrap();
            }
        }
    }
}
