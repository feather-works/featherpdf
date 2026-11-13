//! Aba Formulários: preencher os campos na própria página, criar campo novo
//! arrastando, detectar campos, limpar e levar os dados ao FeatherSheet
//! (deste PDF ou de uma pasta inteira de fichas, uma linha por arquivo).

use std::path::{Path, PathBuf};

use feather_ui::dialogo::{self as dlg, Dialogo};
use featherpdf_core::{Campo, CampoNovo, DocumentoPdf, TipoCampo};

use pelicano::*;
use pelicano::column;
use crate::compat::*;
use crate::edicao::na_tela;
use crate::{App, Ferramenta, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Novo { Texto, Caixa, Opcao, Lista, Data, Assinatura }

impl Novo {
    fn nome(self) -> &'static str {
        match self { Novo::Texto => "Campo de texto", Novo::Caixa => "Caixa de marcar", Novo::Opcao => "Opção", Novo::Lista => "Lista", Novo::Data => "Data", Novo::Assinatura => "Assinatura" }
    }
    /// Tamanho (pt) do campo criado com um clique, sem arrastar.
    fn padrao(self) -> (f32, f32) {
        match self { Novo::Caixa | Novo::Opcao => (12.0, 12.0), Novo::Assinatura => (180.0, 40.0), Novo::Data => (80.0, 18.0), _ => (160.0, 18.0) }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_do_calendario() {
        assert_eq!(dias_de(1970, 1, 1), 0);
        assert_eq!(data_de(dias_de(2026, 10, 19)), (2026, 10, 19));
        assert_eq!((dias_no_mes(2024, 2), dias_no_mes(2026, 2), dias_no_mes(2026, 12)), (29, 28, 31));
        assert_eq!((dias_de(2026, 10, 1) + 4).rem_euclid(7), 4, "1/10/2026 é quinta");
        assert_eq!(ler_data("05/03/26"), Some((2026, 3, 5)));
        assert_eq!(ler_data("31/02/2026"), None);
    }
}

#[derive(Debug, Clone)]
pub struct Criando { pagina: u32, caixa: [f32; 4], tipo: Novo, nome: String, opcoes: String }

#[derive(Debug, Clone, Default)]
pub struct Formulario {
    /// Campos do documento; `None` depois de uma edição (relidos no próximo `update`).
    pub campos: Option<Vec<Campo>>,
    /// Campo sendo digitado: o primeiro toque guarda o Ctrl+Z, os seguintes não.
    digitando: Option<(u32, u16)>,
    /// Conta as teclas: a página só é redesenhada quando a última ficou parada um instante.
    geracao: u64,
    pendente: bool,
    /// Calendário aberto: campo, ano e mês mostrados.
    calendario: Option<(usize, i32, u32)>,
    pub criando: Option<Criando>,
}

#[derive(Debug, Clone)]
pub enum Fm {
    Valor(usize, String),
    Assentar(u64),
    Confirmar,
    Calendario(usize),
    Mes(i32),
    Dia(i32, u32, u32),
    Alternar(usize),
    Caixa { pagina: u32, a: Point, b: Point },
    Nome(String),
    Opcoes(String),
    Criar,
    Fechar,
    Detectar,
    Limpar,
    ParaSheet,
    Pasta,
    PastaEscolhida(Option<PathBuf>),
    Modelo(featherpdf_core::Modelo),
}

fn id_campo(c: &Campo) -> String { format!("campo-{}-{}", c.widget.0, c.widget.1) }

pub fn id_nome() -> &'static str { "campo-nome" }

/// Dias desde 1/1/1970 (algoritmo de Howard Hinnant), e a volta.
fn dias_de(ano: i32, mes: u32, dia: u32) -> i64 {
    let a = if mes <= 2 { ano - 1 } else { ano } as i64;
    let era = a.div_euclid(400);
    let ae = a - era * 400;
    let m = mes as i64;
    let dda = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + dia as i64 - 1;
    era * 146_097 + ae * 365 + ae / 4 - ae / 100 + dda - 719_468
}

fn data_de(dias: i64) -> (i32, u32, u32) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let de = z - era * 146_097;
    let ae = (de - de / 1460 + de / 36_524 - de / 146_096) / 365;
    let dda = de - (365 * ae + ae / 4 - ae / 100);
    let mp = (5 * dda + 2) / 153;
    let dia = (dda - (153 * mp + 2) / 5 + 1) as u32;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((ae + era * 400 + i64::from(mes <= 2)) as i32, mes, dia)
}

