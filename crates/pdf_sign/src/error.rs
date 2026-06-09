use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignError {
    #[error("pdf parse/serialize: {0}")]
    Pdf(String),
    #[error("signature placeholder too small: need {need} bytes, have {have}")]
    PlaceholderTooSmall { need: usize, have: usize },
    #[error("could not locate the signature placeholder in the serialized PDF")]
    PlaceholderNotFound,
    #[error("pkcs12: {0}")]
    Pkcs12(String),
    #[error("unsupported key algorithm: {0}")]
    UnsupportedKey(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("asn.1 encode: {0}")]
    Asn1(String),
    #[error("timestamp (RFC 3161): {0}")]
    Timestamp(String),
    #[error("verification: {0}")]
    Verify(String),
    #[cfg(windows)]
    #[error("windows cert store: {0}")]
    Windows(String),
}

pub type SignResult<T> = Result<T, SignError>;
