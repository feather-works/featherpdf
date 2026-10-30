//! Painel "Limpar e sanitizar" (aba Proteger): escolhe o que sai do arquivo antes de
//! mandá-lo para fora e mostra o relatório do que foi tirado. Um passo de Ctrl+Z.

use feather_ui::dialogo as dlg;
use feather_ui::tema;
use featherpdf_core::OpcoesLimpeza;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone, Default)]
pub struct Sanitizar {
    pub opcoes: OpcoesLimpeza,
    /// Relatório da última limpeza (vazio = não havia nada a tirar).
    pub relatorio: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
pub enum Sz { Abrir, Opcao(usize, bool), Aplicar }

/// (rótulo, explicação) de cada opção, na ordem de `campo`.
const OPCOES: [(&str, &str); 7] = [
    ("Metadados", "Autor, título, programa e datas guardados no arquivo."),
    ("Histórico de edição", "Dados privados de programas, miniaturas e revisões anteriores."),
    ("Anexos", "Arquivos embutidos no PDF."),
    ("JavaScript", "Código que roda ao abrir, ao clicar ou ao preencher."),
    ("Autores dos comentários", "Os comentários ficam, sem o nome de quem escreveu."),
    ("Camadas ocultas", "Conteúdo de camadas desligadas, que não aparece mas está lá."),
    ("Links externos", "Links para sites, para outros arquivos e envio de formulário."),
];

fn campo(o: &mut OpcoesLimpeza, i: usize) -> &mut bool {
    match i { 0 => &mut o.metadados, 1 => &mut o.historico, 2 => &mut o.anexos, 3 => &mut o.javascript,
        4 => &mut o.autores, 5 => &mut o.camadas_ocultas, _ => &mut o.links_externos }
}

impl App {
    pub fn update_sanitizar(&mut self, m: Sz) -> Task<Msg> {
        match m {
            Sz::Abrir => { self.sanitizar.relatorio = None; self.painel = Some(Painel::Sanitizar); }
            Sz::Opcao(i, v) => *campo(&mut self.sanitizar.opcoes, i) = v,
            Sz::Aplicar => {
                let opcoes = self.sanitizar.opcoes;
                let mut relatorio = Vec::new();
                let tarefa = self.editar(|pdf| {
                    relatorio = pdf.sanitizar(opcoes)?;
                    if relatorio.is_empty() { return Err("Nada a tirar: o arquivo já estava limpo nesses itens.".into()); }
                    Ok(format!("{} item(ns) removido(s). Ctrl+Z desfaz.", relatorio.len()))
                });
                self.sanitizar.relatorio = Some(relatorio);
                return tarefa;
            }
        }
        Task::none()
    }

    pub fn painel_sanitizar(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let mut corpo = column![
            text("Tira do arquivo o que não aparece na página mas viaja junto. Salve depois com Salvar como para manter o original.").size(12.5),
            dlg::secao("Remover"),
        ].spacing(10);
        let mut opcoes = self.sanitizar.opcoes;
        for (i, (rotulo, explicacao)) in OPCOES.iter().enumerate() {
            let marcado = *campo(&mut opcoes, i);
            corpo = corpo.push(column![
                checkbox(*rotulo, marcado).on_toggle(move |v| Msg::Sanitizar(Sz::Opcao(i, v))).size(16).text_size(13),
                row![Space::with_width(26), text(*explicacao).size(11.5).color(muted)],
            ].spacing(2));
        }
        let algum = (0..OPCOES.len()).any(|i| *campo(&mut opcoes, i));
        corpo = corpo.push(row![Space::with_width(Length::FILL), dlg::botao_primario("Limpar", algum.then_some(Msg::Sanitizar(Sz::Aplicar)))]);
        if let Some(r) = &self.sanitizar.relatorio {
            let mut lista = column![dlg::secao("Removido")].spacing(4);
            if r.is_empty() { lista = lista.push(text("Nada: o arquivo já estava limpo nesses itens.").size(12.5).color(muted)); }
            for linha in r { lista = lista.push(text(format!("• {linha}")).size(12.5)); }
            corpo = corpo.push(container(lista).padding([8, 12]).width(Length::FILL).style(dlg::estilo_bloco));
        }
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
