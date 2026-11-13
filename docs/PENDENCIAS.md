# Pendências do FeatherPDF

## Modo escuro (13/11/2026)

Corrigido: imagem quase toda branca (logotipo em fundo branco, página escaneada) escurece junto
na Página escura (antes ficava um bloco branco, e o PDF escaneado não escurecia nada); a barra de
rolagem da faixa ficou fina e escura no tema escuro; nome de arquivo comprido não empurra mais o
"salvo" e o × para fora da aba (o nome é cortado com "…"); "Pesquisar comandos…" cabe inteiro. Limite: foto de produto em fundo branco também
escurece (mais da metade branca); página girada escurece tudo, fotos incluídas. Sem conferir no
app aberto, só pela captura (`FEATHERPDF_CAPTURA_ESCURO`, `FEATHERPDF_CAPTURA_PAGINA_ESCURA`).

## Auditoria de 11/11/2026

Corrigido neste dia:
- PDF malicioso: todo fluxo descomprimido tem teto de 256 MB (bomba de compressão no /ToUnicode
  derrubava o app ao abrir); fórmula de campo com milhares de parênteses é recusada (estourava a
  pilha ao digitar); tabela /W com faixas enormes tem teto; árvore /Dests com /Kids repetido não
  multiplica a busca.
- Tarja: as marcas são descartadas ao trocar de aba, abrir outro arquivo, editar ou desfazer (caíam
  na página errada e o texto ficava legível); imagem usada em várias páginas é apagada numa cópia
  só da página tarjada; /ActualText e /Alt dos elementos de estrutura da página saem.
- Comprimir e extrair/dividir páginas para arquivo mantêm a senha do PDF; tirar anexos tira
  também os do /AF (fatura PDF/A-3).
- Comprimir sem cópia recusa PDF assinado e fecha a aba antiga do arquivo trocado.
- Marcador e link escolhidos e comentário pendente não passam para a outra aba; "Juntar" confere a
  aba pelo id; cancelar o "Salvar como" do fechar não fecha a aba no próximo Ctrl+S; abrir um
  arquivo só reaproveita a aba do Ctrl+N intocado (foto ou imagens de uma página não se perdem).
- Ctrl+S confere assinatura pelo mapa do arquivo, sem o teto de 128 MB.
- `instancia.txt` com permissão 0600 e pedido com teto de 64 KB.
- Ordem de Tab "por linhas" em faixas (o sort não era transitivo e podia entrar em pânico);
  número "1,234.56" lido como 1234,56.

Segunda auditoria do dia (sobre o que entrou):
- Tarja: dois nomes (/Im1, /Im2) da mesma imagem passam os dois para a cópia tarjada.
- Tirar um anexo tira também do /AF; medir anexos sem /Size tem 64 MB para todos juntos; a busca
  conta também o anexo binário no teto.
- Referência solta no sumário não desalinha o marcador escolhido do que é editado.
- Excluir, mover e colar páginas mantêm o nome lógico de cada página (/PageLabels refeita).
- Link em edição é esquecido depois de qualquer edição; girar imagem só troca largura/altura do
  painel se girou; o nome lógico da barra de baixo fica guardado até a próxima edição; elementos
  de estrutura lidos uma vez por tarja.
- Comprimir sem cópia não fecha a aba que tem algo por salvar.

Ficou para depois:
- Anexar arquivo (lê e comprime até 256 MB) e Copiar/Recortar páginas (extrai o documento) rodam
  na thread da janela.
- Trabalho pesado ainda na thread da janela: abrir o Ctrl+F (lê todos os objetos e anexos), achar
  e aplicar tarja, proteger com senha, levar ao FeatherDocs. Passar para `spawn_blocking`.
- Cache `objetos` só cresce ao rolar documento enorme: soltar as páginas longe da tela.
- Tarja de texto desenhado sem `Tf` (fonte vinda de `gs` com /Font) não é achada.
- Link de site abre o navegador num clique (há a opção de desligar); pensar em confirmar o domínio.
- `instancia.txt` velho depois de travar: apagar ao sair.

## Audit de 04/11/2026

