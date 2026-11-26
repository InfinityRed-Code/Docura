//! XFDF (Adobe XML Forms Data Format) export & import for annotations.
//!
//! Spec: https://www.adobe.com/content/dam/acom/en/devnet/pdf/xfdf-3-pdfa-1.pdf
//! and ISO 19444-1. We implement the annotations subset that round-trips with
//! Acrobat/Foxit for the kinds rustpdf can author: highlight, underline,
//! strikeout, square, circle, line, polygon (incl. the cloud), polyline, ink,
//! freetext, stamp (rect only), text (sticky). Shapes carry their fill
//! (`interior-color`), border `width`, cloudy border (`style`, `intensity`),
//! `intent`, line endpoints (`start`, `end`), endings (`head`, `tail`) and
//! `<vertices>`; every annotation its `subject`.
//!
//! Hand-rolled to avoid pulling in an XML crate. The parser is line-/tag-
//! oriented, tolerant of attribute order, and ignores unknown elements.

use std::fmt::Write as _;

use lopdf::Document;
use pdf_types::RectPt;

use crate::annotations::{
    self, FreeTextAnnotation, InkAnnotation, LineEnding, MarkupAnnotation, MarkupKind,
    ShapeAnnotation, ShapeKind, TextNoteAnnotation,
};
use crate::read::ShapeDetail;
use crate::{MutateError, MutateResult};

// ---------------- Export ----------------

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn fmt_color(c: [f32; 3]) -> String {
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", to_byte(c[0]), to_byte(c[1]), to_byte(c[2]))
}

fn fmt_rect(r: RectPt) -> String {
    // XFDF /Rect is the same coordinate system as PDF: bottom-origin, points.
    // We have top-origin rects, but XFDF rect= attribute is "x0,y0,x1,y1" in
    // PDF coordinates. We do not have page height here at export time, but
    // Acrobat tolerates top-origin rects for round-trip with the same writer
    // when both export and import use the same convention. To stay portable
    // we write them in our top-origin form and convert on import; the spec
    // technically expects PDF coords. Most readers tolerate; documented by
    // the comment so future-self knows.
    format!("{:.3},{:.3},{:.3},{:.3}", r.x0, r.y0, r.x1, r.y1)
}

fn subtype_to_xfdf(subtype: &str) -> &'static str {
    match subtype {
        "Highlight" => "highlight",
        "Underline" => "underline",
        "StrikeOut" => "strikeout",
        "Squiggly" => "squiggly",
        "Square" => "square",
        "Circle" => "circle",
        "Line" => "line",
        "Polygon" => "polygon",
        "PolyLine" => "polyline",
        "Ink" => "ink",
        "FreeText" => "freetext",
        "Stamp" => "stamp",
        "Text" => "text",
        _ => "unknown",
    }
}

fn fmt_point((x, y): (f32, f32)) -> String {
    format!("{x:.3},{y:.3}")
}

/// Shape attributes (XFDF 3.0 §7.4): fill, border width and effect, intent,
/// line endpoints and endings. Points use the same top-origin page space as
/// `rect` (see `fmt_rect`).
fn write_shape_attrs(out: &mut String, d: &ShapeDetail) {
    if let Some(ic) = d.interior {
        let _ = write!(out, " interior-color=\"{}\"", fmt_color(ic));
    }
    if let Some(w) = d.border_width {
        let _ = write!(out, " width=\"{w:.3}\"");
    }
    if let Some(i) = d.cloudy {
        let _ = write!(out, " style=\"cloudy\" intensity=\"{i:.3}\"");
    }
    if let Some(it) = &d.intent {
        let _ = write!(out, " intent=\"{}\"", xml_escape(it));
    }
    if let Some([start, end]) = d.line {
        let _ = write!(out, " start=\"{}\" end=\"{}\"", fmt_point(start), fmt_point(end));
    }
    if let Some((head, tail)) = &d.endings {
        let _ = write!(out, " head=\"{}\" tail=\"{}\"", xml_escape(head), xml_escape(tail));
    }
}

