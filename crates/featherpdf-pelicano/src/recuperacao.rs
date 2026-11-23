//! Recuperação automática: de minuto em minuto, cada documento com alteração por salvar ganha uma
//! cópia em `~/.config/featherpdf/recuperacao` (só do usuário). Salvar ou fechar apaga a cópia;
//! se o app cair, a próxima abertura traz os documentos de volta como "não salvos".

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use featherpdf_core::DocumentoPdf;

// Testes não podem tocar na pasta real do usuário.
fn pasta() -> Option<PathBuf> {
    if cfg!(test) { None } else { crate::recentes::pasta_config().map(|p| p.join("recuperacao")) }
}

/// O nome que o documento mostra (arquivo de origem ou o sugerido), para a cópia e a volta.
fn nome_do(pdf: &DocumentoPdf) -> String {
    pdf.origem().and_then(|c| c.file_name()).map(|n| n.to_string_lossy().into_owned())
        .or_else(|| pdf.nome_sugerido().map(String::from))
        .unwrap_or_else(|| "documento.pdf".into())
}

/// Grava a cópia de cada documento e apaga as que não valem mais (salvos ou fechados).
pub fn gravar(docs: Vec<(u64, DocumentoPdf)>) {
    if let Some(p) = pasta() {
        if let Err(e) = gravar_em(&p, docs) { crate::monitor::anotar(format!("[recuperação] {e}")); }
    }
}

pub(crate) fn gravar_em(pasta: &Path, docs: Vec<(u64, DocumentoPdf)>) -> Result<(), String> {
    std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    // A cópia leva o conteúdo do PDF: a pasta é só do usuário, como a dos temporários.
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?; }
    let mut manter = HashSet::new();
    for (id, pdf) in docs {
        // shortcut: PDF com senha fica sem cópia (sem a senha ela exporia o conteúdo; com ela, reabrir pede a senha); fazer se pedirem.
        if pdf.tem_senha() { continue; }
        let nome = format!("{id}-{}", nome_do(&pdf));
        let bytes = pdf.bytes_para_gravar()?;
        let temporario = pasta.join(format!(".{nome}.tmp"));
        std::fs::write(&temporario, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&temporario, pasta.join(&nome)).map_err(|e| e.to_string())?;
        manter.insert(nome);
    }
    for entrada in std::fs::read_dir(pasta).map_err(|e| e.to_string())?.flatten() {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if !manter.contains(&nome) && !nome.ends_with(".tmp") { let _ = std::fs::remove_file(entrada.path()); }
    }
    Ok(())
}

/// Apaga a cópia de um documento (salvo ou fechado sem salvar).
pub fn apagar(id: u64) {
    let Some(p) = pasta() else { return };
    let prefixo = format!("{id}-");
    for entrada in std::fs::read_dir(p).into_iter().flatten().flatten() {
        if entrada.file_name().to_string_lossy().starts_with(&prefixo) { let _ = std::fs::remove_file(entrada.path()); }
    }
}

/// Fechar a janela de propósito: nada a recuperar.
pub fn apagar_tudo() {
    if let Some(p) = pasta() { let _ = std::fs::remove_dir_all(p); }
}

/// As cópias que uma execução que caiu deixou, como documentos sem arquivo. Lidas, são apagadas:
/// os documentos voltam com alteração por salvar e ganham cópia nova no minuto seguinte.
pub fn recolher() -> Vec<DocumentoPdf> {
    pasta().map(|p| recolher_de(&p)).unwrap_or_default()
}

pub(crate) fn recolher_de(pasta: &Path) -> Vec<DocumentoPdf> {
    let mut entradas: Vec<_> = std::fs::read_dir(pasta).into_iter().flatten().flatten().map(|e| e.path()).collect();
    entradas.sort();
    let mut docs = Vec::new();
    for caminho in entradas {
        let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if nome.ends_with(".tmp") { let _ = std::fs::remove_file(&caminho); continue; }
        let Ok(bytes) = std::fs::read(&caminho) else { continue };
        // Uma cópia ilegível (gravação cortada) fica na pasta para quem quiser olhar.
        let Ok(mut pdf) = DocumentoPdf::de_bytes(&bytes, None) else { continue };
        let original = nome.split_once('-').map_or(nome.as_str(), |(_, resto)| resto);
        let base = original.strip_suffix(".pdf").or_else(|| original.strip_suffix(".PDF")).unwrap_or(original);
        pdf.sugerir_nome(format!("{base} (recuperado).pdf"));
        docs.push(pdf);
        let _ = std::fs::remove_file(&caminho);
    }
    docs
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pdf_de_teste() -> DocumentoPdf {
        let bytes = featherpdf_io::criar_a4().unwrap().serializar().unwrap();
        DocumentoPdf::de_bytes(&bytes, Some(PathBuf::from("/tmp/relatorio.pdf"))).unwrap()
    }

    #[test]
    fn copia_volta_com_o_nome_e_some_quando_o_documento_sai() {
        let pasta = std::env::temp_dir().join(format!("featherpdf-recuperacao-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        gravar_em(&pasta, vec![(7, pdf_de_teste())]).unwrap();
        assert!(pasta.join("7-relatorio.pdf").exists());
        // O documento 7 foi salvo ou fechado: a cópia some no tique seguinte.
        gravar_em(&pasta, vec![]).unwrap();
        assert!(!pasta.join("7-relatorio.pdf").exists());
        gravar_em(&pasta, vec![(3, pdf_de_teste())]).unwrap();
        let docs = recolher_de(&pasta);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].nome_sugerido(), Some("relatorio (recuperado).pdf"));
        assert!(docs[0].modificado() && docs[0].origem().is_none(), "volta como não salvo");
        assert!(recolher_de(&pasta).is_empty(), "lida uma vez, a cópia é apagada");
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
