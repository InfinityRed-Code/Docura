//! Password protection (encrypt side).
//!
//! The engine pipeline (pdfium render + lopdf mutation) only works on
//! plaintext bytes. An encrypted PDF is therefore flattened to plaintext
//! when opened (via pdfium `FPDF_SaveAsCopy`, which strips security — see
//! the tauri layer) and re-encrypted here when the file is written back.
//!
//! lopdf 0.38 cannot *load* password-protected files (its reader only
//! parses the encryption dictionary unless the empty password works), so
//! decryption deliberately does not live in this module.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::Document;

use crate::{MutateError, MutateResult};

/// User-facing permission switches. Maps onto the PDF /P bitfield.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PermissionOptions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
}

impl Default for PermissionOptions {
    fn default() -> Self {
        Self { print: true, modify: true, copy: true, annotate: true }
    }
}

impl PermissionOptions {
    fn to_lopdf(self) -> Permissions {
        // Accessibility extraction stays allowed per PDF 2.0 guidance.
        let mut p = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.print {
            p |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.modify {
            p |= Permissions::MODIFIABLE | Permissions::ASSEMBLABLE;
        }
        if self.copy {
            p |= Permissions::COPYABLE;
        }
        if self.annotate {
            p |= Permissions::ANNOTABLE | Permissions::FILLABLE;
        }
        p
    }
}

/// Drop stale cross-reference and object streams left over from parsing.
/// Their contents are already materialised in `doc.objects`; letting
/// `encrypt()` touch them (or the writer re-emit them) corrupts the file.
fn strip_structural_streams(doc: &mut Document) {
    doc.objects.retain(|_, obj| match obj.as_stream() {
        Ok(s) => !(s.dict.has_type(b"ObjStm") || s.dict.has_type(b"XRef")),
        Err(_) => true,
    });
}

/// Cheap scan for the `/Encrypt` trailer key. False positives are fine —
/// callers follow up with a real check.
pub fn probably_encrypted(bytes: &[u8]) -> bool {
    let needle = b"/Encrypt";
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// True when the bytes are an encrypted PDF (trailer references /Encrypt).
/// Cheap for plaintext files (heuristic gate); for encrypted files lopdf
/// only extracts raw objects, so the confirming parse is cheap too.
pub fn is_encrypted_pdf(bytes: &[u8]) -> bool {
    probably_encrypted(bytes)
        && Document::load_mem(bytes)
            .map(|d| d.is_encrypted())
            .unwrap_or(false)
}

/// Encrypt plaintext PDF bytes with AES-256 (PDF 2.0, security handler V5/R6).
/// `owner_password` falls back to the user password when empty.
pub fn encrypt_pdf(
    bytes: &[u8],
    user_password: &str,
    owner_password: &str,
    permissions: PermissionOptions,
) -> MutateResult<Vec<u8>> {
    let mut doc = Document::load_mem(bytes).map_err(|e| MutateError::Parse(e.to_string()))?;
    if doc.is_encrypted() {
        return Err(MutateError::Crypt("document is already encrypted".into()));
    }
    let owner = if owner_password.is_empty() { user_password } else { owner_password };
    strip_structural_streams(&mut doc);

    let mut file_encryption_key = [0u8; 32];
    getrandom::fill(&mut file_encryption_key)
        .map_err(|e| MutateError::Crypt(format!("rng: {e}")))?;

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        file_encryption_key: &file_encryption_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: owner,
        user_password,
        permissions: permissions.to_lopdf(),
    };
    let state = EncryptionState::try_from(version)
        .map_err(|e| MutateError::Crypt(format!("state: {e}")))?;
    doc.encrypt(&state)
        .map_err(|e| MutateError::Crypt(format!("encrypt: {e}")))?;

    let mut buf = Vec::new();
    doc.save_to(&mut Cursor::new(&mut buf))
        .map_err(|e| MutateError::Serialize(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lopdf::{Dictionary, Object};

    pub fn tiny_pdf() -> Vec<u8> {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let page_id = d.new_object_id();
        let cat_id = d.new_object_id();
        let content_id = d.new_object_id();
        let content = lopdf::Stream::new(
            Dictionary::new(),
            b"BT /F1 12 Tf 72 712 Td (hello secret) Tj ET".to_vec(),
        );
        d.objects.insert(content_id, Object::Stream(content));
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
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
        d.max_id = content_id.0;
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn encrypted_output_authenticates() {
        let plain = tiny_pdf();
        assert!(!probably_encrypted(&plain));

        let perms = PermissionOptions { print: true, modify: false, copy: false, annotate: true };
        let enc = encrypt_pdf(&plain, "user-pw", "owner-pw", perms).unwrap();
        assert!(probably_encrypted(&enc));

        // lopdf can still parse trailer + encryption dict and authenticate
        // both passwords against the AES-256 verification data.
        let doc = Document::load_mem(&enc).unwrap();
        assert!(doc.is_encrypted());
        doc.authenticate_password("user-pw").unwrap();
        doc.authenticate_password("owner-pw").unwrap();
        assert!(doc.authenticate_password("wrong").is_err());
    }

    #[test]
    fn double_encrypt_is_rejected() {
        let enc =
            encrypt_pdf(&tiny_pdf(), "pw", "", PermissionOptions::default()).unwrap();
        match encrypt_pdf(&enc, "pw2", "", PermissionOptions::default()) {
            Err(MutateError::Crypt(_)) => {}
            other => panic!("expected Crypt error, got {:?}", other.map(|b| b.len())),
        }
    }
}
