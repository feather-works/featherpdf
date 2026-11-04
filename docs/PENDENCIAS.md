# Pendências do FeatherPDF

## Auditoria de 19/10/2026 (código novo da leva dos 29 itens "planejado")

Nada disto foi conferido rodando a GUI. "Copiar como LaTeX" foi retirado a pedido do
usuário (19/10), e com ele o achado sobre `\alphax`.

### Precisa corrigir

1. ✅ (04/11) **"Limpar caderno" estraga fotos e logotipos.** Agora só limpa imagem grande e quase toda papel (`parece_papel`). `featherpdf-io/src/lib.rs`
   `limpar_caderno_jpeg` (chamado por `Msg::LimparCaderno` e pelo bloco do fluxo). O
   `limpar_fundo` passa *toda* imagem pelo filtro; o `clarear_fundo_jpeg` testa antes se a
   imagem parece papel (`niveis_do_papel`), o caderno não. Correção: a mesma checagem no
   começo, ou só imagem que cobre a página inteira.
2. **Refluxo invade a coluna ao lado.** `featherpdf-core/src/refluxo.rs` (margem com uma
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
6. **Testes novos da GUI nunca rodaram:** `edicao.rs` (`arrasto_gruda_na_guia_e_na_grade`)
   e `scripts.rs` (`script_escreve_le_e_erro_nao_mexe`). Rodar
   `cargo test -p featherpdf-pelicano` uma vez (com o teto de memória).

### Seria bom

7. **Fonte sem `/Encoding` muda o apóstrofo.** `featherpdf-core/src/fontes.rs`: vira
   WinAnsi, e 0x27/0x60 (’ ‘ no padrão Type1) viram ' e `. Recusar essas fontes ou pôr
   `shortcut:` dizendo o limite.
8. **Cada equação cria três fontes novas.** `featherpdf-core/src/latex.rs`
   `inserir_equacao`: reaproveitar a fonte com o mesmo `BaseFont` já nos recursos.
9. **Réguas roubam o clique na beirada.** `featherpdf-pelicano/src/edicao.rs`: só começar a guia
   se `self.achar(p)` não achar objeto.
10. Menores: vírgula sobrando num `format!` do `latex.rs`; o bloco "Comprimir" do fluxo
    grava o documento uma vez a mais só para o relatório de tamanho.

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

Faltam: **Mesa infinita** e **Resumir e traduzir com IA (MCP)**. Depois, atualizar o
artifact de status.