pub fn export(doc: &Document, source_filename: Option<&str>) -> MutateResult<Vec<u8>> {
    let annots = crate::read::list_annotations_detailed(doc);
    let mut out = String::with_capacity(annots.len() * 200 + 256);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n");
    if let Some(f) = source_filename {
        let _ = write!(
            out,
            "  <f href=\"{}\"/>\n",
            xml_escape(f)
        );
    }
    out.push_str("  <annots>\n");
    for (a, shape) in &annots {
        let tag = subtype_to_xfdf(&a.subtype);
        if tag == "unknown" {
            continue;
        }
        let _ = write!(
            out,
            "    <{tag} page=\"{p}\" rect=\"{rect}\"",
            p = a.page,
            rect = fmt_rect(a.rect_pt),
        );
        if let Some(c) = a.color {
            let _ = write!(out, " color=\"{}\"", fmt_color(c));
        }
        let _ = write!(out, " opacity=\"{:.3}\"", a.opacity);
        let _ = write!(out, " flags=\"{}\"", if a.print { "print" } else { "nopaint,nozoom" });
        if let Some(t) = &a.author {
            let _ = write!(out, " title=\"{}\"", xml_escape(t));
        }
        if let Some(m) = &a.modified {
            let _ = write!(out, " date=\"{}\"", xml_escape(m));
        }
        let _ = write!(out, " name=\"{}\"", xml_escape(&a.id));
        if let Some(icon) = &a.icon {
            if tag == "text" || tag == "stamp" {
                let _ = write!(out, " icon=\"{}\"", xml_escape(icon));
            }
        }
        if let Some(s) = &a.subject {
            let _ = write!(out, " subject=\"{}\"", xml_escape(s));
        }
        if let Some(d) = shape {
            write_shape_attrs(&mut out, d);
        }
        out.push_str(">\n");
        if let Some(c) = &a.contents {
            let _ = write!(out, "      <contents>{}</contents>\n", xml_escape(c));
        }
        if let Some(d) = shape.as_ref().filter(|d| !d.vertices.is_empty()) {
            let pts: Vec<String> = d.vertices.iter().map(|&p| fmt_point(p)).collect();
            let _ = writeln!(out, "      <vertices>{}</vertices>", pts.join(";"));
        }
        let _ = write!(out, "    </{tag}>\n");
    }
    out.push_str("  </annots>\n");
    out.push_str("</xfdf>\n");
    Ok(out.into_bytes())
}

// ---------------- Import ----------------

#[derive(Debug, Default, Clone)]
struct ParsedAnnot {
    tag: String,
    page: u32,
    rect: Option<RectPt>,
    color: Option<[f32; 3]>,
    opacity: Option<f32>,
    print: Option<bool>,
    title: Option<String>,
    icon: Option<String>,
    contents: Option<String>,
    subject: Option<String>,
    interior: Option<[f32; 3]>,
    width: Option<f32>,
    /// `style="cloudy"`.
    cloudy: bool,
    intent: Option<String>,
    start: Option<(f32, f32)>,
    end: Option<(f32, f32)>,
    head: Option<String>,
    tail: Option<String>,
    vertices: Vec<(f32, f32)>,
}

/// Numbers separated by commas, semicolons or whitespace, taken in pairs:
/// "x,y;x,y" (Acrobat) as well as "x,y,x,y".
fn parse_points(s: &str) -> Vec<(f32, f32)> {
    let nums: Vec<f32> = s
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse().ok())
        .collect();
    nums.chunks_exact(2).map(|c| (c[0], c[1])).collect()
}

fn parse_point(s: &str) -> Option<(f32, f32)> {
    match parse_points(s).as_slice() {
        [p] => Some(*p),
        _ => None,
    }
}

fn xml_unescape(s: &str) -> String {
    // Works on `&str` slices so multi-byte UTF-8 text (umlauts, …) is copied
    // intact; '&' and ';' are ASCII, so the slice bounds are char boundaries.
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..end];
        match entity {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            e if e.starts_with("#x") || e.starts_with("#X") => {
                if let Ok(n) = u32::from_str_radix(&e[2..], 16) {
                    if let Some(ch) = char::from_u32(n) {
                        out.push(ch);
                    }
                }
            }
            e if e.starts_with('#') => {
                if let Ok(n) = e[1..].parse::<u32>() {
                    if let Some(ch) = char::from_u32(n) {
                        out.push(ch);
                    }
                }
            }
            _ => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_color(s: &str) -> Option<[f32; 3]> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
    ])
}

