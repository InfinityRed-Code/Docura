//! Verification of digital signatures already present in a PDF.

use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier, SignerInfo};
use der::oid::ObjectIdentifier;
use der::{Decode, Encode};
use lopdf::{Document, Object};
use sha2::{Digest, Sha256};
use signature::Verifier;
use x509_cert::Certificate;

use crate::error::{SignError, SignResult};

const OID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const OID_SIG_TST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");
const OID_CN: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.3");
const OID_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const OID_RSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const OID_ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const OID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// Outcome of the cryptographic check on a single signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureStatus {
    /// Signature math verified and the signed digest matches the document bytes.
    Valid,
    /// Something failed: bad signature, tampered bytes, or digest mismatch.
    Invalid,
    /// Signature could not be checked (unsupported algorithm / parse error).
    Unsupported,
}

/// Per-signature verification report.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SignatureReport {
    pub field: Option<String>,
    pub signer_cn: Option<String>,
    pub signing_time: Option<String>,
    pub status: SignatureStatus,
    pub digest_matches: bool,
    pub covers_whole_document: bool,
    pub has_timestamp: bool,
    pub detail: String,
}

/// Verify every signature in `pdf`. Returns one report per signature field
/// (empty if the document is unsigned).
pub fn verify(pdf: &[u8]) -> SignResult<Vec<SignatureReport>> {
    let doc = Document::load_mem(pdf).map_err(|e| SignError::Verify(e.to_string()))?;
    let mut reports = Vec::new();

    for obj in doc.objects.values() {
        let Ok(d) = obj.as_dict() else { continue };
        let is_sig = d.get(b"Type").and_then(Object::as_name).map(|n| n == b"Sig").unwrap_or(false);
        if !is_sig || !d.has(b"ByteRange") || !d.has(b"Contents") {
            continue;
        }
        reports.push(verify_one(pdf, d));
    }
    Ok(reports)
}

fn verify_one(pdf: &[u8], sig: &lopdf::Dictionary) -> SignatureReport {
    let mut rep = SignatureReport {
        field: None,
        signer_cn: None,
        signing_time: sig
            .get(b"M")
            .and_then(Object::as_str)
            .ok()
            .map(|s| String::from_utf8_lossy(s).into_owned()),
        status: SignatureStatus::Unsupported,
        digest_matches: false,
        covers_whole_document: false,
        has_timestamp: false,
        detail: String::new(),
    };

    let br: Vec<i64> = match sig.get(b"ByteRange").and_then(Object::as_array) {
        Ok(a) => a.iter().filter_map(|o| o.as_i64().ok()).collect(),
        Err(_) => { rep.detail = "no ByteRange".into(); return rep; }
    };
    if br.len() != 4 {
        rep.detail = "malformed ByteRange".into();
        return rep;
    }
    // Untrusted input: negative values or sums past the file must not wrap
    // into a bogus (panicking) slice range.
    let in_file = |off: i64, len: i64| -> Option<(usize, usize)> {
        let (off, len) = (usize::try_from(off).ok()?, usize::try_from(len).ok()?);
        (off.checked_add(len)? <= pdf.len()).then_some((off, len))
    };
    let (Some((a, b)), Some((c, d))) = (in_file(br[0], br[1]), in_file(br[2], br[3])) else {
        rep.detail = "ByteRange exceeds file".into();
        rep.status = SignatureStatus::Invalid;
        return rep;
    };
    rep.covers_whole_document = a == 0 && c + d == pdf.len();

    // SHA-256 over the two signed ranges.
    let mut signed = Vec::with_capacity(b + d);
    signed.extend_from_slice(&pdf[a..a + b]);
    signed.extend_from_slice(&pdf[c..c + d]);
    let digest = Sha256::digest(&signed);

    let contents = match sig.get(b"Contents").and_then(Object::as_str) {
        Ok(s) => s,
        Err(_) => { rep.detail = "no Contents".into(); return rep; }
    };
    let cms_der = match trim_der(contents) {
        Some(end) => &contents[..end],
        None => { rep.detail = "Contents not DER".into(); return rep; }
    };

    match verify_cms(cms_der, &digest, &mut rep) {
        Ok(()) => {}
        Err(e) => {
            rep.detail = e.to_string();
            if rep.status == SignatureStatus::Unsupported {
                // leave as unsupported
            }
        }
    }
    rep
}

