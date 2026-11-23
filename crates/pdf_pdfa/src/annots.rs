//! Annotations: permitted types, flags, appearance dictionaries.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::level::Level;
use crate::util::{self, deref, dict_mut, num};
use crate::{ap, Ctx, Result};

const PRINT: i64 = 4;
const INVISIBLE: i64 = 1;
const HIDDEN: i64 = 2;
const NOVIEW: i64 = 32;
const TOGGLE_NOVIEW: i64 = 256;

/// Whether `subtype` may appear at `level`.
pub fn type_allowed(level: Level, subtype: &[u8]) -> bool {
    const P1: [&[u8]; 16] = [
        b"Text", b"Link", b"FreeText", b"Line", b"Square", b"Circle", b"Highlight", b"Underline", b"Squiggly",
        b"StrikeOut", b"Stamp", b"Ink", b"Popup", b"Widget", b"PrinterMark", b"TrapNet",
    ];
    const P2: [&[u8]; 6] = [b"Polygon", b"PolyLine", b"Caret", b"FileAttachment", b"Watermark", b"Redact"];
    match level.part() {
        1 => P1.contains(&subtype),
        2 | 3 => P1.contains(&subtype) || P2.contains(&subtype),
        _ => {
            P1.contains(&subtype)
                || (P2.contains(&subtype) && (subtype != b"FileAttachment" || level.attachments() == crate::AttachmentPolicy::Any))
                || subtype == b"Projection"
                || (level.allows_3d() && (subtype == b"3D" || subtype == b"RichMedia"))
        }
    }
}

