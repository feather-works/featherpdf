//! Arquivo ▸ Propriedades do documento (Ctrl+D, como no Acrobat): título, autor, assunto e
//! palavras-chave editáveis; programa, datas, versão, páginas, tamanho e arquivo só para ler.

use feather_ui::dialogo::Dialogo;
use feather_ui::tema;
use featherpdf_core::Propriedades;

use pelicano::*;
use pelicano::{column, row};
use crate::{App, Msg};

#[derive(Debug, Clone)]
pub enum Pp {
    Abrir,
    Campo(usize, String),
    Salvar,
    Fechar,
}

/// Os quatro campos editáveis (título, autor, assunto, palavras-chave) e o resto, lido ao abrir.
#[derive(Debug, Clone, Default)]
pub struct Janela {
    campos: [String; 4],
    lidas: Propriedades,
}

const ROTULOS: [&str; 4] = ["Título", "Autor", "Assunto", "Palavras-chave"];

impl App {
    pub fn update_propriedades(&mut self, m: Pp) -> Task<Msg> {
        match m {
            Pp::Abrir => if let Some(a) = &self.aberto {
                let p = a.pdf.propriedades();
                self.propriedades = Some(Janela {
                    campos: [p.titulo.clone(), p.autor.clone(), p.assunto.clone(), p.palavras.clone()], lidas: p,
                });
                return crate::focar("propriedade-0");
            },
            Pp::Campo(i, t) => if let Some(j) = &mut self.propriedades { if let Some(c) = j.campos.get_mut(i) { *c = t } },
            Pp::Salvar => if let Some(j) = self.propriedades.take() {
                let [t, a, s, k] = j.campos;
                if (t.trim(), a.trim(), s.trim(), k.trim()) == (j.lidas.titulo.as_str(), j.lidas.autor.as_str(),
                    j.lidas.assunto.as_str(), j.lidas.palavras.as_str()) { return Task::none(); }
                return self.editar(move |pdf| {
                    pdf.definir_propriedades(&t, &a, &s, &k)?;
                    Ok("Propriedades do documento gravadas (Ctrl+Z desfaz).".into())
                });
            },
            Pp::Fechar => self.propriedades = None,
        }
        Task::none()
    }

    pub fn com_propriedades<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(j) = &self.propriedades else { return base };
        let linha = |rotulo: &str, valor: Element<'a, Msg>| row![
            text(rotulo.to_owned()).size(13).color(tema::COR_TEXTO_MUTED()).width(120.0), valor,
        ].spacing(10).align(Align::Center);
        let mut corpo = column![].spacing(8);
        for (i, rotulo) in ROTULOS.iter().enumerate() {
            corpo = corpo.push(linha(rotulo, text_input("", &j.campos[i]).id(format!("propriedade-{i}"))
                .on_input(move |t| Msg::Prop(Pp::Campo(i, t))).on_submit(Msg::Prop(Pp::Salvar))
                .padding([6, 9]).size(13).into()));
        }
        let p = &j.lidas;
        let arquivo = self.aberto.as_ref().and_then(|a| a.pdf.origem()).map(|c| {
            let bytes = std::fs::metadata(c).map(|m| m.len()).unwrap_or(0);
            let tamanho = if bytes >= 1 << 20 { format!("{:.1} MB", bytes as f64 / 1048576.0).replace('.', ",") } else { format!("{} KB", bytes.div_ceil(1024)) };
            (c.display().to_string(), tamanho)
        });
        let ler = |t: String| -> Element<'a, Msg> { text(if t.is_empty() { "—".into() } else { t }).size(13).into() };
        corpo = corpo.push(Space::new(0.0, 6.0));
        if let Some((caminho, tamanho)) = arquivo {
            corpo = corpo.push(linha("Arquivo", ler(caminho))).push(linha("Tamanho", ler(tamanho)));
        }
        corpo = corpo
            .push(linha("Páginas", ler(format!("{} · primeira com {}", p.paginas,
                p.tamanho_mm.map_or("—".into(), |(l, a)| format!("{l:.0} × {a:.0} mm"))))))
            .push(linha("Versão do PDF", ler(format!("{}{}", p.versao, if p.marcado { " · marcado (acessível)" } else { "" }))))
            .push(linha("Criado em", ler(p.criado.clone())))
            .push(linha("Modificado em", ler(p.modificado.clone())))
            .push(linha("Aplicativo", ler(p.criador.clone())))
            .push(linha("Gerador do PDF", ler(p.produtor.clone())));
        Dialogo::new("Propriedades do documento", corpo).icone("info-circle").largura(560.0)
            .secundario("Cancelar", Some(Msg::Prop(Pp::Fechar)))
            .primario("Gravar", Some(Msg::Prop(Pp::Salvar)))
            .ao_fechar(Msg::Prop(Pp::Fechar)).topo(110.0)
            .sobre(base)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn abrir_editar_e_gravar_entra_no_desfazer() {
        let (mut app, _) = App::novo();
        let _ = app.update(Msg::NovoA4);
        let _ = app.update(Msg::Prop(Pp::Abrir));
        assert!(app.propriedades.is_some());
        let _ = app.update(Msg::Prop(Pp::Campo(0, "Contrato".into())));
        let _: Element<'_, Msg> = app.com_propriedades(Space::new(Length::FILL, Length::FILL).into());
        let _ = app.update(Msg::Prop(Pp::Salvar));
        assert!(app.propriedades.is_none());
        assert_eq!(app.aberto.as_ref().unwrap().pdf.propriedades().titulo, "Contrato");
        assert!(app.pode_desfazer(), "Ctrl+Z desfaz");
        // Sem mudança, Gravar só fecha.
        let _ = app.update(Msg::Prop(Pp::Abrir));
        let n = app.historico.len();
        let _ = app.update(Msg::Prop(Pp::Salvar));
        assert_eq!(app.historico.len(), n);
    }
}
