//! PDF/A-1 has no transparency and no JPEG 2000. Pages that use either are
//! rendered through the [`Rasterizer`](crate::Rasterizer) and replaced by
//! an image plus an invisible text layer, so they look the same and stay
//! searchable. Other parts never take this path.

use std::collections::BTreeSet;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

use crate::content;
use crate::util::{self, deref, dict_mut};
use crate::{fonts, Ctx, PdfaError, Result};

pub fn is_standard_blend(n: &[u8]) -> bool {
    matches!(
        n,
        b"Normal" | b"Compatible" | b"Multiply" | b"Screen" | b"Overlay" | b"Darken" | b"Lighten" | b"ColorDodge"
            | b"ColorBurn" | b"HardLight" | b"SoftLight" | b"Difference" | b"Exclusion" | b"Hue" | b"Saturation"
            | b"Color" | b"Luminosity"
    )
}

/// Why a page cannot stay as it is in PDF/A-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Transparency,
    Jpx,
}

struct Scan<'a> {
    doc: &'a Document,
    seen: BTreeSet<ObjectId>,
    reason: Option<Reason>,
}

impl Scan<'_> {
    fn gs_transparent(&self, gs: &Dictionary) -> bool {
        let d = self.doc;
        if let Some(sm) = util::get(d, gs, b"SMask") {
            if !matches!(sm, Object::Name(n) if n == b"None") {
                return true;
            }
        }
        for k in [b"CA".as_slice(), b"ca"] {
            if let Some(v) = util::get_num(d, gs, k) {
                if v < 1.0 {
                    return true;
                }
            }
        }
        match util::get(d, gs, b"BM") {
            Some(Object::Name(n)) => !(n == b"Normal" || n == b"Compatible"),
            Some(Object::Array(a)) => a.iter().any(|x| !matches!(deref(d, x), Object::Name(n) if n == b"Normal" || n == b"Compatible")),
            _ => false,
        }
    }

    fn image_bad(&self, s: &Dictionary) -> Option<Reason> {
        if s.has(b"SMask") || util::get_num(self.doc, s, b"SMaskInData").unwrap_or(0.0) > 0.0 {
            return Some(Reason::Transparency);
        }
        let jpx = match util::get(self.doc, s, b"Filter") {
            Some(Object::Name(n)) => n == b"JPXDecode",
            Some(Object::Array(a)) => a.iter().any(|x| matches!(deref(self.doc, x), Object::Name(n) if n == b"JPXDecode")),
            _ => false,
        };
        if jpx {
            return Some(Reason::Jpx);
        }
        None
    }

    /// Scan a content stream with its resources for used transparency.
    fn content(&mut self, data: &[u8], res: Option<&Dictionary>) {
        if self.reason.is_some() {
            return;
        }
        let d = self.doc;
        let mut used_gs: Vec<Vec<u8>> = Vec::new();
        let mut used_xo: Vec<Vec<u8>> = Vec::new();
        let mut used_pat: Vec<Vec<u8>> = Vec::new();
        let mut used_font: Vec<Vec<u8>> = Vec::new();
        content::visit(data, |op| {
            let first_name = || op.args.first().and_then(|a| a.as_name()).map(|n| n.to_vec());
            match op.name {
                b"gs" => used_gs.extend(first_name()),
                b"Do" => used_xo.extend(first_name()),
                b"Tf" => used_font.extend(first_name()),
                b"scn" | b"SCN" => {
                    if let Some(n) = op.args.last().and_then(|a| a.as_name()) {
                        used_pat.push(n.to_vec());
                    }
                }
                _ => {}
            }
            true
        });
        let Some(res) = res else { return };
        let sub = |key: &[u8]| util::get_dict(d, res, key);
        if let Some(gsd) = sub(b"ExtGState") {
            for n in &used_gs {
                if let Some(g) = util::get_dict(d, gsd, n) {
                    if self.gs_transparent(g) {
                        self.reason = Some(Reason::Transparency);
                        return;
                    }
                    // Soft masks are forms too, but any SMask already counts.
                }
            }
        }
        if let Some(xo) = sub(b"XObject") {
            for n in &used_xo {
                let Ok(r) = xo.get(n) else { continue };
                if let Object::Reference(id) = r {
                    if !self.seen.insert(*id) {
                        continue;
                    }
                }
                if let Object::Stream(s) = deref(d, r) {
                    match util::get_name(d, &s.dict, b"Subtype") {
                        Some(b"Image") => {
                            if let Some(why) = self.image_bad(&s.dict) {
                                self.reason = Some(why);
                                return;
                            }
                        }
                        Some(b"Form") => {
                            // A transparency group alone changes nothing
                            // without transparent content (checked below).
                            if let Some(data) = util::stream_data(s) {
                                let r = util::get_dict(d, &s.dict, b"Resources");
                                self.content(&data, r.or(Some(res)));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(pd) = sub(b"Pattern") {
            for n in &used_pat {
                let Some(p) = pd.get(n).ok() else { continue };
                if let Object::Stream(s) = deref(d, p) {
                    if let Some(data) = util::stream_data(s) {
                        let r = util::get_dict(d, &s.dict, b"Resources");
                        self.content(&data, r);
                    }
                } else if let Some(pdict) = util::dict_of(d, p) {
                    if let Some(g) = util::get_dict(d, pdict, b"ExtGState") {
                        if self.gs_transparent(g) {
                            self.reason = Some(Reason::Transparency);
                            return;
                        }
                    }
                }
            }
        }
        if let Some(fd) = sub(b"Font") {
            for n in &used_font {
                let Some(f) = util::get_dict(d, fd, n) else { continue };
                if util::get_name(d, f, b"Subtype") != Some(b"Type3".as_slice()) {
                    continue;
                }
                let r = util::get_dict(d, f, b"Resources");
                if let Some(cp) = util::get_dict(d, f, b"CharProcs") {
                    for (_, v) in cp.iter() {
                        if let Object::Stream(s) = deref(d, v) {
                            if let Some(data) = util::stream_data(s) {
                                self.content(&data, r);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Why `page` has to be rendered for PDF/A-1, if it does.
pub fn page_reason(doc: &Document, page: ObjectId) -> Option<Reason> {
    let mut scan = Scan { doc, seen: BTreeSet::new(), reason: None };
    let res = util::inherited(doc, page, b"Resources").and_then(|o| util::dict_of(doc, &o).cloned());
    let data = util::page_content(doc, page);
    scan.content(&data, res.as_ref());
    if scan.reason.is_some() {
        return scan.reason;
    }
    // Annotations: constant opacity or transparent appearances.
    for a in util::page_annots(doc, page) {
        let Some(ad) = util::dict_of(doc, &a) else { continue };
        if util::get_num(doc, ad, b"CA").map(|v| v < 1.0).unwrap_or(false) || util::get_num(doc, ad, b"ca").map(|v| v < 1.0).unwrap_or(false) {
            return Some(Reason::Transparency);
        }
        if let Some(Object::Name(bm)) = util::get(doc, ad, b"BM") {
            if !(bm == b"Normal" || bm == b"Compatible") {
                return Some(Reason::Transparency);
            }
        }
        let Some(n) = util::get_dict(doc, ad, b"AP").and_then(|ap| ap.get(b"N").ok()) else { continue };
        let streams: Vec<&Object> = match deref(doc, n) {
            Object::Stream(_) => vec![n],
            Object::Dictionary(sd) => sd.iter().map(|(_, v)| v).collect(),
            _ => Vec::new(),
        };
        for s in streams {
            if let Object::Stream(st) = deref(doc, s) {
                if let Some(data) = util::stream_data(st) {
                    let r = util::get_dict(doc, &st.dict, b"Resources");
                    scan.content(&data, r);
                    if scan.reason.is_some() {
                        return scan.reason;
                    }
                }
            }
        }
    }
    None
}

pub fn run(ctx: &mut Ctx) -> Result<()> {
    if ctx.level.part() != 1 {
        return Ok(());
    }
    let pages = util::pages(&ctx.doc);
    let mut flattened = Vec::new();
    for (idx, page) in pages.iter().enumerate() {
        if let Some(reason) = page_reason(&ctx.doc, *page) {
            let Some(r) = ctx.opts.rasterizer.clone() else {
                let what = match reason {
                    Reason::Transparency => "transparency",
                    Reason::Jpx => "JPEG 2000 image",
                };
                return Err(PdfaError::NeedsRasterizer(format!("{what} on page {}", idx + 1)));
            };
            let raster = r
                .render_page(ctx.source, idx as u32, ctx.opts.flatten_dpi)
                .map_err(|e| PdfaError::Unsupported(format!("rendering page {} failed: {e}", idx + 1)))?;
            replace_page(ctx, *page, &raster)?;
            flattened.push(idx + 1);
        }
    }
    // Groups and constant opacity that no longer matter.
    let ids: Vec<ObjectId> = ctx.doc.objects.keys().copied().collect();
    for id in ids {
        let doc = &mut ctx.doc;
        let (has_group, is_annot) = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => (d.has(b"Group"), d.has(b"Rect") && d.has(b"Subtype")),
            Some(Object::Stream(s)) => (s.dict.has(b"Group"), false),
            _ => (false, false),
        };
        if let Some(d) = dict_mut(doc, id) {
            if has_group {
                let transparency = match d.get(b"Group") {
                    Ok(Object::Dictionary(g)) => g.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"Transparency".as_slice()),
                    Ok(Object::Reference(_)) => true,
                    _ => false,
                };
                if transparency {
                    d.remove(b"Group");
                }
            }
            if is_annot {
                d.remove(b"CA");
                d.remove(b"ca");
                d.remove(b"BM");
            }
        }
    }
    if !flattened.is_empty() {
        let list: Vec<String> = flattened.iter().map(|p| p.to_string()).collect();
        ctx.report.note("pages_rendered", list.join(", "));
    }
    Ok(())
}

/// Page box corners → image placement matrix for the displayed bitmap.
pub fn placement(bx: [f64; 4], rotate: i64) -> [f64; 6] {
    let [x0, y0, x1, y1] = bx;
    let (w, h) = (x1 - x0, y1 - y0);
    match rotate {
        90 => [0.0, h, -w, 0.0, x1, y0],
        180 => [-w, 0.0, 0.0, -h, x1, y1],
        270 => [0.0, -h, w, 0.0, x0, y1],
        _ => [w, 0.0, 0.0, h, x0, y0],
    }
}

pub(crate) fn replace_page(ctx: &mut Ctx, page: ObjectId, raster: &crate::RasterPage) -> Result<()> {
    let doc = &mut ctx.doc;
    let bx = util::page_box(doc, page, b"CropBox");
    let rot = util::page_rotation(doc, page);
    let m = placement(bx, rot);

    let img = encode_image(raster)?;
    let img_id = doc.add_object(Object::Stream(img));

    let mut c = Vec::new();
    let nums = |c: &mut Vec<u8>, v: &[f64]| {
        for x in v {
            content::write_real(c, *x);
            c.push(b' ');
        }
    };
    c.extend_from_slice(b"q ");
    nums(&mut c, &m);
    c.extend_from_slice(b"cm /Im0 Do Q\n");

    let mut res = Dictionary::new();
    let mut xo = Dictionary::new();
    xo.set("Im0", Object::Reference(img_id));
    res.set("XObject", Object::Dictionary(xo));

    // Invisible text layer.
    if !raster.words.is_empty() {
        let (font, layer) = fonts::text_layer(doc, raster, &m, ctx.level)?;
        let mut fd = Dictionary::new();
        fd.set("FT0", font);
        res.set("Font", Object::Dictionary(fd));
        c.extend_from_slice(&layer);
    }

    let cid = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &c)));
    // Keep only link annotations; everything else is in the bitmap.
    let kept: Vec<Object> = util::page_annots(doc, page)
        .into_iter()
        .filter(|a| util::dict_of(doc, a).and_then(|d| util::get_name(doc, d, b"Subtype")) == Some(b"Link".as_slice()))
        .collect();
    util::set_page_annots(doc, page, kept);
    if let Some(d) = dict_mut(doc, page) {
        d.set("Contents", Object::Reference(cid));
        d.set("Resources", Object::Dictionary(res));
        d.remove(b"Group");
        d.remove(b"Thumb");
    }
    Ok(())
}

/// Rendered page → JPEG image XObject (DeviceRGB).
fn encode_image(r: &crate::RasterPage) -> Result<lopdf::Stream> {
    if r.width == 0 || r.height == 0 || r.rgb.len() < (r.width * r.height * 3) as usize {
        return Err(PdfaError::Unsupported("empty page rendering".into()));
    }
    let mut jpg = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 90);
    enc.encode(&r.rgb, r.width, r.height, image::ExtendedColorType::Rgb8)
        .map_err(|e| PdfaError::Unsupported(format!("JPEG encoding failed: {e}")))?;
    let mut s = lopdf::Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => r.width as i64,
            "Height" => r.height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpg,
    );
    s.allows_compression = false;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_matrices() {
        let b = [0.0, 0.0, 100.0, 200.0];
        assert_eq!(placement(b, 0), [100.0, 0.0, 0.0, 200.0, 0.0, 0.0]);
        assert_eq!(placement(b, 90), [0.0, 200.0, -100.0, 0.0, 100.0, 0.0]);
        assert_eq!(placement(b, 180), [-100.0, 0.0, 0.0, -200.0, 100.0, 200.0]);
        assert_eq!(placement(b, 270), [0.0, -200.0, 100.0, 0.0, 0.0, 200.0]);
        assert!(is_standard_blend(b"Multiply") && !is_standard_blend(b"Foo"));
    }
}
