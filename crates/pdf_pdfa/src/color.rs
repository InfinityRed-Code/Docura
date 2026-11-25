//! Colour: OutputIntent, device-independent CMYK, ICC checks, ExtGState.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

use crate::content::{self, Operand};
use crate::icc;
use crate::util::{self, deref, dict_mut};
use crate::Ctx;

const INTENTS: [&[u8]; 4] = [b"RelativeColorimetric", b"AbsoluteColorimetric", b"Perceptual", b"Saturation"];

pub fn run(ctx: &mut Ctx) {
    let part = ctx.level.part();
    let doc = &mut ctx.doc;
    output_intent(doc);
    fix_icc_spaces(doc, part);
    fix_extgstates(doc, part);
    if uses_device_cmyk(doc) {
        let cmyk = add_cmyk_space(doc);
        default_cmyk_everywhere(doc, cmyk);
    }
}

/// Replace all OutputIntents with one PDF/A sRGB intent.
fn output_intent(doc: &mut Document) {
    let mut s = util::flate_stream(dictionary! { "N" => 3 }, &icc::srgb_profile());
    s.allows_compression = false;
    let prof = doc.add_object(Object::Stream(s));
    let oi = doc.add_object(dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::string_literal(icc::SRGB_IDENTIFIER),
        "RegistryName" => Object::string_literal("http://www.color.org"),
        "Info" => Object::string_literal(icc::SRGB_IDENTIFIER),
        "DestOutputProfile" => prof,
    });
    if let Some(c) = util::catalog_mut(doc) {
        c.set("OutputIntents", Object::Array(vec![Object::Reference(oi)]));
    }
    // Page-level output intents (PDF 2.0) would need the same profile.
    for p in util::pages(doc) {
        if let Some(d) = dict_mut(doc, p) {
            d.remove(b"OutputIntents");
        }
    }
}

/// The `[/ICCBased cmyk]` colour space, created once.
fn add_cmyk_space(doc: &mut Document) -> Object {
    let s = util::flate_stream(dictionary! { "N" => 4, "Alternate" => "DeviceCMYK" }, &icc::cmyk_profile());
    let id = doc.add_object(Object::Stream(s));
    let cs = doc.add_object(Object::Array(vec![Object::Name(b"ICCBased".to_vec()), Object::Reference(id)]));
    Object::Reference(cs)
}

/// Whether DeviceCMYK appears anywhere (operators, names, inline images).
pub fn uses_device_cmyk(doc: &Document) -> bool {
    fn has_name(o: &Object, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        match o {
            Object::Name(n) => n == b"DeviceCMYK",
            Object::Array(a) => a.iter().any(|x| has_name(x, depth + 1)),
            Object::Dictionary(d) => d.iter().any(|(_, v)| has_name(v, depth + 1)),
            Object::Stream(s) => s.dict.iter().any(|(_, v)| has_name(v, depth + 1)),
            _ => false,
        }
    }
    for obj in doc.objects.values() {
        if has_name(obj, 0) {
            return true;
        }
    }
    for data in content_streams(doc) {
        let mut found = false;
        content::visit(&data, |op| {
            match op.name {
                b"k" | b"K" => found = true,
                b"cs" | b"CS" => {
                    if op.args.first().and_then(|a| a.as_name()) == Some(b"DeviceCMYK") {
                        found = true;
                    }
                }
                b"BI" => {
                    if let Some(img) = op.inline {
                        let cs = img.dict.iter().find(|(k, _)| k == b"CS" || k == b"ColorSpace").map(|(_, v)| v);
                        if matches!(cs, Some(Operand::Name(n)) if n == b"CMYK" || n == b"DeviceCMYK") {
                            found = true;
                        }
                    }
                }
                _ => {}
            }
            !found
        });
        if found {
            return true;
        }
    }
    false
}

