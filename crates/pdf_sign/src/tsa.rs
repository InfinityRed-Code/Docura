//! RFC 3161 timestamp client (PAdES-B-T).
//!
//! Requests a timestamp token over the CMS signature value and returns the
//! DER `TimeStampToken` (a `ContentInfo`) to embed as the
//! `id-aa-signatureTimeStampToken` unsigned attribute.

use std::io::Read;

use cms::content_info::ContentInfo;
use der::asn1::OctetString;
use der::oid::ObjectIdentifier;
use der::{Any, Decode, Encode, Sequence};
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;

use crate::error::{SignError, SignResult};
use crate::signer::TimestampClient;

const OID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

#[derive(Sequence)]
struct MessageImprint {
    hash_algorithm: AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

/// `TimeStampReq` (only the fields we need; reqPolicy/nonce/extensions omitted).
#[derive(Sequence)]
struct TimeStampReq {
    version: u8,
    message_imprint: MessageImprint,
    cert_req: bool,
}

/// `PKIStatusInfo` — only the leading status integer is needed; any trailing
/// optional fields are captured loosely.
#[derive(Sequence)]
struct PkiStatusInfo {
    status: i32,
    #[asn1(optional = "true")]
    _rest: Option<Any>,
}

#[derive(Sequence)]
struct TimeStampResp {
    status: PkiStatusInfo,
    #[asn1(optional = "true")]
    token: Option<ContentInfo>,
}

/// A timestamp authority reachable over HTTP.
pub struct TsaClient {
    url: String,
}

impl TsaClient {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    fn build_request(message: &[u8]) -> SignResult<Vec<u8>> {
        let imprint = Sha256::digest(message);
        let req = TimeStampReq {
            version: 1,
            message_imprint: MessageImprint {
                hash_algorithm: AlgorithmIdentifierOwned { oid: OID_SHA256, parameters: None },
                hashed_message: OctetString::new(imprint.to_vec())
                    .map_err(|e| SignError::Timestamp(e.to_string()))?,
            },
            cert_req: true,
        };
        req.to_der().map_err(|e| SignError::Timestamp(e.to_string()))
    }

    fn parse_response(resp: &[u8]) -> SignResult<Vec<u8>> {
        let parsed = TimeStampResp::from_der(resp)
            .map_err(|e| SignError::Timestamp(format!("bad TSA response: {e}")))?;
        if !matches!(parsed.status.status, 0 | 1) {
            return Err(SignError::Timestamp(format!(
                "TSA rejected request (PKIStatus {})",
                parsed.status.status
            )));
        }
        let token = parsed
            .token
            .ok_or_else(|| SignError::Timestamp("TSA returned no token".into()))?;
        token.to_der().map_err(|e| SignError::Timestamp(e.to_string()))
    }
}

impl TimestampClient for TsaClient {
    fn timestamp(&self, message: &[u8]) -> SignResult<Vec<u8>> {
        let req = Self::build_request(message)?;
        let resp = ureq::post(&self.url)
            .set("Content-Type", "application/timestamp-query")
            .timeout(std::time::Duration::from_secs(20))
            .send_bytes(&req)
            .map_err(|e| SignError::Timestamp(format!("TSA request failed: {e}")))?;
        let mut buf = Vec::new();
        resp.into_reader()
            .take(1 << 20)
            .read_to_end(&mut buf)
            .map_err(|e| SignError::Timestamp(e.to_string()))?;
        Self::parse_response(&buf)
    }
}
