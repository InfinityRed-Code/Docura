//! PDF annotation writing.
//!
//! Annotations live on the page dictionary's `/Annots` array, separate from
//! the content stream. PDF 1.7 §12.5.6 defines the families:
//! - text-markup (highlight, underline, strikeout) using /QuadPoints
//! - shape (square, circle, line) using /Rect, /BS, /IC
//! - free text, ink, stamp — separate functions in this module
//!
//! The outward API takes `RectPt` and points in our top-origin convention;
//! conversion to PDF-native bottom origin happens here using each page's
//! MediaBox height.

use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};
use pdf_types::RectPt;

use crate::{MutateError, MutateResult};

#[derive(Clone, Copy, Debug)]
pub enum MarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

impl MarkupKind {
    fn subtype(self) -> &'static str {
        match self {
            MarkupKind::Highlight => "Highlight",
            MarkupKind::Underline => "Underline",
            MarkupKind::StrikeOut => "StrikeOut",
            MarkupKind::Squiggly => "Squiggly",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MarkupAnnotation {
    pub kind: MarkupKind,
    /// Rectangles in our top-origin RectPt convention. Each rect becomes one
    /// quadrilateral (4 points) in /QuadPoints.
    pub rects: Vec<RectPt>,
    /// RGB color in 0..1.
    pub color: [f32; 3],
    /// Stroke/fill opacity 0..1.
    pub opacity: f32,
    /// Optional /Contents text (popup comment).
    pub contents: Option<String>,
    /// Optional author / title bar of the popup.
    pub author: Option<String>,
    /// PDF /F bit 2 (Print). True = annotation included in print output.
    pub print: bool,
}

// PDF 1.7 §12.5.3 annotation flags. We only toggle Print (bit 2 = value 4).
fn annot_flags(print: bool) -> i64 {
    if print { 4 } else { 0 }
}

fn page_view(doc: &Document, page_id: ObjectId) -> ([f32; 4], i32) {
    (crate::resolve_view_box(doc, page_id), crate::page_rotation(doc, page_id))
}

fn pdf_string_utf16be(s: &str) -> Object {
    let mut bytes: Vec<u8> = Vec::with_capacity(2 + s.len() * 2);
    bytes.push(0xFE);
    bytes.push(0xFF);
    for u in s.encode_utf16() {
        bytes.extend_from_slice(&u.to_be_bytes());
    }
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

fn now_pdf_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Crude UTC; PDF date format: D:YYYYMMDDHHmmSSZ
    let (y, mo, d, h, mi, s) = epoch_to_utc(secs);
    format!("D:{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}Z")
}

fn epoch_to_utc(mut t: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (t % 60) as u32;
    t /= 60;
    let mi = (t % 60) as u32;
    t /= 60;
    let h = (t % 24) as u32;
    t /= 24;
    let mut days = t as i64;
    let mut y: i64 = 1970;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let mdays = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut mo = 0usize;
    while mo < 12 && days >= mdays[mo] {
        days -= mdays[mo];
        mo += 1;
    }
    (y as u32, (mo + 1) as u32, (days + 1) as u32, h, mi, s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn random_nm() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("rustpdf-{nanos:x}")
}

fn rect_native(r: &RectPt, view: ([f32; 4], i32)) -> (f32, f32, f32, f32) {
    // Top-origin view space -> PDF user space. Returns (llx, lly, urx, ury).
    // The view box (CropBox intersect MediaBox) may not start at (0,0).
    let (a, b) = (
        crate::point_to_pdf_space(r.x0, r.y0, view.0, view.1),
        crate::point_to_pdf_space(r.x1, r.y1, view.0, view.1),
    );
    let llx = a.0.min(b.0);
    let urx = a.0.max(b.0);
    let lly = a.1.min(b.1);
    let ury = a.1.max(b.1);
    (llx, lly, urx, ury)
}

/// Add a single text-markup annotation (covering all `rects`) to the given page.
/// Returns the new annotation's object id.
pub fn add_markup_annotation(
    doc: &mut Document,
    page_index: u32,
    annot: &MarkupAnnotation,
) -> MutateResult<ObjectId> {
    if annot.rects.is_empty() {
        return Err(MutateError::Parse("annotation: empty rects".into()));
    }
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let page_id = *pages
        .get(&(page_index + 1))
        .ok_or(MutateError::PageOutOfRange(page_index, total))?;
    let view = page_view(doc, page_id);

    // Collect quadpoints + bounding rect.
    let mut quad: Vec<Object> = Vec::with_capacity(annot.rects.len() * 8);
    let mut rects_native: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(annot.rects.len());
    let mut bbox: Option<(f32, f32, f32, f32)> = None;
    for r in &annot.rects {
        let (llx, lly, urx, ury) = rect_native(r, view);
        rects_native.push((llx, lly, urx, ury));
        // PDF spec QuadPoints order for highlight (PDF 1.7 §12.5.6.10):
        // [x1 y1 x2 y2 x3 y3 x4 y4] = top-left, top-right, bottom-left, bottom-right.
        // Acrobat interprets in that order; some viewers expect bottom-first,
        // but Acrobat / PDFium / Foxit accept the upper-line-first form.
        for v in [
            llx, ury, urx, ury, llx, lly, urx, lly,
        ] {
            quad.push(Object::Real(v));
        }
        let bb = bbox.get_or_insert((llx, lly, urx, ury));
        bb.0 = bb.0.min(llx);
        bb.1 = bb.1.min(lly);
        bb.2 = bb.2.max(urx);
        bb.3 = bb.3.max(ury);
    }
    let (rllx, rlly, rurx, rury) = bbox.unwrap();

    let color = [
        annot.color[0].clamp(0.0, 1.0),
        annot.color[1].clamp(0.0, 1.0),
        annot.color[2].clamp(0.0, 1.0),
    ];
    let opacity = annot.opacity.clamp(0.0, 1.0);

    let mut adict = Dictionary::new();
    adict.set("Type", Object::Name(b"Annot".to_vec()));
    adict.set("Subtype", Object::Name(annot.kind.subtype().as_bytes().to_vec()));
    adict.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rllx),
            Object::Real(rlly),
            Object::Real(rurx),
            Object::Real(rury),
        ]),
    );
    adict.set("QuadPoints", Object::Array(quad));
    adict.set(
        "C",
        Object::Array(vec![
            Object::Real(color[0]),
            Object::Real(color[1]),
            Object::Real(color[2]),
        ]),
    );
    adict.set("CA", Object::Real(opacity));
    adict.set("F", Object::Integer(annot_flags(annot.print)));
    adict.set(
        "NM",
        Object::String(random_nm().into_bytes(), lopdf::StringFormat::Literal),
    );
    adict.set(
        "M",
        Object::String(now_pdf_date().into_bytes(), lopdf::StringFormat::Literal),
    );
    if let Some(c) = &annot.contents {
        adict.set("Contents", pdf_string_utf16be(c));
    }
    if let Some(a) = &annot.author {
        adict.set("T", pdf_string_utf16be(a));
    }
    adict.set("P", Object::Reference(page_id));

    // Explicit appearance stream (/AP /N). Relying on the viewer to synthesise
    // one leaves highlights flat (covering text) and renders inconsistently
    // across viewers and when printed/flattened. Highlights paint with the
    // Multiply blend mode so text shows through like a real highlighter.
    let w = rurx - rllx;
    let h = rury - rlly;
    let local: Vec<(f32, f32, f32, f32)> = rects_native
        .iter()
        .map(|&(a, b, c, d)| (a - rllx, b - rlly, c - rllx, d - rlly))
        .collect();
    let content = build_markup_appearance(annot.kind, &local, color);
    let resources = if matches!(annot.kind, MarkupKind::Highlight) {
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("BM", Object::Name(b"Multiply".to_vec()));
        gs.set("ca", Object::Real(opacity));
        let mut gsd = Dictionary::new();
        gsd.set("GShl", Object::Dictionary(gs));
        let mut res = Dictionary::new();
        res.set("ExtGState", Object::Dictionary(gsd));
        Some(res)
    } else {
        None
    };
    let form_id = make_form_xobject(doc, w, h, content, resources);
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    adict.set("AP", Object::Dictionary(ap));

    let annot_id = doc.add_object(Object::Dictionary(adict));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

/// Build the `/AP /N` content stream for a text-markup annotation. Coordinates
/// are local to the appearance BBox (page coords minus the bbox origin).
fn build_markup_appearance(
    kind: MarkupKind,
    quads: &[(f32, f32, f32, f32)],
    rgb: [f32; 3],
) -> Vec<u8> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::with_capacity(128);
    let [r, g, b] = rgb;
    match kind {
        MarkupKind::Highlight => {
            // Multiply blend via the /GShl ExtGState set on the form resources.
            let _ = write!(&mut buf, "q /GShl gs {r:.3} {g:.3} {b:.3} rg\n");
            for &(x0, y0, x1, y1) in quads {
                let qw = (x1 - x0).max(0.0);
                let qh = (y1 - y0).max(0.0);
                let _ = write!(&mut buf, "{x0:.3} {y0:.3} {qw:.3} {qh:.3} re\n");
            }
            buf.extend_from_slice(b"f Q\n");
        }
        MarkupKind::Underline | MarkupKind::StrikeOut => {
            let _ = write!(&mut buf, "q {r:.3} {g:.3} {b:.3} RG\n");
            for &(x0, y0, x1, y1) in quads {
                let qh = (y1 - y0).max(1.0);
                let lw = (qh * 0.07).max(0.6);
                let y = match kind {
                    // Underline sits just above the baseline; strikeout crosses
                    // the vertical middle of the text quad.
                    MarkupKind::Underline => y0 + qh * 0.08,
                    _ => y0 + qh * 0.5,
                };
                let _ = write!(&mut buf, "{lw:.3} w {x0:.3} {y:.3} m {x1:.3} {y:.3} l S\n");
            }
            buf.extend_from_slice(b"Q\n");
        }
        MarkupKind::Squiggly => {
            // A wavy underline drawn as a triangle wave just above the baseline.
            let _ = write!(&mut buf, "q {r:.3} {g:.3} {b:.3} RG\n");
            for &(x0, y0, x1, y1) in quads {
                let qh = (y1 - y0).max(1.0);
                let lw = (qh * 0.06).max(0.5);
                let amp = (qh * 0.09).max(1.0);
                let base = y0 + qh * 0.05;
                let step = (amp * 1.6).max(2.5);
                let _ = write!(&mut buf, "{lw:.3} w {x0:.3} {base:.3} m\n");
                let mut x = x0;
                let mut up = true;
                while x < x1 - 0.01 {
                    let nx = (x + step).min(x1);
                    let y = if up { base + amp } else { base };
                    let _ = write!(&mut buf, "{nx:.3} {y:.3} l\n");
                    x = nx;
                    up = !up;
                }
                buf.extend_from_slice(b"S\n");
            }
            buf.extend_from_slice(b"Q\n");
        }
    }
    buf
}

