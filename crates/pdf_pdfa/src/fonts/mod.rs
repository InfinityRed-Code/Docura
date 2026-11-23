//! Fonts: embedding, glyph and width consistency, Unicode mapping.
//!
//! Every font that is used for rendering is checked the way PDF/A
//! validators do (glyph present for each used code, width in the font
//! dictionary = width in the program, no .notdef, TrueType encoding rules).
//! Fonts that fail, and fonts that are not embedded at all, are re-embedded
//! as composite fonts (see [`embed`]) from the embedded program itself or a
//! matching system font. Fonts that cannot be fixed that way leave their
//! pages to the rasterizer fallback.

mod embed;
mod encoding;
mod sfnt;
pub mod tables;
mod type1;
mod usage;
mod sysfonts;

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use crate::cmap::{self, CMap, Code};
use crate::level::Level;
use crate::util::{self, deref, dict_mut, num};
use crate::{Ctx, PdfaError, RasterPage, Result};

pub use usage::FontUse;

/// Unicode replacement for codes whose meaning is unknown (valid for every
/// PDF/A level, unlike 0 / FEFF / FFFE).
const UNKNOWN: &str = "\u{FFFD}";

enum Outcome {
    Kept,
    Fixed,
    Reembedded,
    Unfixable(String),
}

pub fn run(ctx: &mut Ctx) -> Result<()> {
    field_appearances(ctx)?;
    let uses = usage::collect(&ctx.doc);
    let mut reembedded = Vec::new();
    let mut unfixable: Vec<(ObjectId, String)> = Vec::new();
    for (fid, u) in &uses {
        let name = font_name(&ctx.doc, *fid);
        match fix_font(ctx, *fid, u)? {
            Outcome::Kept | Outcome::Fixed => {}
            Outcome::Reembedded => reembedded.push(name),
            Outcome::Unfixable(why) => unfixable.push((*fid, format!("{name}: {why}"))),
        }
    }
    cleanup_descriptors(ctx, &uses);
    if !reembedded.is_empty() {
        reembedded.sort();
        reembedded.dedup();
        ctx.report.note("fonts_embedded", reembedded.join(", "));
    }
    if !unfixable.is_empty() {
        render_pages_using(ctx, &unfixable)?;
    }
    Ok(())
}

fn font_name(doc: &Document, fid: ObjectId) -> String {
    doc.objects
        .get(&fid)
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| util::get_name(doc, d, b"BaseFont"))
        .map(|n| encoding::strip_subset(&String::from_utf8_lossy(n)).to_string())
        .unwrap_or_else(|| "Type3".into())
}

fn fix_font(ctx: &mut Ctx, fid: ObjectId, u: &FontUse) -> Result<Outcome> {
    let d = match ctx.doc.objects.get(&fid) {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => return Ok(Outcome::Kept),
    };
    match util::get_name(&ctx.doc, &d, b"Subtype") {
        Some(b"Type3") => fix_type3(ctx, fid, &d, u),
        Some(b"Type0") => fix_type0(ctx, fid, &d, u),
        Some(b"Type1") | Some(b"MMType1") | Some(b"TrueType") => fix_simple(ctx, fid, &d, u),
        _ => Ok(Outcome::Kept),
    }
}

/// The embedded font program of a descriptor.
#[derive(Clone)]
enum Prog {
    TrueType(Vec<u8>),
    Type1(Vec<u8>, Option<usize>),
    Cff(Vec<u8>),
    OpenType(Vec<u8>),
}

fn program(doc: &Document, desc: Option<&Dictionary>) -> Option<Prog> {
    let desc = desc?;
    if let Some(Object::Stream(s)) = desc.get(b"FontFile2").ok().map(|o| deref(doc, o)) {
        return util::stream_data(s).map(Prog::TrueType);
    }
    if let Some(Object::Stream(s)) = desc.get(b"FontFile").ok().map(|o| deref(doc, o)) {
        let l1 = util::get_num(doc, &s.dict, b"Length1").map(|v| v as usize);
        return util::stream_data(s).map(|d| Prog::Type1(d, l1));
    }
    if let Some(Object::Stream(s)) = desc.get(b"FontFile3").ok().map(|o| deref(doc, o)) {
        let sub = util::get_name(doc, &s.dict, b"Subtype").map(|n| n.to_vec());
        let data = util::stream_data(s)?;
        return Some(match sub.as_deref() {
            Some(b"OpenType") => Prog::OpenType(data),
            _ => Prog::Cff(data),
        });
    }
    None
}

fn descriptor(doc: &Document, d: &Dictionary) -> Option<Dictionary> {
    util::get_dict(doc, d, b"FontDescriptor").cloned()
}

fn flags(doc: &Document, desc: Option<&Dictionary>) -> i64 {
    desc.and_then(|d| util::get_num(doc, d, b"Flags")).unwrap_or(32.0) as i64
}

/// Existing ToUnicode map of a font.
fn existing_tounicode(doc: &Document, d: &Dictionary) -> BTreeMap<Code, String> {
    match d.get(b"ToUnicode").ok().map(|o| deref(doc, o)) {
        Some(Object::Stream(s)) => util::stream_data(s).map(|x| CMap::parse(&x).bf).unwrap_or_default(),
        _ => BTreeMap::new(),
    }
}

fn has_pua(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD))
}

fn valid_unicode(s: &str) -> bool {
    !s.is_empty() && !s.contains('\0') && !s.contains('\u{FEFF}') && !s.contains('\u{FFFE}')
}

// ---------------------------------------------------------------- simple

/// Width the dictionary declares for `code` (1/1000 em).
fn simple_dict_width(doc: &Document, d: &Dictionary, desc: Option<&Dictionary>, code: u32) -> Option<f64> {
    let first = util::get_num(doc, d, b"FirstChar");
    let widths = util::get_array(doc, d, b"Widths");
    if let (Some(f), Some(w)) = (first, widths) {
        let i = code as i64 - f as i64;
        if i >= 0 && (i as usize) < w.len() {
            return num(deref(doc, &w[i as usize]));
        }
        return desc.and_then(|x| util::get_num(doc, x, b"MissingWidth")).or(Some(0.0));
    }
    None
}

/// TrueType glyph selection (ISO 32000-1 9.6.6.4).
fn truetype_gid(face: &ttf_parser::Face, code: u8, name: Option<&str>, symbolic: bool) -> u16 {
    let Some(cmap) = face.tables().cmap else { return 0 };
    let lookup = |pid: ttf_parser::PlatformId, eid: u16, c: u32| -> Option<u16> {
        cmap.subtables
            .into_iter()
            .find(|s| s.platform_id == pid && s.encoding_id == eid)
            .and_then(|s| s.glyph_index(c))
            .map(|g| g.0)
            .filter(|g| *g != 0)
    };
    use ttf_parser::PlatformId::{Macintosh, Windows};
    if !symbolic {
        if let Some(n) = name {
            if let Some(u) = encoding::glyph_unicode(n).and_then(|s| s.chars().next()) {
                if let Some(g) = lookup(Windows, 1, u as u32) {
                    return g;
                }
            }
            if let Some(mc) = encoding::mac_roman_code(n) {
                if let Some(g) = lookup(Macintosh, 0, mc as u32) {
                    return g;
                }
            }
            if let Some(g) = face.glyph_index_by_name(n).map(|g| g.0).filter(|g| *g != 0) {
                return g;
            }
        }
    }
    for base in [0u32, 0xF000, 0xF100, 0xF200] {
        if let Some(g) = lookup(Windows, 0, base + code as u32) {
            return g;
        }
    }
    if let Some(g) = lookup(Macintosh, 0, code as u32) {
        return g;
    }
    if symbolic {
        if let Some(g) = lookup(Windows, 1, code as u32) {
            return g;
        }
    }
    0
}

