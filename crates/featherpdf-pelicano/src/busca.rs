//! Localizar (Ctrl+F) e Localizar e trocar (Ctrl+H): barra flutuante no canto
//! da mesa, achados em amarelo nas páginas, Enter vai para o próximo.
//! A busca é por linha de texto (os objetos da ferramenta Editar texto); por escolha,
//! diferencia maiúscula e só pega a palavra inteira. Trocar regrava a linha na fonte
//! dela, com Ctrl+Z; os achados saem em CSV.

use featherpdf_core::{achar, OpcoesBusca, TipoObjeto};
use feather_ui::{icones, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::Cursor;
use crate::compat::canvas::{self, Frame, Geometry};
use crate::edicao::na_tela;
use crate::{App, Msg};

pub fn id_localizar() -> &'static str { "featherpdf.localizar" }
fn id_trocar() -> &'static str { "featherpdf.trocar" }

#[derive(Debug, Clone)]
pub enum Busca {
    /// Abre a barra; `true` mostra também o campo de trocar.
    Abrir(bool),
    Fechar,
    Termo(String),
    Novo(String),
    Proximo,
    Anterior,
    Trocar,
    TrocarTudo,
    /// Liga/desliga refazer o parágrafo ao trocar.
    Refluxo,
    Maiusculas(bool),
    Palavra(bool),
    /// Pede onde gravar os achados em CSV.
    Exportar,
    /// O CSV pronto e o destino escolhido.
    Gravar(String, Option<std::path::PathBuf>),
}

/// Um achado: página, objeto (linha) e posição do termo na linha, em caracteres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Achado { pub pagina: u32, pub objeto: usize, pub inicio: usize }

#[derive(Debug, Default)]
pub struct EstadoBusca {
    pub aberta: bool,
    pub trocar: bool,
    pub termo: String,
    pub novo: String,
    pub achados: Vec<Achado>,
    pub atual: usize,
    /// Trocar refaz as quebras do parágrafo (a frase nova não passa da margem).
    pub refluxo: bool,
    pub opcoes: OpcoesBusca,
    /// Campos (nome, valor, página) e anexos (nome, texto quando é texto) lidos ao abrir a barra.
    campos: Vec<(String, String, u32)>,
    anexos: Vec<(String, Option<String>)>,
    /// Achados fora das linhas da página: o que mostrar e a página (campo) ou nada (anexo).
    pub outros: Vec<(String, Option<u32>)>,
}

/// Teto do texto de anexo lido para a busca: um por um e no total (anexo grande não enche a memória).
const TETO_ANEXO: usize = 4 << 20;
const TETO_ANEXOS: usize = 32 << 20;

/// Destaques amarelos dos achados de uma página (o atual mais forte).
struct Destaques { caixas: Vec<(Rect, bool)> }

impl canvas::Program<Msg> for Destaques {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        for (r, atual) in &self.caixas {
            let cor = if *atual { Color::rgba(1.0, 0.55, 0.0, 0.45) } else { Color::rgba(1.0, 0.85, 0.0, 0.38) };
            f.fill_rectangle(r.position(), r.size(), cor);
        }
        vec![f.into_geometry()]
    }
}

impl App {
    /// Lê os objetos de todas as páginas (para buscar). ~1–2 ms por página.
    fn ler_todos_objetos(&mut self) {
        let total = self.total_paginas();
        let pendentes: Vec<u32> = (1..=total).filter(|p| !self.objetos.contains_key(p)).collect();
        self.ler_objetos_sempre(&pendentes);
    }

    /// Lê os campos e o texto dos anexos (uma vez por abertura da barra ou troca).
    fn ler_campos_e_anexos(&mut self) {
        let Some(a) = &self.aberto else { return };
        self.busca.campos = a.pdf.campos().into_iter().map(|c| (c.nome, c.valor, c.pagina)).collect();
        let mut lido = 0;
        self.busca.anexos = a.pdf.anexos().into_iter().enumerate().map(|(i, an)| {
            // `anexo_ate`: o /Size pode mentir; a leitura para no teto de qualquer jeito.
            // Conta o que foi lido antes de ver se é texto: binário também custa descomprimir.
            let texto = (an.tamanho <= TETO_ANEXO && lido + an.tamanho <= TETO_ANEXOS).then(|| a.pdf.anexo_ate(i, TETO_ANEXO as u64)).flatten()
                .and_then(|(_, b)| { lido += b.len(); String::from_utf8(b).ok() });
            (an.nome, texto)
        }).collect();
    }

