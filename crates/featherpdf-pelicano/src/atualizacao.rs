//! Procurar atualizações, como no FeatherSheet: ao abrir (no máximo uma vez por dia, desligável
//! em Configurações → Sobre) e quando a pessoa clica. Havendo versão nova, um cartão no canto
//! oferece Novidades / Depois / Atualizar.
//!
//! Privacidade: a consulta só **lê** do GitHub qual é a versão mais nova
//! (`api.github.com/repos/feather-works/featherpdf/releases`); nada da pessoa ou dos PDFs é
//! enviado. Quem consulta e baixa é o `curl` do sistema (Linux e Windows 10/11 já trazem).
//!
//! Instalar acontece **à vista**: o app grava um roteiro comentado e abre o terminal com ele. A
//! pessoa vê cada passo (baixar, conferir o SHA-256 contra o `SHA256SUMS` publicado, instalar por
//! cima) e, no `.deb`, digita a senha no próprio terminal. No Snap a loja atualiza sozinha.

use std::path::Path;
use std::process::Command;

use feather_ui::tema;
use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

pub const REPOSITORIO: &str = "feather-works/featherpdf";
pub const PAGINA_VERSOES: &str = "https://github.com/feather-works/featherpdf/releases";

pub fn versao_atual() -> &'static str { env!("CARGO_PKG_VERSION") }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancamento {
    /// Sem o `v` da tag (`0.1.5`).
    pub versao: String,
    /// Página do release no GitHub.
    pub pagina: String,
}

/// Como o FeatherPDF foi instalado nesta máquina (decide como atualizar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instalacao {
    /// Pacote `.deb` (`/usr/bin/featherpdf`): `sudo apt install` por cima.
    Deb,
    /// `.tar.gz` com `instalar.sh` (`~/.local/bin`): troca sem senha.
    PastaDoUsuario,
    /// Instalador do Windows (`Arquivos de Programas\FeatherWorks\FeatherPDF`).
    Windows,
    /// Compilado à mão, zip portátil ou outro lugar: só o link da página de versões.
    Desconhecida,
}

impl Instalacao {
    /// Nome do arquivo publicado no release para esta instalação.
    pub fn arquivo(self) -> Option<&'static str> {
        match self {
            Self::Deb => Some("featherpdf-linux-amd64.deb"),
            Self::PastaDoUsuario => Some("featherpdf-linux-x86_64.tar.gz"),
            Self::Windows => Some("FeatherPDF-windows-x64.exe"),
            Self::Desconhecida => None,
        }
    }

    pub fn descricao(self) -> &'static str {
        match self {
            Self::Deb => "Instalado pelo pacote .deb",
            Self::PastaDoUsuario => "Instalado na pasta do usuário (.tar.gz)",
            Self::Windows => "Instalado pelo instalador do Windows",
            Self::Desconhecida => "Cópia compilada, portátil ou fora da pasta de instalação",
        }
    }
}

/// Descobre a instalação pelo caminho do executável (`casa` = pasta do usuário). Só compara texto.
pub fn instalacao_de(exe: &Path, casa: Option<&Path>) -> Instalacao {
    let texto = exe.to_string_lossy().replace('\\', "/");
    let minusculo = texto.to_lowercase();
    if minusculo.contains("/featherworks/featherpdf/") && minusculo.ends_with(".exe") { return Instalacao::Windows; }
    if texto == "/usr/bin/featherpdf" { return Instalacao::Deb; }
    if casa.is_some_and(|c| exe == c.join(".local/bin/featherpdf")) { return Instalacao::PastaDoUsuario; }
    Instalacao::Desconhecida
}

pub fn instalacao_atual() -> Instalacao {
    let casa = std::env::var_os("HOME").map(std::path::PathBuf::from);
    std::env::current_exe().map_or(Instalacao::Desconhecida, |e| instalacao_de(&e, casa.as_deref()))
}

