//! Concrete software signer backed by an in-memory private key (RSA or P-256).

use der::oid::ObjectIdentifier;
use der::Decode;
use rsa::pkcs8::DecodePrivateKey;
use sha2::Sha256;
use signature::{SignatureEncoding, Signer as _};

use crate::error::{SignError, SignResult};
use crate::signer::{SigAlg, Signer};

const OID_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const OID_EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");

enum PrivateKey {
    Rsa(Box<rsa::RsaPrivateKey>),
    EcdsaP256(Box<p256::ecdsa::SigningKey>),
}

/// A software signing identity loaded from a PKCS#8 key + DER certificate(s).
pub struct KeySigner {
    cert: Vec<u8>,
    chain: Vec<Vec<u8>>,
    key: PrivateKey,
}

impl KeySigner {
    /// Build from an unencrypted PKCS#8 private key (DER), a leaf certificate
    /// (DER), and any intermediate certificates (DER).
    pub fn from_pkcs8(pkcs8_der: &[u8], cert_der: Vec<u8>, chain: Vec<Vec<u8>>) -> SignResult<Self> {
        let info = pkcs8::PrivateKeyInfo::from_der(pkcs8_der)
            .map_err(|e| SignError::Crypto(format!("pkcs8 parse: {e}")))?;
        let key = match info.algorithm.oid {
            OID_RSA => {
                let k = rsa::RsaPrivateKey::from_pkcs8_der(pkcs8_der)
                    .map_err(|e| SignError::Crypto(format!("rsa key: {e}")))?;
                PrivateKey::Rsa(Box::new(k))
            }
            OID_EC_PUBLIC_KEY => {
                let sk = p256::SecretKey::from_pkcs8_der(pkcs8_der)
                    .map_err(|e| SignError::UnsupportedKey(format!("ec key (only P-256): {e}")))?;
                PrivateKey::EcdsaP256(Box::new(p256::ecdsa::SigningKey::from(&sk)))
            }
            other => return Err(SignError::UnsupportedKey(other.to_string())),
        };
        Ok(Self { cert: cert_der, chain, key })
    }
}

impl Signer for KeySigner {
    fn cert_der(&self) -> &[u8] {
        &self.cert
    }
    fn chain_der(&self) -> &[Vec<u8>] {
        &self.chain
    }
    fn alg(&self) -> SigAlg {
        match self.key {
            PrivateKey::Rsa(_) => SigAlg::RsaPkcs1Sha256,
            PrivateKey::EcdsaP256(_) => SigAlg::EcdsaP256Sha256,
        }
    }
    fn sign(&self, tbs: &[u8]) -> SignResult<Vec<u8>> {
        match &self.key {
            PrivateKey::Rsa(k) => {
                let sk = rsa::pkcs1v15::SigningKey::<Sha256>::new((**k).clone());
                let sig: rsa::pkcs1v15::Signature = sk
                    .try_sign(tbs)
                    .map_err(|e| SignError::Crypto(format!("rsa sign: {e}")))?;
                Ok(sig.to_vec())
            }
            PrivateKey::EcdsaP256(k) => {
                let sig: p256::ecdsa::Signature = k
                    .try_sign(tbs)
                    .map_err(|e| SignError::Crypto(format!("ecdsa sign: {e}")))?;
                Ok(sig.to_der().to_bytes().to_vec())
            }
        }
    }
}