/// TrueType encoding rules of PDF/A (6.2.11.6 / 6.3.7).
fn truetype_rules_ok(doc: &Document, d: &Dictionary, face: &ttf_parser::Face, symbolic: bool, part: u8) -> bool {
    let Some(cmap) = face.tables().cmap else { return false };
    let subs: Vec<(ttf_parser::PlatformId, u16)> = cmap.subtables.into_iter().map(|s| (s.platform_id, s.encoding_id)).collect();
    let has30 = subs.iter().any(|s| s.0 == ttf_parser::PlatformId::Windows && s.1 == 0);
    let has31 = subs.iter().any(|s| s.0 == ttf_parser::PlatformId::Windows && s.1 == 1);
    let enc = d.get(b"Encoding").ok().map(|o| deref(doc, o));
    if symbolic {
        if enc.is_some() {
            return false;
        }
        return if part == 1 { subs.len() == 1 } else { subs.len() == 1 || has30 };
    }
    // Non-symbolic.
    let base_ok = |n: &[u8]| n == b"WinAnsiEncoding" || n == b"MacRomanEncoding";
    let enc_ok = match enc {
        Some(Object::Name(n)) => base_ok(n),
        Some(Object::Dictionary(e)) => {
            let b = util::get_name(doc, e, b"BaseEncoding").map(base_ok).unwrap_or(false);
            let diffs_ok = match util::get_array(doc, e, b"Differences") {
                None => true,
                Some(a) => has31 && a.iter().all(|x| match deref(doc, x) {
                    Object::Name(n) => encoding::glyph_unicode(&String::from_utf8_lossy(n)).is_some(),
                    _ => true,
                }),
            };
            b && diffs_ok
        }
        _ => false,
    };
    let cmap_ok = if has30 { subs.len() > 1 } else { !subs.is_empty() };
    enc_ok && cmap_ok
}

struct SimpleCheck {
    missing: Vec<u32>,
    widths: Vec<(u32, f64)>,
    notdef: Vec<u32>,
    rules: bool,
}

fn fix_simple(ctx: &mut Ctx, fid: ObjectId, d: &Dictionary, u: &FontUse) -> Result<Outcome> {
    let level = ctx.level;
    let doc = &ctx.doc;
    let desc = descriptor(doc, d);
    let fl = flags(doc, desc.as_ref());
    let symbolic = fl & 4 != 0;
    let base = util::get_name(doc, d, b"BaseFont").map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
    let std = encoding::std14_name(&base);
    let prog = program(doc, desc.as_ref());

    // Program's own encoding (Type 1 / CFF).
    let t1 = match &prog {
        Some(Prog::Type1(data, l1)) => type1::parse(data, *l1),
        _ => None,
    };
    let cff_builtin: Option<[Option<String>; 256]> = match &prog {
        Some(Prog::Cff(data)) => ttf_parser::cff::Table::parse(data).map(|t| {
            std::array::from_fn(|c| t.glyph_index(c as u8).and_then(|g| t.glyph_name(g)).map(|s| s.to_string()).filter(|s| s != ".notdef"))
        }),
        _ => None,
    };
    let builtin = t1.as_ref().and_then(|t| t.encoding.clone()).or(cff_builtin);
    let names = encoding::simple_encoding(doc, d, builtin.as_ref());
    let tu_existing = existing_tounicode(doc, d);
    let unicode = |code: u32| -> Option<String> {
        if let Some(s) = tu_existing.get(&(code, 1)).filter(|s| valid_unicode(s)) {
            return Some(s.clone());
        }
        let n = names[code as usize & 0xFF].as_deref()?;
        if std == Some("ZapfDingbats") {
            if let Some(s) = encoding::dingbat_unicode(n) {
                return Some(s);
            }
        }
        encoding::glyph_unicode(n)
    };

    let Some(prog) = prog else {
        if u.rendered.is_empty() {
            if level.needs_unicode() {
                ensure_tounicode(&mut ctx.doc, fid, &u.codes, &unicode, &CMap::one_byte().codespace, level.needs_structure());
            }
            return Ok(Outcome::Kept);
        }
        let want = sysfonts::Want {
            name: base.clone(),
            bold: fl & (1 << 18) != 0 || base.to_ascii_lowercase().contains("bold"),
            italic: fl & 64 != 0 || base.to_ascii_lowercase().contains("italic") || base.to_ascii_lowercase().contains("oblique"),
            serif: fl & 2 != 0,
            fixed: fl & 1 != 0,
        };
        let src = sysfonts::find(&ctx.opts.font_dirs, &want, std, false).unwrap_or_else(|| sysfonts::bundled(&want));
        return reembed_simple(ctx, fid, d, desc.as_ref(), u, &src, &names, &unicode, std, None);
    };

    // Embedded: check like a validator.
    let mut chk = SimpleCheck { missing: Vec::new(), widths: Vec::new(), notdef: Vec::new(), rules: true };
    match &prog {
        Prog::TrueType(data) => {
            let Ok(face) = ttf_parser::Face::parse(data, 0) else {
                return reembed_or_render(ctx, fid, d, desc.as_ref(), u, &names, &unicode, std, &base, fl);
            };
            let upem = face.units_per_em() as f64;
            chk.rules = truetype_rules_ok(doc, d, &face, symbolic, level.part());
            for &(code, _) in &u.codes {
                let gid = truetype_gid(&face, code as u8, names[code as usize & 0xFF].as_deref(), symbolic);
                if gid == 0 || gid >= face.number_of_glyphs() {
                    chk.notdef.push(code);
                    if u.rendered.contains(&(code, 1)) {
                        chk.missing.push(code);
                    }
                    continue;
                }
                if u.rendered.contains(&(code, 1)) {
                    let pw = face.glyph_hor_advance(ttf_parser::GlyphId(gid)).unwrap_or(0) as f64 * 1000.0 / upem;
                    if let Some(dw) = simple_dict_width(doc, d, desc.as_ref(), code) {
                        if (pw - dw).abs() > 1.0 {
                            chk.widths.push((code, pw));
                        }
                    }
                }
            }
            let ok = chk.rules && chk.missing.is_empty() && chk.widths.is_empty() && (level.part() == 1 || chk.notdef.is_empty());
            if ok {
                if level.needs_unicode() {
                    ensure_tounicode(&mut ctx.doc, fid, &u.codes, &unicode, &CMap::one_byte().codespace, level.needs_structure());
                }
                return Ok(Outcome::Kept);
            }
            // Re-embed from the program itself: identical glyphs.
            let src = sysfonts::FontData { data: std::sync::Arc::new(data.clone()), index: 0, ps_name: encoding::strip_subset(&base).to_string() };
            let gid_for = |code: u32| -> Option<u16> {
                let g = truetype_gid(&face, code as u8, names[code as usize & 0xFF].as_deref(), symbolic);
                (g != 0).then_some(g)
            };
            return reembed_simple(ctx, fid, d, desc.as_ref(), u, &src, &names, &unicode, std, Some(&gid_for)).map(|o| match o {
                Outcome::Reembedded => Outcome::Fixed,
                other => other,
            });
        }
        Prog::Type1(..) | Prog::Cff(_) | Prog::OpenType(_) => {
            // Glyph names and widths from the program.
            let (present, width): (Box<dyn Fn(&str) -> bool>, Box<dyn Fn(&str) -> Option<f64>>) = match &prog {
                Prog::Type1(..) => {
                    let Some(t) = t1.as_ref() else {
                        return reembed_or_render(ctx, fid, d, desc.as_ref(), u, &names, &unicode, std, &base, fl);
                    };
                    let w = t.widths.clone();
                    let w2 = t.widths.clone();
                    let sc = t.scale;
                    (Box::new(move |n: &str| w.contains_key(n)), Box::new(move |n: &str| w2.get(n).map(|v| v * sc)))
                }
                Prog::Cff(data) | Prog::OpenType(data) => {
                    let cff_data: Vec<u8> = match &prog {
                        Prog::OpenType(_) => ttf_parser::Face::parse(data, 0)
                            .ok()
                            .and_then(|f| f.raw_face().table(ttf_parser::Tag::from_bytes(b"CFF ")).map(|t| t.to_vec()))
                            .unwrap_or_default(),
                        _ => data.clone(),
                    };
                    let Some(t) = ttf_parser::cff::Table::parse(&cff_data) else {
                        return reembed_or_render(ctx, fid, d, desc.as_ref(), u, &names, &unicode, std, &base, fl);
                    };
                    let scale = { let m = t.matrix(); if m.sx != 0.0 { m.sx as f64 / 0.001 } else { 1.0 } };
                    let mut map: BTreeMap<String, Option<f64>> = BTreeMap::new();
                    for g in 0..t.number_of_glyphs() {
                        if let Some(n) = t.glyph_name(ttf_parser::GlyphId(g)) {
                            map.insert(n.to_string(), t.glyph_width(ttf_parser::GlyphId(g)).map(|w| w as f64 * scale));
                        }
                    }
                    let m2 = map.clone();
                    (Box::new(move |n: &str| map.contains_key(n)), Box::new(move |n: &str| m2.get(n).copied().flatten()))
                }
                Prog::TrueType(_) => unreachable!(),
            };
            let mut fix_widths: Vec<(u32, f64)> = Vec::new();
            for &(code, _) in &u.codes {
                let n = names[code as usize & 0xFF].as_deref();
                let ok = n.map(|n| n != ".notdef" && present(n)).unwrap_or(false);
                if !ok {
                    chk.notdef.push(code);
                    if u.rendered.contains(&(code, 1)) {
                        chk.missing.push(code);
                    }
                    continue;
                }
                if u.rendered.contains(&(code, 1)) {
                    if let (Some(pw), Some(dw)) = (n.and_then(&width), simple_dict_width(doc, d, desc.as_ref(), code)) {
                        if (pw - dw).abs() > 1.0 {
                            fix_widths.push((code, pw));
                        }
                    }
                }
            }
            if !chk.missing.is_empty() || (level.part() > 1 && !chk.notdef.is_empty()) {
                return reembed_or_render(ctx, fid, d, desc.as_ref(), u, &names, &unicode, std, &base, fl);
            }
            let mut outcome = Outcome::Kept;
            if !fix_widths.is_empty() {
                set_simple_widths(&mut ctx.doc, fid, &fix_widths);
                ctx.report.note("font_widths_adjusted", font_name(&ctx.doc, fid));
                outcome = Outcome::Fixed;
            }
            if level.needs_unicode() {
                ensure_tounicode(&mut ctx.doc, fid, &u.codes, &unicode, &CMap::one_byte().codespace, level.needs_structure());
            }
            Ok(outcome)
        }
    }
}

