//! Painéis Assinar (certificado A1 .pfx/.p12) e Assinaturas
//! (validar), os dois na lateral direita. A assinatura sai numa cópia "— assinado.pdf"
//! pelo mesmo caminho de gravação do Proteger.

use std::path::PathBuf;

use feather_ui::dialogo as dlg;
use feather_ui::tema;
use featherpdf_core::assinatura::{self, AssinaturaLida, OpcoesAssinatura};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::{proteger::Pr, App, Msg, Painel};

#[derive(Debug, Clone, Default)]
pub struct Assinar {
    arquivo: Option<PathBuf>,
    p12: Vec<u8>,
    senha: String,
    /// O que a senha digitada abre: (titular, emissor, validade, vencido) ou o motivo do erro.
    conferido: Option<Result<(String, String, String, bool), String>>,
    motivo: String,
    local: String,
    visivel: bool,
    /// Lugar marcado na página (página, caixa em pontos); `None` = canto inferior direito.
    lugar: Option<(u32, [f32; 4])>,
    /// Arrastando na página para marcar o lugar.
    marcando: bool,
    /// "Incluir dados do computador" ligado: `Some("")` enquanto coleta.
    dispositivo: Option<String>,
    erro: Option<String>,
    /// "Assinar com certificado digital" ligado: mostra a parte do certificado.
    certificado: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Campo { Senha, Motivo, Local }

#[derive(Debug, Clone)]
pub enum As {
    Abrir,
    Validar,
    EscolherCertificado,
    Certificado(Option<PathBuf>),
    Campo(Campo, String),
    Visivel(bool),
    Marcar(bool),
    Dispositivo(bool),
    Coletado(String),
    Lugar { pagina: u32, a: Point, b: Point },
    Confirmar,
    Fechar,
    UsarCertificado(bool),
}

const VERDE: Color = Color::rgb(0x28 as f32 / 255.0, 0x78 as f32 / 255.0, 0x46 as f32 / 255.0);
const AMBAR: Color = Color::rgb(0xb0 as f32 / 255.0, 0x7d as f32 / 255.0, 0x0b as f32 / 255.0);

impl App {
    /// O PDF como está no disco (o que vale para a assinatura); sem arquivo ou com
    /// mudanças não salvas, o documento atual serializado.
    fn bytes_para_assinar(&self) -> Result<Vec<u8>, String> {
        let a = self.aberto.as_ref().ok_or("Nenhum documento aberto.")?;
        match a.pdf.origem() {
            Some(p) if !a.pdf.modificado() => featherpdf_io::ler_bytes(p),
            // Serializar sairia sem a senha e regravaria o arquivo, quebrando quem já assinou.
            _ if a.pdf.tem_senha() => Err("O PDF tem senha e mudanças não salvas: salve (ou tire a senha) antes de assinar.".into()),
            _ if tem_assinatura(&a.pdf) => Err("O PDF já está assinado e tem mudanças: assinar agora quebraria a assinatura anterior. \
                Desfaça as mudanças para acrescentar sua assinatura.".into()),
            _ => a.pdf.serializar(),
        }
    }