fn hoje() -> (i32, u32, u32) {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    data_de(s.div_euclid(86_400))
}

fn dias_no_mes(ano: i32, mes: u32) -> u32 {
    let (a, m) = if mes == 12 { (ano + 1, 1) } else { (ano, mes + 1) };
    (dias_de(a, m, 1) - dias_de(ano, mes, 1)) as u32
}

/// "dd/mm/aaaa" -> (ano, mês, dia).
fn ler_data(t: &str) -> Option<(i32, u32, u32)> {
    let mut p = t.trim().split(['/', '-', '.']);
    let (d, m, a) = (p.next()?.parse().ok()?, p.next()?.parse().ok()?, p.next()?.parse::<i32>().ok()?);
    let a = if a < 100 { a + 2000 } else { a };
    ((1..=12).contains(&m) && d >= 1 && d <= dias_no_mes(a, m)).then_some((a, m, d))
}

const MESES: [&str; 12] = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];

/// Ícone de calendário (o catálogo do feather-ui não tem um).
fn icone_calendario(tamanho: f32) -> Element<'static, Msg> {
    let desenho = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#236ac7" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z M16 3v4 M8 3v4 M4 11h16"/></svg>"##;
    svg(Svg::from_bytes(desenho.as_bytes()).expect("ícone")).width(tamanho).height(tamanho).into()
}

fn azul() -> Color { Color::rgb8(0x23, 0x6a, 0xc7) }

/// Valor do campo como vai para a planilha.
fn valor_planilha(c: &Campo) -> String {
    match c.tipo {
        TipoCampo::Caixa => if c.marcada { "VERDADEIRO" } else { "FALSO" }.into(),
        _ if c.valor == "Off" => String::new(),
        _ => c.valor.clone(),
    }
}

/// Nomes (na ordem) e valores dos campos com dado de um documento.
fn linha(campos: &[Campo]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    for c in campos.iter().filter(|c| !matches!(c.tipo, TipoCampo::Assinatura | TipoCampo::Botao)) {
        match v.iter_mut().find(|(n, _)| *n == c.nome) {
            // Opção: vale a marcada do grupo.
            Some((_, x)) => if c.marcada && x.is_empty() { *x = valor_planilha(c) },
            None => v.push((c.nome.clone(), if c.tipo == TipoCampo::Opcao && !c.marcada { String::new() } else { valor_planilha(c) })),
        }
    }
    v
}

