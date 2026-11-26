//! Appearance streams for annotations that come without one.
//!
//! PDF/A-2 and later require an appearance for every visible annotation.
//! Creators normally write them; this covers the common markup types when
//! they do not. The streams use the annotation /Rect as /BBox with an
//! identity matrix, so drawing happens directly in page coordinates.

use std::io::Write;

use lopdf::{dictionary, Dictionary, Document, Object, Stream};

use crate::content::write_real;
use crate::util::{self, deref, num};

fn f(out: &mut Vec<u8>, v: f64) {
    write_real(out, v);
    out.push(b' ');
}

/// Colour operator for a /C or /IC array (`stroke` picks RG/G/K).
fn color_op(out: &mut Vec<u8>, c: &[f64], stroke: bool) -> bool {
    match c.len() {
        1 => {
            f(out, c[0]);
            out.extend_from_slice(if stroke { b"G\n" } else { b"g\n" });
        }
        3 => {
            for v in c {
                f(out, *v);
            }
            out.extend_from_slice(if stroke { b"RG\n" } else { b"rg\n" });
        }
        4 => {
            for v in c {
                f(out, *v);
            }
            out.extend_from_slice(if stroke { b"K\n" } else { b"k\n" });
        }
        _ => return false,
    }
    true
}

fn nums(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<Vec<f64>> {
    util::get_array(doc, d, key).map(|a| a.iter().filter_map(|x| num(deref(doc, x))).collect())
}

fn border_width(doc: &Document, d: &Dictionary) -> f64 {
    if let Some(bs) = util::get_dict(doc, d, b"BS") {
        if let Some(w) = util::get_num(doc, bs, b"W") {
            return w.max(0.0);
        }
    }
    if let Some(b) = nums(doc, d, b"Border") {
        if b.len() >= 3 {
            return b[2].max(0.0);
        }
    }
    1.0
}

pub fn form(rect: [f64; 4], content: Vec<u8>, resources: Option<Dictionary>) -> Stream {
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => Object::Array(rect.iter().map(|v| Object::Real(*v as f32)).collect()),
    };
    if let Some(r) = resources {
        dict.set("Resources", Object::Dictionary(r));
    }
    util::flate_stream(dict, &content)
}

pub fn empty_form(rect: [f64; 4]) -> Stream {
    form(rect, Vec::new(), None)
}

fn ellipse(out: &mut Vec<u8>, x0: f64, y0: f64, x1: f64, y1: f64) {
    let k = 0.5522847498;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let _ = writeln!(out, "{} {} m", cx + rx, cy);
    let seg = |out: &mut Vec<u8>, a: [f64; 6]| {
        for v in a {
            f(out, v);
        }
        out.extend_from_slice(b"c\n");
    };
    seg(out, [cx + rx, cy + k * ry, cx + k * rx, cy + ry, cx, cy + ry]);
    seg(out, [cx - k * rx, cy + ry, cx - rx, cy + k * ry, cx - rx, cy]);
    seg(out, [cx - rx, cy - k * ry, cx - k * rx, cy - ry, cx, cy - ry]);
    seg(out, [cx + k * rx, cy - ry, cx + rx, cy - k * ry, cx + rx, cy]);
}

