//! End-to-end: sign a PDF and verify it (pure-Rust roundtrip + OpenSSL oracle).

use lopdf::{Dictionary, Document, Object};
use pdf_sign::{sign_pdf, verify, KeySigner, SignOptions, SignatureStatus};

const CERT_DER: &[u8] = include_bytes!("fixtures/cert.der");
const KEY_PKCS8: &[u8] = include_bytes!("fixtures/key.pkcs8.der");
const PFX_AES: &[u8] = include_bytes!("fixtures/signer-aes.p12");

fn tiny_pdf() -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let page_id = d.new_object_id();
    let cat_id = d.new_object_id();
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set("MediaBox", Object::Array(vec![
        0.into(), 0.into(), 612.into(), 792.into(),
    ]));
    d.objects.insert(page_id, Object::Dictionary(page));
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages.set("Count", Object::Integer(1));
    d.objects.insert(pages_id, Object::Dictionary(pages));
    let mut cat = Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    d.objects.insert(cat_id, Object::Dictionary(cat));
    d.trailer.set("Root", Object::Reference(cat_id));
    d.max_id = cat_id.0;
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

fn opts() -> SignOptions {
    SignOptions {
        reason: Some("Test signature".into()),
        name: Some("RustPDF Test Signer".into()),
        location: Some("DE".into()),
        pdf_date: "D:20260609120000+00'00'".into(),
        signing_time_epoch: 1_780_000_000,
        ..Default::default()
    }
}

#[test]
fn sign_with_pkcs8_then_verify_roundtrip() {
    let pdf = tiny_pdf();
    let signer = KeySigner::from_pkcs8(KEY_PKCS8, CERT_DER.to_vec(), vec![]).unwrap();
    let signed = sign_pdf(&pdf, &signer, &opts(), None).unwrap();

    // Original bytes preserved as an incremental update.
    assert_eq!(&signed[..pdf.len()], &pdf[..]);

    let reports = verify(&signed).unwrap();
    assert_eq!(reports.len(), 1, "expected exactly one signature");
    let r = &reports[0];
    assert_eq!(r.status, SignatureStatus::Valid, "detail: {}", r.detail);
    assert!(r.digest_matches);
    assert!(r.covers_whole_document);
    assert!(!r.has_timestamp);
    assert_eq!(r.signer_cn.as_deref(), Some("RustPDF Test Signer"));
}

#[test]
fn tampering_breaks_verification() {
    let pdf = tiny_pdf();
    let signer = KeySigner::from_pkcs8(KEY_PKCS8, CERT_DER.to_vec(), vec![]).unwrap();
    let mut signed = sign_pdf(&pdf, &signer, &opts(), None).unwrap();

    // Flip a byte inside the first signed range (the original content).
    signed[20] ^= 0xff;
    let reports = verify(&signed).unwrap();
    assert_eq!(reports[0].status, SignatureStatus::Invalid);
}

#[test]
fn load_pkcs12_aes_and_sign() {
    let pdf = tiny_pdf();
    let signer = pdf_sign::load_pkcs12(PFX_AES, "test123").unwrap();
    let signed = sign_pdf(&pdf, &signer, &opts(), None).unwrap();
    let reports = verify(&signed).unwrap();
    assert_eq!(reports[0].status, SignatureStatus::Valid, "detail: {}", reports[0].detail);
}

/// Independent oracle: OpenSSL must accept the detached CMS over the byte
/// ranges. Skipped when the `openssl` CLI is unavailable.
#[test]
fn openssl_accepts_signature() {
    use std::io::Write;
    use std::process::Command;

    if Command::new("openssl").arg("version").output().is_err() {
        eprintln!("openssl CLI not found — skipping oracle check");
        return;
    }

    let pdf = tiny_pdf();
    let signer = KeySigner::from_pkcs8(KEY_PKCS8, CERT_DER.to_vec(), vec![]).unwrap();
    let signed = sign_pdf(&pdf, &signer, &opts(), None).unwrap();

    // Extract /ByteRange + /Contents from the signed PDF.
    let doc = Document::load_mem(&signed).unwrap();
    let (br, contents) = doc
        .objects
        .values()
        .find_map(|o| {
            let d = o.as_dict().ok()?;
            if d.get(b"Type").and_then(Object::as_name).ok()? != b"Sig" {
                return None;
            }
            let br: Vec<i64> = d.get(b"ByteRange").and_then(Object::as_array).ok()?
                .iter().filter_map(|x| x.as_i64().ok()).collect();
            let c = d.get(b"Contents").and_then(Object::as_str).ok()?.to_vec();
            Some((br, c))
        })
        .expect("signature dict");

    // Trim trailing zero padding to the DER length.
    let der_len = {
        let n = (contents[1] & 0x7f) as usize;
        2 + n + contents[2..2 + n].iter().fold(0usize, |a, &b| (a << 8) | b as usize)
    };
    let cms_der = &contents[..der_len];

    let mut signed_content = Vec::new();
    signed_content.extend_from_slice(&signed[br[0] as usize..(br[0] + br[1]) as usize]);
    signed_content.extend_from_slice(&signed[br[2] as usize..(br[2] + br[3]) as usize]);

    let dir = std::env::temp_dir();
    let sig_path = dir.join("rustpdf_test_sig.der");
    let content_path = dir.join("rustpdf_test_content.bin");
    std::fs::File::create(&sig_path).unwrap().write_all(cms_der).unwrap();
    std::fs::File::create(&content_path).unwrap().write_all(&signed_content).unwrap();

    let out = Command::new("openssl")
        .args(["cms", "-verify", "-inform", "DER", "-in"])
        .arg(&sig_path)
        .arg("-content")
        .arg(&content_path)
        .args(["-no_content_verify", "-noverify", "-out"])
        .arg(dir.join("rustpdf_test_out.txt"))
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "openssl cms -verify failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
