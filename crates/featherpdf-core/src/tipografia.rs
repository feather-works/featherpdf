//! Tipografia do texto que já está na página: tamanho, cor, espaço entre letras (tracking) e
//! entre palavras. Muda só as linhas escolhidas: o estado de antes volta logo depois delas.

use lopdf::content::Operation;
use lopdf::Object;

use crate::ordem::estado_em;
use crate::{DocumentoPdf, TipoObjeto};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EstiloTexto {
    /// Multiplica o tamanho da letra (1,2 = 20% maior).
    pub escala: Option<f32>,
    pub cor: Option<[f32; 3]>,
    /// Espaço a mais entre letras, em pontos da página.
    pub letras: Option<f32>,
    /// Espaço a mais entre palavras, em pontos da página.
    pub palavras: Option<f32>,
    /// Largura das letras em % (100 = normal): aperta a linha sem mudar a altura.
    pub horizontal: Option<f32>,
}

impl DocumentoPdf {
    /// Aplica o estilo nos textos escolhidos (os outros objetos ficam). Devolve quantos mudaram.
    pub fn estilizar_texto(&mut self, pagina: u32, objetos: &[usize], e: &EstiloTexto) -> Result<usize, String> {
        let mut analise = self.analisar(pagina)?;
        Self::validar_indices(&analise, objetos)?;
        let mut alvos: Vec<usize> = objetos.iter().copied()
            .filter(|&k| matches!(analise.objetos[k].tipo, TipoObjeto::Texto { .. }) && !analise.lugares[k].escritas.is_empty()).collect();
        if alvos.is_empty() { return Err("Tamanho, cor e espaçamento valem para texto.".into()); }
        // De trás para a frente: inserir num objeto não muda a posição dos anteriores.
        alvos.sort_unstable_by_key(|&k| std::cmp::Reverse(analise.lugares[k].escritas[0]));
        let real = |v: f32| Object::Real(v);
        for &k in &alvos {
            let lugar = analise.lugares[k].clone();
            let (inicio, fim) = (lugar.escritas[0], *lugar.escritas.last().unwrap_or(&lugar.escritas[0]) + 1);
            let antes = estado_em(&analise.ops, inicio).operacoes();
            let ultimo = |o: &str| antes.iter().rev().find(|x| x.operator == o).cloned();
            // Espaço em pontos da página levado ao espaço do texto (a matriz da linha escala tudo).
            let m = lugar.matriz;
            let escala_m = (m[0] * m[3] - m[1] * m[2]).abs().sqrt().max(1e-6);
            let (mut novo, mut volta) = (Vec::new(), Vec::new());
            if let Some(c) = e.cor {
                novo.push(Operation::new("rg", c.map(real).to_vec()));
                let cor: Vec<Operation> = antes.iter().filter(|x| matches!(x.operator.as_str(), "g" | "rg" | "k" | "cs" | "sc" | "scn")).cloned().collect();
                volta.extend(if cor.is_empty() { vec![Operation::new("g", vec![real(0.0)])] } else { cor });
            }
            if let Some(s) = e.escala {
                let tf = ultimo("Tf").ok_or("A linha não diz a fonte (Tf); não dá para mudar o tamanho.")?;
                let tamanho = tf.operands.get(1).and_then(|o| o.as_float().ok()).unwrap_or(12.0);
                novo.push(Operation::new("Tf", vec![tf.operands[0].clone(), real(tamanho * s.clamp(0.1, 20.0))]));
                volta.push(tf);
            }
            for (valor, op) in [(e.letras, "Tc"), (e.palavras, "Tw")] {
                let Some(v) = valor else { continue };
                // Soma ao que já havia: clicar de novo abre mais.
                let atual = ultimo(op).and_then(|o| o.operands.first().and_then(|x| x.as_float().ok())).unwrap_or(0.0);
                novo.push(Operation::new(op, vec![real(atual + v / escala_m)]));
                volta.push(ultimo(op).unwrap_or_else(|| Operation::new(op, vec![real(0.0)])));
            }
            if let Some(h) = e.horizontal {
                novo.push(Operation::new("Tz", vec![real(h)]));
                volta.push(ultimo("Tz").unwrap_or_else(|| Operation::new("Tz", vec![real(100.0)])));
            }
            analise.ops.splice(fim..fim, volta);
            analise.ops.splice(inicio..inicio, novo);
        }
        self.gravar_conteudo(pagina, &analise.ops)?;
        Ok(alvos.len())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::objetos::testes::pdf_com;

    #[test]
    fn so_a_linha_escolhida_muda_e_o_resto_volta() {
        let mut pdf = pdf_com("0.2 g BT /F1 12 Tf 100 700 Td (Primeira) Tj 0 -20 Td (Segunda) Tj ET");
        let antes = pdf.objetos(1).unwrap();
        assert_eq!(antes.len(), 2);
        let estilo = EstiloTexto { escala: Some(2.0), cor: Some([1.0, 0.0, 0.0]), letras: Some(1.0), palavras: None, horizontal: None };
        assert_eq!(pdf.estilizar_texto(1, &[0], &estilo).unwrap(), 1);
        let depois = pdf.objetos(1).unwrap();
        let tamanho = |o: &crate::ObjetoPdf| match &o.tipo { TipoObjeto::Texto { tamanho, .. } => *tamanho, _ => 0.0 };
        assert!((tamanho(&depois[0]) - 2.0 * tamanho(&antes[0])).abs() < 0.5, "{depois:?}");
        assert!((tamanho(&depois[1]) - tamanho(&antes[1])).abs() < 0.01, "a segunda linha fica igual");
        let conteudo = String::from_utf8_lossy(&pdf.documento.get_page_content(*pdf.documento.get_pages().get(&1).unwrap())).into_owned();
        // Vermelho antes da primeira, e o cinza, o 12 e o Tc 0 voltam antes da segunda.
        let (vermelho, segunda) = (conteudo.find("1 0 0 rg").unwrap(), conteudo.find("(Segunda)").unwrap());
        let volta = &conteudo[conteudo.find("(Primeira)").unwrap()..segunda];
        assert!(vermelho < conteudo.find("(Primeira)").unwrap() && volta.contains("0.2 g") && volta.contains("/F1 12 Tf") && volta.contains("0 Tc"), "{conteudo}");
        assert!(pdf.estilizar_texto(1, &[5], &estilo).is_err());
    }
}
