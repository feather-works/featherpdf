//! Medir na página: distância, perímetro, área e ângulo, com escala (proporção
//! 1:N ou calibrada por um traço de tamanho conhecido). Pontos no espaço da
//! página, em pontos tipográficos (1/72 pol).

/// Unidade de medida mostrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unidade { Mm, Cm, M, Km, Pol, Pe }

impl Unidade {
    pub const TODAS: [Unidade; 6] = [Unidade::Mm, Unidade::Cm, Unidade::M, Unidade::Km, Unidade::Pol, Unidade::Pe];

    pub fn sigla(self) -> &'static str {
        match self { Unidade::Mm => "mm", Unidade::Cm => "cm", Unidade::M => "m", Unidade::Km => "km", Unidade::Pol => "pol", Unidade::Pe => "pé" }
    }

    /// Quanto 1 pt do papel vale nesta unidade.
    fn por_ponto(self) -> f64 {
        let mm = 25.4 / 72.0;
        match self {
            Unidade::Mm => mm, Unidade::Cm => mm / 10.0, Unidade::M => mm / 1000.0, Unidade::Km => mm / 1.0e6,
            Unidade::Pol => 1.0 / 72.0, Unidade::Pe => 1.0 / 864.0,
        }
    }
}

impl std::fmt::Display for Unidade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.sigla()) }
}

/// Quanto vale 1 pt da página no mundo real.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Escala {
    pub unidade: Unidade,
    /// Unidades reais por ponto do papel.
    pub fator: f64,
    /// `Some(n)` para proporção 1:n; `None` quando calibrada por um traço.
    pub proporcao: Option<f64>,
}

impl Default for Escala {
    fn default() -> Self { Self::proporcao(Unidade::Cm, 1.0) }
}

impl Escala {
    /// 1:n — 1 cm no papel vale n cm no mundo, mostrado em `unidade`.
    pub fn proporcao(unidade: Unidade, n: f64) -> Self {
        Self { unidade, fator: unidade.por_ponto() * n, proporcao: Some(n) }
    }

    /// Um traço de `pontos` pt na página mede `valor` `unidade`.
    pub fn calibrada(pontos: f32, valor: f64, unidade: Unidade) -> Option<Self> {
        (pontos > 0.0 && valor > 0.0 && valor.is_finite()).then(|| Self { unidade, fator: valor / pontos as f64, proporcao: None })
    }