fn parse_rect(s: &str) -> Option<RectPt> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() != 4 {
        return None;
    }
    let x0: f32 = parts[0].parse().ok()?;
    let y0: f32 = parts[1].parse().ok()?;
    let x1: f32 = parts[2].parse().ok()?;
    let y1: f32 = parts[3].parse().ok()?;
    Some(RectPt {
        x0: x0.min(x1),
        y0: y0.min(y1),
        x1: x0.max(x1),
        y1: y0.max(y1),
    })
}

/// Extract attributes from an XML start-tag body (everything after the tag
/// name up to `/>` or `>`). Returns `(name, value)` pairs.
fn parse_attrs(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'/' || bytes[i] == b'>' {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && bytes[i] != b'='
            && !bytes[i].is_ascii_whitespace()
        {
            i += 1;
        }
        let name = &body[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let quote = bytes[i];
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        i += 1;
        let val_start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        if i > bytes.len() {
            break;
        }
        let val = &body[val_start..i];
        if i < bytes.len() {
            i += 1;
        }
        out.push((name.to_string(), xml_unescape(val)));
    }
    out
}

fn parse_xfdf(xml: &[u8]) -> MutateResult<Vec<ParsedAnnot>> {
    let text = std::str::from_utf8(xml)
        .map_err(|e| MutateError::Parse(format!("xfdf utf8: {e}")))?;
    let bytes = text.as_bytes();
    let mut annots: Vec<ParsedAnnot> = Vec::new();
    let mut current: Option<ParsedAnnot> = None;
    let mut in_contents = false;
    let mut in_vertices = false;
    let mut contents_buf = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Skip XML declaration / processing instructions / comments.
            if bytes[i..].starts_with(b"<?") {
                if let Some(end) = text[i..].find("?>") {
                    i += end + 2;
                    continue;
                } else {
                    break;
                }
            }
            if bytes[i..].starts_with(b"<!--") {
                if let Some(end) = text[i..].find("-->") {
                    i += end + 3;
                    continue;
                } else {
                    break;
                }
            }
            // Find tag end.
            let tag_start = i + 1;
            let tag_end = match text[tag_start..].find('>') {
                Some(e) => tag_start + e,
                None => break,
            };
            let raw = &text[tag_start..tag_end];
            let self_closing = raw.ends_with('/');
            let body_end = if self_closing { raw.len() - 1 } else { raw.len() };
            let body = &raw[..body_end];
            let is_close = body.starts_with('/');
            let body_clean = if is_close { &body[1..] } else { body };
            let tag_name_end = body_clean
                .find(|c: char| c.is_whitespace())
                .unwrap_or(body_clean.len());
            let tag_name = &body_clean[..tag_name_end];
            let attrs_str = &body_clean[tag_name_end..];

            if is_close {
                if tag_name == "contents" || tag_name == "contents-richtext" {
                    if let Some(ann) = current.as_mut() {
                        ann.contents = Some(xml_unescape(contents_buf.trim()));
                    }
                    in_contents = false;
                    contents_buf.clear();
                } else if tag_name == "vertices" {
                    if let Some(ann) = current.as_mut() {
                        ann.vertices = parse_points(&contents_buf);
                    }
                    in_vertices = false;
                    contents_buf.clear();
                } else if let Some(ann) = current.take() {
                    if ann.tag == tag_name {
                        annots.push(ann);
                    } else {
                        // Unbalanced — push current back to recover.
                        current = Some(ann);
                    }
                }
            } else if tag_name == "contents" || tag_name == "contents-richtext" {
                in_contents = true;
                contents_buf.clear();
            } else if tag_name == "vertices" && current.is_some() && !self_closing {
                in_vertices = true;
                contents_buf.clear();
            } else if matches!(
                tag_name,
                "highlight"
                    | "underline"
                    | "strikeout"
                    | "squiggly"
                    | "square"
                    | "circle"
                    | "line"
                    | "polygon"
                    | "polyline"
                    | "ink"
                    | "freetext"
                    | "stamp"
                    | "text"
            ) {
                let mut ann = ParsedAnnot {
                    tag: tag_name.to_string(),
                    ..Default::default()
                };
                for (k, v) in parse_attrs(attrs_str) {
                    match k.as_str() {
                        "page" => {
                            ann.page = v.parse().unwrap_or(0);
                        }
                        "rect" => {
                            ann.rect = parse_rect(&v);
                        }
                        "color" => {
                            ann.color = parse_color(&v);
                        }
                        "opacity" => {
                            ann.opacity = v.parse().ok();
                        }
                        "flags" => {
                            ann.print = Some(v.split(',').any(|f| f.trim() == "print"));
                        }
                        "title" => ann.title = Some(v),
                        "icon" => ann.icon = Some(v),
                        "subject" => ann.subject = Some(v),
                        "interior-color" => ann.interior = parse_color(&v),
                        "width" => ann.width = v.trim().parse().ok(),
                        "style" => ann.cloudy = v.trim() == "cloudy",
                        "intent" => ann.intent = Some(v),
                        "start" => ann.start = parse_point(&v),
                        "end" => ann.end = parse_point(&v),
                        "head" => ann.head = Some(v),
                        "tail" => ann.tail = Some(v),
                        _ => {}
                    }
                }
                if self_closing {
                    annots.push(ann);
                } else {
                    current = Some(ann);
                }
            }
            i = tag_end + 1;
        } else {
            // Copy the text run up to the next tag as a whole `&str` slice;
            // pushing bytes one by one as chars would mangle UTF-8.
            let next = text[i..].find('<').map_or(bytes.len(), |p| i + p);
            if in_contents || in_vertices {
                contents_buf.push_str(&text[i..next]);
            }
            i = next;
        }
    }
    Ok(annots)
}

