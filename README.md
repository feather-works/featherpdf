# FeatherPDF

**Produto FeatherOffice independente · versão 0.1.4**

## Baixar

[![Get it from the Snap Store](https://snapcraft.io/static/images/badges/en/snap-store-black.svg)](https://snapcraft.io/featherpdf)

| Sistema | Arquivo | Como instalar |
|---|---|---|
| **Windows 10/11** | [FeatherPDF-windows-x64.exe](https://github.com/feather-works/featherpdf/releases/latest/download/FeatherPDF-windows-x64.exe) | rode o instalador; ou use o [zip portátil](https://github.com/feather-works/featherpdf/releases/latest/download/FeatherPDF-windows-x64-portatil.zip) |
| **Linux** (Snap Store) | [snapcraft.io/featherpdf](https://snapcraft.io/featherpdf) | `sudo snap install featherpdf` (atualiza sozinho) |
| **Linux** (Ubuntu, Debian, Mint…) | [featherpdf-linux-amd64.deb](https://github.com/feather-works/featherpdf/releases/latest/download/featherpdf-linux-amd64.deb) | `sudo apt install ./featherpdf-linux-amd64.deb` |
| **Linux** (qualquer distribuição) | [featherpdf-linux-x86_64.tar.gz](https://github.com/feather-works/featherpdf/releases/latest/download/featherpdf-linux-x86_64.tar.gz) | extraia e rode `./instalar.sh` |

Todas as versões e os `SHA256SUMS` ficam na [página de releases](https://github.com/feather-works/featherpdf/releases).

Editor nativo em **Rust + pelicano**. A página é exibida como PDF de verdade
(textos, imagens e elementos gráficos), e o documento permanece vetorial para
edição e salvamento. A visualização usa **Hayro 0.8**, um rasterizador em Rust,
sem exigir a instalação de PDFium ou MuPDF.

## Arquitetura — reaproveitar antes de implementar

O FeatherPDF consome os motores oficiais já existentes:

| Crate | Responsabilidade |
|---|---|
| `feather-draw` | Cria a cena vetorial A4 usada na criação de documentos. |
| `featheroffice-io` | `Registro::padrao().gravar("pdf", ...)` exporta a cena para PDF; `seguranca` fornece leitura limitada e gravação atômica para PDFs editados. |
| `featherpdf-core` | Domínio específico: estrutura de objetos PDF via `lopdf`, extração textual, rotação, exclusão, substituição textual limitada e serialização para bytes. **Sem dependência runtime de IO/interface.** |
| `featherpdf-io` | Adaptador entre core, FeatherIO e renderização PDF: criar, abrir, salvar, renderizar a página visualmente. |
| `feather-ui` | Tema, marca FeatherPDF, barra Chrome, Ribbon, Ajuda F1, atalhos globais e gestão de janelas (versão pelicano). |
| `featheroffice-charts` | Desenha o histórico real da memória no Monitor F12, exportando SVG via FeatherOffice Drawing. |
| `featherpdf-pelicano` | Janela, navegação, zoom e comandos de edição, sem implementação de parser/codec PDF. |
| `hayro` | Renderiza PDFs existentes em pixels (não substitui FeatherDrawing e não altera o arquivo). |

Fluxo de dependências: `featherpdf-pelicano → featherpdf-io → featherpdf-core` e
`featherpdf-io → featheroffice-{io,drawing}`. O FeatherCore não importa
FeatherIO, nem a interface. Para páginas de PDFs existentes, o formato de objetos
é específico de PDF e ainda utiliza `lopdf`; nenhum codec compartilhado
declara suporte à **leitura** de PDF de páginas complexas. Não anunciamos
suporte de leitura inexistente no registro comum.

## Funcionalidades disponíveis no código

- Abrir arquivos PDF de até **128 MiB** (limite conservador do FeatherIO).
- Mostrar a **página gráfica** com Hayro (fontes, imagens, formas e layout
  conforme suporte da biblioteca); **visualização gráfica padrão**.
- Zoom de **50% a 300%** e navegação entre páginas, com visualização em mesa cinza e painel lateral.
- Ribbon com as abas Arquivo, Páginas, Editar e Exibir, reaproveitando `feather-ui::ribbon`.
- Janela nativa de Ajuda pesquisável via **F1** (`feather-ui::ajuda`).
- Janela nativa de Monitor via **F12**, amostragem real da RAM e histórico SVG do motor `featheroffice-charts`.
- `Ctrl+F1` alterna a faixa, `Ctrl+N` novo, `Ctrl+O` abrir, `Ctrl+S` salvar como e `PageUp/PageDown` navegam.
- Modo opcional **Ver texto extraído**.
- Criar PDF A4 com o exportador vetorial compartilhado do FeatherOffice.
- Girar páginas em passos de 90° e excluir páginas (mantendo uma).
- Substituir texto quando o stream e a fonte são compatíveis.
- Inserir texto, imagens, desenhos e equações; comentar e anotar.
- Formulários: criar, preencher, campos calculados, importar/exportar dados.
- OCR (`featherpdf-ocr`, modelos ONNX baixados sob demanda e conferidos por SHA-256).
- Tarja (redação) que remove o conteúdo do arquivo, limpeza de JavaScript,
  anexos e metadados, senha (AES-256/AES-128) e assinatura digital PAdES com certificado A1.
- Scripts em JavaScript no sandbox das macros da suíte (sem arquivo nem rede).
- Converter: FeatherSheet/Excel, Word/ODT, PowerPoint, PDF/A, texto, Markdown, HTML e imagens.
- Imprimir pelo CUPS (`lp`).
- Salvar PDF estrutural (sem converter páginas inteiras em imagem).
- Gravação **atômica**: o FeatherIO escreve em arquivo temporário e renomeia
  somente quando a operação termina sem erro; links simbólicos e permissões do
  arquivo existente são mantidos.
- Renderização pesada em `tokio::spawn_blocking`; mensagens antigas são
  descartadas quando a página, documento ou zoom mudam.

**Importante:** o preview rasterizado é apenas o que aparece na janela.
O PDF salvo **não** é um screenshot; seus vetores e objetos continuam no
documento PDF.

## Comandos

Nesta pasta (o `feather-ui-pelicano` precisa estar na pasta irmã, ver `[patch]` no `Cargo.toml`):

```bash
cargo run -p featherpdf-pelicano
cargo check --workspace --all-targets
cargo test --workspace
```

## Limitações e validação

O projeto **ainda não é um editor de PDF com 100% das funções**. A substituição
textual é parcial. Fontes raras e efeitos gráficos complexos podem não renderizar
corretamente no Hayro. Editar um PDF assinado e salvar regrava o arquivo inteiro:
as assinaturas antigas deixam de valer (o Ctrl+S recusa gravar por cima de um
original assinado). O verificador confere integridade, não a identidade: não há
validação da cadeia até uma certificadora confiável.

A renderização usa pixels com zoom configurável e limite de **20 milhões de
pixels por página**; evita criar buffers sem limite. O arquivo original
não é rasterizado nem sobreposto automaticamente.

**Validação:** `cargo test --workspace` cobre core, io, OCR e a lógica da interface; a interface em si é conferida à mão.
