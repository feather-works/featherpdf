//! Menu do botão direito: na página (vista do documento, miniaturas, Organizar) e no objeto em edição.
//! Só entram ações que já existem; cada item manda a mesma mensagem da faixa.

use feather_ui::{icones as ic, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::busca::Busca;
use crate::edicao::Obj;
use crate::exportar::{Ex, Formato};
use crate::ocr::Oc;
use crate::organizar::Pag;
use crate::{App, Aba, Msg};

const LARGURA: f32 = 240.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alvo {
    Pagina(u32),
    Objeto { pagina: u32, objeto: usize },
}

#[derive(Debug, Clone)]
pub enum Ct {
    Abrir(Alvo),
    Fechar,
    /// Fecha o menu e repassa a ação.
    Acao(Box<Msg>),
    CopiarTexto(u32),
    /// Abre a busca com o texto selecionado.
    Buscar(String),
    Submenu(bool),
}

fn acao(m: Msg) -> Msg { Msg::Ctx(Ct::Acao(Box::new(m))) }

impl App {
    pub fn update_contexto(&mut self, m: Ct) -> Task<Msg> {
        match m {
            Ct::Abrir(alvo) => {
                let mut tarefa = Task::none();
                match alvo {
                    // As ações de página agem sobre `alvo()`: a seleção do Organizar ou a página atual.
                    Alvo::Pagina(n) if self.aba == Aba::Paginas => {
                        if !self.org.selecao.contains(&n) { self.org.selecao = [n].into(); }
                    }
                    Alvo::Pagina(n) => self.pagina = n,
                    Alvo::Objeto { pagina, objeto } => tarefa = self.update(Msg::Obj(Obj::Escolher { pagina, objeto })),
                }
                self.contexto = Some((alvo, crate::inserir::ponteiro()));
                self.contexto_sub = false;
                return tarefa;
            }
            Ct::Fechar => self.contexto = None,
            Ct::Submenu(v) => self.contexto_sub = v,
            Ct::Buscar(t) => {
                self.contexto = None;
                let abrir = self.update(Msg::Busca(Busca::Abrir(false)));
                return Task::batch([abrir, self.update(Msg::Busca(Busca::Termo(t)))]);
            }
            Ct::Acao(m) => {
                self.contexto = None;
                return self.update(*m);
            }
            Ct::CopiarTexto(n) => {
                let Some(a) = &self.aberto else { return Task::none() };
                match a.pdf.texto_pagina(n) {
                    Ok(t) if !t.trim().is_empty() => {
                        self.status = format!("Texto da página {n} copiado.");
                        return Task::write_clipboard(t);
                    }
                    _ => self.status = format!("A página {n} não tem texto. Reconhecer texto (OCR) cria a camada de texto."),
                }
            }
        }
        Task::none()
    }

    /// Itens de página: os mesmos no menu da página e no submenu "Página N ›" da seleção.
    fn itens_pagina(&self, n: u32) -> Vec<Option<Item>> {
        let k = self.alvo().len();
        let pags = if k > 1 { format!("{k} páginas") } else { "página".into() };
        vec![
            Some(Item::new("copy", format!("Copiar texto da página {n}"), Msg::Ctx(Ct::CopiarTexto(n)))),
            None,
            Some(Item::new("refresh", "Girar à direita".into(), acao(Msg::GirarDireita))),
            Some(Item::new("refresh", "Girar à esquerda".into(), acao(Msg::GirarEsquerda))),
            None,
            Some(Item::new("file-plus", "Inserir página em branco depois".into(), acao(Msg::Pag(Pag::InserirBranco)))),
            Some(Item::new("copy", format!("Duplicar {pags}"), acao(Msg::Pag(Pag::Duplicar)))),
            Some(Item::new("file-export", format!("Extrair {pags}…"), acao(Msg::Pag(Pag::Extrair)))),
            Some(Item { perigo: true, ..Item::new("trash", format!("Excluir {pags}"), acao(Msg::Excluir)) }),
            None,
            Some(Item::new("screenshot", "Reconhecer texto (OCR)…".into(), acao(Msg::Ocr(Oc::Abrir)))),
            Some(Item::new("photo", "Exportar como imagem…".into(), acao(Msg::Exp(Ex::Abrir(Formato::Imagens))))),
        ]
    }

    pub fn com_menu_contexto<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some((alvo, (x, y))) = self.contexto else { return crate::inicio::sem_camada(base) };
        let selecao = match alvo {
            Alvo::Pagina(n) => self.selecionado().filter(|s| s.0 == n),
            Alvo::Objeto { .. } => None,
        };
        let mut sub_y = None;
        let (menu, altura) = match (alvo, &selecao) {
            // Com texto selecionado: o que fazer com o trecho, e a página num submenu.
            (Alvo::Pagina(n), Some((_, texto, _))) => {
                use crate::selecao::Sel;
                use featherpdf_core::Marcacao;
                let letras = texto.chars().count();
                let mut trecho: String = texto.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(30).collect();
                if letras > 30 { trecho.push('…'); }
                let bloco = |icone: &'static str, rotulo: &'static str, m: Marcacao| button(column![ic::icone(icone, 18.0), text(rotulo).size(11)]
                    .spacing(4).align_x(Align::Center)).width(Length::FILL).height(52).padding(0)
                    .style(tema::estilo_item_menu(false, true)).on_press(acao(Msg::Selecao(Sel::Marcar(m))));
                let tarja = container(Space::new(16, 10)).style(|_| Style { background: Some(Background::Color(Color::rgb8(0x20, 0x21, 0x24))),
                    border: Border { radius: [2.0; 4], ..Border::default() }, ..Default::default() });
                let c = column![
                    container(text(format!("“{trecho}” · {letras} caractere{}", if letras == 1 { "" } else { "s" })).size(11).color(tema::COR_TEXTO_MUTED())).padding([6, 10]),
                    Item { atalho: "Ctrl+C", ..Item::new("copy", "Copiar".into(), acao(Msg::Selecao(Sel::Copiar))) }.view(),
                    separador(),
                    row![bloco("highlight", "Destacar", Marcacao::Destacar), bloco("underline", "Sublinhar", Marcacao::Sublinhar), bloco("strikethrough", "Riscar", Marcacao::Riscar),
                        bloco("underline-double", "Ondulado", Marcacao::Ondulado)]
                        .spacing(4).padding([2, 4]),
                    separador(),
                    linha(tarja.into(), "Marcar para tarjar".into(), "", false, acao(Msg::Selecao(Sel::Tarjar))),
                    Item::new("search", "Buscar no documento".into(), Msg::Ctx(Ct::Buscar(texto.clone()))).view(),
                    separador(),
                    mouse_area(button(row![Space::with_width(16), text(format!("Página {n}")).size(13).width(Length::FILL), text("›").size(14)]
                        .spacing(10).align_y(Align::Center)).width(Length::FILL).height(34).padding([0, 10])
                        .style(tema::estilo_item_menu(self.contexto_sub, true)).on_press(Msg::Ctx(Ct::Submenu(!self.contexto_sub))))
                        .on_enter(Msg::Ctx(Ct::Submenu(true))),
                ].spacing(1);
                sub_y = Some(272.0);
                (c, 312.0)
            }
            (Alvo::Pagina(n), None) => lista(self.itens_pagina(n)),
            (Alvo::Objeto { pagina, objeto }, _) => {
                let texto = self.objetos.get(&pagina).and_then(|o| o.lista.get(objeto))
                    .is_some_and(|ob| matches!(ob.tipo, featherpdf_core::TipoObjeto::Texto { .. }));
                let mut v = vec![];
                if texto { v.push(Some(Item::new("pencil", "Editar texto".into(), acao(Msg::Obj(Obj::Editar { pagina, objeto }))))); }
                v.push(Some(Item::new("arrow-up", "Trazer para a frente".into(), acao(Msg::Obj(Obj::Ordem(true))))));
                v.push(Some(Item::new("arrow-down", "Mandar para trás".into(), acao(Msg::Obj(Obj::Ordem(false))))));
                v.push(None);
                v.push(Some(Item { perigo: true, ..Item::new("trash", "Excluir".into(), acao(Msg::Obj(Obj::Excluir))) }));
                lista(v)
            }
        };
        let largura = if selecao.is_some() { 264.0 } else { LARGURA };
        // Perto da borda da janela o menu abre para dentro, como nos outros programas.
        let (jl, ja) = crate::inserir::janela();
        let x = if jl > 0.0 && x + largura + 16.0 > jl { (x - largura - 12.0).max(4.0) } else { x };
        let y = if ja > 0.0 && y + altura + 16.0 > ja { (y - altura - 12.0).max(4.0) } else { y };
        let mut camadas = Stack::new().width(Length::FILL).height(Length::FILL)
            .push(base)
            .push(mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::Ctx(Ct::Fechar)).on_right_press(Msg::Ctx(Ct::Fechar)))
            .push(column![Space::with_height(y), row![Space::with_width(x), opaque(container(menu.width(largura)).padding(6).style(estilo_menu))]]);
        if let (Some(dy), true, Alvo::Pagina(n)) = (sub_y, self.contexto_sub, alvo) {
            let (sub, alt) = lista(self.itens_pagina(n));
            let sx = if jl > 0.0 && x + largura + LARGURA + 24.0 > jl { (x - LARGURA - 4.0).max(4.0) } else { x + largura + 4.0 };
            let sy = if ja > 0.0 && y + dy + alt + 16.0 > ja { (ja - alt - 16.0).max(4.0) } else { y + dy };
            camadas = camadas.push(column![Space::with_height(sy), row![Space::with_width(sx), opaque(container(sub.width(LARGURA)).padding(6).style(estilo_menu))]]);
        }
        camadas.into()
    }
}

