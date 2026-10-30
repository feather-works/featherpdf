//! Assinatura digital PAdES com certificado A1 (.pfx/.p12) e a conferência das
//! assinaturas de um PDF. Trabalha só sobre bytes: quem lê e grava o arquivo é o IO.
//!
//! Assinar = atualização incremental (o arquivo anterior fica intacto, então as
//! assinaturas que já existiam continuam valendo) com um campo `/Sig`,
//! `/SubFilter /ETSI.CAdES.detached`, `/ByteRange` e `/Contents` reservado, onde
//! entra o CMS (SignedData destacado, atributos assinados com signing-certificate-v2).

use std::time::{SystemTime, UNIX_EPOCH};

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos};
use der::asn1::{ObjectIdentifier as Oid, OctetString, SetOfVec};
use der::{Any, Decode, DateTime, Encode, SliceReader};
use lopdf::{dictionary, text_string, Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream, StringFormat};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256, Sha384, Sha512};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::Attribute;
use x509_cert::name::Name;
use x509_cert::Certificate;

/// Bytes reservados para o CMS. A cadeia ICP-Brasil (3–4 certificados) cabe com folga.
const RESERVA: usize = 16 * 1024;
/// Marcador do /ByteRange antes de saber as posições (10 dígitos de largura).
const MARCA: i64 = 9_999_999_999;

const OID_DATA: Oid = Oid::new_unwrap("1.2.840.113549.1.7.1");
const OID_SIGNED_DATA: Oid = Oid::new_unwrap("1.2.840.113549.1.7.2");
const OID_CONTENT_TYPE: Oid = Oid::new_unwrap("1.2.840.113549.1.9.3");
const OID_MESSAGE_DIGEST: Oid = Oid::new_unwrap("1.2.840.113549.1.9.4");
const OID_SIGNING_TIME: Oid = Oid::new_unwrap("1.2.840.113549.1.9.5");
const OID_SIGNING_CERT_V2: Oid = Oid::new_unwrap("1.2.840.113549.1.9.16.2.47");
const OID_RSA: Oid = Oid::new_unwrap("1.2.840.113549.1.1.1");
const OID_SHA256: Oid = Oid::new_unwrap("2.16.840.1.101.3.4.2.1");
const OID_SHA384: Oid = Oid::new_unwrap("2.16.840.1.101.3.4.2.2");
const OID_SHA512: Oid = Oid::new_unwrap("2.16.840.1.101.3.4.2.3");

/// Certificado A1 aberto: a chave privada e a cadeia (o do titular primeiro).
pub struct Certificado {
    chave: RsaPrivateKey,
    cadeia: Vec<Certificate>,
}

impl std::fmt::Debug for Certificado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Certificado({})", self.titular())
    }
}

impl Certificado {
    /// Nome do titular (CN), como vai na assinatura.
    pub fn titular(&self) -> String { nome_comum(self.cadeia[0].tbs_certificate().subject()) }
    /// Nome da autoridade que emitiu o certificado.
    pub fn emissor(&self) -> String { nome_comum(self.cadeia[0].tbs_certificate().issuer()) }
    /// Validade, "dd/mm/aaaa".
    pub fn valido_ate(&self) -> String {
        let d = self.cadeia[0].tbs_certificate().validity().not_after.to_date_time();
        format!("{:02}/{:02}/{}", d.day(), d.month(), d.year())
    }
    /// A validade já passou (assinar ainda funciona, mas quem validar vai recusar).
    pub fn vencido(&self) -> bool {
        let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        self.cadeia[0].tbs_certificate().validity().not_after.to_unix_duration() < agora
    }
}

/// Abre o .pfx/.p12 com a senha. Só chaves RSA (o comum no A1 ICP-Brasil).
pub fn ler_certificado(p12: &[u8], senha: &str) -> Result<Certificado, String> {
    let ks = p12_keystore::KeyStore::from_pkcs12(p12, senha, p12_keystore::Pkcs12ImportPolicy::Strict)
        .map_err(|e| match e {
            p12_keystore::error::Error::MacError(_) => "Senha do certificado incorreta.".to_string(),
            e => format!("Não foi possível abrir o certificado: {e}"),
        })?;
    let (_, par) = ks.private_key_chain().ok_or("O arquivo não tem chave privada ligada a um certificado.")?;
    let chave = RsaPrivateKey::from_pkcs8_der(par.key().as_der())
        .map_err(|_| "Só certificados com chave RSA são aceitos por enquanto.".to_string())?;
    let cadeia = par.certs().iter().map(|c| Certificate::from_der(c.as_der()))
        .collect::<Result<Vec<_>, _>>().map_err(|e| format!("Certificado ilegível: {e}"))?;
    if cadeia.is_empty() { return Err("O arquivo não traz o certificado do titular.".into()); }
    Ok(Certificado { chave, cadeia })
}