fn csv_campo(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// Grava o CSV (cabeçalho = união dos nomes, uma linha por documento) e abre no FeatherSheet.
pub(crate) fn para_sheet(linhas: &[(String, Vec<(String, String)>)], base: &str) -> Result<String, String> {
    let mut cabecalho: Vec<String> = vec!["Arquivo".into()];
    for (_, l) in linhas { for (n, _) in l { if !cabecalho.contains(n) { cabecalho.push(n.clone()); } } }
    let mut csv = cabecalho.iter().map(|c| csv_campo(c)).collect::<Vec<_>>().join(",") + "\n";
    for (arquivo, l) in linhas {
        let mut celulas = vec![csv_campo(arquivo)];
        for n in &cabecalho[1..] { celulas.push(csv_campo(l.iter().find(|(k, _)| k == n).map_or("", |(_, v)| v.as_str()))); }
        csv += &(celulas.join(",") + "\n");
    }
    let pasta = crate::recentes::pasta_temporaria().map_err(|e| format!("Não consegui gravar os dados: {e}"))?;
    let destino = pasta.join(format!("{base} — campos.csv"));
    featherpdf_io::gravar_atomico(&destino, csv.as_bytes()).map_err(|e| format!("Não consegui gravar os dados: {e}"))?;
    crate::abrir_no("feathersheet", "FeatherSheet", &destino)
        .map(|_| format!("{} linha(s) e {} coluna(s) abertas no FeatherSheet.", linhas.len(), cabecalho.len()))
}

fn ler_pasta(pasta: &Path) -> Vec<(String, Vec<(String, String)>)> {
    let mut arquivos: Vec<PathBuf> = std::fs::read_dir(pasta).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))).collect();
    arquivos.sort();
    arquivos.iter().filter_map(|p| {
        let pdf: DocumentoPdf = featherpdf_io::abrir(p).ok()?;
        let l = linha(&pdf.campos());
        (!l.is_empty()).then(|| (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), l))
    }).collect()
}

impl App {
    pub fn novo_campo(&self) -> Option<Novo> {
        match self.ferramenta { Ferramenta::Campo(n) if self.aberto.is_some() => Some(n), _ => None }
    }

    /// Relê os campos depois de uma edição (chamado a cada `update`).
    pub fn reler_campos(&mut self) {
        if self.form.campos.is_none() { self.form.campos = self.aberto.as_ref().map(|a| a.pdf.campos()); }
    }

    /// Grava o campo novo (um Ctrl+Z).
    fn criar_agora(&mut self, c: Criando) -> Task<Msg> {
        let tipo = match c.tipo {
            Novo::Texto if c.caixa[3] - c.caixa[1] > 30.0 => CampoNovo::Multilinha,
            Novo::Texto => CampoNovo::Texto,
            Novo::Caixa => CampoNovo::Caixa,
            Novo::Opcao => CampoNovo::Opcao,
            Novo::Lista => CampoNovo::Lista(c.opcoes.split(';').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect()),
            Novo::Data => CampoNovo::Data,
            Novo::Assinatura => CampoNovo::Assinatura,
        };
        self.editar(move |pdf| pdf.criar_campo(c.pagina, c.caixa, &tipo, &c.nome).map(|n| format!("Campo \u{201c}{n}\u{201d} criado. Ctrl+Z desfaz.")))
    }

    fn campo(&self, i: usize) -> Option<Campo> { self.form.campos.as_ref()?.get(i).cloned() }

    /// Grava o valor sem redesenhar a página: o campo na tela já mostra o texto.
    /// Devolve `true` quando outro campo tinha sido digitado e a página precisa ser redesenhada.
    fn preencher_aqui(&mut self, c: &Campo, valor: &str) -> bool {
        let anterior = self.form.digitando;
        if anterior != Some(c.campo) {
            self.form.digitando = Some(c.campo);
            if let Some(a) = &self.aberto { self.guardar_desfazer(a.pdf.clone()); }
        }
        // Cada tecla muda o documento: um script rodando sobre a versão anterior não pode sobrescrevê-la.
        self.scripts.rodada += 1;
        let Some(a) = &mut self.aberto else { return false };
        match a.pdf.digitar(c, valor) {
            Ok(()) => self.form.campos = None,
            Err(e) => self.status = e,
        }
        anterior.is_some_and(|x| x != c.campo)
    }

    /// A pessoa parou de digitar: o campo com regra ganha a máscara (ou o aviso do que está errado).
    fn aplicar_mascara(&mut self) {
        let (Some(dono), Some(a)) = (self.form.digitando, &mut self.aberto) else { return };
        let Some(c) = a.pdf.campos().into_iter().find(|c| c.campo == dono && c.regra.is_some()) else { return };
        match a.pdf.preencher(&c, &c.valor) {
            Ok(()) => self.form.campos = None,
            Err(e) => self.status = format!("{}: {e}", c.nome),
        }
    }