/// Set /Widths entries for `codes` (extending FirstChar/LastChar).
fn set_simple_widths(doc: &mut Document, fid: ObjectId, fixes: &[(u32, f64)]) {
    let Some(d) = doc.objects.get(&fid).and_then(|o| o.as_dict().ok()).cloned() else { return };
    let first = util::get_num(doc, &d, b"FirstChar").unwrap_or(0.0) as i64;
    let mut widths: Vec<Object> = util::get_array(doc, &d, b"Widths").cloned().unwrap_or_default();
    let mut first = first;
    for &(code, w) in fixes {
        let c = code as i64;
        if c < first {
            let mut pre = vec![Object::Integer(0); (first - c) as usize];
            pre.extend(widths);
            widths = pre;
            first = c;
        }
        let i = (c - first) as usize;
        if i >= widths.len() {
            widths.resize(i + 1, Object::Integer(0));
        }
        widths[i] = Object::Real(w as f32);
    }
    if let Some(dd) = dict_mut(doc, fid) {
        dd.set("FirstChar", first);
        dd.set("LastChar", first + widths.len() as i64 - 1);
        dd.set("Widths", Object::Array(widths));
    }
}

/// Embedded Type 1 / CFF program cannot be repaired: try the same font
/// from the system, else leave the pages to the rasterizer.
#[allow(clippy::too_many_arguments)]
fn reembed_or_render(
    ctx: &mut Ctx,
    fid: ObjectId,
    d: &Dictionary,
    desc: Option<&Dictionary>,
    u: &FontUse,
    names: &[Option<String>; 256],
    unicode: &dyn Fn(u32) -> Option<String>,
    std: Option<&str>,
    base: &str,
    fl: i64,
) -> Result<Outcome> {
    let want = sysfonts::Want {
        name: base.to_string(),
        bold: fl & (1 << 18) != 0,
        italic: fl & 64 != 0,
        serif: fl & 2 != 0,
        fixed: fl & 1 != 0,
    };
    match sysfonts::find(&ctx.opts.font_dirs, &want, std, true) {
        Some(src) => reembed_simple(ctx, fid, d, desc, u, &src, names, unicode, std, None),
        None => Ok(Outcome::Unfixable("glyphs missing in the embedded font".into())),
    }
}

/// Re-embed a simple font as Type0 with the original one-byte codes.
#[allow(clippy::too_many_arguments)]
fn reembed_simple(
    ctx: &mut Ctx,
    fid: ObjectId,
    d: &Dictionary,
    desc: Option<&Dictionary>,
    u: &FontUse,
    src: &sysfonts::FontData,
    names: &[Option<String>; 256],
    unicode: &dyn Fn(u32) -> Option<String>,
    std: Option<&str>,
    gid_override: Option<&dyn Fn(u32) -> Option<u16>>,
) -> Result<Outcome> {
    let level = ctx.level;
    let face = match ttf_parser::Face::parse(&src.data, src.index) {
        Ok(f) => f,
        Err(_) => return Ok(Outcome::Unfixable("substitute font unreadable".into())),
    };
    let upem = face.units_per_em() as f64;
    let sym_cmap = face
        .tables()
        .cmap
        .map(|c| c.subtables.into_iter().any(|s| s.platform_id == ttf_parser::PlatformId::Windows && s.encoding_id == 0))
        .unwrap_or(false);
    let mut glyphs = Vec::new();
    let mut map = BTreeMap::new();
    let mut tu = BTreeMap::new();
    for &(code, _) in &u.codes {
        let name = names[code as usize & 0xFF].as_deref();
        let uni = unicode(code);
        let gid = match gid_override {
            Some(f) => f(code),
            None => {
                let mut g = None;
                if sym_cmap {
                    let t = truetype_gid(&face, code as u8, name, true);
                    if t != 0 {
                        g = Some(t);
                    }
                }
                if g.is_none() {
                    if let Some(c) = uni.as_deref().and_then(|s| s.chars().next()) {
                        g = face.glyph_index(c).map(|x| x.0).filter(|x| *x != 0);
                    }
                }
                if g.is_none() {
                    if let Some(n) = name {
                        g = face.glyph_index_by_name(n).map(|x| x.0).filter(|x| *x != 0);
                    }
                }
                g
            }
        };
        let width = simple_dict_width(&ctx.doc, d, desc, code)
            .or_else(|| std.and_then(|s| name.and_then(|n| encoding::std14_width(s, n))).map(|w| w as f64))
            .or_else(|| gid.and_then(|g| face.glyph_hor_advance(ttf_parser::GlyphId(g))).map(|a| a as f64 * 1000.0 / upem));
        glyphs.push(embed::Glyph { src: gid, width });
        map.insert((code, 1u8), glyphs.len() as u32);
        let text = uni.filter(|s| valid_unicode(s)).unwrap_or_else(|| UNKNOWN.to_string());
        tu.insert((code, 1u8), text);
    }
    let enc = embed::Enc::CMap { map, codespace: CMap::one_byte().codespace };
    let nd = embed::build_type0(&mut ctx.doc, src, &glyphs, enc, &tu, level)?;
    ctx.doc.objects.insert(fid, Object::Dictionary(nd));
    Ok(Outcome::Reembedded)
}