#[derive(Debug, Clone, Default)]
pub struct OpcoesAssinatura {
    pub motivo: String,
    pub local: String,
    /// Página da caixa visível; `None` = assinatura invisível.
    pub visivel: Option<u32>,
    /// Lugar da caixa na página, em pontos do PDF; `None` = canto inferior direito.
    pub caixa: Option<[f32; 4]>,
    /// Dados do computador que a pessoa escolheu incluir ("IP … · MAC … · PC …"); vazio = nada.
    pub dispositivo: String,
}

/// Assina `base` (o PDF exatamente como está no disco) e devolve o arquivo novo:
/// `base` intacto + a atualização incremental com a assinatura.
pub fn assinar(base: &[u8], cert: &Certificado, op: &OpcoesAssinatura) -> Result<Vec<u8>, String> {
    let anterior = Document::load_mem(base).map_err(|e| format!("Não foi possível ler o PDF: {e}"))?;
    if anterior.trailer.get(b"Encrypt").is_ok() {
        return Err("Tire a senha do PDF antes de assinar.".into());
    }
    let paginas = anterior.get_pages();
    let num = op.visivel.unwrap_or(1);
    let pagina = *paginas.get(&num).ok_or_else(|| format!("Página {num} inexistente."))?;
    let catalogo = anterior.trailer.get(b"Root").and_then(Object::as_reference)
        .map_err(|_| "PDF sem catálogo (/Root).".to_string())?;
    let campos_antes = anterior.objects.values()
        .filter(|o| o.as_dict().is_ok_and(|d| d.get(b"FT").and_then(Object::as_name).is_ok_and(|n| n == b"Sig"))).count();
    let caixa = op.visivel.map(|_| op.caixa.unwrap_or_else(|| caixa_visivel(&anterior, pagina)));
    let mut inc = IncrementalDocument::create_from(base.to_vec(), anterior);

    let agora = DateTime::from_unix_duration(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    let titular = cert.titular();
    let mut sig = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "ETSI.CAdES.detached",
        "ByteRange" => vec![0.into(), MARCA.into(), MARCA.into(), MARCA.into()],
        "Contents" => Object::String(vec![0; RESERVA], StringFormat::Hexadecimal),
        "M" => Object::string_literal(format!("D:{:04}{:02}{:02}{:02}{:02}{:02}+00'00'",
            agora.year(), agora.month(), agora.day(), agora.hour(), agora.minutes(), agora.seconds())),
        "Name" => text_string(&titular),
    };
    if !op.motivo.is_empty() { sig.set("Reason", text_string(&op.motivo)); }
    if !op.local.is_empty() { sig.set("Location", text_string(&op.local)); }
    // Dentro do /ByteRange: mexer depois quebra a assinatura.
    if !op.dispositivo.is_empty() { sig.set("FeatherDispositivo", text_string(&op.dispositivo)); }
    let sig_id = inc.new_document.add_object(sig);

    let mut campo = dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Sig",
        "T" => text_string(&format!("Assinatura{}", campos_antes + 1)),
        "V" => sig_id, "F" => 132, "P" => pagina,
        "Rect" => caixa.map_or(vec![0.into(); 4], |c| c.iter().map(|v| Object::Real(*v)).collect()),
    };
    if let Some(c) = caixa {
        let data = format!("{:02}/{:02}/{} {:02}:{:02} UTC", agora.day(), agora.month(), agora.year(), agora.hour(), agora.minutes());
        let ap = aparencia(c[2] - c[0], c[3] - c[1], &titular, &data, &op.motivo, &op.dispositivo);
        campo.set("AP", dictionary! { "N" => inc.new_document.add_object(ap) });
    }
    let campo_id = inc.new_document.add_object(campo);
    acrescentar(&mut inc, pagina, b"Annots", campo_id.into())?;

    // O /AcroForm vira objeto próprio (se vier embutido no catálogo) para receber o campo.
    let formulario = match inc.get_prev_documents().get_dictionary(catalogo).map_err(|e| e.to_string())?.get(b"AcroForm") {
        Ok(Object::Reference(r)) => *r,
        outro => {
            let dicionario = match outro { Ok(Object::Dictionary(d)) => d.clone(), _ => Dictionary::new() };
            let id = inc.new_document.add_object(dicionario);
            inc.opt_clone_object_to_new_document(catalogo).map_err(|e| e.to_string())?;
            inc.new_document.get_dictionary_mut(catalogo).map_err(|e| e.to_string())?.set("AcroForm", id);
            id
        }
    };
    acrescentar(&mut inc, formulario, b"Fields", campo_id.into())?;
    inc.new_document.get_dictionary_mut(formulario).map_err(|e| e.to_string())?.set("SigFlags", 3);

    let mut saida = Vec::new();
    inc.save_to(&mut saida).map_err(|e| format!("Falha ao gravar a assinatura: {e}"))?;

    // Acha os espaços reservados só na parte nova.
    let achar = |agulha: &[u8], de: usize| saida[de..].windows(agulha.len()).position(|w| w == agulha).map(|p| p + de);
    let mut vazio = vec![b'0'; RESERVA * 2 + 2];
    vazio[0] = b'<';
    *vazio.last_mut().unwrap() = b'>';
    let inicio = achar(&vazio, base.len()).ok_or("Espaço da assinatura não encontrado.")?;
    let fim = inicio + vazio.len();
    let chave_br = achar(b"/ByteRange", base.len()).ok_or("/ByteRange não encontrado.")?;
    let abre = achar(b"[", chave_br).ok_or("/ByteRange malformado.")?;
    let fecha = achar(b"]", abre).ok_or("/ByteRange malformado.")?;
    let faixa = format!("0 {inicio} {fim} {}", saida.len() - fim);
    let largura = fecha - abre - 1;
    if faixa.len() > largura { return Err("/ByteRange sem espaço.".into()); }
    saida[abre + 1..fecha].copy_from_slice(format!("{faixa:<largura$}").as_bytes());

    let resumo = Sha256::new().chain_update(&saida[..inicio]).chain_update(&saida[fim..]).finalize();
    let cms = cms_assinado(cert, &resumo)?;
    if cms.len() > RESERVA { return Err("A assinatura não coube no espaço reservado.".into()); }
    let hex: String = cms.iter().map(|b| format!("{b:02X}")).collect();
    saida[inicio + 1..inicio + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(saida)
}