    pub fn update_assinatura(&mut self, m: As) -> Task<Msg> {
        match m {
            As::Abrir => if self.aberto.is_some() {
                self.assinatura.get_or_insert_with(|| Assinar { visivel: true, ..Default::default() });
                self.rubrica.ler_salvas();
                self.painel = if self.painel == Some(Painel::Assinar) { None } else { Some(Painel::Assinar) };
            },
            As::Validar => if let Some(a) = &self.aberto {
                // Validar confere o arquivo salvo: o que está só na memória não tem assinatura.
                let bytes = match a.pdf.origem() {
                    Some(p) => featherpdf_io::ler_bytes(p),
                    None => a.pdf.serializar(),
                };
                self.validacao = Some(bytes.and_then(|b| assinatura::verificar_assinaturas(&b)));
                self.painel = Some(Painel::Assinaturas);
            },
            As::Fechar => {
                if matches!(self.painel, Some(Painel::Assinar | Painel::Assinaturas)) { self.painel = None; }
                self.assinatura = None;
            }
            As::EscolherCertificado => return Task::perform(async {
                rfd::AsyncFileDialog::new().add_filter("Certificado A1", &["pfx", "p12", "PFX", "P12"]).pick_file().await
                    .map(|h| h.path().to_path_buf())
            }, |c| Msg::Assin(As::Certificado(c))),
            As::Certificado(c) => if let (Some(s), Some(c)) = (&mut self.assinatura, c) {
                match featherpdf_io::ler_bytes(&c) {
                    Ok(b) => { s.p12 = b; s.arquivo = Some(c); s.erro = None; }
                    Err(e) => s.erro = Some(e),
                }
                s.conferir();
            },
            As::Campo(campo, t) => if let Some(s) = &mut self.assinatura {
                match campo { Campo::Senha => s.senha = t, Campo::Motivo => s.motivo = t, Campo::Local => s.local = t }
                if matches!(campo, Campo::Senha) { s.conferir(); }
                s.erro = None;
            },
            As::UsarCertificado(v) => if let Some(s) = &mut self.assinatura { s.certificado = v },
            As::Visivel(v) => if let Some(s) = &mut self.assinatura { s.visivel = v },
            As::Marcar(m) => if let Some(s) = &mut self.assinatura {
                s.marcando = m;
                if !m { s.lugar = None; }
            },
            As::Dispositivo(ligar) => if let Some(s) = &mut self.assinatura {
                s.dispositivo = ligar.then(String::new);
                if ligar {
                    return Task::perform(async {
                        tokio::task::spawn_blocking(featherpdf_io::dados_do_computador).await.unwrap_or_default()
                    }, |d| Msg::Assin(As::Coletado(d)));
                }
            },
            As::Coletado(d) => if let Some(s) = &mut self.assinatura {
                if s.dispositivo.is_some() { s.dispositivo = Some(d); }
            },
            As::Lugar { pagina, a, b } => {
                let Some([x0, y0, x1, y1]) = self.caixa_na_pagina(pagina, a, b) else { return Task::none() };
                // Um clique só (sem arrastar) põe a caixa de 240 × 64 pt a partir do ponto.
                let caixa = if x1 - x0 < 40.0 || y1 - y0 < 12.0 { [x0, y1 - 64.0, x0 + 240.0, y1] } else { [x0, y0, x1, y1] };
                if let Some(s) = &mut self.assinatura { (s.lugar, s.marcando, s.visivel) = (Some((pagina, caixa)), false, true); }
            }
            As::Confirmar => {
                let Some(s) = &self.assinatura else { return Task::none() };
                if !matches!(s.conferido, Some(Ok(_))) { return Task::none() }
                let op = OpcoesAssinatura { motivo: s.motivo.trim().into(), local: s.local.trim().into(),
                    visivel: s.visivel.then(|| s.lugar.map_or(self.pagina, |l| l.0)), caixa: s.lugar.filter(|_| s.visivel).map(|l| l.1),
                    dispositivo: s.dispositivo.clone().unwrap_or_default() };
                let r = assinatura::ler_certificado(&s.p12, &s.senha)
                    .and_then(|cert| assinatura::assinar(&self.bytes_para_assinar()?, &cert, &op).map(|b| (b, cert.titular())));
                match r {
                    Ok((bytes, titular)) => {
                        self.assinatura = None;
                        if self.painel == Some(Painel::Assinar) { self.painel = None; }
                        let tarefa = self.update_protecao(Pr::Pronto(Ok(bytes), None, "assinado"));
                        self.status = format!("Assinado por {titular}. Escolha onde salvar a cópia.");
                        return tarefa;
                    }
                    Err(e) => if let Some(s) = &mut self.assinatura { s.erro = Some(e) },
                }
            }
        }
        Task::none()
    }

