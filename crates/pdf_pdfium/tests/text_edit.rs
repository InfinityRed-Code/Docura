//! Editing text that is part of the page content: `page_text_blocks` groups
//! text objects into lines, `replace_page_text` swaps a line for new text.

use lopdf::{Dictionary, Document, Object};
use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, TextReplace};

/// Page with a two-object line, a separate line further down and an
/// invisible (OCR-style) line of single-word objects.
fn sample_pdf() -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let page_id = d.new_object_id();
    let cat_id = d.new_object_id();
    let font_id = d.new_object_id();
    let content_id = d.new_object_id();

    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Times-Bold".to_vec()));
    d.objects.insert(font_id, Object::Dictionary(font));

    let content = lopdf::Stream::new(
        Dictionary::new(),
        b"BT /F1 20 Tf 1 0 0 rg 72 700 Td (Hello ) Tj (World) Tj ET \
          BT /F1 12 Tf 0 g 72 600 Td (Second line stays) Tj ET \
          BT 3 Tr /F1 10 Tf 1 0 0 1 72 500 Tm (scanned) Tj \
          1 0 0 1 112 500 Tm (words) Tj ET"
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
fn lists_lines_and_replaces_them() {
    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(sample_pdf()).unwrap();
    let p = PageIndex(0);

    let blocks = engine.page_text_blocks(id, p).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(blocks.len(), 3, "blocks: {texts:?}");

    // The two objects on one baseline form a single line.
    let hello = blocks.iter().find(|b| b.text == "Hello World").expect("merged line");
    assert_eq!(hello.objects.len(), 2);
    assert!((hello.font_size - 20.0).abs() < 0.01);
    assert_eq!(hello.font_family, "Times");
    assert!(hello.bold && !hello.italic && !hello.invisible);
    assert!(hello.color[0] > 0.9 && hello.color[1] < 0.1);
    assert_eq!(hello.dir, 0);
    // Baseline start in top-origin points: (72, 792 - 700).
    assert!((hello.origin_x - 72.0).abs() < 0.5, "origin_x {}", hello.origin_x);
    assert!((hello.origin_y - 92.0).abs() < 0.5, "origin_y {}", hello.origin_y);
    assert!(hello.rect_pt.y0 < 92.0 && hello.rect_pt.y1 >= 92.0);

    // Invisible single-word objects join with a space in between.
    let ocr = blocks.iter().find(|b| b.invisible).expect("invisible line");
    assert_eq!(ocr.text, "scanned words");

    // Replace the visible line; umlauts stay on the standard font.
    let edit = TextReplace {
        objects: hello.objects.clone(),
        text: "Grüße ändern".into(),
        font_size: hello.font_size,
        font_family: hello.font_family.clone(),
        bold: hello.bold,
        italic: hello.italic,
        color: hello.color,
        cover: None,
        font_file: None,
    };
    let bytes = engine.replace_page_text(id, p, &edit).unwrap();
    // The loaded document is untouched until the caller reloads it.
    assert!(engine.extract_text(id, p).unwrap().contains("Hello"));
    engine.reload_bytes(id, bytes).unwrap();

    let text = engine.extract_text(id, p).unwrap();
    assert!(text.contains("Grüße ändern"), "text: {text:?}");
    assert!(!text.contains("Hello") && !text.contains("World"), "text: {text:?}");
    assert!(text.contains("Second line stays"), "text: {text:?}");

    let blocks = engine.page_text_blocks(id, p).unwrap();
    let new = blocks.iter().find(|b| b.text == "Grüße ändern").expect("new line");
    assert!((new.origin_x - 72.0).abs() < 0.5 && (new.origin_y - 92.0).abs() < 0.5);
    assert!((new.font_size - 20.0).abs() < 0.01);
    assert_eq!(new.font_family, "Times");
    assert!(new.bold && new.color[0] > 0.9);

    // Replace the invisible line with visible text over a cover.
    let ocr = blocks.iter().find(|b| b.invisible).expect("invisible line");
    let edit = TextReplace {
        objects: ocr.objects.clone(),
        text: "typed words".into(),
        font_size: 10.0,
        font_family: "Helv".into(),
        bold: false,
        italic: false,
        color: [0.0, 0.0, 0.0],
        cover: Some([1.0, 1.0, 1.0]),
        font_file: None,
    };
    let bytes = engine.replace_page_text(id, p, &edit).unwrap();
    engine.reload_bytes(id, bytes).unwrap();
    let blocks = engine.page_text_blocks(id, p).unwrap();
    let typed = blocks.iter().find(|b| b.text == "typed words").expect("typed line");
    assert!(!typed.invisible);
    assert!(!engine.extract_text(id, p).unwrap().contains("scanned"));

    // Text beyond Latin-1 falls back to an embedded system font (Windows).
    if cfg!(windows) {
        let edit = TextReplace {
            objects: typed.objects.clone(),
            text: "Привет 5 €".into(),
            font_size: 10.0,
            font_family: "Helv".into(),
            bold: false,
            italic: false,
            color: [0.0, 0.0, 0.0],
            cover: None,
            font_file: None,
        };
        let bytes = engine.replace_page_text(id, p, &edit).unwrap();
        engine.reload_bytes(id, bytes).unwrap();
        assert!(engine.extract_text(id, p).unwrap().contains("Привет 5 €"));
    }

    // A bundled font is embedded and recognised again when the line is
    // reopened, so a second edit keeps the family.
    let blocks = engine.page_text_blocks(id, p).unwrap();
    let new = blocks.iter().find(|b| b.text == "Grüße ändern").expect("new line");
    let edit = TextReplace {
        objects: new.objects.clone(),
        text: "Roboto – Grüße €".into(),
        font_size: 20.0,
        font_family: "Roboto".into(),
        bold: true,
        italic: true,
        color: [0.0, 0.0, 0.0],
        cover: None,
        font_file: None,
    };
    let bytes = engine.replace_page_text(id, p, &edit).unwrap();
    engine.reload_bytes(id, bytes).unwrap();
    assert!(engine.extract_text(id, p).unwrap().contains("Roboto – Grüße €"));
    let blocks = engine.page_text_blocks(id, p).unwrap();
    let roboto = blocks.iter().find(|b| b.text == "Roboto – Grüße €").expect("roboto line");
    assert_eq!(roboto.font_family, "Roboto");
    assert!(roboto.bold && roboto.italic && roboto.font_file.is_none());
    let size_once = engine.document_bytes(id).unwrap().len();
    let edit = TextReplace { objects: roboto.objects.clone(), text: "Noch einmal".into(), ..edit };
    let bytes = engine.replace_page_text(id, p, &edit).unwrap();
    assert!(
        bytes.len() < size_once + 20_000,
        "the font embedded by the first edit is reused ({} -> {})",
        size_once,
        bytes.len()
    );
    engine.reload_bytes(id, bytes).unwrap();
    assert!(engine.extract_text(id, p).unwrap().contains("Noch einmal"));

    // Empty text just removes the line; stale indices are rejected.
    let blocks = engine.page_text_blocks(id, p).unwrap();
    let second = blocks.iter().find(|b| b.text == "Second line stays").unwrap();
    let mut edit = TextReplace {
        objects: second.objects.clone(),
        text: String::new(),
        font_size: 12.0,
        font_family: "Helv".into(),
        bold: false,
        italic: false,
        color: [0.0, 0.0, 0.0],
        cover: None,
        font_file: None,
    };
    let bytes = engine.replace_page_text(id, p, &edit).unwrap();
    edit.objects = vec![9999];
    assert!(engine.replace_page_text(id, p, &edit).is_err());
    engine.reload_bytes(id, bytes).unwrap();
    assert!(!engine.extract_text(id, p).unwrap().contains("Second line"));
}
