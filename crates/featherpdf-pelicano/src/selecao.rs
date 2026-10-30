//! Selecionar texto arrastando na página (ferramenta Selecionar), como num leitor de PDF:
//! a seleção corre na ordem de leitura do ponto onde o arrasto começou até onde parou.
//! Ctrl+C e o botão direito copiam.

use featherpdf_core::{AnotacaoNova, Marcacao, TipoObjeto};

use pelicano::*;
use crate::compat::*;
use crate::compat::mouse::{self, Cursor};
use crate::compat::{event, canvas::{self, Frame, Geometry}};
use crate::edicao::na_tela;
use crate::{App, Ferramenta, Msg};

const AZUL: Color = Color { r: 0.16, g: 0.45, b: 0.86, a: 0.30 };

/// Posição no texto da página: (linha na ordem de leitura, caractere).
pub type Posicao = (usize, usize);

#[derive(Debug, Clone)]
pub enum Sel {
    Feita { pagina: u32, de: Posicao, ate: Posicao },
    Limpar,
    Copiar,
    Marcar(Marcacao),
    Tarjar,
}

/// Uma linha de texto na tela: caixa, número de caracteres e fileira (linhas lado a lado dividem a fileira).
#[derive(Debug, Clone)]
pub struct LinhaTela { caixa: Rect, letras: usize, fileira: usize }

/// Ordem de leitura: de cima para baixo por fileira e, dentro dela, da esquerda para a direita.
/// Devolve os índices dos objetos de texto e as linhas na tela, na mesma ordem.
fn em_ordem(caixas: Vec<(usize, Rect, usize)>) -> (Vec<usize>, Vec<LinhaTela>) {
    let mut v = caixas;
    v.sort_by(|a, b| (a.1.y + a.1.height / 2.0).total_cmp(&(b.1.y + b.1.height / 2.0)));
    let mut fileiras = Vec::with_capacity(v.len());
    let (mut fileira, mut centro) = (0, f32::NEG_INFINITY);
    for (_, r, _) in &v {
        let c = r.y + r.height / 2.0;
        if c - centro > r.height / 2.0 { fileira += 1; centro = c; }
        fileiras.push(fileira);
    }
    let mut juntos: Vec<_> = v.into_iter().zip(fileiras).collect();
    juntos.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.1.x.total_cmp(&b.0.1.x)));
    juntos.into_iter().map(|((i, caixa, letras), fileira)| (i, LinhaTela { caixa, letras, fileira })).unzip()
}

/// A linha mais perto do ponto e o caractere sob ele (pela fração da largura: a linha não guarda a largura de cada letra).
fn posicao(linhas: &[LinhaTela], p: Point) -> Option<Posicao> {
    let distancia = |r: &Rect| {
        let dx = (r.x - p.x).max(p.x - (r.x + r.width)).max(0.0);
        let dy = (r.y - p.y).max(p.y - (r.y + r.height)).max(0.0);
        dy * 4.0 + dx
    };
    let (i, l) = linhas.iter().enumerate().min_by(|a, b| distancia(&a.1.caixa).total_cmp(&distancia(&b.1.caixa)))?;
    let fracao = ((p.x - l.caixa.x) / l.caixa.width.max(1.0)).clamp(0.0, 1.0);
    Some((i, (fracao * l.letras as f32).round() as usize))
}

fn ordenar(a: Posicao, b: Posicao) -> (Posicao, Posicao) { if a <= b { (a, b) } else { (b, a) } }

/// Trecho [de, ate) de cada linha tocada pela seleção.
fn trechos(linhas: &[LinhaTela], de: Posicao, ate: Posicao) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let (de, ate) = ordenar(de, ate);
    (de.0..=ate.0.min(linhas.len().saturating_sub(1))).filter_map(move |i| {
        let n = linhas.get(i)?.letras;
        let (a, b) = (if i == de.0 { de.1.min(n) } else { 0 }, if i == ate.0 { ate.1.min(n) } else { n });
        (a < b).then_some((i, a, b))
    })
}

struct Camada { pagina: u32, linhas: Vec<LinhaTela>, fixa: Option<(Posicao, Posicao)> }

impl canvas::Program<Msg> for Camada {
    type State = Option<(Posicao, Posicao)>;