/// `"v0.1.10"` → `(0, 1, 10)`; aceita com ou sem `v` e ignora sufixos (`-alpha`).
fn numeros(versao: &str) -> Option<(u32, u32, u32)> {
    let v = versao.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut p = v.split('.').map(|x| x.parse::<u32>().ok());
    Some((p.next()??, p.next().flatten().unwrap_or(0), p.next().flatten().unwrap_or(0)))
}

/// `outra` é mais nova que `atual`?
pub fn mais_nova(atual: &str, outra: &str) -> bool {
    matches!((numeros(atual), numeros(outra)), (Some(a), Some(b)) if b > a)
}

/// A versão mais nova da lista `releases` (pré-lançamentos inclusive; rascunhos não).
pub fn ler_lista(json: &str) -> Option<Lancamento> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.as_array()?.iter()
        .filter(|r| !r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?;
            numeros(tag)?;
            // A versão entra no roteiro (bash/PowerShell) sem aspas: só letras, dígitos, ponto e hífen.
            if !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') { return None; }
            Some(Lancamento {
                versao: tag.trim_start_matches(['v', 'V']).to_owned(),
                pagina: r.get("html_url").and_then(|b| b.as_str()).unwrap_or(PAGINA_VERSOES).to_owned(),
            })
        })
        .max_by_key(|l| numeros(&l.versao))
}

fn curl() -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut c = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    c
}

/// Consulta a versão mais nova publicada (bloqueia; chamar fora da thread da janela).
pub fn consultar() -> Result<Lancamento, String> {
    let saida = curl()
        .args(["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", "-H"])
        .arg(format!("User-Agent: FeatherPDF/{}", versao_atual()))
        .arg(format!("https://api.github.com/repos/{REPOSITORIO}/releases?per_page=15"))
        .output()
        .map_err(|_| "Não encontrei o programa curl neste sistema para consultar o GitHub.".to_owned())?;
    if !saida.status.success() { return Err("Não consegui falar com o GitHub. Confira a conexão e tente de novo.".into()); }
    ler_lista(&String::from_utf8_lossy(&saida.stdout)).ok_or_else(|| "O GitHub respondeu, mas não reconheci a versão publicada.".into())
}

fn url_arquivo(versao: &str, arquivo: &str) -> String {
    format!("https://github.com/{REPOSITORIO}/releases/download/v{versao}/{arquivo}")
}

