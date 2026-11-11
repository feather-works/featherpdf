//! Janelas Comprimir e Senha e permissões: as duas gravam uma cópia nova
//! (o Comprimir pode também trocar o próprio arquivo).

use std::path::PathBuf;

use feather_ui::dialogo::{self as dlg, Dialogo};
use feather_ui::tema;
use featherpdf_core::{numero, DocumentoPdf, OpcoesCompressao, Permissoes, Peso, Protecao};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{App, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nivel { Leve, Equilibrado, Maximo }

impl Nivel {
    const TODOS: [Nivel; 3] = [Nivel::Leve, Nivel::Equilibrado, Nivel::Maximo];
    fn dpi(self) -> f32 { match self { Nivel::Leve => 200.0, Nivel::Equilibrado => 150.0, Nivel::Maximo => 96.0 } }
    fn qualidade(self) -> u8 { match self { Nivel::Leve => 85, Nivel::Equilibrado => 75, Nivel::Maximo => 60 } }
    fn nome(self) -> &'static str { match self { Nivel::Leve => "Leve", Nivel::Equilibrado => "Equilibrado", Nivel::Maximo => "Máximo" } }
    fn uso(self) -> &'static str {
        match self { Nivel::Leve => "Imagens a 200 dpi. Para imprimir.", Nivel::Equilibrado => "150 dpi. Para mandar por e-mail.", Nivel::Maximo => "96 dpi e compressão mais lenta. Só para ler na tela." }
    }
    fn indice(self) -> usize { self as usize }
}

#[derive(Debug, Clone)]
pub struct Comprimir {
    nivel: Nivel,
    limpar: bool,
    anexos: bool,
    copia: bool,
    total: u64,
    peso: Peso,
    /// Tamanho de cada nível, calculado em segundo plano; `geracao` descarta conta velha.
    estimativas: [Option<Result<u64, String>>; 3],
    geracao: u64,
    ver_peso: bool,
    /// Desde quando a compressão final está rodando (barra animada no diálogo).
    desde: Option<std::time::Instant>,
}

#[derive(Debug, Clone)]
pub struct Senha {
    abrir: bool,
    senha: String,
    repetir: String,
    limitar: bool,
    perm: Permissoes,
    dono: String,
    aes256: bool,
}

#[derive(Debug, Clone)]
pub enum Janela { Comprimir(Comprimir), Senha(Senha) }

#[derive(Debug, Clone, Copy)]
pub enum Campo { Senha, Repetir, Dono }

#[derive(Debug, Clone, Copy)]
pub enum Marca { Limpar, Anexos, Copia, VerPeso, Abrir, Limitar, Imprimir, AltaQualidade, Copiar, Preencher, Comentar, Editar, Aes256 }

#[derive(Debug, Clone)]
pub enum Pr {
    AbrirComprimir,
    AbrirSenha,
    Nivel(Nivel),
    Marcar(Marca, bool),
    Campo(Campo, String),
    Estimado(u64, Nivel, Result<u64, String>),
    Confirmar,
    /// Bytes prontos e para onde vão (`None`: perguntar onde salvar).
    /// O terceiro campo vai no nome da cópia ("— comprimido.pdf").
    Pronto(Result<Vec<u8>, String>, Option<PathBuf>, &'static str),
    Destino(Option<PathBuf>),
    Fechar,
    /// O PDF veio com senha: a cópia salva sai sem ela.
    TirarSenha,
    /// Só redesenha a barra enquanto comprime.
    Tique,
}

/// Bytes em "4,8 MB" / "320 KB".
fn tamanho(b: u64) -> String {
    if b >= 1024 * 1024 { format!("{} MB", numero(b as f64 / 1048576.0, 1)) } else { format!("{} KB", numero((b as f64 / 1024.0).ceil(), 0)) }
}

/// Força da senha de 0 a 4, pelo tamanho e pelos tipos de caractere.
fn forca(s: &str) -> usize {
    let tipos = [s.chars().any(|c| c.is_lowercase()), s.chars().any(|c| c.is_uppercase()),
        s.chars().any(|c| c.is_ascii_digit()), s.chars().any(|c| !c.is_alphanumeric())].iter().filter(|x| **x).count();
    match s.chars().count() { 0 => 0, 1..=5 => 1, 6..=9 => 1 + (tipos >= 3) as usize, 10..=13 => 2 + (tipos >= 3) as usize, _ => 3 + (tipos >= 2) as usize }
}

fn sufixo(nome: &str, sufixo: &str) -> String {
    let base = nome.strip_suffix(".pdf").or_else(|| nome.strip_suffix(".PDF")).unwrap_or(nome);
    format!("{base} — {sufixo}.pdf")
}

impl Comprimir {
    fn opcoes(&self, nivel: Nivel) -> OpcoesCompressao {
        OpcoesCompressao { dpi: Some(nivel.dpi()), qualidade: nivel.qualidade(), limpar: self.limpar, anexos: self.anexos,
            zopfli: nivel == Nivel::Maximo }
    }
}

fn comprimir(pdf: DocumentoPdf, opcoes: OpcoesCompressao) -> Task<Result<Vec<u8>, String>> {
    Task::perform(async move {
        tokio::task::spawn_blocking(move || pdf.comprimido(opcoes, &featherpdf_io::reamostrar_jpeg))
            .await.unwrap_or_else(|e| Err(format!("Falha na compressão: {e}")))
    }, |r| r)
}

impl App {
    /// A compressão final está rodando (a barra do diálogo precisa de tiques).
    pub fn comprimindo(&self) -> bool { matches!(&self.protecao, Some(Janela::Comprimir(c)) if c.desde.is_some()) }

