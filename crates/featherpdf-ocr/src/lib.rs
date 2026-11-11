//! OCR em Rust puro: os modelos PP-OCR (PaddleOCR, Apache-2.0) em ONNX rodando no `tract`.
//!
//! Duas etapas, como no PaddleOCR: a detecção (DB) acha as linhas de texto num mapa de
//! probabilidade; o reconhecimento (CTC) lê cada linha recortada. Recebe a página já
//! desenhada em RGB e devolve as linhas com a caixa em pixels.

use std::path::Path;

use tract_onnx::prelude::*;
use tract_onnx::tract_hir::infer::Factoid;

type Plano = std::sync::Arc<TypedRunnableModel>;

/// Uma linha de texto achada na imagem.
#[derive(Debug, Clone)]
pub struct Linha {
    pub texto: String,
    /// x0, y0, x1, y1 em pixels da imagem (y para baixo).
    pub caixa: [f32; 4],
    /// Média da confiança de cada letra, 0..1.
    pub confianca: f32,
    /// As palavras da linha, com a caixa tirada da posição de cada letra no CTC.
    pub palavras: Vec<Palavra>,
}

#[derive(Debug, Clone)]
pub struct Palavra {
    pub texto: String,
    /// x0, y0, x1, y1 em pixels da imagem (a altura é a da linha).
    pub caixa: [f32; 4],
    pub confianca: f32,
}

/// Imagem RGB 8 bits, linha por linha.
pub struct Imagem<'a> {
    pub largura: u32,
    pub altura: u32,
    pub rgb: &'a [u8],
}

pub struct Ocr {
    det: InferenceModel,
    /// Detecção montada por tamanho de entrada: o tract não fecha a conta com altura e
    /// largura simbólicas neste modelo, e as páginas de um PDF quase sempre têm o mesmo tamanho.
    deteccao: std::sync::Mutex<std::collections::HashMap<(usize, usize), Plano>>,
    rec: InferenceModel,
    /// Reconhecimento por faixa de largura (`LARGURAS`), pelo mesmo motivo.
    reconhecimento: std::sync::Mutex<std::collections::HashMap<usize, Plano>>,
    /// Índice 0 é o "branco" do CTC; o último é o espaço.
    letras: Vec<String>,
    altura_linha: usize,
}

/// Maior lado da imagem que vai para a detecção (o PaddleOCR usa 960; documento pede mais).
const LADO_DETECCAO: u32 = 1600;
const LIMIAR: f32 = 0.3;
const LIMIAR_CAIXA: f32 = 0.6;
const EXPANDIR: f32 = 1.5;
/// Larguras em que a linha é lida; a linha é completada à direita até a faixa.
const LARGURAS: [usize; 8] = [160, 320, 480, 640, 960, 1280, 1920, 2560];

/// De cima para baixo e, na mesma linha, da esquerda para a direita. Agrupa contra a
/// primeira caixa do grupo (comparar par a par não é ordem total e o `sort_by` pode entrar em pânico).
fn ordem_de_leitura(linhas: &mut [Linha]) {
    linhas.sort_by(|a, b| a.caixa[1].total_cmp(&b.caixa[1]));
    let mut inicio = 0;
    while inicio < linhas.len() {
        let p = linhas[inicio].caixa;
        let fim = inicio + 1 + linhas[inicio + 1..].iter().take_while(|l| {
            (l.caixa[1] - p[1]).abs() < (l.caixa[3] - l.caixa[1]).min(p[3] - p[1]) * 0.5
        }).count();
        linhas[inicio..fim].sort_by(|a, b| a.caixa[0].total_cmp(&b.caixa[0]));
        inicio = fim;
    }
}

impl Ocr {
    /// `pasta` tem `det.onnx`, `rec.onnx` e `dict.txt`.
    pub fn abrir(pasta: &Path) -> Result<Self, String> {
        let ler = |nome: &str| std::fs::read(pasta.join(nome)).map_err(|e| format!("{nome}: {e}"));
        let dicionario = String::from_utf8(ler("dict.txt")?).map_err(|e| format!("dict.txt: {e}"))?;
        Self::de_bytes(&ler("det.onnx")?, &ler("rec.onnx")?, &dicionario)
    }

    /// Os modelos que vêm dentro do programa (`modelos/`, ~10 MB).
    pub fn embutido() -> Result<Self, String> {
        Self::de_bytes(include_bytes!("../modelos/det.onnx"), include_bytes!("../modelos/rec.onnx"),
            include_str!("../modelos/dict.txt"))
    }

