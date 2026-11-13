//! Tarja definitiva: marcar áreas na página, achar por busca ou padrão
//! (CPF, CNPJ, e-mail, telefone, CEP) e aplicar numa cópia "— tarjado.pdf".

use feather_ui::{icones as ic, tema};
use featherpdf_core::Padrao;

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::compat::mouse::Cursor;
use crate::compat::canvas::{self, Frame, Geometry, Path, Stroke};
use crate::edicao::na_tela;
use crate::proteger::Pr;
use crate::{App, Ferramenta, Msg, Painel};

use crate::inicio::VERMELHO;

#[derive(Debug, Clone)]
pub struct Marca { pub pagina: u32, pub caixa: [f32; 4], texto: String, origem: String, ativa: bool }

#[derive(Debug, Clone, Default)]
pub struct Tarjas { pub marcas: Vec<Marca>, busca: String, rotulo: String, metadados: bool }

#[derive(Debug, Clone)]
pub enum Tj {
    Area { pagina: u32, a: Point, b: Point },
    Busca(String),
    Buscar,
    Padroes,
    Ativar(usize, bool),
    Apagar(usize),
    Limpar,
    Rotulo(String),
    Metadados(bool),
    Aplicar,
}

pub fn id_busca() -> &'static str { "tarja-busca" }

/// As marcas por cima da página: vermelho translúcido (a aplicar) ou contorno tracejado (desligada).
struct Desenho { caixas: Vec<(Rect, bool)> }

impl canvas::Program<Msg> for Desenho {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        for (r, ativa) in &self.caixas {
            if *ativa { f.fill_rectangle(r.position(), r.size(), Color { a: 0.22, ..VERMELHO }); }
            let traco = Stroke::default().with_color(VERMELHO).with_width(if *ativa { 2.0 } else { 1.0 });
            f.stroke(&Path::rectangle(r.position(), r.size()), traco);
        }
        vec![f.into_geometry()]
    }
}

impl App {
    fn marcar(&mut self, marcas: impl IntoIterator<Item = Marca>) -> usize {
        let antes = self.tarjas.marcas.len();
        for m in marcas {
            // A mesma caixa achada de novo (busca repetida) não duplica.
            if !self.tarjas.marcas.iter().any(|x| x.pagina == m.pagina && x.caixa == m.caixa) { self.tarjas.marcas.push(m); }
        }
        self.painel = Some(Painel::Tarja);
        self.tarjas.marcas.len() - antes
    }

    /// Marca para tarjar o texto selecionado na página (uma caixa por linha).
    pub(crate) fn tarjar_selecao(&mut self, pagina: u32, caixas: Vec<[f32; 4]>, texto: String) {
        let novas = self.marcar(caixas.into_iter().map(|caixa| Marca { pagina, caixa, texto: texto.clone(), origem: "seleção".into(), ativa: true }));
        self.status = format!("{novas} trecho(s) marcado(s) para tarjar. Aplicar no painel Tarjar.");
    }

    fn achar(&mut self, busca: &str, padroes: &[Padrao]) {
        let Some(a) = &self.aberto else { return };
        let achados = a.pdf.achar_para_tarja(busca, padroes);
        let total = achados.len();
        let novas = self.marcar(achados.into_iter().map(|x| Marca {
            pagina: x.pagina, caixa: x.caixa, texto: x.texto,
            origem: x.tipo.map_or("busca", Padrao::nome).into(), ativa: true,
        }));
        self.status = match total {
            0 => "Nada encontrado para tarjar.".into(),
            _ => format!("{total} encontrado(s), {novas} marcado(s) agora."),
        };
    }

