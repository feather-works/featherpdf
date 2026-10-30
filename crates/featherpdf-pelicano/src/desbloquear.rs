//! PDF que pede senha para abrir: a caixa da senha por cima da tela, e de novo se ela estiver errada.
//! Aberto, o documento fica com a proteção (volta ao salvar) até "Tirar a senha" em Proteger → Senha.

use std::path::PathBuf;

use feather_ui::dialogo::Dialogo;
use feather_ui::tema;

use pelicano::*;
use pelicano::column;
use crate::{App, Msg};

#[derive(Debug, Clone)]
pub enum Sb {
    /// O arquivo e o erro da tentativa anterior (`None` na primeira).
    Pedir(PathBuf, Option<String>),
    Texto(String),
    Abrir,
    Fechar,
}

#[derive(Debug, Clone, Default)]
pub struct PedidoSenha {
    caminho: PathBuf,
    senha: String,
    erro: Option<String>,
}

fn id_campo() -> &'static str { "senha-abrir" }

impl App {
    pub fn update_desbloquear(&mut self, m: Sb) -> Task<Msg> {
        match m {
            Sb::Pedir(caminho, erro) => {
                self.status = String::new();
                self.pedido_senha = Some(PedidoSenha { caminho, senha: String::new(), erro });
                return crate::focar(id_campo());
            }
            Sb::Texto(t) => if let Some(p) = &mut self.pedido_senha { p.senha = t },
            Sb::Abrir => if let Some(p) = self.pedido_senha.take().filter(|p| !p.senha.is_empty()) {
                self.status = format!("Abrindo {}…", p.caminho.display());
                return Self::abrir_caminho(p.caminho, Some(p.senha));
            },
            Sb::Fechar => self.pedido_senha = None,
        }
        Task::none()
    }

    pub fn com_pedido_senha<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(p) = &self.pedido_senha else { return crate::inicio::sem_camada(base) };
        let nome = p.caminho.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let mut corpo = column![
            text(format!("“{nome}” é protegido. Digite a senha de abrir (ou a do dono).")).size(13),
            text_input("Senha", &p.senha).secure(true).id(id_campo()).on_input(|t| Msg::Senha(Sb::Texto(t)))
                .on_submit(Msg::Senha(Sb::Abrir)).padding([7, 10]).size(13),
        ].spacing(10);
        if let Some(e) = &p.erro { corpo = corpo.push(text(e.clone()).size(12).color(tema::COR_ERRO())); }
        corpo = corpo.push(text("Ao salvar, o arquivo continua com a mesma senha. Para tirá-la: Proteger → Senha.").size(12).color(tema::COR_TEXTO_MUTED()));
        Dialogo::new("Senha do PDF", corpo).icone("lock").largura(440.0)
            .secundario("Cancelar", Some(Msg::Senha(Sb::Fechar)))
            .primario("Abrir", (!p.senha.is_empty()).then_some(Msg::Senha(Sb::Abrir)))
            .ao_fechar(Msg::Senha(Sb::Fechar)).topo(160.0)
            .sobre(base)
    }
}
