//! Painel "Regras dos campos" (aba Formulários): validação com máscara, limite de caracteres,
//! campo calculado e ordem do Tab da página. Cada mudança é um Ctrl+Z; o motor está em
//! `featherpdf_core::Regra`/`Calculo`.

use feather_ui::{dialogo as dlg, tema};
use featherpdf_core::{Calculo, Campo, OpCalculo, Regra, TipoCampo};

/// O id do widget no PDF (o `ObjectId` do lopdf).
type ObjectId = (u32, u16);

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoCalc { Nenhum, Op(OpCalculo), Formula }

impl TipoCalc {
    fn nome(self) -> &'static str { match self { TipoCalc::Nenhum => "Nenhum", TipoCalc::Op(o) => o.nome(), TipoCalc::Formula => "Fórmula" } }
}

#[derive(Debug, Clone)]
pub struct RegrasUi { campo: Option<String>, limite: String, calc: TipoCalc, texto: String }

impl Default for RegrasUi {
    fn default() -> Self { Self { campo: None, limite: String::new(), calc: TipoCalc::Nenhum, texto: String::new() } }
}

#[derive(Debug, Clone)]
pub enum Rg {
    Abrir,
    Campo(String),
    Regra(Option<Regra>),
    Limite(String),
    AplicarLimite,
    Calc(TipoCalc),
    Texto(String),
    AplicarCalc,
    Mover(ObjectId, i32),
    PorLinhas,
}

impl RegrasUi {
    fn carregar(&mut self, c: &Campo) {
        self.campo = Some(c.nome.clone());
        self.limite = c.limite.map_or(String::new(), |n| n.to_string());
        (self.calc, self.texto) = match &c.calculo {
            None => (TipoCalc::Nenhum, String::new()),
            Some(Calculo::Simples(op, v)) => (TipoCalc::Op(*op), v.join("; ")),
            Some(Calculo::Formula(f)) => (TipoCalc::Formula, f.clone()),
        };
    }

    fn calculo(&self) -> Option<Calculo> {
        match self.calc {
            TipoCalc::Nenhum => None,
            TipoCalc::Op(op) => Some(Calculo::Simples(op, self.texto.split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())),
            TipoCalc::Formula => Some(Calculo::Formula(self.texto.trim().to_string())),
        }
    }
}

impl App {
    fn campo_escolhido(&self) -> Option<Campo> {
        let nome = self.regras.campo.as_ref()?;
        self.aberto.as_ref()?.pdf.campos().into_iter().find(|c| &c.nome == nome)
    }

    /// Grava regra/cálculo/limite do campo escolhido, mudando só o que `f` mexe.
    fn gravar_regras(&mut self, f: impl FnOnce(&mut (Option<Regra>, Option<Calculo>, Option<usize>))) -> Task<Msg> {
        let Some(c) = self.campo_escolhido() else { return Task::none() };
        let mut r = (c.regra, c.calculo.clone(), c.limite);
        f(&mut r);
        self.editar(move |pdf| {
            pdf.definir_regras(&c, r.0, r.1, r.2)?;
            Ok(format!("Regras de “{}” gravadas (Ctrl+Z desfaz).", c.nome))
        })
    }