    /// Recalcula os três tamanhos com as opções atuais.
    fn estimar(&mut self) -> Task<Msg> {
        let (Some(a), Some(Janela::Comprimir(c))) = (&self.aberto, &mut self.protecao) else { return Task::none() };
        c.geracao += 1;
        c.estimativas = [None, None, None];
        let g = c.geracao;
        // A estimativa do Máximo vai sem o zopfli (segundos a mais); o arquivo final sai um pouco menor.
        Task::batch(Nivel::TODOS.map(|n| comprimir(a.pdf.clone(), OpcoesCompressao { zopfli: false, ..c.opcoes(n) })
            .map(move |r| Msg::Prot(Pr::Estimado(g, n, r.map(|b| b.len() as u64))))))
    }

    pub fn update_protecao(&mut self, m: Pr) -> Task<Msg> {
        match m {
            Pr::AbrirComprimir => {
                let Some(a) = &self.aberto else { return Task::none() };
                let total = a.pdf.origem().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len())
                    .or_else(|| a.pdf.serializar().ok().map(|b| b.len() as u64)).unwrap_or(0);
                self.protecao = Some(Janela::Comprimir(Comprimir {
                    nivel: Nivel::Equilibrado, limpar: true, anexos: false, copia: true, total, peso: a.pdf.peso(total),
                    estimativas: [None, None, None], geracao: 0, ver_peso: false, desde: None,
                }));
                return self.estimar();
            }
            Pr::AbrirSenha => if self.aberto.is_some() {
                self.protecao = Some(Janela::Senha(Senha {
                    abrir: true, senha: String::new(), repetir: String::new(), limitar: false,
                    perm: Permissoes { imprimir: true, alta_qualidade: true, copiar: false, preencher: true, comentar: false, editar: false },
                    dono: String::new(), aes256: true,
                }));
                return crate::focar(id_senha());
            },
            Pr::Fechar => self.protecao = None,
            Pr::TirarSenha => if let Some(a) = &mut self.aberto {
                a.pdf.tirar_senha();
                self.scripts.rodada += 1;
                self.protecao = None;
                self.status = "A senha sai do arquivo quando você salvar (Ctrl+S).".into();
            },
            Pr::Tique => {}
            Pr::Nivel(n) => if let Some(Janela::Comprimir(c)) = &mut self.protecao { c.nivel = n },
            Pr::Estimado(g, n, r) => if let Some(Janela::Comprimir(c)) = &mut self.protecao {
                if c.geracao == g { c.estimativas[n.indice()] = Some(r); }
            },
            Pr::Marcar(marca, v) => {
                let mut reestimar = false;
                match &mut self.protecao {
                    Some(Janela::Comprimir(c)) => match marca {
                        Marca::Limpar => { c.limpar = v; reestimar = true }
                        Marca::Anexos => { c.anexos = v; reestimar = true }
                        Marca::Copia => c.copia = v,
                        Marca::VerPeso => c.ver_peso = v,
                        _ => {}
                    },
                    Some(Janela::Senha(s)) => match marca {
                        Marca::Abrir => s.abrir = v,
                        Marca::Limitar => s.limitar = v,
                        Marca::Imprimir => s.perm.imprimir = v,
                        Marca::AltaQualidade => s.perm.alta_qualidade = v,
                        Marca::Copiar => s.perm.copiar = v,
                        Marca::Preencher => s.perm.preencher = v,
                        Marca::Comentar => s.perm.comentar = v,
                        Marca::Editar => s.perm.editar = v,
                        Marca::Aes256 => s.aes256 = v,
                        _ => {}
                    },
                    None => {}
                }
                if reestimar { return self.estimar(); }
            }
            Pr::Campo(campo, t) => if let Some(Janela::Senha(s)) = &mut self.protecao {
                match campo { Campo::Senha => s.senha = t, Campo::Repetir => s.repetir = t, Campo::Dono => s.dono = t }
            },
            Pr::Confirmar => {
                let (Some(a), Some(j)) = (&self.aberto, &self.protecao) else { return Task::none() };
                match j {
                    Janela::Comprimir(c) => {
                        if c.desde.is_some() { return Task::none(); }
                        // Sem a cópia, troca o próprio arquivo e o reabre.
                        let destino = (!c.copia).then(|| a.pdf.origem().map(PathBuf::from)).flatten();
                        // Trocar o próprio arquivo assinado quebraria as assinaturas, como no Ctrl+S.
                        if let Some(Err((_, e))) = destino.as_deref().map(crate::original_sem_assinatura) {
                            self.status = format!("{e} Marque \"Salvar como cópia\".");
                            return Task::none();
                        }
                        let tarefa = comprimir(a.pdf.clone(), c.opcoes(c.nivel));
                        if let Some(Janela::Comprimir(c)) = &mut self.protecao { c.desde = Some(std::time::Instant::now()); }
                        self.status = "Comprimindo…".into();
                        return tarefa.map(move |r| Msg::Prot(Pr::Pronto(r, destino.clone(), "comprimido")));
                    }
                    Janela::Senha(s) => {
                        let Some(p) = protecao_valida(s) else { return Task::none() };
                        let r = a.pdf.protegido(&p);
                        return self.update_protecao(Pr::Pronto(r, None, "protegido"));
                    }
                }
            }
            // Cancelou no meio: o resultado da compressão chega depois e é descartado.
            Pr::Pronto(_, _, "comprimido") if !matches!(self.protecao, Some(Janela::Comprimir(_))) => self.status = "Compressão cancelada.".into(),
            Pr::Pronto(Err(e), _, _) => {
                if let Some(Janela::Comprimir(c)) = &mut self.protecao { c.desde = None; }
                self.status = e;
            }
            Pr::Pronto(Ok(bytes), Some(destino), _) => {
                self.status = match featherpdf_io::gravar_pdf(&destino, &bytes) {
                    Ok(()) => {
                        self.protecao = None;
                        // A aba do arquivo trocado sai (o comprimido já tem as edições dela): senão ficavam
                        // duas abas do mesmo arquivo e salvar a antiga desfazia a compressão.
                        let fechar = if self.aberto.as_ref().and_then(|a| a.pdf.origem()) == Some(destino.as_path()) {
                            self.fechar_pendente = Some(self.ativa);
                            self.fechar_aba(self.ativa)
                        } else { Task::none() };
                        return Task::batch([fechar, self.update(Msg::AbrirCaminho(destino))]);
                    }
                    Err(e) => {
                        if let Some(Janela::Comprimir(c)) = &mut self.protecao { c.desde = None; }
                        e
                    }
                };
            }
            Pr::Pronto(Ok(bytes), None, rotulo) => {
                if let Some(Janela::Comprimir(c)) = &mut self.protecao { c.desde = None; }
                let nome = sufixo(&self.nome_documento().unwrap_or_else(|| "documento.pdf".into()), rotulo);
                self.saida_pendente = Some(bytes);
                return Task::perform(async move {
                    rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(nome).save_file().await
                        .map(|h| h.path().to_path_buf())
                }, |d| Msg::Prot(Pr::Destino(d)));
            }
            Pr::Destino(None) => self.saida_pendente = None,
            Pr::Destino(Some(destino)) => if let Some(bytes) = self.saida_pendente.take() {
                self.status = match featherpdf_io::gravar_pdf(&destino, &bytes) {
                    Ok(()) => {
                        let antes = match &self.protecao { Some(Janela::Comprimir(c)) => format!("{} → ", tamanho(c.total)), _ => String::new() };
                        self.protecao = None;
                        format!("{antes}{} · salvo em {}", tamanho(bytes.len() as u64), destino.display())
                    }
                    Err(e) => e,
                };
            },
        }
        Task::none()
    }