// ---------------------------------------------------------------- Type0

fn cid_widths(doc: &Document, cidfont: &Dictionary) -> (f64, BTreeMap<u32, f64>) {
    let dw = util::get_num(doc, cidfont, b"DW").unwrap_or(1000.0);
    let mut map = BTreeMap::new();
    if let Some(w) = util::get_array(doc, cidfont, b"W") {
        let items: Vec<Object> = w.iter().map(|x| deref(doc, x).clone()).collect();
        let mut i = 0;
        while i < items.len() {
            let Some(first) = num(&items[i]) else { i += 1; continue };
            match items.get(i + 1) {
                Some(Object::Array(a)) => {
                    for (k, x) in a.iter().enumerate() {
                        if let Some(v) = num(deref(doc, x)) {
                            map.insert(first as u32 + k as u32, v);
                        }
                    }
                    i += 2;
                }
                Some(o) => {
                    let (Some(last), Some(v)) = (num(o), items.get(i + 2).and_then(num)) else { break };
                    if last >= first && last - first < 65536.0 {
                        for c in first as u32..=last as u32 {
                            map.insert(c, v);
                        }
                    }
                    i += 3;
                }
                None => break,
            }
        }
    }
    (dw, map)
}

fn cid_to_gid(doc: &Document, cidfont: &Dictionary) -> Option<Vec<u16>> {
    match cidfont.get(b"CIDToGIDMap").ok().map(|o| deref(doc, o)) {
        Some(Object::Stream(s)) => util::stream_data(s).map(|d| d.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect()),
        _ => None,
    }
}