    pub fn update_regras(&mut self, m: Rg) -> Task<Msg> {
        match m {
            Rg::Abrir => {
                self.painel = if self.painel == Some(Painel::RegrasCampos) { None } else { Some(Painel::RegrasCampos) };
                if self.regras.campo.is_none() {
                    if let Some(c) = self.aberto.as_ref().and_then(|a| a.pdf.campos().into_iter().find(|c| matches!(c.tipo, TipoCampo::Texto { .. }))) { self.regras.carregar(&c); }
                }
            }
            Rg::Campo(n) => if let Some(c) = self.aberto.as_ref().and_then(|a| a.pdf.campos().into_iter().find(|c| c.nome == n)) {
                self.regras.carregar(&c);
                return self.update_interno(Msg::IrPagina(c.pagina));
            },
            Rg::Regra(r) => return self.gravar_regras(|x| x.0 = r),
            Rg::Limite(t) => self.regras.limite = t.chars().filter(char::is_ascii_digit).collect(),
            Rg::AplicarLimite => { let n = self.regras.limite.parse().ok(); return self.gravar_regras(|x| x.2 = n); }
            Rg::Calc(t) => {
                if t == TipoCalc::Nenhum && self.regras.calc != TipoCalc::Nenhum { self.regras.calc = t; return self.gravar_regras(|x| x.1 = None); }
                self.regras.calc = t;
            }
            Rg::Texto(t) => self.regras.texto = t,
            Rg::AplicarCalc => { let k = self.regras.calculo(); return self.gravar_regras(|x| x.1 = k); }
            Rg::Mover(w, passo) => {
                let pagina = self.pagina;
                return self.editar(move |pdf| {
                    let mut ids: Vec<ObjectId> = pdf.ordem_tab(pagina)?.iter().map(|c| c.widget).collect();
                    let i = ids.iter().position(|x| *x == w).ok_or("Campo fora desta página.")?;
                    let j = (i as i32 + passo).clamp(0, ids.len() as i32 - 1) as usize;
                    ids.swap(i, j);
                    pdf.definir_ordem_tab(pagina, &ids)?;
                    Ok("Ordem do Tab mudada (Ctrl+Z desfaz).".into())
                });
            }
            Rg::PorLinhas => {
                let pagina = self.pagina;
                return self.editar(move |pdf| { pdf.ordem_tab_por_linhas(pagina)?; Ok("Tab de cima para baixo, da esquerda para a direita.".into()) });
            }
        }
        Task::none()
    }

