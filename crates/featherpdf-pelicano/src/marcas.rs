//! Painel "Cabeçalho, rodapé e marca d'água" (aba Páginas): marca d'água de texto
//! translúcida e girada, e cabeçalho/rodapé com texto livre e campos. O motor está em
//! `featherpdf_core::marcas`; cada aplicação é um passo de Ctrl+Z.

use feather_ui::dialogo as dlg;
use feather_ui::tema;
use featherpdf_core::{paginas_do_texto, CabecalhoRodape, MarcaDagua};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

const NEGRITO: Font = Font::SEMIBOLD;

/// Cores oferecidas (nome, RGB de 0 a 1).
pub const CORES: [(&str, [f32; 3]); 5] = [
    ("Cinza", [0.55, 0.55, 0.55]), ("Vermelho", [0.80, 0.12, 0.12]), ("Azul", [0.12, 0.30, 0.75]),
    ("Verde", [0.13, 0.50, 0.25]), ("Preto", [0.0, 0.0, 0.0]),
];
const CAMPOS: [(&str, &str); 4] = [("Nº da página", "{pagina}"), ("Total", "{total}"), ("Data", "{data}"), ("Nome do arquivo", "{arquivo}")];
const LUGARES: [&str; 6] = ["Cabeçalho à esquerda", "Cabeçalho ao centro", "Cabeçalho à direita", "Rodapé à esquerda", "Rodapé ao centro", "Rodapé à direita"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modo { #[default] Marca, Cabecalho }

#[derive(Debug, Clone)]
pub struct Marcas {
    pub modo: Modo,
    pub texto: String,
    pub tamanho: String,
    pub cor: usize,
    pub opacidade: f32,
    pub angulo: f32,
    /// Lugares do cabeçalho (0–2) e do rodapé (3–5).
    pub lugares: [String; 6],
    /// Último lugar digitado: é nele que o botão de campo escreve.
    pub ultimo: usize,
    pub tamanho_cr: String,
    pub cor_cr: usize,
    /// Intervalo de páginas ("" = todas).
    pub paginas: String,
    /// Marca d'água numa camada própria, que dá para esconder.
    pub em_camada: bool,
}

impl Default for Marcas {
    fn default() -> Self {
        Self { modo: Modo::Marca, texto: "CONFIDENCIAL".into(), tamanho: "72".into(), cor: 1, opacidade: 0.25, angulo: 45.0,
            lugares: Default::default(), ultimo: 4, tamanho_cr: "9".into(), cor_cr: 4, paginas: String::new(), em_camada: true }
    }
}

#[derive(Debug, Clone)]
pub enum Mc {
    Abrir(Modo),
    Modo(Modo),
    Texto(String),
    Tamanho(String),
    Cor(usize),
    Opacidade(f32),
    Angulo(f32),
    Lugar(usize, String),
    Campo(&'static str),
    TamanhoCr(String),
    CorCr(usize),
    Paginas(String),
    EmCamada(bool),
    Aplicar,
}

/// Botão de escolha: o ativo com borda e texto na cor de destaque (o mesmo do Imprimir).
pub(crate) fn escolha<'a>(rotulo: impl Into<String>, ativo: bool, msg: Msg) -> Element<'a, Msg> {
    button(text(rotulo.into()).size(12.5).center().font(if ativo { NEGRITO } else { Font::DEFAULT })).height(32).padding([0, 10])
        .style(move |_, s| Style {
            background: Some(Background::Color(if ativo { tema::COR_AZUL_FUNDO() } else if s == Status::Hovered { tema::COR_HOVER() } else { tema::COR_PAINEL() })),
            text_color: Some(if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_TEXTO() }),
            border: Border { color: if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_BORDA() }, width: if ativo { 2.0 } else { 1.0 }, radius: [8.0; 4] },
            ..Default::default()
        }).on_press(msg).into()
}

/// Amostra de cor clicável (a escolhida com anel de destaque).
fn amostra<'a>(rgb: [f32; 3], ativo: bool, msg: Msg) -> Element<'a, Msg> {
    let c = Color::rgb(rgb[0], rgb[1], rgb[2]);
    button(Space::new(18, 18)).padding(0).width(24).height(24)
        .style(move |_, _| Style {
            background: Some(Background::Color(c)),
            border: Border { color: if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_BORDA() }, width: if ativo { 3.0 } else { 1.0 }, radius: [12.0; 4] },
            ..Default::default()
        }).on_press(msg).into()
}

