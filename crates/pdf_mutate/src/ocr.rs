//! Invisible OCR text layer injection.
//!
//! Takes word boxes recognised on a rendered page bitmap (Tesseract TSV,
//! pixel coordinates with top-left origin) and appends a content stream
//! that draws the words as invisible text (render mode 3) at the matching
//! positions in PDF user space. The page becomes searchable and the text
//! selectable/extractable while the scanned image stays untouched.
//!
//! Font sizing: each word's font size is its box height; the horizontal
//! scaling operator `Tz` stretches the Helvetica advance widths to the box
//! width so search-hit rectangles line up with the printed glyphs.

use std::io::Write;

use lopdf::{Document, dictionary};

use crate::overlay::{
    append_content_stream, ensure_own_resources, escape_pdf_string, register_alias,
    text_width_pt, to_winansi_bytes,
};
use crate::{MutateError, MutateResult, page_rotation, point_to_pdf_space, resolve_view_box};

/// One recognised word on a rendered page bitmap (top-left pixel coords).
#[derive(Clone, Debug)]
pub struct OcrWord {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Append an invisible text layer to a single 0-indexed page.
///
/// `img_w`/`img_h` are the pixel dimensions of the bitmap the words were
/// recognised on (the page rendered with its /Rotate applied, as pdfium
/// does); the mapping back into unrotated PDF user space happens here.
pub fn add_ocr_text_layer(
    doc: &mut Document,
    page_index: u32,
    words: &[OcrWord],
    img_w: u32,
    img_h: u32,
) -> MutateResult<()> {
    if img_w == 0 || img_h == 0 {
        return Err(MutateError::Parse("ocr: empty bitmap".into()));
    }
    let pages: Vec<lopdf::ObjectId> = doc.page_iter().collect();
    let total = pages.len() as u32;
    let Some(&page_id) = pages.get(page_index as usize) else {
        return Err(MutateError::PageOutOfRange(page_index, total));
    };

    let vb = resolve_view_box(doc, page_id);
    let rot = page_rotation(doc, page_id);
    let (box_w, box_h) = (vb[2] - vb[0], vb[3] - vb[1]);
    // Displayed (rotated) page size in points — the bitmap covers this.
    let (disp_w, disp_h) = if rot == 90 || rot == 270 {
        (box_h, box_w)
    } else {
        (box_w, box_h)
    };
    let scale_x = disp_w / img_w as f32;
    let scale_y = disp_h / img_h as f32;

    // Text baseline direction in PDF space for the display rotation.
    let (dir_c, dir_s) = match rot {
        90 => (0.0_f32, 1.0_f32),
        180 => (-1.0, 0.0),
        270 => (0.0, -1.0),
        _ => (1.0, 0.0),
    };

    let mut buf: Vec<u8> = Vec::with_capacity(words.len() * 64 + 32);
    buf.extend_from_slice(b"q\nBT\n3 Tr\n");
    let mut emitted = 0usize;
    for word in words {
        let text = word.text.trim();
        if text.is_empty() {
            continue;
        }
        let bytes = to_winansi_bytes(text);
        let font_size = (word.h * scale_y).max(1.0);
        let natural_w = text_width_pt(&bytes, font_size);
        if natural_w <= 0.01 {
            continue;
        }
        let target_w = (word.w * scale_x).max(0.1);
        let tz = (target_w / natural_w * 100.0).clamp(5.0, 1000.0);
        // Baseline: slightly above the box bottom to leave room for
        // descenders (Helvetica descent ≈ 21% of the em square).
        let bx = word.x * scale_x;
        let by = (word.y + word.h * 0.8) * scale_y;
        let (px, py) = point_to_pdf_space(bx, by, vb, rot);
        let _ = write!(
            &mut buf,
            "/FOCR {font_size:.2} Tf\n{tz:.1} Tz\n{dir_c:.4} {dir_s:.4} {:.4} {dir_c:.4} {px:.2} {py:.2} Tm\n",
            -dir_s,
        );
        buf.extend_from_slice(&escape_pdf_string(&bytes));
        buf.extend_from_slice(b" Tj\n");
        emitted += 1;
    }
    buf.extend_from_slice(b"ET\nQ\n");
    if emitted == 0 {
        return Ok(());
    }

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let rid = ensure_own_resources(doc, page_id)?;
    register_alias(doc, rid, b"Font", b"FOCR", font_id)?;
    append_content_stream(doc, page_id, buf)?;
    Ok(())
}
