//! PDFs abertos recentemente: caminho, número de páginas, quando foi aberto e a página onde parou.
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
    /// Página onde a pessoa estava ao sair do documento (1 = começo).
    pub pagina: u32,
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

/// Pasta dos arquivos de passagem (imprimir, levar ao FeatherDocs ou ao FeatherSheet), só do usuário:
/// o /tmp é de todos do computador, e esses arquivos levam o conteúdo do PDF (sem a senha dele).
pub(crate) fn pasta_temporaria() -> std::io::Result<PathBuf> {
    let pasta = pasta_config().map_or_else(|| std::env::temp_dir().join("featherpdf"), |p| p.join("temporarios"));
    std::fs::create_dir_all(&pasta)?;
    // Falha (e nada é gravado) se a pasta for de outro usuário.
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o700))?; }
    Ok(pasta)
}

// Testes que salvam PDFs não podem encher a lista real do usuário.
fn arquivo() -> Option<PathBuf> { if cfg!(test) { None } else { pasta_config().map(|p| p.join("recentes.tsv")) } }

pub fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Marca do caminho escrito com %XX: só o que tem TAB ou quebra de linha (o resto fica como
/// sempre foi, e as listas antigas, com `\` do Windows, continuam valendo).
const CODIFICADO: char = '\u{1}';

fn caminho_para_texto(c: &Path) -> String {
    let t = c.display().to_string();
    if !t.contains(['\t', '\n', '\r']) { return t; }
    let mut s = String::from(CODIFICADO);
    for ch in t.chars() {
        match ch { '%' => s.push_str("%25"), '\t' => s.push_str("%09"), '\n' => s.push_str("%0A"), '\r' => s.push_str("%0D"), c => s.push(c) }
    }
    s
}

fn caminho_do_texto(t: &str) -> String {
    let Some(resto) = t.strip_prefix(CODIFICADO) else { return t.to_owned() };
    resto.replace("%09", "\t").replace("%0A", "\n").replace("%0D", "\r").replace("%25", "%")
}

impl Recentes {
    pub fn carregar() -> Self {
        let texto = arquivo().and_then(|a| std::fs::read_to_string(a).ok()).unwrap_or_default();
        Self::de_texto(&texto)
    }

    fn de_texto(texto: &str) -> Self {
        let itens = texto.lines().filter_map(|linha| {
            let mut partes = linha.split('\t');
            let caminho = PathBuf::from(caminho_do_texto(partes.next()?));
            let paginas = partes.next()?.parse().ok()?;
            let aberto_em = partes.next()?.parse().ok()?;
            // Coluna nova: listas gravadas antes dela voltam para o começo.
            let pagina = partes.next().and_then(|p| p.parse().ok()).unwrap_or(1);
            Some(Recente { caminho, paginas, aberto_em, pagina })
        }).take(MAXIMO).collect();
        Self { itens }
    }

    fn para_texto(&self) -> String {
        self.itens.iter()
            .map(|r| format!("{}\t{}\t{}\t{}\n", caminho_para_texto(&r.caminho), r.paginas, r.aberto_em, r.pagina))
            .collect()
    }

    /// Esvazia a lista e grava.
    pub fn limpar(&mut self) {
        self.itens.clear();
        if let Some(a) = arquivo() { let _ = featherpdf_io::gravar_atomico(&a, b""); }
    }

    /// Põe o arquivo no topo da lista e grava. Falha ao gravar não é erro de quem abriu.
    pub fn registrar(&mut self, caminho: &Path, paginas: u32) {
        let pagina = self.pagina(caminho).filter(|&p| p <= paginas).unwrap_or(1);
        self.itens.retain(|r| r.caminho != caminho);
        self.itens.insert(0, Recente { caminho: caminho.to_path_buf(), paginas, aberto_em: agora(), pagina });
        self.itens.truncate(MAXIMO);
        self.gravar();
    }

    /// Onde a pessoa parou neste arquivo, se ele está na lista.
    pub fn pagina(&self, caminho: &Path) -> Option<u32> {
        self.itens.iter().find(|r| r.caminho == caminho).map(|r| r.pagina)
    }

    /// Guarda a página onde a pessoa saiu do documento (só grava se mudou).
    pub fn lembrar(&mut self, caminho: &Path, pagina: u32) {
        let Some(r) = self.itens.iter_mut().find(|r| r.caminho == caminho) else { return };
        if r.pagina == pagina { return; }
        r.pagina = pagina;
        self.gravar();
    }

    fn gravar(&self) {
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
        let r = Recentes::de_texto("/a/x.pdf\t3\t100\t2\nlixo\n/b/y.pdf\t1\t50\n");
        assert_eq!(r.itens.len(), 2);
        assert_eq!((r.itens[0].pagina, r.itens[1].pagina), (2, 1), "sem a coluna, começa na 1");
        assert_eq!(Recentes::de_texto(&r.para_texto()).itens, r.itens);
    }

    #[test]
    fn caminho_com_tab_ou_quebra_de_linha_volta_inteiro() {
        let mut r = Recentes::default();
        for c in ["/a/um\tdois.pdf", "/a/linha\nnova 100%.pdf", "C:\\Users\\new\\x.pdf"] { r.registrar(Path::new(c), 2); }
        let volta = Recentes::de_texto(&r.para_texto());
        assert_eq!(volta.itens, r.itens);
        assert_eq!(volta.itens.len(), 3);
    }

    #[test]
    fn lembra_a_pagina_e_esquece_se_o_arquivo_encolheu() {
        let mut r = Recentes::default();
        let c = Path::new("/a/x.pdf");
        r.registrar(c, 10);
        r.lembrar(c, 7);
        r.registrar(c, 10);
        assert_eq!(r.pagina(c), Some(7));
        r.registrar(c, 5);
        assert_eq!(r.pagina(c), Some(1));
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