/// Acrescenta `item` à lista `chave` do objeto `dono` (lista direta ou por referência).
fn acrescentar(inc: &mut IncrementalDocument, dono: ObjectId, chave: &[u8], item: Object) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(dono).map_err(|e| e.to_string())?;
    let atual = inc.new_document.get_dictionary(dono).map_err(|e| e.to_string())?.get(chave).ok().cloned();
    let lista = match atual {
        Some(Object::Reference(r)) => {
            inc.opt_clone_object_to_new_document(r).map_err(|e| e.to_string())?;
            if let Ok(v) = inc.new_document.get_object_mut(r).and_then(Object::as_array_mut) {
                v.push(item);
                return Ok(());
            }
            vec![item]
        }
        Some(Object::Array(mut v)) => { v.push(item); v }
        _ => vec![item],
    };
    inc.new_document.get_dictionary_mut(dono).map_err(|e| e.to_string())?.set(chave, lista);
    Ok(())
}

/// Caixa de 240 × 64 pt no canto inferior direito da página.
// shortcut: ignora /Rotate da página; com página girada a caixa fica no canto "de pé" do arquivo.
fn caixa_visivel(doc: &Document, pagina: ObjectId) -> [f32; 4] {
    // A /MediaBox pode vir herdada da árvore de páginas.
    let mut no = doc.get_dictionary(pagina).ok();
    let mut mb = vec![0.0, 0.0, 595.0, 842.0];
    for _ in 0..16 {
        let Some(d) = no else { break };
        let caixa: Option<Vec<f32>> = d.get(b"MediaBox").ok().and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_array().ok().map(|v| v.iter().filter_map(|n| n.as_float().ok()).collect()));
        if let Some(c) = caixa.filter(|c| c.len() == 4) { mb = c; break; }
        no = d.get(b"Parent").and_then(Object::as_reference).and_then(|r| doc.get_dictionary(r)).ok();
    }
    let (x, y) = (mb[2] - 36.0 - 240.0, mb[1] + 36.0);
    [x, y, x + 240.0, y + 64.0]
}

