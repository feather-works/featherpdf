//! Uma janela só: dois cliques num PDF com o FeatherPDF já aberto viram uma aba nova nele,
//! em vez de outro processo (outros 200–300 MB de RAM).
//!
//! A primeira instância escuta em 127.0.0.1 numa porta qualquer e grava porta e uma senha
//! aleatória em `instancia.txt`, na pasta de configuração do usuário. A seguinte lê o arquivo,
//! manda senha e caminho e sai. Sem a senha (outro usuário da máquina), o pedido é ignorado.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use pelicano::futures::channel::mpsc;

/// Caminhos que chegaram de outra instância; a assinatura em `App::subscription` tira daqui.
static RECEBIDOS: Mutex<Option<mpsc::UnboundedReceiver<PathBuf>>> = Mutex::new(None);

fn arquivo() -> Option<PathBuf> { crate::recentes::pasta_config().map(|p| p.join("instancia.txt")) }

/// Manda `caminho` para o FeatherPDF que já está aberto. `true`: ele recebeu, esta pode sair.
pub fn mandar_para_aberta(caminho: &Path) -> bool {
    let Some(texto) = arquivo().and_then(|a| std::fs::read_to_string(a).ok()) else { return false };
    let mut partes = texto.split_whitespace();
    let (Some(porta), Some(senha)) = (partes.next().and_then(|p| p.parse::<u16>().ok()), partes.next()) else { return false };
    // Relativo ("abrir com" de um terminal) vira absoluto: a outra instância roda em outra pasta.
    let caminho = std::path::absolute(caminho).unwrap_or_else(|_| caminho.to_path_buf());
    let Some(caminho) = caminho.to_str() else { return false };
    let enviar = || -> std::io::Result<bool> {
        let mut c = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, porta)), Duration::from_millis(400))?;
        c.set_read_timeout(Some(Duration::from_secs(2)))?;
        writeln!(c, "{senha}\n{caminho}")?;
        let mut resposta = String::new();
        BufReader::new(c).read_line(&mut resposta)?;
        Ok(resposta.trim() == "ok")
    };
    enviar().unwrap_or(false)
}

/// Passa a escutar os pedidos das próximas instâncias (falhar aqui só desliga o recurso).
pub fn escutar() {
    let Some(destino) = arquivo() else { return };
    let Ok(ouvinte) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else { return };
    let Ok(porta) = ouvinte.local_addr().map(|a| a.port()) else { return };
    let senha = senha_aleatoria();
    let _ = destino.parent().map(std::fs::create_dir_all);
    if featherpdf_io::gravar_atomico(&destino, format!("{porta} {senha}\n").as_bytes()).is_err() { return; }
    let (tx, rx) = mpsc::unbounded();
    *RECEBIDOS.lock().unwrap_or_else(|e| e.into_inner()) = Some(rx);
    std::thread::spawn(move || {
        for conexao in ouvinte.incoming().flatten() {
            if let Some(c) = ler_pedido(conexao, &senha) { let _ = tx.unbounded_send(c); }
        }
    });
}

/// Senha e caminho, uma linha cada; responde "ok" só com a senha certa.
fn ler_pedido(conexao: TcpStream, senha: &str) -> Option<PathBuf> {
    conexao.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut leitor = BufReader::new(conexao.try_clone().ok()?);
    let (mut s, mut caminho) = (String::new(), String::new());
    leitor.read_line(&mut s).ok()?;
    leitor.read_line(&mut caminho).ok()?;
    if s.trim() != senha { return None; }
    let caminho = PathBuf::from(caminho.trim_end_matches(['\r', '\n']));
    let _ = (&conexao).write_all(b"ok\n");
    caminho.is_file().then_some(caminho)
}

/// 128 bits do gerador que o próprio Rust semeia para os HashMap (sem dependência nova).
fn senha_aleatoria() -> String {
    use std::hash::{BuildHasher, Hasher};
    let parte = || { let mut h = std::collections::hash_map::RandomState::new().build_hasher(); h.write_u32(std::process::id()); h.finish() };
    format!("{:016x}{:016x}", parte(), parte())
}

/// Os caminhos recebidos, como fluxo para a assinatura (o receptor sai do lugar uma vez só).
pub fn recebidos() -> impl pelicano::futures::Stream<Item = PathBuf> + Send + 'static {
    use pelicano::futures::StreamExt;
    pelicano::futures::stream::once(async { RECEBIDOS.lock().unwrap_or_else(|e| e.into_inner()).take() })
        .filter_map(|r| async move { r })
        .flatten()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pedido_com_a_senha_certa_entra_e_sem_ela_nao() {
        let ouvinte = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let arquivo = std::env::current_exe().unwrap();
        let caminho = arquivo.to_str().unwrap().to_string();
        let mandar = move |senha: &'static str| {
            let caminho = caminho.clone();
            std::thread::spawn(move || {
                let mut c = TcpStream::connect((Ipv4Addr::LOCALHOST, porta)).unwrap();
                writeln!(c, "{senha}\n{caminho}").unwrap();
                let mut r = String::new();
                let _ = BufReader::new(c).read_line(&mut r);
                r
            })
        };
        let certo = mandar("abc");
        assert_eq!(ler_pedido(ouvinte.accept().unwrap().0, "abc"), Some(arquivo.clone()));
        assert_eq!(certo.join().unwrap().trim(), "ok");
        let errado = mandar("xyz");
        assert_eq!(ler_pedido(ouvinte.accept().unwrap().0, "abc"), None);
        assert_eq!(errado.join().unwrap().trim(), "");
        assert_ne!(senha_aleatoria(), senha_aleatoria());
    }
}
