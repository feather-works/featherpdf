//! Configurações do FeatherPDF: a janela (Aparência, Exibição, Documentos, Atalhos, Sobre) e
//! as preferências gravadas em `config.txt` ("chave=valor", uma por linha) na pasta de
//! configuração do usuário, ao lado do `recentes.tsv`.

use std::path::{Path, PathBuf};

use feather_ui::{cromo, tema};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tema { Automatico, Claro, Escuro }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomInicial { Largura, Inteira, Cem, Manter }

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub tema: Tema,
    pub zoom: ZoomInicial,
    /// Painel Ferramentas e faixa como ficaram da última vez.
    pub ferramentas: bool,
    pub faixa: bool,
    /// Clicar num link de site do PDF abre o navegador; desligado, só mostra o endereço.
    pub abrir_sites: bool,
}

impl Default for Config {
    fn default() -> Self { Self { tema: Tema::Automatico, zoom: ZoomInicial::Manter, ferramentas: true, faixa: true, abrir_sites: true } }
}

#[derive(Debug, Clone)]
pub enum Cf { Tema(Tema), Zoom(ZoomInicial), LimparRecentes, LeitorPadrao, AbrirSites(bool) }

fn arquivo() -> Option<PathBuf> { crate::recentes::pasta_config().map(|p| p.join("config.txt")) }

impl Config {
    pub fn carregar() -> Self { arquivo().map(|a| Self::carregar_de(&a)).unwrap_or_default() }

    /// Chave desconhecida ou valor estranho fica no padrão: um arquivo velho ou mexido à mão não quebra a abertura.
    fn carregar_de(a: &Path) -> Self {
        let mut c = Self::default();
        for linha in std::fs::read_to_string(a).unwrap_or_default().lines() {
            let Some((k, v)) = linha.split_once('=') else { continue };
            match (k.trim(), v.trim()) {
                ("tema", "claro") => c.tema = Tema::Claro,
                ("tema", "escuro") => c.tema = Tema::Escuro,
                ("zoom", "largura") => c.zoom = ZoomInicial::Largura,
                ("zoom", "inteira") => c.zoom = ZoomInicial::Inteira,
                ("zoom", "100") => c.zoom = ZoomInicial::Cem,
                ("ferramentas", v) => c.ferramentas = v != "0",
                ("faixa", v) => c.faixa = v != "0",
                ("sites", v) => c.abrir_sites = v != "0",
                _ => {}
            }
        }
        c
    }

    fn texto(&self) -> String {
        let tema = match self.tema { Tema::Automatico => "auto", Tema::Claro => "claro", Tema::Escuro => "escuro" };
        let zoom = match self.zoom { ZoomInicial::Largura => "largura", ZoomInicial::Inteira => "inteira", ZoomInicial::Cem => "100", ZoomInicial::Manter => "manter" };
        format!("tema={tema}\nzoom={zoom}\nferramentas={}\nfaixa={}\nsites={}\n", self.ferramentas as u8, self.faixa as u8, self.abrir_sites as u8)
    }

    /// Falha ao gravar não atrapalha o uso: só a preferência não volta na próxima abertura.
    pub fn gravar(&self) {
        if cfg!(test) { return; }
        if let Some(a) = arquivo() {
            let _ = a.parent().map(std::fs::create_dir_all);
            let _ = featherpdf_io::gravar_atomico(&a, self.texto().as_bytes());
        }
    }

    /// O tema escuro vale agora? (Automático segue o sistema.)
    pub fn escuro(&self) -> bool {
        match self.tema { Tema::Automatico => tema::escuro_resolvido(true, false), Tema::Claro => false, Tema::Escuro => true }
    }
}

impl App {
    pub fn update_config(&mut self, m: Cf) -> Task<Msg> {
        match m {
            Cf::Tema(t) => {
                self.config.tema = t;
                self.tema_escuro = self.config.escuro();
                tema::definir_tema(self.tema_escuro, crate::ACENTO_PDF);
            }
            Cf::Zoom(z) => self.config.zoom = z,
            Cf::AbrirSites(v) => self.config.abrir_sites = v,
            Cf::LimparRecentes => { self.recentes.limpar(); return Task::none(); }
            Cf::LeitorPadrao => {
                self.leitor_padrao = Some(crate::leitor_padrao::pedir().unwrap_or_else(|e| e));
                return Task::none();
            }
        }
        self.config.gravar();
        Task::none()
    }