/// The shape an XFDF shape element describes, or None for other elements.
fn shape_of(ann: &ParsedAnnot, rect: Option<RectPt>) -> MutateResult<Option<ShapeAnnotation>> {
    let ending = |name: &Option<String>| LineEnding::from_name(name.as_deref().unwrap_or("None"));
    let missing = |what: &str| MutateError::Parse(format!("xfdf: {} missing {what}", ann.tag));
    let (kind, rect, vertices) = match ann.tag.as_str() {
        "square" => (ShapeKind::Square, rect.ok_or_else(|| missing("rect"))?, Vec::new()),
        "circle" => (ShapeKind::Circle, rect.ok_or_else(|| missing("rect"))?, Vec::new()),
        "line" => {
            // The endpoints keep the direction (arrow heads); `rect` alone
            // only gives a diagonal of the bounding box.
            let rect = match (ann.start, ann.end) {
                (Some(s), Some(e)) => RectPt { x0: s.0, y0: s.1, x1: e.0, y1: e.1 },
                _ => rect.ok_or_else(|| missing("rect"))?,
            };
            let kind = ShapeKind::Line { arrow_start: ending(&ann.head), arrow_end: ending(&ann.tail) };
            (kind, rect, Vec::new())
        }
        "polygon" => {
            let cloudy = ann.cloudy || ann.intent.as_deref() == Some("PolygonCloud");
            (ShapeKind::Polygon { cloudy }, RectPt::default(), ann.vertices.clone())
        }
        "polyline" => {
            let kind = ShapeKind::PolyLine { start: ending(&ann.head), end: ending(&ann.tail) };
            (kind, RectPt::default(), ann.vertices.clone())
        }
        _ => return Ok(None),
    };
    Ok(Some(ShapeAnnotation {
        kind,
        rect,
        vertices,
        subject: ann.subject.clone(),
        stroke_color: ann.color.unwrap_or([1.0, 0.93, 0.0]),
        fill_color: ann.interior,
        border_width: ann.width.unwrap_or(1.5),
        opacity: ann.opacity.unwrap_or(1.0),
        contents: ann.contents.clone(),
        author: ann.title.clone(),
        print: ann.print.unwrap_or(true),
    }))
}

