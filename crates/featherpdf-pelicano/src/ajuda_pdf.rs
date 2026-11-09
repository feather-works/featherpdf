//! Conteúdo próprio do FeatherPDF dentro do componente F1 compartilhado.
//! A primeira linha de cada tópico vira o resumo do cartão: curta, para caber em três colunas.
//! Ids como "comecar", "dados", "formulas" e "tabelas" ficam de fora de propósito: o componente
//! liga a eles guias rápidos de planilha ("Criar planilha", "Inserir função"…).
use std::sync::OnceLock;
use feather_ui::ajuda::{Catalogo, Linha, Secao, Topico};

fn t(s: &str) -> Linha { Linha::Texto(s.into()) }
fn m(s: &str) -> Linha { Linha::Marcador(s.into()) }
fn a(descricao: &str, tecla: &str) -> Linha { Linha::Atalho { descricao: descricao.into(), tecla: tecla.into() } }
fn topico(id: &str, titulo: &str, linhas: Vec<Linha>) -> Topico { Topico { id: id.into(), titulo: titulo.into(), linhas } }

pub fn modelo() -> &'static Catalogo {
    static CATALOGO: OnceLock<Catalogo> = OnceLock::new();
    CATALOGO.get_or_init(|| Catalogo {
        titulo: "Ajuda do FeatherPDF".into(),
        busca_rotulo: "Pesquisar na ajuda".into(),
        nada_encontrado: "Nenhum tópico encontrado. Tente outra palavra, como “tarja”, “OCR” ou “senha”.".into(),
        anterior: "Anterior".into(),
        proximo: "Próximo".into(),
        nesta_secao: "Nesta seção".into(),
        secoes: vec![
            Secao {
                id: "inicio".into(),
                titulo: "Como usar".into(),
                introducao: "Abra, edite, comente, proteja e converta PDFs sem internet. O arquivo salvo continua vetorial: textos e desenhos não viram imagem.".into(),
                topicos: vec![
                    topico("arquivos", "Abrir, criar e salvar", vec![
                        t("Abrir (Ctrl+O), novo (Ctrl+N), salvar (Ctrl+S)."),
                        m("Arraste PDFs ou imagens (PNG, JPG, TIFF) para a janela: cada um abre numa aba. Imagem vira PDF de uma página; o TIFF de várias páginas do scanner vira um PDF com todas."),
                        m("PDF com senha pede a senha ao abrir."),
                        m("Ctrl+S grava por cima; Ctrl+Shift+S salva uma cópia com outro nome."),
                        m("A gravação é segura: o arquivo novo é escrito à parte e só troca o antigo no fim, mantendo as permissões."),
                        m("PDF assinado não é regravado por cima: o app pede “Salvar como”, porque mudar o arquivo invalida as assinaturas."),
                        m("Fechar uma aba com alterações não salvas pede confirmação."),
                        m("Ctrl+D (ou ☰ Menu ▸ Propriedades do documento): título, autor, assunto e palavras-chave editáveis, e o programa que gerou, datas, versão, páginas e tamanho."),
                        m("A página inicial mostra os recentes e atalhos para abrir, criar, converter, dividir e extrair páginas; ela aparece quando não há documento aberto."),
                        t("Os recentes ficam no menu Arquivo (☰ Menu)."),
                    ]),
                    topico("abas", "Páginas", vec![
                        t("Girar, excluir, juntar, extrair e dividir."),
                        m("Aba Páginas ou Organizar páginas: miniaturas para arrastar e mudar a ordem; Delete exclui as escolhidas."),
                        m("Juntar acrescenta outro PDF; Extrair e Dividir geram arquivos novos sem mexer no aberto."),
                        m("Números de página, Numeração Bates, Marca d'água e Cabeçalho e rodapé."),
                        m("Caixas da página, Padronizar tamanho, Frente e verso e Dividir colunas (scan com duas páginas lado a lado)."),
                        t("Sempre fica pelo menos uma página no documento."),
                    ]),
                    topico("texto", "Editar texto e objetos", vec![
                        t("Troque palavras e mova objetos no próprio PDF."),
                        m("A troca usa a fonte do arquivo; letra que a fonte não tem é recusada para não estragar o texto."),
                        m("Ctrl+H localiza e substitui em todas as páginas."),
                        m("Insira texto, imagem, tabela, equação, link, formas e ícones pela aba Editar. No link, um e-mail sozinho (fulano@site.com) vira link de e-mail."),
                        m("Escolha objetos para mover (setas; Shift move 10), alinhar, trazer para frente ou excluir (Delete)."),
                        m("Editar imagem ajusta brilho, contraste e recorte da foto escolhida."),
                        t("Ctrl+Z desfaz e Ctrl+Y refaz."),
                    ]),
                    topico("preencher", "Comentar e anotar", vec![
                        t("Destaque, risque, escreva notas e desenhe."),
                        m("Aba Comentar: Destacar, Sublinhar, Riscar, Ondular, Substituir, Nota, Caixa de texto, Desenhar e Carimbo."),
                        m("Carimbos prontos: Aprovado, Revisado, Recebido, Pago, Conferido, Rascunho, Confidencial e Reprovado."),
                        m("O painel Comentários lista tudo: Responder, Editar (troca o texto de uma nota ou caixa de texto já criada) e a lixeira; a lista pode ir para CSV ou para o FeatherSheet."),
                    ]),
                    topico("formas", "Formulários", vec![
                        t("Preencha, crie campos e faça cálculos."),
                        m("Clique num campo para digitar."),
                        m("Crie campos de texto, caixa de marcar, opção, lista, data e assinatura; Detectar campos acha linhas e caixas de um formulário impresso."),
                        m("Regras e Tab: formato (CPF, CNPJ, CEP, telefone, moeda, e-mail, número), ordem do Tab e cálculos (soma, média, produto, menor, maior)."),
                        m("Importar e exportar dados (JSON, XFDF, CSV) e Pasta de fichas para ler vários formulários de uma vez."),
                    ]),
                    topico("medir", "Medir e contar", vec![
                        t("Distâncias, áreas e perímetros em plantas."),
                        m("Escala… calibra pela cota do desenho (ex.: 1:50) ou por uma linha de tamanho conhecido."),
                        m("Clique ponto a ponto; Enter termina e Backspace desfaz o último ponto."),
                        m("Ímã: o ponto gruda nos cantos e pontas das linhas do desenho (marcador verde). Desligue no painel se atrapalhar."),
                        m("Segure Shift para travar o ângulo em passos de 45°."),
                        m("Área e perímetro fecham sozinhos: clique de novo no primeiro ponto ou use Fechar."),
                        m("Medidas lista tudo o que foi medido na página."),
                    ]),
                    topico("proteger", "Proteger e assinar", vec![
                        t("Senha, assinatura digital e limpeza."),
                        m("Senha: senha para abrir (AES-256) e permissões de imprimir, copiar e editar. A senha de dono precisa ser diferente da de abrir. Num PDF que já tem senha, a janela mostra a cifra e o que ele não deixa fazer."),
                        m("Assinar: certificado A1 (.pfx/.p12), visível ou não. Validar confere se o arquivo mudou depois de assinado."),
                        m("O verificador não consulta a cadeia ICP-Brasil: identidade não verificada aparece em âmbar. Para valor jurídico, confira em validar.iti.gov.br."),
                        m("Limpar e sanitizar tira JavaScript, links externos, anexos, metadados (inclusive EXIF de fotos), camadas ocultas e, se marcado, os comentários."),
                        m("Achatar transforma campos e comentários em desenho fixo."),
                    ]),
                    topico("tarja", "Tarja definitiva", vec![
                        t("Apague de verdade CPF, nomes e sigilos."),
                        m("Marcar área: arraste sobre o que quer apagar. Buscar e tarjar acha um texto; Padrões acha CPF, CNPJ, e-mail, telefone, CEP e nomes."),
                        m("Aplicar apaga o texto, as imagens e os comentários embaixo da tarja, não só cobre com preto."),
                        m("O resultado sai numa cópia “— tarjado.pdf”; o arquivo aberto não muda."),
                        m("Se o PDF era assinado, as assinaturas não valem na cópia tarjada."),
                    ]),
                    topico("converter", "Converter e exportar", vec![
                        t("Word, planilha, imagens, texto e mais."),
                        m("Word (.docx) e ODT, Excel (.xlsx) e CSV, apresentação (.pptx), texto, HTML e Markdown."),
                        m("Imagens: PNG, JPG, WebP, BMP, GIF, TIFF de várias páginas e SVG vetorial. Em Páginas, \"Escolher\" aceita intervalos como 1-3, 5."),
                        m("FeatherDocs e FeatherSheet abrem o resultado direto no app da suíte."),
                        m("Tabela do scan e Gráfico para dados: arraste em volta e os números vão para planilha."),
                        m("PDF/A para arquivamento, Comprimir (fotos a menos dpi), Embutir fontes e Para a web (linearizar)."),
                    ]),
                    topico("ocr", "OCR", vec![
                        t("Texto de páginas escaneadas, sem internet."),
                        m("Converter → OCR abre a janela: escolha as páginas e o idioma e clique em Reconhecer."),
                        m("Português e as línguas latinas já vêm no app; grego, cirílico, árabe, coreano, chinês e japonês são baixados na primeira vez."),
                        m("O resultado pode ser texto invisível atrás da imagem (busca e cópia passam a funcionar) ou texto editável no lugar da imagem."),
                        m("Corrija palavras clicando nelas; âmbar indica pouca certeza."),
                        m("OCR da área: arraste em volta de um trecho para ler só ele, copiar ou tornar pesquisável."),
                        m("Endireitar a página gira o scan torto antes de ler."),
                    ]),
                    topico("fluxos", "Fluxos em blocos", vec![
                        t("A mesma sequência em vários PDFs de uma vez."),
                        m("Converter → Fluxos em lote abre a janela de blocos."),
                        m("Entrada: arraste pastas, PDFs ou imagens para a janela, ou use o arquivo aberto. Saída: uma pasta (padrão: “processados” ao lado da entrada)."),
                        m("Modelos prontos montam a sequência de uma vez; o modo diz se sai um arquivo por entrada, tudo junto num PDF, dividido, PNG, Word ou planilha."),
                        m("Juntar imagens: várias fotos viram um PDF com uma imagem por página."),
                        m("Blocos: girar (todas, pares, ímpares ou escolhidas), numerar, marca-d'água, cabeçalho, tirar páginas em branco, OCR, tarjar padrões, clarear fundo, limpar caderno, normalizar tamanho, apagar metadados, embutir fontes, comprimir, PDF/A e linearizar."),
                        m("Arraste os blocos para mudar a ordem. Para tirar: ×, Delete ou arraste para fora. Botão direito: subir, descer, duplicar, remover."),
                        m("Testar com 1 roda no primeiro arquivo sem gravar e mostra a prévia. Rodar grava todos; os originais não mudam."),
                    ]),
                    topico("scripts", "Scripts", vec![
                        t("Automatize tarefas com JavaScript."),
                        m("Converter → Script abre o editor ao lado da página."),
                        m("pdf.pages(), pdf.text(p), pdf.find(termo), pdf.addText(p, x, y, texto, tamanho), pdf.rotate(páginas, graus), pdf.deletePages(páginas), pdf.fields(), pdf.setField(nome, valor) e log(…)."),
                        m("O script não acessa arquivos nem a rede. Ctrl+Z desfaz o que ele fez."),
                        Linha::Exemplo("for (let p = 1; p <= pdf.pages(); p++) {\n  pdf.addText(p, 500, 30, 'Pág. ' + p, 9);\n}".into()),
                    ]),
                    topico("versoes", "Versões e comparar", vec![
                        t("Versões guardadas no arquivo e diferenças."),
                        m("Converter → Versões lista as atualizações guardadas dentro do arquivo (comum em PDFs assinados)."),
                        m("Comentar → Comparar mostra o que mudou entre o aberto e outro PDF."),
                    ]),
                    topico("exibir", "Exibir e navegar", vec![
                        t("Zoom, duas páginas, leitura e tela cheia."),
                        m("Ctrl++ e Ctrl+- mudam o zoom; Ctrl+0 volta a 100%. Na caixa do zoom dá para digitar o percentual (Enter aplica). Largura, Altura e Inteira ajustam à janela."),
                        m("Página escura inverte as cores para ler à noite; Leitura esconde as barras."),
                        m("Painéis: Miniaturas, Marcadores, Comentários, Camadas e Anexos, na barra da direita."),
                        m("Ctrl+K pesquisa comandos pelo nome."),
                    ]),
                    topico("inspetor", "Inspetor (F12)", vec![
                        t("Objetos, fontes, problemas e desempenho."),
                        m("Desempenho mostra memória, CPU e o tempo de desenho de cada página."),
                        m("Console guarda o que o app fez nesta sessão, útil para relatar um erro."),
                        m("Preflight (Converter) aponta o que impede imprimir ou arquivar."),
                    ]),
                    topico("configuracoes", "Configurações", vec![
                        t("A engrenagem no alto abre as Configurações."),
                        m("Aparência (tema) e preferências dos documentos."),
                        m("Leitor de PDF padrão ▸ Tornar padrão: no Linux os PDFs passam a abrir no FeatherPDF com dois cliques; no Windows abre a tela de Aplicativos padrão já no FeatherPDF, para escolher .pdf."),
                    ]),
                ],
            },
            Secao {
                id: "atalhos".into(),
                titulo: "Atalhos".into(),
                introducao: "Teclas do FeatherPDF e da suíte FeatherWorks.".into(),
                topicos: vec![
                    topico("globais", "Janelas e painéis", vec![
                        t("Ajuda, inspetor, faixa, comandos e tela cheia."),
                        a("Ajuda pesquisável", "F1"),
                        a("Inspetor e desempenho", "F12"),
                        a("Recolher ou expandir a faixa", "Ctrl+F1"),
                        a("Pesquisar comandos", "Ctrl+K"),
                        a("Tela cheia", "F11"),
                        a("Fechar painel ou cancelar", "Esc"),
                    ]),
                    topico("teclas-arquivos", "Arquivos e abas", vec![
                        t("Novo, abrir, salvar, imprimir e trocar de aba."),
                        a("Novo PDF A4", "Ctrl+N"),
                        a("Abrir PDF", "Ctrl+O"),
                        a("Salvar", "Ctrl+S"),
                        a("Salvar como", "Ctrl+Shift+S"),
                        a("Imprimir", "Ctrl+P"),
                        a("Fechar documento", "Ctrl+W"),
                        a("Próxima aba", "Ctrl+Tab"),
                    ]),
                    topico("teclas-editar", "Editar", vec![
                        t("Desfazer, localizar, copiar, colar e mover."),
                        a("Desfazer", "Ctrl+Z"),
                        a("Refazer", "Ctrl+Y / Ctrl+Shift+Z"),
                        a("Localizar", "Ctrl+F"),
                        a("Localizar e substituir", "Ctrl+H"),
                        a("Copiar texto selecionado", "Ctrl+C"),
                        a("Colar imagem ou texto", "Ctrl+V"),
                        a("Mover objeto (Shift: 10 pt)", "Setas"),
                        a("Excluir objeto ou página escolhida", "Delete"),
                        a("Escolher todas as páginas (Organizar)", "Ctrl+A"),
                    ]),
                    topico("teclas-exibir", "Navegar e zoom", vec![
                        t("Páginas, zoom e medidas."),
                        a("Página anterior", "PageUp"),
                        a("Próxima página", "PageDown"),
                        a("Aumentar zoom", "Ctrl++"),
                        a("Diminuir zoom", "Ctrl+-"),
                        a("Zoom 100%", "Ctrl+0"),
                        a("Terminar medida / desfazer ponto", "Enter / Backspace"),
                    ]),
                ],
            },
        ],
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn ajuda_f1_e_f12_compartilhada() {
        let c = modelo();
        for termo in ["F1", "F12", "salvar", "tarja", "OCR", "fluxos", "senha", "assinatura"] {
            assert!(c.quantidade_encontrada(termo) > 0, "{termo}");
        }
    }

    #[test]
    fn resumo_dos_cartoes_cabe() {
        // O cartão corta em 84 letras; acima de ~48 o resumo passa da borda quando o título ocupa duas linhas.
        for t in &modelo().secoes[0].topicos {
            let Some(Linha::Texto(r)) = t.linhas.first() else { panic!("{} sem resumo", t.id) };
            assert!(r.chars().count() <= 48, "{}: {} letras", t.id, r.chars().count());
        }
    }
}
