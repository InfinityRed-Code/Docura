//! End-to-end redaction check: render a page via pdfium, black out the
//! region with the text, replace the page with the bitmap, and verify the
//! text is gone from the reloaded document while the page still renders.

use lopdf::{Dictionary, Document, Object};
use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, ZoomLevel};

fn pdf_with_text() -> Vec<u8> {
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

    let content = lopdf::Stream::new(
        Dictionary::new(),
        b"BT /F1 24 Tf 72 700 Td (CONFIDENTIAL-SSN-12345) Tj ET".to_vec(),
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
fn redacted_text_is_gone_and_page_still_renders() {
    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(pdf_with_text()).unwrap();

    let before = engine.extract_text(id, PageIndex(0)).unwrap();
    assert!(before.contains("CONFIDENTIAL-SSN-12345"), "text: {before:?}");

    // Render the page (≈200 DPI) and black out the band with the text.
    let zoom = 200.0f32 / 72.0;
    let mut rendered = engine
        .render(RenderRequest {
            document: id,
            page: PageIndex(0),
            zoom: ZoomLevel(zoom),
            rotation: Rotation::R0,
        })
        .unwrap();
    let (w, h) = (rendered.width_px, rendered.height_px);
    let sz = engine.page_size(id, PageIndex(0)).unwrap();
    // Top-origin rect roughly covering the text line (y_pdf 700 → y_top 92-ish).
    let (x0, y0, x1, y1) = (60.0f32, 60.0f32, 480.0f32, 120.0f32);
    let sx = w as f32 / sz.width_pt;
    let sy = h as f32 / sz.height_pt;
    for y in (y0 * sy) as u32..(y1 * sy) as u32 {
        for x in (x0 * sx) as u32..(x1 * sx) as u32 {
            let o = ((y * w + x) * 4) as usize;
            rendered.rgba[o] = 0;
            rendered.rgba[o + 1] = 0;
            rendered.rgba[o + 2] = 0;
            rendered.rgba[o + 3] = 255;
        }
    }
    // RGBA → RGB JPEG via the image crate.
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in rendered.rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .unwrap();

    let bytes = engine.document_bytes(id).unwrap();
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).unwrap();
    mdoc.replace_page_with_jpeg(0, jpeg, w, h, sz.width_pt, sz.height_pt)
        .unwrap();
    let out = mdoc.to_bytes().unwrap();

    // The raw file must not contain the secret anywhere.
    let needle = b"CONFIDENTIAL-SSN-12345";
    assert!(
        !out.windows(needle.len()).any(|win| win == needle),
        "secret still present in raw bytes"
    );

    engine.reload_bytes(id, out).unwrap();
    let after = engine.extract_text(id, PageIndex(0)).unwrap();
    assert!(!after.contains("CONFIDENTIAL"), "text after: {after:?}");
    let re = engine
        .render(RenderRequest {
            document: id,
            page: PageIndex(0),
            zoom: ZoomLevel(1.0),
            rotation: Rotation::R0,
        })
        .unwrap();
    assert!(re.width_px > 0 && re.height_px > 0);
    // Sampled pixel inside the redacted band must be black.
    let cx = (200.0 * re.width_px as f32 / sz.width_pt) as u32;
    let cy = (90.0 * re.height_px as f32 / sz.height_pt) as u32;
    let o = ((cy * re.width_px + cx) * 4) as usize;
    assert!(re.rgba[o] < 30 && re.rgba[o + 1] < 30 && re.rgba[o + 2] < 30,
        "pixel not black: {:?}", &re.rgba[o..o + 3]);
    engine.close(id).unwrap();
}