    pub fn com_protecao<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        match &self.protecao {
            Some(Janela::Comprimir(c)) => self.dialogo_comprimir(c).sobre(base),
            Some(Janela::Senha(s)) => dialogo_senha(s, self.aberto.as_ref().and_then(|a| a.pdf.seguranca())).sobre(base),
            None => crate::inicio::sem_camada(base),
        }
    }

    fn dialogo_comprimir<'a>(&'a self, c: &'a Comprimir) -> Dialogo<'a, Msg> {
        let muted = tema::COR_TEXTO_MUTED();
        let mut niveis = row![].spacing(10);
        for n in Nivel::TODOS {
            let ativo = n == c.nivel;
            let valor = match &c.estimativas[n.indice()] {
                None => "calculando…".to_string(),
                Some(Ok(b)) => format!("≈ {}", tamanho(*b)),
                Some(Err(_)) => "não deu".to_string(),
            };
            let cartao = column![
                text(n.nome()).size(14).font(Font::SEMIBOLD),
                text(n.uso()).size(12).color(muted),
                text(valor).size(18).color(if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_TEXTO() }),
            ].spacing(4);
            niveis = niveis.push(button(cartao).width(Length::FILL).padding(12).on_press(Msg::Prot(Pr::Nivel(n)))
                .style(move |_, status| {
                    let cor = if ativo { tema::COR_AZUL_ACENTO() } else { tema::COR_BORDA() };
                    Style {
                        background: Some(Background::Color(if ativo { Color { a: 0.06, ..tema::COR_AZUL_ACENTO() } }
                            else if status == Status::Hovered { tema::COR_HOVER() } else { Color::TRANSPARENT })),
                        border: Border { color: cor, width: if ativo { 2.0 } else { 1.0 }, radius: [8.0; 4] },
                        text_color: Some(tema::COR_TEXTO()),
                        ..Default::default()
                    }
                }));
        }
        let mut corpo = column![niveis].spacing(12);
        if let Some(Ok(b)) = c.estimativas[c.nivel.indice()] {
            if b >= c.total {
                corpo = corpo.push(dlg::aviso("info-circle", "Este arquivo já está enxuto: a cópia não fica menor."));
            }
        }
        corpo = corpo.push(checkbox("Onde está o peso", c.ver_peso).on_toggle(|v| Msg::Prot(Pr::Marcar(Marca::VerPeso, v))).size(16).text_size(13));
        if c.ver_peso {
            let barra = |nome: &'a str, b: u64, cor: Color| -> Element<'a, Msg> {
                let parte = if c.total == 0 { 0.0 } else { b as f32 / c.total as f32 };
                row![
                    text(nome).size(12).width(70),
                    container(container(Space::new(Length::Fill(((parte * 1000.0) as u16).max(1)), 6))
                        .style(move |_| Style { background: Some(Background::Color(cor)), border: Border { radius: [3.0; 4], ..Default::default() }, ..Default::default() }))
                        .width(Length::FILL),
                    text(tamanho(b)).size(12).width(70),
                ].spacing(8).align_y(Align::Center).into()
            };
            corpo = corpo.push(column![
                barra("Imagens", c.peso.imagens, tema::COR_AZUL_ACENTO()),
                barra("Fontes", c.peso.fontes, Color::rgb8(0xd1, 0xa2, 0x19)),
                barra("Resto", c.peso.resto, Color::rgb8(0x9a, 0xa0, 0xa6)),
            ].spacing(4).padding([0, 24]));
        }
        corpo = corpo
            .push(checkbox("Tirar miniaturas, dados de editor e objetos soltos; juntar imagens repetidas", c.limpar)
                .on_toggle(|v| Msg::Prot(Pr::Marcar(Marca::Limpar, v))).size(16).text_size(13))
            .push(checkbox("Remover anexos", c.anexos).on_toggle(|v| Msg::Prot(Pr::Marcar(Marca::Anexos, v))).size(16).text_size(13));
        if let Some(desde) = c.desde {
            // Barra que vai e volta: o motor não conta progresso, então mostra que está vivo e há quanto tempo.
            let t = desde.elapsed().as_millis() as u64;
            let fase = (t / 15 % 150) as u16;
            let pos = if fase < 75 { fase } else { 150 - fase };
            let trilho = |p: u16| Space::new(Length::Fill(p.max(1)), 6);
            let pedaco = container(Space::new(Length::FILL, 6)).width(Length::Fill(25))
                .style(|_| Style { background: Some(Background::Color(tema::COR_AZUL_ACENTO())), border: Border { radius: [3.0; 4], ..Default::default() }, ..Default::default() });
            let barra = container(row![trilho(pos), pedaco, trilho(75 - pos)]).width(Length::FILL)
                .style(|_| Style { background: Some(Background::Color(tema::COR_HOVER())), border: Border { radius: [3.0; 4], ..Default::default() }, ..Default::default() });
            let dica = if c.nivel == Nivel::Maximo { " · o Máximo recomprime tudo sem perda e leva mais" } else { "" };
            corpo = corpo.push(column![barra, text(format!("Comprimindo… {} s{dica}", t / 1000)).size(12).color(muted)].spacing(6));
        }
        let tem_origem = self.aberto.as_ref().is_some_and(|a| a.pdf.origem().is_some());
        let copia = checkbox("Salvar como cópia", c.copia || !tem_origem).size(16).text_size(13);
        let copia = if tem_origem { copia.on_toggle(|v| Msg::Prot(Pr::Marcar(Marca::Copia, v))) } else { copia };
        let paginas = self.total_paginas();
        Dialogo::new("Comprimir", corpo).icone("arrows-maximize")
            .subtitulo(format!("hoje {} · {paginas} {}", tamanho(c.total), if paginas == 1 { "página" } else { "páginas" }))
            .largura(620.0).rodape_esquerda(copia)
            .secundario("Cancelar", Some(Msg::Prot(Pr::Fechar)))
            .primario(if c.desde.is_some() { "Comprimindo…" } else { "Comprimir" }, c.desde.is_none().then_some(Msg::Prot(Pr::Confirmar)))
            .ao_fechar(Msg::Prot(Pr::Fechar)).topo(120.0)
    }
}