/// Texto para string literal em WinAnsi (o que faltar vira "?"), com escape.
fn literal(s: &str, max: usize) -> String {
    let mut t: String = s.chars().take(max).collect();
    if s.chars().count() > max { t.pop(); t.push('…'); }
    t.chars().map(|c| crate::inserir::win_ansi(c).unwrap_or(b'?')).flat_map(|b| match b {
        b'(' | b')' | b'\\' => vec!['\\', b as char],
        b if b < 0x80 => vec![b as char],
        b => format!("\\{b:03o}").chars().collect(),
    }).collect()
}

/// Caixa visível: pena do Feather à esquerda e o texto ao lado, desenhados para 240 × 64 pt
/// e escalados para a caixa que a pessoa marcou.
fn aparencia(l: f32, a: f32, nome: &str, data: &str, motivo: &str, dispositivo: &str) -> Stream {
    let e = (l / 240.0).min(a / 64.0);
    let mut c = format!("q 0.96 0.98 0.96 rg 0.16 0.47 0.27 RG 0.8 w 0.4 0.4 {} {} re B Q\n", l - 0.8, a - 0.8);
    c += &format!("q {e} 0 0 {e} 0 {} cm\n", a - 64.0 * e);
    // Pena: o desenho ocupa 13..116 do quadrado de 128; vira 44 pt a partir de (8, 10).
    c += &format!("q 0.427 0 0 0.427 2.45 4.66 cm\n{}Q\n", include_str!("pena.pdfops"));
    c += "q 0.16 0.47 0.27 RG 0.5 w 58 9 m 58 55 l S Q\n";
    let grande = if nome.chars().count() > 24 { 8.5 } else { 10.0 };
    let linhas = [("Helv", 6.5, 52.0, "Assinado digitalmente com FeatherPDF".to_string(), "0.37 0.39 0.41"),
        ("HeBo", grande, 39.0, literal(nome, 30), "0 0 0"),
        ("Helv", 6.5, 28.0, literal(&format!("Data: {data}"), 48), "0.2 0.2 0.2"),
        ("Helv", 6.5, 19.0, literal(motivo, 48), "0.2 0.2 0.2"),
        ("Helv", 5.0, 9.0, literal(dispositivo, 66), "0.37 0.39 0.41")];
    for (fonte, tam, y, txt, cor) in linhas {
        if !txt.is_empty() { c += &format!("BT /{fonte} {tam} Tf {cor} rg 64 {y} Td ({txt}) Tj ET\n"); }
    }
    c += "Q\n";
    let fonte = |base: &str| dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base.to_string(), "Encoding" => "WinAnsiEncoding" };
    Stream::new(dictionary! {
        "Type" => "XObject", "Subtype" => "Form",
        "BBox" => vec![0.into(), 0.into(), Object::Real(l), Object::Real(a)],
        "Resources" => dictionary! { "Font" => dictionary! { "Helv" => fonte("Helvetica"), "HeBo" => fonte("Helvetica-Bold") } },
    }, c.into_bytes())
}

fn alg(oid: Oid, nulo: bool) -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned { oid, parameters: nulo.then(Any::null) }
}

fn atributo(oid: Oid, valor: Any) -> Result<Attribute, String> {
    Ok(Attribute { oid, values: SetOfVec::try_from(vec![valor]).map_err(|e| e.to_string())? })
}

/// CMS SignedData destacado sobre o resumo SHA-256 do /ByteRange.
fn cms_assinado(cert: &Certificado, resumo: &[u8]) -> Result<Vec<u8>, String> {
    let erro = |e: der::Error| e.to_string();
    let titular = &cert.cadeia[0];
    // ESS signing-certificate-v2: SEQ { SEQ { SEQ { OCTET STRING sha256(cert) } } } (sha256 é o padrão, omitido).
    let hash_cert = Sha256::digest(titular.to_der().map_err(erro)?);
    let mut ess = vec![0x30, 0x26, 0x30, 0x24, 0x30, 0x22, 0x04, 0x20];
    ess.extend_from_slice(&hash_cert);
    let atributos = SetOfVec::try_from(vec![
        atributo(OID_CONTENT_TYPE, Any::encode_from(&OID_DATA).map_err(erro)?)?,
        atributo(OID_MESSAGE_DIGEST, Any::encode_from(&OctetString::new(resumo).map_err(erro)?).map_err(erro)?)?,
        atributo(OID_SIGNING_CERT_V2, Any::from_der(&ess).map_err(erro)?)?,
    ]).map_err(erro)?;
    let assinatura = cert.chave.sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(atributos.to_der().map_err(erro)?))
        .map_err(|e| format!("Falha ao assinar: {e}"))?;
    let signer = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: titular.tbs_certificate().issuer().clone(),
            serial_number: titular.tbs_certificate().serial_number().clone(),
        }),
        digest_alg: alg(OID_SHA256, false),
        signed_attrs: Some(atributos),
        signature_algorithm: alg(OID_RSA, true),
        signature: OctetString::new(assinatura).map_err(erro)?,
        unsigned_attrs: None,
    };
    let certificados = cert.cadeia.iter().cloned().map(CertificateChoices::Certificate).collect::<Vec<_>>();
    let dados = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![alg(OID_SHA256, false)]).map_err(erro)?,
        encap_content_info: EncapsulatedContentInfo { econtent_type: OID_DATA, econtent: None },
        certificates: Some(CertificateSet(SetOfVec::try_from(certificados).map_err(erro)?)),
        crls: None,
        signer_infos: SignerInfos(SetOfVec::try_from(vec![signer]).map_err(erro)?),
    };
    ContentInfo { content_type: OID_SIGNED_DATA, content: Any::encode_from(&dados).map_err(erro)? }
        .to_der().map_err(erro)
}

