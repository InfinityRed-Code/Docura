//! CMS `SignedData` builder for PAdES-B-B / PAdES-B-T detached signatures.

use std::time::Duration;

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{GeneralizedTime, OctetString, SetOfVec, UtcTime};
use der::oid::ObjectIdentifier;
use der::{Any, Decode, Encode, Sequence};
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::{Attribute, Attributes};
use x509_cert::Certificate;

use crate::error::{SignError, SignResult};
use crate::signer::{SigAlg, Signer, TimestampClient};

// --- OIDs ---
const OID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const OID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const OID_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const OID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const OID_SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const OID_SIGNING_CERT_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");
const OID_SIG_TST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");
const OID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const OID_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const OID_ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");

/// ESS `ESSCertIDv2` — hash algorithm defaults to SHA-256 (omitted),
/// `issuerSerial` omitted (optional). Just binds the certificate hash.
#[derive(Sequence)]
struct EssCertIdV2 {
    cert_hash: OctetString,
}

/// ESS `SigningCertificateV2`.
#[derive(Sequence)]
struct SigningCertificateV2 {
    certs: Vec<EssCertIdV2>,
}

fn asn1<E: std::fmt::Display>(e: E) -> SignError {
    SignError::Asn1(e.to_string())
}

fn any_of<T: Encode>(v: &T) -> SignResult<Any> {
    Any::from_der(&v.to_der().map_err(asn1)?).map_err(asn1)
}

fn set_of<T: der::DerOrd + Encode>(v: Vec<T>) -> SignResult<SetOfVec<T>> {
    SetOfVec::try_from(v).map_err(asn1)
}

fn alg(oid: ObjectIdentifier, params: Option<Any>) -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned { oid, parameters: params }
}

fn attribute(oid: ObjectIdentifier, value: Any) -> SignResult<Attribute> {
    Ok(Attribute { oid, values: set_of(vec![value])? })
}

fn signing_time_value(epoch: i64) -> SignResult<Any> {
    let dur = Duration::from_secs(epoch.max(0) as u64);
    if let Ok(t) = UtcTime::from_unix_duration(dur) {
        any_of(&t)
    } else {
        let g = GeneralizedTime::from_unix_duration(dur).map_err(asn1)?;
        any_of(&g)
    }
}

/// Build a detached CMS `SignedData` (DER `ContentInfo`) over `message_digest`
/// = SHA-256 of the PDF byte ranges. Optionally adds an RFC 3161 signature
/// timestamp (PAdES-B-T) as an unsigned attribute.
pub fn build(
    signer: &dyn Signer,
    message_digest: &[u8],
    signing_time_epoch: i64,
    tsa: Option<&dyn TimestampClient>,
) -> SignResult<Vec<u8>> {
    let leaf = Certificate::from_der(signer.cert_der()).map_err(asn1)?;

    // --- signed attributes ---
    let cert_hash = Sha256::digest(signer.cert_der());
    let signing_cert = SigningCertificateV2 {
        certs: vec![EssCertIdV2 {
            cert_hash: OctetString::new(cert_hash.to_vec()).map_err(asn1)?,
        }],
    };
    let attrs_vec = vec![
        attribute(OID_CONTENT_TYPE, any_of(&OID_DATA)?)?,
        attribute(
            OID_MESSAGE_DIGEST,
            any_of(&OctetString::new(message_digest.to_vec()).map_err(asn1)?)?,
        )?,
        attribute(OID_SIGNING_TIME, signing_time_value(signing_time_epoch)?)?,
        attribute(OID_SIGNING_CERT_V2, any_of(&signing_cert)?)?,
    ];
    let signed_attrs: Attributes = set_of(attrs_vec)?;

    // The bytes actually signed are the DER `SET OF` (tag 0x31) of the attrs.
    let tbs = signed_attrs.to_der().map_err(asn1)?;
    let signature = signer.sign(&tbs)?;

    let sig_alg = match signer.alg() {
        SigAlg::RsaPkcs1Sha256 => alg(OID_RSA, Some(Any::null())),
        SigAlg::EcdsaP256Sha256 => alg(OID_ECDSA_SHA256, None),
    };

    let mut signer_info = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: leaf.tbs_certificate.issuer.clone(),
            serial_number: leaf.tbs_certificate.serial_number.clone(),
        }),
        digest_alg: alg(OID_SHA256, None),
        signed_attrs: Some(signed_attrs),
        signature_algorithm: sig_alg,
        signature: OctetString::new(signature.clone()).map_err(asn1)?,
        unsigned_attrs: None,
    };

    // --- PAdES-B-T: signature timestamp over the signature value ---
    if let Some(tsa) = tsa {
        let token = tsa.timestamp(&signature)?;
        let tst_attr = attribute(OID_SIG_TST, Any::from_der(&token).map_err(asn1)?)?;
        signer_info.unsigned_attrs = Some(set_of(vec![tst_attr])?);
    }

    // --- certificates set: leaf + chain ---
    let mut choices = vec![CertificateChoices::Certificate(leaf)];
    for c in signer.chain_der() {
        let cert = Certificate::from_der(c).map_err(asn1)?;
        choices.push(CertificateChoices::Certificate(cert));
    }
    let cert_set = CertificateSet(set_of(choices)?);

    let signed_data = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: set_of(vec![alg(OID_SHA256, None)])?,
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: OID_DATA,
            econtent: None,
        },
        certificates: Some(cert_set),
        crls: None,
        signer_infos: SignerInfos(set_of(vec![signer_info])?),
    };

    let ci = ContentInfo {
        content_type: OID_SIGNED_DATA,
        content: any_of(&signed_data)?,
    };
    ci.to_der().map_err(asn1)
}