/// Roteiro comentado que o terminal executa: (nome do arquivo, conteúdo). `pid` = este processo
/// (o Windows espera o app fechar antes de trocar o executável).
pub fn roteiro(inst: Instalacao, versao: &str, pid: u32) -> Option<(String, String)> {
    let arquivo = inst.arquivo()?;
    let url = url_arquivo(versao, arquivo);
    let somas = url_arquivo(versao, "SHA256SUMS");
    let cabecalho = format!("# Atualização do FeatherPDF para a versão {versao}.\n\
        # Tudo o que acontece está neste arquivo: só baixa do GitHub\n\
        # (github.com/{REPOSITORIO}), confere e instala por cima.\n\
        # Nada sobre você ou seus PDFs é enviado.\n");
    let unix = |passos: &str| format!(
        "#!/bin/bash\n{cabecalho}\npassos() {{\n  set -e\n  pasta=\"$(mktemp -d)\"\n  cd \"$pasta\"\n\
         \n  echo \"== Baixando a versão {versao} do GitHub\"\n  curl -fL --progress-bar -o {arquivo} {url}\n  curl -fsSL -o SHA256SUMS {somas}\n\
         \n  echo \"== Conferindo se o arquivo é exatamente o publicado\"\n  grep \" {arquivo}\\$\" SHA256SUMS | sha256sum -c -\n\
         {passos}}}\n\
         \nif passos; then\n  echo\n  echo \"Pronto: FeatherPDF {versao} instalado. Feche o FeatherPDF e abra de novo.\"\nelse\n  echo\n  echo \"Algo deu errado; a versão instalada continua a mesma.\"\nfi\nread -r -p \"Pressione Enter para fechar esta janela. \" _\n");
    Some(match inst {
        Instalacao::Deb => ("featherpdf-atualizar.sh".into(), unix(&format!(
            "\n  echo \"== Instalando por cima da versão atual (o apt pede sua senha)\"\n  sudo apt install -y \"./{arquivo}\"\n"))),
        Instalacao::PastaDoUsuario => ("featherpdf-atualizar.sh".into(), unix(&format!(
            "\n  echo \"== Trocando os arquivos em ~/.local (sem senha)\"\n  tar xzf {arquivo}\n  \"./featherpdf-{versao}/instalar.sh\"\n"))),
        Instalacao::Windows => ("featherpdf-atualizar.ps1".into(), format!(
            "{cabecalho}\n$ErrorActionPreference = 'Stop'\ntry {{\n\
             \x20 $pasta = Join-Path $env:TEMP 'featherpdf-{versao}'\n  New-Item -ItemType Directory -Force $pasta | Out-Null\n\
             \x20 $instalador = Join-Path $pasta '{arquivo}'\n\
             \n  Write-Host '== Baixando a versão {versao} do GitHub'\n  Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile $instalador\n  Invoke-WebRequest -UseBasicParsing -Uri '{somas}' -OutFile (Join-Path $pasta 'SHA256SUMS')\n\
             \n  Write-Host '== Conferindo se o arquivo é exatamente o publicado'\n  $linha = Select-String -Path (Join-Path $pasta 'SHA256SUMS') -Pattern ' {arquivo}$' | Select-Object -First 1\n  $esperado = $linha.Line.Split(' ')[0].ToLower()\n  $obtido = (Get-FileHash $instalador -Algorithm SHA256).Hash.ToLower()\n  if ($obtido -ne $esperado) {{ throw 'O arquivo baixado não confere com o publicado.' }}\n\
             \n  Write-Host '== Esperando o FeatherPDF fechar (feche a janela dele)'\n  Wait-Process -Id {pid} -ErrorAction SilentlyContinue\n\
             \n  Write-Host '== Instalando por cima da versão atual (o Windows pede permissão)'\n  Start-Process -FilePath $instalador -ArgumentList '/S' -Verb RunAs -Wait\n  Start-Process (Join-Path $env:ProgramFiles 'FeatherWorks\\FeatherPDF\\featherpdf.exe')\n  Write-Host ''\n  Write-Host 'Pronto: FeatherPDF {versao} instalado.'\n\
             }} catch {{\n  Write-Host ''\n  Write-Host \"Algo deu errado; a versão instalada continua a mesma. ($_)\"\n}}\nRead-Host 'Pressione Enter para fechar esta janela'\n")),
        Instalacao::Desconhecida => return None,
    })
}

/// Grava o roteiro numa pasta só deste processo e abre o terminal do sistema com ele.
pub fn abrir_no_terminal(inst: Instalacao, versao: &str) -> Result<(), String> {
    let (nome, texto) = roteiro(inst, versao, std::process::id())
        .ok_or_else(|| "Esta cópia não foi instalada por um pacote: baixe pela página de versões.".to_owned())?;
    // Pasta nova 0700: com nome fixo em /tmp, outro usuário podia trocar o roteiro enquanto o sudo pede a senha.
    let pasta = std::env::temp_dir().join(format!("featherpdf-atualizar-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
    let mut criar = std::fs::DirBuilder::new();
    #[cfg(unix)]
    { use std::os::unix::fs::DirBuilderExt; criar.mode(0o700); }
    criar.create(&pasta).map_err(|e| format!("Não consegui gravar o roteiro: {e}"))?;
    let caminho = pasta.join(nome);
    std::fs::OpenOptions::new().write(true).create_new(true).open(&caminho)
        .and_then(|mut f| std::io::Write::write_all(&mut f, texto.as_bytes()))
        .map_err(|e| format!("Não consegui gravar o roteiro: {e}"))?;
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o755)); }
    let arq = caminho.to_string_lossy().into_owned();
    if inst == Instalacao::Windows {
        let mut c = Command::new("powershell.exe");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &arq]);
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; c.creation_flags(0x0000_0010); }
        return c.spawn().map(|_| ()).map_err(|e| format!("Não consegui abrir o PowerShell: {e}"));
    }
    let tentativas: [(&str, &[&str]); 7] = [
        ("x-terminal-emulator", &["-e", "bash"]), ("gnome-terminal", &["--", "bash"]), ("kgx", &["--", "bash"]),
        ("konsole", &["-e", "bash"]), ("xfce4-terminal", &["-x", "bash"]), ("tilix", &["-e", "bash"]), ("xterm", &["-e", "bash"]),
    ];
    for (prog, args) in tentativas {
        if Command::new(prog).args(args).arg(&arq).spawn().is_ok() { return Ok(()); }
    }
    Err(format!("Não achei um terminal para abrir. Rode à mão: bash {arq}"))
}

