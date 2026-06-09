//! PKCS#12 (`.pfx`/`.p12`) loader.
//!
//! Supports modern, RustCrypto-decodable protection: PBES2 (PBKDF2 + AES/3DES)
//! for both the shrouded key bag and the encrypted cert bags. Legacy
//! RC2/RC4-based PKCS#12 PBE (the old OpenSSL/Windows default) is **not**
//! supported — re-export with AES, e.g.
//! `openssl pkcs12 -export -keypbe AES-256-CBC -certpbe AES-256-CBC`.

use cms::content_info::ContentInfo;
use cms::encrypted_data::EncryptedData;
use der::asn1::OctetString;
use der::oid::ObjectIdentifier;
use der::{Decode, Encode};
use pkcs12::cert_type::CertBag;
use pkcs12::pfx::Pfx;
use pkcs12::safe_bag::SafeBag;
use pkcs12::{PKCS_12_CERT_BAG_OID, PKCS_12_KEY_BAG_OID, PKCS_12_PKCS8_KEY_BAG_OID};

use crate::error::{SignError, SignResult};
use crate::keys::KeySigner;

const OID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const OID_ENCRYPTED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.6");

fn p12<E: std::fmt::Display>(e: E) -> SignError {
    SignError::Pkcs12(e.to_string())
}

/// `SafeBag.bag_value` retains its `[0] EXPLICIT` wrapper; return the inner TLV.
fn explicit_inner(b: &[u8]) -> SignResult<&[u8]> {
    if b.len() < 2 || b[0] != 0xA0 {
        return Err(SignError::Pkcs12("bag value is not [0] EXPLICIT".into()));
    }
    let l0 = b[1];
    let (hdr, len) = if l0 & 0x80 == 0 {
        (2usize, l0 as usize)
    } else {
        let n = (l0 & 0x7f) as usize;
        if n == 0 || n > 4 || b.len() < 2 + n {
            return Err(SignError::Pkcs12("bad bag length".into()));
        }
        let mut v = 0usize;
        for &byte in &b[2..2 + n] {
            v = (v << 8) | byte as usize;
        }
        (2 + n, v)
    };
    b.get(hdr..hdr + len)
        .ok_or_else(|| SignError::Pkcs12("truncated bag value".into()))
}

/// The bytes inside a ContentInfo whose content is an `OCTET STRING`.
fn content_octets(ci: &ContentInfo) -> SignResult<Vec<u8>> {
    let der = ci.content.to_der().map_err(p12)?;
    let os = OctetString::from_der(&der).map_err(p12)?;
    Ok(os.as_bytes().to_vec())
}

fn decrypt_encrypted_data(ci: &ContentInfo, password: &str) -> SignResult<Vec<u8>> {
    let der = ci.content.to_der().map_err(p12)?;
    let ed = EncryptedData::from_der(&der).map_err(p12)?;
    let eci = ed.enc_content_info;
    let ct = eci
        .encrypted_content
        .ok_or_else(|| SignError::Pkcs12("encrypted-data: no content".into()))?;
    let alg_der = eci.content_enc_alg.to_der().map_err(p12)?;
    let scheme = pkcs5::EncryptionScheme::from_der(&alg_der).map_err(|e| {
        SignError::Pkcs12(format!(
            "unsupported cert encryption ({e}) — re-export the .pfx with -certpbe AES-256-CBC"
        ))
    })?;
    scheme
        .decrypt(password.as_bytes(), ct.as_bytes())
        .map_err(|e| SignError::Pkcs12(format!("cert bag decrypt failed (wrong password?): {e}")))
}

/// Parse a PKCS#12 blob and build a [`KeySigner`].
pub fn load(pfx_der: &[u8], password: &str) -> SignResult<KeySigner> {
    let pfx = Pfx::from_der(pfx_der).map_err(p12)?;
    if pfx.auth_safe.content_type != OID_DATA {
        return Err(SignError::Pkcs12("authSafe is not plain data".into()));
    }
    let authsafe = content_octets(&pfx.auth_safe)?;
    let safes = Vec::<ContentInfo>::from_der(&authsafe).map_err(p12)?;

    let mut certs: Vec<Vec<u8>> = Vec::new();
    let mut key_pkcs8: Option<Vec<u8>> = None;

    for ci in &safes {
        let bags_der = if ci.content_type == OID_DATA {
            content_octets(ci)?
        } else if ci.content_type == OID_ENCRYPTED_DATA {
            decrypt_encrypted_data(ci, password)?
        } else {
            continue;
        };
        let bags = Vec::<SafeBag>::from_der(&bags_der).map_err(p12)?;
        for bag in bags {
            let inner = explicit_inner(&bag.bag_value)?;
            if bag.bag_id == PKCS_12_CERT_BAG_OID {
                let cb = CertBag::from_der(inner).map_err(p12)?;
                certs.push(cb.cert_value.as_bytes().to_vec());
            } else if bag.bag_id == PKCS_12_PKCS8_KEY_BAG_OID {
                let epki = pkcs8::EncryptedPrivateKeyInfo::from_der(inner).map_err(p12)?;
                let secret = epki.decrypt(password.as_bytes()).map_err(|e| {
                    SignError::Pkcs12(format!("key decrypt failed (wrong password?): {e}"))
                })?;
                key_pkcs8 = Some(secret.as_bytes().to_vec());
            } else if bag.bag_id == PKCS_12_KEY_BAG_OID {
                key_pkcs8 = Some(inner.to_vec());
            }
        }
    }

    let key = key_pkcs8.ok_or_else(|| SignError::Pkcs12("no private key in .pfx".into()))?;
    if certs.is_empty() {
        return Err(SignError::Pkcs12("no certificate in .pfx".into()));
    }
    let leaf = certs.remove(0);
    KeySigner::from_pkcs8(&key, leaf, certs)
}