    fn update(&self, estado: &mut Self::State, evento: canvas::Event, limites: Rect, cursor: Cursor) -> (event::Status, Option<Msg>) {
        match evento {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_in(limites) else { return (event::Status::Ignored, None) };
                *estado = posicao(&self.linhas, p).map(|q| (q, q));
                (event::Status::Captured, Some(Msg::Selecao(Sel::Limpar)))
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => match (estado.as_mut(), cursor.position_from(limites.position())) {
                (Some((_, b)), Some(p)) => {
                    if let Some(q) = posicao(&self.linhas, p) { *b = q; }
                    (event::Status::Captured, None)
                }
                _ => (event::Status::Ignored, None),
            },
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => match estado.take() {
                Some((de, ate)) if de != ate => (event::Status::Captured, Some(Msg::Selecao(Sel::Feita { pagina: self.pagina, de, ate }))),
                Some(_) => (event::Status::Captured, None),
                None => (event::Status::Ignored, None),
            },
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(&self, estado: &Self::State, renderer: &Renderer, _: &Theme, limites: Rect, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, limites.size());
        if let Some((de, ate)) = estado.or(self.fixa) {
            for (i, a, b) in trechos(&self.linhas, de, ate) {
                let l = &self.linhas[i];
                let k = l.caixa.width / l.letras.max(1) as f32;
                f.fill_rectangle(Point::new(l.caixa.x + a as f32 * k, l.caixa.y), Size::new((b - a) as f32 * k, l.caixa.height), AZUL);
            }
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, estado: &Self::State, limites: Rect, cursor: Cursor) -> mouse::Interaction {
        let Some(p) = cursor.position_in(limites) else { return mouse::Interaction::default() };
        if estado.is_some() || self.linhas.iter().any(|l| l.caixa.expand(2.0).contains(p)) { mouse::Interaction::Text } else { mouse::Interaction::default() }
    }
}

impl App {
    /// Linhas de texto da página `n` em ordem de leitura: índices dos objetos e caixas na tela.
    fn linhas_em_ordem(&self, n: u32) -> Option<(Vec<usize>, Vec<LinhaTela>)> {
        let o = self.objetos.get(&n)?;
        // shortcut: o caractere sai da fração da largura no eixo x; em página girada 90°/270° a seleção dentro da linha erra.
        let caixas = o.lista.iter().enumerate().filter_map(|(i, ob)| match &ob.tipo {
            TipoObjeto::Texto { texto, .. } if !texto.trim().is_empty() =>
                Some((i, na_tela(ob.caixa, o.caixa, o.giro, self.zoom), texto.trim_end().chars().count())),
            _ => None,
        }).collect();
        Some(em_ordem(caixas))
    }

    pub fn camada_selecao(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        if self.ferramenta != Ferramenta::Selecionar { return None; }
        let (_, linhas) = self.linhas_em_ordem(n)?;
        if linhas.is_empty() { return None; }
        let fixa = self.selecao_texto.filter(|s| s.0 == n).map(|s| (s.1, s.2));
        Some(canvas::Canvas::new(Camada { pagina: n, linhas, fixa }).width(l).height(a).into())
    }

    /// O texto selecionado (linhas da mesma fileira separadas por espaço, fileiras por quebra de linha)
    /// e uma caixa por linha tocada, no espaço da página.
    pub fn selecionado(&self) -> Option<(u32, String, Vec<[f32; 4]>)> {
        let (n, de, ate) = self.selecao_texto?;
        let (indices, linhas) = self.linhas_em_ordem(n)?;
        let o = self.objetos.get(&n)?;
        let (mut saida, mut caixas) = (String::new(), vec![]);
        let mut anterior = None;
        for (i, a, b) in trechos(&linhas, de, ate) {
            let ob = o.lista.get(indices[i])?;
            let TipoObjeto::Texto { texto, .. } = &ob.tipo else { continue };
            match anterior {
                Some(f) if f == linhas[i].fileira => saida.push(' '),
                Some(_) => saida.push('\n'),
                None => {}
            }
            anterior = Some(linhas[i].fileira);
            saida.extend(texto.trim_end().chars().skip(a).take(b - a));
            let [x0, y0, x1, y1] = ob.caixa;
            let k = (x1 - x0) / linhas[i].letras.max(1) as f32;
            caixas.push([x0 + a as f32 * k, y0, x0 + b as f32 * k, y1]);
        }
        (!saida.is_empty()).then_some((n, saida, caixas))
    }

    pub fn update_selecao(&mut self, m: Sel) -> Task<Msg> {
        match m {
            Sel::Feita { pagina, de, ate } => {
                self.selecao_texto = Some((pagina, de, ate));
                self.pagina = pagina;
            }
            Sel::Limpar => self.selecao_texto = None,
            Sel::Marcar(tipo) => {
                let Some((pagina, texto, caixas)) = self.selecionado() else { return Task::none() };
                self.selecao_texto = None;
                return self.anotar(pagina, AnotacaoNova::Marcacao { tipo, caixas, texto }, "Marcado. Ctrl+Z desfaz.");
            }
            Sel::Tarjar => {
                let Some((pagina, texto, caixas)) = self.selecionado() else { return Task::none() };
                self.selecao_texto = None;
                self.tarjar_selecao(pagina, caixas, texto);
            }
            Sel::Copiar => {
                let Some((_, t, _)) = self.selecionado() else { return Task::none() };
                self.status = format!("{} caractere(s) copiado(s).", t.chars().count());
                return Task::write_clipboard(t);
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn r(x: f32, y: f32, w: f32) -> Rect { Rect::new(Point::new(x, y), Size::new(w, 10.0)) }

    #[test]
    fn ordem_de_leitura_e_trechos() {
        // Duas colunas na mesma fileira (a da direita chega primeiro) e uma linha embaixo.
        let (ind, linhas) = em_ordem(vec![(0, r(200.0, 1.0, 100.0), 10), (1, r(0.0, 0.0, 100.0), 10), (2, r(0.0, 20.0, 100.0), 10)]);
        assert_eq!(ind, vec![1, 0, 2]);
        assert_eq!(linhas[0].fileira, linhas[1].fileira);
        // Do meio da primeira linha até o começo da terceira, arrastando de trás para a frente.
        let de = posicao(&linhas, Point::new(50.0, 5.0)).unwrap();
        let ate = posicao(&linhas, Point::new(30.0, 25.0)).unwrap();
        assert_eq!((de, ate), ((0, 5), (2, 3)));
        assert_eq!(trechos(&linhas, ate, de).collect::<Vec<_>>(), vec![(0, 5, 10), (1, 0, 10), (2, 0, 3)]);
    }
}