fn append_annot_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    annot_id: ObjectId,
) -> MutateResult<()> {
    let existing = doc
        .get_dictionary(page_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Annots")
        .ok()
        .cloned();

    match existing {
        None => {
            if let Ok(pd) = doc.get_dictionary_mut(page_id) {
                pd.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
            }
        }
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(annot_id));
            if let Ok(pd) = doc.get_dictionary_mut(page_id) {
                pd.set("Annots", Object::Array(arr));
            }
        }
        Some(Object::Reference(rid)) => {
            if let Ok(obj) = doc.get_object_mut(rid) {
                if let Object::Array(arr) = obj {
                    arr.push(Object::Reference(annot_id));
                }
            }
        }
        Some(_) => {
            if let Ok(pd) = doc.get_dictionary_mut(page_id) {
                pd.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
            }
        }
    }
    Ok(())
}

fn page_id_for(doc: &Document, page_index: u32) -> MutateResult<ObjectId> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    pages
        .get(&(page_index + 1))
        .copied()
        .ok_or(MutateError::PageOutOfRange(page_index, total))
}

/// The stable id read::list_annotations reports for an annotation: its `NM`
/// name string, falling back to `obj-<num>-<gen>`. Must mirror that logic so
/// the frontend can address an annotation for deletion.
fn annot_id_string(doc: &Document, dict: &Dictionary, oid: Option<ObjectId>) -> String {
    // Same resolution + text decoding as read::list_annotations (indirect
    // /NM, UTF-16BE / PDFDocEncoding), or non-ASCII ids never match.
    let nm = dict.get(b"NM").ok().and_then(|o| match o {
        Object::Reference(r) => doc.get_object(*r).ok(),
        other => Some(other),
    });
    if let Some(Ok(s)) = nm.map(|o| o.as_str()) {
        let dec = crate::read::decode_pdf_string(s);
        if !dec.is_empty() {
            return dec;
        }
    }
    match oid {
        Some(o) => format!("obj-{}-{}", o.0, o.1),
        None => "obj-0-0".to_string(),
    }
}

/// Remove the annotation whose id (see [`annot_id_string`]) matches `id` from
/// the given 0-indexed page. Returns whether one was removed. The `Annots`
/// array may be stored inline on the page or as an indirect array object.
pub fn delete_annotation(doc: &mut Document, page_index: u32, id: &str) -> MutateResult<bool> {
    let page_id = page_id_for(doc, page_index)?;
    let annots_obj = doc
        .get_dictionary(page_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Annots")
        .ok()
        .cloned();
    let Some(annots_obj) = annots_obj else {
        return Ok(false);
    };
    let (mut arr, target_ref): (Vec<Object>, Option<ObjectId>) = match annots_obj {
        Object::Array(a) => (a, None),
        Object::Reference(r) => match doc.get_object(r) {
            Ok(Object::Array(a)) => (a.clone(), Some(r)),
            _ => return Ok(false),
        },
        _ => return Ok(false),
    };

    let mut remove_at: Option<usize> = None;
    let mut remove_oid: Option<ObjectId> = None;
    for (i, entry) in arr.iter().enumerate() {
        let (oid, dict) = match entry {
            Object::Reference(r) => match doc.get_dictionary(*r) {
                Ok(d) => (Some(*r), d),
                Err(_) => continue,
            },
            Object::Dictionary(d) => (None, d),
            _ => continue,
        };
        if annot_id_string(doc, dict, oid) == id {
            remove_at = Some(i);
            remove_oid = oid;
            break;
        }
    }
    let Some(i) = remove_at else {
        return Ok(false);
    };
    arr.remove(i);

    match target_ref {
        Some(r) => {
            if let Ok(obj) = doc.get_object_mut(r) {
                *obj = Object::Array(arr);
            }
        }
        None => {
            if let Ok(pd) = doc.get_dictionary_mut(page_id) {
                pd.set("Annots", Object::Array(arr));
            }
        }
    }
    if let Some(oid) = remove_oid {
        doc.objects.remove(&oid);
    }
    Ok(true)
}

fn base_annot_dict(
    page_id: ObjectId,
    subtype: &str,
    rect_native: (f32, f32, f32, f32),
    color_rgb: [f32; 3],
    opacity: f32,
    contents: Option<&str>,
    author: Option<&str>,
    print: bool,
) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(subtype.as_bytes().to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect_native.0),
            Object::Real(rect_native.1),
            Object::Real(rect_native.2),
            Object::Real(rect_native.3),
        ]),
    );
    d.set(
        "C",
        Object::Array(vec![
            Object::Real(color_rgb[0].clamp(0.0, 1.0)),
            Object::Real(color_rgb[1].clamp(0.0, 1.0)),
            Object::Real(color_rgb[2].clamp(0.0, 1.0)),
        ]),
    );
    d.set("CA", Object::Real(opacity.clamp(0.0, 1.0)));
    d.set("F", Object::Integer(annot_flags(print)));
    d.set(
        "NM",
        Object::String(random_nm().into_bytes(), lopdf::StringFormat::Literal),
    );
    d.set(
        "M",
        Object::String(now_pdf_date().into_bytes(), lopdf::StringFormat::Literal),
    );
    if let Some(c) = contents {
        d.set("Contents", pdf_string_utf16be(c));
    }
    if let Some(a) = author {
        d.set("T", pdf_string_utf16be(a));
    }
    d.set("P", Object::Reference(page_id));
    d
}

// ----------------- Form XObject helper -----------------

fn make_form_xobject(
    doc: &mut Document,
    width: f32,
    height: f32,
    content: Vec<u8>,
    resources: Option<Dictionary>,
) -> ObjectId {
    use lopdf::Stream;
    let mut form_dict = Dictionary::new();
    form_dict.set("Type", Object::Name(b"XObject".to_vec()));
    form_dict.set("Subtype", Object::Name(b"Form".to_vec()));
    form_dict.set("FormType", Object::Integer(1));
    form_dict.set(
        "BBox",
        Object::Array(vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(width.max(0.0)),
            Object::Real(height.max(0.0)),
        ]),
    );
    form_dict.set(
        "Resources",
        Object::Dictionary(resources.unwrap_or_default()),
    );
    doc.add_object(Object::Stream(Stream::new(form_dict, content)))
}

// ----------------- Shape annotations -----------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    None,
    OpenArrow,
    ClosedArrow,
}

impl LineEnding {
    fn name(self) -> &'static [u8] {
        match self {
            LineEnding::None => b"None",
            LineEnding::OpenArrow => b"OpenArrow",
            LineEnding::ClosedArrow => b"ClosedArrow",
        }
    }

    /// The ending for a PDF / XFDF line-ending name; styles this writer
    /// cannot draw (Circle, Square, …) come back as `None`.
    pub fn from_name(name: &str) -> LineEnding {
        match name {
            "OpenArrow" => LineEnding::OpenArrow,
            "ClosedArrow" => LineEnding::ClosedArrow,
            _ => LineEnding::None,
        }
    }
}