struct Item {
    icone: &'static str,
    rotulo: String,
    atalho: &'static str,
    perigo: bool,
    msg: Msg,
}

impl Item {
    fn new(icone: &'static str, rotulo: String, msg: Msg) -> Self { Self { icone, rotulo, atalho: "", perigo: false, msg } }

    fn view(self) -> Element<'static, Msg> {
        let cor = if self.perigo { Color::rgb8(0xb3, 0x26, 0x1e) } else { tema::COR_TEXTO() };
        let icone = ic::icone_traco(self.icone, 15.0).color(cor);
        linha(icone.into(), self.rotulo, self.atalho, self.perigo, self.msg)
    }
}

fn linha(icone: Element<'static, Msg>, rotulo: String, atalho: &'static str, perigo: bool, msg: Msg) -> Element<'static, Msg> {
    let cor = if perigo { Color::rgb8(0xb3, 0x26, 0x1e) } else { tema::COR_TEXTO() };
    button(row![container(icone).width(16).center_x(16), text(rotulo).size(13).color(cor).width(Length::FILL), text(atalho).size(11.5).color(tema::COR_TEXTO_MUTED())]
        .spacing(10).align_y(Align::Center)).width(Length::FILL).height(32).padding([0, 10])
        .style(tema::estilo_item_menu(false, true)).on_press(msg).into()
}

fn separador() -> Element<'static, Msg> {
    container(container(Space::new(Length::FILL, 1)).style(|_| Style {
        background: Some(Background::Color(tema::COR_BORDA_SUAVE())), ..Default::default()
    })).padding([5, 4]).into()
}

/// Lista simples de itens (`None` = separador) e a altura que ocupa.
fn lista(itens: Vec<Option<Item>>) -> (Column<'static, Msg>, f32) {
    let mut c = column![].spacing(1);
    let mut altura = 12.0;
    for i in itens {
        altura += if i.is_some() { 33.0 } else { 11.0 };
        c = c.push(match i { Some(i) => i.view(), None => separador() });
    }
    (c, altura)
}

fn estilo_menu(t: &Theme) -> Style {
    let mut e = tema::estilo_popup_menu(t);
    e.border = Border { color: tema::COR_BORDA(), width: 1.0, radius: [10.0; 4] };
    e
}