/// O que se lê de uma assinatura do PDF.
#[derive(Debug, Clone)]
pub struct AssinaturaLida {
    pub campo: String,
    /// CN do certificado de quem assinou.
    pub quem: String,
    /// CN de quem emitiu o certificado.
    pub emissor: String,
    /// "dd/mm/aaaa hh:mm", da /M (declarada por quem assinou, sem carimbo de tempo).
    pub data: Option<String>,
    pub motivo: Option<String>,
    /// Dados do computador gravados pelo FeatherPDF (IP, MAC, nome do PC), se a pessoa incluiu.
    pub dispositivo: Option<String>,
    /// O resumo do /ByteRange bate com o assinado (o trecho coberto não mudou).
    pub integra: bool,
    /// A assinatura confere com a chave pública do certificado embutido.
    pub confere: bool,
    /// O /ByteRange vai até o fim do arquivo (nada foi acrescentado depois).
    pub cobre_tudo: bool,
    /// Emissor igual ao titular: qualquer um gera um desses com o nome que quiser.
    /// Mesmo sem isso a identidade não é verificada (não há cadeia até uma raiz confiável).
    pub autoassinado: bool,
    /// Por que não deu para conferir, quando não deu.
    pub problema: Option<String>,
}

impl AssinaturaLida {
    pub fn valida(&self) -> bool { self.integra && self.confere && self.problema.is_none() }
}

/// Lista e confere as assinaturas de `bytes` (o arquivo como está no disco).
/// Não valida a cadeia até uma raiz confiável nem revogação.
pub fn verificar_assinaturas(bytes: &[u8]) -> Result<Vec<AssinaturaLida>, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("Não foi possível ler o PDF: {e}"))?;
    let mut nomes = std::collections::HashMap::new();
    for o in doc.objects.values() {
        if let Ok(d) = o.as_dict() {
            if let (Ok(Object::Reference(v)), Ok(t)) = (d.get(b"V"), d.get(b"T")) {
                nomes.insert(*v, lopdf::decode_text_string(t).unwrap_or_default());
            }
        }
    }
    let mut lista = Vec::new();
    for (id, o) in &doc.objects {
        let Ok(d) = o.as_dict() else { continue };
        let (Ok(Object::Array(faixa)), Ok(Object::String(conteudo, _))) = (d.get(b"ByteRange"), d.get(b"Contents")) else { continue };
        let texto = |k: &[u8]| d.get(k).ok().and_then(|t| lopdf::decode_text_string(t).ok()).filter(|t| !t.is_empty());
        let mut a = AssinaturaLida {
            campo: nomes.get(id).cloned().unwrap_or_else(|| "Assinatura".into()),
            quem: texto(b"Name").unwrap_or_default(), emissor: String::new(),
            data: texto(b"M").and_then(|m| data_pdf(&m)), motivo: texto(b"Reason"), dispositivo: texto(b"FeatherDispositivo"),
            integra: false, confere: false, cobre_tudo: false, autoassinado: false, problema: None,
        };
        if let Err(e) = conferir(bytes, faixa, conteudo, &mut a) { a.problema = Some(e); }
        lista.push(a);
    }
    Ok(lista)
}