/// Decoded data of every content stream (pages, forms, patterns, glyphs).
pub fn content_streams(doc: &Document) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for p in util::pages(doc) {
        out.push(util::page_content(doc, p));
    }
    for obj in doc.objects.values() {
        if let Object::Stream(s) = obj {
            let ty = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            let is_form = ty == Some(b"Form".as_slice());
            let is_pattern = s.dict.get(b"PatternType").ok().and_then(|o| o.as_i64().ok()) == Some(1);
            if is_form || is_pattern {
                if let Some(d) = util::stream_data(s) {
                    out.push(d);
                }
            }
        }
    }
    // Type3 glyph procedures.
    for obj in doc.objects.values() {
        if let Object::Dictionary(d) = obj {
            if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type3".as_slice()) {
                if let Some(cp) = util::get_dict(doc, d, b"CharProcs") {
                    for (_, v) in cp.iter() {
                        if let Object::Stream(s) = deref(doc, v) {
                            if let Some(data) = util::stream_data(s) {
                                out.push(data);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Put `/DefaultCMYK` into every resource dictionary, and give every
/// content stream without resources one.
fn default_cmyk_everywhere(doc: &mut Document, cmyk: Object) {
    // Content streams lacking resources: pages, forms, tiling patterns, Type3.
    for p in util::pages(doc) {
        util::ensure_page_resources(doc, p);
    }
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs = match doc.objects.get(&id) {
            Some(Object::Stream(s)) => {
                let is_form = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Form".as_slice());
                let is_pattern = s.dict.get(b"PatternType").ok().and_then(|o| o.as_i64().ok()) == Some(1);
                (is_form || is_pattern) && !s.dict.has(b"Resources")
            }
            Some(Object::Dictionary(d)) => {
                d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type3".as_slice()) && !d.has(b"Resources")
            }
            _ => false,
        };
        if needs {
            if let Some(d) = dict_mut(doc, id) {
                d.set("Resources", Object::Dictionary(Dictionary::new()));
            }
        }
    }
    for loc in util::resource_locations(doc) {
        let cs = util::res_ref(doc, loc).and_then(|r| r.get(b"ColorSpace").ok()).cloned();
        match cs {
            Some(Object::Reference(id)) => {
                let shared = match doc.objects.get(&id) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => Dictionary::new(),
                };
                let mut d = shared;
                set_default(&mut d, &cmyk);
                // Keep sharing: write back into the shared object.
                doc.objects.insert(id, Object::Dictionary(d));
            }
            Some(Object::Dictionary(mut d)) => {
                set_default(&mut d, &cmyk);
                if let Some(r) = util::res_mut(doc, loc) {
                    r.set("ColorSpace", Object::Dictionary(d));
                }
            }
            _ => {
                let mut d = Dictionary::new();
                set_default(&mut d, &cmyk);
                if let Some(r) = util::res_mut(doc, loc) {
                    r.set("ColorSpace", Object::Dictionary(d));
                }
            }
        }
    }
    // Transparency group blending spaces.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let group_cs_cmyk = |d: &Dictionary| -> bool {
            matches!(d.get(b"Group").ok().map(|g| deref(doc, g)), Some(Object::Dictionary(g)) if matches!(g.get(b"CS").ok().map(|c| deref(doc, c)), Some(Object::Name(n)) if n == b"DeviceCMYK"))
        };
        let hit = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => group_cs_cmyk(d),
            Some(Object::Stream(s)) => group_cs_cmyk(&s.dict),
            _ => false,
        };
        if hit {
            let g = util::dict_mut(doc, id).and_then(|d| d.get(b"Group").ok().cloned());
            match g {
                Some(Object::Reference(gid)) => {
                    if let Some(gd) = dict_mut(doc, gid) {
                        gd.set("CS", cmyk.clone());
                    }
                }
                Some(Object::Dictionary(mut gd)) => {
                    gd.set("CS", cmyk.clone());
                    if let Some(d) = dict_mut(doc, id) {
                        d.set("Group", Object::Dictionary(gd));
                    }
                }
                _ => {}
            }
        }
    }
}

fn set_default(d: &mut Dictionary, cmyk: &Object) {
    d.set("DefaultCMYK", cmyk.clone());
}

/// ICCBased spaces must carry a valid profile whose component count
/// matches /N; broken ones fall back to the device space (covered by the
/// OutputIntent / DefaultCMYK).
fn fix_icc_spaces(doc: &mut Document, part: u8) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut bad: Vec<(ObjectId, u32)> = Vec::new();
    for id in &ids {
        let s = match doc.objects.get(id) {
            Some(Object::Stream(s)) if !s.dict.has(b"First") && !s.dict.has(b"Type") => s,
            _ => continue,
        };
        // ICC profile streams are referenced from [/ICCBased x]; recognise
        // them by /N and profile signature.
        let n = match s.dict.get(b"N").ok().and_then(|o| o.as_i64().ok()) {
            Some(n) => n as u32,
            None => continue,
        };
        let data = match util::stream_data(s) {
            Some(d) => d,
            None => {
                bad.push((*id, n));
                continue;
            }
        };
        let ok = match icc::parse_header(&data) {
            Some(h) => {
                let comps_ok = h.components() == Some(n);
                let version_ok = part > 1 || h.major_version < 4;
                comps_ok && version_ok
            }
            None => false,
        };
        if !ok {
            bad.push((*id, n));
        }
        if let Some(Object::Stream(s)) = doc.objects.get_mut(id) {
            s.dict.remove(b"Metadata");
        }
    }
    if bad.is_empty() {
        return;
    }
    // Replace [/ICCBased bad] arrays with the device space.
    for id in ids {
        let replace = |o: &mut Object| {
            fix_icc_refs(o, &bad);
        };
        if let Some(o) = doc.objects.get_mut(&id) {
            replace(o);
        }
    }
}

