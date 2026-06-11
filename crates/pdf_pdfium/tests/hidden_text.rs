//! `page_hidden_text` finds text drawn with rendering mode 3 (invisible)
//! and ignores normally rendered text.

use lopdf::{Dictionary, Document, Object};
use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::PageIndex;

fn pdf_with_invisible_text() -> Vec<u8> {
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

    // One visible line, one invisible line (3 Tr = render mode invisible).
    let content = lopdf::Stream::new(
        Dictionary::new(),
        b"BT /F1 24 Tf 72 700 Td (visible line) Tj \
          3 Tr 0 -40 Td (GHOST-TEXT-99) Tj 0 Tr ET"
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

// One test fn on purpose: cargo runs #[test]s in parallel threads, and two
// engine instances calling into the (non-thread-safe) shared PDFium binding
// concurrently hang.
#[test]
fn finds_invisible_text_and_char_boxes() {
    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(pdf_with_invisible_text()).unwrap();

    // Invisible text (render mode 3) is reported, visible text is not.
    let snippets = engine.page_hidden_text(id, PageIndex(0)).unwrap();
    assert!(
        snippets.iter().any(|s| s.contains("GHOST-TEXT-99")),
        "invisible text not found: {snippets:?}"
    );
    assert!(
        !snippets.iter().any(|s| s.contains("visible line")),
        "visible text wrongly reported: {snippets:?}"
    );

    // Char boxes cover the page text with sane, in-page rects.
    let chars = engine.page_char_boxes(id, PageIndex(0)).unwrap();
    let text: String = chars.iter().map(|c| c.ch).collect();
    assert!(text.contains("visible line"), "text: {text:?}");
    for c in &chars {
        let r = c.rect_pt;
        if c.ch.is_whitespace() {
            continue;
        }
        assert!(r.x1 > r.x0 && r.y1 > r.y0, "degenerate box for {:?}", c.ch);
        assert!(r.x0 >= 0.0 && r.x1 <= 612.0 && r.y0 >= 0.0 && r.y1 <= 792.0);
    }

    engine.close(id).unwrap();
}
