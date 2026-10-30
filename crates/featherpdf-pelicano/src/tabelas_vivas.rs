//! Painel Tabelas da página: acrescentar linha ou coluna numa tabela que já está no PDF, e o
//! resto (o que vem embaixo ou à direita, e as bordas) se ajusta. O motor está em
//! `DocumentoPdf::{acrescentar_linha, acrescentar_coluna}`.

use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::TabelaPdf;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone)]
pub enum Tv { Abrir, Valores(String), Depois(String), Linha(usize), Coluna(usize) }

#[derive(Default)]
pub struct TabelasVivas { pub valores: String, pub depois: String, pub lista: Option<(u32, Result<Vec<TabelaPdf>, String>)> }

impl App {
    fn ler_tabelas_vivas(&mut self) {
        let pagina = self.pagina;
        self.tabelas_vivas.lista = self.aberto.as_ref().map(|a| (pagina, a.pdf.tabelas(false).map(|v| v.into_iter().filter(|t| t.pagina == pagina).collect())));
    }

    pub fn update_tabelas_vivas(&mut self, m: Tv) -> Task<Msg> {
        match m {
            Tv::Abrir => {
                self.painel = if self.painel == Some(Painel::Tabelas) { None } else { Some(Painel::Tabelas) };
                self.ler_tabelas_vivas();
            }
            Tv::Valores(t) => self.tabelas_vivas.valores = t,
            Tv::Depois(t) => self.tabelas_vivas.depois = t,
            Tv::Linha(i) | Tv::Coluna(i) => {
                let Some((pagina, Ok(lista))) = &self.tabelas_vivas.lista else { return Task::none() };
                let Some(t) = lista.get(i) else { return Task::none() };
                let (pagina, caixa, linhas) = (*pagina, t.caixa, t.linhas.len());
                let valores: Vec<String> = self.tabelas_vivas.valores.split(';').map(|v| v.trim().to_string()).collect();
                let depois = self.tabelas_vivas.depois.trim().parse::<usize>().unwrap_or(linhas).min(linhas);
                let linha = matches!(m, Tv::Linha(_));
                let tarefa = self.editar(move |pdf| {
                    if linha {
                        pdf.acrescentar_linha(pagina, caixa, depois, &valores)?;
                        Ok(format!("Linha nova depois da linha {depois}; o que estava embaixo desceu (Ctrl+Z desfaz)."))
                    } else {
                        pdf.acrescentar_coluna(pagina, caixa, &valores)?;
                        Ok("Coluna nova no fim; o que estava à direita andou (Ctrl+Z desfaz).".into())
                    }
                });
                self.ler_tabelas_vivas();
                return tarefa;
            }
        }
        Task::none()
    }

    pub fn painel_tabelas_vivas(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let tv = &self.tabelas_vivas;
        let mut corpo = column![
            text_input("Valores, separados por ;", &tv.valores).on_input(|t| Msg::Tabelas(Tv::Valores(t))).size(13).padding([6, 8]),
            row![text("Linha nova depois da linha").size(12), text_input("fim", &tv.depois).on_input(|t| Msg::Tabelas(Tv::Depois(t))).size(12).padding([3, 6]).width(50),
                text("(0 = cabeçalho)").size(11).color(muted)].spacing(6).align_y(Align::Center),
        ].spacing(10);
        match &tv.lista {
            Some((p, _)) if *p != self.pagina => corpo = corpo.push(button(text(format!("Ler as tabelas da página {}", self.pagina)).size(12)).padding([4, 10])
                .style(tema::estilo_botao_pequeno(false)).on_press(Msg::Tabelas(Tv::Abrir))),
            Some((_, Err(e))) => corpo = corpo.push(dlg::aviso("alert-triangle", e.clone())),
            Some((_, Ok(l))) if l.is_empty() => corpo = corpo.push(text("Nenhuma tabela com texto nesta página. Página escaneada precisa do OCR antes.").size(13).color(muted)),
            Some((_, Ok(l))) => for (i, t) in l.iter().enumerate() {
                let nome = if t.titulo.is_empty() { t.cabecalho.join(" · ") } else { t.titulo.clone() };
                corpo = corpo.push(container(column![
                    text(nome).size(13),
                    text(format!("{} coluna(s), {} linha(s)", t.cabecalho.len(), t.linhas.len())).size(11.5).color(muted),
                    row![Space::with_width(Length::FILL),
                        button(text("+ Linha").size(12)).padding([3, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Tabelas(Tv::Linha(i))),
                        button(text("+ Coluna no fim").size(12)).padding([3, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Tabelas(Tv::Coluna(i)))].spacing(6),
                ].spacing(4)).padding([8, 10]).width(Length::FILL).style(dlg::estilo_bloco));
            },
            None => {}
        }
        corpo = corpo.push(text("Linha: um valor por coluna. Coluna: o cabeçalho e depois um valor por linha. Tabela desenhada como uma peça só não estica.").size(11.5).color(muted));
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