/// Path operators for a line ending whose tip is at `tip`, pointing along
/// `dir`: stroked (open arrow) or filled and stroked (closed arrow).
fn line_ending_ops(tip: (f32, f32), dir: (f32, f32), kind: LineEnding, width: f32) -> String {
    let head_size = (width * 6.0).max(8.0);
    let len = dir.0.hypot(dir.1).max(1e-6);
    let (ux, uy) = (dir.0 / len, dir.1 / len);
    // Perpendicular.
    let (perp_x, perp_y) = (-uy, ux);
    let (base_x, base_y) = (tip.0 - ux * head_size, tip.1 - uy * head_size);
    let half = head_size * 0.45;
    let (p1x, p1y) = (base_x + perp_x * half, base_y + perp_y * half);
    let (p2x, p2y) = (base_x - perp_x * half, base_y - perp_y * half);
    let (tx, ty) = tip;
    match kind {
        LineEnding::None => String::new(),
        LineEnding::OpenArrow => {
            format!("{p1x:.3} {p1y:.3} m {tx:.3} {ty:.3} l {p2x:.3} {p2y:.3} l S\n")
        }
        LineEnding::ClosedArrow => {
            format!("{tx:.3} {ty:.3} m {p1x:.3} {p1y:.3} l {p2x:.3} {p2y:.3} l h B\n")
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeKind {
    /// Rectangular outline (PDF /Square).
    Square,
    /// Elliptical outline (PDF /Circle).
    Circle,
    /// Straight line between (x0, y0) and (x1, y1) in top-origin RectPt convention.
    /// Stored in `ShapeAnnotation::rect` as x0, y0 = first point; x1, y1 = second point.
    /// Optional arrow heads at start/end.
    Line {
        arrow_start: LineEnding,
        arrow_end: LineEnding,
    },
    /// Closed outline through `ShapeAnnotation::vertices` (PDF /Polygon):
    /// triangles, stars, speech bubbles, free polygons. `cloudy` draws the
    /// border as a row of arcs — Acrobat's cloud (/BE /S /C, /IT
    /// /PolygonCloud).
    Polygon { cloudy: bool },
    /// Open path through `ShapeAnnotation::vertices` (PDF /PolyLine), with
    /// optional endings at the first and the last point.
    PolyLine { start: LineEnding, end: LineEnding },
}

#[derive(Clone, Debug)]
pub struct ShapeAnnotation {
    pub kind: ShapeKind,
    /// Top-origin RectPt. For Square/Circle: bounding rect. For Line: x0/y0 = start, x1/y1 = end.
    /// Unused for Polygon / PolyLine.
    pub rect: RectPt,
    /// Polygon / PolyLine corner points, top-origin page points.
    pub vertices: Vec<(f32, f32)>,
    /// /Subj — the shape's name as shown in comment lists ("Stern").
    pub subject: Option<String>,
    pub stroke_color: [f32; 3],
    pub fill_color: Option<[f32; 3]>,
    pub border_width: f32,
    pub opacity: f32,
    pub contents: Option<String>,
    pub author: Option<String>,
    pub print: bool,
}

pub fn add_shape_annotation(
    doc: &mut Document,
    page_index: u32,
    annot: &ShapeAnnotation,
) -> MutateResult<ObjectId> {
    let page_id = page_id_for(doc, page_index)?;
    let view = page_view(doc, page_id);
    let bw = annot.border_width.max(0.0);

    if let ShapeKind::Polygon { .. } | ShapeKind::PolyLine { .. } = annot.kind {
        return add_path_shape(doc, page_id, view, annot);
    }

    let (subtype, rect_native, line_endpoints) = match annot.kind {
        ShapeKind::Square => {
            let (llx, lly, urx, ury) = rect_native(&annot.rect, view);
            ("Square", (llx, lly, urx, ury), None)
        }
        ShapeKind::Circle => {
            let (llx, lly, urx, ury) = rect_native(&annot.rect, view);
            ("Circle", (llx, lly, urx, ury), None)
        }
        ShapeKind::Line { .. } => {
            // Endpoints in PDF native coords.
            let (p0x, p0y) = crate::point_to_pdf_space(annot.rect.x0, annot.rect.y0, view.0, view.1);
                        let (p1x, p1y) = crate::point_to_pdf_space(annot.rect.x1, annot.rect.y1, view.0, view.1);
                        // /Rect must enclose the annotation including arrow heads. Pad by border width.
            let pad = bw.max(8.0) + 4.0;
            let llx = p0x.min(p1x) - pad;
            let lly = p0y.min(p1y) - pad;
            let urx = p0x.max(p1x) + pad;
            let ury = p0y.max(p1y) + pad;
            ("Line", (llx, lly, urx, ury), Some((p0x, p0y, p1x, p1y)))
        }
        ShapeKind::Polygon { .. } | ShapeKind::PolyLine { .. } => unreachable!("handled above"),
    };

    let mut d = base_annot_dict(
        page_id,
        subtype,
        rect_native,
        annot.stroke_color,
        annot.opacity,
        annot.contents.as_deref(),
        annot.author.as_deref(),
        annot.print,
    );

    // Border style.
    let mut bs = Dictionary::new();
    bs.set("Type", Object::Name(b"Border".to_vec()));
    bs.set("W", Object::Real(bw));
    bs.set("S", Object::Name(b"S".to_vec())); // solid
    d.set("BS", Object::Dictionary(bs));

    if let Some(fill) = annot.fill_color {
        d.set(
            "IC",
            Object::Array(vec![
                Object::Real(fill[0].clamp(0.0, 1.0)),
                Object::Real(fill[1].clamp(0.0, 1.0)),
                Object::Real(fill[2].clamp(0.0, 1.0)),
            ]),
        );
    }

    if let ShapeKind::Line { arrow_start, arrow_end } = annot.kind {
        let (p0x, p0y, p1x, p1y) = line_endpoints.unwrap();
        d.set(
            "L",
            Object::Array(vec![
                Object::Real(p0x),
                Object::Real(p0y),
                Object::Real(p1x),
                Object::Real(p1y),
            ]),
        );
        d.set(
            "LE",
            Object::Array(vec![
                Object::Name(arrow_start.name().to_vec()),
                Object::Name(arrow_end.name().to_vec()),
            ]),
        );

        // Build /AP /N — PDFium does not auto-generate appearance for /Line.
        let (llx, lly, urx, ury) = rect_native;
        let w = urx - llx;
        let h = ury - lly;
        let lx0 = p0x - llx;
        let ly0 = p0y - lly;
        let lx1 = p1x - llx;
        let ly1 = p1y - lly;
        let bw_eff = bw.max(0.5);
        let stroke = annot.stroke_color;
        let content = build_line_appearance(
            lx0, ly0, lx1, ly1, bw_eff, stroke, arrow_start, arrow_end,
        );
        let form_id = make_form_xobject(doc, w, h, content, None);
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(form_id));
        d.set("AP", Object::Dictionary(ap));
    } else {
        // Square / Circle: optional /AP improves cross-viewer compat. PDFium
        // renders both natively, so for now we leave them without /AP.
    }

    if let Some(s) = &annot.subject {
        d.set("Subj", pdf_string_utf16be(s));
    }
    let annot_id = doc.add_object(Object::Dictionary(d));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

fn color_array(c: [f32; 3]) -> Object {
    Object::Array(c.iter().map(|v| Object::Real(v.clamp(0.0, 1.0))).collect())
}

/// Bump radius of a cloud border: proportional to the shape, within the
/// range Acrobat's intensity 1 produces.
fn cloud_radius(pts: &[(f32, f32)]) -> f32 {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    ((x1 - x0).min(y1 - y0) / 10.0).clamp(3.0, 12.0)
}

/// /Polygon or /PolyLine through `annot.vertices`, with an explicit
/// appearance stream (PDFium does not generate one for these subtypes).
fn add_path_shape(
    doc: &mut Document,
    page_id: ObjectId,
    view: ([f32; 4], i32),
    annot: &ShapeAnnotation,
) -> MutateResult<ObjectId> {
    let closed = matches!(annot.kind, ShapeKind::Polygon { .. });
    let cloudy = matches!(annot.kind, ShapeKind::Polygon { cloudy: true });
    let min = if closed { 3 } else { 2 };
    if annot.vertices.len() < min {
        return Err(MutateError::Parse(format!("shape: needs at least {min} vertices")));
    }
    let bw = annot.border_width.max(0.0);
    let pts: Vec<(f32, f32)> = annot
        .vertices
        .iter()
        .map(|&(x, y)| crate::point_to_pdf_space(x, y, view.0, view.1))
        .collect();
    let radius = if cloudy { cloud_radius(&pts) } else { 0.0 };
    let endings = match annot.kind {
        ShapeKind::PolyLine { start, end } => (start, end),
        _ => (LineEnding::None, LineEnding::None),
    };
    let has_endings = endings != (LineEnding::None, LineEnding::None);
    // /Rect encloses the outline, the stroke, the bumps (cloud) and the
    // line endings.
    let mut pad = bw / 2.0 + radius + 2.0;
    if has_endings {
        pad = pad.max((bw * 6.0).max(8.0) + bw + 2.0);
    }
    let llx = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min) - pad;
    let lly = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min) - pad;
    let urx = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max) + pad;
    let ury = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max) + pad;

    let mut d = base_annot_dict(
        page_id,
        if closed { "Polygon" } else { "PolyLine" },
        (llx, lly, urx, ury),
        annot.stroke_color,
        annot.opacity,
        annot.contents.as_deref(),
        annot.author.as_deref(),
        annot.print,
    );
    d.set(
        "Vertices",
        Object::Array(pts.iter().flat_map(|&(x, y)| [Object::Real(x), Object::Real(y)]).collect()),
    );
    d.set("BS", dictionary! { "Type" => "Border", "W" => bw, "S" => "S" });
    let fill = annot.fill_color.filter(|_| closed);
    if let Some(fill) = fill {
        d.set("IC", color_array(fill));
    }
    if cloudy {
        d.set("BE", dictionary! { "S" => "C", "I" => 1 });
        d.set("IT", Object::Name(b"PolygonCloud".to_vec()));
    }
    if has_endings {
        d.set(
            "LE",
            Object::Array(vec![
                Object::Name(endings.0.name().to_vec()),
                Object::Name(endings.1.name().to_vec()),
            ]),
        );
    }

    let local: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| (x - llx, y - lly)).collect();
    let mut content = build_path_appearance(&local, closed, cloudy.then_some(radius), bw.max(0.5), annot.stroke_color, fill);
    if has_endings {
        // Heads at the first and the last point, pointing outwards, filled
        // with the stroke color.
        let n = local.len();
        let [r, g, b] = annot.stroke_color.map(|v| v.clamp(0.0, 1.0));
        let mut heads = format!("q\n{r:.3} {g:.3} {b:.3} RG\n{r:.3} {g:.3} {b:.3} rg\n{:.3} w\n", bw.max(0.5));
        let dir = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0, a.1 - b.1);
        heads += &line_ending_ops(local[0], dir(local[0], local[1]), endings.0, bw.max(0.5));
        heads += &line_ending_ops(local[n - 1], dir(local[n - 1], local[n - 2]), endings.1, bw.max(0.5));
        heads.push_str("Q\n");
        content.extend_from_slice(heads.as_bytes());
    }
    let form_id = make_form_xobject(doc, urx - llx, ury - lly, content, None);
    d.set("AP", dictionary! { "N" => form_id });
    if let Some(s) = &annot.subject {
        d.set("Subj", pdf_string_utf16be(s));
    }

    let annot_id = doc.add_object(Object::Dictionary(d));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