    pub fn painel_assinar<'a>(&'a self) -> Element<'a, Msg> {
        let Some(s) = &self.assinatura else {
            return text("Abra um documento e use Proteger → Assinar.").size(13).color(tema::COR_TEXTO_MUTED()).into();
        };
        let muted = tema::COR_TEXTO_MUTED();
        let campo = |dica: &'static str, valor: &'a str, c: Campo| text_input(dica, valor)
            .on_input(move |t| Msg::Assin(As::Campo(c, t))).on_submit(Msg::Assin(As::Confirmar)).padding([6, 8]).size(13);
        let mut topo = column![
            self.painel_rubrica(),
            crate::inicio::linha_horizontal(),
            checkbox("Assinar com certificado digital", s.certificado).on_toggle(|v| Msg::Assin(As::UsarCertificado(v))).size(16).text_size(13),
        ].spacing(12);
        if !s.certificado {
            topo = topo.push(text("Com o certificado A1 (.pfx), o PDF sai travado e com validade jurídica; \
                sem ele, a assinatura é só a imagem na página.").size(12).color(muted));
            return scrollable(topo).height(Length::FILL).into();
        }
        let nome = s.arquivo.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "nenhum escolhido".into());
        let mut corpo = column![
            topo,
            text("Certificado A1 (.pfx ou .p12)").size(13),
            row![
                button(text("Escolher…").size(13)).padding([5, 10]).style(tema::estilo_botao_pequeno(false))
                    .on_press(Msg::Assin(As::EscolherCertificado)),
                text(nome).size(13).color(if s.arquivo.is_some() { tema::COR_TEXTO() } else { muted }),
            ].spacing(8).align_y(Align::Center),
            campo("Senha do certificado", &s.senha, Campo::Senha).secure(true),
        ].spacing(10);
        corpo = match &s.conferido {
            Some(Ok((titular, emissor, validade, vencido))) => corpo.push(column![
                text(format!("✓ Senha correta · {titular}")).size(13).color(VERDE),
                text(if emissor == titular { "Certificado pessoal (autoassinado): identidade não confirmada".into() }
                    else { format!("Emitido por {emissor}") }).size(12).color(if emissor == titular { AMBAR } else { muted }),
                text(if *vencido { format!("Venceu em {validade}: quem conferir vai recusar a assinatura.") }
                    else { format!("Válido até {validade}") }).size(12).color(if *vencido { AMBAR } else { muted }),
            ].spacing(2)),
            Some(Err(e)) => corpo.push(text(e).size(13).color(tema::COR_ERRO())),
            None => corpo.push(text("A senha é a que você definiu ao emitir ou exportar o certificado \
                (na certificadora, ou ao exportar o .pfx do navegador ou do Windows). Ela não fica guardada no FeatherPDF.")
                .size(12).color(muted)),
        };
        corpo = corpo.push(campo("Motivo (opcional)", &s.motivo, Campo::Motivo))
            .push(campo("Local (opcional)", &s.local, Campo::Local))
            .push(checkbox("Mostrar a assinatura na página", s.visivel).on_toggle(|v| Msg::Assin(As::Visivel(v))).size(16).text_size(13));
        if s.visivel {
            let (onde, botao) = match (s.marcando, s.lugar) {
                (true, _) => ("Arraste na página o lugar da caixa (ou clique no ponto de início). Esc cancela.".to_string(), None),
                (false, Some((p, _))) => (format!("No lugar marcado na página {p}."), Some(("Voltar ao canto", false))),
                (false, None) => (format!("Canto inferior direito da página {}.", self.pagina), Some(("Marcar na página", true))),
            };
            let mut linha = column![text(onde).size(12).color(muted)].spacing(4);
            if let Some((rotulo, marcar)) = botao {
                linha = linha.push(button(text(rotulo).size(12)).padding([4, 8]).style(tema::estilo_botao_pequeno(false))
                    .on_press(Msg::Assin(As::Marcar(marcar))));
            }
            corpo = corpo.push(linha);
        }
        corpo = corpo.push(checkbox("Incluir dados do computador (IP público, MAC e nome do PC)", s.dispositivo.is_some())
            .on_toggle(|v| Msg::Assin(As::Dispositivo(v))).size(16).text_size(13));
        if let Some(d) = &s.dispositivo {
            corpo = corpo.push(text(if d.is_empty() { "Coletando…".to_string() }
                else { format!("{d}\nVai na caixa visível e dentro da assinatura: quem abrir o PDF vê.") }).size(12).color(muted));
        }
        if self.aberto.as_ref().is_some_and(|a| a.pdf.modificado() && a.pdf.origem().is_some()) {
            corpo = corpo.push(dlg::aviso("info-circle",
                "Há mudanças não salvas: a cópia assinada sai com elas, regravada, e assinaturas anteriores deixam de valer."));
        }
        if let Some(e) = &s.erro { corpo = corpo.push(text(e).size(13).color(tema::COR_ERRO())); }
        let pronto = matches!(s.conferido, Some(Ok(_))) && s.dispositivo.as_ref().is_none_or(|d| !d.is_empty());
        corpo = corpo.push(button(text("Assinar e salvar").size(13)).padding([6, 14]).style(tema::estilo_botao_primario)
                .on_press_maybe(pronto.then_some(Msg::Assin(As::Confirmar))))
            .push(text("Trava o documento: qualquer mudança depois invalida a assinatura. \
                O arquivo aberto não muda; a assinatura vai numa cópia “— assinado.pdf”.").size(12).color(muted));
        scrollable(corpo).height(Length::FILL).into()
    }

    pub fn painel_assinaturas(&self) -> Element<'_, Msg> {
        let mut corpo = column![].spacing(8);
        match &self.validacao {
            None => corpo = corpo.push(text("Use Proteger → Validar.").size(13).color(tema::COR_TEXTO_MUTED())),
            Some(Err(e)) => corpo = corpo.push(text(e).size(13).color(tema::COR_ERRO())),
            Some(Ok(v)) if v.is_empty() => corpo = corpo.push(dlg::aviso_vazio("Este PDF não tem assinaturas digitais.")),
            Some(Ok(v)) => for a in v { corpo = corpo.push(cartao(a)) },
        }
        corpo = corpo.push(text("Confere se o trecho assinado mudou e se a assinatura bate com o certificado embutido. \
            Não verifica a cadeia até a raiz ICP-Brasil nem a revogação (CRL/OCSP), e a hora é a declarada por quem assinou \
            (sem carimbo de tempo). Para validade jurídica, confira também em validar.iti.gov.br.")
            .size(12).color(tema::COR_TEXTO_MUTED()));
        scrollable(corpo).height(Length::FILL).into()
    }
}