pub fn id_senha() -> &'static str { "senha-abrir" }

/// O que vai para o motor, ou `None` enquanto falta algo.
fn protecao_valida(s: &Senha) -> Option<Protecao> {
    let abrir_ok = !s.abrir || (!s.senha.is_empty() && s.senha == s.repetir);
    // Senha do dono igual à de abrir: quem abre também tira as restrições.
    let limitar_ok = !s.limitar || (!s.dono.is_empty() && !(s.abrir && s.dono == s.senha));
    (abrir_ok && limitar_ok && (s.abrir || s.limitar)).then(|| Protecao {
        senha_abrir: if s.abrir { s.senha.clone() } else { String::new() },
        limitar: s.limitar.then(|| (s.perm, s.dono.clone())),
        aes256: s.aes256,
    })
}

fn dialogo_senha(s: &Senha, seguranca: Option<String>) -> Dialogo<'_, Msg> {
    let muted = tema::COR_TEXTO_MUTED();
    let marca = |rotulo: &'static str, v: bool, m: Marca| checkbox(rotulo, v).on_toggle(move |v| Msg::Prot(Pr::Marcar(m, v))).size(16).text_size(13);
    let campo = |dica: &'static str, valor: &str, c: Campo| text_input(dica, valor.to_owned()).secure(true)
        .on_input(move |t| Msg::Prot(Pr::Campo(c, t))).on_submit(Msg::Prot(Pr::Confirmar)).padding([6, 8]).size(13);
    let mut corpo = column![].spacing(10);
    if let Some(seguranca) = seguranca {
        // Abriu com a senha (ou só tinha restrições): ela volta ao salvar, a menos que saia aqui.
        corpo = corpo.push(container(row![
            column![
                text("Este PDF já tem senha ou restrições, e elas continuam ao salvar.").size(13),
                text(seguranca).size(12).color(muted),
            ].spacing(2).width(Length::FILL),
            button(text("Tirar a senha").size(13)).padding([5, 12]).style(tema::estilo_botao_secundario).on_press(Msg::Prot(Pr::TirarSenha)),
        ].spacing(10).align_y(Align::Center)).padding([8, 10]).style(|_| Style {
            background: Some(Background::Color(tema::COR_PAINEL_FAIXA())),
            border: Border { color: tema::COR_BORDA_SUAVE(), width: 1.0, radius: [6.0; 4] }, ..Default::default()
        }));
    }
    corpo = corpo.push(marca("Pedir senha para abrir", s.abrir, Marca::Abrir));
    if s.abrir {
        let f = forca(&s.senha);
        let (nome, cor) = [("", muted), ("fraca", tema::COR_ERRO()), ("média", Color::rgb8(0xb0, 0x7d, 0x0b)),
            ("boa", Color::rgb8(0x28, 0x78, 0x46)), ("forte", Color::rgb8(0x28, 0x78, 0x46))][f];
        let iguais = match (s.repetir.is_empty(), s.senha == s.repetir) { (true, _) => "", (false, true) => " · as duas iguais ✓", (false, false) => " · as duas não batem" };
        let medidor = container(Space::new(Length::Fill(f.max(1) as u16 * 25), 5))
            .style(move |_| Style { background: Some(Background::Color(if f == 0 { Color::TRANSPARENT } else { cor })), ..Default::default() });
        corpo = corpo.push(column![
            row![campo("Senha", &s.senha, Campo::Senha).id(id_senha()), campo("Repetir a senha", &s.repetir, Campo::Repetir)].spacing(8),
            row![container(row![medidor, Space::with_width(Length::Fill((4 - f.min(4)) as u16 * 25 + 1))]).width(120)
                .style(|_| Style { background: Some(Background::Color(tema::COR_BORDA())), border: Border { radius: [3.0; 4], ..Default::default() }, ..Default::default() }),
                text(format!("{nome}{iguais}")).size(12).color(if s.senha == s.repetir || s.repetir.is_empty() { cor } else { tema::COR_ERRO() })]
                .spacing(8).align_y(Align::Center),
        ].spacing(6).padding(Padding { left: 24.0, ..Padding::ZERO }));
    }
    corpo = corpo.push(marca("Limitar o que se pode fazer", s.limitar, Marca::Limitar));
    if s.limitar {
        let p = s.perm;
        let mut imprimir = row![marca("Imprimir", p.imprimir, Marca::Imprimir)].spacing(12);
        if p.imprimir { imprimir = imprimir.push(marca("em alta qualidade", p.alta_qualidade, Marca::AltaQualidade)); }
        corpo = corpo.push(column![
            imprimir,
            marca("Copiar texto e imagens", p.copiar, Marca::Copiar),
            marca("Preencher formulários e assinar", p.preencher, Marca::Preencher),
            marca("Comentar", p.comentar, Marca::Comentar),
            marca("Editar páginas", p.editar, Marca::Editar),
            text("Senha de quem pode mudar isso").size(13),
            campo("Senha do dono", &s.dono, Campo::Dono),
            text(if s.abrir && !s.dono.is_empty() && s.dono == s.senha { "Use uma senha diferente da de abrir: com a mesma, quem abre também tira as restrições." } else { "" })
                .size(12).color(tema::COR_ERRO()),
        ].spacing(6).padding(Padding { left: 24.0, ..Padding::ZERO }));
    }
    corpo = corpo
        .push(row![text("Criptografia").size(13),
            button(text("AES-256 (PDF 2.0)").size(13)).padding([5, 10]).style(tema::estilo_botao_pequeno(s.aes256)).on_press(Msg::Prot(Pr::Marcar(Marca::Aes256, true))),
            button(text("AES-128 (leitores antigos)").size(13)).padding([5, 10]).style(tema::estilo_botao_pequeno(!s.aes256)).on_press(Msg::Prot(Pr::Marcar(Marca::Aes256, false))),
        ].spacing(6).align_y(Align::Center))
        .push(text("As permissões são respeitadas pelos leitores, mas não impedem quem tem a senha de abrir. \
            Esquecer a senha de abrir = perder o arquivo. O arquivo aberto continua sem senha; a proteção vai numa cópia.")
            .size(12).color(muted));
    Dialogo::new("Senha e permissões", corpo).icone("lock").largura(540.0)
        .secundario("Cancelar", Some(Msg::Prot(Pr::Fechar)))
        .primario("Proteger e salvar cópia", protecao_valida(s).map(|_| Msg::Prot(Pr::Confirmar)))
        .ao_fechar(Msg::Prot(Pr::Fechar)).topo(90.0)
}