/// Appearance of a polygon / polyline: the path through `pts` (form space),
/// or for a cloud a chain of outward arcs along every edge.
fn build_path_appearance(
    pts: &[(f32, f32)],
    closed: bool,
    cloud: Option<f32>,
    width: f32,
    stroke: [f32; 3],
    fill: Option<[f32; 3]>,
) -> Vec<u8> {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(64 + pts.len() * 40);
    let [r, g, b] = stroke.map(|v| v.clamp(0.0, 1.0));
    let _ = writeln!(s, "q\n{r:.3} {g:.3} {b:.3} RG\n{width:.3} w\n1 j 1 J");
    if let Some([r, g, b]) = fill.map(|f| f.map(|v| v.clamp(0.0, 1.0))) {
        let _ = writeln!(s, "{r:.3} {g:.3} {b:.3} rg");
    }
    match cloud {
        Some(radius) => cloud_path(&mut s, pts, radius),
        None => {
            for (i, (x, y)) in pts.iter().enumerate() {
                let _ = writeln!(s, "{x:.3} {y:.3} {}", if i == 0 { "m" } else { "l" });
            }
            if closed {
                s.push_str("h\n");
            }
        }
    }
    s.push_str(if closed && fill.is_some() { "B\n" } else { "S\n" });
    s.push_str("Q\n");
    s.into_bytes()
}

/// Closed cloud outline around the polygon `pts`: every edge is split into
/// segments of about two radii, each drawn as a half circle bulging away from
/// the polygon.
fn cloud_path(s: &mut String, pts: &[(f32, f32)], radius: f32) {
    use std::f32::consts::PI;
    use std::fmt::Write as _;
    // Signed area: positive = counter-clockwise (y up). Its outside lies to
    // the right of each edge, which a counter-clockwise sweep from the
    // segment start passes through.
    let area: f32 = pts
        .iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum();
    let outward = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut first = true;
    for (i, &a) in pts.iter().enumerate() {
        let b = pts[(i + 1) % pts.len()];
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len < 1e-3 {
            continue;
        }
        let n = (len / (2.0 * radius)).round().max(1.0) as usize;
        for k in 0..n {
            let t0 = k as f32 / n as f32;
            let t1 = (k + 1) as f32 / n as f32;
            let p0 = (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0);
            let p1 = (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1);
            let c = ((p0.0 + p1.0) / 2.0, (p0.1 + p1.1) / 2.0);
            let r = len / n as f32 / 2.0;
            if first {
                let _ = writeln!(s, "{:.3} {:.3} m", p0.0, p0.1);
                first = false;
            }
            // Half circle from p0 to p1; the sign picks the outer side.
            let start = (p0.1 - c.1).atan2(p0.0 - c.0);
            let sweep = outward * PI;
            for half in 0..2 {
                let a0 = start + sweep / 2.0 * half as f32;
                let a1 = a0 + sweep / 2.0;
                let kk = 4.0 / 3.0 * ((a1 - a0) / 4.0).tan();
                let (s0, c0) = a0.sin_cos();
                let (s1, c1) = a1.sin_cos();
                let q0 = (c.0 + r * c0, c.1 + r * s0);
                let q3 = (c.0 + r * c1, c.1 + r * s1);
                let q1 = (q0.0 - kk * r * s0, q0.1 + kk * r * c0);
                let q2 = (q3.0 + kk * r * s1, q3.1 - kk * r * c1);
                let _ = writeln!(
                    s,
                    "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c",
                    q1.0, q1.1, q2.0, q2.1, q3.0, q3.1
                );
            }
        }
    }
    s.push_str("h\n");
}

fn build_line_appearance(
    x0: f32, y0: f32, x1: f32, y1: f32,
    width: f32,
    rgb: [f32; 3],
    le_start: LineEnding,
    le_end: LineEnding,
) -> Vec<u8> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let r = rgb[0].clamp(0.0, 1.0);
    let g = rgb[1].clamp(0.0, 1.0);
    let b = rgb[2].clamp(0.0, 1.0);
    let _ = write!(&mut buf, "q\n{r:.3} {g:.3} {b:.3} RG\n{r:.3} {g:.3} {b:.3} rg\n{width:.3} w\n");
    // Line itself.
    let _ = write!(&mut buf, "{x0:.3} {y0:.3} m {x1:.3} {y1:.3} l S\n");

    // End head: direction from start → end; start head: end → start.
    buf.extend_from_slice(line_ending_ops((x1, y1), (x1 - x0, y1 - y0), le_end, width).as_bytes());
    buf.extend_from_slice(line_ending_ops((x0, y0), (x0 - x1, y0 - y1), le_start, width).as_bytes());

    buf.extend_from_slice(b"Q\n");
    buf
}

// ----------------- FreeText annotation -----------------

#[derive(Clone, Debug)]
pub struct FreeTextAnnotation {
    pub rect: RectPt,
    pub text: String,
    /// RGB text color 0..1.
    pub color: [f32; 3],
    /// RGBA background. Alpha == 0 means "no background fill".
    pub bg_color: [f32; 4],
    pub font_size: f32,
    /// Logical font family: "Helv" (default), "Times" or "Cour".
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    /// Viewer rotation at insert time (0/1/2/3 = 0/90/180/270 deg CW), on
    /// top of the page's /Rotate. Both are compensated in the appearance so
    /// the text reads upright in the rotated view.
    pub rotation_quadrant: u8,
    pub opacity: f32,
    pub author: Option<String>,
    pub print: bool,
}

/// Private annotation key holding the styling the appearance stream was
/// built from, so a FreeText box can be reopened in the editor.
pub(crate) const FREETEXT_STYLE_KEY: &[u8] = b"RustpdfFreeText";

/// Resource name and /BaseFont of the standard font for a logical family
/// ("Helv" | "Times" | "Cour", anything else = Helvetica) and style.
pub(crate) fn std_font(family: &str, bold: bool, italic: bool) -> (&'static str, &'static str) {
    match (family, bold, italic) {
        ("Times", false, false) => ("TiRo", "Times-Roman"),
        ("Times", true, false) => ("TiBo", "Times-Bold"),
        ("Times", false, true) => ("TiIt", "Times-Italic"),
        ("Times", true, true) => ("TiBI", "Times-BoldItalic"),
        ("Cour", false, false) => ("Cour", "Courier"),
        ("Cour", true, false) => ("CoBo", "Courier-Bold"),
        ("Cour", false, true) => ("CoOb", "Courier-Oblique"),
        ("Cour", true, true) => ("CoBO", "Courier-BoldOblique"),
        (_, false, false) => ("Helv", "Helvetica"),
        (_, true, false) => ("HeBo", "Helvetica-Bold"),
        (_, false, true) => ("HeOb", "Helvetica-Oblique"),
        (_, true, true) => ("HeBO", "Helvetica-BoldOblique"),
    }
}

/// Ensures the document catalog has an /AcroForm /DR /Font /<key> entry,
/// creating the font with `create` when there is none yet. Returns the font's
/// object id (reused by later annotations). An existing AcroForm (inline or
/// indirect) and its /DR fonts are extended, never replaced — the AcroForm
/// carries the document's form /Fields.
fn ensure_dr_font(
    doc: &mut Document,
    key: &str,
    create: impl FnOnce(&mut Document) -> MutateResult<ObjectId>,
) -> MutateResult<ObjectId> {
    let root_id = doc
        .trailer
        .get(b"Root")
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .as_reference()
        .map_err(|e| MutateError::Parse(e.to_string()))?;

    // AcroForm as an indirect object; an inline one is promoted as-is.
    let af_entry = doc
        .get_dictionary(root_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"AcroForm")
        .ok()
        .cloned();
    let af_id = match af_entry {
        Some(Object::Reference(r)) if doc.get_dictionary(r).is_ok() => r,
        other => {
            let af = match other {
                Some(Object::Dictionary(d)) => d,
                _ => {
                    let mut d = Dictionary::new();
                    d.set("Fields", Object::Array(Vec::new()));
                    d
                }
            };
            let id = doc.add_object(Object::Dictionary(af));
            if let Ok(catalog) = doc.get_dictionary_mut(root_id) {
                catalog.set("AcroForm", Object::Reference(id));
            }
            id
        }
    };

    // /DR and its /Font sub-dictionary may each be inline or indirect.
    let resolve = |doc: &Document, o: Option<Object>| -> (Option<ObjectId>, Dictionary) {
        match o {
            Some(Object::Reference(r)) => {
                (Some(r), doc.get_dictionary(r).cloned().unwrap_or_default())
            }
            Some(Object::Dictionary(d)) => (None, d),
            _ => (None, Dictionary::new()),
        }
    };
    let dr_entry = doc.get_dictionary(af_id).ok().and_then(|d| d.get(b"DR").ok().cloned());
    let (dr_ref, mut dr) = resolve(doc, dr_entry);
    let (font_ref, mut fonts) = resolve(doc, dr.get(b"Font").ok().cloned());
    if let Ok(Object::Reference(font_id)) = fonts.get(key.as_bytes()) {
        if doc.get_dictionary(*font_id).is_ok() {
            return Ok(*font_id);
        }
    }

    // Create the font and register it next to the existing ones.
    let font_id = create(doc)?;
    fonts.set(key, Object::Reference(font_id));
    match font_ref {
        Some(r) => {
            if let Ok(o) = doc.get_object_mut(r) {
                *o = Object::Dictionary(fonts);
            }
        }
        None => {
            dr.set("Font", Object::Dictionary(fonts));
            match dr_ref {
                Some(r) => {
                    if let Ok(o) = doc.get_object_mut(r) {
                        *o = Object::Dictionary(dr);
                    }
                }
                None => {
                    if let Ok(af) = doc.get_dictionary_mut(af_id) {
                        af.set("DR", Object::Dictionary(dr));
                    }
                }
            }
        }
    }
    Ok(font_id)
}

/// The standard Type1 font `base_font`, registered in /DR as `key`.
fn ensure_std_font(doc: &mut Document, key: &str, base_font: &str) -> MutateResult<ObjectId> {
    ensure_dr_font(doc, key, |doc| {
        Ok(doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
            "Encoding" => "WinAnsiEncoding",
        }))
    })
}