fn fix_type0(ctx: &mut Ctx, fid: ObjectId, d: &Dictionary, u: &FontUse) -> Result<Outcome> {
    let level = ctx.level;
    let doc = &ctx.doc;
    let Some(desc_ref) = util::get_array(doc, d, b"DescendantFonts").and_then(|a| a.first().cloned()) else {
        return Ok(Outcome::Unfixable("no descendant font".into()));
    };
    let Some(cf) = util::dict_of(doc, &desc_ref).cloned() else { return Ok(Outcome::Unfixable("no descendant font".into())) };
    let cf_id = desc_ref.as_reference().ok();
    let cmap = usage::font_cmap(doc, d);
    let enc_name = match d.get(b"Encoding").ok().map(|o| deref(doc, o)) {
        Some(Object::Name(n)) => Some(n.clone()),
        _ => None,
    };
    let uni_cmap = enc_name.as_deref().map(cmap::is_unicode_cmap).unwrap_or(false);
    let desc = descriptor(doc, &cf);
    let prog = program(doc, desc.as_ref());
    let base = util::get_name(doc, &cf, b"BaseFont").or(util::get_name(doc, d, b"BaseFont")).map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
    let (dw, wmap) = cid_widths(doc, &cf);
    let tu_existing = existing_tounicode(doc, d);
    let unicode = |code: Code| -> Option<String> {
        if let Some(s) = tu_existing.get(&code).filter(|s| valid_unicode(s)) {
            return Some(s.clone());
        }
        if uni_cmap {
            let bytes = code.0.to_be_bytes();
            let s = cmap::utf16be(&bytes[4 - code.1 as usize..]);
            if valid_unicode(&s) {
                return Some(s);
            }
        }
        None
    };
    let cid_of = |code: Code| -> u32 { cmap.cid(code).unwrap_or(if enc_name.as_deref().map(|n| n.starts_with(b"Identity")).unwrap_or(false) { code.0 } else { 0 }) };
    let wmode = cmap.wmode;
    let is_type2 = util::get_name(doc, &cf, b"Subtype") == Some(b"CIDFontType2".as_slice());

    let Some(prog) = prog else {
        if u.rendered.is_empty() {
            if level.needs_unicode() {
                ensure_tounicode_codes(&mut ctx.doc, fid, &u.codes, &unicode, &cmap.codespace, level.needs_structure());
            }
            return Ok(Outcome::Kept);
        }
        if wmode != 0 {
            return Ok(Outcome::Unfixable("vertical writing without embedded font".into()));
        }
        let fl = flags(doc, desc.as_ref());
        let ordering = util::get_dict(doc, &cf, b"CIDSystemInfo").and_then(|c| c.get(b"Ordering").ok()).and_then(util::decode_text).unwrap_or_default();
        let want = sysfonts::Want { name: base.clone(), bold: fl & (1 << 18) != 0, italic: fl & 64 != 0, serif: fl & 2 != 0, fixed: fl & 1 != 0, };
        let src = sysfonts::find(&ctx.opts.font_dirs, &want, None, true)
            .or_else(|| cjk_fallback(&ctx.opts.font_dirs, &ordering))
            .unwrap_or_else(|| sysfonts::bundled(&want));
        let face = match ttf_parser::Face::parse(&src.data, src.index) {
            Ok(f) => f,
            Err(_) => return Ok(Outcome::Unfixable("substitute font unreadable".into())),
        };
        let mut glyphs = Vec::new();
        let mut map = BTreeMap::new();
        let mut tu = BTreeMap::new();
        let mut unknown = 0;
        for &code in &u.codes {
            let uni = unicode(code);
            let gid = uni.as_deref().and_then(|s| s.chars().next()).and_then(|c| face.glyph_index(c)).map(|g| g.0).filter(|g| *g != 0);
            if uni.is_none() && u.rendered.contains(&code) {
                unknown += 1;
            }
            let cid = cid_of(code);
            let w = *wmap.get(&cid).unwrap_or(&dw);
            glyphs.push(embed::Glyph { src: gid, width: Some(w) });
            map.insert(code, glyphs.len() as u32);
            tu.insert(code, uni.filter(|s| valid_unicode(s)).unwrap_or_else(|| UNKNOWN.to_string()));
        }
        if unknown > 0 && unknown * 2 > u.rendered.len() {
            return Ok(Outcome::Unfixable("text without Unicode mapping in a font that is not embedded".into()));
        }
        let nd = embed::build_type0(&mut ctx.doc, &src, &glyphs, embed::Enc::CMap { map, codespace: cmap.codespace.clone() }, &tu, level)?;
        ctx.doc.objects.insert(fid, Object::Dictionary(nd));
        return Ok(Outcome::Reembedded);
    };

    match &prog {
        Prog::TrueType(data) if is_type2 => {
            let Ok(face) = ttf_parser::Face::parse(data, 0) else {
                return Ok(Outcome::Unfixable("unreadable embedded TrueType font".into()));
            };
            let upem = face.units_per_em() as f64;
            let map = cid_to_gid(doc, &cf);
            let gid_of = |cid: u32| -> u16 {
                match &map {
                    Some(m) => m.get(cid as usize).copied().unwrap_or(0),
                    None => cid.min(65535) as u16,
                }
            };
            let mut bad = false;
            for &code in &u.codes {
                let cid = cid_of(code);
                let gid = gid_of(cid);
                if gid >= face.number_of_glyphs() || (gid == 0 && (level.part() > 1 || u.rendered.contains(&code))) {
                    bad = true;
                    break;
                }
                if u.rendered.contains(&code) {
                    let pw = face.glyph_hor_advance(ttf_parser::GlyphId(gid)).unwrap_or(0) as f64 * 1000.0 / upem;
                    let dwv = *wmap.get(&cid).unwrap_or(&dw);
                    if (pw - dwv).abs() > 1.0 {
                        bad = true;
                        break;
                    }
                }
            }
            if !bad {
                if !cf.has(b"CIDToGIDMap") {
                    if let Some(id) = cf_id {
                        if let Some(c) = dict_mut(&mut ctx.doc, id) {
                            c.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
                        }
                    }
                }
                if level.needs_unicode() {
                    ensure_tounicode_codes(&mut ctx.doc, fid, &u.codes, &unicode, &cmap.codespace, level.needs_structure());
                }
                return Ok(Outcome::Kept);
            }
            if wmode != 0 {
                return Ok(Outcome::Unfixable("vertical font with inconsistent glyphs".into()));
            }
            // Rebuild from the embedded program.
            let src = sysfonts::FontData { data: std::sync::Arc::new(data.clone()), index: 0, ps_name: encoding::strip_subset(&base).to_string() };
            let reverse = reverse_cmap(&face);
            let mut glyphs = Vec::new();
            let mut cm = BTreeMap::new();
            let mut tu = BTreeMap::new();
            for &code in &u.codes {
                let cid = cid_of(code);
                let gid = gid_of(cid);
                let g = (gid != 0 && gid < face.number_of_glyphs()).then_some(gid);
                glyphs.push(embed::Glyph { src: g, width: Some(*wmap.get(&cid).unwrap_or(&dw)) });
                cm.insert(code, glyphs.len() as u32);
                let text = unicode(code).or_else(|| g.and_then(|g| reverse.get(&g).map(|c| c.to_string())));
                tu.insert(code, text.filter(|s| valid_unicode(s)).unwrap_or_else(|| UNKNOWN.to_string()));
            }
            let nd = embed::build_type0(&mut ctx.doc, &src, &glyphs, embed::Enc::CMap { map: cm, codespace: cmap.codespace.clone() }, &tu, level)?;
            ctx.doc.objects.insert(fid, Object::Dictionary(nd));
            Ok(Outcome::Fixed)
        }
        _ => {
            // CFF-based CIDFont: CIDs must exist in the charset.
            let cff_data: Vec<u8> = match &prog {
                Prog::OpenType(data) => ttf_parser::Face::parse(data, 0)
                    .ok()
                    .and_then(|f| f.raw_face().table(ttf_parser::Tag::from_bytes(b"CFF ")).map(|t| t.to_vec()))
                    .unwrap_or_default(),
                Prog::Cff(data) => data.clone(),
                _ => Vec::new(),
            };
            let present: BTreeSet<u32> = match ttf_parser::cff::Table::parse(&cff_data) {
                Some(t) => {
                    let mut s = BTreeSet::new();
                    for g in 0..t.number_of_glyphs() {
                        match t.glyph_cid(ttf_parser::GlyphId(g)) {
                            Some(c) => {
                                s.insert(c as u32);
                            }
                            None => {
                                s.insert(g as u32);
                            }
                        }
                    }
                    s
                }
                None => BTreeSet::new(),
            };
            let missing = u.codes.iter().any(|&code| {
                let cid = cid_of(code);
                (cid == 0 && (level.part() > 1 || u.rendered.contains(&code))) || (!present.is_empty() && !present.contains(&cid) && u.rendered.contains(&code))
            });
            if missing {
                return Ok(Outcome::Unfixable("glyphs missing in the embedded font".into()));
            }
            if level.needs_unicode() {
                ensure_tounicode_codes(&mut ctx.doc, fid, &u.codes, &unicode, &cmap.codespace, level.needs_structure());
            }
            Ok(Outcome::Kept)
        }
    }
}

fn reverse_cmap(face: &ttf_parser::Face) -> BTreeMap<u16, char> {
    let mut m = BTreeMap::new();
    if let Some(c) = face.tables().cmap {
        for s in c.subtables {
            if !s.is_unicode() {
                continue;
            }
            s.codepoints(|cp| {
                if let (Some(ch), Some(g)) = (char::from_u32(cp), s.glyph_index(cp)) {
                    m.entry(g.0).or_insert(ch);
                }
            });
        }
    }
    m
}

fn cjk_fallback(dirs: &[std::path::PathBuf], ordering: &str) -> Option<sysfonts::FontData> {
    let names: &[&str] = match ordering {
        "Japan1" => &["Yu Gothic", "MS Gothic", "Meiryo", "Noto Sans CJK JP", "Noto Sans JP"],
        "GB1" => &["Microsoft YaHei", "SimSun", "Noto Sans CJK SC", "Noto Sans SC"],
        "CNS1" => &["Microsoft JhengHei", "MingLiU", "Noto Sans CJK TC", "Noto Sans TC"],
        "Korea1" => &["Malgun Gothic", "Gulim", "Noto Sans CJK KR", "Noto Sans KR"],
        _ => &["Arial Unicode MS", "Segoe UI", "Arial", "DejaVu Sans", "Noto Sans"],
    };
    for n in names {
        let w = sysfonts::Want { name: n.to_string(), ..Default::default() };
        if let Some(f) = sysfonts::find(dirs, &w, None, true) {
            return Some(f);
        }
    }
    None
}

// ---------------------------------------------------------------- Type3

