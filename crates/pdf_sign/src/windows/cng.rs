//! Windows certificate-store (CNG) signing.
//!
//! Enumerates the user `MY` store, and signs a hash with a non-exportable
//! private key via `NCryptSignHash`. SHA-256 is computed in Rust; only the
//! private-key operation crosses into CNG.

use std::ffi::c_void;

use der::oid::ObjectIdentifier;
use der::Decode;
use sha2::{Digest, Sha256};
use windows::core::w;
use windows::Win32::Foundation::BOOL;
use windows::Win32::Security::Cryptography::{
    CertCloseStore, CertEnumCertificatesInStore, CertFindCertificateInStore,
    CertFreeCertificateContext, CertGetCertificateContextProperty, CertGetNameStringW,
    CertOpenSystemStoreW, CryptAcquireCertificatePrivateKey, NCryptFreeObject, NCryptSignHash,
    BCRYPT_PKCS1_PADDING_INFO, CERT_CONTEXT, CERT_FIND_HASH, CERT_NAME_SIMPLE_DISPLAY_TYPE,
    CERT_QUERY_ENCODING_TYPE, CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG, CRYPT_INTEGER_BLOB,
    NCRYPT_KEY_HANDLE, NCRYPT_PAD_PKCS1_FLAG, NCRYPT_SILENT_FLAG, X509_ASN_ENCODING,
};

use crate::error::{SignError, SignResult};
use crate::signer::{SigAlg, Signer};

const CERT_SHA1_HASH_PROP_ID: u32 = 3;
const CERT_KEY_PROV_INFO_PROP_ID: u32 = 2;
const OID_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const OID_EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");

fn win<E: std::fmt::Display>(e: E) -> SignError {
    SignError::Windows(e.to_string())
}

/// A certificate available for signing in the Windows store.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WinCertEntry {
    /// Uppercase hex SHA-1 thumbprint (the stable identifier).
    pub thumbprint: String,
    /// Human-readable subject (CN / display name).
    pub subject: String,
    /// Whether a usable private key is associated.
    pub has_private_key: bool,
}

struct Store(windows::Win32::Security::Cryptography::HCERTSTORE);

impl Store {
    fn open_my() -> SignResult<Self> {
        // SAFETY: standard system-store open; handle freed in Drop.
        let h = unsafe { CertOpenSystemStoreW(None, w!("MY")) }.map_err(win)?;
        Ok(Store(h))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        unsafe {
            let _ = CertCloseStore(self.0, 0);
        }
    }
}

unsafe fn cert_der(ctx: *const CERT_CONTEXT) -> Vec<u8> {
    let c = &*ctx;
    std::slice::from_raw_parts(c.pbCertEncoded, c.cbCertEncoded as usize).to_vec()
}

unsafe fn cert_thumbprint(ctx: *const CERT_CONTEXT) -> Option<String> {
    let mut len: u32 = 0;
    CertGetCertificateContextProperty(ctx, CERT_SHA1_HASH_PROP_ID, None, &mut len).ok()?;
    let mut buf = vec![0u8; len as usize];
    CertGetCertificateContextProperty(
        ctx,
        CERT_SHA1_HASH_PROP_ID,
        Some(buf.as_mut_ptr() as *mut c_void),
        &mut len,
    )
    .ok()?;
    buf.truncate(len as usize);
    Some(buf.iter().map(|b| format!("{b:02X}")).collect())
}

unsafe fn cert_subject(ctx: *const CERT_CONTEXT) -> String {
    let len = CertGetNameStringW(ctx, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None);
    if len <= 1 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize];
    CertGetNameStringW(ctx, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf));
    String::from_utf16_lossy(&buf[..buf.len().saturating_sub(1)])
}

unsafe fn cert_has_key(ctx: *const CERT_CONTEXT) -> bool {
    let mut len: u32 = 0;
    CertGetCertificateContextProperty(ctx, CERT_KEY_PROV_INFO_PROP_ID, None, &mut len).is_ok()
}

/// List signing-capable certificates in the user `MY` store.
pub fn list_certificates() -> SignResult<Vec<WinCertEntry>> {
    let store = Store::open_my()?;
    let mut out = Vec::new();
    let mut ctx: *const CERT_CONTEXT = std::ptr::null();
    loop {
        ctx = unsafe { CertEnumCertificatesInStore(store.0, Some(ctx)) };
        if ctx.is_null() {
            break;
        }
        unsafe {
            let Some(thumbprint) = cert_thumbprint(ctx) else { continue };
            out.push(WinCertEntry {
                thumbprint,
                subject: cert_subject(ctx),
                has_private_key: cert_has_key(ctx),
            });
        }
    }
    Ok(out)
}

/// A signer bound to a certificate in the Windows store (by thumbprint).
pub struct WindowsSigner {
    cert: Vec<u8>,
    alg: SigAlg,
    key: NCRYPT_KEY_HANDLE,
}

impl WindowsSigner {
    /// Resolve a certificate by its SHA-1 thumbprint (hex) and acquire its key.
    pub fn from_thumbprint(thumbprint_hex: &str) -> SignResult<Self> {
        let bytes = hex_to_bytes(thumbprint_hex)
            .ok_or_else(|| SignError::Windows("invalid thumbprint hex".into()))?;
        let store = Store::open_my()?;
        let blob = CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        };
        let ctx = unsafe {
            CertFindCertificateInStore(
                store.0,
                CERT_QUERY_ENCODING_TYPE(X509_ASN_ENCODING.0),
                0,
                CERT_FIND_HASH,
                Some(&blob as *const _ as *const c_void),
                None,
            )
        };
        if ctx.is_null() {
            return Err(SignError::Windows("certificate not found".into()));
        }

