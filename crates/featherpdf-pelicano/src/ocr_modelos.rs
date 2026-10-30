//! Modelos extras do OCR, baixados só quando a pessoa escolhe: outras escritas (o latino
//! vem embutido) e o classificador que endireita a página. Ficam na pasta de configuração,
//! conferidos pelo SHA-256 publicado no Hugging Face (monkt/paddleocr-onnx, Apache-2.0).

use std::path::PathBuf;

use sha2::{Digest, Sha256};

const BASE: &str = "https://huggingface.co/monkt/paddleocr-onnx/resolve/main/";

/// Arquivo remoto: caminho no repositório, nome local, SHA-256 e tamanho.
type Arquivo = (&'static str, &'static str, &'static str, u64);

pub struct Modelo {
    pub id: &'static str,
    pub nome: &'static str,
    arquivos: &'static [Arquivo],
    /// Escrita da direita para a esquerda: o CTC lê na ordem da imagem.
    pub direita_esquerda: bool,
}

impl Modelo {
    pub fn megas(&self) -> u64 { self.arquivos.iter().map(|a| a.3).sum::<u64>().div_ceil(1_000_000) }
    pub fn caminho(&self, nome: &str) -> Option<PathBuf> { pasta().map(|p| p.join(self.id).join(nome)) }
    /// Já baixado (todos os arquivos com o tamanho certo).
    pub fn presente(&self) -> bool {
        self.arquivos.iter().all(|a| self.caminho(a.1).and_then(|p| std::fs::metadata(p).ok()).is_some_and(|m| m.len() == a.3))
    }
}

pub const ORIENTACAO: Modelo = Modelo { id: "orientacao", nome: "Endireitar a página", direita_esquerda: false, arquivos: &[
    ("preprocessing/doc-orientation/PP-LCNet_x1_0_doc_ori.onnx", "modelo.onnx", "f5516822af9262711e197ff224a8a9d884f8046a6321b762e34f8cbf082c45ef", 6_784_410),
] };

/// Escritas que se baixam; `None` no índice 0 é o latino embutido.
pub const LINGUAS: [Option<Modelo>; 6] = [
    None,
    Some(Modelo { id: "grego", nome: "Grego", direita_esquerda: false, arquivos: &[
        ("languages/greek/rec.onnx", "rec.onnx", "13373f736dbb229e96945fc41c2573403d91503b0775c7b7294839e0c5f3a7a3", 7_791_200),
        ("languages/greek/dict.txt", "dict.txt", "31defc62c0c3ad3674a82da6192226a2ba98ef4ff014a7045cb88d59f9c3de31", 1_103),
    ] }),
    Some(Modelo { id: "cirilico", nome: "Cirílico (russo, ucraniano…)", direita_esquerda: false, arquivos: &[
        ("languages/eslav/rec.onnx", "rec.onnx", "dc6bf0e855247decce214ba6dae5bc135fa0ad725a5918a7fcfb59fad6c9cdee", 7_870_092),
        ("languages/eslav/dict.txt", "dict.txt", "3e95f1581557162870cacdba5af91a4c6be2890710d395b0c3c7578e7ee5e6eb", 1_663),
    ] }),
    Some(Modelo { id: "arabe", nome: "Árabe", direita_esquerda: true, arquivos: &[
        ("languages/arabic/rec.onnx", "rec.onnx", "7982d371612785238fd99080cff36354deaec84fdc6ff7da9c82af4243fa0c9a", 8_978_664),
        ("languages/arabic/dict.txt", "dict.txt", "637c27c88512c22089bef927b34ada08f748dc132ac70facd68d8202384c2726", 405),
    ] }),
    Some(Modelo { id: "coreano", nome: "Coreano", direita_esquerda: false, arquivos: &[
        ("languages/korean/rec.onnx", "rec.onnx", "322f140154c820fcb83c3d24cfe42c9ec70dd1a1834163306a7338136e4f1eaa", 13_401_252),
        ("languages/korean/dict.txt", "dict.txt", "a88071c68c01707489baa79ebe0405b7beb5cca229f4fc94cc3ef992328802d7", 47_451),
    ] }),
    Some(Modelo { id: "chines", nome: "Chinês e japonês", direita_esquerda: false, arquivos: &[
        ("languages/chinese/rec.onnx", "rec.onnx", "26fa4f47060f58e25962b9af6beaee05c8182b90e026c4ecc6db165d9dfdc38a", 84_468_836),
        ("languages/chinese/dict.txt", "dict.txt", "d1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b", 74_012),
    ] }),
];

pub fn nome_lingua(i: usize) -> String {
    match &LINGUAS[i] {
        None => "Português, inglês, espanhol… (latino)".into(),
        Some(m) if m.presente() => m.nome.into(),
        Some(m) => format!("{} (baixar {} MB)", m.nome, m.megas()),
    }
}

fn pasta() -> Option<PathBuf> { crate::recentes::pasta_config().map(|p| p.join("ocr")) }

fn sha256(bytes: &[u8]) -> String { Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect() }

/// Bloqueante. Baixa o que falta e confere cada arquivo antes de pôr no lugar.
// shortcut: usa o `curl` do sistema (Linux, macOS e Windows 10+ já trazem) em vez de um cliente HTTPS embutido; trocar se aparecer sistema sem curl.
pub fn baixar(m: &Modelo) -> Result<(), String> {
    for &(remoto, local, hash, tamanho) in m.arquivos {
        let destino = m.caminho(local).ok_or("Sem pasta de configuração.")?;
        if std::fs::metadata(&destino).is_ok_and(|x| x.len() == tamanho) { continue; }
        std::fs::create_dir_all(destino.parent().unwrap_or(&destino)).map_err(|e| e.to_string())?;
        let parcial = destino.with_extension("parcial");
        // Conexão parada (Wi-Fi caiu) desiste em 1 min, senão o download trava até reiniciar o app.
        let saida = std::process::Command::new("curl").args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "--speed-limit", "1024", "--speed-time", "60", "-o"]).arg(&parcial).arg(format!("{BASE}{remoto}"))
            .output().map_err(|e| format!("Não achei o curl para baixar o modelo: {e}"))?;
        if !saida.status.success() {
            let _ = std::fs::remove_file(&parcial);
            return Err(format!("Não consegui baixar o modelo (sem internet?): {}", String::from_utf8_lossy(&saida.stderr).trim()));
        }
        let bytes = std::fs::read(&parcial).map_err(|e| e.to_string())?;
        if bytes.len() as u64 != tamanho || sha256(&bytes) != hash {
            let _ = std::fs::remove_file(&parcial);
            return Err("O modelo baixado veio diferente do esperado; nada foi guardado.".into());
        }
        std::fs::rename(&parcial, &destino).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    #[test]
    fn sha256_conhecido() {
        assert_eq!(super::sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