fn numero(t: &str) -> Option<f32> { t.trim().replace(',', ".").parse().ok() }

impl App {
    pub fn update_marcas(&mut self, m: Mc) -> Task<Msg> {
        let total = self.total_paginas();
        let e = &mut self.marcas;
        match m {
            Mc::Abrir(modo) => { e.modo = modo; self.painel = Some(Painel::Marcas); }
            Mc::Modo(modo) => e.modo = modo,
            Mc::Texto(t) => e.texto = t,
            Mc::Tamanho(t) => e.tamanho = t,
            Mc::Cor(i) => e.cor = i,
            Mc::Opacidade(v) => e.opacidade = v,
            Mc::Angulo(v) => e.angulo = v,
            Mc::Lugar(i, t) => { e.lugares[i] = t; e.ultimo = i; }
            Mc::Campo(c) => e.lugares[e.ultimo].push_str(c),
            Mc::TamanhoCr(t) => e.tamanho_cr = t,
            Mc::CorCr(i) => e.cor_cr = i,
            Mc::Paginas(t) => e.paginas = t,
            Mc::EmCamada(v) => e.em_camada = v,
            Mc::Aplicar => {
                let paginas = match paginas_do_texto(&e.paginas, total) {
                    Ok(p) => p,
                    Err(erro) => { self.status = erro; return Task::none(); }
                };
                let e = self.marcas.clone();
                return match e.modo {
                    Modo::Marca => {
                        let m = MarcaDagua { texto: e.texto.clone(), tamanho: numero(&e.tamanho).unwrap_or(0.0), cor: CORES[e.cor].1,
                            opacidade: e.opacidade, angulo: e.angulo, paginas, em_camada: e.em_camada };
                        self.editar(move |pdf| pdf.marca_dagua(&m).map(|n| format!("Marca d'água em {n} página(s). Ctrl+Z desfaz.")))
                    }
                    Modo::Cabecalho => {
                        let l = &e.lugares;
                        let op = CabecalhoRodape {
                            cabecalho: [l[0].clone(), l[1].clone(), l[2].clone()], rodape: [l[3].clone(), l[4].clone(), l[5].clone()],
                            tamanho: numero(&e.tamanho_cr).unwrap_or(0.0), margem: 28.0, cor: CORES[e.cor_cr].1, paginas,
                            arquivo: self.nome_documento().unwrap_or_default(), data: featherpdf_core::hoje(),
                        };
                        self.editar(move |pdf| pdf.cabecalho_rodape(&op).map(|n| format!("Cabeçalho e rodapé em {n} página(s). Ctrl+Z desfaz.")))
                    }
                };
            }
        }
        Task::none()
    }

