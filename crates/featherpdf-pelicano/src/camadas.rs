//! Painel Camadas: ligar e desligar (como antes), renomear, travar, criar camada nova e dizer
//! quando ela aparece (só na tela, só na impressão, numa faixa de zoom).
//! Tudo vale para o PDF salvo, em qualquer leitor; cada mudança é um Ctrl+Z.

use feather_ui::dialogo as dlg;
use feather_ui::{icones as ic, tema};
use featherpdf_core::Visibilidade;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::comentar::Cm;
use crate::{App, Msg};

#[derive(Debug, Clone, Default)]
pub struct EstadoCamadas {
    /// Camada sendo renomeada e o nome digitado.
    pub renomeando: Option<((u32, u16), String)>,
    pub nova: String,
}

#[derive(Debug, Clone)]
pub enum Ca {
    Renomear((u32, u16), String),
    Nome(String),
    ConfirmarNome,
    CancelarNome,
    Travar((u32, u16), bool),
    Uso((u32, u16), Uso),
    Nova(String),
    Criar,
}

/// Visibilidade com o nome que aparece na lista.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Uso(Visibilidade);

impl Uso {
    const OPCOES: [Uso; 5] = [Uso(Visibilidade::Sempre), Uso(Visibilidade::SoTela), Uso(Visibilidade::SoImpressao),
        Uso(Visibilidade::Zoom { min: 150.0, max: 0.0 }), Uso(Visibilidade::Zoom { min: 0.0, max: 100.0 })];
}

impl std::fmt::Display for Uso {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Visibilidade::Sempre => write!(f, "Sempre"),
            Visibilidade::SoTela => write!(f, "Só na tela"),
            Visibilidade::SoImpressao => write!(f, "Só na impressão"),
            Visibilidade::Zoom { min, max } if max <= 0.0 => write!(f, "Com zoom de {min:.0}% ou mais"),
            Visibilidade::Zoom { min, max } if min <= 0.0 => write!(f, "Até {max:.0}% de zoom"),
            Visibilidade::Zoom { min, max } => write!(f, "Zoom de {min:.0}% a {max:.0}%"),
        }
    }
}

pub fn id_nome() -> &'static str { "camada-nome" }

impl App {
    pub fn update_camadas(&mut self, m: Ca) -> Task<Msg> {
        let e = &mut self.camadas;
        match m {
            Ca::Renomear(id, atual) => { e.renomeando = Some((id, atual)); return crate::focar(id_nome()); }
            Ca::Nome(t) => if let Some((_, n)) = &mut e.renomeando { *n = t },
            Ca::CancelarNome => e.renomeando = None,
            Ca::ConfirmarNome => {
                let Some((id, nome)) = e.renomeando.take() else { return Task::none() };
                return self.editar(move |pdf| pdf.renomear_camada(id, &nome).map(|_| "Camada renomeada. Ctrl+Z desfaz.".into()));
            }
            Ca::Travar(id, v) => return self.editar(move |pdf| pdf.travar_camada(id, v)
                .map(|_| if v { "Camada travada: o leitor não deixa ligar nem desligar.".into() } else { "Camada destravada.".into() })),
            Ca::Uso(id, u) => return self.editar(move |pdf| pdf.visibilidade_camada(id, u.0).map(|_| format!("Camada: {u}. Vale no Acrobat e nos leitores que seguem o PDF; Ctrl+Z desfaz."))),
            Ca::Nova(t) => e.nova = t,
            Ca::Criar => {
                let nome = std::mem::take(&mut e.nova);
                return self.editar(move |pdf| pdf.criar_camada(&nome).map(|_| format!("Camada \"{}\" criada. Ctrl+Z desfaz.", nome.trim())));
            }
        }
        Task::none()
    }

    pub fn painel_camadas(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let camadas = self.aberto.as_ref().map(|a| a.pdf.camadas()).unwrap_or_default();
        let mut col = column![].spacing(8);
        if camadas.is_empty() {
            col = col.push(dlg::aviso_vazio("Este PDF não tem camadas (conteúdo opcional)."));
        } else {
            col = col.push(text("Desmarcar esconde a camada na tela e no PDF salvo. Travada: o leitor não deixa mudar. Ctrl+Z desfaz.").size(12).color(muted));
        }
        for c in camadas {
            let id = c.id;
            match &self.camadas.renomeando {
                Some((r, nome)) if *r == id => {
                    col = col.push(row![
                        text_input("Nome da camada", nome).id(id_nome()).on_input(|t| Msg::Cam(Ca::Nome(t)))
                            .on_submit(Msg::Cam(Ca::ConfirmarNome)).padding([4, 8]).size(13),
                        button(ic::icone("check", 13.0)).padding(4).style(tema::estilo_botao_pequeno(false))
                            .on_press_maybe((!nome.trim().is_empty()).then_some(Msg::Cam(Ca::ConfirmarNome))),
                        button(ic::icone("x", 13.0)).padding(4).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Cam(Ca::CancelarNome)),
                    ].spacing(4).align_y(Align::Center));
                }
                _ => {
                    let marcar = checkbox(c.nome.clone(), c.visivel).size(16).text_size(13);
                    let marcar = if c.travada { marcar } else { marcar.on_toggle(move |v| Msg::Com(Cm::Camada(id, v))) };
                    col = col.push(row![
                        marcar, Space::with_width(Length::FILL),
                        button(ic::icone("edit", 13.0)).padding(4).style(tema::estilo_botao_pequeno(false))
                            .on_press(Msg::Cam(Ca::Renomear(id, c.nome.clone()))),
                        button(ic::icone("lock", 13.0)).padding(4).style(tema::estilo_botao_pequeno(c.travada))
                            .on_press(Msg::Cam(Ca::Travar(id, !c.travada))),
                    ].spacing(4).align_y(Align::Center));
                    let mut opcoes = Uso::OPCOES.to_vec();
                    if !opcoes.contains(&Uso(c.uso)) { opcoes.push(Uso(c.uso)); }
                    col = col.push(row![Space::with_width(24), text("Aparece").size(11.5).color(muted),
                        suspensa(opcoes, Some(Uso(c.uso)), move |u| Msg::Cam(Ca::Uso(id, u))).text_size(12).padding([2, 6]).width(Length::FILL)]
                        .spacing(6).align_y(Align::Center));
                }
            }
        }
        let nova_ok = !self.camadas.nova.trim().is_empty();
        col = col.push(dlg::secao("Nova camada"))
            .push(row![
                text_input("Nome (ex.: Revisão)", &self.camadas.nova).on_input(|t| Msg::Cam(Ca::Nova(t)))
                    .on_submit_maybe(nova_ok.then_some(Msg::Cam(Ca::Criar))).padding([4, 8]).size(13),
                button(text("Criar").size(12)).padding([4, 10]).style(tema::estilo_botao_pequeno(false)).on_press_maybe(nova_ok.then_some(Msg::Cam(Ca::Criar))),
            ].spacing(4).align_y(Align::Center))
            .push(text("A marca d'água pode ir numa camada própria (aba Páginas → Marca d'água).").size(11.5).color(muted));
        scrollable(col).height(Length::FILL).into()
    }
}
