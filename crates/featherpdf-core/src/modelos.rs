//! Modelos de formulário prontos (aba Formulários › Modelos): rótulos, linhas e
//! campos preenchíveis numa página A4 em branco.

use crate::{CampoNovo, DocumentoPdf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modelo { Cadastro, Recibo, Declaracao, Presenca, Pesquisa }

impl Modelo {
    pub const TODOS: [Modelo; 5] = [Modelo::Cadastro, Modelo::Recibo, Modelo::Declaracao, Modelo::Presenca, Modelo::Pesquisa];

    pub fn nome(self) -> &'static str {
        match self {
            Modelo::Cadastro => "Ficha de cadastro",
            Modelo::Recibo => "Recibo",
            Modelo::Declaracao => "Declaração",
            Modelo::Presenca => "Lista de presença",
            Modelo::Pesquisa => "Pesquisa de satisfação",
        }
    }
}

impl std::fmt::Display for Modelo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.nome()) }
}

/// Uma faixa do modelo, de cima para baixo.
enum Item {
    /// Rótulos e campos lado a lado: (rótulo, tipo, fração da largura) e a altura do campo.
    Linha(Vec<(&'static str, CampoNovo, f32)>, f32),
    /// Pergunta com opções que se excluem (grupo, opções).
    Opcoes(&'static str, &'static [&'static str]),
    /// Caixa de marcar com o texto ao lado (texto, nome do campo).
    Marcar(&'static str, &'static str),
    /// Tabela de linhas numeradas: (cabeçalho, fração) e o número de linhas.
    Tabela(&'static [(&'static str, f32)], usize),
}

const UFS: [&str; 27] = ["AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA", "PB",
    "PE", "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO"];
const MARGEM: f32 = 50.0;
const LARGURA: f32 = 495.0;
const ALTURA_A4: f32 = 842.0;
const ROTULO: [f32; 3] = [0.35, 0.37, 0.4];
const TINTA: [f32; 3] = [0.12, 0.13, 0.15];

fn itens(m: Modelo) -> (&'static str, Vec<Item>) {
    use CampoNovo::*;
    use Item::*;
    match m {
        Modelo::Cadastro => ("Preencha todos os campos. Os dados são usados só para este cadastro.", vec![
            Linha(vec![("Nome completo", Texto, 1.0)], 20.0),
            Linha(vec![("CPF", Texto, 0.34), ("Data de nascimento", Data, 0.33), ("Telefone", Texto, 0.33)], 20.0),
            Linha(vec![("E-mail", Texto, 1.0)], 20.0),
            Linha(vec![("Endereço", Texto, 0.75), ("Número", Texto, 0.25)], 20.0),
            Linha(vec![("Cidade", Texto, 0.5), ("UF", Lista(UFS.iter().map(|u| u.to_string()).collect()), 0.2), ("CEP", Texto, 0.3)], 20.0),
            Linha(vec![("Observações", Multilinha, 1.0)], 70.0),
            Marcar("Li e aceito o uso dos meus dados para este cadastro.", "Aceite"),
            Linha(vec![("Local e data", Texto, 0.45), ("Assinatura", Assinatura, 0.55)], 40.0),
        ]),
        Modelo::Recibo => ("", vec![
            Linha(vec![("Número", Texto, 0.3), ("Valor (R$)", Texto, 0.7)], 20.0),
            Linha(vec![("Recebi de", Texto, 1.0)], 20.0),
            Linha(vec![("CPF/CNPJ de quem pagou", Texto, 1.0)], 20.0),
            Linha(vec![("A importância de (por extenso)", Texto, 1.0)], 20.0),
            Linha(vec![("Referente a", Multilinha, 1.0)], 70.0),
            Linha(vec![("Cidade", Texto, 0.6), ("Data", Data, 0.4)], 20.0),
            Linha(vec![("Nome de quem recebeu", Texto, 0.5), ("Assinatura", Assinatura, 0.5)], 40.0),
        ]),
        Modelo::Declaracao => ("", vec![
            Linha(vec![("Nome", Texto, 0.65), ("CPF", Texto, 0.35)], 20.0),
            Linha(vec![("Declaro, para os devidos fins, que", Multilinha, 1.0)], 220.0),
            Linha(vec![("Cidade", Texto, 0.6), ("Data", Data, 0.4)], 20.0),
            Linha(vec![("Assinatura", Assinatura, 1.0)], 45.0),
        ]),
        Modelo::Presenca => ("", vec![
            Linha(vec![("Evento", Texto, 0.7), ("Data", Data, 0.3)], 20.0),
            Tabela(&[("Nome", 0.5), ("Documento", 0.2), ("Assinatura", 0.3)], 20),
        ]),
        Modelo::Pesquisa => ("Sua opinião ajuda a melhorar. Leva menos de um minuto.", vec![
            Linha(vec![("Nome (opcional)", Texto, 1.0)], 20.0),
            Opcoes("Como você avalia o atendimento?", &["Ótimo", "Bom", "Regular", "Ruim"]),
            Opcoes("Recomendaria a um amigo?", &["Sim", "Não"]),
            Linha(vec![("Comentários", Multilinha, 1.0)], 110.0),
        ]),
    }
}

impl DocumentoPdf {
    /// `inserir_texto` com `y` medido do topo da página A4.
    #[allow(clippy::too_many_arguments)]
    fn texto_no_topo(&mut self, pagina: u32, x: f32, y: f32, texto: &str, tamanho: f32, negrito: bool, cor: [f32; 3]) -> Result<(), String> {
        self.inserir_texto(pagina, x, ALTURA_A4 - y, texto, tamanho, negrito, cor)
    }

    /// Desenha o modelo na página `pagina` (pensado para uma A4 em branco). Devolve quantos campos criou.
    pub fn aplicar_modelo(&mut self, pagina: u32, m: Modelo) -> Result<usize, String> {
        let (subtitulo, itens) = itens(m);
        // `y` é a distância do topo; texto e campo vão para coordenadas do PDF (y para cima).
        let caixa = |x: f32, topo: f32, l: f32, a: f32| [x, ALTURA_A4 - topo - a, x + l, ALTURA_A4 - topo];
        let mut linhas = String::new();
        let mut n = 0;
        self.texto_no_topo(pagina, MARGEM, 48.0, m.nome(), 20.0, true, TINTA)?;
        let mut y = 76.0;
        if !subtitulo.is_empty() {
            self.texto_no_topo(pagina, MARGEM, y, subtitulo, 9.5, false, ROTULO)?;
            y += 16.0;
        }
        linhas += &format!("0.16 0.47 0.27 RG 1.5 w {MARGEM} {:.1} m {} {:.1} l S\n", ALTURA_A4 - y, MARGEM + LARGURA, ALTURA_A4 - y);
        y += 18.0;
        for item in itens {
            match item {
                Item::Linha(campos, altura) => {
                    let mut x = MARGEM;
                    let vao = 10.0;
                    let util = LARGURA - vao * (campos.len() as f32 - 1.0);
                    for (rotulo, tipo, fracao) in campos {
                        let l = util * fracao;
                        self.texto_no_topo(pagina, x, y, rotulo, 8.5, false, ROTULO)?;
                        self.criar_campo(pagina, caixa(x, y + 12.0, l, altura), &tipo, rotulo)?;
                        n += 1;
                        x += l + vao;
                    }
                    y += altura + 24.0;
                }
                Item::Opcoes(pergunta, opcoes) => {
                    self.texto_no_topo(pagina, MARGEM, y, pergunta, 9.5, false, TINTA)?;
                    let mut x = MARGEM;
                    for o in opcoes {
                        self.criar_campo(pagina, caixa(x, y + 16.0, 11.0, 11.0), &CampoNovo::Opcao, pergunta)?;
                        self.texto_no_topo(pagina, x + 16.0, y + 16.5, o, 9.5, false, TINTA)?;
                        n += 1;
                        x += 90.0;
                    }
                    y += 44.0;
                }
                Item::Marcar(texto, nome) => {
                    self.criar_campo(pagina, caixa(MARGEM, y, 11.0, 11.0), &CampoNovo::Caixa, nome)?;
                    self.texto_no_topo(pagina, MARGEM + 16.0, y + 0.5, texto, 9.5, false, TINTA)?;
                    n += 1;
                    y += 28.0;
                }
                Item::Tabela(colunas, total) => {
                    let numero = 22.0;
                    let mut x = MARGEM + numero;
                    for (titulo, fracao) in colunas {
                        self.texto_no_topo(pagina, x, y, titulo, 8.5, true, ROTULO)?;
                        x += (LARGURA - numero) * fracao;
                    }
                    y += 14.0;
                    for i in 1..=total {
                        self.texto_no_topo(pagina, MARGEM, y + 5.0, &format!("{i}."), 8.5, false, ROTULO)?;
                        let mut x = MARGEM + numero;
                        for (titulo, fracao) in colunas {
                            let l = (LARGURA - numero) * fracao;
                            self.criar_campo(pagina, caixa(x, y, l - 6.0, 20.0), &CampoNovo::Texto, &format!("{titulo} {i}"))?;
                            n += 1;
                            x += l;
                        }
                        linhas += &format!("0.85 g {MARGEM} {:.1} {LARGURA} 0.5 re f\n", ALTURA_A4 - y - 23.0);
                        y += 26.0;
                    }
                }
            }
        }
        self.texto_no_topo(pagina, MARGEM, ALTURA_A4 - 40.0, "Formulário criado no FeatherPDF", 7.0, false, ROTULO)?;
        self.por_grupo(pagina, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 595.0, ALTURA_A4, linhas)?;
        Ok(n)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todos_os_modelos_cabem_e_criam_campos() {
        for m in Modelo::TODOS {
            let mut pdf = crate::objetos::testes::pdf_com("");
            let n = pdf.aplicar_modelo(1, m).unwrap();
            let campos = pdf.campos();
            assert_eq!(campos.len(), n, "{m}");
            // Nada passa da margem de baixo.
            assert!(campos.iter().all(|c| c.caixa[1] >= 45.0), "{m}: {:?}", campos.iter().map(|c| c.caixa[1]).fold(f32::MAX, f32::min));
        }
    }
}
