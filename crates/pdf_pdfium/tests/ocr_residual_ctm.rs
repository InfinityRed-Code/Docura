//! Regression: pages whose content stream leaves a non-identity CTM active
//! (e.g. Canva exports that apply a page-level `cm` and never restore it)
//! must not drag the injected OCR text layer off-page. `append_content_stream`
//! isolates prior content in `q`/`Q` so the OCR words land in default user
//! space and stay searchable/extractable/selectable.

use lopdf::{Dictionary, Document, Object, Stream};
use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, SearchQuery};

/// A4-ish page (612×792 pt) whose content scales by 2× and never restores —
/// a residual CTM that would multiply any naively appended content.
fn page_with_residual_ctm() -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let page_id = d.new_object_id();
    let cat_id = d.new_object_id();
    let content_id = d.new_object_id();

    // No matching Q: the 2× scale stays active at end of stream.
    let content = Stream::new(Dictionary::new(), b"2 0 0 2 0 0 cm\n".to_vec());
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

fn fake_words() -> Vec<pdf_mutate::OcrWord> {
    // px coords on a 2550×3300 (300-DPI) bitmap of the 612×792 page.
    vec![pdf_mutate::OcrWord {
        text: "Rechnungsbetrag".into(),
        x: 300.0,
        y: 400.0,
        w: 900.0,
        h: 60.0,
    }]
}

#[test]
fn ocr_layer_ignores_residual_ctm() {
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&page_with_residual_ctm()).unwrap();
    mdoc.add_ocr_text_layer(0, &fake_words(), 2550, 3300)
        .unwrap();
    let bytes = mdoc.to_bytes().unwrap();

    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(bytes).unwrap();

    // Extractable (BoundedText over the page rect) — empty if the text sat
    // off-page under the 2× residual CTM.
    let text = engine.extract_text(id, PageIndex(0)).unwrap();
    assert!(
        text.contains("Rechnungsbetrag"),
        "OCR text not extractable on-page (residual CTM leaked): {text:?}"
    );

    // Hit lands at the un-scaled position: px(300,400) on a 2550px bitmap of
    // a 612pt page → x≈72, y≈96 (top-origin). A leaked 2× CTM would double it.
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
    assert!((r.x0 - 72.0).abs() < 8.0, "hit x0 {} (expected ~72)", r.x0);
    assert!((r.y0 - 96.0).abs() < 12.0, "hit y0 {} (expected ~96)", r.y0);
    engine.close(id).unwrap();
}