    pub fn painel_marcas(&self) -> Element<'_, Msg> {
        let e = &self.marcas;
        let muted = tema::COR_TEXTO_MUTED();
        let total = self.total_paginas();
        let mc = |m: Mc| Msg::Marca(m);
        let mut corpo = column![row![
            escolha("Marca d'água", e.modo == Modo::Marca, mc(Mc::Modo(Modo::Marca))),
            escolha("Cabeçalho e rodapé", e.modo == Modo::Cabecalho, mc(Mc::Modo(Modo::Cabecalho))),
        ].spacing(6)].spacing(10);
        let campo_num = |rotulo: &'static str, valor: &str, f: fn(String) -> Mc| -> Element<'_, Msg> {
            row![text(rotulo).size(13).width(Length::FILL), text_input("", valor.to_owned()).on_input(move |t| Msg::Marca(f(t))).width(70).padding([4, 8]).size(13)]
                .align_y(Align::Center).into()
        };
        let (pronto, previa): (bool, Element<'_, Msg>) = match e.modo {
            Modo::Marca => {
                let tamanho_ok = numero(&e.tamanho).is_some_and(|t| (4.0..=400.0).contains(&t));
                corpo = corpo.push(dlg::secao("Texto"))
                    .push(text_input("Ex.: CONFIDENCIAL, RASCUNHO, CÓPIA", &e.texto).on_input(|t| Msg::Marca(Mc::Texto(t))).padding([6, 8]).size(13))
                    .push(campo_num("Tamanho (pt)", &e.tamanho, Mc::Tamanho))
                    .push(dlg::secao("Cor"))
                    .push(row(CORES.iter().enumerate().map(|(i, (_, c))| amostra(*c, i == e.cor, mc(Mc::Cor(i))))).spacing(8))
                    .push(row![text("Opacidade").size(13).width(Length::FILL), text(format!("{:.0}%", e.opacidade * 100.0)).size(13)])
                    .push(slider(0.05..=1.0, e.opacidade, |v| Msg::Marca(Mc::Opacidade(v))).step(0.05f32))
                    .push(row![text("Ângulo").size(13).width(Length::FILL), text(format!("{:.0}°", e.angulo)).size(13)])
                    .push(slider(-90.0..=90.0, e.angulo, |v| Msg::Marca(Mc::Angulo(v))).step(5.0f32))
                    .push([0.0f32, 45.0, 90.0].into_iter().map(|a| escolha(format!("{a:.0}°"), e.angulo == a, mc(Mc::Angulo(a))))
                        .fold(row![].spacing(6), |r, b| r.push(b)))
                    .push(checkbox("Em camada própria (dá para esconder no painel Camadas)", e.em_camada)
                        .on_toggle(|v| Msg::Marca(Mc::EmCamada(v))).size(15).text_size(12.5));
                let [r, g, b] = CORES[e.cor].1;
                let amostra_txt = text(e.texto.clone()).size(20).font(NEGRITO).color(Color::rgba(r, g, b, e.opacidade.max(0.15)));
                (tamanho_ok && !e.texto.trim().is_empty(),
                    container(amostra_txt).width(Length::FILL).height(64).center_x(Length::FILL).center_y(64).style(dlg::estilo_bloco).into())
            }
            Modo::Cabecalho => {
                corpo = corpo.push(dlg::secao("Textos"));
                for (i, lugar) in LUGARES.iter().enumerate() {
                    corpo = corpo.push(column![
                        text(*lugar).size(12).color(if i == e.ultimo { tema::COR_AZUL_ACENTO() } else { muted }),
                        text_input("", &e.lugares[i]).on_input(move |t| Msg::Marca(Mc::Lugar(i, t))).padding([5, 8]).size(13),
                    ].spacing(3));
                }
                let mut campos = row![].spacing(4);
                for (rotulo, c) in CAMPOS { campos = campos.push(button(text(rotulo).size(11.5)).padding([3, 7]).style(tema::estilo_botao_pequeno(false)).on_press(mc(Mc::Campo(c)))); }
                corpo = corpo.push(text(format!("Inserir campo em \"{}\":", LUGARES[e.ultimo])).size(12).color(muted))
                    .push(scrollable(campos).direction(Direction::Horizontal))
                    .push(campo_num("Tamanho (pt)", &e.tamanho_cr, Mc::TamanhoCr))
                    .push(dlg::secao("Cor"))
                    .push(row(CORES.iter().enumerate().map(|(i, (_, c))| amostra(*c, i == e.cor_cr, mc(Mc::CorCr(i))))).spacing(8));
                let exemplo = featherpdf_core::expandir_campos(&e.lugares[4], 1, total, &featherpdf_core::hoje(), &self.nome_documento().unwrap_or_default());
                let tamanho_ok = numero(&e.tamanho_cr).is_some_and(|t| (4.0..=72.0).contains(&t));
                (tamanho_ok && e.lugares.iter().any(|l| !l.trim().is_empty()),
                    text(if exemplo.trim().is_empty() { String::new() } else { format!("Rodapé da página 1: {exemplo}") }).size(12).color(muted).into())
            }
        };
        let paginas_ok = paginas_do_texto(&e.paginas, total).is_ok();
        corpo = corpo.push(dlg::secao("Páginas"))
            .push(text_input(format!("Todas (ou 1-3, 5…  de 1 a {total})"), &e.paginas).on_input(|t| Msg::Marca(Mc::Paginas(t))).padding([5, 8]).size(13));
        if !paginas_ok { corpo = corpo.push(dlg::aviso("alert-triangle", format!("Use intervalos de 1 a {total}, como 1-3, 5."))); }
        corpo = corpo.push(previa)
            .push(text("Entra como texto da página, por cima do conteúdo, em Helvetica. Ctrl+Z desfaz.").size(12).color(muted))
            .push(row![Space::with_width(Length::FILL), dlg::botao_primario("Aplicar", (pronto && paginas_ok).then_some(mc(Mc::Aplicar)))]);
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