fn verify_cms(cms_der: &[u8], digest: &[u8], rep: &mut SignatureReport) -> SignResult<()> {
    let ci = ContentInfo::from_der(cms_der).map_err(|e| SignError::Verify(e.to_string()))?;
    let sd = ci
        .content
        .decode_as::<SignedData>()
        .map_err(|e| SignError::Verify(format!("signedData: {e}")))?;
    let si = sd
        .signer_infos
        .0
        .as_slice()
        .first()
        .ok_or_else(|| SignError::Verify("no SignerInfo".into()))?;
    // `digest` is SHA-256; comparing it against a SHA-1/-384/-512 message
    // digest would report a valid third-party signature as tampered.
    if si.digest_alg.oid != OID_SHA256 {
        return Err(SignError::Verify(format!(
            "unsupported digest algorithm {}",
            si.digest_alg.oid
        )));
    }

    rep.has_timestamp = si
        .unsigned_attrs
        .as_ref()
        .map(|a| a.iter().any(|attr| attr.oid == OID_SIG_TST))
        .unwrap_or(false);

    // message-digest signed attribute must equal our recomputed digest.
    let signed_attrs = si
        .signed_attrs
        .as_ref()
        .ok_or_else(|| SignError::Verify("no signed attributes".into()))?;
    let md_attr = signed_attrs
        .iter()
        .find(|a| a.oid == OID_MESSAGE_DIGEST)
        .ok_or_else(|| SignError::Verify("no message-digest attribute".into()))?;
    let md_val = md_attr.values.as_slice().first().ok_or_else(|| SignError::Verify("empty md".into()))?;
    let md_octets = md_val
        .decode_as::<der::asn1::OctetString>()
        .map_err(|e| SignError::Verify(e.to_string()))?;
    rep.digest_matches = md_octets.as_bytes() == digest;

    // Find the signing certificate (by issuer + serial).
    let cert = find_signer_cert(&sd, si).ok_or_else(|| SignError::Verify("signer cert not found".into()))?;
    rep.signer_cn = subject_cn(&cert);

    // Bytes actually signed: signed attributes re-encoded as SET OF.
    let tbs = signed_attrs.to_der().map_err(|e| SignError::Verify(e.to_string()))?;
    let sig_bytes = si.signature.as_bytes();
    let spki_der = cert
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .map_err(|e| SignError::Verify(e.to_string()))?;

    let sig_ok = verify_signature(&si.signature_algorithm.oid, &spki_der, &tbs, sig_bytes)?;

    rep.status = if sig_ok && rep.digest_matches {
        SignatureStatus::Valid
    } else {
        SignatureStatus::Invalid
    };
    if rep.detail.is_empty() {
        rep.detail = if rep.status == SignatureStatus::Valid {
            "signature verified".into()
        } else if !sig_ok {
            "signature math failed".into()
        } else {
            "document bytes do not match the signed digest".into()
        };
    }
    Ok(())
}

