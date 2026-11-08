# Paridade do FeatherPDF com o Adobe Acrobat Pro

**Data:** 19/10/2026 (revisão completa em 08/11/2026)

> Em 08/11/2026 a lista inteira foi conferida de novo no código: cada item ❌ e 🟡 foi procurado no motor e na tela (botão, menu ou janela). Só conta como ✅ o que dá para fazer no app. Entraram os recursos que estavam marcados errado como ❌ (comparar texto, fluxos em lote, scripts, Bates, marca d'água, imagens para PDF, Sanitizar, Dividir e Extrair, leitor padrão de PDF e outros).
**Base:** lista de funcionalidades do Acrobat (26 seções, 723 itens).

## Resumo

Total: **723** itens. ✅ feito: **272** · 🟡 parcial: **71** · ❌ falta: **309** · ⛔ fora do escopo (serviço da Adobe): **71**.

Ver a contagem por seção abaixo. Os itens ⛔ dependem de nuvem ou de serviço da Adobe (Acrobat Sign remoto, PDF Spaces, Adobe Express, compartilhamento em nuvem) e ficam de fora da comparação de código.

| Seção | Itens | ✅ feito | 🟡 parcial | ❌ falta | ⛔ Adobe |
|---|---|---|---|---|---|
| 1. Visualização e leitura de PDF | 35 | 22 | 5 | 8 | 0 |
| 2. Edição de texto | 24 | 11 | 4 | 9 | 0 |
| 3. Edição de imagens | 18 | 10 | 2 | 6 | 0 |
| 4. Organização de páginas | 27 | 21 | 1 | 5 | 0 |
| 5. Criação de PDF | 22 | 5 | 4 | 13 | 0 |
| 6. Conversão e exportação | 33 | 17 | 3 | 13 | 0 |
| 7. Compressão e otimização de PDF | 41 | 20 | 1 | 20 | 0 |
| 8. OCR e digitalização | 22 | 11 | 4 | 7 | 0 |
| 9. Comentários e anotações | 40 | 18 | 11 | 11 | 0 |
| 10. Formulários PDF | 47 | 22 | 5 | 16 | 4 |
| 11. Assinaturas eletrônicas e digitais | 36 | 14 | 5 | 6 | 11 |
| 12. Segurança e proteção de documentos | 33 | 18 | 5 | 10 | 0 |
| 13. Hiperlinks, marcadores e navegação | 21 | 4 | 1 | 16 | 0 |
| 14. Camadas PDF | 18 | 7 | 0 | 11 | 0 |
| 15. Cabeçalhos, rodapés, marcas d'água e numeração | 33 | 15 | 2 | 16 | 0 |
| 16. Acessibilidade | 25 | 0 | 3 | 22 | 0 |
| 17. Impressão | 25 | 15 | 0 | 10 | 0 |
| 18. Produção gráfica profissional | 41 | 11 | 4 | 26 | 0 |
| 19. Comparação e análise de documentos | 14 | 6 | 4 | 4 | 0 |
| 20. Automação, processamento em lote e JavaScript | 32 | 10 | 2 | 20 | 0 |
| 21. Pesquisa avançada | 18 | 2 | 0 | 16 | 0 |
| 22. Recursos de engenharia, medições e PDF 3D | 28 | 8 | 1 | 19 | 0 |
| 23. Multimídia e documentos interativos | 18 | 0 | 2 | 16 | 0 |
| 24. Compartilhamento e colaboração | 15 | 0 | 0 | 0 | 15 |
| 25. Inteligência artificial — Acrobat AI Assistant e Acrobat Studio | 40 | 0 | 0 | 0 | 40 |
| 26. Preferências e personalização | 17 | 5 | 2 | 9 | 1 |
| **Total** | **723** | **272** | **71** | **309** | **71** |

## Próximos lotes sugeridos

Os 15 itens ❌ ou 🟡 de maior valor para quem usa PDF no dia a dia, agrupados em 5 lotes.

Critério: uso diário de PDF, sem os itens exóticos (3D, geoespacial, PDF/X, PDF/E, trapping). Cada lote fica num entregável. Os itens entram uma vez só.

**Lote 1: ver e imprimir**
1. Visualizar propriedades do documento (seção 1): ❌. Não há tela com título, autor, versão do PDF e tamanho; o inspetor (F12) só mostra os objetos crus.
2. Rotacionar temporariamente a visualização (seção 1): ❌. Para ver uma página deitada é preciso girar a página de verdade.
3. Configurar tamanho de papel (seção 17): ❌. A impressão usa o papel padrão da impressora, sem escolher A4, Carta ou Ofício.

**Lote 2: páginas e arquivos**
4. Copiar e colar páginas (seção 4): ❌. Sem área de transferência de páginas, então não há como levar página de um arquivo para outro.
5. Alterar numeração lógica das páginas (seção 4): ❌. O PageLabels só é apagado, não dá para dizer que a página 1 é a "i".
6. TIFF → PDF (seção 6): ❌. Imagens para PDF e Juntar só aceitam PNG e JPG; scanner costuma gravar TIFF.

**Lote 3: edição de texto e imagem**
7. Modificar fonte (seção 2): ❌. Tamanho, cor e espaço do texto existente já mudam; a fonte e o negrito não.
8. Rotacionar imagens (seção 3): ❌. Editar imagem troca, corta e redimensiona, mas não gira.
9. Extrair imagens do PDF (seção 3): ❌. Só dá para exportar a página inteira como imagem ou copiar uma área.

**Lote 4: formulários e comentários**
10. Campo obrigatório em formulário (seção 10): ❌. As regras têm formato e cálculo, mas nenhum campo é obrigatório.
11. Definir formato de datas em campo (seção 10): ❌. O campo de data tem calendário, mas sem formato configurável.
12. Editar comentários (seção 9): ❌. Dá para responder e apagar, mas o texto de uma nota não muda depois de criada.

**Lote 5: navegação, marcas e acessibilidade**
13. Criar e editar marcadores (seção 13): ❌. O painel Marcadores só lê o sumário que já veio no arquivo.
14. Editar e remover cabeçalho e rodapé (seção 15): ❌. Aplicar de novo soma ao anterior; só o Ctrl+Z tira.
15. Verificar acessibilidade (seção 16): ❌. Não há verificação nem tags; o inspetor só lista problemas de estrutura.

Os lotes 1 e 2 são os de maior uso no dia a dia.

## Observações


- Esta é uma leitura estática do código (grep e leitura de arquivos). Nenhum build, teste ou execução foi feito. Os itens ✅ podem ter bugs que só aparecem rodando.
- Caminhos das evidências, relativos a `crates/`: gui = `featherpdf-pelicano/src`, core = `featherpdf-core/src`, io = `featherpdf-io/src`, ocr = `featherpdf-ocr/src`.
- O `README.md` do repositório diz que o projeto não tem OCR, anotações, formulários nem redação. O código já tem esses recursos (seções 8, 9, 10 e 12). Vale atualizar o README.

## Detalhe por seção

## 1. Visualização e leitura de PDF

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Abrir arquivos PDF. | ✅ feito | io abrir (Ctrl+O) |
| Abrir vários PDFs simultaneamente. | ✅ feito | abas de documento (main.rs `guias`) |
| Visualização por abas. | ✅ feito | abas de documento (tela.rs) |
| Visualização de página única. | 🟡 parcial | "Uma página" é coluna contínua, sem modo página a página |
| Visualização contínua. | ✅ feito | rolagem contínua da mesa (documento.rs mesa_paginas) |
| Visualização de duas páginas. | ✅ feito | Disposicao::DuasPaginas (main.rs) |
| Visualização contínua de duas páginas. | 🟡 parcial | linhas de duas páginas já rolam, sem modo separado |
| Modo de tela cheia. | ✅ feito | F11 (Msg::TelaCheia) |
| Modo apresentação. | ❌ falta | não há no código |
| Ajustar página à janela. | ✅ feito | Msg::AjustarPagina |
| Ajustar à largura. | ✅ feito | Msg::AjustarLargura (main.rs) |
| Ajustar à altura. | ✅ feito | Msg::AjustarAltura (botão Altura) |
| Zoom personalizado. | ✅ feito | caixa do zoom aceita o percentual digitado (Msg::CampoZoom) |
| Zoom por seleção de área. | ❌ falta | não há no código |
| Navegação por número de página. | ✅ feito | campo de página (Msg::CampoPagina) |
| Navegação por miniaturas. | ✅ feito | painel Miniaturas (documento.rs) |
| Navegação por marcadores. | ✅ feito | painel Marcadores, clique leva à página (navegar.rs) |
| Painel de anexos. | ✅ feito | painel Anexos, salvar anexo (navegar.rs) |
| Painel de camadas. | ✅ feito | painel Camadas (comentar.rs) |
| Painel de comentários. | ✅ feito | painel Comentários (comentar.rs) |
| Rotacionar temporariamente a visualização. | ❌ falta | não há no código |
| Selecionar texto. | ✅ feito | ferramenta Selecionar, ordem de leitura (selecao.rs) |
| Selecionar imagens. | 🟡 parcial | imagens só como objeto na ferramenta Editar texto |
| Copiar texto. | ✅ feito | Ctrl+C e botão direito (selecao.rs) |
| Copiar imagens. | 🟡 parcial | Copiar área copia o trecho da página como imagem (regiao.rs), não a imagem original |
| Copiar uma região como imagem. | ✅ feito | regiao.rs "Copiar como imagem" |
| Ler documentos em voz alta. | ❌ falta | não há no código |
| Rolagem automática. | ❌ falta | não há no código |
| Visualização adaptada para leitura (Reflow). | ❌ falta | não há no código |
| Modo de leitura com recursos de acessibilidade. | 🟡 parcial | Leitura sem recursos de acessibilidade |
| Visualizar propriedades do documento. | ❌ falta | não há no código |
| Visualizar fontes utilizadas. | ✅ feito | tabela de fontes do inspetor F12 (inspetor.rs) |
| Visualizar informações de segurança. | ✅ feito | janela Senha mostra cifra e restrições (core `seguranca`) |
| Visualizar assinaturas digitais. | ✅ feito | painel Assinaturas valida (assinar.rs) |
| Visualizar metadados. | ❌ falta | não há no código |


## 2. Edição de texto

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Adicionar texto. | ✅ feito | inserir_texto (core inserir.rs) |
| Editar texto existente. | 🟡 parcial | edição mantém a fonte da linha, só com as letras da fonte (edicao.rs) |
| Excluir texto. | ✅ feito | excluir objeto (edicao.rs Obj::Excluir) |
| Substituir palavras. | ✅ feito | Ctrl+H, trocar regrava a linha (busca.rs) |
| Corrigir erros ortográficos. | ❌ falta | não há no código |
| Copiar e colar texto. | ✅ feito | Ctrl+C copia; Ctrl+V cola texto ou imagem como objeto novo (inserir.rs Ins::Colar) |
| Recortar texto. | ❌ falta | não há no código |
| Mover blocos de texto. | ✅ feito | arrastar objeto (edicao.rs Obj::Arrastou) |
| Redimensionar blocos de texto. | ❌ falta | não há no código |
| Modificar fonte. | ❌ falta | não há no código |
| Modificar tamanho da fonte. | ✅ feito | Tipografia no texto existente (core tipografia.rs `estilizar_texto`) |
| Modificar cor da fonte. | ✅ feito | EstiloTexto.cor no texto existente |
| Aplicar negrito. | 🟡 parcial | negrito só no texto novo (Ins::Negrito) |
| Aplicar itálico. | ❌ falta | não há no código |
| Aplicar sublinhado quando suportado pela formatação. | ❌ falta | não há no código |
| Modificar alinhamento. | ❌ falta | alinha objetos entre si, não o parágrafo |
| Ajustar espaçamento entre caracteres. | ✅ feito | Editar ▸ Texto ▸ Mais espaço / Menos espaço (core tipografia.rs) |
| Ajustar espaçamento entre linhas. | ❌ falta | não há no código |
| Modificar dimensões da caixa de texto. | ❌ falta | não há no código |
| Inserir caracteres especiais. | 🟡 parcial | Helvetica no texto novo, letras fora da fonte não entram (core inserir.rs) |
| Editar textos identificados por OCR. | ✅ feito | OCR troca a imagem por texto editável (ocr.rs) |
| Reorganizar texto dentro de blocos editáveis. | 🟡 parcial | Ctrl+H com “Refazer o parágrafo” refaz as linhas (core refluxo.rs); não dá para digitar dentro do bloco |
| Localizar palavras. | ✅ feito | Ctrl+F (busca.rs) |
| Substituir ocorrências de texto. | ✅ feito | Ctrl+H, trocar (busca.rs) |


## 3. Edição de imagens

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Inserir imagens. | ✅ feito | Imagem (inserir.rs inserir_imagem) |
| Remover imagens. | ✅ feito | excluir objeto de imagem (edicao.rs) |
| Substituir imagens existentes. | ✅ feito | Editar imagem ▸ Trocar por outra imagem (imagem.rs) |
| Redimensionar imagens. | ✅ feito | Editar imagem ▸ Tamanho, com proporção (imagem.rs); sem alça de arrastar |
| Mover imagens. | ✅ feito | arrastar e setas (edicao.rs) |
| Rotacionar imagens. | ❌ falta | não há no código |
| Recortar imagens. | ✅ feito | Editar imagem ▸ Cortar (imagem.rs) |
| Alinhar imagens. | ✅ feito | alinhar entre si e à página (inserir.rs Ins::Alinhar) |
| Ajustar a posição de objetos gráficos. | ✅ feito | mover e alinhar (edicao.rs) |
| Editar objetos rasterizados. | ✅ feito | core editar_imagem.rs (redimensionar, cortar, trocar, brilho e contraste) |
| Editar objetos vetoriais. | 🟡 parcial | desenho agrupado move e apaga (edicao.rs), sem editar o traço |
| Extrair imagens do PDF. | ❌ falta | não há no código |
| Copiar imagens para outros documentos. | 🟡 parcial | Copiar área + Ctrl+V cola noutro PDF como figura, não a imagem original |
| Abrir imagens em um editor externo compatível. | ❌ falta | não há no código |
| Atualizar imagens editadas externamente. | ❌ falta | não há no código |
| Alterar propriedades de objetos gráficos. | ✅ feito | core objetos.rs `estilizar_desenhos` em qualquer desenho |
| Alterar espaços de cor de objetos. | ❌ falta | não há no código |
| Alterar propriedades de renderização. | ❌ falta | não há no código |


## 4. Organização de páginas

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Inserir páginas. | ✅ feito | inserir_em_branco e inserir_pdf (paginas.rs) |
| Excluir páginas. | ✅ feito | excluir_paginas (paginas.rs) |
| Duplicar páginas. | ✅ feito | duplicar_paginas (paginas.rs) |
| Copiar páginas. | ❌ falta | não há no código |
| Recortar páginas. | ❌ falta | não há no código |
| Colar páginas. | ❌ falta | não há no código |
| Mover páginas. | ✅ feito | mover_paginas (paginas.rs, arrastar em organizar.rs) |
| Reordenar páginas. | ✅ feito | grade de miniaturas (organizar.rs) |
| Extrair páginas. | ✅ feito | extrair (paginas.rs) |
| Extrair intervalos de páginas. | ✅ feito | ler_intervalos (paginas.rs) |
| Salvar páginas como novos PDFs. | ✅ feito | Extrair na aba Páginas |
| Separar um PDF em vários documentos. | ✅ feito | Dividir (organizar.rs partes_divisao) |
| Dividir por quantidade de páginas. | ✅ feito | Dividir "A cada N páginas" |
| Dividir por tamanho de arquivo. | ✅ feito | Dividir ▸ Por tamanho de arquivo (MB) (core divisao.rs) |
| Dividir por marcadores de nível superior. | ✅ feito | Dividir ▸ Por marcador (capítulos do sumário) (core divisao.rs) |
| Mesclar vários PDFs. | ✅ feito | Juntar arquivos (organizar.rs Pag::Juntar) |
| Combinar arquivos de formatos diferentes em um PDF. | 🟡 parcial | Juntar aceita PDF e imagens (PNG, JPG), não Word nem Excel (juntar.rs) |
| Rotacionar páginas individualmente. | ✅ feito | girar_paginas (paginas.rs) |
| Rotacionar várias páginas. | ✅ feito | girar várias (paginas.rs) |
| Recortar páginas. | ✅ feito | cortar_margens (Caixas da página) |
| Alterar limites visíveis da página. | ✅ feito | cortar_margens define a área visível |
| Definir tamanhos das páginas. | ✅ feito | padronizar_tamanho (paginas.rs) |
| Alterar numeração lógica das páginas. | ❌ falta | PageLabels só é removido (paginas.rs:201) |
| Adicionar páginas em branco. | ✅ feito | inserir_em_branco (paginas.rs) |
| Inserir páginas provenientes de outros PDFs. | ✅ feito | inserir_pdf (paginas.rs) |
| Substituir páginas preservando determinadas informações de navegação. | ❌ falta | não há no código |
| Organizar páginas visualmente por miniaturas. | ✅ feito | miniaturas arrastáveis (organizar.rs) |


## 5. Criação de PDF

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Criar PDF em branco. | ✅ feito | criar_a4 e inserir_em_branco |
| Criar PDF a partir de documentos Word. | ❌ falta | não há no código |
| Criar PDF a partir de planilhas Excel. | ❌ falta | não há no código |
| Criar PDF a partir de apresentações PowerPoint. | ❌ falta | não há no código |
| Criar PDF a partir de imagens. | ✅ feito | Página inicial ▸ Imagens para PDF; arrastar PNG/JPG abre como PDF (juntar.rs) |
| Criar PDF a partir de arquivos de texto. | ❌ falta | não há no código |
| Criar PDF a partir de páginas HTML. | ❌ falta | não há no código |
| Criar PDF a partir de páginas da internet. | ❌ falta | não há no código |
| Criar PDF a partir da área de transferência. | 🟡 parcial | Novo PDF (Ctrl+N) e Ctrl+V cola a imagem ou o texto; sem comando direto |
| Criar PDF utilizando scanner. | ❌ falta | não há no código |
| Criar PDF a partir de fotografias. | ✅ feito | Imagens para PDF (uma foto por página) e Limpar caderno (core caderno.rs) |
| Criar PDF usando impressão virtual. | ❌ falta | não há no código |
| Converter múltiplos documentos em lote. | 🟡 parcial | Fluxos em lote junta pastas de fotos em PDF; Word e Excel não entram |
| Combinar arquivos durante a criação. | ✅ feito | Imagens para PDF e Juntar arquivos (PDF e imagens num só) |
| Criar documentos PDF com marcadores. | ❌ falta | não há no código |
| Criar documentos PDF com links. | 🟡 parcial | Novo PDF + Editar ▸ Link; a conversão de outros formatos não existe |
| Criar documentos PDF com formulários. | ✅ feito | modelos de formulário A4 (core modelos.rs) |
| Criar documentos PDF com anexos. | ❌ falta | não há no código |
| Criar PDFs com configurações personalizadas de compressão. | ❌ falta | não há no código |
| Configurar qualidade de imagens durante a criação. | ❌ falta | não há no código |
| Configurar incorporação de fontes. | ❌ falta | não há no código |
| Criar documentos em formatos PDF padronizados. | 🟡 parcial | Converter ▸ PDF/A, só PDF/A-2b (core pdfa.rs, exportar.rs) |


## 6. Conversão e exportação

| Item | Estado | Evidência / o que falta |
|---|---|---|
| PDF → DOCX. | ✅ feito | Word (.docx) via gravar_docx (io) |
| PDF → DOC. | ❌ falta | não há no código |
| PDF → XLSX. | ✅ feito | ModoPlanilha::Pagina (página inteira em grade) ou só tabelas |
| PDF → PPTX. | 🟡 parcial | Apresentação (.pptx): cada página vira um slide como imagem, sem texto editável (io pptx.rs) |
| PDF → RTF. | ❌ falta | não há no código |
| PDF → TXT. | ✅ feito | Texto puro (exportar.rs) |
| PDF → HTML. | ✅ feito | Texto puro e HTML (exportar.rs) |
| PDF → XML. | ❌ falta | não há no código |
| PDF → JPEG. | ✅ feito | Imagens JPG (exportar.rs TipoImagem) |
| PDF → PNG. | ✅ feito | Imagens PNG (exportar.rs) |
| PDF → TIFF. | ✅ feito | Imagens TIFF (io gravar_tiff) |
| PDF → outros formatos de imagem compatíveis. | ✅ feito | PNG, JPG, WebP, BMP, GIF, TIFF e SVG |
| PDF → texto acessível. | ❌ falta | texto acessível com tags não é exportado |
| PDF → imagens individuais por página. | ✅ feito | uma imagem por página (exportar.rs) |
| DOCX → PDF. | ❌ falta | não há no código |
| XLSX → PDF. | ❌ falta | não há no código |
| PPTX → PDF. | ❌ falta | não há no código |
| TXT → PDF. | ❌ falta | não há no código |
| RTF → PDF. | ❌ falta | não há no código |
| HTML → PDF. | ❌ falta | não há no código |
| JPEG → PDF. | ✅ feito | Imagens para PDF; arrastar JPG abre como PDF (juntar.rs) |
| PNG → PDF. | ✅ feito | Imagens para PDF; arrastar PNG abre como PDF (juntar.rs) |
| TIFF → PDF. | ❌ falta | não há no código |
| Documentos digitalizados → PDF. | ✅ feito | scan em PNG/JPG vira PDF (Imagens para PDF) e o OCR torna pesquisável |
| Converter vários arquivos em lote. | ✅ feito | Fluxos em lote: saída PDF, Word, planilha ou PNG (fluxos.rs Modo) |
| Preservar imagens na conversão. | ✅ feito | imagens levadas ao docx (io lib.rs) |
| Preservar tabelas quando possível. | 🟡 parcial | tabelas (tabelas.rs, tabela.rs) |
| Preservar fontes e formatação. | 🟡 parcial | negrito, tamanho e centro (fluxo.rs) |
| Preservar hiperlinks. | ❌ falta | não há no código |
| Configurar resolução das imagens exportadas. | ✅ feito | resolução (Dpi em exportar.rs) |
| Selecionar páginas para exportação. | ✅ feito | Todas, Só a atual ou Escolher (1-3, 5) |
| Exportar apenas determinadas imagens. | ❌ falta | não há no código |
| Converter imagens digitalizadas em documentos editáveis. | ✅ feito | OCR troca a imagem por texto; tabelas por OCR |


## 7. Compressão e otimização de PDF

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Compactação JPEG. | ✅ feito | reamostrar_jpeg e qualidade (otimizar.rs) |
| Compactação JPEG2000. | ❌ falta | não há no código |
| Compactação ZIP/Flate. | ✅ feito | zopfli e compressão de streams (otimizar.rs) |
| Compactação CCITT Group 4. | ❌ falta | não há no código |
| Compactação JBIG2 sem perdas. | ❌ falta | não há no código |
| Compactação JBIG2 com perdas. | ❌ falta | não há no código |
| Redução de resolução (*downsampling*). | ✅ feito | reamostrar por DPI (otimizar.rs) |
| Configurar DPI. | ✅ feito | DPI (Comprimir, proteger.rs) |
| Compressão separada para imagens coloridas. | ❌ falta | não há no código |
| Compressão separada para imagens em tons de cinza. | ❌ falta | não há no código |
| Compressão separada para imagens monocromáticas. | ❌ falta | não há no código |
| Configurar qualidade de compressão. | ✅ feito | qualidade 1-100 (otimizar.rs) |
| Selecionar métodos de amostragem. | ❌ falta | não há no código |
| Remover fontes incorporadas quando permitido e apropriado. | ❌ falta | não há no código |
| Criar subconjuntos de fontes. | ❌ falta | não há no código |
| Otimizar informações de fontes. | ❌ falta | não há no código |
| Remover objetos desnecessários. | 🟡 parcial | limpar cobre miniaturas, PieceInfo e streams vazios, não outros objetos |
| Remover miniaturas incorporadas. | ✅ feito | miniaturas (otimizar.rs limpar) |
| Remover índices de pesquisa. | ❌ falta | não há no código |
| Remover objetos não referenciados. | ✅ feito | prune_objects em serializar (lib.rs) |
| Descartar destinos nomeados sem uso. | ❌ falta | não há no código |
| Remover links inválidos. | ❌ falta | não há no código |
| Remover marcadores inválidos. | ❌ falta | não há no código |
| Otimizar fluxos de conteúdo. | ✅ feito | zopfli (otimizar.rs) |
| Comprimir fluxos não compactados. | ✅ feito | streams não compactados (otimizar.rs) |
| Substituir compressão LZW por Flate quando apropriado. | ❌ falta | não há no código |
| Otimizar representações internas dos objetos PDF. | ✅ feito | object streams e xref streams (otimizar.rs) |
| Achatar transparências. | ❌ falta | não há no código |
| Achatar campos de formulário. | ✅ feito | Comentar ▸ Achatar (core achatamento.rs) |
| Achatar camadas. | ❌ falta | não há no código |
| Remover comentários. | ✅ feito | Limpar e sanitizar ▸ Comentários (core sanitizar.rs) |
| Remover anexos. | ✅ feito | Remover anexos (proteger.rs) |
| Remover metadados. | ✅ feito | limpar_metadados (documento.rs) |
| Remover dados privados de aplicativos. | ✅ feito | PieceInfo em limpar (otimizar.rs) |
| Remover conteúdo de camadas ocultas. | ✅ feito | Limpar e sanitizar ▸ Camadas ocultas (core sanitizar.rs) |
| Remover multimídia. | ❌ falta | não há no código |
| Otimizar para visualização rápida na web (*Fast Web View*). | ✅ feito | Converter ▸ Para a web (core linearizar.rs) |
| Processar vários PDFs em lote. | ✅ feito | Fluxos em lote, modelo “Comprimir vários PDFs” (fluxos.rs) |
| Analisar o espaço utilizado por cada categoria de objeto. | ✅ feito | peso por imagens, fontes e resto (otimizar.rs peso) |
| Exibir relatório de uso de espaço. | ✅ feito | "Onde está o peso" no Comprimir (imagens, fontes, resto) |
| Salvar perfis de otimização. | ❌ falta | não há no código |


## 8. OCR e digitalização

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Digitalizar documentos. | ❌ falta | não há no código |
| Importar documentos digitalizados. | ✅ feito | Imagens para PDF e arrastar PNG/JPG para a janela (juntar.rs) |
| Reconhecer texto em imagens. | ✅ feito | PP-OCR embutido (featherpdf-ocr) |
| Reconhecer texto em páginas PDF. | ✅ feito | lê páginas do PDF (ocr.rs) |
| OCR de múltiplas páginas. | ✅ feito | Todas, só a atual ou só as sem texto (ocr.rs) |
| OCR de intervalos selecionados. | ✅ feito | páginas escolhidas (ocr.rs) |
| Selecionar idioma do OCR. | ✅ feito | Idioma (ocr.rs, ocr_modelos.rs) |
| Criar PDF pesquisável. | ✅ feito | camada de texto invisível (ocr.rs, inserir_camada_ocr) |
| Criar PDF com texto reconhecido sobre a imagem original. | ✅ feito | texto por trás da imagem original (ocr.rs) |
| Criar PDF editável após reconhecimento. | 🟡 parcial | texto por linha (Editar texto), não por parágrafo |
| Corrigir problemas de reconhecimento. | ✅ feito | conferir e corrigir antes de aplicar (ocr.rs) |
| Melhorar nitidez do texto. | ❌ falta | não há no código |
| Remover ruídos de digitalização. | ❌ falta | não há no código |
| Corrigir inclinação das páginas (*deskew*). | 🟡 parcial | Endireitar a página pelo classificador de orientação, sem deskew fino |
| Remover padrões de retícula (*descreen*). | ❌ falta | não há no código |
| Remover ou suavizar fundos irregulares. | 🟡 parcial | Limpar o fundo (ocr.rs, core tarja.rs) |
| Melhorar fotografias de documentos. | ✅ feito | Limpar caderno tira sombra, pauta e quadriculado da foto (core caderno.rs) |
| Corrigir perspectiva de imagens capturadas por câmera. | ❌ falta | não há no código |
| Ajustar qualidade da digitalização. | ❌ falta | não há no código |
| Configurar compressão de páginas digitalizadas. | 🟡 parcial | bloco Comprimir (Leve, Padrão, Forte) no fluxo, sem opção própria do scan |
| Configurar resolução do documento digitalizado. | ❌ falta | não há no código |
| Criar PDF/A durante o fluxo de digitalização. | ✅ feito | Fluxos em lote: blocos OCR e Converter para PDF/A (modelo “Arquivo de contratos”) |


## 9. Comentários e anotações

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Adicionar comentários. | ✅ feito | Nota (comentar.rs, anotacoes.rs) |
| Adicionar notas adesivas. | ✅ feito | nota adesiva (anotacoes.rs) |
| Destacar texto. | ✅ feito | Destacar (comentar.rs) |
| Sublinhar texto. | ✅ feito | Sublinhar (comentar.rs) |
| Riscar texto. | ✅ feito | Riscar (comentar.rs) |
| Marcar texto para substituição. | ✅ feito | Comentar ▸ Substituir (core revisao.rs, riscado + Caret) |
| Marcar texto para exclusão. | ✅ feito | Comentar ▸ Riscar (comentar.rs) |
| Inserir sugestões de texto. | 🟡 parcial | Substituir grava o riscado com o texto novo; sem inserir sem riscar |
| Adicionar caixas de texto. | ✅ feito | Caixa de texto (comentar.rs) |
| Adicionar balões explicativos. | ❌ falta | não há no código |
| Desenhar livremente. | ✅ feito | desenho à mão (comentar.rs, anotacoes.rs) |
| Utilizar lápis. | 🟡 parcial | desenho à mão com espessura, sem lápis separado |
| Utilizar borracha nas marcações. | ❌ falta | não há no código |
| Desenhar linhas. | 🟡 parcial | linha no catálogo de formas, não como anotação de revisão (inserir.rs) |
| Desenhar setas. | 🟡 parcial | seta no catálogo de formas |
| Desenhar retângulos. | 🟡 parcial | retângulo no catálogo de formas |
| Desenhar círculos. | 🟡 parcial | círculo no catálogo de formas |
| Desenhar polígonos. | 🟡 parcial | polígono no catálogo de formas |
| Desenhar linhas poligonais. | 🟡 parcial | polilinha no catálogo de formas |
| Desenhar nuvens de revisão. | 🟡 parcial | nuvem de revisão na Medir (medir.rs gravar_nuvem) |
| Alterar cores das anotações. | 🟡 parcial | cor escolhida antes de desenhar |
| Alterar espessura das linhas. | 🟡 parcial | espessura escolhida antes de desenhar |
| Alterar opacidade. | ❌ falta | não há no código |
| Adicionar carimbos. | ✅ feito | Carimbo (comentar.rs, cor_do_carimbo) |
| Criar carimbos personalizados. | ✅ feito | Carimbo: texto próprio ou imagem PNG/JPG (comentar.rs) |
| Criar carimbos dinâmicos. | ✅ feito | Carimbo com linha de baixo “Nome e data” ou “Só a data” (comentar.rs) |
| Inserir anexos como comentários. | ❌ falta | não há no código |
| Adicionar comentários a vídeos. | ❌ falta | não há no código |
| Responder comentários. | ✅ feito | painel Comentários ▸ Responder (/IRT, core revisao.rs) |
| Editar comentários. | ❌ falta | não há no código |
| Excluir comentários. | ✅ feito | apagar_anotacao (anotacoes.rs) |
| Agrupar comentários. | ❌ falta | não há no código |
| Desagrupar comentários. | ❌ falta | não há no código |
| Filtrar comentários. | ✅ feito | painel Comentários: filtro por texto, autor ou tipo (revisao.rs) |
| Pesquisar comentários. | ✅ feito | filtro do painel Comentários procura no texto (revisao.rs) |
| Marcar comentários como resolvidos. | ✅ feito | estado de revisão aceito, rejeitado ou concluído (core revisao.rs) |
| Marcar comentários como não lidos. | ❌ falta | não há no código |
| Reagir a comentários. | ❌ falta | não há no código |
| Exportar informações de revisão. | ✅ feito | painel Comentários ▸ CSV ou Para o FeatherSheet (revisao.rs) |
| Imprimir resumo dos comentários. | ❌ falta | não há no código |


## 10. Formulários PDF

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Criar formulários do zero. | ✅ feito | criar campo novo arrastando (formularios.rs) |
| Converter documentos existentes em formulários. | ✅ feito | core formularios.rs `detectar_campos` + `criar_campo` |
| Reconhecer automaticamente campos. | ✅ feito | acha ____, traços e quadradinhos com rótulo |
| Adicionar campos de texto. | ✅ feito | TipoCampo::Texto (core formularios.rs) |
| Adicionar campos numéricos. | ✅ feito | core regras.rs Regra::Numero e Moeda |
| Adicionar campos de data. | ✅ feito | campo de data com calendário (formularios.rs) |
| Adicionar caixas de seleção. | ✅ feito | TipoCampo::Caixa |
| Adicionar botões de opção (radio). | ✅ feito | TipoCampo::Opcao |
| Adicionar listas suspensas. | ✅ feito | CampoNovo::Lista com FF_COMBO |
| Adicionar caixas de lista. | 🟡 parcial | TipoCampo::Lista, sem caixa de lista separada |
| Adicionar botões de ação. | 🟡 parcial | TipoCampo::Botao existe, sem ação |
| Adicionar campos de assinatura. | ✅ feito | Novo::Assinatura (/FT /Sig) |
| Adicionar campos de código de barras. | ❌ falta | não há no código |
| Copiar campos. | ❌ falta | não há no código |
| Mover campos. | ❌ falta | não há no código |
| Redimensionar campos. | ❌ falta | não há no código |
| Alinhar campos. | ❌ falta | não há no código |
| Definir ordem de navegação entre campos. | ✅ feito | Regras e Tab ▸ ordem do Tab (core regras.rs) |
| Definir campos obrigatórios. | ❌ falta | não há no código |
| Definir campos somente leitura. | ❌ falta | não há no código |
| Configurar valores padrão. | ❌ falta | não há no código |
| Definir formato de números. | ✅ feito | Regras e Tab ▸ Número (core regras.rs) |
| Definir formato de datas. | ❌ falta | não há no código |
| Definir formato de moedas. | ✅ feito | Regras e Tab ▸ Moeda (core regras.rs) |
| Definir formatos personalizados. | 🟡 parcial | só formatos prontos (CPF, CNPJ, CEP, telefone, e-mail), sem formato livre |
| Aplicar máscaras de entrada. | ✅ feito | máscara de CPF, CNPJ, CEP e telefone (core regras.rs) |
| Validar informações. | ✅ feito | Validar e calcular, com dígito verificador (core regras.rs) |
| Configurar cálculos automáticos. | ✅ feito | soma, média, produto, menor, maior e fórmula (core regras.rs) |
| Configurar scripts JavaScript. | ❌ falta | não há no código |
| Exibir e ocultar campos. | ❌ falta | não há no código |
| Configurar eventos dos campos. | ❌ falta | não há no código |
| Definir aparência dos controles. | ❌ falta | não há no código |
| Configurar botões para limpar formulário. | ✅ feito | Limpar (formularios.rs, limpar_campos) |
| Configurar botões para enviar formulário. | ❌ falta | não há no código |
| Configurar ações de abertura de arquivos ou links. | ❌ falta | não há no código |
| Preencher formulários interativos. | ✅ feito | preencher na própria página (formularios.rs, preencher) |
| Preencher formulários não interativos. | ✅ feito | detectar e criar campo, ou texto livre |
| Preenchimento automático de informações. | ❌ falta | não há no código |
| Salvar informações preenchidas. | ✅ feito | Salvar |
| Imprimir formulários. | 🟡 parcial | Imprimir (Ctrl+P) sai com a aparência gravada dos campos; sem opção “só os campos” |
| Distribuir formulários. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Coletar respostas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Acompanhar formulários distribuídos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Consolidar respostas. | 🟡 parcial | Pasta de fichas, uma linha por arquivo (formularios.rs ler_pasta), sem painel de respostas |
| Exportar dados de formulários. | ✅ feito | Para o FeatherSheet (formularios.rs para_sheet) |
| Importar dados de formulários. | ✅ feito | Importar dados JSON, XFDF ou CSV (core dados_form.rs, form_dados.rs) |
| Publicar formulários na web por meio dos serviços compatíveis. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |


## 11. Assinaturas eletrônicas e digitais

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Adicionar assinatura desenhada. | ✅ feito | rubrica.rs (desenhada, digitada, tirada de imagem, guardadas) |
| Adicionar assinatura digitada. | ✅ feito | rubrica.rs (desenhada, digitada, tirada de imagem, guardadas) |
| Adicionar assinatura por imagem. | ✅ feito | rubrica.rs (desenhada, digitada, tirada de imagem, guardadas) |
| Salvar assinaturas para reutilização. | ✅ feito | rubrica.rs (desenhada, digitada, tirada de imagem, guardadas) |
| Posicionar assinaturas. | ✅ feito | posicionar ao clicar (inserir.rs) |
| Redimensionar assinaturas. | ✅ feito | assinatura é imagem: painel Imagem muda o tamanho |
| Preencher documentos antes da assinatura. | ✅ feito | Preencher e assinar (faixa Formulários) |
| Adicionar iniciais. | 🟡 parcial | sem iniciais próprias, a assinatura serve |
| Assinar documentos com certificado digital. | ✅ feito | PAdES com A1 .pfx/.p12 (core assinatura.rs, assinar.rs) |
| Utilizar identificações digitais. | 🟡 parcial | só certificado A1 de arquivo, sem gerenciador |
| Criar identificações digitais autoassinadas. | ❌ falta | não há no código |
| Importar certificados. | 🟡 parcial | importa um .pfx pela janela (assinar.rs) |
| Gerenciar certificados. | ❌ falta | não há no código |
| Verificar certificados. | 🟡 parcial | titular, emissor e validade (core assinatura.rs Certificado) |
| Adicionar carimbo de tempo. | ❌ falta | não há no código |
| Validar assinaturas. | ✅ feito | verificar_assinaturas (core assinatura.rs, Validar) |
| Verificar integridade do documento. | ✅ feito | conferência de integridade (core assinatura.rs) |
| Certificar documentos. | ❌ falta | não há certificação (DocMDP) |
| Criar assinaturas digitais visíveis. | ✅ feito | assinatura visível (assinar.rs Visivel) |
| Criar assinaturas digitais invisíveis. | ✅ feito | assinatura invisível (desligar o visível) |
| Permitir múltiplas assinaturas digitais. | ✅ feito | teste com duas assinaturas válidas (core assinatura.rs) |
| Configurar permissões após certificação. | ❌ falta | não há no código |
| Inspecionar alterações posteriores à assinatura. | ✅ feito | `cobre_tudo` e painel Versões |
| Comparar versões assinadas. | 🟡 parcial | Versões abre a versão assinada; salvar e usar Comparar, em dois passos |
| Incorporar informações de validação de longo prazo. | ❌ falta | não há no código |
| Solicitar assinaturas de outras pessoas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Enviar documentos para assinatura. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Definir destinatários. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Definir ordem de assinatura. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Enviar solicitações em massa. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Acompanhar documentos enviados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Acompanhar documentos assinados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerenciar contratos pendentes. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Visualizar registros de auditoria. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Receber notificações do processo. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerenciar acordos e fluxos de assinatura. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |


## 12. Segurança e proteção de documentos

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Proteger PDF com senha de abertura. | ✅ feito | senha de abertura (otimizar.rs senha_abrir) |
| Proteger permissões com senha. | ✅ feito | senha de permissões (otimizar.rs limitar) |
| Alterar senhas. | 🟡 parcial | grava com nova senha, sem troca separada |
| Remover proteção mediante autorização. | ✅ feito | Proteger ▸ Senha ▸ Tirar a senha, com o PDF aberto pela senha (proteger.rs) |
| Criptografar documentos. | ✅ feito | AES-256 (otimizar.rs aes256) |
| Criptografar com certificados digitais. | ❌ falta | não há no código |
| Restringir impressão. | ✅ feito | Permissoes::imprimir |
| Restringir modificações. | ✅ feito | Permissoes::editar |
| Restringir cópia de conteúdo. | ✅ feito | Permissoes::copiar |
| Restringir extração. | 🟡 parcial | extração cai na permissão de cópia de acessibilidade |
| Gerenciar permissões de acesso. | 🟡 parcial | cinco permissões em uma tela |
| Criar políticas de segurança. | ❌ falta | não há no código |
| Aplicar políticas de segurança existentes. | ❌ falta | não há no código |
| Ocultar informações para posterior remoção definitiva (*redaction*). | ✅ feito | Marcar área e Buscar e tarjar (tarja.rs) |
| Remover permanentemente textos confidenciais. | ✅ feito | apaga o texto debaixo da marca (core tarja.rs) |
| Remover permanentemente imagens confidenciais. | ✅ feito | apaga os pixels debaixo da marca (core tarja.rs ApagarPixels) |
| Procurar informações sensíveis. | ✅ feito | padrões CPF, CNPJ, e-mail, telefone, CEP (core tarja.rs Padrao) |
| Remover ocorrências de palavras específicas. | ✅ feito | busca de palavra (tarja.rs) |
| Remover padrões identificáveis. | ✅ feito | padrões (tarja.rs) |
| Configurar a aparência das tarjas de remoção. | 🟡 parcial | texto sobre a tarja (tarja.rs), sem escolher a cor |
| Utilizar códigos de classificação nas remoções. | 🟡 parcial | o texto sobre a tarja serve de código, sem lista de códigos |
| Remover informações ocultas. | ✅ feito | Limpar e sanitizar: metadados, histórico, anexos, JavaScript, camadas ocultas e links (sanitizar.rs) |
| Remover metadados confidenciais. | ✅ feito | limpar_metadados (documento.rs) |
| Remover comentários e anexos sensíveis. | ✅ feito | Sanitizar: anexos e, se marcado, comentários |
| Remover conteúdo de camadas ocultas. | ✅ feito | Limpar e sanitizar ▸ Camadas ocultas (core sanitizar.rs) |
| Sanitizar documentos antes do compartilhamento. | ✅ feito | Proteger ▸ Limpar e sanitizar, oito opções (sanitizar.rs) |
| Abrir documentos em modo protegido. | ❌ falta | não há no código |
| Isolar documentos não confiáveis. | ❌ falta | não há no código |
| Controlar execução de JavaScript. | ❌ falta | não há no código |
| Controlar abertura de links externos. | ❌ falta | não há no código |
| Controlar abertura de anexos. | ❌ falta | não há no código |
| Configurar locais confiáveis. | ❌ falta | não há no código |
| Gerenciar certificados confiáveis. | ❌ falta | não há no código |


## 13. Hiperlinks, marcadores e navegação

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Adicionar hiperlinks. | ✅ feito | inserir_link (core inserir.rs, Editar > Link) |
| Editar hiperlinks. | ❌ falta | não há no código |
| Excluir hiperlinks. | 🟡 parcial | Limpar e sanitizar ▸ Links externos tira todos; sem apagar um link só |
| Criar links para páginas internas. | ✅ feito | Destino::Pagina (core inserir.rs) |
| Criar links para páginas externas. | ✅ feito | Destino::Endereco (core inserir.rs) |
| Criar links para arquivos. | ❌ falta | não há no código |
| Criar links para endereços de email. | ✅ feito | e-mail digitado vira mailto: (core inserir.rs) |
| Modificar a aparência dos links. | ❌ falta | não há no código |
| Redimensionar áreas clicáveis. | ❌ falta | não há no código |
| Criar destinos nomeados. | ❌ falta | não há no código |
| Gerenciar destinos nomeados. | ❌ falta | não há no código |
| Criar marcadores (*bookmarks*). | ❌ falta | não há no código |
| Editar marcadores. | ❌ falta | não há no código |
| Excluir marcadores. | ❌ falta | não há no código |
| Criar marcadores hierárquicos. | ❌ falta | não há no código |
| Reorganizar marcadores. | ❌ falta | não há no código |
| Associar marcadores a páginas. | ❌ falta | não há no código |
| Associar marcadores a ações. | ❌ falta | não há no código |
| Configurar ampliação ao seguir marcadores. | ❌ falta | não há no código |
| Adicionar ações a elementos de navegação. | ❌ falta | não há no código |
| Criar navegação entre documentos. | ❌ falta | não há no código |


## 14. Camadas PDF

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Visualizar camadas existentes. | ✅ feito | camadas() e painel Camadas (core anotacoes.rs, comentar.rs) |
| Importar arquivos como camadas. | ❌ falta | não há no código |
| Criar uma camada a partir de um arquivo importado. | ❌ falta | não há no código |
| Renomear camadas. | ✅ feito | painel Camadas ▸ renomear (core camadas.rs) |
| Reorganizar camadas. | ❌ falta | não há no código |
| Mostrar camadas. | ✅ feito | mostrar_camada (anotacoes.rs) |
| Ocultar camadas. | ✅ feito | mostrar_camada, ocultar (anotacoes.rs) |
| Configurar visibilidade inicial. | ✅ feito | ligar ou desligar no painel Camadas grava como o PDF abre (comentar.rs) |
| Configurar visibilidade para impressão. | ✅ feito | painel Camadas: Só na tela ou Só na impressão (core camadas.rs) |
| Configurar visibilidade para exportação. | ❌ falta | não há no código |
| Alterar propriedades das camadas. | ✅ feito | painel Camadas: travar, quando aparece e faixa de zoom (core camadas.rs) |
| Gerenciar grupos de camadas. | ❌ falta | não há no código |
| Mesclar camadas. | ❌ falta | não há no código |
| Achatar camadas (*flatten*). | ❌ falta | não há no código |
| Vincular camadas a marcadores. | ❌ falta | não há no código |
| Vincular camadas a ações. | ❌ falta | não há no código |
| Importar conteúdo para camadas existentes. | ❌ falta | não há no código |
| Preservar camadas ao combinar PDFs compatíveis. | ❌ falta | não há no código |


## 15. Cabeçalhos, rodapés, marcas d'água e numeração

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Adicionar cabeçalho. | ✅ feito | Páginas ▸ Cabeçalho e rodapé, três lugares (marcas.rs) |
| Adicionar rodapé. | ✅ feito | Páginas ▸ Cabeçalho e rodapé, três lugares (marcas.rs) |
| Editar cabeçalho. | ❌ falta | não há no código |
| Editar rodapé. | ❌ falta | não há no código |
| Remover cabeçalhos. | ❌ falta | não há no código |
| Remover rodapés. | ❌ falta | não há no código |
| Inserir data. | ✅ feito | campo {data} no cabeçalho e rodapé (core marcas.rs) |
| Inserir números de página. | ✅ feito | Números de página (organizar.rs Dialogos::Numeros, paginas.rs numerar) |
| Aplicar cabeçalhos somente a páginas selecionadas. | ✅ feito | campo de páginas (1-3, 5) no painel (marcas.rs) |
| Aplicar configurações a vários documentos. | ✅ feito | Fluxos em lote, bloco Cabeçalho e rodapé |
| Adicionar marca d'água de texto. | ✅ feito | Páginas ▸ Marca d'água (core marcas.rs) |
| Adicionar marca d'água de imagem. | ❌ falta | não há no código |
| Ajustar opacidade. | ✅ feito | Marca d'água ▸ Opacidade (marcas.rs) |
| Ajustar tamanho. | ✅ feito | Marca d'água ▸ Tamanho (pt) (marcas.rs) |
| Ajustar rotação. | ✅ feito | Marca d'água ▸ ângulo (marcas.rs) |
| Configurar posicionamento. | ❌ falta | não há no código |
| Colocar marca d'água à frente do conteúdo. | ✅ feito | sempre por cima do conteúdo (core marcas.rs) |
| Colocar marca d'água atrás do conteúdo. | ❌ falta | não há no código |
| Atualizar marca d'água. | ❌ falta | não há no código |
| Remover marca d'água. | 🟡 parcial | na camada própria dá para esconder no painel Camadas; sem remover |
| Aplicar marcas d'água em lote. | ✅ feito | Fluxos em lote, bloco Marca d'água |
| Adicionar numeração Bates. | ✅ feito | Páginas ▸ Numeração Bates… (core paginas.rs numerar_bates) |
| Configurar numeração sequencial. | ✅ feito | Bates: primeiro número e algarismos (organizar.rs) |
| Configurar prefixos. | ✅ feito | Bates: prefixo (organizar.rs) |
| Configurar sufixos. | ❌ falta | não há no código |
| Aplicar numeração a múltiplos documentos. | 🟡 parcial | Bates diz onde o próximo arquivo começa; um arquivo de cada vez |
| Remover numeração Bates. | ❌ falta | não há no código |
| Pesquisar documentos numerados. | ❌ falta | não há no código |
| Adicionar fundo colorido. | ❌ falta | não há no código |
| Adicionar fundo de imagem. | ❌ falta | não há no código |
| Atualizar fundos. | ❌ falta | não há no código |
| Remover fundos. | ❌ falta | não há no código |
| Aplicar fundos a intervalos de páginas. | ❌ falta | não há no código |


## 16. Acessibilidade

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Verificar acessibilidade. | ❌ falta | não há no código |
| Gerar relatório de acessibilidade. | ❌ falta | não há no código |
| Identificar problemas estruturais. | 🟡 parcial | inspetor lista problemas (core inspecao.rs Problema), sem relatório de acessibilidade |
| Criar tags automaticamente. | ❌ falta | não há no código |
| Editar tags. | ❌ falta | não há no código |
| Definir ordem de leitura. | ❌ falta | não há no código |
| Corrigir ordem de leitura. | ❌ falta | não há no código |
| Marcar títulos e parágrafos. | ❌ falta | não há no código |
| Identificar listas. | ❌ falta | não há no código |
| Identificar tabelas. | 🟡 parcial | tabelas detectadas para converter (core tabelas.rs), sem marcar tabelas no PDF |
| Corrigir estruturas de tabelas. | ❌ falta | não há no código |
| Adicionar textos alternativos a imagens. | ❌ falta | não há no código |
| Configurar idioma do documento. | ❌ falta | não há no código |
| Configurar idioma de trechos específicos. | ❌ falta | não há no código |
| Criar links acessíveis. | ❌ falta | não há no código |
| Criar formulários acessíveis. | ❌ falta | não há no código |
| Adicionar descrições aos campos. | 🟡 parcial | campo tem dica /TU lida (formularios.rs), edição não confirmada |
| Definir ordem de navegação pelo teclado. | ❌ falta | não há no código |
| Suportar leitores de tela. | ❌ falta | não há no código |
| Oferecer leitura em voz alta. | ❌ falta | não há no código |
| Oferecer modo Reflow. | ❌ falta | não há no código |
| Exportar texto acessível. | ❌ falta | não há no código |
| Verificar conformidade PDF/UA. | ❌ falta | não há no código |
| Verificar requisitos de acessibilidade relacionados ao WCAG. | ❌ falta | não há no código |
| Corrigir problemas de estrutura lógica. | ❌ falta | não há no código |


## 17. Impressão

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Imprimir PDF. | ✅ feito | imprimir.rs (CUPS) e imprimir_windows.rs |
| Imprimir página atual. | ✅ feito | Paginas::Atual |
| Imprimir páginas selecionadas. | ✅ feito | Paginas::Intervalo |
| Imprimir intervalos. | ✅ feito | Paginas::Intervalo |
| Imprimir páginas pares. | ✅ feito | Paginas::Pares |
| Imprimir páginas ímpares. | ✅ feito | Paginas::Impares |
| Imprimir em ordem inversa. | ✅ feito | Imprimir ▸ Ordem inversa (imprimir.rs) |
| Configurar orientação. | ✅ feito | Orientacao (automática, retrato, paisagem) |
| Configurar tamanho de papel. | ❌ falta | não há no código |
| Configurar tamanhos personalizados. | ❌ falta | não há no código |
| Ajustar impressão à página. | ✅ feito | Imprimir ▸ Tamanho ▸ Ajustar à folha (imprimir.rs) |
| Imprimir no tamanho real. | ✅ feito | Imprimir ▸ Tamanho ▸ Tamanho real (imprimir.rs) |
| Configurar escala personalizada. | ❌ falta | não há no código |
| Imprimir em preto e branco. | ✅ feito | opção preto e branco do imprimir.rs |
| Imprimir em tons de cinza. | ✅ feito | Imprimir ▸ Cor ▸ Preto e branco (print-color-mode=monochrome) |
| Imprimir em cores. | ✅ feito | Imprimir ▸ Cor ▸ Colorido (imprimir.rs) |
| Imprimir em frente e verso. | ✅ feito | Lados (borda longa/curta) |
| Imprimir múltiplas páginas por folha. | ✅ feito | Imprimir ▸ Páginas por folha (2, 4, 6, 9) (imprimir.rs) |
| Imprimir livretos. | ❌ falta | não há no código |
| Imprimir pôsteres divididos em folhas. | ❌ falta | não há no código |
| Imprimir regiões selecionadas. | ❌ falta | não há no código |
| Imprimir comentários. | ❌ falta | não há no código |
| Imprimir resumo de comentários. | ❌ falta | não há no código |
| Imprimir camadas conforme suas propriedades. | ❌ falta | não há no código |
| Criar configurações predefinidas de impressão. | ❌ falta | não há no código |


## 18. Produção gráfica profissional

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Visualizar separações de cores. | ❌ falta | não há no código |
| Visualizar CMYK. | ❌ falta | não há no código |
| Visualizar cores especiais. | ❌ falta | não há no código |
| Gerenciar perfis ICC. | ❌ falta | não há no código |
| Converter espaços de cores. | ❌ falta | não há no código |
| Converter RGB para CMYK. | ❌ falta | não há no código |
| Visualizar sobreimpressão (*overprint*). | ❌ falta | não há no código |
| Simular saída de impressão. | ❌ falta | não há no código |
| Identificar problemas de cores. | ✅ feito | Preflight: CMYK, RGB e cores especiais (core preflight.rs) |
| Gerenciar tintas. | ❌ falta | não há no código |
| Configurar tratamento de tintas especiais. | ❌ falta | não há no código |
| Executar verificações Preflight. | ✅ feito | Converter ▸ Preflight (preflight.rs) |
| Utilizar perfis de verificação. | ✅ feito | Preflight por destino: gráfica, escritório ou tela (core preflight.rs Destino) |
| Criar perfis personalizados. | ❌ falta | não há no código |
| Identificar erros de impressão. | ✅ feito | Preflight aponta o que impede imprimir (core preflight.rs) |
| Corrigir problemas detectados. | 🟡 parcial | Preflight só lê; a correção fica em PDF/A, Comprimir e Achatar |
| Verificar fontes incorporadas. | ✅ feito | core preflight.rs |
| Verificar resolução de imagens. | ✅ feito | core preflight.rs |
| Verificar espaços de cor. | ✅ feito | core preflight.rs |
| Identificar linhas excessivamente finas. | ❌ falta | não há no código |
| Corrigir linhas finas. | ❌ falta | não há no código |
| Achatar transparências. | ❌ falta | não há no código |
| Visualizar resultado do achatamento. | ❌ falta | não há no código |
| Adicionar marcas de corte. | ❌ falta | não há no código |
| Adicionar marcas de registro. | ❌ falta | não há no código |
| Configurar caixas da página. | ✅ feito | core caixas.rs (Bleed, Trim, Art) |
| Configurar sangria (*bleed*). | ✅ feito | Caixas da página ▸ Sangria (BleedBox) (core caixas.rs) |
| Configurar área de corte (*trim*). | ✅ feito | Caixas da página ▸ Refile (TrimBox) (core caixas.rs) |
| Configurar área de mídia (*media box*). | 🟡 parcial | Padronizar tamanho muda a folha; sem campo próprio para a MediaBox |
| Configurar área de arte (*art box*). | ✅ feito | Caixas da página ▸ Arte (ArtBox) (core caixas.rs) |
| Configurar trapping. | ❌ falta | não há no código |
| Criar configurações de trapping. | ❌ falta | não há no código |
| Converter documentos para PDF/A. | 🟡 parcial | Converter ▸ PDF/A, só PDF/A-2b (core pdfa.rs) |
| Validar PDF/A. | 🟡 parcial | verificação própria, não substitui veraPDF |
| Converter documentos para PDF/X. | ❌ falta | não há no código |
| Validar PDF/X. | ❌ falta | não há no código |
| Converter documentos para PDF/E. | ❌ falta | não há no código |
| Validar PDF/E. | ❌ falta | não há no código |
| Verificar conformidade PDF/VT. | ❌ falta | não há no código |
| Verificar conformidade PDF/UA. | ❌ falta | não há no código |
| Gerenciar perfis de conformidade. | ❌ falta | não há no código |


## 19. Comparação e análise de documentos

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Comparar duas versões de um PDF. | ✅ feito | Comentar ▸ Comparar (comparar.rs, core comparar.rs) |
| Identificar textos adicionados. | ✅ feito | Comparar ▸ Texto: “Entrou” (core comparar.rs) |
| Identificar textos removidos. | ✅ feito | Comparar ▸ Texto: “Saiu” (core comparar.rs) |
| Identificar textos modificados. | 🟡 parcial | texto trocado aparece como saiu + entrou |
| Identificar alterações de formatação. | ❌ falta | não há no código |
| Identificar alterações em imagens. | 🟡 parcial | Sobrepor mostra em cor o que mudou no desenho da página |
| Identificar diferenças no layout. | 🟡 parcial | Sobrepor: vermelho só na antiga, azul só na nova |
| Visualizar documentos lado a lado. | ❌ falta | não há no código |
| Destacar diferenças. | ✅ feito | Sobrepor em vermelho e azul; lista Saiu/Entrou (comparar.rs) |
| Navegar entre diferenças. | ✅ feito | clique na página da diferença leva até ela (comparar.rs) |
| Gerar relatório de comparação. | ❌ falta | não há no código |
| Visualizar resumo das alterações. | ✅ feito | Comparar ▸ Texto lista o que saiu e o que entrou, com a página |
| Comparar documentos grandes. | ❌ falta | não há no código |
| Analisar alterações em documentos assinados. | 🟡 parcial | Versões abre a versão assinada; Comparar a confronta depois de salva |


## 20. Automação, processamento em lote e JavaScript

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Criar ações automatizadas. | ✅ feito | Converter ▸ Fluxos em lote (fluxos.rs) |
| Executar ações predefinidas. | ✅ feito | Fluxos em lote ▸ Modelos prontos (fluxos.rs) |
| Executar ações personalizadas. | ✅ feito | Fluxos em lote: arrastar os blocos na ordem desejada |
| Processar múltiplos arquivos. | ✅ feito | Fluxos em lote: pasta, PDFs ou imagens na entrada |
| Automatizar conversões. | ✅ feito | Fluxos em lote: saída Word, planilha, PNG ou PDF/A |
| Automatizar otimizações. | ✅ feito | Fluxos em lote: Comprimir, Embutir fontes, Linearizar |
| Automatizar aplicação de marcas d'água. | ✅ feito | Fluxos em lote, bloco Marca d'água |
| Automatizar alterações de segurança. | ✅ feito | Fluxos em lote: senha, assinatura, tarjar padrões, apagar metadados |
| Automatizar verificações. | ❌ falta | não há no código |
| Importar configurações de ações. | ❌ falta | não há no código |
| Exportar configurações de ações. | ❌ falta | não há no código |
| Executar JavaScript compatível com a API do Acrobat. | 🟡 parcial | Converter ▸ Script roda JavaScript com API própria (pdf.*), não a do Acrobat |
| Criar scripts associados ao documento. | ❌ falta | não há no código |
| Executar scripts em eventos. | ❌ falta | não há no código |
| Adicionar scripts aos campos de formulário. | ❌ falta | não há no código |
| Criar validações personalizadas. | ❌ falta | não há no código |
| Criar cálculos personalizados. | 🟡 parcial | Regras e Tab ▸ Fórmula (Qtd * [Valor] - Desconto), sem JavaScript livre |
| Controlar ações de botões. | ❌ falta | não há no código |
| Manipular propriedades do documento por script. | ❌ falta | não há no código |
| Manipular campos de formulário. | ✅ feito | Script: pdf.fields() e pdf.setField() (scripts.rs) |
| Configurar ações na abertura do documento. | ❌ falta | não há no código |
| Abrir editor JavaScript. | ✅ feito | Converter ▸ Script abre o editor ao lado da página (scripts.rs) |
| Depurar scripts. | ❌ falta | não há no código |
| Adicionar pontos de interrupção. | ❌ falta | não há no código |
| Inspecionar variáveis. | ❌ falta | não há no código |
| Gerenciar scripts existentes. | ❌ falta | não há no código |
| Converter PostScript em PDF. | ❌ falta | não há no código |
| Aplicar configurações de conversão PostScript. | ❌ falta | não há no código |
| Configurar incorporação de fontes. | ❌ falta | não há no código |
| Utilizar perfis de geração PDF. | ❌ falta | não há no código |
| Monitorar pastas para conversão automática (*watched folders*). | ❌ falta | não há no código |
| Compartilhar configurações de geração. | ❌ falta | não há no código |


## 21. Pesquisa avançada

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Pesquisar texto no documento. | ✅ feito | Ctrl+F, busca por linha de texto (busca.rs) |
| Pesquisar palavras inteiras. | ❌ falta | não há no código |
| Diferenciar maiúsculas de minúsculas. | ❌ falta | não há no código |
| Pesquisar expressões. | ❌ falta | não há no código |
| Pesquisar em comentários. | ✅ feito | filtro do painel Comentários (revisao.rs) |
| Pesquisar em marcadores. | ❌ falta | não há no código |
| Pesquisar em campos de formulário. | ❌ falta | não há no código |
| Pesquisar em anexos. | ❌ falta | não há no código |
| Pesquisar em camadas. | ❌ falta | não há no código |
| Pesquisar em vários documentos. | ❌ falta | não há no código |
| Utilizar operadores booleanos. | ❌ falta | não há no código |
| Pesquisar palavras próximas. | ❌ falta | não há no código |
| Pesquisar variações de palavras. | ❌ falta | não há no código |
| Criar índices de pesquisa. | ❌ falta | não há no código |
| Pesquisar índices existentes. | ❌ falta | não há no código |
| Salvar resultados de pesquisa. | ❌ falta | não há no código |
| Exportar resultados para PDF. | ❌ falta | não há no código |
| Exportar resultados para CSV. | ❌ falta | não há no código |


## 22. Recursos de engenharia, medições e PDF 3D

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Medir distâncias. | ✅ feito | distância, perímetro, área, escala, unidades e medidas gravadas (core medida.rs, medir.rs) |
| Medir perímetros. | ✅ feito | distância, perímetro, área, escala, unidades e medidas gravadas (core medida.rs, medir.rs) |
| Medir áreas. | ✅ feito | distância, perímetro, área, escala, unidades e medidas gravadas (core medida.rs, medir.rs) |
| Configurar escala. | ✅ feito | distância, perímetro, área, escala, unidades e medidas gravadas (core medida.rs, medir.rs) |
| Configurar unidades. | ✅ feito | distância, perímetro, área, escala, unidades e medidas gravadas (core medida.rs, medir.rs) |
| Adicionar marcações de medição. | ✅ feito | distância, perímetro, área, escala, unidades e medidas gravadas (core medida.rs, medir.rs) |
| Ajustar medições por pontos de referência. | ✅ feito | Escala… calibra por uma linha de tamanho conhecido (medir.rs) |
| Utilizar encaixe em pontos e linhas. | ✅ feito | Ímã gruda nos cantos e pontas das linhas (medir.rs) |
| Visualizar modelos 3D incorporados. | ❌ falta | não há no código |
| Adicionar modelos 3D compatíveis. | ❌ falta | não há no código |
| Rotacionar modelos. | ❌ falta | não há no código |
| Aplicar zoom em modelos. | ❌ falta | não há no código |
| Alterar visualizações. | ❌ falta | não há no código |
| Medir distâncias entre pontos 3D. | ❌ falta | não há no código |
| Medir ângulos. | 🟡 parcial | Medir ▸ Ângulo na página (core medida.rs), sem modelo 3D |
| Medir raios. | ❌ falta | não há no código |
| Medir distâncias perpendiculares. | ❌ falta | não há no código |
| Utilizar pontos de encaixe. | ❌ falta | não há no código |
| Configurar unidades dos modelos. | ❌ falta | não há no código |
| Criar visualizações predefinidas. | ❌ falta | não há no código |
| Visualizar PDFs com informações geográficas. | ❌ falta | não há no código |
| Identificar coordenadas. | ❌ falta | não há no código |
| Navegar por localizações. | ❌ falta | não há no código |
| Medir distâncias geográficas. | ❌ falta | não há no código |
| Medir áreas geográficas. | ❌ falta | não há no código |
| Configurar unidades geográficas. | ❌ falta | não há no código |
| Importar dados geográficos compatíveis. | ❌ falta | não há no código |
| Exportar marcações geográficas. | ❌ falta | não há no código |


## 23. Multimídia e documentos interativos

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Adicionar vídeos ao PDF. | ❌ falta | não há no código |
| Adicionar arquivos de áudio. | ❌ falta | não há no código |
| Vincular conteúdo multimídia externo. | ❌ falta | não há no código |
| Reproduzir multimídia compatível. | ❌ falta | não há no código |
| Controlar propriedades da reprodução. | ❌ falta | não há no código |
| Adicionar objetos interativos. | ❌ falta | não há no código |
| Configurar ações associadas a elementos. | ❌ falta | não há no código |
| Configurar ações de abertura de página. | ❌ falta | não há no código |
| Configurar ações de clique. | ❌ falta | não há no código |
| Configurar transições de página compatíveis. | ❌ falta | não há no código |
| Criar botões interativos. | ❌ falta | não há no código |
| Associar botões a arquivos e links. | ❌ falta | não há no código |
| Abrir anexos incorporados. | 🟡 parcial | painel Anexos salva o arquivo; não abre direto |
| Criar PDF Portfolios. | ❌ falta | não há no código |
| Adicionar documentos a Portfolios. | ❌ falta | não há no código |
| Criar pastas em Portfolios. | ❌ falta | não há no código |
| Extrair arquivos de Portfolios. | 🟡 parcial | painel Anexos salva cada arquivo, sem a estrutura de Portfolio |
| Organizar componentes de Portfolios. | ❌ falta | não há no código |


## 24. Compartilhamento e colaboração

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Compartilhar documentos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar links de compartilhamento. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Compartilhar por email. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Configurar permissões de acesso. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Compartilhar para visualização. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Compartilhar para revisão. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Coletar comentários. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Participar de revisões compartilhadas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Acompanhar comentários recebidos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Acompanhar revisões. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerenciar arquivos compartilhados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Controlar acesso a documentos armazenados na nuvem. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Integrar serviços de armazenamento compatíveis. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Trabalhar com Adobe Cloud Storage. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Compartilhar documentos para assinatura. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |


## 25. Inteligência artificial — Acrobat AI Assistant e Acrobat Studio

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Gerar resumo automático. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar resumo por seção. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Fazer perguntas sobre documentos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Extrair informações relevantes. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Localizar informações específicas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Analisar contratos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Identificar cláusulas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Identificar informações importantes. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Responder perguntas com referências ao documento. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Navegar até as fontes das respostas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar insights a partir de documentos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Analisar múltiplos arquivos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Resumir transcrições de reuniões. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar edições por linguagem natural. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar formatação de texto por IA. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar destaque de texto. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar remoção de informações confidenciais. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar organização de páginas. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar conversão de arquivos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar compressão de PDFs. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Solicitar outras operações compatíveis com o assistente. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Criar espaços de trabalho com documentos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Adicionar múltiplos arquivos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Adicionar links. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Consultar informações de diferentes documentos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Criar assistentes personalizados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Compartilhar espaços. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Colaborar em espaços compartilhados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar resumos integrados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar apresentações com IA. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar resumos em áudio. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Criar conteúdo em formato de podcast. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Personalizar a apresentação de informações. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Aplicar modelos de design. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Criar páginas de capa. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Criar materiais visuais. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Estilizar documentos. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Gerar imagens com IA. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Utilizar recursos gráficos do Adobe Express. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Criar apresentações e materiais de comunicação. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |


## 26. Preferências e personalização

| Item | Estado | Evidência / o que falta |
|---|---|---|
| Definir aplicativo padrão para PDF. | ✅ feito | Configurações ▸ Leitor de PDF padrão ▸ Tornar padrão (leitor_padrao.rs) |
| Configurar idioma. | ❌ falta | não há no código |
| Configurar aparência da interface. | ✅ feito | tema claro e escuro (Configurações > Aparência) |
| Personalizar barra de ferramentas. | ❌ falta | não há no código |
| Personalizar painéis. | 🟡 parcial | liga e desliga o painel Ferramentas e a faixa (Configurações), sem escolher o conteúdo |
| Personalizar ações rápidas. | ❌ falta | não há no código |
| Configurar atalhos de teclado disponíveis. | 🟡 parcial | atalhos fixos, listados na Ajuda (F1), não configuráveis |
| Configurar comportamento de abertura. | ❌ falta | não há no código |
| Configurar visualização inicial. | ✅ feito | Configurações ▸ Zoom ao abrir: Largura, Inteira ou Manter |
| Configurar preferências de zoom. | ✅ feito | Configurações ▸ Zoom ao abrir (configuracoes.rs) |
| Configurar renderização de fontes. | ❌ falta | não há no código |
| Configurar acessibilidade. | ❌ falta | não há no código |
| Configurar mecanismos de segurança. | ❌ falta | não há no código |
| Configurar preferências de JavaScript. | ❌ falta | não há no código |
| Configurar certificados. | ❌ falta | não há no código |
| Configurar serviços conectados. | ⛔ fora do escopo (serviço da Adobe) | depende de serviço da Adobe |
| Configurar opções de atualização. | ✅ feito | Configurações ▸ Sobre ▸ Procurar atualizações ao abrir (atualizacao.rs) |