pub fn run(ctx: &mut Ctx) -> Result<()> {
    let level = ctx.level;
    let mut removed = 0usize;
    let mut flattened = 0usize;
    let mut generated = 0usize;
    for page in util::pages(&ctx.doc) {
        let annots = util::page_annots(&ctx.doc, page);
        if annots.is_empty() {
            if let Some(d) = dict_mut(&mut ctx.doc, page) {
                d.remove(b"Annots");
            }
            continue;
        }
        let mut keep: Vec<Object> = Vec::new();
        let mut popups_to_drop: Vec<ObjectId> = Vec::new();
        for a in annots {
            // Annotations must be addressable: make direct ones indirect.
            let id = match a {
                Object::Reference(id) if matches!(ctx.doc.objects.get(&id), Some(Object::Dictionary(_))) => id,
                Object::Dictionary(d) => ctx.doc.add_object(Object::Dictionary(d)),
                _ => continue,
            };
            let d = match ctx.doc.objects.get(&id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let subtype = util::get_name(&ctx.doc, &d, b"Subtype").map(|s| s.to_vec()).unwrap_or_default();
            let flags = util::get_num(&ctx.doc, &d, b"F").unwrap_or(0.0) as i64;

            // Hidden annotations are invisible today; making them printable
            // would change the document, so they go.
            if subtype != b"Popup" && flags & (HIDDEN | NOVIEW) != 0 {
                removed += 1;
                continue;
            }
            if subtype == b"Popup" {
                if level.part() == 1 {
                    // PDF/A-1 has no Popup exemption; viewers recreate
                    // pop-ups from the parent's /Contents.
                    popups_to_drop.push(id);
                    removed += 1;
                    continue;
                }
                if let Some(dd) = dict_mut(&mut ctx.doc, id) {
                    dd.remove(b"F");
                    dd.remove(b"AP");
                }
                keep.push(Object::Reference(id));
                continue;
            }
            if !type_allowed(level, &subtype) {
                // Keep what it looks like where possible.
                if flatten_into_page(&mut ctx.doc, page, id) {
                    flattened += 1;
                } else {
                    removed += 1;
                }
                continue;
            }

            let rect = rect_of(&ctx.doc, &d);
            let zero = rect.map(|r| r[0] == r[2] && r[1] == r[3]).unwrap_or(true);
            let is_widget = subtype == b"Widget";

            // Flags: print on, hidden/invisible/noview/togglenoview off.
            let new_flags = (flags | PRINT) & !(INVISIBLE | HIDDEN | NOVIEW | TOGGLE_NOVIEW);
            let mut dd = d.clone();
            dd.set("F", new_flags);
            if level.part() == 1 {
                // Flattening (transparency) runs later and needs CA; any
                // left afterwards is forced to 1 there.
            }
            if let Ok(Object::Name(bm)) = dd.get(b"BM") {
                if !crate::flatten::is_standard_blend(bm) {
                    dd.set("BM", Object::Name(b"Normal".to_vec()));
                }
            }

            // Appearance: only /N; N a stream (subdictionary for buttons).
            let is_btn = is_widget && field_type(&ctx.doc, &dd).as_deref() == Some(b"Btn".as_slice());
            let ap = dd.get(b"AP").ok().map(|o| deref(&ctx.doc, o).clone());
            let mut normal: Option<Object> = match &ap {
                Some(Object::Dictionary(apd)) => apd.get(b"N").ok().cloned(),
                _ => None,
            };
            normal = normal.and_then(|n| normalise_normal(&mut ctx.doc, &dd, n, is_btn));
            if normal.is_none() && !zero && subtype != b"Link" && !is_widget {
                if let Some(s) = ap::generate(&mut ctx.doc, &dd, rect.unwrap_or([0.0; 4])) {
                    normal = Some(Object::Reference(ctx.doc.add_object(Object::Stream(s))));
                    generated += 1;
                }
            }
            match normal {
                Some(n) => {
                    dd.set("AP", dictionary! { "N" => n });
                }
                None => {
                    dd.remove(b"AP");
                    if !zero && subtype != b"Link" && !is_widget {
                        // Unknown appearance and nothing to draw: an empty
                        // appearance keeps the annotation (and its comment)
                        // without inventing content.
                        let r = rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
                        let s = ap::empty_form(r);
                        let sid = ctx.doc.add_object(Object::Stream(s));
                        dd.set("AP", dictionary! { "N" => sid });
                        generated += 1;
                    }
                }
            }
            if !is_btn {
                // /AS only makes sense with a state subdictionary.
                if !matches!(dd.get(b"AP").ok().and_then(|a| a.as_dict().ok()).and_then(|a| a.get(b"N").ok()).map(|n| deref(&ctx.doc, n)), Some(Object::Dictionary(_))) {
                    dd.remove(b"AS");
                }
            }
            ctx.doc.objects.insert(id, Object::Dictionary(dd));
            keep.push(Object::Reference(id));
        }
        // Drop references to removed pop-ups.
        if !popups_to_drop.is_empty() {
            for k in &keep {
                if let Object::Reference(kid) = k {
                    if let Some(kd) = dict_mut(&mut ctx.doc, *kid) {
                        if let Ok(Object::Reference(p)) = kd.get(b"Popup") {
                            if popups_to_drop.contains(p) {
                                kd.remove(b"Popup");
                            }
                        }
                    }
                }
            }
        }
        // Remove dangling /Popup references to annotations no longer kept.
        let kept_ids: Vec<ObjectId> = keep.iter().filter_map(|o| o.as_reference().ok()).collect();
        for kid in &kept_ids {
            let dangling = match ctx.doc.objects.get(kid) {
                Some(Object::Dictionary(kd)) => matches!(kd.get(b"Popup"), Ok(Object::Reference(p)) if !kept_ids.contains(p)),
                _ => false,
            };
            if dangling {
                if let Some(kd) = dict_mut(&mut ctx.doc, *kid) {
                    kd.remove(b"Popup");
                }
            }
        }
        util::set_page_annots(&mut ctx.doc, page, keep);
    }
    if removed > 0 {
        ctx.report.note("annotations_removed", format!("{removed}"));
    }
    if flattened > 0 {
        ctx.report.note("annotations_flattened", format!("{flattened}"));
    }
    if generated > 0 {
        ctx.report.note("appearances_generated", format!("{generated}"));
    }
    Ok(())
}

pub fn rect_of(doc: &Document, d: &Dictionary) -> Option<[f64; 4]> {
    let a = util::get_array(doc, d, b"Rect")?;
    if a.len() != 4 {
        return None;
    }
    let v: Vec<f64> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
    if v.len() != 4 {
        return None;
    }
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

/// Field type of a widget (inherited through /Parent).
pub fn field_type(doc: &Document, d: &Dictionary) -> Option<Vec<u8>> {
    let mut cur = d.clone();
    for _ in 0..32 {
        if let Some(n) = util::get_name(doc, &cur, b"FT") {
            return Some(n.to_vec());
        }
        match cur.get(b"Parent").ok().and_then(|p| util::dict_of(doc, p)) {
            Some(p) => cur = p.clone(),
            None => break,
        }
    }
    None
}

/// Make /N valid: a form stream, or for buttons a subdictionary of form
/// streams. Returns `None` when there is nothing usable.
fn normalise_normal(doc: &mut Document, annot: &Dictionary, n: Object, is_btn: bool) -> Option<Object> {
    let rect = rect_of(doc, annot).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    match deref(doc, &n).clone() {
        Object::Stream(_) => {
            let id = n.as_reference().ok()?;
            fix_form(doc, id, rect);
            if is_btn {
                // Buttons need a state dictionary; keep the current state.
                let state = annot.get(b"AS").ok().and_then(|o| o.as_name().ok()).map(|s| s.to_vec()).unwrap_or_else(|| b"On".to_vec());
                let mut sd = Dictionary::new();
                sd.set(state, Object::Reference(id));
                return Some(Object::Dictionary(sd));
            }
            Some(Object::Reference(id))
        }
        Object::Dictionary(sd) => {
            let mut states = Dictionary::new();
            for (k, v) in sd.iter() {
                if let (Object::Reference(sid), Object::Stream(_)) = (v, deref(doc, v)) {
                    states.set(k.clone(), Object::Reference(*sid));
                }
            }
            for (_, v) in states.iter() {
                if let Object::Reference(sid) = v {
                    fix_form(doc, *sid, rect);
                }
            }
            if states.is_empty() {
                return None;
            }
            if is_btn {
                return Some(Object::Dictionary(states));
            }
            // Non-buttons: pick the current state's stream.
            let state = annot.get(b"AS").ok().and_then(|o| o.as_name().ok()).map(|s| s.to_vec());
            let chosen = state
                .and_then(|s| states.get(&s).ok().cloned())
                .or_else(|| states.iter().next().map(|(_, v)| v.clone()));
            chosen
        }
        _ => None,
    }
}

/// Appearance streams must be form XObjects with a BBox.
fn fix_form(doc: &mut Document, id: ObjectId, rect: [f64; 4]) {
    if let Some(Object::Stream(s)) = doc.objects.get_mut(&id) {
        if !s.dict.has(b"Subtype") {
            s.dict.set("Subtype", Object::Name(b"Form".to_vec()));
        }
        if !s.dict.has(b"BBox") {
            s.dict.set(
                "BBox",
                Object::Array(vec![0.into(), 0.into(), Object::Real((rect[2] - rect[0]) as f32), Object::Real((rect[3] - rect[1]) as f32)]),
            );
        }
        if !s.dict.has(b"Type") {
            s.dict.set("Type", Object::Name(b"XObject".to_vec()));
        }
    }
}

/// Paint the annotation's normal appearance into the page content (the
/// annotation itself is removed by the caller). Returns false when there is
/// no appearance to paint.
pub fn flatten_into_page(doc: &mut Document, page: ObjectId, annot: ObjectId) -> bool {
    let d = match doc.objects.get(&annot) {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => return false,
    };
    let flags = util::get_num(doc, &d, b"F").unwrap_or(0.0) as i64;
    if flags & (HIDDEN | NOVIEW) != 0 {
        return false;
    }
    let Some(rect) = rect_of(doc, &d) else { return false };
    let n = match util::get_dict(doc, &d, b"AP").and_then(|ap| ap.get(b"N").ok()).cloned() {
        Some(n) => n,
        None => return false,
    };
    let form_id = match deref(doc, &n) {
        Object::Stream(_) => n.as_reference().ok(),
        Object::Dictionary(sd) => {
            let st = d.get(b"AS").ok().and_then(|o| o.as_name().ok()).map(|s| s.to_vec());
            st.and_then(|s| sd.get(&s).ok().and_then(|o| o.as_reference().ok()))
        }
        _ => None,
    };
    let Some(form_id) = form_id else { return false };
    let (bbox, matrix) = match doc.objects.get(&form_id) {
        Some(Object::Stream(s)) => (
            util::get_array(doc, &s.dict, b"BBox").map(|a| a.iter().filter_map(|x| num(deref(doc, x))).collect::<Vec<f64>>()),
            util::get_array(doc, &s.dict, b"Matrix").map(|a| a.iter().filter_map(|x| num(deref(doc, x))).collect::<Vec<f64>>()),
        ),
        _ => return false,
    };
    let Some(bbox) = bbox.filter(|b| b.len() == 4) else { return false };
    let m = matrix.filter(|m| m.len() == 6).unwrap_or(vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    // Transformed bbox.
    let pts = [(bbox[0], bbox[1]), (bbox[2], bbox[1]), (bbox[0], bbox[3]), (bbox[2], bbox[3])];
    let tp: Vec<(f64, f64)> = pts.iter().map(|&(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])).collect();
    let (minx, maxx) = tp.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
    let (miny, maxy) = tp.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let (bw, bh) = (maxx - minx, maxy - miny);
    if bw <= 0.0 || bh <= 0.0 {
        return false;
    }
    let sx = (rect[2] - rect[0]) / bw;
    let sy = (rect[3] - rect[1]) / bh;
    let a = [sx, 0.0, 0.0, sy, rect[0] - minx * sx, rect[1] - miny * sy];
    let loc = util::ensure_page_resources(doc, page);
    let name = add_xobject(doc, loc, form_id);
    let mut c = Vec::new();
    use std::io::Write;
    let _ = writeln!(
        c,
        "q {} {} {} {} {} {} cm /{} Do Q",
        fmt(a[0]),
        fmt(a[1]),
        fmt(a[2]),
        fmt(a[3]),
        fmt(a[4]),
        fmt(a[5]),
        String::from_utf8_lossy(&name)
    );
    append_page_content(doc, page, &c);
    true
}

fn fmt(v: f64) -> String {
    let mut o = Vec::new();
    crate::content::write_real(&mut o, v);
    String::from_utf8(o).unwrap_or_default()
}

/// Register `xobj` in the resource dictionary at `loc`; returns its name.
pub fn add_xobject(doc: &mut Document, loc: util::ResLoc, xobj: ObjectId) -> Vec<u8> {
    let existing = util::res_ref(doc, loc).and_then(|r| r.get(b"XObject").ok()).cloned();
    let mut xd = match &existing {
        Some(Object::Dictionary(d)) => d.clone(),
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        },
        _ => Dictionary::new(),
    };
    let mut i = 0;
    let name = loop {
        let n = format!("FxA{i}").into_bytes();
        if !xd.has(&n) {
            break n;
        }
        i += 1;
    };
    xd.set(name.clone(), Object::Reference(xobj));
    match existing {
        Some(Object::Reference(id)) => {
            doc.objects.insert(id, Object::Dictionary(xd));
        }
        _ => {
            if let Some(r) = util::res_mut(doc, loc) {
                r.set("XObject", Object::Dictionary(xd));
            }
        }
    }
    name
}

/// Append a content stream to the page (wrapping existing content in q/Q).
pub fn append_page_content(doc: &mut Document, page: ObjectId, data: &[u8]) {
    let existing: Vec<Object> = match doc.objects.get(&page).and_then(|o| o.as_dict().ok()).and_then(|d| d.get(b"Contents").ok()) {
        Some(Object::Array(a)) => a.clone(),
        Some(o @ Object::Reference(_)) => match deref(doc, o) {
            Object::Array(a) => a.clone(),
            _ => vec![o.clone()],
        },
        _ => Vec::new(),
    };
    let pre = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"q\n".to_vec())));
    let mut post_data = b"\nQ\n".to_vec();
    post_data.extend_from_slice(data);
    let post = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &post_data)));
    let mut arr = vec![Object::Reference(pre)];
    arr.extend(existing);
    arr.push(Object::Reference(post));
    if let Some(d) = dict_mut(doc, page) {
        d.set("Contents", Object::Array(arr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_types_per_part() {
        let l1: Level = "1b".parse().unwrap();
        let l2: Level = "2b".parse().unwrap();
        let l4: Level = "4".parse().unwrap();
        let l4f: Level = "4f".parse().unwrap();
        let l4e: Level = "4e".parse().unwrap();
        assert!(type_allowed(l1, b"Square"));
        assert!(!type_allowed(l1, b"Polygon"));
        assert!(type_allowed(l2, b"Polygon"));
        assert!(type_allowed(l2, b"FileAttachment"));
        assert!(!type_allowed(l2, b"3D"));
        assert!(!type_allowed(l4, b"FileAttachment"));
        assert!(type_allowed(l4f, b"FileAttachment"));
        assert!(type_allowed(l4e, b"3D") && !type_allowed(l4, b"RichMedia"));
        assert!(type_allowed(l4, b"Projection") && !type_allowed(l2, b"Projection"));
        assert!(!type_allowed(l2, b"Sound") && !type_allowed(l2, b"Movie") && !type_allowed(l2, b"Screen"));
    }
}