/// The font a FreeText box is written with.
enum TextFont {
    /// One of the standard 14 fonts; strings are WinAnsi bytes.
    Standard,
    /// A bundled TrueType face, embedded as a Type0 font; strings are
    /// glyph ids.
    Embedded(Box<ttf_parser::Face<'static>>),
}

impl TextFont {
    fn encode(&self, line: &str) -> Vec<u8> {
        match self {
            TextFont::Standard => pdf_literal_string(line),
            TextFont::Embedded(face) => crate::truetype::encode(face, line),
        }
    }
}

/// Resource key, font object and encoder for a FreeText font: a bundled
/// family (see `pdf_fonts`) is embedded once per document and style, the
/// standard families use the standard 14 fonts.
fn freetext_font(
    doc: &mut Document,
    family: &str,
    bold: bool,
    italic: bool,
) -> MutateResult<(String, ObjectId, TextFont)> {
    if let Some(fam) = pdf_fonts::family(family) {
        let (bold, italic) = fam.effective_style(bold, italic);
        let data = fam.face(bold, italic);
        let suffix = match (bold, italic) {
            (false, false) => "",
            (true, false) => "B",
            (false, true) => "I",
            (true, true) => "BI",
        };
        let key = format!("RP{}{suffix}", fam.id);
        let serif = fam.category == pdf_fonts::Category::Serif;
        let id = ensure_dr_font(doc, &key, |doc| crate::truetype::add_type0_font(doc, data, serif))?;
        return Ok((key, id, TextFont::Embedded(Box::new(crate::truetype::parse(data)?))));
    }
    let (key, base_font) = std_font(family, bold, italic);
    let id = ensure_std_font(doc, key, base_font)?;
    Ok((key.to_string(), id, TextFont::Standard))
}

pub fn add_freetext_annotation(
    doc: &mut Document,
    page_index: u32,
    annot: &FreeTextAnnotation,
) -> MutateResult<ObjectId> {
    let page_id = page_id_for(doc, page_index)?;
    let view = page_view(doc, page_id);
    let (font_key, font_id, font) =
        freetext_font(doc, &annot.font_family, annot.bold, annot.italic)?;
    // A family without a bold / italic face is written (and recorded) upright
    // or regular.
    let (bold, italic) = pdf_fonts::family(&annot.font_family)
        .map_or((annot.bold, annot.italic), |f| f.effective_style(annot.bold, annot.italic));

    let (llx, lly, urx, ury) = rect_native(&annot.rect, view);
    let mut d = base_annot_dict(
        page_id,
        "FreeText",
        (llx, lly, urx, ury),
        annot.color,
        annot.opacity,
        Some(annot.text.as_str()),
        annot.author.as_deref(),
        annot.print,
    );

    let fs = annot.font_size.max(4.0);
    let c = [
        annot.color[0].clamp(0.0, 1.0),
        annot.color[1].clamp(0.0, 1.0),
        annot.color[2].clamp(0.0, 1.0),
    ];
    let da = format!("{:.3} {:.3} {:.3} rg /{font_key} {:.2} Tf", c[0], c[1], c[2], fs);
    d.set(
        "DA",
        Object::String(da.into_bytes(), lopdf::StringFormat::Literal),
    );
    d.set("Q", Object::Integer(0)); // left align

    // Build /AP /N — PDFium needs an explicit appearance stream for FreeText.
    let w = urx - llx;
    let h = ury - lly;
    let bg = if annot.bg_color[3] > 0.0 {
        Some([
            annot.bg_color[0].clamp(0.0, 1.0),
            annot.bg_color[1].clamp(0.0, 1.0),
            annot.bg_color[2].clamp(0.0, 1.0),
        ])
    } else {
        None
    };
    // Styling for reopening the box in the editor (see `read`).
    let mut style = Dictionary::new();
    let family = match annot.font_family.as_str() {
        f @ ("Times" | "Cour") => f,
        f if pdf_fonts::is_bundled(f) => f,
        _ => "Helv",
    };
    style.set("Font", Object::Name(family.as_bytes().to_vec()));
    style.set("Bold", Object::Boolean(bold));
    style.set("Italic", Object::Boolean(italic));
    style.set("Rot", Object::Integer((annot.rotation_quadrant & 3) as i64));
    if let Some(bg) = bg {
        style.set("BG", Object::Array(bg.iter().map(|v| Object::Real(*v)).collect()));
    }
    d.set(FREETEXT_STYLE_KEY, Object::Dictionary(style));

    // The page's own /Rotate turns the appearance on screen just like the
    // viewer rotation does, so the text is counter-rotated by both. Without
    // the page part, text on a rotated page (common for CAD plans: portrait
    // MediaBox shown as landscape) came out turned by that angle (#11).
    let page_quadrant = (view.1 / 90) as u8;
    let content = build_freetext_appearance(
        &annot.text,
        &font_key,
        &font,
        fs,
        c,
        bg,
        w,
        h,
        ((annot.rotation_quadrant & 3) + page_quadrant) & 3,
    );
    let mut font_dict = Dictionary::new();
    font_dict.set(font_key.as_str(), Object::Reference(font_id));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(font_dict));
    let form_id = make_form_xobject(doc, w, h, content, Some(resources));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    d.set("AP", Object::Dictionary(ap));

    let annot_id = doc.add_object(Object::Dictionary(d));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

fn winansi_byte(c: char) -> u8 {
    let cp = c as u32;
    if cp <= 0x7F {
        cp as u8
    } else if (0xA0..=0xFF).contains(&cp) {
        cp as u8
    } else {
        match c {
            '€' => 0x80, '‚' => 0x82, 'ƒ' => 0x83, '„' => 0x84,
            '…' => 0x85, '†' => 0x86, '‡' => 0x87, 'ˆ' => 0x88,
            '‰' => 0x89, 'Š' => 0x8A, '‹' => 0x8B, 'Œ' => 0x8C,
            'Ž' => 0x8E, '\u{2018}' => 0x91, '\u{2019}' => 0x92,
            '\u{201C}' => 0x93, '\u{201D}' => 0x94, '•' => 0x95,
            '–' => 0x96, '—' => 0x97, '˜' => 0x98, '™' => 0x99,
            'š' => 0x9A, '›' => 0x9B, 'œ' => 0x9C, 'ž' => 0x9E,
            'Ÿ' => 0x9F,
            _ => b'?',
        }
    }
}

fn pdf_literal_string(s: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(s.len() + 4);
    out.push(b'(');
    for c in s.chars() {
        let b = winansi_byte(c);
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            _ => out.push(b),
        }
    }
    out.push(b')');
    out
}

fn build_freetext_appearance(
    text: &str,
    font_key: &str,
    font: &TextFont,
    fs: f32,
    rgb: [f32; 3],
    bg: Option<[f32; 3]>,
    w: f32,
    h: f32,
    rotation_quadrant: u8,
) -> Vec<u8> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::with_capacity(text.len() + 200);
    let r = rgb[0];
    let g = rgb[1];
    let b = rgb[2];
    let margin = 2.0_f32;
    let leading = fs * 1.2;

    if let Some(bg) = bg {
        let _ = write!(
            &mut buf,
            "q\n{br:.3} {bg2:.3} {bb:.3} rg\n0 0 {w:.2} {h:.2} re f\nQ\n",
            br = bg[0],
            bg2 = bg[1],
            bb = bg[2],
        );
    }

    // Pick text matrix + first-line origin so the rendered text reads upright
    // in the user's rotated viewer.  Each quadrant rotates the glyph orientation
    // by the same angle the viewer is rotated, which cancels out at render time.
    // The line-advance vector is the displacement between successive baselines,
    // applied via `Td` (we cannot rely on `T*` because T*'s direction depends on
    // text-space orientation and would point "up" in the user's view for q=2).
    let (tm, origin, advance) = match rotation_quadrant {
        // No rotation: existing behavior — top-left origin, baseline goes +x.
        0 => ("1 0 0 1", (margin, h - margin - fs), (0.0_f32, -leading)),
        // 90° CW viewer rotation: text reads bottom-up in PDF (view left-to-right).
        1 => ("0 1 -1 0", (margin + fs, margin), (leading, 0.0)),
        // 180° viewer rotation: text reads right-to-left and lines stack downward
        // in PDF space (= downward in viewer too, since both flip together).
        2 => ("-1 0 0 -1", (w - margin, margin + fs), (0.0, leading)),
        // 90° CCW viewer rotation (= 270° CW): text reads top-down in PDF.
        3 => (
            "0 -1 1 0",
            (w - margin - fs, h - margin),
            (-leading, 0.0),
        ),
        _ => ("1 0 0 1", (margin, h - margin - fs), (0.0, -leading)),
    };

    let _ = write!(&mut buf, "q\nBT\n/{font_key} {fs:.2} Tf\n");
    let _ = write!(&mut buf, "{r:.3} {g:.3} {b:.3} rg\n");
    let _ = write!(&mut buf, "{tm} {ex:.2} {ey:.2} Tm\n", ex = origin.0, ey = origin.1);
    let mut first = true;
    for line in text.split('\n') {
        if !first {
            let _ = write!(&mut buf, "{ax:.2} {ay:.2} Td\n", ax = advance.0, ay = advance.1);
        }
        first = false;
        buf.extend_from_slice(&font.encode(line));
        buf.extend_from_slice(b" Tj\n");
    }
    buf.extend_from_slice(b"ET\nQ\n");
    buf
}

// ----------------- Ink annotation -----------------

#[derive(Clone, Debug)]
pub struct InkAnnotation {
    /// One stroke = a list of points in our top-origin convention.
    /// Each sub-Vec becomes one InkList[i] (a polyline).
    pub strokes: Vec<Vec<(f32, f32)>>,
    pub color: [f32; 3],
    pub width: f32,
    pub opacity: f32,
    pub contents: Option<String>,
    pub author: Option<String>,
    pub print: bool,
}

