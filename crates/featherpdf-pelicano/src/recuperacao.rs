//! Recuperação automática: de minuto em minuto, cada documento com alteração por salvar ganha uma
//! cópia em `~/.config/featherpdf/recuperacao/<execução>/` (só do usuário). Salvar ou fechar apaga a
//! cópia; se o app cair, a próxima abertura traz os documentos de volta como "não salvos".
//!
//! Cada execução tem a sua pasta, presa por uma trava (`File::try_lock`): duas janelas do app não
//! mexem nas cópias uma da outra, e só se recupera a pasta cuja trava está solta (a execução caiu).

use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use featherpdf_core::DocumentoPdf;

const TRAVA: &str = "trava";

/// A pasta desta execução e a trava aberta (fica presa enquanto o app roda).
static MINHA: Mutex<Option<(PathBuf, File)>> = Mutex::new(None);
/// Pastas de uma execução que caiu, lidas na abertura, com a trava delas presa (outra janela aberta
/// depois não as recupera de novo): só somem depois que as cópias novas existem.
static RECOLHIDAS: Mutex<Vec<(PathBuf, File)>> = Mutex::new(Vec::new());
/// A janela está fechando: uma gravação em fundo que termine depois apaga a pasta em vez de deixá-la.
static FECHANDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Apaga as pastas recolhidas (solta a trava antes: no Windows o arquivo aberto não se apaga).
fn apagar_recolhidas() {
    for (p, trava) in std::mem::take(&mut *RECOLHIDAS.lock().unwrap_or_else(|e| e.into_inner())) {
        drop(trava);
        let _ = std::fs::remove_dir_all(p);
    }
}

/// Nenhum documento segue alterado (todos salvos ou fechados): o que veio da queda já não volta.
pub fn esquecer_recolhidas() { apagar_recolhidas(); }

// Testes não podem tocar na pasta real do usuário.
fn base() -> Option<PathBuf> {
    if cfg!(test) { None } else { crate::recentes::pasta_config().map(|p| p.join("recuperacao")) }
}

/// Cria (uma vez) a pasta desta execução dentro de `base`, só do usuário, com a trava presa.
fn minha_pasta(base: &Path) -> Result<PathBuf, String> {
    let mut minha = MINHA.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, _)) = minha.as_ref() { return Ok(p.clone()); }
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let pasta = base.join(format!("{}-{agora}", std::process::id()));
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    // A cópia leva o conteúdo do PDF: as pastas são só do usuário, como a dos temporários.
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        for p in [base, pasta.as_path()] { std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?; }
    }
    let trava = File::create(pasta.join(TRAVA)).map_err(|e| e.to_string())?;
    trava.try_lock().map_err(|e| e.to_string())?;
    *minha = Some((pasta.clone(), trava));
    Ok(pasta)
}

/// O nome que o documento mostra (arquivo de origem ou o sugerido), para a cópia e a volta.
fn nome_do(pdf: &DocumentoPdf) -> String {
    pdf.origem().and_then(|c| c.file_name()).map(|n| n.to_string_lossy().into_owned())
        .or_else(|| pdf.nome_sugerido().map(String::from))
        .unwrap_or_else(|| "documento.pdf".into())
}

/// Grava a cópia de cada documento e apaga as que não valem mais (salvos ou fechados).
pub fn gravar(docs: Vec<(u64, DocumentoPdf)>) {
    let Some(b) = base() else { return };
    let pasta = minha_pasta(&b);
    let r = pasta.clone().and_then(|p| gravar_em(&p, docs));
    match r {
        // As cópias novas existem: as da execução que caiu já não fazem falta.
        Ok(()) => apagar_recolhidas(),
        Err(e) => crate::monitor::anotar(format!("[recuperação] {e}")),
    }
    // Fechou enquanto esta gravação corria: o que ela deixou não pode voltar na próxima abertura.
    // A pasta é a desta gravação: o `apagar_tudo` da janela já tirou a de `MINHA` (e o remove dele pode ter
    // falhado com o `.tmp` que esta gravação criava).
    if FECHANDO.load(std::sync::atomic::Ordering::Relaxed) {
        if let Ok(p) = pasta { let _ = std::fs::remove_dir_all(p); }
        apagar_tudo();
    }
}

