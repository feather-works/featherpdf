//! Estado do F12. Memória, CPU, console e métricas são do `feather_ui::monitor`,
//! o mesmo do FeatherSheet; aqui o FeatherPDF entrega o que é dele (as mensagens
//! e o tempo das ações no Console, as métricas do documento) e guarda as amostras
//! que o quadro Recursos do inspetor (`inspetor.rs`) mostra.
use pelicano::*;
use super::{App, Auxiliar, Msg};
use feather_ui::monitor::{Metricas, Monitor, Nivel};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Acima disto a thread da janela ficou presa: anota no Console, como o vigia do Sheet.
const PRESA_MS: u128 = 200;
/// Página que levou mais que isto para desenhar entra no Console.
const DESENHO_LENTO_MS: u128 = 300;

/// Linhas deixadas por quem roda fora da janela (as etapas de abrir, numa
/// thread de trabalho); a janela recolhe a cada mensagem.
static DIARIO: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn anotar(linha: String) {
    if let Ok(mut fila) = DIARIO.lock() {
        if fila.len() >= 500 { fila.remove(0); }
        fila.push(linha);
    }
}

fn segundos(d: Duration) -> String {
    format!("{:.2} s", d.as_secs_f64()).replace('.', ",")
}

/// Guarda até 80 caracteres e para de formatar no resto: o `Debug` de uma
/// mensagem com a página desenhada (megabytes de RGBA) não pode ser montado inteiro.
struct Curto(String);

impl std::fmt::Write for Curto {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for c in s.chars() {
            if self.0.chars().count() >= 80 {
                self.0.push('…');
                return Err(std::fmt::Error);
            }
            self.0.push(c);
        }
        Ok(())
    }
}

pub fn nome_curto(v: &impl std::fmt::Debug) -> String {
    let mut curto = Curto(String::new());
    let _ = std::fmt::Write::write_fmt(&mut curto, format_args!("{v:?}"));
    curto.0
}

pub fn tempo(d: Duration) -> String {
    if d.as_millis() < 1000 { format!("{} ms", d.as_millis()) } else { segundos(d) }
}

/// Medidas do processo que só o sistema dá (Linux: `/proc/self`); `None` onde não há.
#[derive(Default)]
pub struct Sistema { pub disco: Option<(u64, u64)> }

fn ler_sistema() -> Sistema {
    let io = std::fs::read_to_string("/proc/self/io").unwrap_or_default();
    let campo_io = |nome: &str| io.lines().find_map(|l| l.strip_prefix(nome)?.trim().parse::<u64>().ok());
    Sistema { disco: campo_io("read_bytes:").zip(campo_io("write_bytes:")) }
}

/// Amostras guardadas para os gráficos pequenos do quadro Recursos.
const AMOSTRAS_RECURSOS: usize = 60;

fn guardar<T>(v: &mut Vec<T>, x: T) {
    v.push(x);
    if v.len() > AMOSTRAS_RECURSOS { v.remove(0); }
}

pub struct EstadoMonitor {
    pub tela: Monitor,
    pub insp: crate::inspetor::Inspetor,
    pub sistema: Sistema,
    /// RAM (KiB) e CPU (%) das últimas amostras.
    pub ram: Vec<u64>,
    pub cpu: Vec<f32>,
    /// Quanto levou a última abertura de arquivo (do "Abrindo…" ao documento na tela).
    pub abrir: Option<Duration>,
    /// Ação em curso: o rótulo e quando começou ("Abrindo x.pdf…", "Comprimir", "OCR"…).
    acao: Option<(String, Instant)>,
    desenhos: u64,
    soma_ms: u128,
    pub ultimo: Option<(u128, u32)>,
    mais_lento: Option<(u128, u32)>,
    pub profundidade: u32,
}

impl Default for EstadoMonitor {
    fn default() -> Self {
        Self { tela: Monitor::novo("FeatherPDF"), insp: Default::default(), sistema: Sistema::default(), ram: Vec::new(), cpu: Vec::new(),
            abrir: None, acao: None, desenhos: 0, soma_ms: 0, ultimo: None, mais_lento: None, profundidade: 0 }
    }
}

impl EstadoMonitor {
    /// Uma vez por segundo, com a janela aberta: memória, CPU, threads e disco.
    pub fn amostrar(&mut self) {
        self.tela.amostrar();
        if self.tela.rss_kb() > 0 { guardar(&mut self.ram, self.tela.rss_kb()); }
        if let Some(c) = self.tela.cpu() { guardar(&mut self.cpu, c); }
        self.sistema = ler_sistema();
    }

    pub fn desenhou(&mut self, pagina: u32, ms: u128) {
        self.desenhos += 1;
        self.soma_ms += ms;
        self.ultimo = Some((ms, pagina));
        if self.mais_lento.is_none_or(|(m, _)| ms > m) { self.mais_lento = Some((ms, pagina)); }
        if ms >= DESENHO_LENTO_MS {
            self.tela.registrar(Nivel::Info, "Desenho", &format!("[desenhar] página {pagina} em {ms} ms"));
        }
    }
}

