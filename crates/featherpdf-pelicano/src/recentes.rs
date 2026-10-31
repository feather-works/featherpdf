//! PDFs abertos recentemente: caminho, número de páginas e quando foi aberto.
//! Um arquivo de texto por usuário (`recentes.tsv` na pasta de configuração).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAXIMO: usize = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Recente {
    pub caminho: PathBuf,
    pub paginas: u32,
    /// Segundos desde 1970 (UTC).
    pub aberto_em: u64,
}

#[derive(Debug, Default)]
pub struct Recentes {
    pub itens: Vec<Recente>,
}

pub(crate) fn pasta_config() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("FeatherPDF"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("featherpdf"))
    }
}

// Testes que salvam PDFs não podem encher a lista real do usuário.
fn arquivo() -> Option<PathBuf> { if cfg!(test) { None } else { pasta_config().map(|p| p.join("recentes.tsv")) } }

pub fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Recentes {
    pub fn carregar() -> Self {
        let texto = arquivo().and_then(|a| std::fs::read_to_string(a).ok()).unwrap_or_default();
        Self::de_texto(&texto)
    }

    fn de_texto(texto: &str) -> Self {
        let itens = texto.lines().filter_map(|linha| {
            let mut partes = linha.split('\t');
            let caminho = PathBuf::from(partes.next()?);
            let paginas = partes.next()?.parse().ok()?;
            let aberto_em = partes.next()?.parse().ok()?;
            Some(Recente { caminho, paginas, aberto_em })
        }).take(MAXIMO).collect();
        Self { itens }
    }

    fn para_texto(&self) -> String {
        self.itens.iter()
            .map(|r| format!("{}\t{}\t{}\n", r.caminho.display(), r.paginas, r.aberto_em))
            .collect()
    }

    /// Esvazia a lista e grava.
    pub fn limpar(&mut self) {
        self.itens.clear();
        if let Some(a) = arquivo() { let _ = featherpdf_io::gravar_atomico(&a, b""); }
    }

    /// Põe o arquivo no topo da lista e grava. Falha ao gravar não é erro de quem abriu.
    pub fn registrar(&mut self, caminho: &Path, paginas: u32) {
        self.itens.retain(|r| r.caminho != caminho);
        self.itens.insert(0, Recente { caminho: caminho.to_path_buf(), paginas, aberto_em: agora() });
        self.itens.truncate(MAXIMO);
        if let Some(a) = arquivo() {
            let _ = a.parent().map(std::fs::create_dir_all);
            let _ = featherpdf_io::gravar_atomico(&a, self.para_texto().as_bytes());
        }
    }
}

/// "hoje", "ontem", "há 3 dias", "16/10" ou "16/10/2025".
/// shortcut: conta dias em UTC (sem dependência de fuso); perto da meia-noite
/// local pode dizer "ontem" errado — trocar quando a lista mostrar a hora.
pub fn quando(aberto_em: u64, agora: u64) -> String {
    let dias_agora = agora / 86_400;
    let dias = aberto_em / 86_400;
    match dias_agora.saturating_sub(dias) {
        0 => "hoje".into(),
        1 => "ontem".into(),
        n if n < 7 => format!("há {n} dias"),
        _ => {
            let (a, m, d) = data_civil(dias as i64);
            let (a_agora, _, _) = data_civil(dias_agora as i64);
            if a == a_agora { format!("{d:02}/{m:02}") } else { format!("{d:02}/{m:02}/{a}") }
        }
    }
}

/// Dia desde 1970 para (ano, mês, dia) — algoritmo de Howard Hinnant.
fn data_civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn texto_ida_e_volta_e_linha_quebrada_ignorada() {
        let r = Recentes::de_texto("/a/x.pdf\t3\t100\nlixo\n/b/y.pdf\t1\t50\n");
        assert_eq!(r.itens.len(), 2);
        assert_eq!(Recentes::de_texto(&r.para_texto()).itens, r.itens);
    }

    #[test]
    fn quando_em_dias() {
        let dia = 86_400;
        assert_eq!(quando(10 * dia + 5, 10 * dia + 9), "hoje");
        assert_eq!(quando(9 * dia, 10 * dia), "ontem");
        assert_eq!(data_civil(20_380), (2025, 10, 19));
        assert_eq!(quando(20_380 * dia, 20_400 * dia), "19/10");
    }
}
