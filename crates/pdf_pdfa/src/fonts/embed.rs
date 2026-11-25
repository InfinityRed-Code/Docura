//! Building embedded composite fonts.
//!
//! Every font the converter has to (re-)embed becomes a Type0 font over a
//! CIDFontType2 subset. The original character codes stay as they are in
//! the content streams: an embedded encoding CMap maps each code to its own
//! CID, the CID equals the glyph id in the subset, and every glyph carries
//! the advance the PDF already used for that code. Text therefore lands
//! exactly where it was, the widths agree with the font program, and no
//! code ever reaches .notdef.

use std::collections::BTreeMap;

use lopdf::{dictionary, Dictionary, Document, Object};

use super::sfnt::{self, OutGlyph, Sfnt};
use super::sysfonts::FontData;
use crate::cmap::{self, Code};
use crate::level::Level;
use crate::util;
use crate::{PdfaError, Result};

/// How codes reach CIDs.
pub enum Enc {
    /// Two-byte codes equal to the CID (text layers we write ourselves).
    IdentityH,
    /// Original codes through an embedded CMap.
    CMap { map: BTreeMap<Code, u32>, codespace: Vec<(Vec<u8>, Vec<u8>)> },
}

/// A glyph to put into the subset; its CID/GID is its index + 1.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub src: Option<u16>,
    /// Advance in 1/1000 em; `None` keeps the font's own advance.
    pub width: Option<f64>,
}

pub(super) fn subset_tag(seed: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in seed {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    (0..6).map(|i| (b'A' + ((h >> (i * 5)) % 26) as u8) as char).collect()
}

fn clean_ps(name: &str) -> String {
    let s: String = name.chars().filter(|c| c.is_ascii_graphic() && !"()<>[]{}/%#".contains(*c)).collect();
    if s.is_empty() {
        "Font".into()
    } else {
        s
    }
}

/// Build the Type0 font dictionary (objects are added to `doc`).
pub fn build_type0(
    doc: &mut Document,
    src: &FontData,
    glyphs: &[Glyph],
    enc: Enc,
    to_unicode: &BTreeMap<Code, String>,
    level: Level,
) -> Result<Dictionary> {
    let face = Sfnt::parse(&src.data, src.index).map_err(|e| PdfaError::Unsupported(format!("font {}: {e}", src.ps_name)))?;
    let upem = face.units_per_em() as f64;
    let mut out = vec![OutGlyph { src: Some(0), advance: None }];
    for g in glyphs {
        let adv = g.width.map(|w| (w * upem / 1000.0).round().clamp(0.0, 65535.0) as u16);
        out.push(OutGlyph { src: g.src, advance: adv });
    }
    let sub = sfnt::subset(&face, &out, None).map_err(|e| PdfaError::Unsupported(format!("font {}: {e}", src.ps_name)))?;

    // Widths as stored in the subset (what veraPDF compares against).
    let width_of = |i: usize| -> i64 {
        let og = &out[i];
        let adv = match og.advance {
            Some(a) => a,
            None => og.src.map(|s| face.h_metrics(s).0).unwrap_or(0),
        };
        (adv as f64 * 1000.0 / upem).round() as i64
    };
    let mut wcount: BTreeMap<i64, usize> = BTreeMap::new();
    for i in 1..out.len() {
        *wcount.entry(width_of(i)).or_default() += 1;
    }
    let dw = wcount.iter().max_by_key(|(_, c)| **c).map(|(w, _)| *w).unwrap_or(1000);
    let mut w_arr: Vec<Object> = Vec::new();
    let mut i = 1;
    while i < out.len() {
        let w = width_of(i);
        if w == dw {
            i += 1;
            continue;
        }
        let start = i;
        let mut run = vec![Object::Integer(w)];
        i += 1;
        while i < out.len() && width_of(i) != dw && run.len() < 500 {
            run.push(Object::Integer(width_of(i)));
            i += 1;
        }
        w_arr.push(Object::Integer(start as i64));
        w_arr.push(Object::Array(run));
    }

    let tag = subset_tag(&sub.data[..sub.data.len().min(4096)]);
    let ps = format!("{tag}+{}", clean_ps(&src.ps_name));

    // Descriptor metrics.
    let tf = ttf_parser::Face::parse(&src.data, src.index).ok();
    let sc = |v: f64| (v * 1000.0 / upem).round() as i64;
    let (bbox, ascent, descent, cap, italic) = match &tf {
        Some(f) => {
            let b = f.global_bounding_box();
            (
                [sc(b.x_min as f64), sc(b.y_min as f64), sc(b.x_max as f64), sc(b.y_max as f64)],
                sc(f.ascender() as f64),
                sc(f.descender() as f64),
                sc(f.capital_height().unwrap_or(f.ascender()) as f64),
                f.italic_angle() as f64,
            )
        }
        None => ([0, -200, 1000, 900], 800, -200, 700, 0.0),
    };
    let mut ff = util::flate_stream(dictionary! { "Length1" => sub.data.len() as i64 }, &sub.data);
    ff.allows_compression = false;
    let ff_id = doc.add_object(Object::Stream(ff));
    let mut fd = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(ps.as_bytes().to_vec()),
        "Flags" => 4,
        "FontBBox" => Object::Array(bbox.iter().map(|v| Object::Integer(*v)).collect()),
        "ItalicAngle" => Object::Real(italic as f32),
        "Ascent" => ascent,
        "Descent" => descent,
        "CapHeight" => cap,
        "StemV" => 80,
        "FontFile2" => ff_id,
    };
    if level.part() == 1 {
        // PDF/A-1: subsets list their CIDs (CID = GID here, all present).
        let n = sub.num_glyphs as usize;
        let mut bits = vec![0u8; n.div_ceil(8)];
        for c in 0..n {
            bits[c / 8] |= 0x80 >> (c % 8);
        }
        let cs = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &bits)));
        fd.set("CIDSet", cs);
    }
    let fd_id = doc.add_object(Object::Dictionary(fd));

    let csi = dictionary! {
        "Registry" => Object::string_literal("Adobe"),
        "Ordering" => Object::string_literal("Identity"),
        "Supplement" => 0,
    };
    let mut cidfont = dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(ps.as_bytes().to_vec()),
        "CIDSystemInfo" => csi.clone(),
        "FontDescriptor" => fd_id,
        "DW" => dw,
        "CIDToGIDMap" => "Identity",
    };
    if !w_arr.is_empty() {
        cidfont.set("W", Object::Array(w_arr));
    }
    let cid_id = doc.add_object(Object::Dictionary(cidfont));

    let (encoding, tu_space): (Object, Vec<(Vec<u8>, Vec<u8>)>) = match enc {
        Enc::IdentityH => (Object::Name(b"Identity-H".to_vec()), cmap::CMap::identity().codespace),
        Enc::CMap { map, codespace } => {
            let name = format!("Docura-{tag}");
            let data = cmap::write_encoding(&name, &map, &codespace, 0);
            let s = util::flate_stream(
                dictionary! {
                    "Type" => "CMap",
                    "CMapName" => Object::Name(name.into_bytes()),
                    "CIDSystemInfo" => csi,
                    "WMode" => 0,
                },
                &data,
            );
            (Object::Reference(doc.add_object(Object::Stream(s))), codespace)
        }
    };
    let mut font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(ps.into_bytes()),
        "Encoding" => encoding,
        "DescendantFonts" => vec![Object::Reference(cid_id)],
    };
    if !to_unicode.is_empty() {
        let tu = cmap::write_to_unicode(to_unicode, &tu_space);
        let tid = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &tu)));
        font.set("ToUnicode", tid);
    }
    Ok(font)
}