impl App {
    /// Com "Marcar na página": o arrasto que escolhe o lugar; depois, a prévia da caixa.
    pub fn camada_assinatura(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        let s = self.assinatura.as_ref().filter(|s| s.visivel && self.painel == Some(Painel::Assinar))?;
        if s.marcando {
            return Some(crate::inserir::camada_arrastar(n, |pagina, a, b| Msg::Assin(As::Lugar { pagina, a, b }), l, a));
        }
        let (_, caixa) = s.lugar.filter(|l| l.0 == n)?;
        let (pag, giro) = self.aberto.as_ref()?.pdf.geometria(n).ok()?;
        let r = crate::edicao::na_tela(caixa, pag, giro, self.zoom);
        Some(Stack::new().width(l).height(a).push(column![
            Space::with_height(r.y),
            row![Space::with_width(r.x), container(text("Assinatura").size(12).color(tema::COR_AZUL_ACENTO()))
                .width(r.width).height(r.height).padding(4).style(|_| Style {
                    background: Some(Background::Color(Color { a: 0.08, ..tema::COR_AZUL_ACENTO() })),
                    border: Border { color: tema::COR_AZUL_ACENTO(), width: 1.5, radius: [2.0; 4] }, ..Default::default()
                })],
        ]).into())
    }
}

impl Assinar {
    pub fn marcando(&self) -> bool { self.marcando }

    /// Abre o certificado com a senha digitada para mostrar na hora se ela confere.
    fn conferir(&mut self) {
        self.conferido = (!self.p12.is_empty() && !self.senha.is_empty()).then(|| assinatura::ler_certificado(&self.p12, &self.senha)
            .map(|c| (c.titular(), c.emissor(), c.valido_ate(), c.vencido())));
    }
}

/// O documento tem algum campo de assinatura preenchido.
pub(crate) fn tem_assinatura(pdf: &featherpdf_core::DocumentoPdf) -> bool {
    pdf.campos().iter().any(|c| c.tipo == featherpdf_core::TipoCampo::Assinatura && !c.valor.is_empty())
}

fn cartao(a: &AssinaturaLida) -> Element<'_, Msg> {
        let (situacao, cor) = match (&a.problema, a.integra, a.confere) {
        (Some(p), _, _) => (format!("Não foi possível conferir: {p}"), AMBAR),
        (None, false, _) => ("O documento foi alterado depois da assinatura".to_string(), tema::COR_ERRO()),
        (None, true, false) => ("A assinatura não confere com o certificado".to_string(), tema::COR_ERRO()),
        // Mudança depois da assinatura (atualização incremental) pode trocar o que aparece na tela: não fica verde.
        (None, true, true) if !a.cobre_tudo => ("Há mudanças depois desta assinatura: o trecho assinado não mudou, o resto do arquivo sim".to_string(), AMBAR),
        (None, true, true) => ("Assinatura íntegra: o trecho assinado não mudou".to_string(), if a.autoassinado { AMBAR } else { VERDE }),
    };
    let mut detalhes = vec![];
    if !a.emissor.is_empty() { detalhes.push(format!("emitido por {}", a.emissor)); }
    if let Some(d) = &a.data { detalhes.push(d.clone()); }
    if let Some(m) = &a.motivo { detalhes.push(m.clone()); }
    if let Some(d) = &a.dispositivo { detalhes.push(d.clone()); }
    let mut c = column![
        text(format!("{} · {}", a.campo, if a.quem.is_empty() { "?" } else { &a.quem }))
            .size(14).font(Font::SEMIBOLD),
        text(situacao).size(13).color(cor),
        text(detalhes.join(" · ")).size(12).color(tema::COR_TEXTO_MUTED()),
    ].spacing(3);
    // Íntegra diz que o conteúdo não mudou; quem assinou só uma cadeia até uma raiz confiável confirmaria.
    if a.valida() {
        c = c.push(text(if a.autoassinado { "Certificado autoassinado: qualquer um gera um com esse nome. Identidade não verificada." }
            else { "Identidade não verificada: o FeatherPDF não confere a cadeia até uma certificadora confiável." }).size(12).color(AMBAR));
    }
    if a.valida() && !a.cobre_tudo {
        c = c.push(text("O arquivo recebeu mudanças depois desta assinatura (pode ser outra assinatura).")
            .size(12).color(AMBAR));
    }
    container(c).padding(10).width(Length::FILL).style(move |_| Style {
        background: Some(Background::Color(Color { a: 0.06, ..cor })),
        border: Border { color: Color { a: 0.4, ..cor }, width: 1.0, radius: [6.0; 4] },
        ..Default::default()
    }).into()
}

