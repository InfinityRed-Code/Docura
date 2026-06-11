//! Coordinate-space check for pages whose MediaBox does not start at (0,0)
//! or whose CropBox differs from the MediaBox — common in professionally
//! produced PDFs (print marks, scans). Text-layout rects must line up with
//! the rendered bitmap: a black test square is drawn at a known user-space
//! position and the reported text rect of a glyph next to it must match.

use lopdf::{dictionary, Dictionary, Document, Object};
use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, ZoomLevel};

/// Page with MediaBox [20 30 632 822] (612x792 visible, origin offset) and
/// a filled black square at user-space (120,700)-(170,750).
fn pdf_with_offset_mediabox() -> Vec<u8> {
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
        b"0 0 0 rg 120 700 50 50 re f\nBT /F1 20 Tf 200 715 Td (MARKER) Tj ET".to_vec(),
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
        Object::Array(vec![20.into(), 30.into(), 632.into(), 822.into()]),
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
    let _ = dictionary! {};

    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

/// Find the bounding box of dark pixels in the rendered RGBA bitmap.
fn dark_bbox(rgba: &[u8], w: u32, h: u32) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let o = ((y * w + x) * 4) as usize;
            if rgba[o] < 60 && rgba[o + 1] < 60 && rgba[o + 2] < 60 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0, y0, x1, y1)
}

#[test]
fn text_rects_match_bitmap_on_offset_mediabox() {
    let engine = PdfiumEngine::new().expect("pdfium.dll");
    let id = engine.open_bytes(pdf_with_offset_mediabox()).unwrap();

    let sz = engine.page_size(id, PageIndex(0)).unwrap();
    assert!((sz.width_pt - 612.0).abs() < 1.0, "width {}", sz.width_pt);
    assert!((sz.height_pt - 792.0).abs() < 1.0, "height {}", sz.height_pt);

    let rendered = engine
        .render(RenderRequest {
            document: id,
            page: PageIndex(0),
            zoom: ZoomLevel(1.0),
            rotation: Rotation::R0,
        })
        .unwrap();
    let (w, h) = (rendered.width_px, rendered.height_px);

    // Where the square actually IS in the bitmap (pdfium ground truth).
    // The square spans x slightly further once MARKER text is included, so
    // measure only the left part of the bitmap (x < 180 view-pt).
    let cut = (185.0 * w as f32 / sz.width_pt) as u32;
    let mut left = rendered.rgba.clone();
    for y in 0..h {
        for x in cut..w {
            let o = ((y * w + x) * 4) as usize;
            left[o] = 255;
            left[o + 1] = 255;
            left[o + 2] = 255;
        }
    }
    let (bx0, by0, bx1, by1) = dark_bbox(&left, w, h);
    let px_per_pt_x = w as f32 / sz.width_pt;
    let px_per_pt_y = h as f32 / sz.height_pt;
    let square_view = (
        bx0 as f32 / px_per_pt_x,
        by0 as f32 / px_per_pt_y,
        bx1 as f32 / px_per_pt_x,
        by1 as f32 / px_per_pt_y,
    );
    // Expected in view space (origin at MediaBox corner): x 120-20=100..150,
    // y_top = 822-750=72 .. 822-700=92 → (100,72)-(150,122)? height 50:
    // y_top0 = (mb_y1 - 750) = 72, y_top1 = (mb_y1 - 700) = 122.
    println!("square in bitmap (view pt): {square_view:?}");

    // Where the TEXT layout claims the MARKER text is (top-origin space).
    let runs = engine.page_text_layout(id, PageIndex(0)).unwrap();
    let marker = runs
        .iter()
        .find(|r| r.text.contains("MARKER"))
        .expect("MARKER run found");
    println!("MARKER rect (reported): {:?}", marker.rect_pt);

    // Text baseline at user-space y=715, 20pt font ⇒ top-origin view y ≈
    // 822-735=87 .. 822-715=107. x starts at 200-20=180.
    let r = &marker.rect_pt;
    assert!(
        (r.x0 - 180.0).abs() < 6.0,
        "text x0 off: got {} want ~180 (offset bug: {})",
        r.x0,
        r.x0 - 180.0
    );
    assert!(
        r.y0 > 80.0 && r.y0 < 100.0,
        "text y0 off: got {} want ~87 (offset bug: {})",
        r.y0,
        r.y0 - 87.0
    );

    // Write-side roundtrip: a highlight placed at the reported text rect must
    // come back at the same top-origin position from the lopdf read path.
    let view_rect = *r;
    let bytes = engine.document_bytes(id).unwrap();
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).unwrap();
    mdoc.add_markup_annotation(
        0,
        &pdf_mutate::MarkupAnnotation {
            kind: pdf_mutate::MarkupKind::Highlight,
            rects: vec![view_rect],
            color: [1.0, 0.9, 0.0],
            opacity: 0.6,
            contents: None,
            author: None,
            print: true,
        },
    )
    .unwrap();
    let out = mdoc.to_bytes().unwrap();
    let mdoc2 = pdf_mutate::MutableDoc::from_bytes(&out).unwrap();
    let annots = mdoc2.list_annotations();
    assert_eq!(annots.len(), 1);
    let got = &annots[0].rect_pt;
    assert!(
        (got.x0 - view_rect.x0).abs() < 0.5 && (got.y0 - view_rect.y0).abs() < 0.5,
        "roundtrip drift: wrote {view_rect:?}, read {got:?}"
    );

    // The highlight must also visually cover the MARKER text in the render.
    engine.reload_bytes(id, out).unwrap();
    let re = engine
        .render(RenderRequest {
            document: id,
            page: PageIndex(0),
            zoom: ZoomLevel(1.0),
            rotation: Rotation::R0,
        })
        .unwrap();
    let cx = ((view_rect.x0 + view_rect.x1) / 2.0 * re.width_px as f32 / sz.width_pt) as u32;
    let cy = ((view_rect.y0 + view_rect.y1) / 2.0 * re.height_px as f32 / sz.height_pt) as u32;
    let o = ((cy * re.width_px + cx) * 4) as usize;
    // Yellow-ish: red+green high relative to blue.
    assert!(
        re.rgba[o] > 150 && re.rgba[o + 1] > 130 && re.rgba[o + 2] < 150,
        "highlight pixel not yellow at text center: {:?}",
        &re.rgba[o..o + 3]
    );
    engine.close(id).unwrap();

    // ---- CropBox variant: MediaBox [0 0 612 792], CropBox [50 40 562 752].
    let mut crop_doc = Document::load_mem(&pdf_with_offset_mediabox()).unwrap();
    {
        let pid = crop_doc.page_iter().next().unwrap();
        let page = crop_doc.get_dictionary_mut(pid).unwrap();
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        page.set(
            "CropBox",
            Object::Array(vec![50.into(), 40.into(), 562.into(), 752.into()]),
        );
    }
    let mut crop_bytes = Vec::new();
    crop_doc.save_to(&mut crop_bytes).unwrap();
    let id2 = engine.open_bytes(crop_bytes).unwrap();
    let sz2 = engine.page_size(id2, PageIndex(0)).unwrap();
    assert!((sz2.width_pt - 512.0).abs() < 1.0, "crop width {}", sz2.width_pt);
    let runs2 = engine.page_text_layout(id2, PageIndex(0)).unwrap();
    let m2 = runs2.iter().find(|r| r.text.contains("MARKER")).expect("marker");
    // Text at user-space x=200 → crop-relative 150; baseline y=715, crop top
    // 752 → y0 ≈ 752-735 = 17.
    assert!(
        (m2.rect_pt.x0 - 150.0).abs() < 6.0,
        "crop x0: got {} want ~150",
        m2.rect_pt.x0
    );
    assert!(
        m2.rect_pt.y0 > 10.0 && m2.rect_pt.y0 < 30.0,
        "crop y0: got {} want ~17",
        m2.rect_pt.y0
    );
    engine.close(id2).unwrap();
}