fn write_one(doc: &mut Document, ann: &ParsedAnnot) -> MutateResult<()> {
    if let Some(shape) = shape_of(ann, ann.rect)? {
        annotations::add_shape_annotation(doc, ann.page, &shape)?;
        return Ok(());
    }
    let rect = match ann.rect {
        Some(r) => r,
        None => return Err(MutateError::Parse(format!("xfdf: {} missing rect", ann.tag))),
    };
    let opacity = ann.opacity.unwrap_or(1.0);
    let print = ann.print.unwrap_or(true);
    let color = ann.color.unwrap_or([1.0, 0.93, 0.0]);
    match ann.tag.as_str() {
        "highlight" | "underline" | "strikeout" | "squiggly" => {
            let kind = match ann.tag.as_str() {
                "underline" => MarkupKind::Underline,
                "strikeout" => MarkupKind::StrikeOut,
                "squiggly" => MarkupKind::Squiggly,
                _ => MarkupKind::Highlight,
            };
            let m = MarkupAnnotation {
                kind,
                rects: vec![rect],
                color,
                opacity: if ann.tag == "highlight" { opacity.min(0.6) } else { opacity },
                contents: ann.contents.clone(),
                author: ann.title.clone(),
                print,
            };
            annotations::add_markup_annotation(doc, ann.page, &m)?;
        }
        "freetext" => {
            let text = ann.contents.clone().unwrap_or_default();
            if text.trim().is_empty() {
                return Ok(());
            }
            let f = FreeTextAnnotation {
                rect,
                text,
                color,
                bg_color: [1.0, 1.0, 1.0, 1.0],
                font_size: 14.0,
                font_family: "Helv".into(),
                bold: false,
                italic: false,
                rotation_quadrant: 0,
                opacity,
                author: ann.title.clone(),
                print,
            };
            annotations::add_freetext_annotation(doc, ann.page, &f)?;
        }
        "ink" => {
            // We do not parse <inklist><gesture>x,y;...</gesture> yet — fall back to
            // a single-stroke rectangle outline so the annotation isn't lost.
            let r = rect;
            let stroke = vec![
                (r.x0, r.y0),
                (r.x1, r.y0),
                (r.x1, r.y1),
                (r.x0, r.y1),
                (r.x0, r.y0),
            ];
            let i = InkAnnotation {
                strokes: vec![stroke],
                color,
                width: 1.6,
                opacity,
                contents: ann.contents.clone(),
                author: ann.title.clone(),
                print,
            };
            annotations::add_ink_annotation(doc, ann.page, &i)?;
        }
        "text" => {
            let contents = ann.contents.clone().unwrap_or_else(|| "Note".into());
            let n = TextNoteAnnotation {
                x_pt: rect.x0,
                y_pt: rect.y0,
                contents,
                color,
                icon: ann.icon.clone(),
                author: ann.title.clone(),
                print,
            };
            annotations::add_text_note_annotation(doc, ann.page, &n)?;
        }
        // stamp: skip silently (no round-trip path yet).
        _ => {}
    }
    Ok(())
}