    /// O zoom escolhido para quando um documento abre.
    pub fn zoom_ao_abrir(&mut self) -> Task<Msg> {
        match self.config.zoom {
            ZoomInicial::Largura => self.update(Msg::AjustarLargura),
            ZoomInicial::Inteira => self.update(Msg::AjustarPagina),
            ZoomInicial::Cem => self.update(Msg::Zoom(1.0)),
            ZoomInicial::Manter => Task::none(),
        }
    }

    /// Mesmo layout das Configurações do FeatherSheet e do FeatherDocs (`feather_ui::configuracoes`).
    pub fn view_configuracoes(&self, id: WindowId) -> Element<'_, Msg> {
        use feather_ui::configuracoes::{linha, menu, tela};
        let secoes = ["Aparência", "Exibição", "Documentos", "Atalhos", "Sobre"];
        let muted = tema::COR_TEXTO_MUTED();
        // Como no FeatherDocs: a opção escolhida fica acesa, e nenhuma fica desativada (desativada some no tema claro).
        let opcao = |nome: &'static str, ativo: bool, msg: Msg| button(text(nome).size(12.5)).padding([6u16, 13])
            .style(tema::estilo_botao_pequeno(ativo)).on_press(msg);
        let c = &self.config;
        let corpo: Element<'_, Msg> = match self.config_secao {
            0 => column![
                linha("Tema", "Automático segue a aparência do sistema na abertura.", row![
                    opcao("Automático", c.tema == Tema::Automatico, Msg::Config(Cf::Tema(Tema::Automatico))),
                    opcao("Claro", c.tema == Tema::Claro, Msg::Config(Cf::Tema(Tema::Claro))),
                    opcao("Escuro", c.tema == Tema::Escuro, Msg::Config(Cf::Tema(Tema::Escuro))),
                ].spacing(4)),
                linha("Página escura", "Inverte o papel e o texto na tela, sem mudar o arquivo. Só nesta sessão.",
                    toggler("", self.modo_escuro).on_toggle(|_| Msg::ModoEscuro)),
            ].into(),
            1 => column![
                linha("Zoom ao abrir", "Como a página aparece quando um PDF é aberto.", row![
                    opcao("Largura", c.zoom == ZoomInicial::Largura, Msg::Config(Cf::Zoom(ZoomInicial::Largura))),
                    opcao("Inteira", c.zoom == ZoomInicial::Inteira, Msg::Config(Cf::Zoom(ZoomInicial::Inteira))),
                    opcao("100%", c.zoom == ZoomInicial::Cem, Msg::Config(Cf::Zoom(ZoomInicial::Cem))),
                    opcao("Manter", c.zoom == ZoomInicial::Manter, Msg::Config(Cf::Zoom(ZoomInicial::Manter))),
                ].spacing(4)),
                linha("Painel Ferramentas", "A lista de ferramentas à esquerda. Fica como você deixar.",
                    toggler("", self.ferramentas_abertas).on_toggle(|_| Msg::AlternarFerramentas)),
                linha("Faixa de opções", "Ctrl+F1 mostra ou recolhe a faixa. Fica como você deixar.",
                    toggler("", self.faixa_aberta).on_toggle(|_| Msg::AlternarFaixa)),
            ].into(),
            2 => {
                column![
                    linha("Leitor de PDF padrão", "Dois cliques num PDF abrem no FeatherPDF.", row![
                        button(text("Tornar padrão").size(12.5))
                            .padding([6u16, 13]).style(tema::estilo_botao_pequeno(false))
                            .on_press(Msg::Config(Cf::LeitorPadrao)),
                        text(self.leitor_padrao.clone().unwrap_or_default()).size(12.5).color(muted),
                    ].spacing(10).align_y(Align::Center)),
                    linha("PDFs recentes", "A lista da tela inicial e do menu Arquivo.", row![
                        text(format!("{} na lista", self.recentes.itens.len())).size(12.5).color(muted),
                        button(text("Limpar lista").size(12.5)).padding([6u16, 13]).style(tema::estilo_botao_pequeno(false))
                            .on_press_maybe((!self.recentes.itens.is_empty()).then_some(Msg::Config(Cf::LimparRecentes))),
                    ].spacing(10).align_y(Align::Center)),
                    linha("Links de sites", "Clicar num link do PDF abre o navegador. Desligado, só mostra o endereço (para PDF de origem duvidosa).",
                        toggler("", c.abrir_sites).on_toggle(|v| Msg::Config(Cf::AbrirSites(v)))),
                    linha("Motor de visualização", "Desenha as páginas na tela e nas miniaturas, sem PDFium nem MuPDF.",
                        text("Hayro (Rust puro)").size(12.5).color(muted)),
                    linha("Gravação", "Salvar grava num arquivo temporário e troca no fim: uma queda no meio não corrompe o PDF.",
                        text("Atômica").size(12.5).color(muted)),
                ].into()
            }
            3 => {
                let atalho = |tecla: &'static str, o_que: &'static str| row![
                    container(text(tecla).size(12)).width(150), text(o_que).size(12.5)].spacing(12);
                column![
                    text("Atalhos de teclado").size(17),
                    atalho("Ctrl+N", "Novo PDF"), atalho("Ctrl+O", "Abrir"), atalho("Ctrl+S", "Salvar"),
                    atalho("Ctrl+Shift+S", "Salvar como"), atalho("Ctrl+P", "Imprimir"), atalho("Ctrl+W", "Fechar documento"),
                    atalho("Ctrl+Z / Ctrl+Y", "Desfazer / refazer"), atalho("Ctrl+K", "Pesquisar comandos"),
                    atalho("Ctrl++ / Ctrl+-", "Aumentar / diminuir zoom"), atalho("PageUp / PageDown", "Página anterior / seguinte"),
                    atalho("Ctrl+F1", "Mostrar ou recolher a faixa"), atalho("F11", "Tela cheia"),
                    atalho("F1", "Ajuda"), atalho("F12", "Monitor e inspetor"),
                ].spacing(8).into()
            }
            _ => column![
                text(format!("FeatherPDF {}", env!("CARGO_PKG_VERSION"))).size(22),
                text("Leitor e editor de PDF nativo da suíte FeatherWorks, em Rust.").size(12.5),
                text("Os arquivos são abertos e processados no computador: sem conta, sem telemetria, sem envio de conteúdo. O OCR roda localmente.").size(12.5),
                text("Tema, botões, menus e esta janela vêm da biblioteca feather-ui.").size(12).color(muted),
                feather_ui::creditos::colaboradores(),
            ].spacing(12).into(),
        };
        let menu = menu("Configurações", secoes.iter().enumerate()
            .map(|(i, nome)| (nome.to_string(), i == self.config_secao, Msg::SecaoConfig(i))));
        cromo::janela_com_barra("Configurações · FeatherPDF",
            tela(menu, secoes[self.config_secao.min(secoes.len() - 1)], corpo, None), Self::acoes_janela(id))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn grava_e_le_de_volta() {
        let a = std::env::temp_dir().join(format!("featherpdf-config-{}.txt", std::process::id()));
        let c = Config { tema: Tema::Escuro, zoom: ZoomInicial::Largura, ferramentas: false, faixa: false, abrir_sites: false };
        std::fs::write(&a, c.texto()).unwrap();
        assert_eq!(Config::carregar_de(&a), c);
        // Arquivo estranho: o que não se entende fica no padrão.
        std::fs::write(&a, "tema=roxo\nlingua_ocr=99\nprocurar_atualizacoes=1\nlixo\nfaixa=0\n").unwrap();
        assert_eq!(Config::carregar_de(&a), Config { faixa: false, ..Config::default() });
        let _ = std::fs::remove_file(&a);
        assert_eq!(Config::carregar_de(&a), Config::default());
    }
}