pub fn add_ink_annotation(
    doc: &mut Document,
    page_index: u32,
    annot: &InkAnnotation,
) -> MutateResult<ObjectId> {
    if annot.strokes.iter().all(|s| s.len() < 2) {
        return Err(MutateError::Parse("ink: no usable strokes".into()));
    }
    let page_id = page_id_for(doc, page_index)?;
    let view = page_view(doc, page_id);

    let mut ink_list: Vec<Object> = Vec::with_capacity(annot.strokes.len());
    let mut bbox: Option<(f32, f32, f32, f32)> = None;
    for stroke in &annot.strokes {
        if stroke.len() < 2 {
            continue;
        }
        let mut pts: Vec<Object> = Vec::with_capacity(stroke.len() * 2);
        for &(x, y) in stroke {
            let (nx, ny) = crate::point_to_pdf_space(x, y, view.0, view.1);
                        pts.push(Object::Real(nx));
            pts.push(Object::Real(ny));
            let bb = bbox.get_or_insert((nx, ny, nx, ny));
            bb.0 = bb.0.min(nx);
            bb.1 = bb.1.min(ny);
            bb.2 = bb.2.max(nx);
            bb.3 = bb.3.max(ny);
        }
        ink_list.push(Object::Array(pts));
    }
    let pad = annot.width.max(1.0) + 4.0;
    let (llx, lly, urx, ury) = bbox.unwrap();
    let rect_native = (llx - pad, lly - pad, urx + pad, ury + pad);

    let mut d = base_annot_dict(
        page_id,
        "Ink",
        rect_native,
        annot.color,
        annot.opacity,
        annot.contents.as_deref(),
        annot.author.as_deref(),
        annot.print,
    );
    let mut bs = Dictionary::new();
    bs.set("Type", Object::Name(b"Border".to_vec()));
    bs.set("W", Object::Real(annot.width.max(0.1)));
    bs.set("S", Object::Name(b"S".to_vec()));
    d.set("BS", Object::Dictionary(bs));
    d.set("InkList", Object::Array(ink_list));

    let annot_id = doc.add_object(Object::Dictionary(d));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

// ----------------- Stamp annotation (image) -----------------

#[derive(Clone, Debug)]
pub struct StampAnnotation {
    pub rect: RectPt,
    /// Decoded image bytes (any common format). Will be re-encoded as JPEG.
    pub image: Vec<u8>,
    pub opacity: f32,
    pub contents: Option<String>,
    pub author: Option<String>,
    pub print: bool,
}

pub fn add_stamp_annotation(
    doc: &mut Document,
    page_index: u32,
    annot: &StampAnnotation,
) -> MutateResult<ObjectId> {
    use lopdf::Stream;
    use std::io::Write;
    let page_id = page_id_for(doc, page_index)?;
    let view = page_view(doc, page_id);

    // Decode image. Images with an alpha channel (e.g. drawn signatures on a
    // transparent background) keep their transparency via an /SMask soft mask
    // so they don't paint an opaque box over the page. Opaque images are
    // re-encoded as JPEG, which is smaller for photos.
    let img = image::load_from_memory(&annot.image)
        .map_err(|e| MutateError::Parse(format!("stamp image decode: {e}")))?;
    let image_id = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let (iw, ih) = (rgba.width(), rgba.height());
        let n = (iw as usize) * (ih as usize);
        let mut rgb: Vec<u8> = Vec::with_capacity(n * 3);
        let mut alpha: Vec<u8> = Vec::with_capacity(n);
        for px in rgba.pixels() {
            rgb.extend_from_slice(&[px[0], px[1], px[2]]);
            alpha.push(px[3]);
        }
        let smask_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => iw as i64,
            "Height" => ih as i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        };
        let mut smask_stream = Stream::new(smask_dict, alpha);
        let _ = smask_stream.compress();
        let smask_id = doc.add_object(Object::Stream(smask_stream));
        let image_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => iw as i64,
            "Height" => ih as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "SMask" => Object::Reference(smask_id),
        };
        let mut image_stream = Stream::new(image_dict, rgb);
        let _ = image_stream.compress();
        doc.add_object(Object::Stream(image_stream))
    } else {
        let rgb = img.to_rgb8();
        let (iw, ih) = (rgb.width(), rgb.height());
        let mut jpeg: Vec<u8> = Vec::new();
        {
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
            encoder
                .encode(rgb.as_raw(), iw, ih, image::ExtendedColorType::Rgb8)
                .map_err(|e| MutateError::Parse(format!("stamp jpeg: {e}")))?;
        }
        let image_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => iw as i64,
            "Height" => ih as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        };
        doc.add_object(Object::Stream(Stream::new(image_dict, jpeg)))
    };

    let (llx, lly, urx, ury) = rect_native(&annot.rect, view);
    let w = (urx - llx).max(1.0);
    let h = (ury - lly).max(1.0);

    // Form XObject acting as the appearance stream. /BBox is local coords;
    // /Matrix maps it into the annotation rectangle (we use the unit square).
    let mut form_content: Vec<u8> = Vec::with_capacity(64);
    let _ = write!(
        &mut form_content,
        "q\n{w:.4} 0 0 {h:.4} 0 0 cm\n/Im0 Do\nQ\n",
    );
    let mut form_dict = Dictionary::new();
    form_dict.set("Type", Object::Name(b"XObject".to_vec()));
    form_dict.set("Subtype", Object::Name(b"Form".to_vec()));
    form_dict.set(
        "BBox",
        Object::Array(vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(w),
            Object::Real(h),
        ]),
    );
    form_dict.set("FormType", Object::Integer(1));
    let mut resources = Dictionary::new();
    let mut xobj = Dictionary::new();
    xobj.set("Im0", Object::Reference(image_id));
    resources.set("XObject", Object::Dictionary(xobj));
    form_dict.set("Resources", Object::Dictionary(resources));
    let form_id = doc.add_object(Object::Stream(Stream::new(form_dict, form_content)));

    let mut d = base_annot_dict(
        page_id,
        "Stamp",
        (llx, lly, urx, ury),
        [1.0, 1.0, 1.0],
        annot.opacity,
        annot.contents.as_deref(),
        annot.author.as_deref(),
        annot.print,
    );
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    d.set("AP", Object::Dictionary(ap));

    let annot_id = doc.add_object(Object::Dictionary(d));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

// ----------------- Text note (sticky note) annotation -----------------

#[derive(Clone, Debug)]
pub struct TextNoteAnnotation {
    /// Top-origin anchor: top-left corner of the note icon (PDF points).
    pub x_pt: f32,
    pub y_pt: f32,
    pub contents: String,
    /// RGB icon color 0..1.
    pub color: [f32; 3],
    /// Standard PDF /Name icon: "Note" | "Comment" | "Help" | "Insert" |
    /// "Key" | "NewParagraph" | "Paragraph". Defaults to "Note".
    pub icon: Option<String>,
    pub author: Option<String>,
    pub print: bool,
}

/// Sticky-note icon size — matches Acrobat (~22pt square).
const TEXT_NOTE_ICON_SIZE_PT: f32 = 22.0;