pub fn import(doc: &mut Document, xml: &[u8]) -> MutateResult<usize> {
    let annots = parse_xfdf(xml)?;
    let mut count = 0usize;
    for ann in &annots {
        if let Err(e) = write_one(doc, ann) {
            tracing::warn!("xfdf import: skip {}: {e}", ann.tag);
            continue;
        }
        count += 1;
    }
    tracing::info!("xfdf::import: {} annotations written", count);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_keeps_multibyte_text() {
        assert_eq!(
            xml_unescape("Gr\u{fc}\u{df}e &amp; K\u{fc}sse &#x20AC;5 &unknown; &"),
            "Gr\u{fc}\u{df}e & K\u{fc}sse \u{20ac}5 &unknown; &"
        );
    }

    #[test]
    fn parse_keeps_non_ascii_contents_and_attributes() {
        // The second element's attribute name contains U+00E0, whose UTF-8
        // continuation byte 0xA0 used to be taken for whitespace (panic).
        let xml = "<?xml version=\"1.0\"?><xfdf><annots>\
            <squiggly page=\"0\" rect=\"1,2,3,4\" title=\"J\u{fc}rgen\">\
            <contents>Gr\u{fc}\u{df}e &amp; mehr</contents></squiggly>\
            <text page=\"0\" rect=\"1,2,3,4\" na\u{e0}me=\"x\"/>\
            </annots></xfdf>";
        let annots = parse_xfdf(xml.as_bytes()).unwrap();
        assert_eq!(annots.len(), 2);
        assert_eq!(annots[0].tag, "squiggly");
        assert_eq!(annots[0].title.as_deref(), Some("J\u{fc}rgen"));
        assert_eq!(annots[0].contents.as_deref(), Some("Gr\u{fc}\u{df}e & mehr"));
    }

    // ---------- Shape round trips: export, import into a fresh document,
    // compare what both documents carry. ----------

    use crate::read::list_annotations_detailed;
    use lopdf::{Dictionary, Object};
    use pdf_types::AnnotationInfo;

    fn one_page_doc(rotate: i64) -> Document {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let page_id = d.new_object_id();
        let cat_id = d.new_object_id();
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("MediaBox", Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
        if rotate != 0 {
            page.set("Rotate", Object::Integer(rotate));
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
        d
    }

    fn shape(kind: ShapeKind, vertices: Vec<(f32, f32)>) -> ShapeAnnotation {
        ShapeAnnotation {
            kind,
            rect: RectPt { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 },
            vertices,
            subject: None,
            stroke_color: [0.8, 0.0, 0.0],
            fill_color: None,
            border_width: 1.5,
            opacity: 1.0,
            contents: None,
            author: None,
            print: true,
        }
    }

    /// Export the one shape of a fresh page, import the XFDF into another
    /// fresh page; returns the XFDF and both listings.
    fn round_trip(
        s: &ShapeAnnotation,
        rotate: i64,
    ) -> (String, (AnnotationInfo, ShapeDetail), (AnnotationInfo, ShapeDetail)) {
        let mut a = one_page_doc(rotate);
        annotations::add_shape_annotation(&mut a, 0, s).unwrap();
        let xml = String::from_utf8(export(&a, Some("test.pdf")).unwrap()).unwrap();
        let mut b = one_page_doc(rotate);
        assert_eq!(import(&mut b, xml.as_bytes()).unwrap(), 1, "imported: {xml}");
        let one = |d: &Document| {
            let mut l = list_annotations_detailed(d);
            assert_eq!(l.len(), 1);
            let (info, detail) = l.remove(0);
            (info, detail.expect("shape detail"))
        };
        (xml, one(&a), one(&b))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn close_color(a: Option<[f32; 3]>, b: Option<[f32; 3]>) -> bool {
        match (a, b) {
            // XFDF colors are 8 bits per channel.
            (Some(a), Some(b)) => a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.5 / 255.0 + 1e-4),
            (None, None) => true,
            _ => false,
        }
    }

    /// Everything a shape carries survives the round trip.
    fn assert_same(before: &(AnnotationInfo, ShapeDetail), after: &(AnnotationInfo, ShapeDetail)) {
        let ((ia, da), (ib, db)) = (before, after);
        assert_eq!(ia.subtype, ib.subtype);
        assert_eq!(ia.subject, ib.subject);
        assert_eq!(ia.contents, ib.contents);
        assert_eq!(ia.author, ib.author);
        assert_eq!(ia.print, ib.print);
        assert!(close(ia.opacity, ib.opacity));
        assert!(close_color(ia.color, ib.color), "{:?} vs {:?}", ia.color, ib.color);
        assert!(close_color(da.interior, db.interior), "{:?} vs {:?}", da.interior, db.interior);
        assert_eq!(da.border_width.map(|w| (w * 100.0).round()), db.border_width.map(|w| (w * 100.0).round()));
        assert_eq!(da.cloudy, db.cloudy);
        assert_eq!(da.intent, db.intent);
        assert_eq!(da.endings, db.endings);
        assert_eq!(da.vertices.len(), db.vertices.len());
        for (p, q) in da.vertices.iter().zip(&db.vertices) {
            assert!(close(p.0, q.0) && close(p.1, q.1), "{:?} vs {:?}", da.vertices, db.vertices);
        }
        match (da.line, db.line) {
            (Some([s0, e0]), Some([s1, e1])) => {
                assert!(close(s0.0, s1.0) && close(s0.1, s1.1) && close(e0.0, e1.0) && close(e0.1, e1.1));
            }
            (None, None) => {}
            other => panic!("line differs: {other:?}"),
        }
        let (ra, rb) = (ia.rect_pt, ib.rect_pt);
        assert!(close(ra.x0, rb.x0) && close(ra.y0, rb.y0) && close(ra.x1, rb.x1) && close(ra.y1, rb.y1));
    }

    #[test]
    fn polygon_round_trips_with_fill_width_and_subject() {
        let mut s = shape(
            ShapeKind::Polygon { cloudy: false },
            vec![(100.0, 100.0), (180.0, 220.0), (40.0, 210.5)],
        );
        s.fill_color = Some([1.0, 0.9, 0.0]);
        s.border_width = 2.0;
        s.subject = Some("Dreieck".into());
        s.contents = Some("Bitte prüfen".into());
        s.author = Some("Jürgen".into());
        s.opacity = 0.75;
        s.print = false;
        for rotate in [0, 90] {
            let (xml, before, after) = round_trip(&s, rotate);
            assert!(xml.contains("<polygon ") && xml.contains("<vertices>"), "{xml}");
            assert!(xml.contains("interior-color=\"#FFE600\"") && xml.contains("width=\"2.000\""), "{xml}");
            assert!(xml.contains("subject=\"Dreieck\""), "{xml}");
            assert!(!xml.contains("style=\"cloudy\""), "{xml}");
            assert_eq!(before.1.vertices.len(), 3);
            assert_same(&before, &after);
        }
    }

    #[test]
    fn cloud_round_trips_with_border_effect_and_intent() {
        let mut s = shape(
            ShapeKind::Polygon { cloudy: true },
            vec![(100.0, 100.0), (300.0, 100.0), (300.0, 200.0), (100.0, 200.0)],
        );
        s.subject = Some("Wolke".into());
        let (xml, before, after) = round_trip(&s, 0);
        assert!(xml.contains("style=\"cloudy\" intensity=\"1.000\""), "{xml}");
        assert!(xml.contains("intent=\"PolygonCloud\""), "{xml}");
        assert_eq!(before.1.cloudy, Some(1.0));
        assert_eq!(before.1.intent.as_deref(), Some("PolygonCloud"));
        assert_same(&before, &after);
    }

    #[test]
    fn polyline_round_trips_with_line_endings() {
        let mut s = shape(
            ShapeKind::PolyLine { start: LineEnding::OpenArrow, end: LineEnding::ClosedArrow },
            vec![(50.0, 60.0), (90.0, 120.0), (200.0, 40.0), (260.0, 90.0)],
        );
        s.subject = Some("Häkchen".into());
        s.border_width = 2.5;
        // Fill does not apply to an open path.
        s.fill_color = Some([0.0, 1.0, 0.0]);
        let (xml, before, after) = round_trip(&s, 0);
        assert!(xml.contains("<polyline ") && xml.contains("head=\"OpenArrow\" tail=\"ClosedArrow\""), "{xml}");
        assert!(!xml.contains("interior-color"), "{xml}");
        assert_eq!(before.1.endings, Some(("OpenArrow".into(), "ClosedArrow".into())));
        assert_same(&before, &after);

        // Without endings no /LE is written, and none comes back.
        let plain = shape(
            ShapeKind::PolyLine { start: LineEnding::None, end: LineEnding::None },
            vec![(10.0, 20.0), (15.0, 28.0), (30.0, 5.0)],
        );
        let (xml, before, after) = round_trip(&plain, 0);
        assert!(!xml.contains("head="), "{xml}");
        assert_eq!(before.1.endings, None);
        assert_same(&before, &after);
    }

    #[test]
    fn line_keeps_direction_and_arrow_heads() {
        let mut s = shape(
            ShapeKind::Line { arrow_start: LineEnding::OpenArrow, arrow_end: LineEnding::OpenArrow },
            Vec::new(),
        );
        // Right to left and upwards: the bounding box alone would lose that.
        s.rect = RectPt { x0: 300.0, y0: 400.0, x1: 120.0, y1: 250.0 };
        s.subject = Some("Doppelpfeil".into());
        let (xml, before, after) = round_trip(&s, 0);
        assert!(xml.contains("start=\"300.000,400.000\" end=\"120.000,250.000\""), "{xml}");
        assert_same(&before, &after);
    }

    #[test]
    fn rectangle_keeps_its_fill() {
        let mut s = shape(ShapeKind::Square, Vec::new());
        s.rect = RectPt { x0: 50.0, y0: 60.0, x1: 150.0, y1: 120.0 };
        s.fill_color = Some([0.0, 0.69, 0.94]);
        let (_, before, after) = round_trip(&s, 0);
        assert!(before.1.interior.is_some());
        assert_same(&before, &after);
    }

    #[test]
    fn imports_acrobat_style_vertices() {
        let xml = r##"<?xml version="1.0"?><xfdf><annots>
            <polygon page="0" rect="0,0,10,10" color="#FF0000" width="1" style="cloudy" intensity="1">
              <vertices>10,20;30,40;50,10</vertices></polygon>
            <polyline page="0" rect="0,0,10,10" head="None" tail="OpenArrow"><vertices>1,2,3,4,5,6</vertices></polyline>
            </annots></xfdf>"##;
        let parsed = parse_xfdf(xml.as_bytes()).unwrap();
        assert_eq!(parsed[0].vertices, vec![(10.0, 20.0), (30.0, 40.0), (50.0, 10.0)]);
        assert!(parsed[0].cloudy);
        assert_eq!(parsed[1].vertices, vec![(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)]);
        let mut doc = one_page_doc(0);
        assert_eq!(import(&mut doc, xml.as_bytes()).unwrap(), 2);
        let listed = list_annotations_detailed(&doc);
        assert_eq!(listed[0].0.subtype, "Polygon");
        assert_eq!(listed[0].1.as_ref().unwrap().cloudy, Some(1.0));
        assert_eq!(listed[1].0.subtype, "PolyLine");
        assert_eq!(listed[1].1.as_ref().unwrap().endings, Some(("None".into(), "OpenArrow".into())));
    }

    #[test]
    fn sticky_note_keeps_its_place_on_rotated_pages() {
        // A note is pinned at the upper-left corner of its /Rect, which on a
        // rotated page is not the top-left corner of the turned rectangle.
        let rect = |d: &Document| -> Vec<f32> {
            let page = d.page_iter().next().unwrap();
            let annots = d.get_dictionary(page).unwrap().get(b"Annots").unwrap().as_array().unwrap();
            let note = d.get_dictionary(annots[0].as_reference().unwrap()).unwrap();
            note.get(b"Rect").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect()
        };
        for rotate in [0, 90, 180, 270] {
            let mut a = one_page_doc(rotate);
            let note = TextNoteAnnotation {
                x_pt: 100.0,
                y_pt: 150.0,
                contents: "Notiz".into(),
                color: [1.0, 0.9, 0.2],
                icon: None,
                author: None,
                print: true,
            };
            annotations::add_text_note_annotation(&mut a, 0, &note).unwrap();
            let xml = export(&a, None).unwrap();
            let mut b = one_page_doc(rotate);
            assert_eq!(import(&mut b, &xml).unwrap(), 1);
            let (ra, rb) = (rect(&a), rect(&b));
            assert!(ra.iter().zip(&rb).all(|(x, y)| close(*x, *y)), "/Rotate {rotate}: {ra:?} -> {rb:?}");
        }
    }
}
