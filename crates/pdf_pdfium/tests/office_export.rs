//! End-to-end: PDF text layout from the engine → pdf_export builds valid
//! .docx / .xlsx containers.
//!
//! Single #[test] fn on purpose — parallel test threads hang the shared
//! (non-thread-safe) PDFium binding.

use lopdf::{Dictionary, Document, Object};
use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::PageIndex;

fn pdf_with_table() -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let page_id = d.new_object_id();
    let cat_id = d.new_object_id();
    let font_id = d.new_object_id();
    let content_id = d.new_object_id();

    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    d.objects.insert(font_id, Object::Dictionary(font));

    // Heading, a paragraph and a small two-column table.
    let content = lopdf::Stream::new(
        Dictionary::new(),
        b"BT /F1 20 Tf 72 740 Td (Rechnung 2026) Tj ET \
          BT /F1 11 Tf 72 700 Td (Vielen Dank fuer Ihren Einkauf bei uns.) Tj ET \
          BT /F1 11 Tf 72 660 Td (Posten) Tj ET \
          BT /F1 11 Tf 300 660 Td (Betrag) Tj ET \
          BT /F1 11 Tf 72 640 Td (Miete) Tj ET \
          BT /F1 11 Tf 300 640 Td (1200) Tj ET"
            .to_vec(),
    );
    d.objects.insert(content_id, Object::Stream(content));

    let mut res = Dictionary::new();
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    res.set("Font", Object::Dictionary(fonts));

    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set("Contents", Object::Reference(content_id));
    page.set("Resources", Object::Dictionary(res));
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
fn engine_layout_feeds_docx_and_xlsx() {
    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(pdf_with_table()).unwrap();

    let runs = engine.page_text_layout(id, PageIndex(0)).unwrap();
    assert!(!runs.is_empty());

    // Flow reconstruction sees the heading and the paragraph.
    let blocks = pdf_export::reconstruct_blocks(&runs);
    assert!(
        blocks.iter().any(|b| matches!(b, pdf_export::Block::Heading(t) if t.contains("Rechnung"))),
        "no heading in {blocks:?}"
    );
    assert!(
        blocks.iter().any(
            |b| matches!(b, pdf_export::Block::Paragraph(t) if t.contains("Einkauf"))
        ),
        "paragraph missing in {blocks:?}"
    );

    // Both exporters produce ZIP containers of plausible size.
    let docx = pdf_export::build_docx(&[runs.clone()]).unwrap();
    assert_eq!(&docx[..2], b"PK");
    assert!(docx.len() > 1000);
    let xlsx = pdf_export::build_xlsx(&[runs]).unwrap();
    assert_eq!(&xlsx[..2], b"PK");
    assert!(xlsx.len() > 1000);

    engine.close(id).unwrap();
}
