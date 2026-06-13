//! Invariant: the page_text_layout run rectangles of an OCR layer never
//! overlap horizontally, even for tightly spaced words (pdfium merges those
//! into a single run). The visual overlap users can see comes from the
//! frontend text layer rendering each run's glyphs at their natural width and
//! overflowing the box — fixed there with a per-run horizontal scale — not
//! from overlapping run geometry, which this test pins down.

use lopdf::{Dictionary, Document, Object};
use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::PageIndex;

fn blank_pdf() -> Vec<u8> {
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

#[test]
fn tight_word_run_overlap() {
    // 612×792 page rendered at 300 DPI = 2550×3300 px. Three words on one
    // line with a tight 6 px gap (~1.4 pt) between successive boxes.
    let h = 60.0;
    let y = 400.0;
    let words = vec![
        pdf_mutate::OcrWord {
            text: "Lorem".into(),
            x: 300.0,
            y,
            w: 240.0,
            h,
        },
        pdf_mutate::OcrWord {
            text: "ipsum".into(),
            x: 546.0,
            y,
            w: 240.0,
            h,
        },
        pdf_mutate::OcrWord {
            text: "dolor".into(),
            x: 792.0,
            y,
            w: 240.0,
            h,
        },
    ];
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&blank_pdf()).unwrap();
    mdoc.add_ocr_text_layer(0, &words, 2550, 3300).unwrap();
    let bytes = mdoc.to_bytes().unwrap();

    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(bytes).unwrap();
    let runs = engine.page_text_layout(id, PageIndex(0)).unwrap();
    engine.close(id).unwrap();

    for r in &runs {
        eprintln!(
            "run x0={:.2} x1={:.2} y0={:.2} y1={:.2} {:?}",
            r.rect_pt.x0, r.rect_pt.x1, r.rect_pt.y0, r.rect_pt.y1, r.text
        );
    }
    // Report horizontal overlap between consecutive non-space runs.
    let words_only: Vec<_> = runs.iter().filter(|r| !r.text.trim().is_empty()).collect();
    let mut max_overlap = 0.0f32;
    for w in words_only.windows(2) {
        let ov = w[0].rect_pt.x1 - w[1].rect_pt.x0;
        eprintln!(
            "gap/overlap between {:?} and {:?}: {:.2}pt",
            w[0].text.trim(),
            w[1].text.trim(),
            ov
        );
        if ov > max_overlap {
            max_overlap = ov;
        }
    }
    eprintln!("MAX_OVERLAP_PT={max_overlap:.2}");
    assert!(
        max_overlap < 1.0,
        "OCR run rectangles overlap by {max_overlap:.2}pt — geometry bug"
    );
}