pub fn add_text_note_annotation(
    doc: &mut Document,
    page_index: u32,
    annot: &TextNoteAnnotation,
) -> MutateResult<ObjectId> {
    if annot.contents.trim().is_empty() {
        return Err(MutateError::Parse("text note: empty contents".into()));
    }
    let page_id = page_id_for(doc, page_index)?;
    let view = page_view(doc, page_id);
    let (llx, ury) = crate::point_to_pdf_space(annot.x_pt, annot.y_pt, view.0, view.1);
        let urx = llx + TEXT_NOTE_ICON_SIZE_PT;
    let lly = ury - TEXT_NOTE_ICON_SIZE_PT;

    let mut d = base_annot_dict(
        page_id,
        "Text",
        (llx, lly, urx, ury),
        annot.color,
        1.0,
        Some(annot.contents.as_str()),
        annot.author.as_deref(),
        annot.print,
    );
    let icon = annot.icon.as_deref().unwrap_or("Note");
    d.set("Name", Object::Name(icon.as_bytes().to_vec()));
    d.set("Open", Object::Boolean(false));

    let annot_id = doc.add_object(Object::Dictionary(d));
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_types::RectPt;

    fn one_page_doc() -> Document {
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
        d
    }

    #[test]
    fn delete_annotation_removes_by_id() {
        let mut doc = one_page_doc();
        let annot = MarkupAnnotation {
            kind: MarkupKind::Highlight,
            rects: vec![RectPt { x0: 72.0, y0: 100.0, x1: 200.0, y1: 120.0 }],
            color: [1.0, 0.9, 0.0],
            opacity: 0.4,
            contents: None,
            author: None,
            print: true,
        };
        add_markup_annotation(&mut doc, 0, &annot).unwrap();

        let listed = crate::read::list_annotations(&doc);
        assert_eq!(listed.len(), 1, "annotation should be listed");
        let id = listed[0].id.clone();

        // Unknown id is a no-op.
        assert!(!delete_annotation(&mut doc, 0, "no-such-id").unwrap());
        assert_eq!(crate::read::list_annotations(&doc).len(), 1);

        // Real id removes it.
        assert!(delete_annotation(&mut doc, 0, &id).unwrap());
        assert!(crate::read::list_annotations(&doc).is_empty());

        // Second delete finds nothing.
        assert!(!delete_annotation(&mut doc, 0, &id).unwrap());
    }

    /// The markup annotation carries an explicit appearance stream, and a
    /// highlight blends with Multiply so text underneath stays readable.
    fn markup_ap_stream(doc: &Document, annot_id: ObjectId) -> (Dictionary, Vec<u8>) {
        let adict = doc.get_dictionary(annot_id).unwrap();
        let ap = adict.get(b"AP").unwrap().as_dict().unwrap();
        let form_ref = ap.get(b"N").unwrap().as_reference().unwrap();
        let stream = doc.get_object(form_ref).unwrap().as_stream().unwrap();
        (stream.dict.clone(), stream.content.clone())
    }

    #[test]
    fn highlight_has_multiply_appearance() {
        let mut doc = one_page_doc();
        let annot = MarkupAnnotation {
            kind: MarkupKind::Highlight,
            rects: vec![RectPt { x0: 72.0, y0: 100.0, x1: 200.0, y1: 120.0 }],
            color: [1.0, 0.9, 0.0],
            opacity: 0.4,
            contents: None,
            author: None,
            print: true,
        };
        let id = add_markup_annotation(&mut doc, 0, &annot).unwrap();
        let (form, content) = markup_ap_stream(&doc, id);

        // Multiply blend mode is declared on the form's ExtGState.
        let res = form.get(b"Resources").unwrap().as_dict().unwrap();
        let gs = res
            .get(b"ExtGState").unwrap().as_dict().unwrap()
            .get(b"GShl").unwrap().as_dict().unwrap();
        assert_eq!(gs.get(b"BM").unwrap().as_name().unwrap(), b"Multiply");
        // The content fills a rectangle and references the blend state.
        let s = String::from_utf8_lossy(&content);
        assert!(s.contains("/GShl gs"), "content: {s}");
        assert!(s.contains(" re") && s.contains("f"), "content: {s}");
    }

    #[test]
    fn underline_appearance_strokes_a_line() {
        let mut doc = one_page_doc();
        let annot = MarkupAnnotation {
            kind: MarkupKind::Underline,
            rects: vec![RectPt { x0: 72.0, y0: 100.0, x1: 200.0, y1: 120.0 }],
            color: [0.0, 0.0, 1.0],
            opacity: 1.0,
            contents: None,
            author: None,
            print: true,
        };
        let id = add_markup_annotation(&mut doc, 0, &annot).unwrap();
        let (_form, content) = markup_ap_stream(&doc, id);
        let s = String::from_utf8_lossy(&content);
        assert!(s.contains(" m ") && s.contains(" l S"), "content: {s}");
    }

    fn freetext(text: &str) -> FreeTextAnnotation {
        FreeTextAnnotation {
            rect: RectPt { x0: 72.0, y0: 72.0, x1: 200.0, y1: 100.0 },
            text: text.into(),
            color: [0.0, 0.0, 0.0],
            bg_color: [1.0, 1.0, 1.0, 0.0],
            font_size: 12.0,
            font_family: "Helv".into(),
            bold: false,
            italic: false,
            rotation_quadrant: 0,
            opacity: 1.0,
            author: None,
            print: true,
        }
    }

    #[test]
    fn freetext_keeps_inline_acroform_fields_and_fonts() {
        let mut doc = one_page_doc();
        let field = doc.add_object(dictionary! { "FT" => "Tx", "T" => Object::string_literal("Name") });
        let zadb = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "ZapfDingbats" });
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(cat_id).unwrap().set(
            "AcroForm",
            Object::Dictionary(dictionary! {
                "Fields" => Object::Array(vec![Object::Reference(field)]),
                "DR" => Object::Dictionary(dictionary! {
                    "Font" => Object::Dictionary(dictionary! { "ZaDb" => Object::Reference(zadb) }),
                }),
            }),
        );
        add_freetext_annotation(&mut doc, 0, &freetext("hi")).unwrap();
        // Idempotent: a second call reuses the registered font.
        add_freetext_annotation(&mut doc, 0, &freetext("again")).unwrap();

        let af = match doc.get_dictionary(cat_id).unwrap().get(b"AcroForm").unwrap() {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap().clone(),
            o => o.as_dict().unwrap().clone(),
        };
        let fields = af.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields, &vec![Object::Reference(field)], "form fields dropped");
        let fonts = af.get(b"DR").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.has(b"ZaDb"), "existing DR font dropped: {fonts:?}");
        assert!(fonts.has(b"Helv"));
    }

    #[test]
    fn freetext_style_round_trips_for_reediting() {
        let mut doc = one_page_doc();
        let mut annot = freetext("Grüße\nZeile 2");
        annot.font_family = "Times".into();
        annot.bold = true;
        annot.color = [0.2, 0.4, 0.6];
        annot.font_size = 18.0;
        annot.rotation_quadrant = 1;
        add_freetext_annotation(&mut doc, 0, &annot).unwrap();
        let mut boxed = freetext("with background");
        boxed.bg_color = [1.0, 0.5, 0.0, 1.0];
        boxed.italic = true;
        add_freetext_annotation(&mut doc, 0, &boxed).unwrap();

        let listed = crate::read::list_annotations(&doc);
        let style = listed[0].freetext.as_ref().expect("freetext style");
        assert_eq!(listed[0].contents.as_deref(), Some("Grüße\nZeile 2"));
        assert_eq!(style.font_family, "Times");
        assert!(style.bold && !style.italic);
        assert_eq!(style.font_size, 18.0);
        assert_eq!(style.rotation_quadrant, 1);
        assert_eq!(style.bg_color, None);
        assert!((style.text_color[1] - 0.4).abs() < 0.001);

        let style = listed[1].freetext.as_ref().expect("freetext style");
        assert_eq!(style.font_family, "Helv");
        assert!(style.italic && !style.bold);
        assert_eq!(style.bg_color, Some([1.0, 0.5, 0.0]));

        // Each style draws with its own standard font.
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let af = doc.get_dictionary(cat_id).unwrap().get(b"AcroForm").unwrap().as_reference().unwrap();
        let fonts = doc.get_dictionary(af).unwrap().get(b"DR").unwrap().as_dict().unwrap()
            .get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.has(b"TiBo") && fonts.has(b"HeOb"), "fonts: {fonts:?}");
    }

    fn dr_fonts(doc: &Document) -> Dictionary {
        let cat_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let af = doc.get_dictionary(cat_id).unwrap().get(b"AcroForm").unwrap().as_reference().unwrap();
        doc.get_dictionary(af).unwrap().get(b"DR").unwrap().as_dict().unwrap()
            .get(b"Font").unwrap().as_dict().unwrap().clone()
    }

    fn count_type0_fonts(doc: &Document) -> usize {
        doc.objects
            .values()
            .filter(|o| o.as_dict().is_ok_and(|d| d.get(b"Subtype").is_ok_and(|s| s.as_name().is_ok_and(|n| n == b"Type0"))))
            .count()
    }

    #[test]
    fn freetext_with_bundled_font_embeds_it_once() {
        let mut doc = one_page_doc();
        let mut annot = freetext("Grüße €");
        annot.font_family = "Roboto".into();
        annot.bold = true;
        let id = add_freetext_annotation(&mut doc, 0, &annot).unwrap();
        add_freetext_annotation(&mut doc, 0, &annot).unwrap();
        assert_eq!(count_type0_fonts(&doc), 1, "the face is embedded once and reused");

        let fonts = dr_fonts(&doc);
        let font_id = fonts.get(b"RPRobotoB").unwrap().as_reference().unwrap();
        let font = doc.get_dictionary(font_id).unwrap();
        assert_eq!(font.get(b"BaseFont").unwrap().as_name().unwrap(), b"Roboto-Bold");
        assert_eq!(font.get(b"Encoding").unwrap().as_name().unwrap(), b"Identity-H");
        let cid = font.get(b"DescendantFonts").unwrap().as_array().unwrap()[0].as_reference().unwrap();
        let desc = doc.get_dictionary(cid).unwrap().get(b"FontDescriptor").unwrap().as_reference().unwrap();
        let file = doc.get_dictionary(desc).unwrap().get(b"FontFile2").unwrap().as_reference().unwrap();
        let data = pdf_fonts::family("Roboto").unwrap().face(true, false);
        let stream = doc.get_object(file).unwrap().as_stream().unwrap();
        assert_eq!(stream.dict.get(b"Length1").unwrap().as_i64().unwrap(), data.len() as i64);

        // The appearance draws glyph ids with the embedded font.
        let (form, content) = markup_ap_stream(&doc, id);
        let res_fonts = form.get(b"Resources").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap();
        assert!(res_fonts.has(b"RPRobotoB"));
        let content = String::from_utf8_lossy(&content);
        let face = crate::truetype::parse(data).unwrap();
        let g = face.glyph_index('G').unwrap().0;
        assert!(content.contains("/RPRobotoB 12.00 Tf"), "{content}");
        assert!(content.contains(&format!("<{g:04X}")), "{content}");

        let listed = crate::read::list_annotations(&doc);
        let style = listed[0].freetext.as_ref().unwrap();
        assert_eq!(style.font_family, "Roboto");
        assert!(style.bold && !style.italic);
    }

    #[test]
    fn freetext_records_the_style_a_family_really_has() {
        let mut doc = one_page_doc();
        let mut annot = freetext("Notiz");
        annot.font_family = "Caveat".into();
        annot.italic = true;
        add_freetext_annotation(&mut doc, 0, &annot).unwrap();
        assert!(dr_fonts(&doc).has(b"RPCaveat"));
        let style = crate::read::list_annotations(&doc)[0].freetext.clone().unwrap();
        assert_eq!(style.font_family, "Caveat");
        assert!(!style.italic, "Caveat has no italic face");
    }

    fn shape(kind: ShapeKind, vertices: Vec<(f32, f32)>) -> ShapeAnnotation {
        ShapeAnnotation {
            kind,
            rect: RectPt { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 },
            vertices,
            subject: Some("Stern".into()),
            stroke_color: [0.8, 0.1, 0.1],
            fill_color: Some([1.0, 1.0, 0.0]),
            border_width: 1.5,
            opacity: 1.0,
            contents: None,
            author: None,
            print: true,
        }
    }

    #[test]
    fn polygon_and_polyline_carry_vertices_and_appearance() {
        let mut doc = one_page_doc();
        let tri = vec![(100.0, 100.0), (150.0, 200.0), (50.0, 200.0)];
        let id = add_shape_annotation(&mut doc, 0, &shape(ShapeKind::Polygon { cloudy: false }, tri)).unwrap();
        let d = doc.get_dictionary(id).unwrap().clone();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Polygon");
        let v: Vec<f32> = d.get(b"Vertices").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
        // Top-origin (100, 100) on a 792pt page is PDF (100, 692).
        assert_eq!(&v[..2], &[100.0, 692.0]);
        assert_eq!(v.len(), 6);
        assert!(d.has(b"IC") && !d.has(b"BE"));
        let (_, content) = markup_ap_stream(&doc, id);
        let content = String::from_utf8_lossy(&content);
        assert!(content.contains(" m\n") && content.contains("h\nB\n"), "{content}");
        // /Rect encloses every vertex.
        let r: Vec<f32> = d.get(b"Rect").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
        assert!(r[0] <= 50.0 && r[1] <= 592.0 && r[2] >= 150.0 && r[3] >= 692.0, "{r:?}");

        let check = vec![(10.0, 20.0), (15.0, 28.0), (30.0, 5.0)];
        let id = add_shape_annotation(&mut doc, 0, &shape(ShapeKind::PolyLine { start: LineEnding::None, end: LineEnding::None }, check)).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"PolyLine");
        assert!(!d.has(b"IC"), "an open path has no interior");
        let (_, content) = markup_ap_stream(&doc, id);
        assert!(String::from_utf8_lossy(&content).ends_with("S\nQ\n"));

        let listed = crate::read::list_annotations(&doc);
        assert_eq!(listed.iter().map(|a| a.subtype.as_str()).collect::<Vec<_>>(), ["Polygon", "PolyLine"]);
        assert_eq!(listed[0].subject.as_deref(), Some("Stern"));

        assert!(add_shape_annotation(&mut doc, 0, &shape(ShapeKind::Polygon { cloudy: false }, vec![(0.0, 0.0), (1.0, 1.0)])).is_err());
    }

    #[test]
    fn polyline_endings_are_written_and_drawn() {
        let mut doc = one_page_doc();
        let pts = vec![(100.0, 100.0), (160.0, 140.0), (220.0, 100.0)];
        let kind = ShapeKind::PolyLine { start: LineEnding::OpenArrow, end: LineEnding::ClosedArrow };
        let id = add_shape_annotation(&mut doc, 0, &shape(kind, pts)).unwrap();
        let d = doc.get_dictionary(id).unwrap().clone();
        let le: Vec<&[u8]> = d.get(b"LE").unwrap().as_array().unwrap().iter().map(|o| o.as_name().unwrap()).collect();
        assert_eq!(le, [b"OpenArrow".as_slice(), b"ClosedArrow".as_slice()]);
        let (_, content) = markup_ap_stream(&doc, id);
        let content = String::from_utf8_lossy(&content);
        // One open head (stroked), one closed head (filled).
        assert!(content.contains(" l S\n") && content.contains(" l h B\n"), "{content}");
        // /Rect leaves room for the heads around the end points.
        let r: Vec<f32> = d.get(b"Rect").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
        assert!(r[0] <= 100.0 - 8.0 && r[2] >= 220.0 + 8.0, "{r:?}");
    }

    #[test]
    fn cloud_bulges_outward_whatever_the_vertex_order() {
        let square = vec![(100.0, 100.0), (200.0, 100.0), (200.0, 200.0), (100.0, 200.0)];
        for pts in [square.clone(), square.into_iter().rev().collect()] {
            let mut doc = one_page_doc();
            let id = add_shape_annotation(&mut doc, 0, &shape(ShapeKind::Polygon { cloudy: true }, pts)).unwrap();
            let d = doc.get_dictionary(id).unwrap().clone();
            let be = d.get(b"BE").unwrap().as_dict().unwrap();
            assert_eq!(be.get(b"S").unwrap().as_name().unwrap(), b"C");
            assert_eq!(d.get(b"IT").unwrap().as_name().unwrap(), b"PolygonCloud");
            let r: Vec<f32> = d.get(b"Rect").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
            let (_, content) = markup_ap_stream(&doc, id);
            let content = String::from_utf8_lossy(&content);
            // Every curve point lies outside the square (form space: the
            // square spans pad..pad+100) and inside the form box.
            let pad = 100.0 - (r[0]);
            let nums: Vec<f32> = content
                .lines()
                .filter(|l| l.ends_with(" c"))
                .flat_map(|l| l.split_whitespace().take(6).map(|t| t.parse::<f32>().unwrap()).collect::<Vec<_>>())
                .collect();
            assert!(nums.len() > 6 * 8, "several arcs per edge");
            let (w, h) = (r[2] - r[0], r[3] - r[1]);
            for xy in nums.chunks(2).skip(2).step_by(3) {
                let (x, y) = (xy[0], xy[1]);
                let inside = x > pad + 0.5 && x < pad + 99.5 && y > pad + 0.5 && y < pad + 99.5;
                assert!(!inside, "arc end ({x}, {y}) inside the square");
                assert!((-0.01..=w + 0.01).contains(&x) && (-0.01..=h + 0.01).contains(&y));
            }
            // Arc mid points (the second end point of each half) sit outside.
            let mids: Vec<(f32, f32)> = nums.chunks(6).step_by(2).map(|c| (c[4], c[5])).collect();
            assert!(mids.iter().all(|&(x, y)| x < pad || x > pad + 100.0 || y < pad || y > pad + 100.0), "bumps point inward: {mids:?}");
        }
    }

    #[test]
    fn delete_annotation_matches_listed_utf16_id() {
        let mut doc = one_page_doc();
        let annot = MarkupAnnotation {
            kind: MarkupKind::Underline,
            rects: vec![RectPt { x0: 72.0, y0: 100.0, x1: 200.0, y1: 120.0 }],
            color: [0.0, 0.0, 1.0],
            opacity: 1.0,
            contents: None,
            author: None,
            print: true,
        };
        let aid = add_markup_annotation(&mut doc, 0, &annot).unwrap();
        // /NM written by another tool as a UTF-16BE text string.
        doc.get_dictionary_mut(aid).unwrap().set("NM", pdf_string_utf16be("Kommentar-Ä1"));
        let listed = crate::read::list_annotations(&doc);
        assert_eq!(listed[0].id, "Kommentar-Ä1");
        assert!(delete_annotation(&mut doc, 0, &listed[0].id).unwrap());
        assert!(crate::read::list_annotations(&doc).is_empty());
    }

    #[test]
    fn listed_rect_matches_written_rect_on_rotated_page() {
        let mut doc = one_page_doc();
        let page_id = doc.page_iter().next().unwrap();
        doc.get_dictionary_mut(page_id).unwrap().set("Rotate", Object::Integer(90));
        // Displayed (rotated) top-origin space, as the UI sends it.
        let r = RectPt { x0: 100.0, y0: 50.0, x1: 300.0, y1: 80.0 };
        let annot = MarkupAnnotation {
            kind: MarkupKind::Highlight,
            rects: vec![r],
            color: [1.0, 1.0, 0.0],
            opacity: 0.5,
            contents: None,
            author: None,
            print: true,
        };
        add_markup_annotation(&mut doc, 0, &annot).unwrap();
        let got = crate::read::list_annotations(&doc)[0].rect_pt;
        for (a, b) in [(got.x0, r.x0), (got.y0, r.y0), (got.x1, r.x1), (got.y1, r.y1)] {
            assert!((a - b).abs() < 0.01, "wrote {r:?}, listed {got:?}");
        }
    }

    /// Turns a point of a box of `size` (PDF y-up space) by `turns` clockwise
    /// quarter turns, the way page /Rotate and the viewer rotation turn an
    /// appearance on screen. Returns the point and the turned box size.
    fn turn_cw(p: (f32, f32), size: (f32, f32), turns: u32) -> ((f32, f32), (f32, f32)) {
        (0..turns % 4).fold((p, size), |((x, y), (w, h)), _| ((y, w - x), (h, w)))
    }

    #[test]
    fn freetext_reads_upright_on_rotated_page() {
        // Issue #11: on a page with its own /Rotate (CAD plans are often a
        // portrait MediaBox shown as landscape this way) a new text box came
        // out turned by that angle: only the viewer rotation was compensated.
        for page_rot in [0_i64, 90, 180, 270, -90, 450] {
            for view_q in 0..4_u8 {
                let mut doc = one_page_doc();
                let page_id = doc.page_iter().next().unwrap();
                doc.get_dictionary_mut(page_id)
                    .unwrap()
                    .set("Rotate", Object::Integer(page_rot));
                let mut annot = freetext("Tor");
                annot.rotation_quadrant = view_q;
                let id = add_freetext_annotation(&mut doc, 0, &annot).unwrap();

                let (form, content) = markup_ap_stream(&doc, id);
                let content = String::from_utf8_lossy(&content);
                let ctx = format!("/Rotate {page_rot}, view {view_q}:\n{content}");
                let tm: Vec<f32> = content
                    .lines()
                    .find(|l| l.ends_with(" Tm"))
                    .expect("text matrix")
                    .split_whitespace()
                    .take(6)
                    .map(|t| t.parse().unwrap())
                    .collect();
                let bbox: Vec<f32> = form
                    .get(b"BBox")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| o.as_float().unwrap())
                    .collect();
                let size = (bbox[2] - bbox[0], bbox[3] - bbox[1]);
                let turns = (page_rot.rem_euclid(360) / 90) as u32 + view_q as u32;

                // On screen the baseline runs left to right and the glyphs
                // stand upright (a zero-size box turns plain vectors).
                let on_screen = |v: (f32, f32)| turn_cw(v, (0.0, 0.0), turns).0;
                assert_eq!(on_screen((tm[0], tm[1])), (1.0, 0.0), "baseline, {ctx}");
                assert_eq!(on_screen((tm[2], tm[3])), (0.0, 1.0), "glyph up, {ctx}");

                // The first line starts at the box's top-left corner as shown.
                let ((x, y), (_, h)) = turn_cw((tm[4], tm[5]), size, turns);
                let top = h - y - annot.font_size;
                assert!((x - 2.0).abs() < 0.02, "left margin {x}, {ctx}");
                assert!((top - 2.0).abs() < 0.02, "top margin {top}, {ctx}");
            }
        }
    }
}