fn fix_type3(ctx: &mut Ctx, fid: ObjectId, d: &Dictionary, u: &FontUse) -> Result<Outcome> {
    let level = ctx.level;
    let doc = &mut ctx.doc;
    let names = encoding::simple_encoding(doc, d, None);
    let mut d2 = d.clone();
    let mut changed = false;
    // Every used code needs a glyph procedure.
    let mut cp = util::get_dict(doc, d, b"CharProcs").cloned().unwrap_or_default();
    let cp_ref = d.get(b"CharProcs").ok().and_then(|o| o.as_reference().ok());
    let mut diffs: Vec<(u32, String)> = Vec::new();
    for &(code, _) in &u.codes {
        let name = names[code as usize & 0xFF].clone();
        let name = match name {
            Some(n) if n != ".notdef" => n,
            _ => {
                let n = format!("g{code}");
                diffs.push((code, n.clone()));
                n
            }
        };
        // Glyph widths: the d0/d1 advance must equal /Widths.
        let dict_w = simple_dict_width(doc, d, None, code).unwrap_or(0.0);
        if !cp.has(name.as_bytes()) {
            let mut c = Vec::new();
            crate::content::write_real(&mut c, dict_w);
            c.extend_from_slice(b" 0 d0\n");
            let sid = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), c)));
            cp.set(name.into_bytes(), Object::Reference(sid));
            changed = true;
        } else if let Ok(Object::Reference(sid)) = cp.get(name.as_bytes()).cloned() {
            if let Some(Object::Stream(s)) = doc.objects.get(&sid) {
                if let Some(data) = util::stream_data(s) {
                    if let Some(fixed) = set_glyph_width(&data, dict_w) {
                        let dict = s.dict.clone();
                        doc.objects.insert(sid, Object::Stream(util::flate_stream(dict, &fixed)));
                        changed = true;
                    }
                }
            }
        }
    }
    if !diffs.is_empty() {
        let mut enc = util::get_dict(doc, d, b"Encoding").cloned().unwrap_or_default();
        let mut arr = util::get_array(doc, &enc, b"Differences").cloned().unwrap_or_default();
        for (c, n) in &diffs {
            arr.push(Object::Integer(*c as i64));
            arr.push(Object::Name(n.as_bytes().to_vec()));
        }
        enc.set("Differences", Object::Array(arr));
        d2.set("Encoding", Object::Dictionary(enc));
        changed = true;
    }
    if changed {
        match cp_ref {
            Some(r) => {
                doc.objects.insert(r, Object::Dictionary(cp));
            }
            None => d2.set("CharProcs", Object::Dictionary(cp)),
        }
        doc.objects.insert(fid, Object::Dictionary(d2.clone()));
    }
    if level.needs_unicode() {
        let tu_existing = existing_tounicode(doc, &d2);
        let names2 = encoding::simple_encoding(doc, &d2, None);
        let unicode = |code: u32| -> Option<String> {
            tu_existing
                .get(&(code, 1))
                .filter(|s| valid_unicode(s))
                .cloned()
                .or_else(|| names2[code as usize & 0xFF].as_deref().and_then(encoding::glyph_unicode))
        };
        ensure_tounicode(doc, fid, &u.codes, &unicode, &CMap::one_byte().codespace, level.needs_structure());
    }
    Ok(if changed { Outcome::Fixed } else { Outcome::Kept })
}

/// Rewrite the advance of a Type 3 glyph procedure's d0/d1 when it differs
/// from `w` (more than the validators' tolerance). `None` = unchanged.
fn set_glyph_width(data: &[u8], w: f64) -> Option<Vec<u8>> {
    let mut out = None;
    crate::content::visit(data, |op| {
        if op.name == b"d0" || op.name == b"d1" {
            let wx = op.args.first().and_then(|a| a.as_f64()).unwrap_or(0.0);
            if (wx - w).abs() > 0.5 {
                let mut args = op.args.to_vec();
                if args.is_empty() {
                    args = vec![crate::content::Operand::Real(w), crate::content::Operand::Int(0)];
                } else {
                    args[0] = crate::content::Operand::Real(w);
                }
                let mut v = data[..op.start].to_vec();
                crate::content::write_op(&mut v, op.name, &args);
                v.extend_from_slice(&data[op.end..]);
                out = Some(v);
            }
        }
        false
    });
    out
}

// ---------------------------------------------------------------- Unicode

/// Make sure the font maps every code in `codes` to Unicode (keeping
/// existing entries); unknown codes get U+FFFD.
fn ensure_tounicode(doc: &mut Document, fid: ObjectId, codes: &BTreeSet<Code>, unicode: &dyn Fn(u32) -> Option<String>, codespace: &[(Vec<u8>, Vec<u8>)], no_pua: bool) {
    ensure_tounicode_codes(doc, fid, codes, &|c: Code| unicode(c.0), codespace, no_pua)
}

fn ensure_tounicode_codes(doc: &mut Document, fid: ObjectId, codes: &BTreeSet<Code>, unicode: &dyn Fn(Code) -> Option<String>, codespace: &[(Vec<u8>, Vec<u8>)], no_pua: bool) {
    // Level a: Private Use Area values would need ActualText; avoid them.
    let valid_unicode = |s: &str| valid_unicode(s) && !(no_pua && has_pua(s));
    let Some(d) = doc.objects.get(&fid).and_then(|o| o.as_dict().ok()).cloned() else { return };
    let mut map = existing_tounicode(doc, &d);
    let mut complete = d.has(b"ToUnicode");
    for &c in codes {
        let ok = map.get(&c).map(|s| valid_unicode(s)).unwrap_or(false);
        if !ok {
            complete = false;
            map.insert(c, unicode(c).filter(|s| valid_unicode(s)).unwrap_or_else(|| UNKNOWN.to_string()));
        }
    }
    // Drop invalid leftovers from the original map.
    let had_invalid = map.values().any(|s| !valid_unicode(s));
    map.retain(|_, s| valid_unicode(s));
    if complete && !had_invalid {
        return;
    }
    let data = cmap::write_to_unicode(&map, codespace);
    let id = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &data)));
    if let Some(dd) = dict_mut(doc, fid) {
        dd.set("ToUnicode", Object::Reference(id));
    }
}

// ---------------------------------------------------------------- descriptors

/// CharSet / CIDSet: required (and must be complete) for PDF/A-1 subsets,
/// optional later — where they cannot be verified they are removed.
fn cleanup_descriptors(ctx: &mut Ctx, uses: &BTreeMap<ObjectId, FontUse>) {
    let part = ctx.level.part();
    let doc = &mut ctx.doc;
    let mut descs: Vec<(ObjectId, ObjectId, bool)> = Vec::new(); // (font, descriptor, is_cid)
    for fid in uses.keys() {
        let Some(d) = doc.objects.get(fid).and_then(|o| o.as_dict().ok()) else { continue };
        let (target, is_cid) = if util::get_name(doc, d, b"Subtype") == Some(b"Type0".as_slice()) {
            match util::get_array(doc, d, b"DescendantFonts").and_then(|a| a.first()).and_then(|o| util::dict_of(doc, o)) {
                Some(cf) => (cf.get(b"FontDescriptor").ok().and_then(|o| o.as_reference().ok()), true),
                None => (None, true),
            }
        } else {
            (d.get(b"FontDescriptor").ok().and_then(|o| o.as_reference().ok()), false)
        };
        if let Some(t) = target {
            descs.push((*fid, t, is_cid));
        }
    }
    for (fid, did, is_cid) in descs {
        let Some(desc) = doc.objects.get(&did).and_then(|o| o.as_dict().ok()).cloned() else { continue };
        let name = desc.get(b"FontName").ok().and_then(|o| o.as_name().ok()).map(|n| String::from_utf8_lossy(n).to_string()).unwrap_or_default();
        let subset = encoding::strip_subset(&name) != name;
        if part > 1 {
            if let Some(dd) = dict_mut(doc, did) {
                dd.remove(b"CIDSet");
                dd.remove(b"CharSet");
            }
            continue;
        }
        if !subset {
            continue;
        }
        if is_cid {
            if desc.has(b"CIDSet") {
                continue;
            }
            if let Some(bits) = cidset_for(doc, fid, &desc) {
                let s = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &bits)));
                if let Some(dd) = dict_mut(doc, did) {
                    dd.set("CIDSet", s);
                }
            }
        } else if let Some(cs) = charset_for(doc, &desc) {
            if let Some(dd) = dict_mut(doc, did) {
                dd.set("CharSet", Object::String(cs.into_bytes(), StringFormat::Literal));
            }
        }
    }
}