    pub fn rotulo(&self) -> String {
        match self.proporcao {
            Some(n) => format!("1:{} em {}", numero(n, 0), self.unidade),
            None => format!("calibrada em {}", self.unidade),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoMedida { Distancia, Perimetro, Area, Angulo, Contagem }

impl TipoMedida {
    pub fn nome(self) -> &'static str {
        match self { TipoMedida::Distancia => "Distância", TipoMedida::Perimetro => "Perímetro", TipoMedida::Area => "Área", TipoMedida::Angulo => "Ângulo", TipoMedida::Contagem => "Contagem" }
    }

    /// Pontos que fecham a medida sozinha (`None`: termina no clique duplo).
    pub fn pontos_fixos(self) -> Option<usize> {
        match self { TipoMedida::Distancia => Some(2), TipoMedida::Angulo => Some(3), _ => None }
    }

    /// Pontos mínimos para a medida valer.
    pub fn minimo(self) -> usize {
        match self { TipoMedida::Contagem => 1, TipoMedida::Distancia | TipoMedida::Perimetro => 2, _ => 3 }
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f64 { ((a.0 - b.0) as f64).hypot((a.1 - b.1) as f64) }

/// Comprimento da linha que passa pelos pontos, em pt.
pub fn comprimento(pontos: &[(f32, f32)]) -> f64 {
    pontos.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Área do polígono (fórmula do laço), em pt².
pub fn area(pontos: &[(f32, f32)]) -> f64 {
    let n = pontos.len();
    if n < 3 { return 0.0; }
    (0..n).map(|i| {
        let (a, b) = (pontos[i], pontos[(i + 1) % n]);
        a.0 as f64 * b.1 as f64 - b.0 as f64 * a.1 as f64
    }).sum::<f64>().abs() / 2.0
}

/// Ângulo em `b` entre os braços `b→a` e `b→c`, em graus (0 a 180).
pub fn angulo(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f64 {
    let (u, v) = (((a.0 - b.0) as f64, (a.1 - b.1) as f64), ((c.0 - b.0) as f64, (c.1 - b.1) as f64));
    let cruz = u.0 * v.1 - u.1 * v.0;
    let ponto = u.0 * v.0 + u.1 * v.1;
    cruz.abs().atan2(ponto).to_degrees()
}

/// Número com vírgula decimal e ponto nos milhares (pt-BR).
pub fn numero(v: f64, casas: usize) -> String {
    let s = format!("{:.*}", casas, v.abs());
    let (inteira, decimal) = s.split_once('.').unwrap_or((&s, ""));
    let mut milhar = String::new();
    for (i, c) in inteira.chars().enumerate() {
        if i > 0 && (inteira.len() - i) % 3 == 0 { milhar.push('.'); }
        milhar.push(c);
    }
    let sinal = if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') { "-" } else { "" };
    if decimal.is_empty() { format!("{sinal}{milhar}") } else { format!("{sinal}{milhar},{decimal}") }
}

/// Valor da medida já na escala (comprimento, área ou graus) e o texto dele.
pub fn medir(tipo: TipoMedida, pontos: &[(f32, f32)], escala: &Escala) -> (f64, String) {
    let u = escala.unidade;
    match tipo {
        TipoMedida::Distancia | TipoMedida::Perimetro => {
            let v = comprimento(pontos) * escala.fator;
            (v, format!("{} {u}", numero(v, 2)))
        }
        TipoMedida::Area => {
            let v = area(pontos) * escala.fator * escala.fator;
            (v, format!("{} {u}²", numero(v, 2)))
        }
        TipoMedida::Contagem => (pontos.len() as f64, format!("{} {}", pontos.len(), if pontos.len() == 1 { "item" } else { "itens" })),
        TipoMedida::Angulo => {
            let v = if pontos.len() >= 3 { angulo(pontos[0], pontos[1], pontos[2]) } else { 0.0 };
            (v, format!("{}°", numero(v, 1)))
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn medidas_na_escala() {
        // 72 pt = 1 polegada = 2,54 cm no papel.
        let papel = Escala::default();
        assert_eq!(medir(TipoMedida::Distancia, &[(0.0, 0.0), (72.0, 0.0)], &papel).1, "2,54 cm");
        // Planta 1:100 em metros: 1 pol no papel = 2,54 m.
        let planta = Escala::proporcao(Unidade::M, 100.0);
        assert_eq!(medir(TipoMedida::Distancia, &[(0.0, 0.0), (72.0, 0.0)], &planta).1, "2,54 m");
        assert_eq!(medir(TipoMedida::Perimetro, &[(0.0, 0.0), (72.0, 0.0), (72.0, 72.0)], &planta).1, "5,08 m");
        assert_eq!(medir(TipoMedida::Area, &[(0.0, 0.0), (72.0, 0.0), (72.0, 72.0), (0.0, 72.0)], &planta).1, "6,45 m²");
        assert_eq!(medir(TipoMedida::Angulo, &[(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)], &planta).1, "90,0°");
        // Calibrada: um traço de 200 pt mede 5 m.
        let cal = Escala::calibrada(200.0, 5.0, Unidade::M).unwrap();
        assert_eq!(medir(TipoMedida::Distancia, &[(0.0, 0.0), (0.0, 400.0)], &cal).1, "10,00 m");
        assert_eq!(numero(1234567.891, 2), "1.234.567,89");
        assert_eq!(medir(TipoMedida::Contagem, &[(1.0, 1.0), (5.0, 5.0), (9.0, 9.0)], &cal).1, "3 itens");
        assert_eq!(Escala::proporcao(Unidade::M, 1000.0).rotulo(), "1:1.000 em m");
    }
}
