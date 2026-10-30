# Assinaturas no FeatherPDF — Lei 14.063/2020 e o que ler no PDF

Estudo de 19/10/2026 para o passo "identificar assinaturas de outras plataformas / gov.br".
**O texto oficial da lei (Planalto) não abriu na pesquisa**: a redação vem de reproduções
(Legjur). Conferir o art. 4º no Planalto antes de citar literalmente.

## Os três tipos (art. 4º)

- **Simples** (I): qualquer assinatura eletrônica; menor confiança em quem assinou.
- **Avançada** (II): certificado fora da ICP-Brasil ou outro meio que comprove autoria e
  integridade; vínculo único com o signatário, controle exclusivo e detecção de mudança
  posterior. É a do **gov.br** (conta prata ou ouro; certificado emitido pelo ITI, grátis,
  validade de 1 ano — [ITI](https://www.gov.br/iti/pt-br/assuntos/assinatura-eletronica-avancada)).
- **Qualificada** (III): certificado **ICP-Brasil** (A1 em arquivo, A3 em token/cartão);
  presunção de autenticidade (MP 2.200-2/2001, art. 10 §1º).

A lei trata das interações com entes públicos; entre particulares, as partes escolhem.
Há divergência prática: a Previdência recusou gov.br para Certidão de Tempo de Contribuição
(parecer S668901/2025), e um artigo relata o STJ validando assinatura fora da ICP-Brasil (2025).

Fontes: [Legjur art. 4º](https://www.legjur.com/legislacao/art/LEI_00140632020-4) ·
[Planalto (texto oficial)](https://www.planalto.gov.br/ccivil_03/_ato2019-2022/2020/lei/l14063.htm) ·
[Validador ITI](https://validar.iti.gov.br/) ·
[Barbieri Advogados](https://barbieriadvogados.com/?p=9059) · [IRIB](https://www.irib.org.br/?p=159036)

## O que o app classifica, a partir do próprio PDF

Não presumir pelo provedor: ler o certificado que está no `/Sig`.

| Achado no certificado | Tipo mostrado |
|---|---|
| Cadeia até a AC Raiz Brasileira (ICP-Brasil) | Qualificada |
| Certificado do gov.br (ITI) | Avançada (gov.br) |
| Outro certificado (DocuSign, Adobe, Clicksign…) | Avançada ou simples, conforme a plataforma |
| Só imagem/rubrica, sem `/Sig` | Simples (desenho), sem verificação |

**A confirmar com PDFs reais** (pedir ao usuário um de cada: gov.br, ICP-Brasil A3 e uma
plataforma privada): nome da AC e OIDs do certificado gov.br; OIDs ICP-Brasil (CPF em
2.16.76.1.3.1, políticas 2.16.76.1.2.x — conferir no DOC-ICP-04/15); a página de "manifesto"
e o QR code de cada plataforma privada.

## Checagens (engenharia, não vêm da lei)

Offline:
- achar todos os campos `/Sig`; ler `/Filter`, `/SubFilter` (`adbe.pkcs7.detached`,
  `ETSI.CAdES.detached`), `/M`, `/Name`, `/Reason`, `/Location` — só informativos;
- recalcular o hash do `/ByteRange` e conferir a assinatura CMS com o certificado embutido;
- mudança depois da assinatura: o `/ByteRange` cobre o arquivo até o fim, ou há atualização
  incremental depois?
- montar a cadeia com os certificados embutidos e um pacote local das raízes ICP-Brasil;
- carimbo do tempo (RFC 3161) e DSS/LTV embutidos: sem carimbo, a hora `/M` é só declaração.

Precisa de rede: revogação (CRL/OCSP) atual, atualizar as raízes, consultar o validador do ITI.
Sem rede, o resultado diz **"revogação não verificada"**, nunca "válida".