fn fix_icc_refs(o: &mut Object, bad: &[(ObjectId, u32)]) {
    match o {
        Object::Array(a) => {
            if a.len() == 2 {
                if let (Object::Name(n), Object::Reference(r)) = (&a[0], &a[1]) {
                    if n == b"ICCBased" {
                        if let Some((_, comps)) = bad.iter().find(|(id, _)| id == r) {
                            let dev: &[u8] = match comps {
                                1 => b"DeviceGray",
                                4 => b"DeviceCMYK",
                                _ => b"DeviceRGB",
                            };
                            *o = Object::Name(dev.to_vec());
                            return;
                        }
                    }
                }
            }
            for x in a.iter_mut() {
                fix_icc_refs(x, bad);
            }
        }
        Object::Dictionary(d) => {
            for (_, v) in d.iter_mut() {
                fix_icc_refs(v, bad);
            }
        }
        Object::Stream(s) => {
            for (_, v) in s.dict.iter_mut() {
                fix_icc_refs(v, bad);
            }
        }
        _ => {}
    }
}

/// Graphics state parameter dictionaries: transfer functions, halftones,
/// rendering intents, overprint mode.
fn fix_extgstates(doc: &mut Document, part: u8) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let is_gs = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => is_extgstate(d),
            _ => false,
        };
        if is_gs {
            if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&id) {
                fix_gs(d, part);
            }
        }
    }
    // Direct ExtGState dictionaries inside resources.
    for loc in util::resource_locations(doc) {
        let gs = util::res_ref(doc, loc).and_then(|r| r.get(b"ExtGState").ok()).cloned();
        let mut gsd = match gs {
            Some(Object::Dictionary(d)) => d,
            _ => continue,
        };
        let mut changed = false;
        for (_, v) in gsd.iter_mut() {
            if let Object::Dictionary(d) = v {
                fix_gs(d, part);
                changed = true;
            }
        }
        if changed {
            if let Some(r) = util::res_mut(doc, loc) {
                r.set("ExtGState", Object::Dictionary(gsd));
            }
        }
    }
    // Rendering intents on images.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(Object::Stream(s)) = doc.objects.get_mut(&id) {
            if let Ok(Object::Name(n)) = s.dict.get(b"Intent") {
                if !INTENTS.contains(&n.as_slice()) {
                    s.dict.set("Intent", Object::Name(b"RelativeColorimetric".to_vec()));
                }
            }
        }
    }
}

fn is_extgstate(d: &Dictionary) -> bool {
    if d.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"ExtGState".as_slice()) {
        return true;
    }
    const KEYS: [&[u8]; 12] = [b"LW", b"LC", b"LJ", b"ML", b"D", b"RI", b"OP", b"op", b"OPM", b"CA", b"ca", b"SMask"];
    !d.has(b"Subtype") && !d.has(b"Type") && KEYS.iter().any(|k| d.has(k)) && !d.has(b"Rect")
}

fn fix_gs(d: &mut Dictionary, part: u8) {
    d.remove(b"TR");
    if let Ok(tr2) = d.get(b"TR2") {
        if !matches!(tr2, Object::Name(n) if n == b"Default") {
            d.remove(b"TR2");
        }
    }
    d.remove(b"HTP");
    if part == 4 {
        d.remove(b"HTO");
    }
    let ht_bad = match d.get(b"HT") {
        Ok(Object::Dictionary(h)) => halftone_bad(h),
        Ok(Object::Stream(s)) => halftone_bad(&s.dict),
        Ok(Object::Name(n)) => n != b"Default",
        Ok(Object::Reference(_)) => true,
        _ => false,
    };
    if ht_bad {
        d.remove(b"HT");
    }
    if let Ok(Object::Name(n)) = d.get(b"RI") {
        if !INTENTS.contains(&n.as_slice()) {
            d.set("RI", Object::Name(b"RelativeColorimetric".to_vec()));
        }
    }
    let op = matches!(d.get(b"OP"), Ok(Object::Boolean(true))) || matches!(d.get(b"op"), Ok(Object::Boolean(true)));
    if op && d.get(b"OPM").ok().and_then(|o| o.as_i64().ok()) == Some(1) {
        d.set("OPM", 0);
    }
}