pub(crate) fn gravar_em(pasta: &Path, docs: Vec<(u64, DocumentoPdf)>) -> Result<(), String> {
    let mut manter = HashSet::new();
    let mut erro = None;
    let mut falhou: Vec<String> = Vec::new();
    for (id, pdf) in docs {
        // shortcut: PDF com senha fica sem cópia (sem a senha ela exporia o conteúdo; com ela, reabrir pede a senha); fazer se pedirem.
        if pdf.tem_senha() { continue; }
        // Nome de arquivo tem teto de 255 bytes: o do documento entra cortado (sem partir uma letra).
        let mut original = nome_do(&pdf);
        while original.len() > 180 { original.pop(); }
        let nome = format!("{id}-{original}");
        // Um documento que falha não deixa os outros sem cópia.
        let feito = pdf.bytes_para_gravar().and_then(|bytes| {
            let temporario = pasta.join(format!(".{nome}.tmp"));
            std::fs::write(&temporario, bytes).map_err(|e| e.to_string())?;
            std::fs::rename(&temporario, pasta.join(&nome)).map_err(|e| e.to_string())
        });
        match feito {
            Ok(()) => { manter.insert(nome); }
            // A cópia do minuto anterior fica: sem ela, uma queda agora perderia o documento.
            Err(e) => { falhou.push(format!("{id}-")); erro = Some(e) }
        }
    }
    podar_em(pasta, |nome| manter.contains(nome) || falhou.iter().any(|p| nome.starts_with(p.as_str())));
    erro.map_or(Ok(()), Err)
}

/// Apaga as cópias desta execução que `fica` recusa (a trava e os temporários ficam).
fn podar_em(pasta: &Path, fica: impl Fn(&str) -> bool) {
    for entrada in std::fs::read_dir(pasta).into_iter().flatten().flatten() {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if nome != TRAVA && !nome.ends_with(".tmp") && !fica(&nome) { let _ = std::fs::remove_file(entrada.path()); }
    }
}

/// Depois de uma gravação em fundo: tira as cópias de quem deixou de estar alterado enquanto ela corria
/// (salvo ou fechado no meio, a cópia voltaria a aparecer depois de uma queda).
pub fn podar(alterados: &[u64]) {
    let Some(pasta) = MINHA.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(p, _)| p.clone()) else { return };
    podar_em(&pasta, |nome| nome.split_once('-').and_then(|(id, _)| id.parse::<u64>().ok()).is_some_and(|id| alterados.contains(&id)));
}

/// Apaga a cópia de um documento (salvo ou fechado sem salvar).
pub fn apagar(id: u64) {
    let Some(pasta) = MINHA.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(p, _)| p.clone()) else { return };
    let prefixo = format!("{id}-");
    podar_em(&pasta, |nome| !nome.starts_with(&prefixo));
}

/// Fechar a janela de propósito: nada a recuperar desta execução nem das que ela reabriu.
pub fn apagar_tudo() {
    FECHANDO.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some((p, trava)) = MINHA.lock().unwrap_or_else(|e| e.into_inner()).take() {
        drop(trava); // no Windows o arquivo aberto não se apaga
        let _ = std::fs::remove_dir_all(p);
    }
    apagar_recolhidas();
}

/// As cópias das execuções que caíram, como documentos sem arquivo. As pastas ficam até as cópias
/// novas existirem ([`gravar`]): uma segunda queda logo depois ainda as encontra.
pub fn recolher() -> Vec<DocumentoPdf> {
    let Some(b) = base() else { return Vec::new() };
    let (docs, pastas) = recolher_de(&b);
    RECOLHIDAS.lock().unwrap_or_else(|e| e.into_inner()).extend(pastas);
    docs
}