    fn de_bytes(det: &[u8], rec: &[u8], dicionario: &str) -> Result<Self, String> {
        let det = tract_onnx::onnx().model_for_read(&mut &det[..]).map_err(|e| format!("Modelo de OCR inválido: {e:?}"))?;
        Self::montar(det, rec, dicionario)
    }

    fn montar(det: InferenceModel, rec: &[u8], dicionario: &str) -> Result<Self, String> {
        let rec = tract_onnx::onnx().model_for_read(&mut &rec[..]).map_err(|e| format!("Modelo de OCR inválido: {e:?}"))?;
        // A altura da linha vem do próprio modelo (48 no PP-OCRv5, 32 nos antigos).
        let altura_linha = rec.input_fact(0).ok().and_then(|f| f.shape.dims().nth(2).and_then(|d| d.concretize()))
            .and_then(|d| if let TDim::Val(v) = d { Some(v as usize) } else { None }).unwrap_or(48);
        let mut letras = vec![String::new()];
        letras.extend(dicionario.lines().map(str::to_string));
        letras.push(" ".into());
        Ok(Ocr { det, deteccao: Default::default(), rec, reconhecimento: Default::default(), letras, altura_linha })
    }

    /// Reconhece a imagem inteira: linhas de cima para baixo, da esquerda para a direita.
    pub fn reconhecer(&self, img: &Imagem<'_>) -> Result<Vec<Linha>, String> {
        let mut linhas = Vec::new();
        let caixas = self.detectar(img)?;
        // Uma thread por núcleo, cada uma com um pedaço das linhas.
        let nucleos = std::thread::available_parallelism().map_or(4, |n| n.get());
        let pedaco = caixas.len().div_ceil(nucleos).max(1);
        let lidas: Vec<Result<Vec<Linha>, String>> = std::thread::scope(|e| {
            let tarefas: Vec<_> = caixas.chunks(pedaco).map(|parte| e.spawn(move || parte.iter().map(|&caixa| {
                self.ler_linha(img, caixa).map(|(texto, confianca, palavras)| Linha { texto: texto.trim().to_string(), caixa, confianca, palavras })
            }).collect::<Result<Vec<_>, _>>())).collect();
            tarefas.into_iter().map(|t| t.join().unwrap_or_else(|_| Err("OCR travou numa linha.".into()))).collect()
        });
        for l in lidas { linhas.extend(l?.into_iter().filter(|l| !l.texto.is_empty())); }
        ordem_de_leitura(&mut linhas);
        Ok(linhas)
    }

    fn plano<K: std::hash::Hash + Eq + Copy>(planos: &std::sync::Mutex<std::collections::HashMap<K, Plano>>, chave: K,
        modelo: &InferenceModel, forma: [usize; 4], teto: usize) -> Result<Plano, String> {
        let mut planos = planos.lock().map_err(|e| e.to_string())?;
        if let Some(p) = planos.get(&chave) { return Ok(p.clone()); }
        let p = modelo.clone().with_input_fact(0, f32::fact(forma).into())
            .and_then(|m| m.into_optimized()).and_then(|m| m.into_runnable()).map_err(|e| format!("{e:?}"))?;
        // Cada plano pesa centenas de MB. A detecção quase nunca repete o tamanho (OCR de área), então guarda só o último;
        // o reconhecimento usa as faixas fixas de LARGURAS.
        if planos.len() >= teto { planos.clear(); }
        planos.insert(chave, p.clone());
        Ok(p)
    }

