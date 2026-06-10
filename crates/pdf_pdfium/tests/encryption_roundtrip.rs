//! End-to-end check of the protection pipeline: lopdf encrypts (AES-256,
//! V5/R6), pdfium opens with the password, `save_copy_bytes` strips the
//! security handler and the plaintext copy is fully usable again.
//!
//! Requires `pdfium.dll` in `vendor/pdfium` (present in this repo).

use lopdf::{Dictionary, Document, Object};
use pdf_core::{CoreError, PdfEngine};
use pdf_pdfium::PdfiumEngine;

fn tiny_pdf() -> Vec<u8> {
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

// One single #[test]: PDFium is not thread-safe and the engine mutex is
// per-instance, so parallel test threads would race inside the library.
#[test]
fn pdfium_opens_lopdf_encrypted_and_strips_security() {
    let engine = PdfiumEngine::new().expect("pdfium.dll available");

    // Sanity: unprotected doc reports as such.
    let id0 = engine.open_bytes(tiny_pdf()).unwrap();
    assert!(!engine.is_protected(id0).unwrap());
    engine.close(id0).unwrap();
    let plain = tiny_pdf();
    let perms = pdf_mutate::PermissionOptions {
        print: true,
        modify: false,
        copy: false,
        annotate: true,
    };
    let enc = pdf_mutate::encrypt_pdf(&plain, "user-pw", "owner-pw", perms).unwrap();

    // No password → PasswordRequired
    match engine.open_bytes(enc.clone()) {
        Err(CoreError::PasswordRequired) => {}
        other => panic!("expected PasswordRequired, got {other:?}"),
    }
    // Wrong password → PasswordIncorrect
    match engine.open_bytes_with_password(enc.clone(), Some("wrong".into())) {
        Err(CoreError::PasswordIncorrect) => {}
        other => panic!("expected PasswordIncorrect, got {other:?}"),
    }

    // User password opens; the doc reports as protected.
    let id = engine
        .open_bytes_with_password(enc.clone(), Some("user-pw".into()))
        .unwrap();
    assert_eq!(engine.page_count(id).unwrap(), 1);
    assert!(engine.is_protected(id).unwrap());

    // /P bits must be readable for AES-256 (R6) — pdfium-render's permission
    // helpers fail there, the raw-binding path must not.
    let p = engine.document_permissions(id).unwrap();
    assert!(p.security_handler.contains("AES-256"), "handler: {}", p.security_handler);
    assert!(p.can_print_high_quality, "print should be allowed");
    assert!(!p.can_modify_content, "modify should be denied");
    assert!(!p.can_extract_text_and_graphics, "copy should be denied");
    assert!(p.can_annotate, "annotate should be allowed");

    // Flatten to plaintext working copy.
    let flat = engine.save_copy_bytes(id).unwrap();
    engine.reload_bytes(id, flat.clone()).unwrap();
    assert!(!engine.is_protected(id).unwrap());
    assert_eq!(engine.page_count(id).unwrap(), 1);
    let text = engine.extract_text(id, pdf_types::PageIndex(0)).unwrap();
    assert!(text.contains("hello secret"), "text was: {text:?}");

    // Rendering must work on the flattened copy — both full-page and
    // thumbnail-sized (the UI thumbnail path uses small zoom factors).
    for zoom in [1.0f32, 0.28] {
        let rendered = engine
            .render(pdf_core::RenderRequest {
                document: id,
                page: pdf_types::PageIndex(0),
                zoom: pdf_types::ZoomLevel(zoom),
                rotation: pdf_types::Rotation::R0,
            })
            .unwrap_or_else(|e| panic!("render at zoom {zoom} failed: {e}"));
        assert!(rendered.width_px > 0 && rendered.height_px > 0);
    }

    // The plaintext copy is mutable again with lopdf.
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&flat).unwrap();
    assert_eq!(mdoc.page_count(), 1);
    mdoc.to_bytes().unwrap();
    engine.close(id).unwrap();

    // Owner password also opens.
    let id2 = engine
        .open_bytes_with_password(enc, Some("owner-pw".into()))
        .unwrap();
    assert!(engine.is_protected(id2).unwrap());
    engine.close(id2).unwrap();
}