    pub fn painel_regras(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let Some(a) = &self.aberto else { return text("").into() };
        let campos = a.pdf.campos();
        let mut nomes: Vec<String> = campos.iter().filter(|c| matches!(c.tipo, TipoCampo::Texto { .. })).map(|c| c.nome.clone()).collect();
        nomes.dedup();
        let chip = |rotulo: String, ativo: bool, msg: Msg| button(text(rotulo).size(12).center()).width(Length::FILL).padding([5, 4])
            .style(tema::estilo_botao_pequeno(ativo)).on_press(msg);
        let mut corpo = column![].spacing(10);
        if nomes.is_empty() {
            corpo = corpo.push(text("Este PDF não tem campo de texto. Crie um pela faixa (Campo de texto) e volte aqui.").size(13).color(muted));
        } else if let Some(c) = self.campo_escolhido() {
            corpo = corpo.push(dlg::secao("Campo"))
                .push(suspensa(nomes, self.regras.campo.clone(), |n| Msg::Regras(Rg::Campo(n))).width(Length::FILL).text_size(13));
            // Validação, dois por linha.
            let mut regras = column![chip("Nenhuma".into(), c.regra.is_none(), Msg::Regras(Rg::Regra(None)))].spacing(4);
            for par in Regra::TODAS.chunks(2) {
                let mut r = row![].spacing(4);
                for &x in par { r = r.push(chip(x.nome().into(), c.regra == Some(x), Msg::Regras(Rg::Regra(Some(x))))); }
                if par.len() == 1 { r = r.push(Space::with_width(Length::FILL)); }
                regras = regras.push(r);
            }
            corpo = corpo.push(dlg::secao("Validação")).push(regras)
                .push(text("Ao terminar de digitar, o valor ganha a máscara (CPF e CNPJ conferem os dígitos). No Acrobat vale o mesmo script.").size(11.5).color(muted));
            corpo = corpo.push(dlg::secao("Limite de caracteres")).push(row![
                text_input("sem limite", &self.regras.limite).on_input(|t| Msg::Regras(Rg::Limite(t))).on_submit(Msg::Regras(Rg::AplicarLimite)).width(100).padding([4, 8]).size(13),
                button(text("Aplicar").size(12)).padding([4, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Regras(Rg::AplicarLimite)),
            ].spacing(6).align_y(Align::Center));
            // Cálculo.
            let tipos = [TipoCalc::Nenhum, TipoCalc::Op(OpCalculo::Soma), TipoCalc::Op(OpCalculo::Media), TipoCalc::Op(OpCalculo::Produto),
                TipoCalc::Op(OpCalculo::Minimo), TipoCalc::Op(OpCalculo::Maximo), TipoCalc::Formula];
            let mut grade = column![].spacing(4);
            for linha in tipos.chunks(3) {
                let mut r = row![].spacing(4);
                for &t in linha { r = r.push(chip(t.nome().into(), self.regras.calc == t, Msg::Regras(Rg::Calc(t)))); }
                for _ in linha.len()..3 { r = r.push(Space::with_width(Length::FILL)); }
                grade = grade.push(r);
            }
            corpo = corpo.push(dlg::secao("Cálculo")).push(grade);
            if self.regras.calc != TipoCalc::Nenhum {
                let dica = if self.regras.calc == TipoCalc::Formula { "Qtd * [Valor unitário] - Desconto" } else { "Campo1; Campo2; Campo3" };
                corpo = corpo.push(text_input(dica, &self.regras.texto).on_input(|t| Msg::Regras(Rg::Texto(t))).on_submit(Msg::Regras(Rg::AplicarCalc)).padding([5, 8]).size(13));
                let k = self.regras.calculo();
                let conferido = k.as_ref().map(|k| k.conferir().and_then(|_| {
                    let faltam: Vec<String> = k.entradas().into_iter().filter(|n| !campos.iter().any(|c| &c.nome == n)).collect();
                    if faltam.is_empty() { Ok(()) } else { Err(format!("Campo que não existe: {}", faltam.join(", "))) }
                }));
                match conferido {
                    Some(Err(e)) => corpo = corpo.push(dlg::aviso("alert-triangle", e)),
                    _ => {
                        let numero = |n: &str| campos.iter().find(|c| c.nome == n).and_then(|c| featherpdf_core::ler_numero(&c.valor)).unwrap_or(0.0);
                        if let Some(Ok(x)) = k.as_ref().map(|k| k.resultado(&numero)) {
                            corpo = corpo.push(text(format!("Com os valores de agora: {}", featherpdf_core::escrever_numero(x, 2))).size(12).color(muted));
                        }
                    }
                }
                corpo = corpo.push(row![Space::with_width(Length::FILL), dlg::botao_primario("Aplicar cálculo", Some(Msg::Regras(Rg::AplicarCalc)))])
                    .push(text("Nomes com espaço vão entre colchetes. O campo calculado não aceita digitação e se atualiza sozinho.").size(11.5).color(muted));
            }
        }
        // Ordem do Tab da página atual.
        let ordem = a.pdf.ordem_tab(self.pagina).unwrap_or_default();
        corpo = corpo.push(dlg::secao(&format!("Ordem do Tab — página {}", self.pagina)));
        if ordem.is_empty() {
            corpo = corpo.push(text("Nenhum campo nesta página.").size(12).color(muted));
        } else {
            let seta = |icone: &'static str, msg: Option<Msg>| button(feather_ui::icones::icone_traco(icone, 13.0)).padding(3).style(tema::estilo_botao_pequeno(false)).on_press_maybe(msg);
            let mut lista = column![].spacing(2);
            for (i, c) in ordem.iter().enumerate() {
                lista = lista.push(row![
                    text(format!("{}.", i + 1)).size(12).color(muted).width(24),
                    text(c.nome.clone()).size(13).width(Length::FILL),
                    seta("chevron-up", (i > 0).then_some(Msg::Regras(Rg::Mover(c.widget, -1)))),
                    seta("chevron-down", (i + 1 < ordem.len()).then_some(Msg::Regras(Rg::Mover(c.widget, 1)))),
                ].spacing(4).align_y(Align::Center));
            }
            corpo = corpo.push(lista).push(button(text("Por linhas (de cima para baixo)").size(12)).padding([4, 10]).width(Length::FILL)
                .style(tema::estilo_botao_pequeno(false)).on_press(Msg::Regras(Rg::PorLinhas)));
        }
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