pub fn abrir_endereco(url: &str) {
    let _ = if cfg!(windows) { Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(url).spawn() }
        else { Command::new("xdg-open").arg(url).spawn() };
}

/// O que a seção Sobre mostra.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Estado {
    #[default]
    Parado,
    Procurando,
    EmDia,
    Disponivel(Lancamento),
    /// O terminal foi aberto com o roteiro.
    Instalando(String),
    Erro(String),
}

#[derive(Debug, Clone)]
pub enum At {
    Procurar,
    Resultado(Result<Lancamento, String>),
    /// A consulta da abertura: sem conexão ou já na mais nova, fica calada.
    ResultadoSozinho(Result<Lancamento, String>),
    Instalar,
    AbrirPagina,
    /// "Depois": fecha o cartão até a próxima abertura.
    Depois,
    ProcurarAoAbrir(bool),
}

const INTERVALO_CONSULTA_S: u64 = 24 * 60 * 60;

/// A consulta da abertura vale agora? (ligada e há mais de um dia da última).
pub fn consultar_ao_abrir(ligada: bool, ultima_s: u64, agora_s: u64) -> bool {
    ligada && agora_s.saturating_sub(ultima_s) >= INTERVALO_CONSULTA_S
}

fn agora_s() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) }

fn consulta(fim: fn(Result<Lancamento, String>) -> At) -> Task<Msg> {
    Task::perform(async { tokio::task::spawn_blocking(consultar).await.unwrap_or_else(|e| Err(e.to_string())) }, move |r| Msg::Atualizacao(fim(r)))
}

impl App {
    /// Na abertura: consulta o GitHub em segundo plano se a pessoa deixou e já passou um dia.
    pub fn procurar_atualizacao_ao_abrir(&mut self) -> Task<Msg> {
        // Snap e Flatpak: a loja atualiza sozinha.
        let pela_loja = std::env::var_os("SNAP").is_some() || std::env::var_os("FLATPAK_ID").is_some();
        if cfg!(test) || pela_loja || !consultar_ao_abrir(self.config.procurar_atualizacoes, self.config.ultima_procura, agora_s()) {
            return Task::none();
        }
        self.config.ultima_procura = agora_s();
        self.config.gravar();
        consulta(At::ResultadoSozinho)
    }