fn verify_signature(
    alg: &ObjectIdentifier,
    spki_der: &[u8],
    tbs: &[u8],
    sig: &[u8],
) -> SignResult<bool> {
    if *alg == OID_RSA || *alg == OID_RSA_SHA256 {
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        use rsa::pkcs8::DecodePublicKey;
        use rsa::RsaPublicKey;
        let pk = RsaPublicKey::from_public_key_der(spki_der)
            .map_err(|e| SignError::Verify(format!("rsa pubkey: {e}")))?;
        let vk = VerifyingKey::<Sha256>::new(pk);
        let signature = Signature::try_from(sig).map_err(|e| SignError::Verify(e.to_string()))?;
        Ok(vk.verify(tbs, &signature).is_ok())
    } else if *alg == OID_ECDSA_SHA256 {
        use p256::ecdsa::{Signature, VerifyingKey};
        use p256::pkcs8::DecodePublicKey;
        let vk = VerifyingKey::from_public_key_der(spki_der)
            .map_err(|e| SignError::Verify(format!("ec pubkey: {e}")))?;
        let signature = Signature::from_der(sig).map_err(|e| SignError::Verify(e.to_string()))?;
        Ok(vk.verify(tbs, &signature).is_ok())
    } else {
        Err(SignError::Verify(format!("unsupported signature algorithm {alg}")))
    }
}

fn find_signer_cert(sd: &SignedData, si: &SignerInfo) -> Option<Certificate> {
    let SignerIdentifier::IssuerAndSerialNumber(ias) = &si.sid else { return None };
    let certs = sd.certificates.as_ref()?;
    for choice in certs.0.iter() {
        if let cms::cert::CertificateChoices::Certificate(c) = choice {
            if c.tbs_certificate.issuer == ias.issuer
                && c.tbs_certificate.serial_number == ias.serial_number
            {
                return Some(c.clone());
            }
        }
    }
    None
}

fn subject_cn(cert: &Certificate) -> Option<String> {
    for rdn in cert.tbs_certificate.subject.0.iter() {
        for atv in rdn.0.iter() {
            if atv.oid == OID_CN {
                if let Ok(s) = atv.value.decode_as::<der::asn1::Utf8StringRef>() {
                    return Some(s.as_str().to_owned());
                }
                if let Ok(s) = atv.value.decode_as::<der::asn1::PrintableStringRef>() {
                    return Some(s.as_str().to_owned());
                }
            }
        }
    }
    None
}

/// Length of the first DER TLV starting at `bytes[0]` (a SEQUENCE), so trailing
/// zero padding in `/Contents` can be ignored.
fn trim_der(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 2 || bytes[0] != 0x30 {
        return None;
    }
    let b1 = bytes[1];
    let end = if b1 & 0x80 == 0 {
        2 + b1 as usize
    } else {
        let n = (b1 & 0x7f) as usize;
        if n == 0 || n > 4 || bytes.len() < 2 + n {
            return None;
        }
        let mut len = 0usize;
        for &byte in &bytes[2..2 + n] {
            len = (len << 8) | byte as usize;
        }
        2 + n + len
    };
    // A length running past /Contents means truncated or garbage data; the
    // caller slices with this value.
    (end <= bytes.len()).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Dictionary;

    #[test]
    fn trim_der_rejects_lengths_past_the_buffer() {
        assert_eq!(trim_der(&[0x30, 0x02, 0x05, 0x00, 0x00, 0x00]), Some(4));
        assert_eq!(trim_der(&[0x30, 0x7F, 0x00]), None);
        assert_eq!(trim_der(&[0x30, 0x84, 0x7F, 0xFF, 0xFF, 0xFF, 0x00]), None);
    }

    #[test]
    fn malformed_byte_range_is_invalid_not_a_panic() {
        let pdf = vec![b'x'; 100];
        for br in [[5, -1, 10, 10], [0, 10, i64::MAX, 2], [0, 50, 60, 41]] {
            let mut sig = Dictionary::new();
            sig.set("ByteRange", Object::Array(br.iter().map(|&v| Object::Integer(v)).collect()));
            sig.set("Contents", Object::string_literal(vec![0x30, 0x00]));
            let rep = verify_one(&pdf, &sig);
            assert_eq!(rep.status, SignatureStatus::Invalid, "{br:?}");
        }
    }
}