    pub fn update_tarja(&mut self, m: Tj) -> Task<Msg> {
        match m {
            Tj::Area { pagina, a, b } => {
                let (Some(p), Some(q)) = (self.ponto_na_pagina(pagina, a), self.ponto_na_pagina(pagina, b)) else { return Task::none() };
                let caixa = [p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1)];
                if caixa[2] - caixa[0] < 2.0 || caixa[3] - caixa[1] < 2.0 { return Task::none(); }
                self.marcar([Marca { pagina, caixa, texto: "Área marcada".into(), origem: "área".into(), ativa: true }]);
            }
            Tj::Busca(t) => self.tarjas.busca = t,
            Tj::Buscar => {
                if self.painel != Some(Painel::Tarja) || self.tarjas.busca.trim().is_empty() {
                    self.painel = Some(Painel::Tarja);
                    return crate::focar(id_busca());
                }
                let busca = self.tarjas.busca.clone();
                self.achar(&busca, &[]);
            }
            Tj::Padroes => self.achar("", &Padrao::TODOS),
            Tj::Ativar(i, v) => if let Some(m) = self.tarjas.marcas.get_mut(i) { m.ativa = v },
            Tj::Apagar(i) => if i < self.tarjas.marcas.len() { self.tarjas.marcas.remove(i); },
            Tj::Limpar => self.tarjas.marcas.clear(),
            Tj::Rotulo(t) => self.tarjas.rotulo = t,
            Tj::Metadados(v) => self.tarjas.metadados = v,
            Tj::Aplicar => {
                let Some(a) = &self.aberto else { return Task::none() };
                let marcas: Vec<(u32, [f32; 4])> = self.tarjas.marcas.iter().filter(|m| m.ativa).map(|m| (m.pagina, m.caixa)).collect();
                if marcas.is_empty() { return Task::none(); }
                let assinado = crate::assinar::tem_assinatura(&a.pdf);
                // A tarja vai numa cópia: o documento aberto continua como estava.
                let mut copia = a.pdf.clone();
                let r = copia.tarjar(&marcas, &self.tarjas.rotulo, &featherpdf_io::apagar_pixels_jpeg).and_then(|_| {
                    if self.tarjas.metadados { copia.limpar_metadados(); }
                    // Com senha, a cópia tarjada sai com a mesma senha.
                    copia.bytes_para_gravar()
                });
                let ok = r.is_ok();
                let tarefa = self.update_protecao(Pr::Pronto(r, None, "tarjado"));
                if ok && assinado { self.status = "A cópia tarjada é regravada inteira: as assinaturas do PDF não valem nela.".into(); }
                return tarefa;
            }
        }
        Task::none()
    }

    /// Marcas desta página e, com a ferramenta Marcar área, a camada que arrasta a caixa.
    pub fn camada_tarja(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        let ferramenta = self.ferramenta == Ferramenta::Tarja;
        let aqui: Vec<&Marca> = self.tarjas.marcas.iter().filter(|m| m.pagina == n).collect();
        if !ferramenta && aqui.is_empty() { return None; }
        let (pag, giro) = self.aberto.as_ref()?.pdf.geometria(n).ok()?;
        let caixas = aqui.iter().map(|m| (na_tela(m.caixa, pag, giro, self.zoom), m.ativa)).collect();
        let mut pilha = Stack::new().width(l).height(a).push(canvas::Canvas::new(Desenho { caixas }).width(l).height(a));
        if ferramenta {
            pilha = pilha.push(crate::inserir::camada_arrastar(n, |pagina, a, b| Msg::Tarja(Tj::Area { pagina, a, b }), l, a));
        }
        Some(pilha.into())
    }

    pub fn painel_tarja(&self) -> Element<'_, Msg> {
        let t = &self.tarjas;
        let muted = tema::COR_TEXTO_MUTED();
        let ativas = t.marcas.iter().filter(|m| m.ativa).count();
        // Contagem por origem, na ordem em que aparecem.
        let mut origens: Vec<(&str, usize)> = Vec::new();
        for m in &t.marcas {
            match origens.iter_mut().find(|(o, _)| *o == m.origem) { Some((_, n)) => *n += 1, None => origens.push((&m.origem, 1)) }
        }
        let mut chips = row![].spacing(6);
        for (o, n) in origens {
            chips = chips.push(container(text(format!("{o} · {n}")).size(12).color(crate::inicio::vermelho_texto())).padding([3, 9])
                .style(|_| Style { background: Some(Background::Color(Color { a: 0.10, ..VERMELHO })), border: Border { radius: [999.0; 4], ..Default::default() }, ..Default::default() }));
        }
        let mut lista = column![].spacing(0);
        for (i, m) in t.marcas.iter().enumerate() {
            lista = lista.push(container(row![
                checkbox("", m.ativa).on_toggle(move |v| Msg::Tarja(Tj::Ativar(i, v))).size(15),
                button(text(m.texto.clone()).size(13)).padding(0).style(tema::estilo_botao_pequeno(false))
                    .on_press(Msg::IrPagina(m.pagina)).width(Length::FILL),
                text(format!("{} · p. {}", m.origem, m.pagina)).size(11).color(muted),
                button(ic::icone("trash", 13.0)).padding(3).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Tarja(Tj::Apagar(i))),
            ].spacing(6).align_y(Align::Center)).padding([5, 8]));
        }
        if t.marcas.is_empty() {
            lista = lista.push(text("Arraste na página com Marcar área, busque um texto ou use Padrões.").size(13).color(muted));
        }
        let busca = row![
            text_input("Buscar texto para tarjar", &t.busca).id(id_busca()).on_input(|s| Msg::Tarja(Tj::Busca(s)))
                .on_submit(Msg::Tarja(Tj::Buscar)).padding([5, 8]).size(13),
            button(ic::icone("search", 14.0)).padding(6).style(tema::estilo_botao_pequeno(false))
                .on_press_maybe((!t.busca.trim().is_empty()).then_some(Msg::Tarja(Tj::Buscar))),
        ].spacing(6).align_y(Align::Center);
        let aplicar = match ativas { 0 => "Nada marcado".to_string(), 1 => "Aplicar 1 tarja e salvar cópia".into(), n => format!("Aplicar {n} tarjas e salvar cópia") };
        column![
            busca,
            button(text("Achar CPF, CNPJ, e-mail, telefone, CEP e nomes").size(12)).padding([5, 10]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Tarja(Tj::Padroes)),
            chips,
            container(scrollable(lista)).height(Length::FILL)
                .style(|_| Style { border: Border { color: tema::COR_BORDA(), width: 1.0, radius: [6.0; 4] }, ..Default::default() }),
            row![text("Texto sobre a tarja").size(12), Space::with_width(Length::FILL),
                button(text("Limpar lista").size(12)).padding([3, 8]).style(tema::estilo_botao_pequeno(false))
                    .on_press_maybe((!t.marcas.is_empty()).then_some(Msg::Tarja(Tj::Limpar)))].align_y(Align::Center),
            text_input("nada (preto liso)", &t.rotulo).on_input(|s| Msg::Tarja(Tj::Rotulo(s))).padding([5, 8]).size(13),
            checkbox("Também tirar metadados e anexos", t.metadados).on_toggle(|v| Msg::Tarja(Tj::Metadados(v))).size(15).text_size(13),
            container(text(format!("Aplicar apaga o texto e a imagem debaixo da tarja — não é um retângulo preto por cima. \
                O arquivo sai como cópia \u{201c}— tarjado.pdf\u{201d}; o aberto fica como está.{}",
                // Título de marcador (gerado dos títulos do Word) não passa pela tarja.
                if self.aberto.as_ref().is_some_and(|a| !a.pdf.marcadores().is_empty()) {
                    " Este PDF tem marcadores: os títulos deles não são tarjados, confira no painel Marcadores."
                } else { "" })).size(12))
                .padding([8, 10]).style(|_| Style {
                    background: Some(Background::Color(Color::rgb8(0xfb, 0xf3, 0xdc))),
                    text_color: Some(Color::rgb8(0x6b, 0x4e, 0x05)), border: Border { radius: [6.0; 4], ..Default::default() }, ..Default::default()
                }),
            button(container(text(aplicar).size(13)).center_x(Length::FILL)).padding([8, 10]).width(Length::FILL)
                .style(tema::estilo_botao_primario).on_press_maybe((ativas > 0).then_some(Msg::Tarja(Tj::Aplicar))),
        ].spacing(10).height(Length::FILL).into()
    }
}
