//! Configurações ▸ "Tornar leitor padrão de PDF". Quem decide é o sistema: no Linux o `xdg-mime`
//! grava a escolha da pessoa; o Windows não deixa programa nenhum se pôr como padrão, então abre a
//! tela de Aplicativos padrão já no FeatherPDF (o instalador o registra em RegisteredApplications).

/// O `.desktop` com que o FeatherPDF foi instalado (o snap renomeia), se o `xdg-mime` alcança.
fn desktop() -> Option<&'static str> {
    if std::env::var_os("FLATPAK_ID").is_some() { return None; }
    Some(if std::env::var_os("SNAP").is_some() { "featherpdf_featherpdf.desktop" } else { "featherpdf.desktop" })
}

fn xdg(args: &[&str]) -> Option<String> {
    let saida = std::process::Command::new("xdg-mime").args(args).output().ok()?;
    saida.status.success().then(|| String::from_utf8_lossy(&saida.stdout).trim().to_owned())
}

/// O FeatherPDF já abre os PDFs? `None` quando não dá para saber (Windows, macOS, Flatpak).
fn eh_padrao() -> Option<bool> {
    if !cfg!(target_os = "linux") { return None; }
    Some(xdg(&["query", "default", "application/pdf"])? == desktop()?)
}

/// Pede ao sistema e devolve o recado para a tela.
pub fn pedir() -> Result<String, String> {
    if cfg!(target_os = "windows") {
        std::process::Command::new("explorer.exe").arg("ms-settings:defaultapps?registeredAppMachine=FeatherPDF")
            .spawn().map_err(|e| format!("Não abriu as configurações do Windows: {e}"))?;
        return Ok("Na janela do Windows, escolha o FeatherPDF para .pdf.".into());
    }
    if cfg!(target_os = "macos") {
        return Err("No macOS: botão direito num PDF ▸ Obter Informações ▸ Abrir com: FeatherPDF ▸ Alterar Tudo.".into());
    }
    let Some(arquivo) = desktop() else {
        return Err("No Flatpak a escolha é do sistema: Configurações ▸ Aplicativos padrão ▸ PDF.".into());
    };
    xdg(&["default", arquivo, "application/pdf"]).ok_or("O xdg-mime não está disponível neste sistema.")?;
    match eh_padrao() {
        Some(true) => Ok("Pronto: os PDFs passam a abrir no FeatherPDF.".into()),
        _ => Err("O sistema não aceitou a troca. Escolha em Configurações ▸ Aplicativos padrão.".into()),
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn nome_do_desktop_segue_o_pacote() {
        // Neste ambiente de teste não há SNAP nem FLATPAK_ID.
        if std::env::var_os("SNAP").is_none() && std::env::var_os("FLATPAK_ID").is_none() {
            assert_eq!(super::desktop(), Some("featherpdf.desktop"));
        }
    }
}