    fn refazer_busca(&mut self) {
        let b = &mut self.busca;
        b.achados.clear();
        b.outros.clear();
        if b.termo.is_empty() { return; }
        let tem = |t: &str| !achar(t, &b.termo, b.opcoes).is_empty();
        let mut outros = Vec::new();
        for (nome, valor, pagina) in &b.campos {
            if tem(nome) || tem(valor) { outros.push((format!("Campo “{nome}”: {valor}"), Some(*pagina))); }
        }
        for (nome, texto) in &b.anexos {
            if tem(nome) { outros.push((format!("Anexo “{nome}” (nome)"), None)); }
            else if texto.as_deref().is_some_and(tem) { outros.push((format!("Anexo “{nome}” (conteúdo)"), None)); }
        }
        b.outros = outros;
        let mut paginas: Vec<&u32> = self.objetos.keys().collect();
        paginas.sort();
        for &p in paginas {
            for (k, o) in self.objetos[&p].lista.iter().enumerate() {
                if let TipoObjeto::Texto { texto, .. } = &o.tipo {
                    b.achados.extend(achar(texto, &b.termo, b.opcoes).into_iter().map(|inicio| Achado { pagina: p, objeto: k, inicio }));
                }
            }
        }
        // Começa no primeiro achado a partir da página atual.
        b.atual = b.achados.iter().position(|a| a.pagina >= self.pagina).unwrap_or(0);
    }

    /// Os achados em CSV (";", UTF-8 com BOM para o Excel): página, posição e a linha inteira.
    fn achados_csv(&self) -> String {
        let mut csv = String::from("\u{FEFF}Página;Posição;Linha\n");
        for a in &self.busca.achados {
            let linha = self.objetos.get(&a.pagina).and_then(|o| o.lista.get(a.objeto)).and_then(|o| match &o.tipo {
                TipoObjeto::Texto { texto, .. } => Some(texto.trim().to_string()),
                _ => None,
            }).unwrap_or_default();
            csv += &format!("{};{};{}\n", a.pagina, a.inicio + 1, celula_csv(&linha));
        }
        csv
    }

    /// Refaz os achados guardando a posição (depois de uma edição).
    pub fn update_busca_refazer(&mut self, atual: usize) {
        self.refazer_busca();
        self.busca.atual = atual.min(self.busca.achados.len().saturating_sub(1));
    }

    fn ir_ao_atual(&mut self) -> Task<Msg> {
        let Some(a) = self.busca.achados.get(self.busca.atual).copied() else { return Task::none() };
        self.pagina = a.pagina;
        self.rolar_para(a.pagina)
    }

