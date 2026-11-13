//! Read-side counterpart to `annotations.rs`. Walks every page's /Annots and
//! returns a flat `Vec<AnnotationInfo>` with rects converted to our top-origin
//! convention. Used by the comment panel and XFDF export.

use lopdf::{Document, Object, ObjectId};
use pdf_types::{AnnotationInfo, FreeTextStyle, RectPt};


fn resolve_obj<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Object> {
    match obj {
        Object::Reference(r) => doc.get_object(*r).ok(),
        other => Some(other),
    }
}

fn obj_as_array<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Vec<Object>> {
    let resolved = resolve_obj(doc, obj)?;
    resolved.as_array().ok()
}

fn obj_to_f32(obj: &Object) -> Option<f32> {
    obj.as_float()
        .ok()
        .or_else(|| obj.as_i64().ok().map(|i| i as f32))
}

/// Decode a PDF text string. Hex strings starting with `FE FF` are UTF-16BE;
/// everything else is treated as Latin-1 (good enough for PDFDocEncoding's
/// ASCII-compatible region — non-ASCII custom glyphs degrade to ?).
pub(crate) fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let mut units: Vec<u16> = Vec::with_capacity((bytes.len() - 2) / 2);
        let mut i = 2;
        while i + 1 < bytes.len() {
            units.push(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        String::from_utf16_lossy(&units)
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let mut units: Vec<u16> = Vec::with_capacity((bytes.len() - 2) / 2);
        let mut i = 2;
        while i + 1 < bytes.len() {
            units.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn read_string_field(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let obj = dict.get(key).ok()?;
    let resolved = resolve_obj(doc, obj)?;
    if let Ok(s) = resolved.as_str() {
        let dec = decode_pdf_string(s);
        if dec.is_empty() {
            None
        } else {
            Some(dec)
        }
    } else {
        None
    }
}

fn read_color(doc: &Document, dict: &lopdf::Dictionary) -> Option<[f32; 3]> {
    let obj = dict.get(b"C").ok()?;
    let arr = obj_as_array(doc, obj)?;
    match arr.len() {
        1 => {
            let g = obj_to_f32(&arr[0])?;
            Some([g, g, g])
        }
        3 => Some([
            obj_to_f32(&arr[0])?,
            obj_to_f32(&arr[1])?,
            obj_to_f32(&arr[2])?,
        ]),
        4 => {
            let c = obj_to_f32(&arr[0])?;
            let m = obj_to_f32(&arr[1])?;
            let y = obj_to_f32(&arr[2])?;
            let k = obj_to_f32(&arr[3])?;
            let r = (1.0 - c) * (1.0 - k);
            let g = (1.0 - m) * (1.0 - k);
            let b = (1.0 - y) * (1.0 - k);
            Some([r, g, b])
        }
        _ => None,
    }
}

/// Inverse of `crate::point_to_pdf_space`: PDF user space → displayed
/// top-origin space (view-box offset removed, page /Rotate applied) — the
/// space the annotation writers take and the pdfium engine reports in.
fn pdf_to_view(px: f32, py: f32, vb: [f32; 4], rot: i32) -> (f32, f32) {
    let w = vb[2] - vb[0];
    let h = vb[3] - vb[1];
    let (xu, yu) = (px - vb[0], vb[3] - py);
    match rot {
        90 => (h - yu, xu),
        180 => (w - xu, h - yu),
        270 => (yu, w - xu),
        _ => (xu, yu),
    }
}

fn read_rect_top(doc: &Document, dict: &lopdf::Dictionary, vb: [f32; 4], rot: i32) -> Option<RectPt> {
    let obj = dict.get(b"Rect").ok()?;
    let arr = obj_as_array(doc, obj)?;
    if arr.len() != 4 {
        return None;
    }
    let llx = obj_to_f32(&arr[0])?;
    let lly = obj_to_f32(&arr[1])?;
    let urx = obj_to_f32(&arr[2])?;
    let ury = obj_to_f32(&arr[3])?;
    let a = pdf_to_view(llx, lly, vb, rot);
    let b = pdf_to_view(urx, ury, vb, rot);
    Some(RectPt {
        x0: a.0.min(b.0),
        y0: a.1.min(b.1),
        x1: a.0.max(b.0),
        y1: a.1.max(b.1),
    })
}

fn page_view_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    crate::resolve_view_box(doc, page_id)
}

fn fallback_id(oid: ObjectId) -> String {
    format!("obj-{}-{}", oid.0, oid.1)
}

/// Styling of a /FreeText annotation: size and text color from the /DA
/// string, the rest from the private style dictionary this app writes.
/// Boxes without one (older versions, other writers) get the old defaults:
/// Helvetica on a white background.
fn read_freetext_style(doc: &Document, dict: &lopdf::Dictionary) -> FreeTextStyle {
    let mut style = FreeTextStyle {
        font_size: 14.0,
        text_color: [0.0, 0.0, 0.0],
        bg_color: Some([1.0, 1.0, 1.0]),
        font_family: "Helv".into(),
        bold: false,
        italic: false,
        rotation_quadrant: 0,
    };
    // "r g b rg /Font size Tf" (gray "g" and other orders tolerated).
    if let Some(da) = read_string_field(doc, dict, b"DA") {
        let toks: Vec<&str> = da.split_whitespace().collect();
        let num = |i: usize| toks.get(i).and_then(|t| t.parse::<f32>().ok());
        for (i, tok) in toks.iter().enumerate() {
            match *tok {
                "Tf" if i >= 1 => {
                    if let Some(size) = num(i - 1).filter(|s| *s > 0.0) {
                        style.font_size = size;
                    }
                }
                "rg" if i >= 3 => {
                    if let (Some(r), Some(g), Some(b)) = (num(i - 3), num(i - 2), num(i - 1)) {
                        style.text_color = [r, g, b];
                    }
                }
                "g" if i >= 1 => {
                    if let Some(v) = num(i - 1) {
                        style.text_color = [v, v, v];
                    }
                }
                _ => {}
            }
        }
    }
    let private = dict
        .get(crate::annotations::FREETEXT_STYLE_KEY)
        .ok()
        .and_then(|o| resolve_obj(doc, o))
        .and_then(|o| o.as_dict().ok());
    if let Some(p) = private {
        if let Some(name) = p.get(b"Font").ok().and_then(|o| o.as_name().ok()) {
            style.font_family = String::from_utf8_lossy(name).to_string();
        }
        let flag = |key: &[u8]| p.get(key).ok().and_then(|o| o.as_bool().ok()).unwrap_or(false);
        style.bold = flag(b"Bold");
        style.italic = flag(b"Italic");
        style.rotation_quadrant =
            p.get(b"Rot").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0).rem_euclid(4) as u8;
        style.bg_color = p
            .get(b"BG")
            .ok()
            .and_then(|o| obj_as_array(doc, o))
            .filter(|a| a.len() == 3)
            .and_then(|a| Some([obj_to_f32(&a[0])?, obj_to_f32(&a[1])?, obj_to_f32(&a[2])?]));
    }
    style
}

pub fn list_annotations(doc: &Document) -> Vec<AnnotationInfo> {
    let mut out: Vec<AnnotationInfo> = Vec::new();
    for (page_idx, page_id) in doc.page_iter().enumerate() {
        let Ok(page_dict) = doc.get_dictionary(page_id) else {
            continue;
        };
        let Ok(annots_obj) = page_dict.get(b"Annots") else {
            continue;
        };
        let Some(annots) = obj_as_array(doc, annots_obj) else {
            continue;
        };
        let vb = page_view_box(doc, page_id);
        let rot = crate::page_rotation(doc, page_id);
        for entry in annots {
            let (annot_oid, dict) = match entry {
                Object::Reference(r) => match doc.get_dictionary(*r) {
                    Ok(d) => (*r, d),
                    Err(_) => continue,
                },
                Object::Dictionary(d) => ((0u32, 0u16), d),
                _ => continue,
            };
            let subtype = match dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) {
                Some(name) => String::from_utf8_lossy(name).to_string(),
                None => continue,
            };
            let rect_pt = match read_rect_top(doc, dict, vb, rot) {
                Some(r) => r,
                None => continue,
            };
            let color = read_color(doc, dict);
            let opacity = dict
                .get(b"CA")
                .ok()
                .and_then(|o| obj_to_f32(o))
                .unwrap_or(1.0);
            let flags = dict
                .get(b"F")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            let print = (flags & 4) != 0;
            let contents = read_string_field(doc, dict, b"Contents");
            let author = read_string_field(doc, dict, b"T");
            let modified = read_string_field(doc, dict, b"M");
            let icon = dict
                .get(b"Name")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).to_string());
            let id = read_string_field(doc, dict, b"NM").unwrap_or_else(|| fallback_id(annot_oid));

            let freetext = (subtype == "FreeText").then(|| read_freetext_style(doc, dict));

            out.push(AnnotationInfo {
                freetext,
                id,
                page: page_idx as u32,
                subtype,
                rect_pt,
                color,
                opacity,
                print,
                contents,
                author,
                modified,
                icon,
            });
        }
    }
    out
}