fn conferir(bytes: &[u8], faixa: &[Object], conteudo: &[u8], a: &mut AssinaturaLida) -> Result<(), String> {
    let n: Vec<usize> = faixa.iter().filter_map(|o| o.as_i64().ok()).filter(|v| *v >= 0).map(|v| v as usize).collect();
    let [i0, l0, i1, l1] = n[..] else { return Err("/ByteRange inválido.".into()) };
    if i0 + l0 > bytes.len() || i1 + l1 > bytes.len() || i0 + l0 > i1 {
        return Err("/ByteRange aponta para fora do arquivo.".into());
    }
    // A faixa começa no início do arquivo e só pula o próprio /Contents (`<hex>`); senão a
    // assinatura "íntegra" pode estar protegendo nada (ex.: [0 0 L 0]).
    let buraco = &bytes[l0..i1];
    let hex = |h: &[u8]| std::str::from_utf8(h).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
    let so_contents = buraco.len() == 2 + 2 * conteudo.len() && buraco.first() == Some(&b'<') && buraco.last() == Some(&b'>')
        && buraco[1..buraco.len() - 1].chunks(2).zip(conteudo).all(|(h, b)| hex(h) == Some(*b));
    if i0 != 0 || l0 == 0 || !so_contents { return Err("O /ByteRange não cobre o arquivo como deveria.".into()); }
    let partes = [&bytes[i0..i0 + l0], &bytes[i1..i1 + l1]];
    // Depois do fim coberto só pode sobrar fim de linha.
    a.cobre_tudo = bytes[i1 + l1..].iter().all(|b| b.is_ascii_whitespace());

    let erro = |e: der::Error| format!("CMS ilegível: {e}");
    let conteudo = ber_para_der(conteudo)?;
    let ci = ContentInfo::decode(&mut SliceReader::new(&conteudo).map_err(erro)?).map_err(erro)?;
    let sd: SignedData = ci.content.decode_as().map_err(erro)?;
    let si = sd.signer_infos.0.iter().next().ok_or("Assinatura sem signatário.")?;
    let certs: Vec<&Certificate> = sd.certificates.as_ref().map(|c| c.0.iter().filter_map(|c| match c {
        CertificateChoices::Certificate(c) => Some(c), _ => None }).collect()).unwrap_or_default();
    let cert = match &si.sid {
        SignerIdentifier::IssuerAndSerialNumber(s) => certs.iter().find(|c|
            c.tbs_certificate().serial_number() == &s.serial_number && c.tbs_certificate().issuer() == &s.issuer),
        SignerIdentifier::SubjectKeyIdentifier(ski) => certs.iter().find(|c| c.tbs_certificate()
            .get_extension::<x509_cert::ext::pkix::SubjectKeyIdentifier>().ok().flatten().is_some_and(|(_, k)| &k == ski)),
    }.or(certs.first()).copied().ok_or("O certificado de quem assinou não veio junto.")?;
    a.quem = nome_comum(cert.tbs_certificate().subject());
    a.emissor = nome_comum(cert.tbs_certificate().issuer());
    a.autoassinado = cert.tbs_certificate().issuer() == cert.tbs_certificate().subject();

    let h = Hash::de(si.digest_alg.oid)?;
    let resumo = h.resumo(&partes);
    let mensagem = match &si.signed_attrs {
        Some(attrs) => {
            let dig = attrs.iter().find(|x| x.oid == OID_MESSAGE_DIGEST).and_then(|x| x.values.iter().next())
                .ok_or("Falta o atributo messageDigest.")?.decode_as::<OctetString>().map_err(erro)?;
            a.integra = dig.as_bytes() == resumo.as_slice();
            if a.data.is_none() {
                a.data = attrs.iter().find(|x| x.oid == OID_SIGNING_TIME).and_then(|x| x.values.iter().next())
                    .and_then(|v| v.decode_as::<x509_cert::time::Time>().ok())
                    .map(|t| { let d = t.to_date_time(); format!("{:02}/{:02}/{} {:02}:{:02} UTC", d.day(), d.month(), d.year(), d.hour(), d.minutes()) });
            }
            h.resumo(&[&attrs.to_der().map_err(erro)?])
        }
        None => { a.integra = true; resumo }
    };
    // rsaEncryption ou sha256/384/512WithRSAEncryption (o PKCS#1 v1.5 é o mesmo).
    let rsa = ["1.2.840.113549.1.1.1", "1.2.840.113549.1.1.11", "1.2.840.113549.1.1.12", "1.2.840.113549.1.1.13"];
    if !rsa.contains(&si.signature_algorithm.oid.to_string().as_str()) {
        return Err("Algoritmo de assinatura não suportado ainda (só RSA).".into());
    }
    let chave = RsaPublicKey::from_public_key_der(&cert.tbs_certificate().subject_public_key_info().to_der().map_err(erro)?)
        .map_err(|_| "Chave do certificado não é RSA.".to_string())?;
    let esquema = match h { Hash::S256 => Pkcs1v15Sign::new::<Sha256>(), Hash::S384 => Pkcs1v15Sign::new::<Sha384>(), Hash::S512 => Pkcs1v15Sign::new::<Sha512>() };
    a.confere = chave.verify(esquema, &mensagem, si.signature.as_bytes()).is_ok();
    Ok(())
}