        let cert = unsafe { cert_der(ctx) };
        let alg = key_alg_from_cert(&cert)?;

        // Acquire the (non-exportable) private key as an NCRYPT handle.
        let mut key_handle = windows::Win32::Security::Cryptography::HCRYPTPROV_OR_NCRYPT_KEY_HANDLE::default();
        let mut key_spec = windows::Win32::Security::Cryptography::CERT_KEY_SPEC::default();
        let mut caller_free = BOOL(0);
        let res = unsafe {
            CryptAcquireCertificatePrivateKey(
                ctx,
                CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG,
                None,
                &mut key_handle,
                Some(&mut key_spec),
                Some(&mut caller_free),
            )
        };
        unsafe {
            let _ = CertFreeCertificateContext(Some(ctx));
        }
        res.map_err(|e| SignError::Windows(format!("acquire private key: {e}")))?;

        Ok(Self { cert, alg, key: NCRYPT_KEY_HANDLE(key_handle.0) })
    }
}

impl Drop for WindowsSigner {
    fn drop(&mut self) {
        unsafe {
            let _ = NCryptFreeObject(windows::Win32::Security::Cryptography::NCRYPT_HANDLE(self.key.0));
        }
    }
}

impl Signer for WindowsSigner {
    fn cert_der(&self) -> &[u8] {
        &self.cert
    }
    fn chain_der(&self) -> &[Vec<u8>] {
        &[]
    }
    fn alg(&self) -> SigAlg {
        self.alg
    }
    fn sign(&self, tbs: &[u8]) -> SignResult<Vec<u8>> {
        let hash = Sha256::digest(tbs);
        match self.alg {
            SigAlg::RsaPkcs1Sha256 => {
                let pad = BCRYPT_PKCS1_PADDING_INFO { pszAlgId: w!("SHA256") };
                ncrypt_sign(self.key, Some(&pad as *const _ as *const c_void), &hash, NCRYPT_PAD_PKCS1_FLAG)
            }
            SigAlg::EcdsaP256Sha256 => {
                let raw = ncrypt_sign(self.key, None, &hash, NCRYPT_SILENT_FLAG)?;
                ecdsa_raw_to_der(&raw)
            }
        }
    }
}

fn ncrypt_sign(
    key: NCRYPT_KEY_HANDLE,
    padding: Option<*const c_void>,
    hash: &[u8],
    flags: windows::Win32::Security::Cryptography::NCRYPT_FLAGS,
) -> SignResult<Vec<u8>> {
    let mut len: u32 = 0;
    unsafe {
        NCryptSignHash(key, padding, hash, None, &mut len, flags)
            .map_err(|e| SignError::Windows(format!("NCryptSignHash (size): {e}")))?;
    }
    let mut sig = vec![0u8; len as usize];
    unsafe {
        NCryptSignHash(key, padding, hash, Some(&mut sig), &mut len, flags)
            .map_err(|e| SignError::Windows(format!("NCryptSignHash: {e}")))?;
    }
    sig.truncate(len as usize);
    Ok(sig)
}

/// Determine the signature algorithm from the certificate's public-key OID.
fn key_alg_from_cert(cert_der: &[u8]) -> SignResult<SigAlg> {
    let cert = x509_cert::Certificate::from_der(cert_der).map_err(win)?;
    let oid = cert.tbs_certificate.subject_public_key_info.algorithm.oid;
    if oid == OID_RSA {
        Ok(SigAlg::RsaPkcs1Sha256)
    } else if oid == OID_EC_PUBLIC_KEY {
        Ok(SigAlg::EcdsaP256Sha256)
    } else {
        Err(SignError::Windows(format!("unsupported key algorithm {oid}")))
    }
}

/// Convert a fixed-width ECDSA signature (`r || s`) to DER `SEQUENCE { r, s }`.
fn ecdsa_raw_to_der(raw: &[u8]) -> SignResult<Vec<u8>> {
    if raw.is_empty() || !raw.len().is_multiple_of(2) {
        return Err(SignError::Windows("bad ECDSA signature length".into()));
    }
    let (r, s) = raw.split_at(raw.len() / 2);
    let r = der_uint(r);
    let s = der_uint(s);
    let mut body = Vec::with_capacity(r.len() + s.len());
    body.extend_from_slice(&r);
    body.extend_from_slice(&s);
    let mut out = vec![0x30];
    push_len(&mut out, body.len());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Encode a big-endian unsigned integer as a DER INTEGER (minimal, non-negative).
fn der_uint(bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes;
    while v.len() > 1 && v[0] == 0 {
        v = &v[1..];
    }
    let mut content = Vec::new();
    if v[0] & 0x80 != 0 {
        content.push(0x00);
    }
    content.extend_from_slice(v);
    let mut out = vec![0x02];
    push_len(&mut out, content.len());
    out.extend_from_slice(&content);
    out
}

fn push_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let mut tmp = Vec::new();
        let mut l = len;
        while l > 0 {
            tmp.push((l & 0xff) as u8);
            l >>= 8;
        }
        tmp.reverse();
        out.push(0x80 | tmp.len() as u8);
        out.extend_from_slice(&tmp);
    }
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    // Byte-offset slicing below needs ASCII (a thumbprint copied from the
    // Windows certificate dialog carries an invisible U+200E prefix).
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