fn charset_for(doc: &Document, desc: &Dictionary) -> Option<String> {
    let names: Vec<String> = match program(doc, Some(desc))? {
        Prog::Type1(data, l1) => type1::parse(&data, l1)?.widths.keys().cloned().collect(),
        Prog::Cff(data) => {
            let t = ttf_parser::cff::Table::parse(&data)?;
            (0..t.number_of_glyphs()).filter_map(|g| t.glyph_name(ttf_parser::GlyphId(g)).map(|s| s.to_string())).collect()
        }
        _ => return None,
    };
    let mut s = String::new();
    for n in names.into_iter().filter(|n| n != ".notdef") {
        s.push('/');
        s.push_str(&n);
    }
    Some(s)
}

fn cidset_for(doc: &Document, fid: ObjectId, desc: &Dictionary) -> Option<Vec<u8>> {
    let d = doc.objects.get(&fid)?.as_dict().ok()?;
    let cf = util::get_array(doc, d, b"DescendantFonts").and_then(|a| a.first()).and_then(|o| util::dict_of(doc, o))?;
    let cids: Vec<u32> = match program(doc, Some(desc))? {
        Prog::TrueType(data) => {
            let face = ttf_parser::Face::parse(&data, 0).ok()?;
            let n = face.number_of_glyphs() as u32;
            match cid_to_gid(doc, cf) {
                Some(m) => m.iter().enumerate().filter(|(c, g)| (**g as u32) < n && (**g != 0 || *c == 0)).map(|(c, _)| c as u32).collect(),
                None => (0..n).collect(),
            }
        }
        Prog::Cff(data) | Prog::OpenType(data) => {
            let t = ttf_parser::cff::Table::parse(&data)?;
            (0..t.number_of_glyphs()).map(|g| t.glyph_cid(ttf_parser::GlyphId(g)).map(|c| c as u32).unwrap_or(g as u32)).collect()
        }
        _ => return None,
    };
    let max = *cids.iter().max()? as usize;
    let mut bits = vec![0u8; max / 8 + 1];
    for c in cids {
        bits[c as usize / 8] |= 0x80 >> (c % 8);
    }
    Some(bits)
}

// ---------------------------------------------------------------- fallback

/// Pages showing text with fonts that could not be fixed are rendered.
fn render_pages_using(ctx: &mut Ctx, fonts: &[(ObjectId, String)]) -> Result<()> {
    let ids: BTreeSet<ObjectId> = fonts.iter().map(|f| f.0).collect();
    let pages = util::pages(&ctx.doc);
    let mut hit = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        if page_uses_fonts(&ctx.doc, *p, &ids) {
            hit.push((i, *p));
        }
    }
    let Some(r) = ctx.opts.rasterizer.clone() else {
        let names: Vec<String> = fonts.iter().map(|f| f.1.clone()).collect();
        return Err(PdfaError::NeedsRasterizer(format!("font problems: {}", names.join("; "))));
    };
    for (i, p) in &hit {
        let raster = r
            .render_page(ctx.source, *i as u32, ctx.opts.flatten_dpi)
            .map_err(|e| PdfaError::Unsupported(format!("rendering page {} failed: {e}", i + 1)))?;
        crate::flatten::replace_page(ctx, *p, &raster)?;
    }
    if !hit.is_empty() {
        let list: Vec<String> = hit.iter().map(|(i, _)| (i + 1).to_string()).collect();
        ctx.report.note("pages_rendered", list.join(", "));
    }
    Ok(())
}