/// BER -> DER: o assinador do gov.br grava o CMS com comprimento indefinido e o leitor `der`
/// só aceita DER. Também junta OCTET STRING construída e ordena os SET, como o DER exige.
fn ber_para_der(b: &[u8]) -> Result<Vec<u8>, String> {
    fn elemento(b: &[u8], i: &mut usize, fundo: u8) -> Option<(Vec<u8>, Vec<u8>)> {
        // CMS de verdade não passa de ~15 níveis; o limite evita estourar a pilha.
        if fundo > 64 { return None; }
        let ini = *i;
        let t = *b.get(*i)?;
        *i += 1;
        if t & 0x1f == 0x1f { while *b.get(*i)? & 0x80 != 0 { *i += 1 } *i += 1; }
        let tag = b.get(ini..*i)?.to_vec();
        let l = *b.get(*i)? as usize;
        *i += 1;
        let tamanho = match l {
            0x80 => None,
            l if l < 0x80 => Some(l),
            l => {
                let n = l & 0x7f;
                let v = b.get(*i..*i + n)?.iter().fold(0usize, |a, &x| a.checked_mul(256).and_then(|a| a.checked_add(x as usize)).unwrap_or(usize::MAX));
                *i += n;
                Some(v)
            }
        };
        if t & 0x20 == 0 {
            let n = tamanho?;
            let c = b.get(*i..i.checked_add(n)?)?.to_vec();
            *i += n;
            return Some((tag, c));
        }
        let fim = match tamanho { Some(n) => Some(i.checked_add(n)?), None => None };
        let mut filhos = vec![];
        loop {
            match fim {
                Some(f) if *i >= f => break,
                None if b.get(*i..*i + 2)? == [0, 0] => { *i += 2; break }
                _ => filhos.push(elemento(b, i, fundo + 1)?),
            }
        }
        if fim.is_some_and(|f| *i != f) { return None; }
        // shortcut: só OCTET STRING construída é juntada; BIT STRING construída não aparece em CMS.
        if t == 0x24 { return Some((vec![0x04], filhos.into_iter().flat_map(|(_, c)| c).collect())); }
        let mut cod: Vec<Vec<u8>> = filhos.into_iter().map(|(t, c)| codificar(t, c)).collect();
        if t == 0x31 { cod.sort(); }
        Some((tag, cod.concat()))
    }
    fn codificar(mut t: Vec<u8>, c: Vec<u8>) -> Vec<u8> {
        let n = c.len();
        if n < 0x80 { t.push(n as u8) } else {
            let bytes: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|&x| x == 0).collect();
            t.push(0x80 | bytes.len() as u8);
            t.extend(bytes);
        }
        t.extend(c);
        t
    }
    let mut i = 0;
    elemento(b, &mut i, 0).map(|(t, c)| codificar(t, c)).ok_or_else(|| "CMS ilegível: codificação BER malformada.".into())
}

#[derive(Clone, Copy)]
enum Hash { S256, S384, S512 }

impl Hash {
    fn de(oid: Oid) -> Result<Self, String> {
        match oid {
            OID_SHA256 => Ok(Hash::S256),
            OID_SHA384 => Ok(Hash::S384),
            OID_SHA512 => Ok(Hash::S512),
            // shortcut: SHA-1 (assinaturas antigas) fica de fora; acrescentar o sha1 se aparecer.
            o => Err(format!("Resumo {o} não suportado.")),
        }
    }
    fn resumo(self, partes: &[&[u8]]) -> Vec<u8> {
        fn r<D: Digest>(partes: &[&[u8]]) -> Vec<u8> {
            partes.iter().fold(D::new(), |d, p| d.chain_update(p)).finalize().to_vec()
        }
        match self { Hash::S256 => r::<Sha256>(partes), Hash::S384 => r::<Sha384>(partes), Hash::S512 => r::<Sha512>(partes) }
    }
}

fn nome_comum(n: &Name) -> String {
    n.common_name().ok().flatten().map(|c| c.value().into_owned()).unwrap_or_else(|| n.to_string())
}

