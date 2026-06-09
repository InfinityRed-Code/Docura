//! Abstraction over the signing key so the CMS builder is agnostic to where
//! the key lives (PKCS#12 file or Windows cert store).

use crate::error::SignResult;

/// Signature algorithm used by a [`Signer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigAlg {
    /// RSASSA-PKCS1-v1_5 with SHA-256.
    RsaPkcs1Sha256,
    /// ECDSA over P-256 with SHA-256 (DER-encoded signature).
    EcdsaP256Sha256,
}

/// A signing identity: a leaf certificate, its chain, and the ability to
/// produce a signature over the to-be-signed bytes (the DER of `signedAttrs`).
pub trait Signer {
    /// DER of the signer's (leaf) X.509 certificate.
    fn cert_der(&self) -> &[u8];
    /// DER of intermediate CA certificates, leaf-to-root order (may be empty).
    fn chain_der(&self) -> &[Vec<u8>];
    /// Signature algorithm.
    fn alg(&self) -> SigAlg;
    /// Sign `tbs` (the DER `SET OF` encoding of the signed attributes). The
    /// implementation hashes with SHA-256 internally and returns the raw
    /// signature value to place in `SignerInfo.signature` (PKCS#1 v1.5 block
    /// for RSA, DER `SEQUENCE { r, s }` for ECDSA).
    fn sign(&self, tbs: &[u8]) -> SignResult<Vec<u8>>;
}

/// Timestamp authority client (RFC 3161). Returns the DER of a
/// `TimeStampToken` (a CMS `ContentInfo`) for the given message imprint input.
pub trait TimestampClient {
    fn timestamp(&self, message: &[u8]) -> SignResult<Vec<u8>>;
}