    /// Caixas das linhas de texto, em pixels da imagem original.
    fn detectar(&self, img: &Imagem<'_>) -> Result<Vec<[f32; 4]>, String> {
        let escala = (LADO_DETECCAO as f32 / img.largura.max(img.altura) as f32).min(1.0);
        // Os lados precisam ser múltiplos de 32.
        let ajusta = |v: u32| (((v as f32 * escala) / 32.0).round() as usize).max(1) * 32;
        let (w, h) = (ajusta(img.largura), ajusta(img.altura));
        let (media, desvio) = ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]);
        let entrada = tract_ndarray::Array4::from_shape_fn((1, 3, h, w), |(_, c, y, x)| {
            // Canais em BGR, como o OpenCV entrega ao PaddleOCR.
            let v = amostra(img, x as f32 * img.largura as f32 / w as f32, y as f32 * img.altura as f32 / h as f32, 2 - c);
            (v / 255.0 - media[c]) / desvio[c]
        });
        let plano = Self::plano(&self.deteccao, (h, w), &self.det, [1, 3, h, w], 1)?;
        let saida = plano.run(tvec!(Tensor::from(entrada).into())).map_err(|e| e.to_string())?;
        let mapa = saida[0].to_plain_array_view::<f32>().map_err(|e| e.to_string())?;
        let mapa = mapa.into_shape_with_order((h, w)).map_err(|e| e.to_string())?;
        let (fx, fy) = (img.largura as f32 / w as f32, img.altura as f32 / h as f32);
        Ok(componentes(&mapa, w, h).into_iter().map(|[x0, y0, x1, y1]| [
            (x0 * fx).max(0.0), (y0 * fy).max(0.0), (x1 * fx).min(img.largura as f32), (y1 * fy).min(img.altura as f32),
        ]).collect())
    }

    /// Lê uma linha: recorta, deixa na altura do modelo e decodifica o CTC.
    fn ler_linha(&self, img: &Imagem<'_>, [x0, y0, x1, y1]: [f32; 4]) -> Result<(String, f32, Vec<Palavra>), String> {
        let h = self.altura_linha;
        let (lc, ac) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let maior = *LARGURAS.last().unwrap();
        let w = ((lc / ac * h as f32).ceil() as usize).clamp(8, maior);
        let faixa = LARGURAS.into_iter().find(|&f| f >= w).unwrap_or(maior);
        // O resto da faixa fica em 0 já normalizado (cinza médio), como o PaddleOCR completa.
        let entrada = tract_ndarray::Array4::from_shape_fn((1, 3, h, faixa), |(_, c, y, x)| {
            if x >= w { return 0.0; }
            let v = amostra(img, x0 + (x as f32 + 0.5) * lc / w as f32, y0 + (y as f32 + 0.5) * ac / h as f32, 2 - c);
            (v / 255.0 - 0.5) / 0.5
        });
        let plano = Self::plano(&self.reconhecimento, faixa, &self.rec, [1, 3, h, faixa], LARGURAS.len())?;
        let saida = plano.run(tvec!(Tensor::from(entrada).into())).map_err(|e| e.to_string())?;
        let p = saida[0].to_plain_array_view::<f32>().map_err(|e| e.to_string())?;
        let (passos, classes) = (p.shape()[1], p.shape()[2]);
        let mut texto = String::new();
        let (mut soma, mut n, mut anterior) = (0.0, 0, 0);
        // Cada passo do CTC cobre `faixa / passos` colunas da linha redimensionada.
        let coluna = |t: f32| x0 + t * faixa as f32 / passos as f32 * lc / w as f32;
        let mut palavras: Vec<Palavra> = Vec::new();
        let mut atual: Option<(String, f32, f32, f32, usize)> = None; // texto, início, fim, soma, n
        let mut fechar = |atual: &mut Option<(String, f32, f32, f32, usize)>| if let Some((t, a, b, s, n)) = atual.take() {
            palavras.push(Palavra { texto: t, caixa: [a.max(x0), y0, b.min(x1), y1], confianca: s / n as f32 });
        };
        for t in 0..passos {
            let (k, v) = (0..classes).map(|k| (k, p[[0, t, k]])).fold((0, f32::MIN), |m, x| if x.1 > m.1 { x } else { m });
            if k != 0 && k != anterior {
                let letra = self.letras.get(k).map_or("", String::as_str);
                texto += letra;
                soma += v;
                n += 1;
                if letra.trim().is_empty() {
                    fechar(&mut atual);
                } else {
                    let (ini, fim) = (coluna(t as f32 - 0.5), coluna(t as f32 + 1.5));
                    match &mut atual {
                        Some((txt, _, b, s, m)) => { *txt += letra; *b = fim; *s += v; *m += 1; }
                        None => atual = Some((letra.to_string(), ini, fim, v, 1)),
                    }
                }
            }
            anterior = k;
        }
        fechar(&mut atual);
        Ok((texto, if n == 0 { 0.0 } else { soma / n as f32 }, palavras))
    }
}