/// "D:20261019164000+00'00'" -> "19/10/2026 16:40".
fn data_pdf(m: &str) -> Option<String> {
    let s = m.strip_prefix("D:").unwrap_or(m);
    let p = |a: usize, b: usize| s.get(a..b).filter(|t| t.bytes().all(|c| c.is_ascii_digit()));
    Some(format!("{}/{}/{} {}:{}", p(6, 8)?, p(4, 6)?, p(0, 4)?, p(8, 10).unwrap_or("00"), p(10, 12).unwrap_or("00")))
}

#[cfg(test)]
mod testes {
    use super::*;

    const P12: &[u8] = include_bytes!("../testes/certificado-teste.p12");

    fn pdf() -> Vec<u8> { crate::testes::pdf_teste().serializar().unwrap() }

    #[test]
    fn ber_indefinido_vira_der() {
        // SEQUENCE indefinida { OCTET STRING construída {"ab", "c"}, SET {02 01 02, 02 01 01} } + enchimento.
        let ber = [0x30, 0x80, 0x24, 0x80, 0x04, 0x02, b'a', b'b', 0x04, 0x01, b'c', 0x00, 0x00,
            0x31, 0x06, 0x02, 0x01, 0x02, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(ber_para_der(&ber).unwrap(), [0x30, 0x0d, 0x04, 0x03, b'a', b'b', b'c', 0x31, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x02]);
        assert!(ber_para_der(&[0x30, 0x80, 0x04]).is_err());
        // Aninhamento sem fim e comprimento que estoura o índice: erro, sem pânico.
        assert!(ber_para_der(&[0x30, 0x80].repeat(100_000)).is_err());
        assert!(ber_para_der(&[0x30, 0x88, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x04, 0x00]).is_err());
    }

    #[test]
    fn assina_confere_e_detecta_alteracao() {
        assert!(ler_certificado(P12, "errada").is_err());
        let cert = ler_certificado(P12, "teste").unwrap();
        assert_eq!(cert.titular(), "Maria Teste:12345678900");

        let base = pdf();
        let op = OpcoesAssinatura { motivo: "Concordo com o conteúdo".into(), local: "São Paulo".into(), visivel: Some(1),
            caixa: Some([50.0, 50.0, 350.0, 134.0]), dispositivo: "PC teste".into() };
        let assinado = assinar(&base, &cert, &op).unwrap();
        assert!(assinado.starts_with(&base), "a atualização é incremental");
        let lidas = verificar_assinaturas(&assinado).unwrap();
        assert_eq!(lidas.len(), 1);
        let a = &lidas[0];
        assert!(a.valida() && a.cobre_tudo, "{a:?}");
        assert_eq!(a.quem, "Maria Teste:12345678900");
        assert_eq!(a.campo, "Assinatura1");
        assert_eq!(a.dispositivo.as_deref(), Some("PC teste"));
        assert!(crate::DocumentoPdf::de_bytes(&assinado, None).is_ok());

        // Um byte trocado dentro do trecho coberto quebra a integridade.
        let mut alterado = assinado.clone();
        let i = alterado.windows(8).position(|w| w == b"/MediaBo").unwrap();
        alterado[i + 1] = b'm';
        let a = &verificar_assinaturas(&alterado).unwrap()[0];
        assert!(!a.integra && !a.valida(), "{a:?}");

        // Segunda assinatura por cima: a primeira continua íntegra, mas não cobre mais tudo.
        let duas = assinar(&assinado, &cert, &OpcoesAssinatura::default()).unwrap();
        let lidas = verificar_assinaturas(&duas).unwrap();
        assert_eq!(lidas.len(), 2);
        assert!(lidas.iter().all(AssinaturaLida::valida), "{lidas:?}");
        assert_eq!(lidas.iter().filter(|a| a.cobre_tudo).count(), 1);

        // /ByteRange que não protege nada não passa por íntegro.
        let i = assinado.windows(10).position(|w| w == b"/ByteRange").unwrap();
        let abre = i + assinado[i..].iter().position(|&b| b == b'[').unwrap();
        let fecha = abre + assinado[abre..].iter().position(|&b| b == b']').unwrap();
        let mut vazio = assinado.clone();
        let falsa = format!("[0 0 {} 0", assinado.len());
        vazio[abre..fecha].copy_from_slice(format!("{falsa:<w$}", w = fecha - abre).as_bytes());
        let a = &verificar_assinaturas(&vazio).unwrap()[0];
        assert!(!a.valida() && a.problema.is_some(), "{a:?}");
    }
}
