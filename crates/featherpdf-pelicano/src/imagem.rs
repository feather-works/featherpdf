//! Painel "Imagem" (ferramenta Editar, com uma imagem escolhida): tamanho em mm com
//! proporção travada, corte por lado, brilho e contraste, e trocar por outro arquivo.
//! Cada mudança é um passo de Ctrl+Z; os pixels nunca são recomprimidos.

use std::sync::Arc;

use feather_ui::dialogo as dlg;
use feather_ui::tema;
use featherpdf_core::{ImagemNova, TipoObjeto};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg, Painel};

const MM: f32 = 72.0 / 25.4;

#[derive(Debug, Clone)]
pub struct EditarImagem {
    pub largura: String,
    pub altura: String,
    pub proporcao: bool,
    /// Esquerda, direita, topo, base, em % da imagem.
    pub corte: [f32; 4],
    pub brilho: f32,
    pub contraste: f32,
}

impl Default for EditarImagem {
    fn default() -> Self {
        Self { largura: String::new(), altura: String::new(), proporcao: true, corte: [0.0; 4], brilho: 0.0, contraste: 1.0 }
    }
}

#[derive(Debug, Clone)]
pub enum Im {
    Abrir,
    Largura(String),
    Altura(String),
    Proporcao(bool),
    AplicarTamanho,
    Corte(usize, f32),
    Cortar,
    Brilho(f32),
    Contraste(f32),
    AplicarAjuste,
    Original,
    Trocar,
    Lida(Result<(Arc<ImagemNova>, u32, u32), String>),
}

fn numero(t: &str) -> Option<f32> { t.trim().replace(',', ".").parse::<f32>().ok().filter(|v| *v > 0.0) }
fn mm(v: f32) -> String { format!("{:.1}", v / MM).replace('.', ",") }

impl App {
    /// A imagem escolhida (uma só) na ferramenta Editar: página, índice e caixa.
    pub fn imagem_escolhida(&self) -> Option<(u32, usize, [f32; 4])> {
        let (p, s) = self.escolhidos.as_ref()?;
        if s.len() != 1 { return None; }
        let k = *s.first()?;
        let o = self.objetos.get(p)?.lista.get(k)?;
        matches!(o.tipo, TipoObjeto::Imagem { .. }).then_some((*p, k, o.caixa))
    }

    pub fn update_imagem(&mut self, m: Im) -> Task<Msg> {
        let Some((pagina, k, c)) = self.imagem_escolhida() else {
            if matches!(m, Im::Abrir) { self.status = "Escolha uma imagem com a ferramenta Editar.".into(); }
            return Task::none();
        };
        let (l0, a0) = (c[2] - c[0], c[3] - c[1]);
        let e = &mut self.imagem;
        match m {
            Im::Abrir => {
                let (b, ct) = self.aberto.as_ref().map_or((0.0, 1.0), |a| a.pdf.ajuste_da_imagem(pagina, k));
                self.imagem = EditarImagem { largura: mm(l0), altura: mm(a0), brilho: b, contraste: ct, ..Default::default() };
                self.painel = Some(Painel::Imagem);
            }
            Im::Largura(t) => {
                if e.proporcao { if let Some(v) = numero(&t) { e.altura = mm(v * MM * a0 / l0); } }
                e.largura = t;
            }
            Im::Altura(t) => {
                if e.proporcao { if let Some(v) = numero(&t) { e.largura = mm(v * MM * l0 / a0); } }
                e.altura = t;
            }
            Im::Proporcao(v) => e.proporcao = v,
            Im::AplicarTamanho => {
                let (Some(l), Some(a)) = (numero(&e.largura), numero(&e.altura)) else { self.status = "Tamanho inválido.".into(); return Task::none() };
                // Cresce a partir do centro, como no Word quando se digita o tamanho.
                let (l, a) = (l * MM, a * MM);
                let (cx, cy) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
                let nova = [cx - l / 2.0, cy - a / 2.0, cx + l / 2.0, cy + a / 2.0];
                return self.editar(move |pdf| pdf.redimensionar_imagem(pagina, k, nova).map(|_| "Imagem redimensionada. Ctrl+Z desfaz.".into()));
            }
            Im::Corte(i, v) => e.corte[i] = v,
            Im::Cortar => {
                let corte = e.corte.map(|v| v / 100.0);
                e.corte = [0.0; 4];
                return self.editar(move |pdf| pdf.cortar_imagem(pagina, k, corte).map(|_| "Imagem cortada (o resto fica escondido). Ctrl+Z desfaz.".into()));
            }
            Im::Brilho(v) => e.brilho = v,
            Im::Contraste(v) => e.contraste = v,
            Im::Original => { e.brilho = 0.0; e.contraste = 1.0; return self.update_imagem(Im::AplicarAjuste); }
            Im::AplicarAjuste => {
                let (b, ct) = (e.brilho, e.contraste);
                return self.editar(move |pdf| pdf.ajustar_imagem(pagina, k, b, ct).map(|_| "Brilho e contraste ajustados. Ctrl+Z desfaz.".into()));
            }
            Im::Trocar => return Task::perform(async {
                let arquivo = rfd::AsyncFileDialog::new()
                    .add_filter("Imagem", &["png", "jpg", "jpeg", "webp", "PNG", "JPG", "JPEG", "WEBP"]).pick_file().await?;
                let bytes = arquivo.read().await;
                Some(tokio::task::spawn_blocking(move || crate::inserir::ler_imagem(bytes)).await
                    .unwrap_or_else(|e| Err(format!("Falha ao ler a imagem: {e}"))))
            }, |r| Msg::Imagem(Im::Lida(r.unwrap_or_else(|| Err(String::new()))))),
            // Erro vazio: a pessoa fechou a janela sem escolher.
            Im::Lida(Err(erro)) => if !erro.is_empty() { self.status = erro },
            Im::Lida(Ok((img, l, a))) => {
                let img = Arc::unwrap_or_clone(img);
                return self.editar(move |pdf| pdf.trocar_imagem(pagina, k, img, l, a).map(|_| "Imagem trocada. Ctrl+Z desfaz.".into()));
            }
        }
        Task::none()
    }