impl App {
    /// O que está demorando agora, pelo estado das ferramentas (não pelo texto).
    pub(crate) fn trabalho_em_curso(&self) -> Option<&'static str> {
        if self.comprimindo() { Some("Comprimir") }
        else if self.ocr.rodando() { Some("OCR") }
        else if self.exportar.trabalhando { Some("Exportar") }
        else { None }
    }

    /// Depois de cada mensagem: o status novo vai ao Console, e o fim de uma
    /// ação demorada leva junto quanto ela levou ("F12 anota o tempo de cada ação").
    pub(crate) fn observar(&mut self, status_antes: &str, trabalho_antes: Option<&'static str>, duracao: Duration, msg: &str) -> Task<Msg> {
        let revisao = self.monitor.tela.console.revisao();
        for linha in DIARIO.lock().map(|mut f| std::mem::take(&mut *f)).unwrap_or_default() {
            self.monitor.tela.registrar(Nivel::Info, "Arquivo", &linha);
        }
        if duracao.as_millis() >= PRESA_MS {
            self.monitor.tela.registrar(Nivel::Aviso, "Tela", &format!("[tela] presa {} ms: {msg}", duracao.as_millis()));
        }
        let trabalho = self.trabalho_em_curso();
        if trabalho != trabalho_antes {
            if let Some((rotulo, desde)) = self.monitor.acao.take() {
                if trabalho_antes.is_some() {
                    self.monitor.tela.registrar(Nivel::Info, "Ação", &format!("[{}] concluído em {}", rotulo.to_lowercase(), segundos(desde.elapsed())));
                }
            }
            if let Some(t) = trabalho { self.monitor.acao = Some((t.into(), Instant::now())); }
        }
        if self.status != status_antes && !self.status.is_empty() {
            // "Abrindo…", "Lendo…", "Dividindo…": a próxima mensagem fecha a ação e diz o tempo.
            let andamento = self.status.ends_with('…');
            let mut linha = self.status.clone();
            if !andamento && trabalho.is_none() {
                if let Some((rotulo, desde)) = self.monitor.acao.take_if(|(r, _)| r.ends_with('…')) {
                    linha.push_str(&format!(" · {}", segundos(desde.elapsed())));
                    if rotulo.starts_with("Abrindo") { self.monitor.abrir = Some(desde.elapsed()); }
                }
            }
            if andamento && trabalho.is_none() { self.monitor.acao = Some((self.status.clone(), Instant::now())); }
            self.monitor.tela.registrar(Nivel::da_mensagem(&self.status), "Aplicativo", &linha);
        }
        // O inspetor relê o documento quando ele muda (ou fecha), não a cada quadro.
        if self.janelas.aberta(Auxiliar::Monitor)
            && (self.monitor.insp.sujo || self.aberto.is_some() != self.monitor.insp.tem_leitura()) {
            self.reler_inspetor();
        }
        if self.monitor.tela.console.revisao() != revisao { self.monitor.tela.acompanhar() } else { Task::none() }
    }

    /// O que a aba Desempenho mostra ao lado da memória (a antiga aba Métricas).
    pub(crate) fn metricas_monitor(&self) -> Metricas {
        let m = &self.monitor;
        let ms = |v: Option<u128>| v.map(|t| format!("{t} ms")).unwrap_or_else(|| "—".into());
        let media = (m.desenhos > 0).then(|| m.soma_ms / m.desenhos as u128);
        let mut linhas = vec![
            ("Documento".to_string(), self.nome_documento().unwrap_or_else(|| "Nenhum".into())),
            ("Página atual".into(), format!("{} de {}", self.pagina, self.total_paginas())),
            ("Zoom".into(), format!("{:.0}%", self.zoom * 100.0)),
            ("Desenho médio".into(), ms(media)),
            ("Desenho mais lento".into(), m.mais_lento.map(|(t, p)| format!("{t} ms (página {p})")).unwrap_or_else(|| "—".into())),
            ("Páginas sendo desenhadas".into(), self.pedidas.len().to_string()),
            ("Passos de desfazer".into(), self.historico.len().to_string()),
        ];
        if let Some((rotulo, desde)) = &m.acao {
            linhas.push(("Operação".into(), format!("{} · {}", rotulo.trim_end_matches('…'), segundos(desde.elapsed()))));
        } else {
            linhas.push(("Operação".into(), "Pronto".into()));
        }
        Metricas {
            cartoes: vec![
                ("Páginas".into(), self.total_paginas().to_string(), "No documento atual".into()),
                ("Em cache".into(), self.desenhadas.len().to_string(), "Páginas desenhadas na memória".into()),
                ("Último desenho".into(), ms(m.ultimo.map(|u| u.0)), format!("{} páginas desenhadas", m.desenhos)),
            ],
            linhas,
            ocupado: m.acao.as_ref().map(|(r, _)| format!("{} em andamento", r.trim_end_matches('…'))),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn segundos_com_virgula() {
        assert_eq!(segundos(Duration::from_millis(1500)), "1,50 s");
    }

    #[test]
    fn desenho_lento_vai_ao_console_e_o_mais_lento_fica() {
        let mut m = EstadoMonitor::default();
        let antes = m.tela.console.revisao();
        m.desenhou(1, 40);
        assert_eq!(m.tela.console.revisao(), antes);
        m.desenhou(2, 900);
        m.desenhou(3, 100);
        assert_eq!(m.mais_lento, Some((900, 2)));
        assert!(m.tela.console.registros().last().unwrap().mensagem.contains("página 2 em 900 ms"));
    }
}