    pub fn update_formulario(&mut self, m: Fm) -> Task<Msg> {
        match m {
            Fm::Valor(i, t) => if let Some(c) = self.campo(i) {
                if self.preencher_aqui(&c, &t) { return self.refazer_vista(); }
                // Parou de digitar: a página ganha o valor com a própria letra, sem precisar de Enter.
                self.form.geracao += 1;
                self.form.pendente = true;
                let g = self.form.geracao;
                return Task::perform(tokio::time::sleep(std::time::Duration::from_millis(400)), move |_| Msg::Form(Fm::Assentar(g)));
            },
            Fm::Assentar(g) => if g == self.form.geracao && self.form.pendente {
                self.form.pendente = false;
                self.aplicar_mascara();
                return self.refazer_vista();
            },
            Fm::Confirmar => { self.aplicar_mascara(); self.form.digitando = None; self.form.pendente = false; return self.refazer_vista(); }
            Fm::Calendario(i) => {
                self.form.calendario = match self.form.calendario {
                    Some((j, ..)) if j == i => None,
                    _ => {
                        let (a, m, _) = self.campo(i).and_then(|c| ler_data(&c.valor)).unwrap_or_else(hoje);
                        Some((i, a, m))
                    }
                };
            }
            Fm::Mes(passo) => if let Some((_, a, m)) = &mut self.form.calendario {
                let total = *a * 12 + *m as i32 - 1 + passo;
                (*a, *m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
            },
            Fm::Dia(a, m, d) => if let Some((i, ..)) = self.form.calendario.take() {
                let Some(c) = self.campo(i) else { return Task::none() };
                self.preencher_aqui(&c, &format!("{d:02}/{m:02}/{a}"));
                self.form.digitando = None;
                self.form.pendente = false;
                return self.refazer_vista();
            },
            Fm::Alternar(i) => if let Some(c) = self.campo(i) {
                self.form.digitando = None;
                let valor = if c.marcada && c.tipo == TipoCampo::Caixa { "" } else { "sim" };
                return self.editar(move |pdf| pdf.preencher(&c, valor).map(|_| String::new()));
            },
            Fm::Caixa { pagina, a, b } => {
                let Some(tipo) = self.novo_campo() else { return Task::none() };
                let (l, al) = tipo.padrao();
                let z = self.zoom;
                let b = if a.distance(b) < 4.0 { Point::new(a.x + l * z, a.y + al * z) } else { b };
                let Some(caixa) = self.caixa_na_pagina(pagina, a, b) else { return Task::none() };
                // O nome sugerido é o texto logo à esquerda da caixa (o rótulo).
                let rotulo = self.aberto.as_ref().and_then(|a| a.pdf.objetos(pagina).ok()).and_then(|obj| obj.into_iter()
                    .filter_map(|o| match o.tipo { featherpdf_core::TipoObjeto::Texto { texto, .. } if o.caixa[2] <= caixa[0] + 2.0
                        && o.caixa[1] < caixa[3] && o.caixa[3] > caixa[1] && caixa[0] - o.caixa[2] < 160.0 => Some((o.caixa[2], texto)), _ => None })
                    .max_by(|x, y| x.0.total_cmp(&y.0)).map(|(_, t)| t.trim().trim_end_matches([':', '_', ' ']).to_string()))
                    .filter(|t| !t.is_empty()).unwrap_or_else(|| tipo.nome().to_string());
                // Só a lista pergunta algo (as opções); o resto nasce na hora, e o de texto já recebe o cursor.
                if tipo == Novo::Lista {
                    self.form.criando = Some(Criando { pagina, caixa, tipo, nome: rotulo, opcoes: String::new() });
                    return crate::focar(id_nome());
                }
                let novo = self.criar_agora(Criando { pagina, caixa, tipo, nome: rotulo, opcoes: String::new() });
                if !matches!(tipo, Novo::Texto | Novo::Data) || !self.ultima_edicao_ok { return novo; }
                self.ferramenta = Ferramenta::Selecionar;
                self.reler_campos();
                let foco = self.form.campos.as_ref().and_then(|v| v.iter().rev().find(|c| c.pagina == pagina && c.caixa == caixa)).map(id_campo);
                return Task::batch([novo].into_iter().chain(foco.map(crate::focar)));
            }
            Fm::Nome(t) => if let Some(c) = &mut self.form.criando { c.nome = t },
            Fm::Opcoes(t) => if let Some(c) = &mut self.form.criando { c.opcoes = t },
            Fm::Fechar => self.form.criando = None,
            Fm::Criar => {
                let Some(c) = self.form.criando.take() else { return Task::none() };
                return self.criar_agora(c);
            }
            Fm::Detectar => {
                let total = self.total_paginas();
                return self.editar(move |pdf| {
                    let mut n = 0;
                    for p in 1..=total {
                        for (caixa, tipo, nome) in pdf.detectar_campos(p)? {
                            let nome = if nome.is_empty() { "Campo".to_string() } else { nome };
                            pdf.criar_campo(p, caixa, &tipo, &nome)?;
                            n += 1;
                        }
                    }
                    if n == 0 { return Err("Não achei linhas ____, traços nem quadradinhos para virar campo.".into()); }
                    Ok(format!("{n} campo(s) criados. Confira na página; Ctrl+Z desfaz."))
                });
            }
            Fm::Limpar => return self.editar(|pdf| pdf.limpar_campos().map(|n| format!("{n} campo(s) esvaziados. Ctrl+Z desfaz."))),
            Fm::ParaSheet => {
                let Some(campos) = &self.form.campos else { return Task::none() };
                let nome = self.nome_documento().unwrap_or_default();
                let base = Path::new(&nome).file_stem().map_or("PDF".into(), |n| n.to_string_lossy().into_owned());
                self.status = para_sheet(&[(nome.clone(), linha(campos))], &base).unwrap_or_else(|e| e);
            }
            Fm::Pasta => return Task::perform(async {
                rfd::AsyncFileDialog::new().set_title("Pasta com as fichas em PDF").pick_folder().await.map(|h| h.path().to_path_buf())
            }, |p| Msg::Form(Fm::PastaEscolhida(p))),
            Fm::Modelo(m) => {
                let novo = featherpdf_io::criar_a4().and_then(|mut pdf| {
                    let n = pdf.aplicar_modelo(1, m)?;
                    Ok((featherpdf_io::Visualizador::novo(pdf.serializar()?)?, pdf, n))
                });
                match novo {
                    Ok((vis, pdf, n)) => {
                        let tarefa = self.entrar(pdf, vis);
                        self.status = format!("{m}: {n} campos prontos para preencher. Salve com Ctrl+S.");
                        return tarefa;
                    }
                    Err(e) => self.status = e,
                }
            }
            Fm::PastaEscolhida(None) => {}
            Fm::PastaEscolhida(Some(pasta)) => {
                let linhas = ler_pasta(&pasta);
                let base = pasta.file_name().map_or("Pasta".into(), |n| n.to_string_lossy().into_owned());
                self.status = if linhas.is_empty() { format!("Nenhum PDF com campos preenchíveis em {}.", pasta.display()) }
                    else { para_sheet(&linhas, &base).unwrap_or_else(|e| e) };
            }
        }
        Task::none()
    }

    /// Campos da página `n` prontos para preencher (fora das ferramentas de edição).
    pub fn camada_formulario(&self, n: u32, l: f32, a: f32) -> Option<Element<'_, Msg>> {
        if self.novo_campo().is_some() {
            return Some(crate::inserir::camada_arrastar(n, |pagina, a, b| Msg::Form(Fm::Caixa { pagina, a, b }), l, a));
        }
        if self.ferramenta != Ferramenta::Selecionar { return None; }
        let campos = self.form.campos.as_ref()?;
        if !campos.iter().any(|c| c.pagina == n) { return None; }
        let (pag, giro) = self.aberto.as_ref()?.pdf.geometria(n).ok()?;
        let mut pilha = Stack::new().width(l).height(a);
        // Assinatura já feita mostra a própria aparência, sem o realce de campo por cima.
        for (i, c) in campos.iter().enumerate().filter(|(_, c)| c.pagina == n && !(c.tipo == TipoCampo::Assinatura && !c.valor.is_empty())) {
            let r = na_tela(c.caixa, pag, giro, self.zoom);
            let (w, h) = (r.width.max(8.0), r.height.max(8.0));
            let fundo = move |destaque: bool| move |_: &Theme| Style {
                background: Some(Background::Color(Color { a: if destaque { 0.25 } else { 0.12 }, ..azul() })),
                border: Border { color: azul(), width: 1.0, radius: [2.0; 4] }, ..Default::default()
            };
            let widget: Element<'_, Msg> = match &c.tipo {
                TipoCampo::Texto { multilinha } => {
                    // O fundo e a borda ficam na caixa inteira do campo; o texto entra sem moldura própria.
                    let multilinha = *multilinha || h > 30.0;
                    let vazio = c.valor.is_empty();
                    // O campo digitado mostra o nosso texto até a página ser redesenhada (pausa, Enter ou outro campo).
                    let digitando = self.form.digitando == Some(c.campo) && self.form.pendente;
                    let z = self.zoom;
                    // A mesma letra e o mesmo recuo (2 pt) que a aparência grava na página.
                    let entrada = text_input("", &c.valor).id(id_campo(c)).on_input(move |t| Msg::Form(Fm::Valor(i, t)))
                        .on_submit(Msg::Form(Fm::Confirmar)).size((c.tamanho * z).max(6.0)).font(Font::with_name("Liberation Sans"))
                        .padding(Padding { top: 0.0, bottom: 0.0, left: 2.0 * z, right: 0.0 }).width(Length::FILL)
                        .style(move |tema, status| {
                            // Fora da edição a página mostra o valor com a própria aparência; o nosso texto
                            // só aparece (com fundo opaco por cima) enquanto o campo tem o foco.
                            let editando = digitando || matches!(status, Status::Focused);
                            let mut s = campo_padrao(tema, status);
                            s.background = Some(Background::Color(if editando { Color::WHITE } else { Color::TRANSPARENT }));
                            s.text_color = Some(if editando || vazio { Color::BLACK } else { Color::TRANSPARENT });
                            s.border = if editando { Border { color: azul(), width: 1.0, radius: [2.0; 4] } } else { Border::default() };
                            s
                        });
                    // Sem moldura: o PDF já desenha as dele. Vazio ganha só um realce leve, para achar onde preencher.
                    let fundo = if vazio { Color { a: 0.08, ..azul() } } else { Color::TRANSPARENT };
                    let conteudo: Element<'_, Msg> = if c.data {
                        let lado = (h - 2.0).clamp(10.0, 18.0);
                        row![entrada, button(icone_calendario(lado)).padding(1).style(botao_texto).on_press(Msg::Form(Fm::Calendario(i)))]
                            .align_y(Align::Center).into()
                    } else { entrada.into() };
                    let caixa = container(conteudo).width(w).height(h).padding([if multilinha { 2.0 * z } else { 0.0 }, 0.0])
                        .align_y(if multilinha { Align::Start } else { Align::Center })
                        .style(move |_| Style { background: Some(Background::Color(fundo)), ..Default::default() });
                    caixa.into()
                }
                TipoCampo::Lista { opcoes } => suspensa(opcoes.clone(), (!c.valor.is_empty()).then(|| c.valor.clone()),
                    move |v| Msg::Form(Fm::Valor(i, v))).text_size((h * 0.6).clamp(6.0, 14.0)).padding([0, 4]).width(w).into(),
                TipoCampo::Caixa | TipoCampo::Opcao => button(Space::new(w, h)).padding(0).on_press(Msg::Form(Fm::Alternar(i)))
                    .style(move |_, status| Style {
                        // A moldura e a marca vêm da aparência do próprio PDF; aqui só o realce do mouse.
                        background: (status == Status::Hovered).then_some(Background::Color(Color { a: 0.18, ..azul() })),
                        border: Border { radius: [if c.tipo == TipoCampo::Opcao { h / 2.0 } else { 2.0 }; 4], ..Border::default() },
                        ..Default::default()
                    }).into(),
                // Campo de assinatura vazio: o clique abre o painel Preencher e assinar.
                TipoCampo::Assinatura => button(Space::new(w, h)).padding(0)
                    .on_press_maybe((self.painel != Some(crate::Painel::Assinar)).then_some(Msg::Assin(crate::assinar::As::Abrir)))
                    .style(move |_, status| Style {
                        background: Some(Background::Color(Color { a: if status == Status::Hovered { 0.25 } else { 0.12 }, ..azul() })),
                        border: Border { color: azul(), width: 1.0, radius: [2.0; 4] }, ..Default::default()
                    }).into(),
                TipoCampo::Botao => container(Space::new(w, h)).style(fundo(false)).into(),
            };
            pilha = pilha.push(column![Space::with_height(r.y), row![Space::with_width(r.x), container(widget).width(w).height(h)]]);
        }
        // Calendário por cima de tudo, logo abaixo do campo (acima, se não couber).
        if let Some((i, ano, mes)) = self.form.calendario {
            if let Some(c) = campos.get(i).filter(|c| c.pagina == n) {
                let r = na_tela(c.caixa, pag, giro, self.zoom);
                let (larg, alt) = (238.0, 236.0);
                let y = if r.y + r.height + alt + 4.0 <= a { r.y + r.height + 2.0 } else { (r.y - alt - 2.0).max(0.0) };
                let x = r.x.min((l - larg).max(0.0));
                pilha = pilha.push(column![Space::with_height(y), row![Space::with_width(x), self.calendario(i, c, ano, mes)]]);
            }
        }
        Some(pilha.into())
    }

    /// Mês em grade, a semana começando no domingo; hoje e o dia escolhido em destaque.
    fn calendario(&self, i: usize, c: &Campo, ano: i32, mes: u32) -> Element<'_, Msg> {
        let escolhido = ler_data(&c.valor);
        let agora = hoje();
        let negrito = Font::SEMIBOLD;
        let seta = |nome: &'static str, passo: i32| button(feather_ui::icones::icone(nome, 14.0)).padding(4).style(botao_texto).on_press(Msg::Form(Fm::Mes(passo)));
        let topo = row![seta("chevron-left", -1), text(format!("{} de {ano}", MESES[mes as usize - 1])).size(13).font(negrito)
            .width(Length::FILL).align_x(Align::Center), seta("chevron-right", 1)].align_y(Align::Center);
        let mut grade = column![row(["D", "S", "T", "Q", "Q", "S", "S"].map(|d| text(d).size(11).color(Color::rgb8(0x80, 0x80, 0x80))
            .width(30).align_x(Align::Center).into()))].spacing(2);
        let inicio = (dias_de(ano, mes, 1) + 4).rem_euclid(7) as u32; // 1/1/1970 foi quinta-feira
        let total = dias_no_mes(ano, mes);
        for semana in 0..6 {
            let celulas = (0..7).map(|d| {
                let dia = (semana * 7 + d) as i64 - inicio as i64 + 1;
                if dia < 1 || dia > total as i64 { return Space::new(30, 26).into(); }
                let dia = dia as u32;
                let marcado = escolhido == Some((ano, mes, dia));
                let e_hoje = agora == (ano, mes, dia);
                button(text(dia.to_string()).size(12).align_x(Align::Center).width(Length::FILL))
                    .width(30).height(26).padding([5, 0]).on_press(Msg::Form(Fm::Dia(ano, mes, dia)))
                    .style(move |_, status| Style {
                        background: if marcado { Some(Background::Color(azul())) }
                            else if status == Status::Hovered { Some(Background::Color(Color { a: 0.15, ..azul() })) } else { None },
                        text_color: Some(if marcado { Color::WHITE } else { feather_ui::tema::COR_TEXTO() }),
                        border: Border { color: azul(), width: if e_hoje && !marcado { 1.0 } else { 0.0 }, radius: [13.0; 4] },
                        ..Default::default()
                    }).into()
            });
            grade = grade.push(row(celulas).spacing(2));
            if (semana + 1) * 7 >= inicio + total { break; }
        }
        let (ha, hm, hd) = agora;
        let rodape = row![
            button(text("Hoje").size(12)).padding([4, 8]).style(botao_texto).on_press(Msg::Form(Fm::Dia(ha, hm, hd))),
            Space::with_width(Length::FILL),
            button(text("Fechar").size(12)).padding([4, 8]).style(botao_texto).on_press(Msg::Form(Fm::Calendario(i))),
        ];
        container(column![topo, grade, rodape].spacing(6)).width(238).padding(8)
            // Cores do tema: com branco fixo, no escuro o mês (texto do tema, claro) sumia no fundo.
            .style(|_| Style {
                background: Some(Background::Color(feather_ui::tema::COR_PAINEL())),
                text_color: Some(feather_ui::tema::COR_TEXTO()),
                border: Border { color: feather_ui::tema::COR_BORDA(), width: 1.0, radius: [6.0; 4] },
                shadow: Some(Shadow { color: Color { a: 0.18, ..Color::BLACK }, offset: Point::new(0.0, 3.0), blur: 10.0 }),
                ..Default::default()
            }).into()
    }

    /// Caixa que pede o nome (e as opções da lista) do campo novo.
    pub fn com_dialogo_campo<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(c) = &self.form.criando else { return crate::inicio::sem_camada(base) };
        let mut corpo = column![
            text(if c.tipo == Novo::Opcao { "Nome do grupo (opções com o mesmo nome se excluem)" } else { "Nome do campo (vira a coluna na planilha)" }).size(12),
            text_input("Nome", &c.nome).id(id_nome()).on_input(|t| Msg::Form(Fm::Nome(t))).on_submit(Msg::Form(Fm::Criar)).padding([6, 8]).size(13),
        ].spacing(8);
        if c.tipo == Novo::Lista {
            corpo = corpo.push(text("Opções, separadas por ;").size(12)).push(
                text_input("Transformador; Disjuntor; Seccionadora", &c.opcoes).on_input(|t| Msg::Form(Fm::Opcoes(t)))
                    .on_submit(Msg::Form(Fm::Criar)).padding([6, 8]).size(13));
        }
        let pronto = !c.nome.trim().is_empty() && (c.tipo != Novo::Lista || c.opcoes.split(';').any(|o| !o.trim().is_empty()));
        if c.tipo == Novo::Assinatura {
            corpo = corpo.push(dlg::aviso("info-circle", "Campo vazio onde alguém vai assinar depois (Proteger › Assinar)."));
        }
        Dialogo::new(c.tipo.nome(), corpo).icone("forms")
            .primario("Criar campo", pronto.then_some(Msg::Form(Fm::Criar)))
            .ao_fechar(Msg::Form(Fm::Fechar)).topo(200.0).sobre(base)
    }

    pub fn faixa_campos(&self) -> Vec<(&'static str, &'static str, Novo)> {
        vec![("input", "Texto", Novo::Texto), ("checkbox", "Caixa", Novo::Caixa), ("circle-dot", "Opção", Novo::Opcao),
             ("list-select", "Lista", Novo::Lista), ("clock", "Data", Novo::Data), ("signature", "Assinatura", Novo::Assinatura)]
    }
}
