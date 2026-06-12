//! OCR text layer (v1.0): `add_ocr_text_layer` injects words recognised on
//! a page bitmap as invisible text. The page must become searchable and
//! extractable, the words must report as hidden (render mode 3), the hit
//! rectangles must roughly match the original word boxes, and the visible
//! rendering must stay unchanged (white page stays white).

use lopdf::{Dictionary, Document, Object};
use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, SearchQuery, ZoomLevel};

/// Blank A4-ish page (612×792 pt) with no content — stands in for a scan
/// whose image carries no text layer.
fn blank_pdf(rotate: Option<i64>) -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let page_id = d.new_object_id();
    let cat_id = d.new_object_id();

    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set(
        "MediaBox",
        Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    );
    if let Some(r) = rotate {
        page.set("Rotate", Object::Integer(r));
    }
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

/// Fake Tesseract output: word boxes in pixel coords on a 300-DPI bitmap
/// of the 612×792 page (2550×3300 px).
fn fake_words() -> Vec<pdf_mutate::OcrWord> {
    vec![
        pdf_mutate::OcrWord {
            text: "Rechnungsbetrag".into(),
            x: 300.0,
            y: 400.0,
            w: 900.0,
            h: 60.0,
        },
        pdf_mutate::OcrWord { text: "Müller".into(), x: 1300.0, y: 400.0, w: 360.0, h: 60.0 },
        pdf_mutate::OcrWord { text: "123,45".into(), x: 300.0, y: 600.0, w: 340.0, h: 55.0 },
    ]
}

fn ocr_bytes(rotate: Option<i64>) -> Vec<u8> {
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&blank_pdf(rotate)).unwrap();
    mdoc.add_ocr_text_layer(0, &fake_words(), 2550, 3300).unwrap();
    mdoc.to_bytes().unwrap()
}

// One test fn on purpose: cargo runs #[test]s in parallel threads, and two
// engine instances calling into the (non-thread-safe) shared PDFium binding
// concurrently hang.
#[test]
fn ocr_layer_is_searchable_invisible_and_positioned() {
    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(ocr_bytes(None)).unwrap();

    // Extractable text contains the words (umlauts intact via WinAnsi).
    let text = engine.extract_text(id, PageIndex(0)).unwrap();
    assert!(text.contains("Rechnungsbetrag"), "text: {text:?}");
    assert!(text.contains("Müller"), "text: {text:?}");
    assert!(text.contains("123,45"), "text: {text:?}");

    // The layer reports as hidden text (render mode 3).
    let hidden = engine.page_hidden_text(id, PageIndex(0)).unwrap();
    assert!(
        hidden.iter().any(|s| s.contains("Rechnungsbetrag")),
        "hidden: {hidden:?}"
    );

    // Search hits land where the fake word boxes were: word 1 at
    // px(300,400,900x60) on 2550px-wide bitmap of a 612pt page →
    // pt x≈72, y≈96 (top-origin), w≈216, h≈14.4.
    let hits = engine
        .search(
            id,
            &SearchQuery {
                term: "Rechnungsbetrag".into(),
                case_sensitive: false,
                whole_word: false,
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1, "hits: {hits:?}");
    let r = hits[0].rects_pt[0];
    assert!((r.x0 - 72.0).abs() < 8.0, "hit rect x0: {r:?}");
    assert!((r.y0 - 96.0).abs() < 12.0, "hit rect y0: {r:?}");
    assert!((r.x1 - r.x0 - 216.0).abs() < 20.0, "hit rect width: {r:?}");

    // Visible rendering unchanged: the page renders pure white.
    let bmp = engine
        .render(RenderRequest {
            document: id,
            page: PageIndex(0),
            zoom: ZoomLevel(1.0),
            rotation: Rotation::R0,
        })
        .unwrap();
    assert!(
        bmp.rgba.chunks_exact(4).all(|p| p[0] >= 250 && p[1] >= 250 && p[2] >= 250),
        "OCR layer must not be visible"
    );
    engine.close(id).unwrap();

    // Rotated page (/Rotate 90): text must still extract and search must
    // still find it — the layer maps display coords back to user space.
    let id = engine.open_bytes(ocr_bytes(Some(90))).unwrap();
    let text = engine.extract_text(id, PageIndex(0)).unwrap();
    assert!(text.contains("Rechnungsbetrag"), "rotated text: {text:?}");
    let hits = engine
        .search(
            id,
            &SearchQuery {
                term: "Müller".into(),
                case_sensitive: false,
                whole_word: false,
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1, "rotated hits: {hits:?}");
    engine.close(id).unwrap();
}