    pub fn update_busca(&mut self, msg: Busca) -> Task<Msg> {
        match msg {
            Busca::Abrir(trocar) => {
                if self.aberto.is_none() { return Task::none(); }
                self.busca.aberta = true;
                self.busca.trocar = trocar;
                self.ler_todos_objetos();
                self.ler_campos_e_anexos();
                self.refazer_busca();
                return crate::focar(id_localizar());
            }
            Busca::Fechar => { self.busca.aberta = false; self.busca.achados.clear(); }
            Busca::Termo(t) => {
                self.busca.termo = t;
                self.refazer_busca();
                return self.ir_ao_atual();
            }
            Busca::Novo(t) => self.busca.novo = t,
            Busca::Refluxo => self.busca.refluxo = !self.busca.refluxo,
            Busca::Maiusculas(v) | Busca::Palavra(v) => {
                if matches!(msg, Busca::Maiusculas(_)) { self.busca.opcoes.maiusculas = v } else { self.busca.opcoes.palavra = v }
                self.refazer_busca();
                return self.ir_ao_atual();
            }
            Busca::Exportar => {
                if self.busca.achados.is_empty() { return Task::none(); }
                let csv = self.achados_csv();
                let nome = format!("{} — busca.csv", self.base_nome());
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().add_filter("CSV (separado por ;)", &["csv"]).set_file_name(nome).save_file().await.map(|h| h.path().to_path_buf())
                }, move |d| Msg::Busca(Busca::Gravar(csv.clone(), d)));
            }
            Busca::Gravar(_, None) => {}
            Busca::Gravar(csv, Some(destino)) => match featherpdf_io::gravar_atomico(&destino, csv.as_bytes()) {
                Ok(()) => self.avisar_salvo(format!("{} achado(s) salvos em {}", self.busca.achados.len(), destino.display()), destino),
                Err(e) => self.status = format!("Não consegui salvar: {e}"),
            },
            Busca::Proximo | Busca::Anterior => {
                let n = self.busca.achados.len();
                if n == 0 { return Task::none(); }
                self.busca.atual = if matches!(msg, Busca::Proximo) { (self.busca.atual + 1) % n } else { (self.busca.atual + n - 1) % n };
                return self.ir_ao_atual();
            }
            Busca::Trocar | Busca::TrocarTudo => {
                let alvos: Vec<(u32, usize)> = if matches!(msg, Busca::Trocar) {
                    self.busca.achados.get(self.busca.atual).map(|a| vec![(a.pagina, a.objeto)]).unwrap_or_default()
                } else {
                    let mut v: Vec<(u32, usize)> = self.busca.achados.iter().map(|a| (a.pagina, a.objeto)).collect();
                    v.dedup();
                    v
                };
                if alvos.is_empty() { return Task::none(); }
                let (termo, novo) = (self.busca.termo.clone(), self.busca.novo.clone());
                let atual = self.busca.atual;
                let refluxo = self.busca.refluxo;
                let op = self.busca.opcoes;
                let tarefa = self.editar(|pdf| {
                    if !refluxo {
                        let n = pdf.trocar_texto_com(&alvos, &termo, &novo, op)?;
                        return Ok(format!("{n} troca(s) feita(s)."));
                    }
                    // De trás para a frente: refazer um parágrafo pode tirar linhas e mudar os índices seguintes.
                    let mut alvos = alvos;
                    alvos.sort_unstable_by(|a, b| b.cmp(a));
                    let mut ultimo = String::new();
                    for (pagina, objeto) in alvos { ultimo = pdf.substituir_com_refluxo_com(pagina, objeto, &termo, &novo, op).map_err(|e| format!("Página {pagina}: {e}"))?; }
                    Ok(ultimo)
                });
                // `editar` esvaziou os objetos: lê de novo e mantém a posição na lista.
                self.ler_todos_objetos();
                self.refazer_busca();
                self.busca.atual = atual.min(self.busca.achados.len().saturating_sub(1));
                return Task::batch([tarefa, self.ir_ao_atual()]);
            }
        }
        Task::none()
    }

    /// Camada de destaques da página `n`, se houver achados nela.
    pub fn destaques(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        if !self.busca.aberta { return None; }
        let o = self.objetos.get(&n)?;
        let termo = self.busca.termo.chars().count();
        let caixas: Vec<(Rect, bool)> = self.busca.achados.iter().enumerate()
            .filter(|(_, ac)| ac.pagina == n)
            .filter_map(|(i, ac)| {
                let ob = o.lista.get(ac.objeto)?;
                let TipoObjeto::Texto { texto, .. } = &ob.tipo else { return None };
                // Posição estimada pela fração de caracteres: a linha não guarda a largura de cada letra.
                let total = texto.trim_end().chars().count().max(1) as f32;
                let [x0, y0, x1, y1] = ob.caixa;
                let (a, b) = (ac.inicio as f32 / total, ((ac.inicio + termo) as f32 / total).min(1.0));
                let caixa = [x0 + (x1 - x0) * a, y0, x0 + (x1 - x0) * b, y1];
                Some((na_tela(caixa, o.caixa, o.giro, self.zoom), i == self.busca.atual))
            }).collect();
        if caixas.is_empty() { return None; }
        Some(canvas::Canvas::new(Destaques { caixas }).width(l).height(a).into())
    }

    /// A barra flutuante (canto de cima à direita da mesa).
    pub fn barra_busca(&self) -> Element<'_, Msg> {
        let b = &self.busca;
        let muted = tema::COR_TEXTO_MUTED();
        let contagem = if b.termo.is_empty() { String::new() }
            else if b.achados.is_empty() { "nada".into() }
            else { format!("{} de {}", b.atual + 1, b.achados.len()) };
        let icone = |nome: &str, dica: &str, msg: Option<Msg>| feather_ui::componentes::botao_icone(icones::icone_traco(nome, 16.), dica, msg, false);
        let alternar = feather_ui::componentes::botao_icone(icones::icone_traco("edit", 16.),
            if b.trocar { "Só localizar (Ctrl+F)" } else { "Trocar (Ctrl+H)" }, Some(Msg::Busca(Busca::Abrir(!b.trocar))), b.trocar);
        let tem = !b.achados.is_empty();
        let mut corpo = column![row![
            text_input("Localizar no documento", &b.termo).id(id_localizar())
                .on_input(|t| Msg::Busca(Busca::Termo(t))).on_submit(Msg::Busca(Busca::Proximo))
                .width(220).padding([5, 8]).size(13),
            text(contagem).size(12).color(muted).width(64).align_x(Align::Center),
            icone("chevron-up", "Anterior (Shift+Enter)", tem.then_some(Msg::Busca(Busca::Anterior))),
            icone("chevron-down", "Próximo (Enter)", tem.then_some(Msg::Busca(Busca::Proximo))),
            alternar,
            icone("x", "Fechar (Esc)", Some(Msg::Busca(Busca::Fechar))),
        ].spacing(4).align_y(Align::Center)].spacing(6);
        corpo = corpo.push(row![
            checkbox("Maiúsculas/minúsculas", b.opcoes.maiusculas).on_toggle(|v| Msg::Busca(Busca::Maiusculas(v))).size(14).text_size(12),
            checkbox("Palavra inteira", b.opcoes.palavra).on_toggle(|v| Msg::Busca(Busca::Palavra(v))).size(14).text_size(12),
            Space::with_width(Length::FILL),
            icone("file-export", "Salvar os achados em CSV (página e linha)", tem.then_some(Msg::Busca(Busca::Exportar))),
        ].spacing(10).align_y(Align::Center));
        if !b.outros.is_empty() {
            let mut outros = column![text(format!("Também em {} campo(s) ou anexo(s):", b.outros.len())).size(11.5).color(muted)].spacing(2);
            for (rotulo, pagina) in b.outros.iter().take(8) {
                let t = text(rotulo.clone()).size(12);
                outros = outros.push(match pagina {
                    Some(p) => Element::from(button(row![t, text(format!("p. {p}")).size(11).color(muted)].spacing(6)).padding([2, 6])
                        .style(tema::estilo_botao_pequeno(false)).on_press(Msg::IrPagina(*p))),
                    None => t.into(),
                });
            }
            if b.outros.len() > 8 { outros = outros.push(text(format!("e mais {}.", b.outros.len() - 8)).size(11.5).color(muted)); }
            corpo = corpo.push(outros);
        }
        if b.trocar {
            let botao = |r: &'static str, msg: Option<Msg>| button(text(r).size(12)).padding([5, 10])
                .style(tema::estilo_botao_pequeno(false)).on_press_maybe(msg);
            corpo = corpo.push(row![
                text_input("Trocar por", &b.novo).id(id_trocar())
                    .on_input(|t| Msg::Busca(Busca::Novo(t))).on_submit(Msg::Busca(Busca::Trocar))
                    .width(220).padding([5, 8]).size(13),
                botao("Trocar", tem.then_some(Msg::Busca(Busca::Trocar))),
                botao("Trocar tudo", tem.then_some(Msg::Busca(Busca::TrocarTudo))),
                checkbox("Refazer o parágrafo", b.refluxo).on_toggle(|_| Msg::Busca(Busca::Refluxo)).size(14).text_size(12),
            ].spacing(6).align_y(Align::Center));
        }
        container(corpo).padding([8, 10]).style(|_| Style {
            background: Some(Background::Color(tema::COR_PAINEL())),
            border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [10.0; 4] },
            shadow: Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.18), offset: Point::new(0.0, 3.0), blur: 12.0 }),
            ..Default::default()
        }).into()
    }
}

/// Texto do PDF numa célula de CSV: entre aspas, e com apóstrofo na frente quando começaria
/// por = + - @ (a planilha leria como fórmula vinda do arquivo).
fn celula_csv(t: &str) -> String {
    let t: String = t.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    let t = if t.starts_with(['=', '+', '-', '@']) { format!("'{t}") } else { t };
    format!("\"{}\"", t.replace('"', "\"\""))
}

#[cfg(test)]
mod testes {
    #[test]
    fn celula_csv_protege_formula_e_aspas() {
        assert_eq!(super::celula_csv("=SOMA(A1)"), "\"'=SOMA(A1)\"");
        assert_eq!(super::celula_csv("diz \"oi\";\nfim"), "\"diz \"\"oi\"\";fim\"");
    }
}