#[test]
#[ignore] // run explicitly: probe only — PDFium single-threaded, run alone
fn probe_rotated_page_text_coords() {
    let engine = PdfiumEngine::new().expect("pdfium.dll");
    let mut doc = Document::load_mem(&pdf_with_offset_mediabox()).unwrap();
    {
        let pid = doc.page_iter().next().unwrap();
        let page = doc.get_dictionary_mut(pid).unwrap();
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        page.set("Rotate", Object::Integer(90));
    }
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let id = engine.open_bytes(bytes).unwrap();
    let sz = engine.page_size(id, PageIndex(0)).unwrap();
    println!("rotated page size: {}x{}", sz.width_pt, sz.height_pt);
    let runs = engine.page_text_layout(id, PageIndex(0)).unwrap();
    let m = runs.iter().find(|r| r.text.contains("MARKER")).unwrap();
    println!("MARKER rect on /Rotate 90 page: {:?}", m.rect_pt);
    // Where is the dark text in the rendered bitmap?
    let rendered = engine
        .render(RenderRequest {
            document: id,
            page: PageIndex(0),
            zoom: ZoomLevel(1.0),
            rotation: Rotation::R0,
        })
        .unwrap();
    let (w, h) = (rendered.width_px, rendered.height_px);
    let bb = dark_bbox(&rendered.rgba, w, h);
    println!("dark bbox in bitmap px (page {w}x{h}): {bb:?}");
    engine.close(id).unwrap();
}