fn halftone_bad(h: &Dictionary) -> bool {
    let t = h.get(b"HalftoneType").ok().and_then(|o| o.as_i64().ok());
    !matches!(t, Some(1) | Some(5)) || h.has(b"HalftoneName") || h.has(b"TransferFunction")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gs_fixes() {
        let mut d = dictionary! {
            "Type" => "ExtGState", "TR" => "Identity", "TR2" => "Identity", "HTP" => 1,
            "HTO" => 1, "RI" => "Bogus", "OP" => true, "OPM" => 1,
            "HT" => dictionary!{ "HalftoneType" => 6 },
        };
        assert!(is_extgstate(&d));
        fix_gs(&mut d, 4);
        for k in [b"TR".as_slice(), b"TR2", b"HTP", b"HTO", b"HT"] {
            assert!(!d.has(k), "{}", String::from_utf8_lossy(k));
        }
        assert_eq!(d.get(b"RI").unwrap().as_name().unwrap(), b"RelativeColorimetric");
        assert_eq!(d.get(b"OPM").unwrap().as_i64().unwrap(), 0);
        let mut keep = dictionary! { "TR2" => "Default", "HT" => "Default" };
        fix_gs(&mut keep, 2);
        assert!(keep.has(b"TR2") && keep.has(b"HT"));
        assert!(!is_extgstate(&dictionary! { "Rect" => 1, "CA" => 1 }));
        assert!(halftone_bad(&dictionary! { "HalftoneType" => 1, "HalftoneName" => "x" }));
        assert!(!halftone_bad(&dictionary! { "HalftoneType" => 5 }));
    }

    #[test]
    fn cmyk_detection_in_content() {
        let doc_with = |content: &[u8], pattern: bool| -> Document {
            let mut doc = Document::with_version("1.7");
            let pages = doc.new_object_id();
            let mut dict = lopdf::Dictionary::new();
            if pattern {
                dict.set("PatternType", 1);
            }
            let c = doc.add_object(lopdf::Stream::new(dict, content.to_vec()));
            let page = if pattern {
                doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "Resources" => dictionary! { "Pattern" => dictionary! { "P" => c } } })
            } else {
                doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "Contents" => c })
            };
            doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
            let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
            doc.trailer.set("Root", cat);
            doc
        };
        assert!(uses_device_cmyk(&doc_with(b"0 0 0 1 k 0 0 1 1 re f", false)));
        assert!(uses_device_cmyk(&doc_with(b"/DeviceCMYK cs 0 0 0 1 sc", false)));
        assert!(uses_device_cmyk(&doc_with(b"BI /W 1 /H 1 /BPC 8 /CS /CMYK ID \x00\x00\x00\x00 EI", false)));
        assert!(uses_device_cmyk(&doc_with(b"BI /W 1 /H 1 /BPC 8 /ColorSpace /DeviceCMYK ID \x00\x00\x00\x00 EI", false)));
        assert!(uses_device_cmyk(&doc_with(b"0 1 0 0 K 0 0 m 1 1 l S", true)));
        assert!(!uses_device_cmyk(&doc_with(b"/DeviceRGB cs 0 0 1 sc BI /W 1 /H 1 /BPC 8 /CS /G ID \x00 EI 1 0 0 rg", false)));
        assert!(!uses_device_cmyk(&doc_with(b"1 0 0 rg", true)));
    }

    #[test]
    fn icc_ref_replacement() {
        let mut o = Object::Array(vec![
            Object::Name(b"Indexed".to_vec()),
            Object::Array(vec![Object::Name(b"ICCBased".to_vec()), Object::Reference((7, 0))]),
        ]);
        fix_icc_refs(&mut o, &[((7, 0), 4)]);
        match o {
            Object::Array(a) => assert_eq!(a[1], Object::Name(b"DeviceCMYK".to_vec())),
            _ => unreachable!(),
        }
    }
}
