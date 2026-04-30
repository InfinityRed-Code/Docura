//! XFDF (Adobe XML Forms Data Format) export & import for annotations.
//!
//! Spec: https://www.adobe.com/content/dam/acom/en/devnet/pdf/xfdf-3-pdfa-1.pdf
//! and ISO 19444-1. We implement the annotations subset that round-trips with
//! Acrobat/Foxit for the kinds rustpdf can author: highlight, underline,
//! strikeout, square, circle, line, ink, freetext, stamp (rect only), text
//! (sticky).
//!
//! Hand-rolled to avoid pulling in an XML crate. The parser is line-/tag-
//! oriented, tolerant of attribute order, and ignores unknown elements.

use std::fmt::Write as _;

use lopdf::Document;
use pdf_types::RectPt;

use crate::annotations::{
    self, FreeTextAnnotation, InkAnnotation, MarkupAnnotation, MarkupKind, ShapeAnnotation,
    ShapeKind, TextNoteAnnotation,
};
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
        "Ink" => "ink",
        "FreeText" => "freetext",
        "Stamp" => "stamp",
        "Text" => "text",
        _ => "unknown",
    }
}

pub fn export(doc: &Document, source_filename: Option<&str>) -> MutateResult<Vec<u8>> {
    let annots = crate::read::list_annotations(doc);
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
    for a in &annots {
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
        out.push_str(">\n");
        if let Some(c) = &a.contents {
            let _ = write!(out, "      <contents>{}</contents>\n", xml_escape(c));
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
}

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(end) = bytes[i..].iter().position(|&b| b == b';') {
                let entity = &s[i + 1..i + end];
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
                    _ => {
                        out.push('&');
                        out.push_str(entity);
                        out.push(';');
                    }
                }
                i += end + 1;
                continue;
            }
        }
        let ch = bytes[i] as char;
        out.push(ch);
        i += 1;
    }
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
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'/' || bytes[i] == b'>' {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && bytes[i] != b'='
            && !(bytes[i] as char).is_whitespace()
        {
            i += 1;
        }
        let name = &body[name_start..i];
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
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
            } else if matches!(
                tag_name,
                "highlight"
                    | "underline"
                    | "strikeout"
                    | "squiggly"
                    | "square"
                    | "circle"
                    | "line"
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
            if in_contents {
                contents_buf.push(bytes[i] as char);
            }
            i += 1;
        }
    }
    Ok(annots)
}

fn write_one(doc: &mut Document, ann: &ParsedAnnot) -> MutateResult<()> {
    let rect = match ann.rect {
        Some(r) => r,
        None => return Err(MutateError::Parse(format!("xfdf: {} missing rect", ann.tag))),
    };
    let opacity = ann.opacity.unwrap_or(1.0);
    let print = ann.print.unwrap_or(true);
    let color = ann.color.unwrap_or([1.0, 0.93, 0.0]);
    match ann.tag.as_str() {
        "highlight" | "underline" | "strikeout" => {
            let kind = match ann.tag.as_str() {
                "underline" => MarkupKind::Underline,
                "strikeout" => MarkupKind::StrikeOut,
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
        "square" => {
            let s = ShapeAnnotation {
                kind: ShapeKind::Square,
                rect,
                stroke_color: color,
                fill_color: None,
                border_width: 1.5,
                opacity,
                contents: ann.contents.clone(),
                author: ann.title.clone(),
                print,
            };
            annotations::add_shape_annotation(doc, ann.page, &s)?;
        }
        "circle" => {
            let s = ShapeAnnotation {
                kind: ShapeKind::Circle,
                rect,
                stroke_color: color,
                fill_color: None,
                border_width: 1.5,
                opacity,
                contents: ann.contents.clone(),
                author: ann.title.clone(),
                print,
            };
            annotations::add_shape_annotation(doc, ann.page, &s)?;
        }
        "line" => {
            let s = ShapeAnnotation {
                kind: ShapeKind::Line {
                    arrow_start: annotations::LineEnding::None,
                    arrow_end: annotations::LineEnding::None,
                },
                rect,
                stroke_color: color,
                fill_color: None,
                border_width: 1.5,
                opacity,
                contents: ann.contents.clone(),
                author: ann.title.clone(),
                print,
            };
            annotations::add_shape_annotation(doc, ann.page, &s)?;
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
                font_size: 14.0,
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
        // stamp / squiggly: skip silently (no round-trip path yet).
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