/// Text font for invisible text layers and generated appearances: glyphs
/// for `chars` from `src`; returns the font dictionary and char → CID.
pub fn text_font(doc: &mut Document, src: &FontData, chars: &[char], level: Level) -> Result<(Dictionary, BTreeMap<char, u16>, Vec<f64>)> {
    let face = ttf_parser::Face::parse(&src.data, src.index).map_err(|e| PdfaError::Unsupported(format!("font: {e}")))?;
    let upem = face.units_per_em() as f64;
    let fallback = face.glyph_index('?').or_else(|| face.glyph_index(' ')).map(|g| g.0);
    let mut glyphs = Vec::new();
    let mut map = BTreeMap::new();
    let mut tu = BTreeMap::new();
    let mut widths = vec![0.0];
    for (i, &c) in chars.iter().enumerate() {
        let gid = face.glyph_index(c).map(|g| g.0).or(fallback);
        let w = gid.and_then(|g| face.glyph_hor_advance(ttf_parser::GlyphId(g))).map(|a| a as f64 * 1000.0 / upem).unwrap_or(500.0);
        glyphs.push(Glyph { src: gid, width: Some(w) });
        let cid = (i + 1) as u16;
        map.insert(c, cid);
        widths.push(w);
        if !(c == '\0' || c == '\u{FEFF}' || c == '\u{FFFE}') {
            tu.insert((cid as u32, 2u8), c.to_string());
        }
    }
    let d = build_type0(doc, src, &glyphs, Enc::IdentityH, &tu, level)?;
    Ok((d, map, widths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::sysfonts;

    #[test]
    fn builds_type0_with_cmap_and_tounicode() {
        let mut doc = Document::with_version("1.7");
        let src = sysfonts::bundled(&sysfonts::Want::default());
        let face = ttf_parser::Face::parse(&src.data, 0).unwrap();
        let ga = face.glyph_index('A').unwrap().0;
        let glyphs = [Glyph { src: Some(ga), width: Some(700.0) }, Glyph { src: None, width: Some(250.0) }];
        let mut map = BTreeMap::new();
        map.insert((0x41, 1), 1u32);
        map.insert((0x20, 1), 2u32);
        let mut tu = BTreeMap::new();
        tu.insert((0x41, 1), "A".to_string());
        let l1: Level = "1b".parse().unwrap();
        let f = build_type0(&mut doc, &src, &glyphs, Enc::CMap { map, codespace: cmap::CMap::one_byte().codespace }, &tu, l1).unwrap();
        assert_eq!(f.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
        let enc = f.get(b"Encoding").unwrap().as_reference().unwrap();
        let es = doc.get_object(enc).unwrap().as_stream().unwrap();
        let m = cmap::CMap::parse(&es.decompressed_content().unwrap());
        assert_eq!(m.cid((0x41, 1)), Some(1));
        let cid = f.get(b"DescendantFonts").unwrap().as_array().unwrap()[0].as_reference().unwrap();
        let cd = doc.get_object(cid).unwrap().as_dict().unwrap();
        let fd = doc.get_object(cd.get(b"FontDescriptor").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
        assert!(fd.has(b"CIDSet"), "PDF/A-1 subsets carry a CIDSet");
        let name = String::from_utf8(f.get(b"BaseFont").unwrap().as_name().unwrap().to_vec()).unwrap();
        assert_eq!(name.find('+'), Some(6));
        let (_, cmap, widths) = text_font(&mut doc, &src, &['H', 'i', '\u{4E2D}'], "2u".parse().unwrap()).unwrap();
        assert_eq!(cmap[&'i'], 2);
        assert_eq!(widths.len(), 4);
    }

    #[test]
    fn unusable_font_programs_are_errors() {
        let mut doc = Document::with_version("1.7");
        let level: Level = "2b".parse().unwrap();
        let garbage = FontData { data: std::sync::Arc::new(b"not a font".to_vec()), index: 0, ps_name: "Broken".into() };
        let e = build_type0(&mut doc, &garbage, &[], Enc::IdentityH, &BTreeMap::new(), level).unwrap_err();
        assert!(matches!(e, PdfaError::Unsupported(m) if m.contains("font Broken")));
        assert!(matches!(text_font(&mut doc, &garbage, &['a'], level), Err(PdfaError::Unsupported(_))));

        // A font without glyph outlines (the glyf table renamed away).
        let good = sysfonts::bundled(&sysfonts::Want::default());
        let mut data = good.data.to_vec();
        let n = u16::from_be_bytes([data[4], data[5]]) as usize;
        for i in 0..n {
            let r = 12 + 16 * i;
            if &data[r..r + 4] == b"glyf" {
                data[r..r + 4].copy_from_slice(b"glyX");
            }
        }
        let no_glyf = FontData { data: std::sync::Arc::new(data), index: 0, ps_name: "NoOutlines".into() };
        let e = build_type0(&mut doc, &no_glyf, &[], Enc::IdentityH, &BTreeMap::new(), level).unwrap_err();
        assert!(matches!(e, PdfaError::Unsupported(m) if m.contains("font NoOutlines")));

        // A font with neither '?' nor a space: unknown characters get glyph 0.
        let src_face = Sfnt::parse(&good.data, 0).unwrap();
        let gid_a = ttf_parser::Face::parse(&good.data, 0).unwrap().glyph_index('A').unwrap().0;
        let mut map = BTreeMap::new();
        map.insert('A' as u32, 1u16);
        let only_a = sfnt::subset(&src_face, &[OutGlyph { src: Some(0), advance: None }, OutGlyph { src: Some(gid_a), advance: None }], Some(&sfnt::CmapSpec { platform: 3, encoding: 1, map })).unwrap();
        let tiny = FontData { data: std::sync::Arc::new(only_a.data), index: 0, ps_name: "OnlyA".into() };
        let (_, cids, widths) = text_font(&mut doc, &tiny, &['A', 'z'], level).unwrap();
        assert_eq!(cids.len(), 2);
        assert!(widths.len() >= 2);

        // Glyphs that keep the font's own advance.
        let face = ttf_parser::Face::parse(&good.data, 0).unwrap();
        let ga = face.glyph_index('A').unwrap();
        let own = (face.glyph_hor_advance(ga).unwrap() as f64 * 1000.0 / face.units_per_em() as f64).round() as i64;
        let f = build_type0(&mut doc, &good, &[Glyph { src: Some(ga.0), width: None }], Enc::IdentityH, &BTreeMap::new(), level).unwrap();
        let cid = f.get(b"DescendantFonts").unwrap().as_array().unwrap()[0].as_reference().unwrap();
        // One glyph: its advance is the default width.
        let cd = doc.get_object(cid).unwrap().as_dict().unwrap();
        assert_eq!(cd.get(b"DW").unwrap().as_i64().unwrap(), own);
    }
}