/// Os documentos das pastas soltas de `base` e essas pastas. A de outra janela aberta (trava presa) fica.
pub(crate) fn recolher_de(base: &Path) -> (Vec<DocumentoPdf>, Vec<(PathBuf, File)>) {
    let mut pastas: Vec<PathBuf> = std::fs::read_dir(base).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    pastas.sort();
    let (mut docs, mut lidas) = (Vec::new(), Vec::new());
    for pasta in pastas {
        // Sem criar a trava: a pasta que ainda não tem uma é de uma janela que está nascendo agora.
        let Ok(trava) = std::fs::OpenOptions::new().write(true).open(pasta.join(TRAVA)) else { continue };
        if trava.try_lock().is_err() { continue; } // outra janela do app está viva
        let mut arquivos: Vec<PathBuf> = std::fs::read_dir(&pasta).into_iter().flatten().flatten().map(|e| e.path()).collect();
        arquivos.sort();
        for caminho in arquivos {
            let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if nome == TRAVA || nome.ends_with(".tmp") { continue; }
            let Ok(bytes) = std::fs::read(&caminho) else { continue };
            let Ok(mut pdf) = DocumentoPdf::de_bytes(&bytes, None) else { continue };
            let original = nome.split_once('-').map_or(nome.as_str(), |(_, resto)| resto);
            let sem_extensao = original.strip_suffix(".pdf").or_else(|| original.strip_suffix(".PDF")).unwrap_or(original);
            pdf.sugerir_nome(format!("{sem_extensao} (recuperado).pdf"));
            docs.push(pdf);
        }
        lidas.push((pasta, trava));
    }
    (docs, lidas)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pdf_de_teste() -> DocumentoPdf {
        let bytes = featherpdf_io::criar_a4().unwrap().serializar().unwrap();
        DocumentoPdf::de_bytes(&bytes, Some(PathBuf::from("/tmp/relatorio.pdf"))).unwrap()
    }

    #[test]
    fn copia_volta_com_o_nome_e_respeita_a_janela_viva() {
        let base = std::env::temp_dir().join(format!("featherpdf-recuperacao-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (viva, caiu) = (base.join("1-viva"), base.join("2-caiu"));
        for p in [&viva, &caiu] { std::fs::create_dir_all(p).unwrap(); }
        // A execução que caiu também tinha a trava dela (já solta).
        drop(File::create(caiu.join(TRAVA)).unwrap());
        let trava_viva = File::create(viva.join(TRAVA)).unwrap();
        trava_viva.try_lock().unwrap();
        gravar_em(&viva, vec![(7, pdf_de_teste())]).unwrap();
        gravar_em(&caiu, vec![(3, pdf_de_teste())]).unwrap();
        // O documento 7 foi salvo ou fechado: a cópia some no tique seguinte, a trava fica.
        gravar_em(&viva, vec![]).unwrap();
        assert!(!viva.join("7-relatorio.pdf").exists() && viva.join(TRAVA).exists());
        gravar_em(&viva, vec![(7, pdf_de_teste())]).unwrap();
        let (docs, pastas) = recolher_de(&base);
        assert_eq!(docs.len(), 1, "a pasta da outra janela viva não é recuperada");
        assert_eq!(docs[0].nome_sugerido(), Some("relatorio (recuperado).pdf"));
        assert!(docs[0].modificado() && docs[0].origem().is_none(), "volta como não salvo");
        assert_eq!(pastas.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(), vec![caiu.clone()]);
        assert_eq!(recolher_de(&base).0.len(), 0, "presa por quem recolheu, outra janela não traz de novo");
        drop(pastas);
        assert!(caiu.join("3-relatorio.pdf").exists(), "lida, a cópia fica até a nova existir");
        drop(trava_viva);
        let _ = std::fs::remove_dir_all(&base);
    }
}