/// Gira a imagem RGB em `graus` (múltiplo de 90) no sentido horário.
pub fn girar(img: &Imagem<'_>, graus: u32) -> (u32, u32, Vec<u8>) {
    let (w, h) = (img.largura as usize, img.altura as usize);
    let (nw, nh) = if graus % 180 == 90 { (h, w) } else { (w, h) };
    let mut saida = vec![0u8; nw * nh * 3];
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = match graus % 360 {
                90 => (y, h - 1 - x),
                180 => (w - 1 - x, h - 1 - y),
                270 => (w - 1 - y, x),
                _ => (x, y),
            };
            let (d, o) = ((y * nw + x) * 3, (sy * w + sx) * 3);
            saida[d..d + 3].copy_from_slice(&img.rgb[o..o + 3]);
        }
    }
    (nw as u32, nh as u32, saida)
}

/// Valor do canal `c` no ponto (x, y), vizinho mais próximo.
fn amostra(img: &Imagem<'_>, x: f32, y: f32, c: usize) -> f32 {
    let xi = (x.max(0.0) as u32).min(img.largura - 1) as usize;
    let yi = (y.max(0.0) as u32).min(img.altura - 1) as usize;
    img.rgb[(yi * img.largura as usize + xi) * 3 + c] as f32
}

/// Regiões ligadas do mapa acima do limiar, como retângulos alinhados, já expandidos
/// (o modelo DB encolhe o texto; a expansão devolve a borda das letras).
// shortcut: retângulo alinhado aos eixos em vez do retângulo girado do PaddleOCR; serve para página reta, trocar se vier scan torto.
fn componentes(mapa: &tract_ndarray::ArrayView2<f32>, w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut visto = vec![false; w * h];
    let mut caixas = Vec::new();
    let mut pilha = Vec::new();
    for inicio in 0..w * h {
        if visto[inicio] || mapa[[inicio / w, inicio % w]] < LIMIAR { continue; }
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut soma, mut n) = (0.0, 0usize);
        visto[inicio] = true;
        pilha.push(inicio);
        while let Some(i) = pilha.pop() {
            let (y, x) = (i / w, i % w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            soma += mapa[[y, x]];
            n += 1;
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 { continue; }
                let j = ny as usize * w + nx as usize;
                if !visto[j] && mapa[[ny as usize, nx as usize]] >= LIMIAR { visto[j] = true; pilha.push(j); }
            }
        }
        if n < 6 || soma / (n as f32) < LIMIAR_CAIXA { continue; }
        let (l, a) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        let d = l * a * EXPANDIR / (2.0 * (l + a));
        caixas.push([x0 as f32 - d, y0 as f32 - d, x1 as f32 + 1.0 + d, y1 as f32 + 1.0 + d]);
    }
    caixas
}

#[cfg(test)]
mod testes {
    use super::*;

    fn linha(texto: &str, x: f32, y: f32, altura: f32) -> Linha {
        Linha { texto: texto.into(), caixa: [x, y, x + 40.0, y + altura], confianca: 1.0, palavras: Vec::new() }
    }

    #[test]
    fn ordem_de_leitura_agrupa_pela_primeira_caixa() {
        let mut l = vec![linha("C", 0.0, 8.0, 10.0), linha("A", 100.0, 0.0, 10.0), linha("B", 50.0, 4.0, 10.0)];
        ordem_de_leitura(&mut l);
        let ordem: String = l.iter().map(|l| l.texto.as_str()).collect();
        assert_eq!(ordem, "BAC");
    }

    #[test]
    fn ordem_de_leitura_nao_entra_em_panico() {
        // Caixas "bagunçadas" (gerador linear simples): sem pânico, e o y nunca volta mais que meia linha.
        let mut semente = 12345u32;
        let mut sorteio = || { semente = semente.wrapping_mul(1103515245).wrapping_add(12345); (semente >> 16) as f32 / 65536.0 };
        let mut l: Vec<Linha> = (0..60).map(|i| linha(&i.to_string(), sorteio() * 500.0, sorteio() * 300.0, 5.0 + sorteio() * 20.0)).collect();
        ordem_de_leitura(&mut l);
        assert_eq!(l.len(), 60);
        for par in l.windows(2) {
            let meia = (par[0].caixa[3] - par[0].caixa[1]).max(par[1].caixa[3] - par[1].caixa[1]) * 0.5;
            assert!(par[1].caixa[1] >= par[0].caixa[1] - meia);
        }
    }
}