fn page_uses_fonts(doc: &Document, page: ObjectId, fonts: &BTreeSet<ObjectId>) -> bool {
    // Conservative: the page's resources (including forms) reference one.
    fn walk(doc: &Document, res: &Dictionary, fonts: &BTreeSet<ObjectId>, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        if let Some(f) = util::get_dict(doc, res, b"Font") {
            if f.iter().any(|(_, v)| matches!(v, Object::Reference(r) if fonts.contains(r))) {
                return true;
            }
        }
        if let Some(x) = util::get_dict(doc, res, b"XObject") {
            for (_, v) in x.iter() {
                if let Object::Stream(s) = deref(doc, v) {
                    if let Some(r) = util::get_dict(doc, &s.dict, b"Resources") {
                        if walk(doc, r, fonts, depth + 1) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
    let res = util::inherited(doc, page, b"Resources").and_then(|o| util::dict_of(doc, &o).cloned());
    if let Some(r) = res {
        if walk(doc, &r, fonts, 0) {
            return true;
        }
    }
    for a in util::page_annots(doc, page) {
        if let Some(ap) = util::dict_of(doc, &a).and_then(|d| util::get_dict(doc, d, b"AP")) {
            for (_, v) in ap.iter() {
                if let Object::Stream(s) = deref(doc, v) {
                    if let Some(r) = util::get_dict(doc, &s.dict, b"Resources") {
                        if walk(doc, r, fonts, 0) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------- text layer

/// Invisible text (render mode 3) for a rendered page. Returns the font
/// object and the content.
pub fn text_layer(doc: &mut Document, raster: &RasterPage, m: &[f64; 6], level: Level) -> Result<(Object, Vec<u8>)> {
    let mut chars: Vec<char> = raster.words.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_control()).collect();
    chars.push(' ');
    chars.sort();
    chars.dedup();
    let src = sysfonts::bundled(&sysfonts::Want::default());
    let (font, cids, widths) = embed::text_font(doc, &src, &chars, level)?;
    let fid = doc.add_object(Object::Dictionary(font));
    let (iw, ih) = (raster.width.max(1) as f64, raster.height.max(1) as f64);
    let p = |u: f64, v: f64| (m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5]);
    let mut c = Vec::new();
    c.extend_from_slice(b"BT 3 Tr\n");
    for w in &raster.words {
        let text: Vec<char> = w.text.chars().filter(|c| !c.is_control()).collect();
        if text.is_empty() || w.w <= 0.0 || w.h <= 0.0 {
            continue;
        }
        let (u0, u1) = (w.x as f64 / iw, (w.x + w.w) as f64 / iw);
        let (vt, vb) = (1.0 - w.y as f64 / ih, 1.0 - (w.y + w.h) as f64 / ih);
        let o = p(u0, vb + (vt - vb) * 0.2);
        let r = p(u1, vb + (vt - vb) * 0.2);
        let t = p(u0, vt);
        let b = p(u0, vb);
        let (dx, dy) = (r.0 - o.0, r.1 - o.1);
        let len = (dx * dx + dy * dy).sqrt();
        let size = ((t.0 - b.0).powi(2) + (t.1 - b.1).powi(2)).sqrt();
        if len <= 0.0 || size <= 0.0 {
            continue;
        }
        let cid: Vec<u16> = text.iter().map(|ch| *cids.get(ch).unwrap_or(&1)).collect();
        let natural: f64 = cid.iter().map(|c| widths.get(*c as usize).copied().unwrap_or(500.0)).sum::<f64>() * size / 1000.0;
        let tz = if natural > 0.0 { (len / natural * 100.0).clamp(1.0, 10000.0) } else { 100.0 };
        let (cx, cy) = (dx / len, dy / len);
        let nums = |c: &mut Vec<u8>, v: &[f64]| {
            for x in v {
                crate::content::write_real(c, *x);
                c.push(b' ');
            }
        };
        c.extend_from_slice(b"/FT0 ");
        nums(&mut c, &[size]);
        c.extend_from_slice(b"Tf ");
        nums(&mut c, &[tz]);
        c.extend_from_slice(b"Tz ");
        nums(&mut c, &[cx, cy, -cy, cx, o.0, o.1]);
        c.extend_from_slice(b"Tm ");
        crate::content::write_string(&mut c, &cid.iter().flat_map(|x| x.to_be_bytes()).collect::<Vec<u8>>());
        c.extend_from_slice(b" Tj\n");
    }
    c.extend_from_slice(b"ET\n");
    Ok((Object::Reference(fid), c))
}

// ---------------------------------------------------------------- forms

/// Text and choice fields without a usable appearance get one drawn with an
/// embedded font, so they stay readable and fillable.
fn field_appearances(ctx: &mut Ctx) -> Result<()> {
    let level = ctx.level;
    let ids: Vec<ObjectId> = ctx.doc.objects.keys().copied().collect();
    let mut todo: Vec<(ObjectId, [f64; 4], String, f64, Vec<f64>, i64)> = Vec::new();
    for id in ids {
        let doc = &ctx.doc;
        let d = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Widget".as_slice()) => d,
            _ => continue,
        };
        let ft = crate::annots::field_type(doc, d);
        if !matches!(ft.as_deref(), Some(b"Tx") | Some(b"Ch")) {
            continue;
        }
        if util::get_dict(doc, d, b"AP").map(|a| a.has(b"N")).unwrap_or(false) {
            continue;
        }
        let Some(rect) = crate::annots::rect_of(doc, d) else { continue };
        if rect[0] == rect[2] && rect[1] == rect[3] {
            continue;
        }
        // Value and DA (inherited through the field hierarchy).
        let mut value = String::new();
        let mut da = String::new();
        let mut q = 0i64;
        let mut cur = d.clone();
        for _ in 0..16 {
            if value.is_empty() {
                if let Some(v) = cur.get(b"V").ok().map(|o| deref(doc, o)) {
                    value = match v {
                        Object::Array(a) => a.iter().filter_map(|x| util::decode_text(deref(doc, x))).collect::<Vec<_>>().join(", "),
                        o => util::decode_text(o).unwrap_or_default(),
                    };
                }
            }
            if da.is_empty() {
                if let Some(s) = cur.get(b"DA").ok().and_then(util::decode_text) {
                    da = s;
                }
            }
            if q == 0 {
                q = util::get_num(doc, &cur, b"Q").unwrap_or(0.0) as i64;
            }
            match cur.get(b"Parent").ok().and_then(|p| util::dict_of(doc, p)) {
                Some(p) => cur = p.clone(),
                None => break,
            }
        }
        let (size, color) = parse_da(&da);
        todo.push((id, rect, value, size, color, q));
    }
    if todo.is_empty() {
        return Ok(());
    }
    let mut chars: Vec<char> = todo.iter().flat_map(|t| t.2.chars()).filter(|c| !c.is_control()).collect();
    chars.push(' ');
    chars.sort();
    chars.dedup();
    let src = sysfonts::bundled(&sysfonts::Want::default());
    let (font, cids, widths) = embed::text_font(&mut ctx.doc, &src, &chars, level)?;
    let font_id = ctx.doc.add_object(Object::Dictionary(font));
    for (id, rect, value, size, color, q) in todo {
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let size = if size <= 0.0 { (h * 0.7).clamp(4.0, 12.0) } else { size };
        let text: Vec<char> = value.chars().filter(|c| !c.is_control()).collect();
        let cid: Vec<u16> = text.iter().map(|c| *cids.get(c).unwrap_or(&1)).collect();
        let tw: f64 = cid.iter().map(|c| widths.get(*c as usize).copied().unwrap_or(500.0)).sum::<f64>() * size / 1000.0;
        let x = match q {
            1 => rect[0] + (w - tw) / 2.0,
            2 => rect[2] - 2.0 - tw,
            _ => rect[0] + 2.0,
        };
        let y = rect[1] + (h - size * 0.8) / 2.0;
        let mut c = Vec::new();
        c.extend_from_slice(b"/Tx BMC q ");
        for v in [rect[0] + 1.0, rect[1] + 1.0, (w - 2.0).max(0.0), (h - 2.0).max(0.0)] {
            crate::content::write_real(&mut c, v);
            c.push(b' ');
        }
        c.extend_from_slice(b"re W n BT /FAp ");
        crate::content::write_real(&mut c, size);
        c.extend_from_slice(b" Tf ");
        for v in &color {
            crate::content::write_real(&mut c, *v);
            c.push(b' ');
        }
        c.extend_from_slice(match color.len() {
            1 => b"g ".as_slice(),
            4 => b"k ".as_slice(),
            _ => b"rg ".as_slice(),
        });
        for v in [x, y] {
            crate::content::write_real(&mut c, v);
            c.push(b' ');
        }
        c.extend_from_slice(b"Td ");
        crate::content::write_string(&mut c, &cid.iter().flat_map(|x| x.to_be_bytes()).collect::<Vec<u8>>());
        c.extend_from_slice(b" Tj ET Q EMC\n");
        let res = dictionary! { "Font" => dictionary! { "FAp" => font_id } };
        let s = crate::ap::form(rect, c, Some(res));
        let sid = ctx.doc.add_object(Object::Stream(s));
        if let Some(dd) = dict_mut(&mut ctx.doc, id) {
            dd.set("AP", dictionary! { "N" => sid });
        }
    }
    ctx.report.note("appearances_generated", "form fields");
    Ok(())
}

/// Font size and colour from a default appearance string.
fn parse_da(da: &str) -> (f64, Vec<f64>) {
    let mut size = 0.0;
    let mut color = vec![0.0];
    crate::content::visit(da.as_bytes(), |op| {
        match op.name {
            b"Tf" => size = op.args.get(1).and_then(|a| a.as_f64()).unwrap_or(0.0),
            b"g" | b"rg" | b"k" => color = op.args.iter().filter_map(|a| a.as_f64()).collect(),
            _ => {}
        }
        true
    });
    (size, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn da_parsing() {
        assert_eq!(parse_da("/Helv 10 Tf 1 0 0 rg"), (10.0, vec![1.0, 0.0, 0.0]));
        assert_eq!(parse_da(""), (0.0, vec![0.0]));
        assert_eq!(parse_da("0 g /F 0 Tf"), (0.0, vec![0.0]));
    }

    #[test]
    fn unicode_validity() {
        assert!(valid_unicode("a"));
        assert!(!valid_unicode(""));
        assert!(!valid_unicode("\u{FEFF}"));
        assert!(!valid_unicode("a\0"));
    }

    #[test]
    fn truetype_lookup_rules() {
        let data = pdf_fonts::family("Roboto").unwrap().face(false, false);
        let face = ttf_parser::Face::parse(data, 0).unwrap();
        let g = truetype_gid(&face, b'A', Some("A"), false);
        assert_eq!(Some(g), face.glyph_index('A').map(|x| x.0));
        assert_eq!(truetype_gid(&face, b'A', Some("nosuchglyph"), false), 0);
        let rev = reverse_cmap(&face);
        assert_eq!(rev.get(&g), Some(&'A'));
    }
}
