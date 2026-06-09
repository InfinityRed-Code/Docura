//! PDF digital signatures — PAdES baseline (B-B / B-T), pure RustCrypto.
//!
//! [`sign_pdf`] appends a signature to a PDF as an incremental update (original
//! bytes preserved, prior signatures stay valid). [`verify`] re-checks a
//! signed PDF. Keys come from a PKCS#12 file ([`load_pkcs12`]) or, on Windows,
//! the certificate store (see `windows` module).

pub mod error;
mod cms;
mod keys;
mod pkcs12;
mod prepare;
mod signer;
mod tsa;
mod verify;
#[cfg(all(windows, feature = "win-store"))]
pub mod windows;

use sha2::{Digest, Sha256};

pub use error::{SignError, SignResult};
pub use keys::KeySigner;
pub use signer::{SigAlg, Signer, TimestampClient};
pub use tsa::TsaClient;
pub use verify::{verify, SignatureReport, SignatureStatus};

/// Default bytes reserved for the CMS blob (32 KB hex). Comfortably fits an
/// RSA-2048/ECDSA signature, full chain, and an RFC 3161 timestamp token.
pub const DEFAULT_PLACEHOLDER_LEN: usize = 16384;

/// Signing options written into the signature dictionary.
#[derive(Clone, Debug)]
pub struct SignOptions {
    pub reason: Option<String>,
    pub name: Option<String>,
    pub location: Option<String>,
    pub contact: Option<String>,
    /// PDF date for `/M`, e.g. `D:20260609120000+00'00'`.
    pub pdf_date: String,
    /// Unix epoch seconds for the CMS signing-time attribute.
    pub signing_time_epoch: i64,
    /// Reserved hex `/Contents` size in bytes.
    pub placeholder_len: usize,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            reason: None,
            name: None,
            location: None,
            contact: None,
            pdf_date: String::new(),
            signing_time_epoch: 0,
            placeholder_len: DEFAULT_PLACEHOLDER_LEN,
        }
    }
}

/// Load a signing identity from a PKCS#12 (`.pfx`/`.p12`) file.
pub fn load_pkcs12(pfx_der: &[u8], password: &str) -> SignResult<KeySigner> {
    pkcs12::load(pfx_der, password)
}

/// Sign `pdf` with `signer`. If `tsa` is supplied, embeds an RFC 3161 signature
/// timestamp (PAdES-B-T). Returns the new PDF bytes.
pub fn sign_pdf(
    pdf: &[u8],
    signer: &dyn Signer,
    opt: &SignOptions,
    tsa: Option<&dyn TimestampClient>,
) -> SignResult<Vec<u8>> {
    let meta = prepare::SigDictMeta {
        pdf_date: opt.pdf_date.clone(),
        reason: opt.reason.clone(),
        name: opt.name.clone(),
        location: opt.location.clone(),
        contact: opt.contact.clone(),
        placeholder_len: opt.placeholder_len,
        field_seq: opt.signing_time_epoch,
    };
    let mut prepared = prepare::prepare(pdf, &meta)?;
    let msg = prepare::signed_message(&prepared.bytes, prepared.byte_range);
    let digest = Sha256::digest(&msg);
    let der = cms::build(signer, &digest, opt.signing_time_epoch, tsa)?;
    prepare::embed(&mut prepared.bytes, prepared.gap, &der)?;
    Ok(prepared.bytes)
}
