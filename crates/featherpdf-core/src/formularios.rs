//! Formulários (AcroForm): ler os campos, preencher, criar campo novo,
//! detectar campos num PDF "chapado" e limpar. Cada valor gravado leva a
//! aparência pronta (/AP), para o campo aparecer igual em qualquer leitor.

use std::collections::{HashMap, HashSet};

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use crate::anotacoes::{hexa, retangulo};
use crate::inserir::{cor, win_ansi};
use crate::objetos::{ItemTj, TipoObjeto};
use crate::DocumentoPdf;
use feather_draw::{largura_texto, FontePdf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TipoCampo { Texto { multilinha: bool }, Caixa, Opcao, Lista { opcoes: Vec<String> }, Assinatura, Botao }

/// Um widget de campo (o pedaço clicável na página).
#[derive(Debug, Clone, PartialEq)]
pub struct Campo {
    pub widget: ObjectId,
    /// Onde mora o /V (o próprio widget ou o campo pai).
    pub campo: ObjectId,
    pub nome: String,
    pub tipo: TipoCampo,
    pub valor: String,
    pub marcada: bool,
    /// Nome do estado ligado da caixa/opção (o que vai no /V).
    pub ligado: String,
    pub pagina: u32,
    pub caixa: [f32; 4],
    /// Letra (pt) com que a aparência escreve o valor: a do /DA, ou a automática.
    pub tamanho: f32,
    /// Campo de data (o nosso, pela dica /TU, ou o do Acrobat, pelo AFDate no formato).
    pub data: bool,
    /// Máscara e validação (`regras.rs`).
    pub regra: Option<crate::Regra>,
    pub calculo: Option<crate::Calculo>,
    /// Máximo de caracteres (/MaxLen).
    pub limite: Option<usize>,
    /// Bit "Required" do /Ff: o formulário não está completo sem ele.
    pub obrigatorio: bool,
    /// Bit "ReadOnly" do /Ff: ninguém digita nele (o cálculo ainda escreve).
    pub so_leitura: bool,
    /// Valor padrão (/DV): o que o campo volta a ter em "Limpar campos".
    pub padrao: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CampoNovo { Texto, Multilinha, Caixa, Opcao, Lista(Vec<String>), Data, Assinatura }

const FF_SO_LEITURA: i64 = 1;
const FF_OBRIGATORIO: i64 = 1 << 1;
const FF_MULTILINHA: i64 = 1 << 12;
const FF_SEM_DESLIGAR: i64 = 1 << 14;
const FF_RADIO: i64 = 1 << 15;
const FF_BOTAO: i64 = 1 << 16;
const FF_COMBO: i64 = 1 << 17;
const AZUL_CAMPO: [f32; 3] = [0.14, 0.42, 0.78];

/// Tamanho da letra no /DA ("/Helv 10 Tf 0 g"); 0 é automático.
fn tamanho_do_da(da: &str) -> f32 {
    let p: Vec<&str> = da.split_whitespace().collect();
    p.iter().position(|x| *x == "Tf").and_then(|i| i.checked_sub(1)).and_then(|i| p[i].parse().ok()).unwrap_or(0.0)
}

/// Os obrigatórios ainda vazios entre `campos` (quem já leu os campos não relê o PDF).
pub fn obrigatorios_vazios_de(campos: &[Campo]) -> Vec<String> {
    let mut vistos = HashSet::new();
    campos.iter().filter(|c| c.obrigatorio && vistos.insert(c.campo))
        .filter(|c| match c.tipo { TipoCampo::Caixa | TipoCampo::Opcao => c.valor.is_empty() || c.valor == "Off", _ => c.valor.trim().is_empty() })
        .map(|c| c.nome.clone()).collect()
}

impl DocumentoPdf {
    pub(crate) fn dict(&self, o: &Object) -> Option<Dictionary> {
        match o { Object::Reference(r) => self.documento.get_dictionary(*r).ok().cloned(), o => o.as_dict().ok().cloned() }
    }

    pub(crate) fn acroform(&self) -> Option<Dictionary> {
        self.dict(self.documento.catalog().ok()?.get(b"AcroForm").ok()?)
    }

    /// Todos os widgets de campo do documento, na ordem do /Fields.
    pub fn campos(&self) -> Vec<Campo> {
        let Some(af) = self.acroform() else { return vec![] };
        // Widget -> página, pelas anotações das páginas.
        let mut paginas: HashMap<ObjectId, u32> = HashMap::new();
        for (n, id) in self.documento.get_pages() {
            for a in self.lista_anotacoes(id) { if let Ok(r) = a.as_reference() { paginas.insert(r, n); } }
        }
        let mut saida = Vec::new();
        let raizes: Vec<ObjectId> = af.get(b"Fields").ok().and_then(|o| self.resolver_array(o))
            .map(|v| v.iter().filter_map(|x| x.as_reference().ok()).collect()).unwrap_or_default();
        // Um /Kids que volta para trás faria a leitura crescer em exponencial: cada campo uma vez.
        let mut vistos = HashSet::new();
        for r in raizes { self.ler_campo(r, String::new(), None, 0, None, &paginas, &mut saida, 0, &mut vistos); }
        saida
    }

    /// Tira da árvore do /AcroForm os widgets em `fora` e os campos que ficarem sem widget.
    pub(crate) fn tirar_campos(&mut self, fora: &std::collections::HashSet<ObjectId>) {
        fn podar(pdf: &mut DocumentoPdf, lista: Vec<Object>, fora: &HashSet<ObjectId>, fundo: usize, vistos: &mut HashSet<ObjectId>) -> Vec<Object> {
            let mut ficam = vec![];
            for o in lista {
                let Ok(r) = o.as_reference() else { ficam.push(o); continue };
                if fora.contains(&r) { continue; }
                // Já visto (um /Kids que volta para trás): fica como está, sem descer de novo.
                if !vistos.insert(r) { ficam.push(o); continue; }
                let filhos = pdf.documento.get_dictionary(r).ok().and_then(|d| d.get(b"Kids").ok().cloned()).and_then(|k| pdf.resolver_array(&k));
                match filhos {
                    Some(k) if fundo < 32 => {
                        let novos = podar(pdf, k, fora, fundo + 1, vistos);
                        if novos.is_empty() { continue; }
                        if let Ok(d) = pdf.documento.get_dictionary_mut(r) { d.set("Kids", novos); }
                    }
                    _ => {}
                }
                ficam.push(o);
            }
            ficam
        }
        let Some(af) = self.acroform() else { return };
        let Some(campos) = af.get(b"Fields").ok().and_then(|o| self.resolver_array(o)) else { return };
        let ficam = podar(self, campos, fora, 0, &mut HashSet::new());
        let alvo = match self.documento.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()) {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        };
        let af = match alvo {
            Some(r) => self.documento.get_dictionary_mut(r).ok(),
            None => self.documento.catalog_mut().ok().and_then(|c| c.get_mut(b"AcroForm").ok()).and_then(|o| o.as_dict_mut().ok()),
        };
        if let Some(af) = af { af.set("Fields", ficam); }
    }

    /// O /DA do widget, ou o padrão do /AcroForm.
    fn da_do_widget(&self, widget: ObjectId) -> String {
        let texto = |o: &Object| o.as_str().ok().map(|s| String::from_utf8_lossy(s).into_owned());
        self.documento.get_dictionary(widget).ok().and_then(|d| d.get(b"DA").ok()).and_then(texto)
            // Sem clonar o /AcroForm inteiro (com o /Fields) a cada widget.
            .or_else(|| {
                let af = self.documento.catalog().ok()?.get(b"AcroForm").ok()?;
                let af = match af { Object::Reference(r) => self.documento.get_dictionary(*r).ok()?, o => o.as_dict().ok()? };
                af.get(b"DA").ok().and_then(texto)
            })
            .unwrap_or_default()
    }

    /// /Opt como (valor de exportação, texto mostrado); opção simples tem os dois iguais.
    fn pares_opt(&self, d: &Dictionary) -> Vec<(String, String)> {
        let texto = |x: &Object| lopdf::decode_text_string(x).ok();
        d.get(b"Opt").ok().and_then(|o| self.resolver_array(o)).unwrap_or_default().iter().filter_map(|o| match o {
            Object::Array(par) => Some((texto(par.first()?)?, texto(par.get(1)?)?)),
            o => texto(o).map(|t| (t.clone(), t)),
        }).collect()
    }

    pub(crate) fn resolver_array(&self, o: &Object) -> Option<Vec<Object>> {
        match o { Object::Reference(r) => self.documento.get_object(*r).ok()?.as_array().ok().cloned(), o => o.as_array().ok().cloned() }
    }

    #[allow(clippy::too_many_arguments)]
    fn ler_campo(&self, id: ObjectId, pai: String, ft: Option<Vec<u8>>, ff: i64, v: Option<(ObjectId, Object)>,
                 paginas: &HashMap<ObjectId, u32>, saida: &mut Vec<Campo>, fundo: usize, vistos: &mut HashSet<ObjectId>) {
        if fundo > 32 || !vistos.insert(id) { return; }
        let Ok(d) = self.documento.get_dictionary(id) else { return };
        let parte = d.get(b"T").ok().and_then(|t| lopdf::decode_text_string(t).ok());
        let nome = match (&parte, pai.is_empty()) { (Some(p), true) => p.clone(), (Some(p), false) => format!("{pai}.{p}"), (None, _) => pai.clone() };
        let ft = d.get(b"FT").and_then(Object::as_name).ok().map(<[u8]>::to_vec).or(ft);
        let ff = d.get(b"Ff").and_then(Object::as_i64).unwrap_or(ff);
        let v = d.get(b"V").ok().map(|o| (id, o.clone())).or(v);
        let filhos: Vec<ObjectId> = d.get(b"Kids").ok().and_then(|o| self.resolver_array(o))
            .map(|k| k.iter().filter_map(|x| x.as_reference().ok()).collect()).unwrap_or_default();
        if !filhos.is_empty() {
            for f in filhos { self.ler_campo(f, nome.clone(), ft.clone(), ff, v.clone(), paginas, saida, fundo + 1, vistos); }
            return;
        }
        if d.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Widget") && !d.has(b"Rect") { return; }
        let pares = self.pares_opt(d);
        let opcoes: Vec<String> = pares.iter().map(|(_, mostra)| mostra.clone()).collect();
        let tipo = match ft.as_deref() {
            Some(b"Tx") => TipoCampo::Texto { multilinha: ff & FF_MULTILINHA != 0 },
            Some(b"Btn") if ff & FF_BOTAO != 0 => TipoCampo::Botao,
            Some(b"Btn") if ff & FF_RADIO != 0 => TipoCampo::Opcao,
            Some(b"Btn") => TipoCampo::Caixa,
            Some(b"Ch") => TipoCampo::Lista { opcoes },
            Some(b"Sig") => TipoCampo::Assinatura,
            _ => return,
        };
        let ligado = d.get(b"AP").ok().and_then(|ap| self.dict(ap)).and_then(|ap| ap.get(b"N").ok().and_then(|n| self.dict(n)))
            .and_then(|n| n.iter().map(|(k, _)| k.clone()).find(|k| k != b"Off"))
            .map(|k| String::from_utf8_lossy(&k).into_owned()).unwrap_or_else(|| "Yes".into());
        let (campo, mut valor) = match &v {
            Some((dono, o)) => (*dono, match o {
                Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
                o => lopdf::decode_text_string(o).unwrap_or_default(),
            }),
            None => (id, String::new()),
        };
        // A lista mostra o texto da opção; o /V guarda o valor de exportação.
        if let TipoCampo::Lista { .. } = tipo { if let Some((_, m)) = pares.iter().find(|(e, _)| *e == valor) { valor = m.clone(); } }
        // O /V da assinatura é um dicionário: marca só que já foi assinado.
        if tipo == TipoCampo::Assinatura && v.is_some() { valor = "assinado".into(); }
        let estado = d.get(b"AS").and_then(Object::as_name).ok().map(|n| String::from_utf8_lossy(n).into_owned());
        let marcada = match tipo { TipoCampo::Caixa | TipoCampo::Opcao => estado.map_or(valor == ligado, |e| e == ligado), _ => false };
        let n: Vec<f32> = d.get(b"Rect").and_then(Object::as_array).map(|r| r.iter().filter_map(|x| x.as_float().ok()).collect()).unwrap_or_default();
        if n.len() != 4 { return; }
        let pagina = paginas.get(&id).copied().or_else(|| d.get(b"P").and_then(Object::as_reference).ok()
            .and_then(|p| self.documento.get_pages().into_iter().find(|(_, x)| *x == p).map(|(n, _)| n))).unwrap_or(1);
        // Campo sem /V próprio nem herdado: o /V fica no dono do nome (o pai do widget sem /T).
        let campo = if v.is_none() && parte.is_none() { d.get(b"Parent").and_then(Object::as_reference).unwrap_or(id) } else { campo };
        let caixa = [n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])];
        // A mesma conta do `aparencia_texto`, para a tela escrever com a letra que vai para a página.
        let tamanho = match tamanho_do_da(&self.da_do_widget(id)) {
            t if t > 0.0 => t,
            _ if matches!(tipo, TipoCampo::Texto { multilinha: true }) => 10.0,
            _ => ((caixa[3] - caixa[1]) * 0.65).clamp(4.0, 12.0),
        };
        let dica = d.get(b"TU").ok().and_then(|t| lopdf::decode_text_string(t).ok()).unwrap_or_default();
        let formato = d.get(b"AA").ok().and_then(|aa| self.dict(aa)).and_then(|aa| aa.get(b"F").ok().and_then(|f| self.dict(f)))
            .and_then(|f| f.get(b"JS").ok().and_then(|js| js.as_str().ok()).map(|js| String::from_utf8_lossy(js).into_owned()));
        let data = matches!(tipo, TipoCampo::Texto { multilinha: false })
            && (dica.starts_with("Data (") || formato.is_some_and(|js| js.contains("AFDate_")));
        let dono = self.documento.get_dictionary(campo).ok();
        let (regra, calculo, limite) = crate::regras::ler_regras(self, &[Some(d), dono].into_iter().flatten().collect::<Vec<_>>());
        let padrao = [dono, Some(d)].into_iter().flatten().find_map(|x| x.get(b"DV").ok()).map(|o| match o {
            // Caixa e opção: o nome do estado; "Off" é desmarcada.
            Object::Name(n) if n.as_slice() == b"Off" => String::new(),
            Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
            o => lopdf::decode_text_string(o).unwrap_or_default(),
        }).unwrap_or_default();
        let (obrigatorio, so_leitura) = (ff & FF_OBRIGATORIO != 0, ff & FF_SO_LEITURA != 0);
        saida.push(Campo { widget: id, campo, nome, tipo, valor, marcada, ligado, pagina, caixa, tamanho, data, regra, calculo, limite, obrigatorio, so_leitura, padrao });
    }

    /// Aparência de texto do widget com `valor` (borda fina, Helvetica).
    fn aparencia_texto(&mut self, c: &Campo, valor: &str, da: &str) -> ObjectId {
        let (l, a) = (c.caixa[2] - c.caixa[0], c.caixa[3] - c.caixa[1]);
        let multilinha = matches!(c.tipo, TipoCampo::Texto { multilinha: true });
        let t = match tamanho_do_da(da) { t if t > 0.0 => t, _ if multilinha => 10.0, _ => (a * 0.65).clamp(4.0, 12.0) };
        let mut conteudo = String::from("/Tx BMC q BT /Helv ");
        conteudo += &format!("{t:.2} Tf 0 g\n");
        let linhas: Vec<String> = if multilinha {
            let mut v = Vec::new();
            for p in valor.lines() {
                let mut atual = String::new();
                for w in p.split(' ') {
                    let teste = if atual.is_empty() { w.to_string() } else { format!("{atual} {w}") };
                    if largura_texto(&teste, FontePdf::Helvetica, t) > l - 4.0 && !atual.is_empty() { v.push(std::mem::replace(&mut atual, w.to_string())); } else { atual = teste; }
                }
                v.push(atual);
            }
            v
        } else { vec![valor.replace('\n', " ")] };
        let mut y = if multilinha { a - 2.0 - t * 0.85 } else { (a - t * 0.7) / 2.0 };
        for linha in linhas {
            if y < -t { break; }
            conteudo += &format!("1 0 0 1 2 {y:.2} Tm {} Tj\n", hexa(&linha));
            y -= t * 1.15;
        }
        conteudo += "ET Q EMC";
        self.documento.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => retangulo([0.0, 0.0, l, a]),
            "Resources" => dictionary! { "Font" => dictionary! { "Helv" => DocumentoPdf::fonte_helvetica(false) } },
        }, conteudo.into_bytes()))
    }

    /// Grava o valor do campo: texto e lista recebem o texto; caixa e opção
    /// ficam marcadas com valor não vazio. Confere a regra e o limite do campo e refaz as contas.
    pub fn preencher(&mut self, c: &Campo, valor: &str) -> Result<(), String> {
        if c.so_leitura { return Err(format!("“{}” é somente leitura.", c.nome)); }
        if c.calculo.is_some() { return Err(format!("“{}” é calculado: muda sozinho quando os campos da conta mudam.", c.nome)); }
        let valor = match c.regra { Some(r) => r.aplicar(valor)?, None => valor.to_string() };
        if let Some(n) = c.limite.filter(|&n| valor.chars().count() > n) { return Err(format!("“{}” aceita até {n} caracteres.", c.nome)); }
        self.gravar_valor(c, &valor)?;
        self.recalcular()?;
        Ok(())
    }

    /// Enquanto a pessoa digita: grava o texto como está (a máscara entra no [`Self::preencher`]
    /// ao confirmar), mas já respeita o limite e refaz as contas.
    pub fn digitar(&mut self, c: &Campo, valor: &str) -> Result<(), String> {
        if c.so_leitura { return Err(format!("“{}” é somente leitura.", c.nome)); }
        if c.calculo.is_some() { return Err(format!("“{}” é calculado: muda sozinho quando os campos da conta mudam.", c.nome)); }
        if let Some(n) = c.limite.filter(|&n| valor.chars().count() > n) { return Err(format!("“{}” aceita até {n} caracteres.", c.nome)); }
        self.gravar_valor(c, valor)?;
        self.recalcular()?;
        Ok(())
    }

    pub(crate) fn gravar_valor(&mut self, c: &Campo, valor: &str) -> Result<(), String> {
        let todos = self.campos();
        self.gravar_valor_em(c, valor, &todos)
    }

    /// O mesmo, com a lista de campos já lida (`todos`), para quem grava muitos de uma vez.
    fn gravar_valor_em(&mut self, c: &Campo, valor: &str, todos: &[Campo]) -> Result<(), String> {
        let faltam: String = valor.chars().filter(|&ch| ch != '\n' && win_ansi(ch).is_none()).collect();
        if !faltam.is_empty() { return Err(format!("A Helvetica do formulário não tem: {faltam}")); }
        match c.tipo {
            TipoCampo::Texto { .. } | TipoCampo::Lista { .. } => {
                let da = self.da_do_widget(c.widget);
                // Opção com par [(BR) (Brasil)]: a tela mostra "Brasil", o /V guarda "BR".
                let pares = [c.widget, c.campo].iter().filter_map(|&i| self.documento.get_dictionary(i).ok()).map(|d| self.pares_opt(d)).find(|p| !p.is_empty()).unwrap_or_default();
                let exportado = pares.iter().find(|(_, m)| m == valor).map_or(valor, |(e, _)| e.as_str());
                self.documento.get_dictionary_mut(c.campo).map_err(|e| e.to_string())?.set("V", lopdf::text_string(exportado));
                // Todos os widgets do mesmo campo mostram o mesmo valor.
                for w in todos.iter().filter(|x| x.campo == c.campo) {
                    let ap = self.aparencia_texto(w, valor, &da);
                    self.documento.get_dictionary_mut(w.widget).map_err(|e| e.to_string())?.set("AP", dictionary! { "N" => Object::Reference(ap) });
                }
            }
            TipoCampo::Caixa | TipoCampo::Opcao => {
                let ligar = !valor.is_empty();
                let estado = if ligar { c.ligado.as_bytes().to_vec() } else { b"Off".to_vec() };
                self.documento.get_dictionary_mut(c.campo).map_err(|e| e.to_string())?.set("V", Object::Name(estado.clone()));
                for w in todos.iter().filter(|x| x.campo == c.campo) {
                    let as_ = if ligar && w.widget == c.widget || (ligar && c.tipo == TipoCampo::Caixa) { w.ligado.as_bytes().to_vec() } else { b"Off".to_vec() };
                    self.documento.get_dictionary_mut(w.widget).map_err(|e| e.to_string())?.set("AS", Object::Name(as_));
                }
            }
            TipoCampo::Assinatura | TipoCampo::Botao => return Err("Esse campo não guarda valor.".into()),
        }
        self.modificado = true;
        Ok(())
    }

    /// Volta todos os campos ao valor padrão (/DV), ou vazio quando não têm.
    pub fn limpar_campos(&mut self) -> Result<usize, String> {
        let campos = self.campos();
        let mut feitos = HashSet::new();
        for c in campos.iter().filter(|c| !matches!(c.tipo, TipoCampo::Assinatura | TipoCampo::Botao)) {
            if !feitos.insert(c.campo) { continue; }
            // Grupo de opções: o padrão é o estado de um dos botões; liga esse, não o primeiro.
            let (alvo, valor) = match c.tipo {
                TipoCampo::Opcao if !c.padrao.is_empty() => match campos.iter().find(|w| w.campo == c.campo && w.ligado == c.padrao) {
                    Some(w) => (w, c.padrao.as_str()),
                    None => (c, ""),
                },
                _ => (c, c.padrao.as_str()),
            };
            // Padrão que a fonte do formulário não escreve: o campo fica vazio, os outros seguem.
            if self.gravar_valor_em(alvo, valor, &campos).is_err() { self.gravar_valor_em(alvo, "", &campos)?; }
        }
        self.recalcular()?;
        Ok(feitos.len())
    }

    /// Marca o campo como obrigatório e/ou somente leitura e grava o valor padrão (`None` não
    /// mexe no padrão; vazio o tira). Um Ctrl+Z (quem chama guarda o documento).
    pub fn definir_comportamento(&mut self, c: &Campo, obrigatorio: bool, so_leitura: bool, padrao: Option<&str>) -> Result<(), String> {
        if matches!(c.tipo, TipoCampo::Assinatura | TipoCampo::Botao) && padrao.is_some_and(|p| !p.is_empty()) {
            return Err("Esse campo não guarda valor.".into());
        }
        // O /Ff pode vir do pai: parte do que vale hoje, para não perder multilinha, lista etc.
        let mut ff = 0;
        let mut atual = Some(c.campo);
        for _ in 0..32 {
            let Some(id) = atual else { break };
            let Ok(d) = self.documento.get_dictionary(id) else { break };
            if let Ok(v) = d.get(b"Ff").and_then(Object::as_i64) { ff = v; break; }
            atual = d.get(b"Parent").and_then(Object::as_reference).ok();
        }
        ff &= !(FF_OBRIGATORIO | FF_SO_LEITURA);
        if obrigatorio { ff |= FF_OBRIGATORIO; }
        if so_leitura { ff |= FF_SO_LEITURA; }
        let padrao_pdf = padrao.map(|p| match c.tipo {
            TipoCampo::Caixa | TipoCampo::Opcao => Object::Name(if p.is_empty() { b"Off".to_vec() } else { c.ligado.as_bytes().to_vec() }),
            _ => lopdf::text_string(p),
        });
        let d = self.documento.get_dictionary_mut(c.campo).map_err(|e| e.to_string())?;
        if ff == 0 { d.remove(b"Ff"); } else { d.set("Ff", ff); }
        match (padrao, padrao_pdf) {
            (Some(""), _) => { d.remove(b"DV"); }
            (Some(_), Some(o)) => d.set("DV", o),
            _ => {}
        }
        self.modificado = true;
        Ok(())
    }

    /// Nomes dos campos obrigatórios ainda vazios (cada campo uma vez).
    pub fn obrigatorios_vazios(&self) -> Vec<String> {
        obrigatorios_vazios_de(&self.campos())
    }

    /// AcroForm do catálogo, criado se não houver; devolve o id dele.
    pub(crate) fn garantir_acroform(&mut self) -> Result<ObjectId, String> {
        let atual = self.documento.catalog().map_err(|e| e.to_string())?.get(b"AcroForm").ok().cloned();
        match atual {
            Some(Object::Reference(r)) => Ok(r),
            outro => {
                let mut af = outro.and_then(|o| o.as_dict().ok().cloned()).unwrap_or_else(|| dictionary! { "Fields" => Vec::<Object>::new() });
                if !af.has(b"DA") { af.set("DA", Object::string_literal("/Helv 0 Tf 0 g")); }
                if !af.has(b"DR") { af.set("DR", dictionary! { "Font" => dictionary! { "Helv" => DocumentoPdf::fonte_helvetica(false) } }); }
                let r = self.documento.add_object(af);
                self.documento.catalog_mut().map_err(|e| e.to_string())?.set("AcroForm", Object::Reference(r));
                Ok(r)
            }
        }
    }

    fn nome_livre(&self, base: &str) -> String {
        let usados: std::collections::HashSet<String> = self.campos().into_iter().map(|c| c.nome).collect();
        let base = if base.trim().is_empty() { "Campo" } else { base.trim() };
        if !usados.contains(base) { return base.to_string(); }
        (2..).map(|n| format!("{base} {n}")).find(|n| !usados.contains(n)).unwrap_or_default()
    }

    fn forma(&mut self, l: f32, a: f32, conteudo: String, recursos: Dictionary) -> ObjectId {
        self.documento.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => retangulo([0.0, 0.0, l, a]), "Resources" => recursos,
        }, conteudo.into_bytes()))
    }

    /// Cria um campo na caixa da página. Devolve o nome usado (único).
    pub fn criar_campo(&mut self, pagina: u32, caixa: [f32; 4], tipo: &CampoNovo, nome: &str) -> Result<String, String> {
        let pid = self.id_pagina(pagina)?;
        let af = self.garantir_acroform()?;
        let (l, a) = (caixa[2] - caixa[0], caixa[3] - caixa[1]);
        if l < 4.0 || a < 4.0 { return Err("Campo pequeno demais.".into()); }
        let borda = format!("{} RG 0.8 w 0.4 0.4 {:.2} {:.2} re S", cor(AZUL_CAMPO), l - 0.8, a - 0.8);
        let mut w = dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "Rect" => retangulo(caixa), "P" => Object::Reference(pid), "F" => 4,
            "MK" => dictionary! { "BC" => AZUL_CAMPO.iter().map(|&v| v.into()).collect::<Vec<Object>>() },
        };
        // Opção com nome de grupo que já existe entra no mesmo grupo.
        if *tipo == CampoNovo::Opcao {
            if let Some(grupo) = self.campos().into_iter().find(|c| c.nome == nome && c.tipo == TipoCampo::Opcao).map(|c| c.campo) {
                let n = self.campos().iter().filter(|c| c.campo == grupo).count() + 1;
                let ligado = format!("Opcao{n}");
                let (on, off) = self.aparencias_marca(l, a, true);
                w.set("Parent", Object::Reference(grupo));
                w.set("AS", "Off");
                w.set("AP", dictionary! { "N" => dictionary! { ligado.as_str() => on, "Off" => off } });
                let wid = self.documento.add_object(w);
                // /Kids pode ser indireto: resolve, para não jogar fora as opções que já existem.
                let mut kids = self.documento.get_dictionary(grupo).ok().and_then(|g| g.get(b"Kids").ok()).and_then(|k| self.resolver_array(k)).unwrap_or_default();
                kids.push(Object::Reference(wid));
                self.documento.get_dictionary_mut(grupo).map_err(|e| e.to_string())?.set("Kids", kids);
                self.por_anotacao(pid, wid)?;
                return Ok(nome.to_string());
            }
        }
        let nome = self.nome_livre(nome);
        let raiz = match tipo {
            CampoNovo::Texto | CampoNovo::Multilinha | CampoNovo::Data => {
                w.set("FT", "Tx");
                w.set("DA", Object::string_literal(if *tipo == CampoNovo::Multilinha { "/Helv 10 Tf 0 g" } else { "/Helv 0 Tf 0 g" }));
                if *tipo == CampoNovo::Multilinha { w.set("Ff", FF_MULTILINHA); }
                if *tipo == CampoNovo::Data { w.set("TU", lopdf::text_string("Data (dd/mm/aaaa)")); }
                let ap = self.forma(l, a, format!("/Tx BMC {borda} EMC"), Dictionary::new());
                w.set("AP", dictionary! { "N" => Object::Reference(ap) });
                w.set("T", lopdf::text_string(&nome));
                self.documento.add_object(w)
            }
            CampoNovo::Lista(opcoes) => {
                w.set("FT", "Ch");
                w.set("Ff", FF_COMBO);
                w.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
                w.set("Opt", opcoes.iter().map(|o| lopdf::text_string(o)).collect::<Vec<Object>>());
                let seta = format!("0.3 g {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l f", l - 12.0, a / 2.0 + 2.0, l - 4.0, a / 2.0 + 2.0, l - 8.0, a / 2.0 - 3.0);
                let ap = self.forma(l, a, format!("/Tx BMC {borda} {seta} EMC"), Dictionary::new());
                w.set("AP", dictionary! { "N" => Object::Reference(ap) });
                w.set("T", lopdf::text_string(&nome));
                self.documento.add_object(w)
            }
            CampoNovo::Caixa => {
                let (on, off) = self.aparencias_marca(l, a, false);
                w.set("FT", "Btn");
                w.set("V", "Off");
                w.set("AS", "Off");
                w.set("AP", dictionary! { "N" => dictionary! { "Sim" => on, "Off" => off } });
                w.set("T", lopdf::text_string(&nome));
                self.documento.add_object(w)
            }
            CampoNovo::Opcao => {
                let (on, off) = self.aparencias_marca(l, a, true);
                w.set("AS", "Off");
                w.set("AP", dictionary! { "N" => dictionary! { "Opcao1" => on, "Off" => off } });
                let pai = self.documento.add_object(dictionary! {
                    "FT" => "Btn", "Ff" => FF_RADIO | FF_SEM_DESLIGAR, "T" => lopdf::text_string(&nome), "V" => "Off",
                });
                w.set("Parent", Object::Reference(pai));
                let wid = self.documento.add_object(w);
                self.documento.get_dictionary_mut(pai).map_err(|e| e.to_string())?.set("Kids", vec![Object::Reference(wid)]);
                self.por_anotacao(pid, wid)?;
                self.por_campo(af, pai)?;
                return Ok(nome);
            }
            CampoNovo::Assinatura => {
                w.set("FT", "Sig");
                let ap = self.forma(l, a, format!("{} RG 0.8 w [3 2] 0 d 0.4 0.4 {:.2} {:.2} re S", cor(AZUL_CAMPO), l - 0.8, a - 0.8), Dictionary::new());
                w.set("AP", dictionary! { "N" => Object::Reference(ap) });
                w.set("T", lopdf::text_string(&nome));
                self.documento.add_object(w)
            }
        };
        self.por_anotacao(pid, raiz)?;
        self.por_campo(af, raiz)?;
        Ok(nome)
    }

    pub(crate) fn por_campo(&mut self, af: ObjectId, campo: ObjectId) -> Result<(), String> {
        let d = self.documento.get_dictionary(af).map_err(|e| e.to_string())?;
        let mut campos = d.get(b"Fields").ok().and_then(|o| self.resolver_array(o)).unwrap_or_default();
        campos.push(Object::Reference(campo));
        self.documento.get_dictionary_mut(af).map_err(|e| e.to_string())?.set("Fields", campos);
        self.modificado = true;
        Ok(())
    }

    /// Aparências ligada e desligada da caixa (✓) ou da opção (bolinha).
    fn aparencias_marca(&mut self, l: f32, a: f32, redonda: bool) -> (ObjectId, ObjectId) {
        let (cx, cy, r) = (l / 2.0, a / 2.0, l.min(a) / 2.0 - 0.8);
        let k = 0.5523 * r;
        let circulo = |r: f32, k: f32| format!("{:.2} {cy:.2} m {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c",
            cx + r, cx + r, cy + k, cx + k, cy + r, cy + r, cx - k, cy + r, cx - r, cy + k, cx - r,
            cx - r, cy - k, cx - k, cy - r, cy - r, cx + k, cy - r, cx + r, cy - k, cx + r);
        let fundo = if redonda { format!("{} RG 0.8 w {} S", cor(AZUL_CAMPO), circulo(r, k)) }
            else { format!("{} RG 0.8 w 0.4 0.4 {:.2} {:.2} re S", cor(AZUL_CAMPO), l - 0.8, a - 0.8) };
        let marca = if redonda { format!("0 g {} f", circulo(r * 0.5, k * 0.5)) }
            else { format!("0 G {:.2} w 1 J 1 j {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l S", (l.min(a) * 0.12).max(1.0), l * 0.2, a * 0.5, l * 0.42, a * 0.25, l * 0.8, a * 0.78) };
        (self.forma(l, a, format!("{fundo}\n{marca}"), Dictionary::new()), self.forma(l, a, fundo, Dictionary::new()))
    }

    /// Lugares que parecem campo numa página sem formulário: sequências de
    /// "____", traços horizontais soltos (linha para escrever) e quadradinhos.
    /// Devolve (caixa, tipo, nome sugerido pelo rótulo à esquerda).
    pub fn detectar_campos(&self, pagina: u32) -> Result<Vec<([f32; 4], CampoNovo, String)>, String> {
        let a = self.analisar_com(pagina, true)?;
        let ja: Vec<[f32; 4]> = self.campos().into_iter().filter(|c| c.pagina == pagina).map(|c| c.caixa).collect();
        let mut achados: Vec<([f32; 4], CampoNovo, String)> = Vec::new();
        let rotulo = |t: &str| t.trim().trim_end_matches([':', '_', ' ', '.']).trim().chars().rev().take(30).collect::<String>().chars().rev().collect::<String>();
        let textos: Vec<([f32; 4], String)> = a.objetos.iter().filter_map(|o| match &o.tipo { TipoObjeto::Texto { texto, .. } => Some((o.caixa, texto.clone())), _ => None }).collect();
        let a_esquerda = |c: [f32; 4]| -> String {
            textos.iter().filter(|(t, _)| t[2] <= c[0] + 2.0 && t[1] < c[3] && t[3] > c[1] - 4.0 && c[0] - t[2] < 160.0)
                .max_by(|x, y| x.0[2].total_cmp(&y.0[2])).map(|(_, s)| rotulo(s)).unwrap_or_default()
        };
        // 1. "____" dentro do texto: o campo fica em cima dos traços.
        for lugar in a.lugares.iter().filter(|l| !l.escritas.is_empty()) {
            let mut antes = String::new();
            let mut corrida: Vec<[f32; 4]> = Vec::new();
            let itens: Vec<&ItemTj> = lugar.escritas.iter().filter_map(|i| a.letras.get(i)).flatten().collect();
            for (k, item) in itens.iter().enumerate() {
                let ItemTj::Glifo { texto, caixa, .. } = item else { continue };
                if texto == "_" { corrida.push(*caixa); }
                let fim = texto != "_" || k + 1 == itens.len();
                if fim && corrida.len() >= 4 {
                    let c = corrida.iter().fold(corrida[0], |k, c| [k[0].min(c[0]), k[1].min(c[1]), k[2].max(c[2]), k[3].max(c[3])]);
                    achados.push(([c[0], c[1] + (c[3] - c[1]) * 0.15, c[2], c[1] + (c[3] - c[1]) * 1.1], CampoNovo::Texto, rotulo(&antes)));
                }
                if texto != "_" { if fim { corrida.clear(); } antes.push_str(texto); }
            }
        }
        // 2. Traço horizontal comprido sem texto logo em cima: linha para escrever.
        let antes_das_linhas = achados.len();
        for s in &a.segmentos {
            let comp = s[2] - s[0];
            if (s[3] - s[1]).abs() > 1.0 || !(40.0..=400.0).contains(&comp) { continue; }
            // Borda de tabela (um traço vertical encosta numa ponta) não é linha para escrever.
            let encosta = |x: f32| a.segmentos.iter().any(|v| (v[2] - v[0]).abs() <= 1.0 && (v[3] - v[1]).abs() > 4.0
                && (v[0] - x).abs() <= 2.0 && v[1].min(v[3]) - 2.0 <= s[1] && s[1] <= v[1].max(v[3]) + 2.0);
            if encosta(s[0]) || encosta(s[2]) { continue; }
            let c = [s[0], s[1] + 1.0, s[2], s[1] + 15.0];
            let ocupado = textos.iter().any(|(t, _)| t[0] < c[2] && c[0] < t[2] && t[1] < c[3] && c[1] < t[3]);
            if !ocupado { achados.push((c, CampoNovo::Texto, a_esquerda(c))); }
        }
        // Dezenas de linhas numa página é grade de tabela ou formulário já desenhado, não linhas soltas.
        if achados.len() - antes_das_linhas > 30 { achados.truncate(antes_das_linhas); }
        // 3. Quadradinho de 6 a 16 pt: caixa de marcar.
        for o in a.objetos.iter().filter(|o| o.tipo == TipoObjeto::Desenho) {
            let (l, h) = (o.caixa[2] - o.caixa[0], o.caixa[3] - o.caixa[1]);
            // Só quadrado de bordas retas: texto desenhado como curva também tem letra de 6–8 pt.
            let perto = |x: f32, y: f32| (x - y).abs() <= 1.5;
            let bordas = a.segmentos.iter().filter(|s| {
                let horizontal = perto(s[1], s[3]) && perto(s[0].min(s[2]), o.caixa[0]) && perto(s[0].max(s[2]), o.caixa[2])
                    && (perto(s[1], o.caixa[1]) || perto(s[1], o.caixa[3]));
                let vertical = perto(s[0], s[2]) && perto(s[1].min(s[3]), o.caixa[1]) && perto(s[1].max(s[3]), o.caixa[3])
                    && (perto(s[0], o.caixa[0]) || perto(s[0], o.caixa[2]));
                horizontal || vertical
            }).count();
            if (6.0..=16.0).contains(&l) && (l - h).abs() < 2.0 && bordas >= 3 {
                let direita = textos.iter().filter(|(t, _)| t[0] >= o.caixa[2] - 1.0 && t[0] - o.caixa[2] < 40.0 && t[1] < o.caixa[3] && t[3] > o.caixa[1])
                    .min_by(|x, y| x.0[0].total_cmp(&y.0[0])).map(|(_, s)| rotulo(s)).unwrap_or_default();
                achados.push((o.caixa, CampoNovo::Caixa, direita));
            }
        }
        // Sem repetir o que já é campo nem o mesmo lugar duas vezes.
        let cruza = |p: [f32; 4], q: [f32; 4]| p[0] < q[2] && q[0] < p[2] && p[1] < q[3] && q[1] < p[3];
        let mut saida: Vec<([f32; 4], CampoNovo, String)> = Vec::new();
        for x in achados {
            if ja.iter().any(|j| cruza(*j, x.0)) || saida.iter().any(|y| cruza(y.0, x.0)) { continue; }
            saida.push(x);
        }
        Ok(saida)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::objetos::testes::pdf_com;

    #[test]
    fn lista_grava_o_valor_de_exportacao() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.criar_campo(1, [100.0, 700.0, 250.0, 720.0], &CampoNovo::Lista(vec!["Brasil".into(), "Portugal".into()]), "Pais").unwrap();
        let c = pdf.campos().remove(0);
        let par = |e: &str, m: &str| Object::Array(vec![lopdf::text_string(e), lopdf::text_string(m)]);
        pdf.documento.get_dictionary_mut(c.campo).unwrap().set("Opt", vec![par("BR", "Brasil"), par("PT", "Portugal")]);
        pdf.preencher(&c, "Brasil").unwrap();
        let v = pdf.documento.get_dictionary(c.campo).unwrap().get(b"V").unwrap().clone();
        assert_eq!(lopdf::decode_text_string(&v).unwrap(), "BR");
        assert_eq!(pdf.campos()[0].valor, "Brasil");
    }

    #[test]
    fn obrigatorio_somente_leitura_e_padrao() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.criar_campo(1, [100.0, 700.0, 250.0, 720.0], &CampoNovo::Multilinha, "Obs").unwrap();
        pdf.criar_campo(1, [100.0, 600.0, 112.0, 612.0], &CampoNovo::Caixa, "Aceito").unwrap();
        let obs = pdf.campos().into_iter().find(|c| c.nome == "Obs").unwrap();
        pdf.definir_comportamento(&obs, true, false, Some("Nada a declarar")).unwrap();
        let aceito = pdf.campos().into_iter().find(|c| c.nome == "Aceito").unwrap();
        pdf.definir_comportamento(&aceito, true, false, None).unwrap();
        let mut faltam = pdf.obrigatorios_vazios();
        faltam.sort();
        assert_eq!(faltam, ["Aceito", "Obs"]);
        pdf.limpar_campos().unwrap();
        let pdf2 = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let obs = pdf2.campos().into_iter().find(|c| c.nome == "Obs").unwrap();
        assert!(obs.obrigatorio && !obs.so_leitura && matches!(obs.tipo, TipoCampo::Texto { multilinha: true }), "o multilinha continua");
        assert_eq!((obs.valor.as_str(), obs.padrao.as_str()), ("Nada a declarar", "Nada a declarar"));
        assert_eq!(pdf2.obrigatorios_vazios(), ["Aceito"]);
        let obs = pdf.campos().into_iter().find(|c| c.nome == "Obs").unwrap();
        pdf.definir_comportamento(&obs, false, true, Some("")).unwrap();
        let obs = pdf.campos().into_iter().find(|c| c.nome == "Obs").unwrap();
        assert!(obs.so_leitura && !obs.obrigatorio && obs.padrao.is_empty());
        assert!(pdf.preencher(&obs, "x").is_err(), "somente leitura não aceita digitação");
        // Importar dados pula o somente leitura em vez de parar no meio.
        assert_eq!(pdf.importar_dados(&[("Obs".into(), "x".into()), ("Aceito".into(), "sim".into())]).unwrap().0, 1);
    }

    #[test]
    fn kids_em_ciclo_nao_trava() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.criar_campo(1, [100.0, 700.0, 250.0, 720.0], &CampoNovo::Texto, "A").unwrap();
        let a = pdf.campos()[0].campo;
        // Sem a marca de visitados, 32 níveis de [A A] dariam 2^33 chamadas.
        pdf.documento.get_dictionary_mut(a).unwrap().set("Kids", vec![Object::Reference(a), Object::Reference(a)]);
        assert!(pdf.campos().is_empty());
        pdf.tirar_campos(&HashSet::new());
    }

    #[test]
    fn criar_preencher_e_ler_campos() {
        let mut pdf = pdf_com("BT /F1 12 Tf 72 700 Td (Nome: ____________) Tj ET 72 600 m 300 600 l S 72 500 10 10 re S");
        let achados = pdf.detectar_campos(1).unwrap();
        assert_eq!(achados.len(), 3, "{achados:?}");
        assert_eq!((achados[0].1.clone(), achados[0].2.as_str()), (CampoNovo::Texto, "Nome"));
        assert_eq!(achados[2].1, CampoNovo::Caixa);
        for (c, t, n) in &achados { pdf.criar_campo(1, *c, t, n).unwrap(); }
        pdf.criar_campo(1, [72.0, 400.0, 84.0, 412.0], &CampoNovo::Opcao, "Turno").unwrap();
        pdf.criar_campo(1, [100.0, 400.0, 112.0, 412.0], &CampoNovo::Opcao, "Turno").unwrap();
        let campos = pdf.campos();
        assert_eq!(campos.len(), 5);
        let nome = campos.iter().find(|c| c.nome == "Nome").unwrap().clone();
        pdf.preencher(&nome, "Ana Lúcia").unwrap();
        let caixa = pdf.campos().into_iter().find(|c| c.tipo == TipoCampo::Caixa).unwrap();
        pdf.preencher(&caixa, "sim").unwrap();
        let turno2 = pdf.campos().into_iter().filter(|c| c.nome == "Turno").nth(1).unwrap();
        pdf.preencher(&turno2, "sim").unwrap();
        let pdf = DocumentoPdf::de_bytes(&pdf.serializar().unwrap(), None).unwrap();
        let campos = pdf.campos();
        assert_eq!(campos.iter().find(|c| c.nome == "Nome").unwrap().valor, "Ana Lúcia");
        assert!(campos.iter().find(|c| c.tipo == TipoCampo::Caixa).unwrap().marcada);
        let turnos: Vec<bool> = campos.iter().filter(|c| c.nome == "Turno").map(|c| c.marcada).collect();
        assert_eq!(turnos, [false, true]);
        let mut pdf = pdf;
        pdf.criar_campo(1, [72.0, 300.0, 152.0, 318.0], &CampoNovo::Data, "Nascimento").unwrap();
        let data = pdf.campos().into_iter().find(|c| c.nome == "Nascimento").unwrap();
        // Data: 18 pt de altura com /DA automático escreve com 18 × 0,65 = 11,7 pt.
        assert!(data.data && (data.tamanho - 11.7).abs() < 0.01, "{data:?}");
        assert!(!pdf.campos().iter().find(|c| c.nome == "Nome").unwrap().data);
        pdf.limpar_campos().unwrap();
        assert!(pdf.campos().iter().all(|c| c.valor.is_empty() || c.valor == "Off"));
    }

    /// Audit: limpar volta o grupo de opções ao botão do padrão, e um padrão fora da fonte
    /// não impede de limpar o resto.
    #[test]
    fn limpar_campos_respeita_o_padrao_do_grupo_e_nao_para_no_padrao_sem_fonte() {
        let mut pdf = crate::testes::pdf_teste();
        pdf.criar_campo(1, [72.0, 500.0, 300.0, 520.0], &CampoNovo::Texto, "Cidade").unwrap();
        pdf.criar_campo(1, [72.0, 400.0, 84.0, 412.0], &CampoNovo::Opcao, "Turno").unwrap();
        pdf.criar_campo(1, [100.0, 400.0, 112.0, 412.0], &CampoNovo::Opcao, "Turno").unwrap();
        let turnos = |p: &DocumentoPdf| p.campos().into_iter().filter(|c| c.nome == "Turno").collect::<Vec<_>>();
        let segundo = turnos(&pdf)[1].clone();
        pdf.definir_comportamento(&segundo, false, false, Some("x")).unwrap();
        pdf.preencher(&turnos(&pdf)[0], "sim").unwrap();
        let cidade = pdf.campos().into_iter().find(|c| c.nome == "Cidade").unwrap();
        pdf.documento.get_dictionary_mut(cidade.campo).unwrap().set("DV", lopdf::text_string("東京"));
        pdf.preencher(&cidade, "Recife").unwrap();
        pdf.limpar_campos().unwrap();
        assert_eq!(turnos(&pdf).iter().map(|c| c.marcada).collect::<Vec<_>>(), [false, true], "volta ao segundo");
        assert_eq!(pdf.campos().into_iter().find(|c| c.nome == "Cidade").unwrap().valor, "", "padrão sem fonte: vazio");
    }
}