    pub fn painel_imagem(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let Some((_, _, c)) = self.imagem_escolhida() else {
            return dlg::aviso_vazio("Escolha uma imagem na página com a ferramenta Editar.");
        };
        let e = &self.imagem;
        let campo = |rotulo: &'static str, valor: &str, f: fn(String) -> Im| row![
            text(rotulo).size(12.5).width(60),
            text_input("mm", valor.to_owned()).on_input(move |t| Msg::Imagem(f(t))).on_submit(Msg::Imagem(Im::AplicarTamanho)).padding([4, 8]).size(13).width(90),
            text("mm").size(12).color(muted),
        ].spacing(6).align_y(Align::Center);
        let lados = ["Esquerda", "Direita", "Topo", "Base"];
        let mut corte = column![].spacing(4);
        for (i, nome) in lados.iter().enumerate() {
            corte = corte.push(row![
                text(*nome).size(12.5).width(70),
                slider(0.0..=90.0, e.corte[i], move |v| Msg::Imagem(Im::Corte(i, v))).step(1.0f32),
                text(format!("{:.0}%", e.corte[i])).size(12).width(40),
            ].spacing(6).align_y(Align::Center));
        }
        let corta = e.corte.iter().any(|v| *v > 0.0);
        let mudou = e.brilho.abs() > 1e-3 || (e.contraste - 1.0).abs() > 1e-3;
        let corpo = column![
            text(format!("Agora: {} × {} mm.", mm(c[2] - c[0]), mm(c[3] - c[1]))).size(12).color(muted),
            dlg::secao("Tamanho"),
            campo("Largura", &e.largura, Im::Largura),
            campo("Altura", &e.altura, Im::Altura),
            checkbox("Manter a proporção", e.proporcao).on_toggle(|v| Msg::Imagem(Im::Proporcao(v))).size(16).text_size(13),
            row![Space::with_width(Length::FILL), button(text("Aplicar tamanho").size(12)).padding([4, 10])
                .style(tema::estilo_botao_pequeno(false)).on_press(Msg::Imagem(Im::AplicarTamanho))],
            dlg::secao("Cortar"),
            text("Quanto sai de cada lado. O que sai fica escondido, não é apagado do arquivo.").size(11.5).color(muted),
            corte,
            row![Space::with_width(Length::FILL), button(text("Cortar").size(12)).padding([4, 10])
                .style(tema::estilo_botao_pequeno(false)).on_press_maybe(corta.then_some(Msg::Imagem(Im::Cortar)))],
            dlg::secao("Brilho e contraste"),
            row![text("Brilho").size(12.5).width(70),
                mouse_area(slider(-0.5..=0.5, e.brilho, |v| Msg::Imagem(Im::Brilho(v))).step(0.05f32)).on_release(Msg::Imagem(Im::AplicarAjuste)),
                text(format!("{:+.0}", e.brilho * 100.0)).size(12).width(40)].spacing(6).align_y(Align::Center),
            row![text("Contraste").size(12.5).width(70),
                mouse_area(slider(0.2..=2.5, e.contraste, |v| Msg::Imagem(Im::Contraste(v))).step(0.05f32)).on_release(Msg::Imagem(Im::AplicarAjuste)),
                text(format!("{:.0}%", e.contraste * 100.0)).size(12).width(40)].spacing(6).align_y(Align::Center),
            text("Vale para imagens em cinza ou RGB.").size(11.5).color(muted),
            row![Space::with_width(Length::FILL), button(text("Original").size(12)).padding([4, 10])
                .style(tema::estilo_botao_pequeno(false)).on_press_maybe(mudou.then_some(Msg::Imagem(Im::Original)))],
            dlg::secao("Trocar"),
            row![button(text("Trocar por outra imagem…").size(12)).padding([4, 10])
                .style(tema::estilo_botao_pequeno(false)).on_press(Msg::Imagem(Im::Trocar))],
            text("A nova fica centrada na mesma caixa, sem distorcer.").size(11.5).color(muted),
        ].spacing(8);
        scrollable(corpo.padding([0, 4])).height(Length::FILL).into()
    }
}