Corrigido neste dia:
- Ctrl+Z com teto de peso (512 MB), além dos 20 passos: PDF grande não enche a memória (issue #2).
- Depois de cada edição, o PDF é regravado e reinterpretado em outra thread: a janela não para
  (issue #3). Quem desenha, imprime ou exporta nesse meio-tempo espera o desenhador novo.
- Arquivos de passagem (imprimir, levar ao FeatherDocs/FeatherSheet) saem do /tmp comum e vão
  para `~/.config/featherpdf/temporarios` (0700).
- Modelo de OCR já baixado é conferido pelo hash, não só pelo tamanho.
- `PARIDADE-ACROBAT.md`: seções 1 e 17 conferidas de novo.

- Salvar (Ctrl+S e "Salvar como") grava em outra thread. Enquanto grava, a aba e a janela não
  fecham; edição feita no meio continua por salvar; falha volta a marcar "não salvo". O "salvar
  antes de fechar" continua na thread da janela (a aba fecha logo depois).
- Segundo audit do dia: o desenhador em segundo plano tem uma thread só e um pedido na fila
  (segurar Ctrl+Z num PDF grande juntava dezenas de cópias); o teto do Ctrl+Z vale para todas as
  abas juntas; colar texto troca TAB por espaço e recusa o que não cabe na página; "Comentários"
  no Sanitizar não tira vídeo, som nem 3D.

Falta medir com PDFs reais grandes (memória e tempo) e ver as telas novas rodando.

## Auditoria de 19/10/2026 (código novo da leva dos 29 itens "planejado")

Nada disto foi conferido rodando a GUI. "Copiar como LaTeX" foi retirado a pedido do
usuário (19/10), e com ele o achado sobre `\alphax`.

### Precisa corrigir

1. ✅ (04/11) **"Limpar caderno" estraga fotos e logotipos.** Agora só limpa imagem grande e quase toda papel (`parece_papel`). `featherpdf-io/src/lib.rs`
   `limpar_caderno_jpeg` (chamado por `Msg::LimparCaderno` e pelo bloco do fluxo). O
   `limpar_fundo` passa *toda* imagem pelo filtro; o `clarear_fundo_jpeg` testa antes se a
   imagem parece papel (`niveis_do_papel`), o caderno não. Correção: a mesma checagem no
   começo, ou só imagem que cobre a página inteira.
2. ✅ (04/11) **Refluxo invade a coluna ao lado.** Linha sozinha usa a margem das linhas da mesma coluna. `featherpdf-core/src/refluxo.rs` (margem com uma
   linha só = folga da esquerda espelhada). Em artigo de duas colunas a linha da esquerda
   cresce até a borda da página, por cima da direita. Correção: com uma linha, usar o maior
   `caixa[2]` das linhas de texto da página que começam no mesmo x (±2 pt).
3. ✅ (já corrigido, conferido em 04/11) **Falso "pode faltar rubrica".** `featherpdf-pelicano/src/reconhecer.rs` (`CANTO`) +
   `featherpdf-core/src/reconhecer.rs` (`rubricas_faltando`). Número de página e rodapé
   impressos contam como tinta: num documento numerado a partir da página 2, a capa é
   apontada. Correção: passar como `ignorar` as caixas de texto do canto (o `conteudo(p)`
   já tem).

### Deveria corrigir

4. ✅ (04/11) **Fluxo em lote recarrega as fontes do sistema a cada PDF.** Agora `OnceLock`.
   `featherpdf-io/src/fontes.rs` `embutir_fontes_do_sistema` cria `fontdb::Database` e
   varre o sistema em toda chamada. Correção: `OnceLock<fontdb::Database>`.
5. ✅ (já corrigido, conferido em 04/11: roda em `spawn_blocking`) **Script trava a janela em PDF grande.** `featherpdf-pelicano/src/scripts.rs` `Sc::Rodar`
   roda na thread da janela e grava o documento inteiro duas vezes só para saber se mudou.
   Correção: o hospedeiro marca `mudou` nas operações que escrevem.
6. ✅ (10/11, os dois passam) **Testes novos da GUI nunca rodaram:** `edicao.rs` (`arrasto_gruda_na_guia_e_na_grade`)
   e `scripts.rs` (`script_escreve_le_e_erro_nao_mexe`). Rodar
   `cargo test -p featherpdf-pelicano` uma vez (com o teto de memória).

### Seria bom

7. ✅ (10/11) **Fonte sem `/Encoding` muda o apóstrofo.** Type1 sem /Encoding ganha /Differences com quoteright/quoteleft em 0x27/0x60 e a largura certa. `featherpdf-core/src/fontes.rs`: vira
   WinAnsi, e 0x27/0x60 (’ ‘ no padrão Type1) viram ' e `. Recusar essas fontes ou pôr
   `shortcut:` dizendo o limite.
8. ✅ (04/11) **Cada equação cria três fontes novas.** Reaproveita a fonte igual que já está no arquivo. `featherpdf-core/src/latex.rs`
   `inserir_equacao`: reaproveitar a fonte com o mesmo `BaseFont` já nos recursos.
9. ✅ (04/11) **Réguas roubam o clique na beirada.** `featherpdf-pelicano/src/edicao.rs`: só começar a guia
   se `self.achar(p)` não achar objeto.
10. ✅ (10/11) Menores: vírgula sobrando num `format!` do `latex.rs`; o bloco "Comprimir" do fluxo
    grava o documento uma vez a mais só para o relatório de tamanho. (Agora o "de X" só aparece quando um bloco anterior já gravou.)

### Não auditado

Assinatura digital (PAdES), criptografia de senha, PDF/A e OCR foram lidos por cima — são
onde um erro custa mais. Nada medido com PDFs reais do usuário.

## Leva dos 29 itens "planejado" (19/10/2026)

Feitos (menor versão que funciona, com teste no motor): Linearizar, Versões, Dividir
colunas, Frente e verso (capítulo na ímpar), Bates, Regras e validação, Campos calculados,
Ordem do Tab, Ordem dos objetos (sem agrupar/travar), Modo escuro, Comparar texto, Sobrepor
revisões, Visibilidade condicional, OCR por região, Tabelas do scan, Gráfico para dados,
Limpar foto de caderno, PDF para apresentação (.pptx), Reconhecer assinaturas (sem comparar
com referência), Fontes (embutir/trocar; sem subconjunto), Tipografia (tamanho, cor, letras,
palavras; sem entrelinha/alinhamento), Grade e réguas, Substituir com refluxo, Tabelas de
verdade (linha no meio/fim, coluna no fim), Fluxos em blocos (sem OCR/tabela/assinar no
fluxo), Scripts (feather-macros), Equações em LaTeX (só digitar → desenho).

Faltam: **Mesa infinita**. (**Resumir e traduzir com IA (MCP)** saiu em 10/11: decisão do usuário, nada de internet nem IA no FeatherPDF.) Depois, atualizar o
artifact de status.