/// Generate an appearance for `annot`, or `None` for types that need text
/// rendering or are not drawable.
pub fn generate(doc: &mut Document, annot: &Dictionary, rect: [f64; 4]) -> Option<Stream> {
    let subtype = util::get_name(doc, annot, b"Subtype")?.to_vec();
    let c = nums(doc, annot, b"C").unwrap_or_default();
    let ic = nums(doc, annot, b"IC").unwrap_or_default();
    let w = border_width(doc, annot);
    let mut o = Vec::new();
    o.extend_from_slice(b"q\n");
    let mut res: Option<Dictionary> = None;
    match subtype.as_slice() {
        b"Square" | b"Circle" | b"Redact" => {
            let fill = color_op(&mut o, &ic, false);
            let stroke = w > 0.0 && color_op(&mut o, if subtype == b"Redact" && c.is_empty() { &[1.0, 0.0, 0.0] } else { &c }, true);
            f(&mut o, w);
            o.extend_from_slice(b"w\n");
            let h = w / 2.0;
            let (x0, y0, x1, y1) = (rect[0] + h, rect[1] + h, rect[2] - h, rect[3] - h);
            if subtype == b"Circle" {
                ellipse(&mut o, x0, y0, x1, y1);
            } else {
                for v in [x0, y0, x1 - x0, y1 - y0] {
                    f(&mut o, v);
                }
                o.extend_from_slice(b"re\n");
            }
            o.extend_from_slice(match (fill, stroke) {
                (true, true) => b"B\n",
                (true, false) => b"f\n",
                (false, true) => b"S\n",
                _ => b"n\n",
            });
        }
        b"Line" | b"Polygon" | b"PolyLine" | b"Ink" => {
            if !color_op(&mut o, &c, true) {
                o.extend_from_slice(b"0 G\n");
            }
            f(&mut o, w.max(0.5));
            o.extend_from_slice(b"w 1 J 1 j\n");
            let paths: Vec<Vec<f64>> = match subtype.as_slice() {
                b"Line" => vec![nums(doc, annot, b"L")?],
                b"Ink" => util::get_array(doc, annot, b"InkList")?
                    .iter()
                    .map(|p| match deref(doc, p) {
                        Object::Array(a) => a.iter().filter_map(|x| num(deref(doc, x))).collect(),
                        _ => Vec::new(),
                    })
                    .collect(),
                _ => vec![nums(doc, annot, b"Vertices")?],
            };
            for p in paths {
                for (i, pt) in p.chunks(2).enumerate() {
                    if pt.len() < 2 {
                        continue;
                    }
                    f(&mut o, pt[0]);
                    f(&mut o, pt[1]);
                    o.extend_from_slice(if i == 0 { b"m\n" } else { b"l\n" });
                }
                if subtype == b"Polygon" {
                    o.extend_from_slice(b"h\n");
                }
            }
            if subtype == b"Polygon" && color_op(&mut o, &ic, false) {
                o.extend_from_slice(b"B\n");
            } else {
                o.extend_from_slice(b"S\n");
            }
        }
        b"Highlight" | b"Underline" | b"StrikeOut" | b"Squiggly" => {
            let q = nums(doc, annot, b"QuadPoints")?;
            let col: Vec<f64> = if c.is_empty() { vec![1.0, 1.0, 0.0] } else { c.clone() };
            if subtype == b"Highlight" {
                let mut gs = Dictionary::new();
                gs.set("GSm", dictionary! { "Type" => "ExtGState", "BM" => "Multiply" });
                let mut r = Dictionary::new();
                r.set("ExtGState", Object::Dictionary(gs));
                res = Some(r);
                o.extend_from_slice(b"/GSm gs\n");
                color_op(&mut o, &col, false);
            } else {
                color_op(&mut o, &col, true);
            }
            for quad in q.chunks(8) {
                if quad.len() < 8 {
                    continue;
                }
                // Points: x1 y1 x2 y2 (top), x3 y3 x4 y4 (bottom).
                let (minx, maxx) = quad.chunks(2).fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
                let (miny, maxy) = quad.chunks(2).fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
                let hgt = maxy - miny;
                match subtype.as_slice() {
                    b"Highlight" => {
                        for v in [minx, miny, maxx - minx, hgt] {
                            f(&mut o, v);
                        }
                        o.extend_from_slice(b"re f\n");
                    }
                    b"Squiggly" => {
                        let lw = (hgt / 18.0).max(0.5);
                        f(&mut o, lw);
                        o.extend_from_slice(b"w\n");
                        let base = miny + hgt * 0.08;
                        let step = (hgt / 6.0).max(1.0);
                        let mut x = minx;
                        let mut up = true;
                        f(&mut o, x);
                        f(&mut o, base);
                        o.extend_from_slice(b"m\n");
                        while x < maxx {
                            x = (x + step).min(maxx);
                            f(&mut o, x);
                            f(&mut o, if up { base + step / 2.0 } else { base });
                            o.extend_from_slice(b"l\n");
                            up = !up;
                        }
                        o.extend_from_slice(b"S\n");
                    }
                    _ => {
                        let lw = (hgt / 14.0).max(0.5);
                        let y = if subtype == b"StrikeOut" { miny + hgt * 0.45 } else { miny + hgt * 0.1 };
                        f(&mut o, lw);
                        o.extend_from_slice(b"w\n");
                        for v in [minx, y] {
                            f(&mut o, v);
                        }
                        o.extend_from_slice(b"m\n");
                        for v in [maxx, y] {
                            f(&mut o, v);
                        }
                        o.extend_from_slice(b"l S\n");
                    }
                }
            }
        }
        b"Text" | b"FileAttachment" | b"Caret" => {
            // A small note / paperclip / caret glyph drawn with paths.
            let col: Vec<f64> = if c.is_empty() { vec![1.0, 0.85, 0.2] } else { c.clone() };
            let (x0, y0) = (rect[0], rect[1]);
            let s = ((rect[2] - rect[0]).min(rect[3] - rect[1])).clamp(4.0, 24.0);
            color_op(&mut o, &col, false);
            o.extend_from_slice(b"0 G 0.6 w\n");
            match subtype.as_slice() {
                b"Caret" => {
                    f(&mut o, x0);
                    f(&mut o, y0);
                    o.extend_from_slice(b"m ");
                    f(&mut o, x0 + s / 2.0);
                    f(&mut o, y0 + s);
                    o.extend_from_slice(b"l ");
                    f(&mut o, x0 + s);
                    f(&mut o, y0);
                    o.extend_from_slice(b"l h B\n");
                }
                _ => {
                    for v in [x0 + 0.5, y0 + 0.5, s - 1.0, s - 1.0] {
                        f(&mut o, v);
                    }
                    o.extend_from_slice(b"re B\n");
                    for i in 1..4 {
                        let y = y0 + s * (i as f64) / 4.0;
                        for v in [x0 + s * 0.2, y] {
                            f(&mut o, v);
                        }
                        o.extend_from_slice(b"m ");
                        for v in [x0 + s * 0.8, y] {
                            f(&mut o, v);
                        }
                        o.extend_from_slice(b"l S\n");
                    }
                }
            }
        }
        b"Stamp" | b"FreeText" | b"Watermark" => {
            // Border/background only; text appearances come from creators.
            let fill = color_op(&mut o, &ic, false) || (subtype == b"FreeText" && color_op(&mut o, &c, false));
            let stroke = subtype != b"FreeText" && color_op(&mut o, if c.is_empty() { &[0.8, 0.1, 0.1] } else { &c }, true);
            if fill || stroke {
                f(&mut o, w.max(1.0));
                o.extend_from_slice(b"w\n");
                let h = w.max(1.0) / 2.0;
                for v in [rect[0] + h, rect[1] + h, rect[2] - rect[0] - 2.0 * h, rect[3] - rect[1] - 2.0 * h] {
                    f(&mut o, v);
                }
                o.extend_from_slice(match (fill, stroke) {
                    (true, true) => b"re B\n",
                    (true, false) => b"re f\n",
                    _ => b"re S\n",
                });
            } else {
                return None;
            }
        }
        _ => return None,
    }
    o.extend_from_slice(b"Q\n");
    Some(form(rect, o, res))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen(sub: &str, extra: Dictionary) -> Option<String> {
        let mut doc = Document::with_version("1.7");
        let mut d = dictionary! { "Subtype" => Object::Name(sub.as_bytes().to_vec()), "C" => vec![1.into(), 0.into(), 0.into()] };
        d.extend(&extra);
        generate(&mut doc, &d, [10.0, 10.0, 60.0, 40.0]).map(|s| {
            let data = s.decompressed_content().unwrap();
            String::from_utf8(data).unwrap()
        })
    }

    #[test]
    fn shapes() {
        let sq = gen("Square", dictionary! { "IC" => vec![0.into()] }).unwrap();
        assert!(sq.contains("re") && sq.contains("B"), "{sq}");
        let ci = gen("Circle", Dictionary::new()).unwrap();
        assert!(ci.contains(" c\n") && ci.contains("S"));
        let li = gen("Line", dictionary! { "L" => vec![10.into(), 10.into(), 50.into(), 30.into()] }).unwrap();
        assert!(li.contains(" l\n"));
        let pg = gen("Polygon", dictionary! { "Vertices" => vec![10.into(), 10.into(), 50.into(), 30.into(), 20.into(), 35.into()], "IC" => vec![0.5.into()] }).unwrap();
        assert!(pg.contains("h\n") && pg.contains("B"));
        let ink = gen("Ink", dictionary! { "InkList" => vec![Object::Array(vec![10.into(), 10.into(), 20.into(), 20.into()])] }).unwrap();
        assert!(ink.contains(" m\n"));
        assert!(gen("Line", Dictionary::new()).is_none());
    }

    #[test]
    fn markup_and_icons() {
        let q = dictionary! { "QuadPoints" => vec![10.into(), 30.into(), 50.into(), 30.into(), 10.into(), 20.into(), 50.into(), 20.into()] };
        let h = gen("Highlight", q.clone()).unwrap();
        assert!(h.contains("/GSm gs") && h.contains("re f"));
        for t in ["Underline", "StrikeOut", "Squiggly"] {
            let s = gen(t, q.clone()).unwrap();
            assert!(s.contains("S\n"), "{t}: {s}");
        }
        assert!(gen("Text", Dictionary::new()).unwrap().contains("re B"));
        assert!(gen("FileAttachment", Dictionary::new()).unwrap().contains("re B"));
        assert!(gen("Caret", Dictionary::new()).unwrap().contains("h B"));
        assert!(gen("Stamp", Dictionary::new()).unwrap().contains("re S"));
        assert!(gen("FreeText", Dictionary::new()).is_some());
        assert!(gen("Movie", Dictionary::new()).is_none());
        let mut c = Vec::new();
        assert!(color_op(&mut c, &[0.1, 0.2, 0.3, 0.4], true));
        assert!(!color_op(&mut c, &[], true));
        assert!(color_op(&mut c, &[0.5], true));
        assert!(String::from_utf8(c).unwrap().contains(" K\n"));
    }
}