    /// Cartão no canto de baixo quando há versão nova.
    pub fn cartao_atualizacao(&self) -> Option<Element<'_, Msg>> {
        let l = self.aviso_atualizacao.as_ref()?;
        let botao = |r: &'static str, primario: bool, m: At| button(text(r).size(12)).padding([6u16, 12])
            .style(if primario { tema::estilo_botao_primario } else { tema::estilo_botao_secundario }).on_press(Msg::Atualizacao(m));
        let mut acoes = row![botao("Novidades", false, At::AbrirPagina), Space::with_width(Length::FILL), botao("Depois", false, At::Depois)]
            .spacing(8).align_y(Align::Center);
        if instalacao_atual().arquivo().is_some() {
            acoes = acoes.push(botao(if cfg!(windows) { "Atualizar e reiniciar" } else { "Atualizar" }, true, At::Instalar));
        }
        let corpo = column![
            row![feather_ui::icones::icone("refresh", 18.0), text(format!("FeatherPDF {}", l.versao)).size(14).font(Font::SEMIBOLD)]
                .spacing(8).align_y(Align::Center),
            text(format!("v{} → {}", versao_atual(), l.versao)).size(11.5).color(tema::COR_TEXTO_MUTED()),
            text("Há uma versão nova do FeatherPDF, com correções e novidades.").size(12),
            acoes,
        ].spacing(8);
        let cartao = container(corpo).padding([14u16, 16]).width(360).style(|_| Style {
            background: Some(Background::Color(tema::COR_PAINEL())),
            border: Border { color: tema::COR_AZUL_ACENTO(), width: 1.0, radius: [10.0; 4] },
            ..Default::default()
        });
        Some(container(cartao).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End)
            .padding(Padding { top: 0.0, right: 24.0, bottom: 48.0, left: 0.0 }).into())
    }

    /// A parte de atualizações da seção Sobre.
    pub fn sobre_atualizacao(&self) -> Element<'_, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let inst = instalacao_atual();
        let snap = std::env::var_os("SNAP").is_some();
        let (frase, botao): (String, Option<(&'static str, At)>) = match &self.atualizacao {
            _ if snap => ("Instalado pela Snap Store: a loja atualiza sozinha.".into(), None),
            Estado::Parado => (String::new(), Some(("Procurar atualizações", At::Procurar))),
            Estado::Procurando => ("Procurando…".into(), None),
            Estado::EmDia => ("Você está na versão mais nova.".into(), Some(("Procurar de novo", At::Procurar))),
            Estado::Disponivel(l) if inst.arquivo().is_some() => (format!("Versão {} disponível.", l.versao), Some(("Atualizar", At::Instalar))),
            Estado::Disponivel(l) => (format!("Versão {} disponível: baixe pela página de versões.", l.versao), Some(("Abrir página", At::AbrirPagina))),
            Estado::Instalando(t) | Estado::Erro(t) => (t.clone(), Some(("Procurar de novo", At::Procurar))),
        };
        let mut linha = row![].spacing(10).align_y(Align::Center);
        if let Some((r, m)) = botao {
            linha = linha.push(button(text(r).size(12.5)).padding([6u16, 13]).style(tema::estilo_botao_pequeno(false)).on_press(Msg::Atualizacao(m)));
        }
        linha = linha.push(text(frase).size(12.5));
        column![
            linha,
            text(inst.descricao()).size(12).color(muted),
            checkbox("Procurar atualizações ao abrir (uma vez por dia)", self.config.procurar_atualizacoes)
                .on_toggle(|v| Msg::Atualizacao(At::ProcurarAoAbrir(v))).size(15).text_size(12),
            text("A consulta só lê do GitHub qual é a versão mais nova; nada seu é enviado.").size(11.5).color(muted),
        ].spacing(8).into()
    }

    pub fn update_atualizacao(&mut self, e: At) -> Task<Msg> {
        match e {
            At::Procurar => { self.atualizacao = Estado::Procurando; return consulta(At::Resultado); }
            At::Resultado(Ok(l)) => self.atualizacao = if mais_nova(versao_atual(), &l.versao) { Estado::Disponivel(l) } else { Estado::EmDia },
            At::Resultado(Err(e)) => self.atualizacao = Estado::Erro(e),
            At::ResultadoSozinho(Ok(l)) if mais_nova(versao_atual(), &l.versao) => {
                self.aviso_atualizacao = Some(l.clone());
                self.atualizacao = Estado::Disponivel(l);
            }
            At::ResultadoSozinho(_) => {}
            At::Depois => self.aviso_atualizacao = None,
            At::ProcurarAoAbrir(v) => { self.config.procurar_atualizacoes = v; self.config.gravar(); }
            At::Instalar => {
                let Estado::Disponivel(l) = &self.atualizacao else { return Task::none() };
                let inst = instalacao_atual();
                self.atualizacao = match abrir_no_terminal(inst, &l.versao.clone()) {
                    Ok(()) if inst == Instalacao::Windows => Estado::Instalando("O terminal abriu e está baixando. Feche o FeatherPDF quando ele pedir: a versão nova abre sozinha.".into()),
                    Ok(()) if inst == Instalacao::Deb => Estado::Instalando("O terminal abriu com cada passo à vista. Digite sua senha lá; depois feche e abra o FeatherPDF de novo.".into()),
                    Ok(()) => Estado::Instalando("O terminal abriu com cada passo à vista. Depois feche e abra o FeatherPDF de novo.".into()),
                    Err(e) => Estado::Erro(e),
                };
                self.aviso_atualizacao = None;
                if let Estado::Instalando(t) | Estado::Erro(t) = &self.atualizacao { self.status = t.clone(); }
            }
            At::AbrirPagina => {
                // O endereço vem da API do GitHub: só abre se for página do próprio repositório.
                let url = match &self.atualizacao {
                    Estado::Disponivel(l) if l.pagina.starts_with("https://github.com/feather-works/featherpdf/") => l.pagina.clone(),
                    _ => PAGINA_VERSOES.to_owned(),
                };
                abrir_endereco(&url);
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reconhece_cada_instalacao() {
        let casa = PathBuf::from("/home/ana");
        let i = |p: &str| instalacao_de(Path::new(p), Some(&casa));
        assert_eq!(i("/usr/bin/featherpdf"), Instalacao::Deb);
        assert_eq!(i("/home/ana/.local/bin/featherpdf"), Instalacao::PastaDoUsuario);
        assert_eq!(i(r"C:\Program Files\FeatherWorks\FeatherPDF\featherpdf.exe"), Instalacao::Windows);
        assert_eq!(i(r"C:\Users\Ana\Downloads\FeatherPDF\featherpdf.exe"), Instalacao::Desconhecida, "zip portátil");
        assert_eq!(i("/home/ana/GitHub/featherpdf/target/release/featherpdf"), Instalacao::Desconhecida);
    }

    #[test]
    fn compara_versoes_e_le_a_lista() {
        assert!(mais_nova("0.1.4", "v0.1.5") && mais_nova("0.1.9", "0.1.10") && !mais_nova("0.1.4", "v0.1.4") && !mais_nova("0.1.4", "lixo"));
        let json = r#"[{"tag_name":"v0.1.4","draft":false},{"tag_name":"v0.2.0","draft":true},
            {"tag_name":"v0.1.10","draft":false,"html_url":"https://github.com/feather-works/featherpdf/releases/tag/v0.1.10"},
            {"tag_name":"v9.9.9;rm","draft":false}]"#;
        let l = ler_lista(json).unwrap();
        assert_eq!(l.versao, "0.1.10", "rascunho e tag com ; ficam de fora");
        assert!(l.pagina.ends_with("/v0.1.10"));
        assert!(ler_lista(r#"{"message":"rate limit"}"#).is_none());
        assert!(consultar_ao_abrir(true, 0, 100_000) && !consultar_ao_abrir(true, 100_000, 100_000 + 3600) && !consultar_ao_abrir(false, 0, 100_000));
    }

    #[test]
    fn roteiros_conferem_antes_de_instalar() {
        let (_, deb) = roteiro(Instalacao::Deb, "0.1.5", 42).unwrap();
        let (baixa, confere, instala) = (deb.find("releases/download/v0.1.5/featherpdf-linux-amd64.deb").unwrap(),
            deb.find("sha256sum -c").unwrap(), deb.find("sudo apt install").unwrap());
        assert!(baixa < confere && confere < instala);
        let (_, tar) = roteiro(Instalacao::PastaDoUsuario, "0.1.5", 42).unwrap();
        assert!(tar.contains("./featherpdf-0.1.5/instalar.sh") && !tar.contains("sudo"));
        let (nome, win) = roteiro(Instalacao::Windows, "0.1.5", 42).unwrap();
        assert!(nome.ends_with(".ps1") && win.contains("Wait-Process -Id 42") && win.contains("'/S'"));
        assert!(win.find("Get-FileHash").unwrap() < win.find("Start-Process -FilePath").unwrap());
        assert!(roteiro(Instalacao::Desconhecida, "0.1.5", 42).is_none());
    }
}
