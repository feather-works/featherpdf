//! Ordem dos objetos: trazer para a frente (fim do conteúdo) ou mandar para trás (começo).
//! O objeto sai do lugar e é reescrito no outro extremo com o estado gráfico que tinha
//! (matriz, cores, espessura, fonte), refeito a partir das operações anteriores a ele.

use lopdf::content::Operation;
use lopdf::Object;

use super::DocumentoPdf;
use crate::objetos::{bloco_do_recorte, ctm_em, Matriz};

/// O estado que muda o desenho, como a última operação de cada tipo.
#[derive(Clone, Default)]
pub(crate) struct Estado { preenchimento: Vec<Operation>, contorno: Vec<Operation>, avulsos: Vec<Operation> }

impl Estado {
    pub(crate) fn aplicar(&mut self, op: &Operation) {
        let o = op.operator.as_str();
        match o {
            "g" | "rg" | "k" | "cs" => self.preenchimento = vec![op.clone()],
            "sc" | "scn" => { self.preenchimento.retain(|x| x.operator == "cs"); self.preenchimento.push(op.clone()); }
            "G" | "RG" | "K" | "CS" => self.contorno = vec![op.clone()],
            "SC" | "SCN" => { self.contorno.retain(|x| x.operator == "CS"); self.contorno.push(op.clone()); }
            // ExtGState se soma (cada um pode mudar uma coisa diferente); o resto vale o último.
            "gs" => self.avulsos.push(op.clone()),
            "w" | "J" | "j" | "M" | "d" | "ri" | "i" | "Tf" | "Tc" | "Tw" | "Tz" | "TL" | "Ts" | "Tr" => {
                self.avulsos.retain(|x| x.operator != o);
                self.avulsos.push(op.clone());
            }
            _ => {}
        }
    }

    pub(crate) fn operacoes(&self) -> Vec<Operation> {
        self.avulsos.iter().chain(&self.preenchimento).chain(&self.contorno).cloned().collect()
    }
}

/// O estado em vigor logo antes da operação `ate`.
pub(crate) fn estado_em(ops: &[Operation], ate: usize) -> Estado {
    let mut pilha = vec![Estado::default()];
    for op in &ops[..ate] {
        match op.operator.as_str() {
            "q" => { let e = pilha.last().cloned().unwrap_or_default(); pilha.push(e); }
            "Q" => if pilha.len() > 1 { pilha.pop(); },
            _ => if let Some(e) = pilha.last_mut() { e.aplicar(op) },
        }
    }
    pilha.pop().unwrap_or_default()
}

fn cm(m: Matriz) -> Operation { Operation::new("cm", m.iter().map(|&v| Object::Real(v)).collect()) }

impl DocumentoPdf {
    /// `frente`: os objetos passam a ser desenhados por último (por cima de tudo); senão, primeiro.
    pub fn reordenar_objetos(&mut self, pagina: u32, objetos: &[usize], frente: bool) -> Result<(), String> {
        let mut analise = self.analisar_com(pagina, false)?;
        if objetos.is_empty() { return Err("Nenhum objeto escolhido.".into()); }
        if let Some(i) = objetos.iter().find(|&&i| i >= analise.objetos.len()) { return Err(format!("O objeto {i} não existe mais nesta página.")); }
        let mut blocos: Vec<Vec<Operation>> = Vec::new();
        let mut remover: Vec<usize> = Vec::new();
        let mut ordem: Vec<usize> = objetos.to_vec();
        ordem.sort_unstable();
        ordem.dedup();
        for &k in &ordem {
            let l = &analise.lugares[k];
            let mut bloco = vec![Operation::new("q", vec![])];
            if l.escritas.is_empty() {
                // Imagem dentro de recorte vai com o recorte; desenho e imagem levam a matriz de onde estavam.
                let (inicio, fim, ctm) = match bloco_do_recorte(&analise.ops, l.insercao, l.fim) {
                    Some((i, f)) => (i, f, ctm_em(&analise.ops, i)),
                    None => (l.insercao, l.fim, l.matriz),
                };
                bloco.push(cm(ctm));
                bloco.extend(estado_em(&analise.ops, inicio).operacoes());
                bloco.extend(analise.ops[inicio..fim].iter().cloned());
                remover.extend(inicio..fim);
            } else {
                // Texto: a matriz da linha já inclui a CTM, então vai como Tm com a CTM limpa.
                bloco.extend(estado_em(&analise.ops, l.insercao).operacoes());
                bloco.push(Operation::new("BT", vec![]));
                bloco.push(Operation::new("Tm", l.matriz.iter().map(|&v| Object::Real(v)).collect()));
                bloco.extend(l.escritas.iter().map(|&i| analise.ops[i].clone()));
                bloco.push(Operation::new("ET", vec![]));
                remover.extend(l.escritas.iter().copied());
            }
            bloco.push(Operation::new("Q", vec![]));
            blocos.push(bloco);
        }
        remover.sort_unstable_by(|a, b| b.cmp(a));
        remover.dedup();
        for i in remover { analise.ops.remove(i); }
        let novos: Vec<Operation> = blocos.into_iter().flatten().collect();
        if frente {
            // O conteúdo antigo fica embrulhado em q…Q, para um estado aberto não vazar no bloco.
            analise.ops.insert(0, Operation::new("q", vec![]));
            analise.ops.push(Operation::new("Q", vec![]));
            analise.ops.extend(novos);
        } else {
            analise.ops.splice(0..0, novos);
        }
        self.gravar_conteudo(pagina, &analise.ops)
    }
}

#[cfg(test)]
mod testes {
    use crate::objetos::testes::pdf_com;
    use crate::TipoObjeto;

    #[test]
    fn retangulo_vermelho_vai_para_a_frente_com_a_cor() {
        let mut pdf = pdf_com("1 0 0 rg 10 10 100 100 re f 0 0 1 rg 50 50 100 100 re f");
        let antes = pdf.objetos(1).unwrap();
        assert_eq!(antes.len(), 2);
        pdf.reordenar_objetos(1, &[0], true).unwrap();
        let depois = pdf.objetos(1).unwrap();
        assert_eq!(depois.len(), 2);
        // O primeiro (vermelho) agora é o último desenhado, no mesmo lugar.
        assert_eq!(depois[1].caixa, antes[0].caixa);
        assert!(matches!(depois[1].tipo, TipoObjeto::Desenho));
        let conteudo = String::from_utf8_lossy(&pdf.documento.get_page_content(*pdf.documento.get_pages().get(&1).unwrap())).into_owned();
        let azul = conteudo.find("0 0 1 rg").unwrap();
        let vermelho = conteudo.rfind("1 0 0 rg").unwrap();
        assert!(vermelho > azul, "{conteudo}");
        pdf.reordenar_objetos(1, &[1], false).unwrap();
        assert_eq!(pdf.objetos(1).unwrap()[0].caixa, antes[0].caixa);
    }
}
