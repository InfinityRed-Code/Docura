//! Embedding a TrueType font as a composite font (Type0 over CIDFontType2,
//! Identity-H encoding) and encoding text for it.
//!
//! The whole font program is embedded (no subsetting), so one font object
//! can serve every later annotation that uses the same face. Strings are
//! two-byte glyph ids; /W carries every glyph's advance and /ToUnicode maps
//! the glyphs back to text, so copying and searching keep working.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use lopdf::{dictionary, Document, Object, ObjectId, Stream, StringFormat};
use ttf_parser::{name_id, Face, GlyphId};

use crate::{MutateError, MutateResult};

pub(crate) fn parse(data: &[u8]) -> MutateResult<Face<'_>> {
    Face::parse(data, 0).map_err(|e| MutateError::Parse(format!("font: {e}")))
}

/// The face's PostScript name, the /BaseFont of the embedded font.
pub(crate) fn postscript_name(face: &Face) -> String {
    face.names()
        .into_iter()
        .filter(|n| n.name_id == name_id::POST_SCRIPT_NAME)
        .find_map(|n| n.to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "EmbeddedFont".into())
        .chars()
        .filter(|c| c.is_ascii_graphic() && !"()<>[]{}/%#".contains(*c))
        .collect()
}

/// Glyph for `c`, falling back to "?" (then .notdef) for characters the
/// font does not have.
pub(crate) fn glyph(face: &Face, c: char) -> GlyphId {
    let c = if c == '\t' { ' ' } else { c };
    face.glyph_index(c)
        .or_else(|| face.glyph_index('?'))
        .unwrap_or(GlyphId(0))
}

/// `line` as a PDF hex string of glyph ids, ready for `Tj`.
pub(crate) fn encode(face: &Face, line: &str) -> Vec<u8> {
    let mut out = String::with_capacity(line.len() * 4 + 2);
    out.push('<');
    for c in line.chars() {
        let _ = write!(out, "{:04X}", glyph(face, c).0);
    }
    out.push('>');
    out.into_bytes()
}

/// Glyph id → the first (lowest) Unicode code point mapped to it.
fn glyph_to_unicode(face: &Face) -> BTreeMap<u16, u32> {
    let mut map = BTreeMap::new();
    let Some(cmap) = face.tables().cmap else {
        return map;
    };
    for sub in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
        sub.codepoints(|cp| {
            if let Some(g) = sub.glyph_index(cp) {
                if g.0 != 0 {
                    let e = map.entry(g.0).or_insert(cp);
                    *e = (*e).min(cp);
                }
            }
        });
    }
    map
}

fn to_unicode_cmap(face: &Face) -> Vec<u8> {
    let map: Vec<(u16, u32)> = glyph_to_unicode(face).into_iter().collect();
    let mut s = String::with_capacity(map.len() * 16 + 400);
    s.push_str(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in map.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for &(g, cp) in chunk {
            let mut units = [0u16; 2];
            let utf16 = char::from_u32(cp)
                .map(|c| c.encode_utf16(&mut units).len())
                .unwrap_or(0);
            let _ = write!(s, "<{g:04X}> <");
            for u in &units[..utf16.max(1)] {
                let _ = write!(s, "{u:04X}");
            }
            s.push_str(">\n");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

fn compressed(dict: lopdf::Dictionary, content: Vec<u8>) -> Stream {
    let mut stream = Stream::new(dict, content);
    let _ = stream.compress();
    stream
}

/// Add the font objects for `data` and return the Type0 font's id.
pub(crate) fn add_type0_font(
    doc: &mut Document,
    data: &[u8],
    serif: bool,
) -> MutateResult<ObjectId> {
    let face = parse(data)?;
    let upem = f32::from(face.units_per_em().max(1));
    let k = |v: f32| (v * 1000.0 / upem).round() as i64;
    let ps_name = postscript_name(&face);

    let font_file = doc.add_object(compressed(
        dictionary! { "Length1" => data.len() as i64 },
        data.to_vec(),
    ));

    // PDF 1.7 §9.8.2 flags: FixedPitch 1, Serif 2, Nonsymbolic 32, Italic 64.
    let mut flags = 32;
    if face.is_monospaced() {
        flags |= 1;
    }
    if serif {
        flags |= 2;
    }
    if face.is_italic() {
        flags |= 64;
    }
    let bbox = face.global_bounding_box();
    let ascent = k(f32::from(face.ascender()));
    let descriptor = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(ps_name.clone().into_bytes()),
        "Flags" => flags,
        "FontBBox" => vec![
            k(f32::from(bbox.x_min)).into(),
            k(f32::from(bbox.y_min)).into(),
            k(f32::from(bbox.x_max)).into(),
            k(f32::from(bbox.y_max)).into(),
        ],
        "ItalicAngle" => face.italic_angle(),
        "Ascent" => ascent,
        "Descent" => k(f32::from(face.descender())),
        "CapHeight" => face.capital_height().map(|h| k(f32::from(h))).unwrap_or(ascent),
        "StemV" => if face.weight().to_number() >= 600 { 120 } else { 80 },
        "FontFile2" => font_file,
    });

    // Every glyph's advance width, in one run starting at glyph 0.
    let widths: Vec<Object> = (0..face.number_of_glyphs())
        .map(|g| {
            Object::Integer(k(f32::from(
                face.glyph_hor_advance(GlyphId(g)).unwrap_or(0),
            )))
        })
        .collect();
    let cid_font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(ps_name.clone().into_bytes()),
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::String(b"Adobe".to_vec(), StringFormat::Literal),
            "Ordering" => Object::String(b"Identity".to_vec(), StringFormat::Literal),
            "Supplement" => 0,
        },
        "FontDescriptor" => descriptor,
        "W" => vec![Object::Integer(0), Object::Array(widths)],
        "CIDToGIDMap" => "Identity",
    });

    let to_unicode = doc.add_object(compressed(lopdf::Dictionary::new(), to_unicode_cmap(&face)));
    Ok(doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(ps_name.into_bytes()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(cid_font)],
        "ToUnicode" => to_unicode,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_glyph_ids_and_maps_them_back() {
        let data = pdf_fonts::family("Roboto").unwrap().face(false, false);
        let face = parse(data).unwrap();
        let hex = String::from_utf8(encode(&face, "Aä€")).unwrap();
        assert_eq!(hex.len(), 2 + 3 * 4, "{hex}");
        let a = face.glyph_index('A').unwrap().0;
        assert!(hex.starts_with(&format!("<{a:04X}")), "{hex}");
        let cmap = String::from_utf8(to_unicode_cmap(&face)).unwrap();
        assert!(cmap.contains(&format!("<{a:04X}> <0041>")));
        let euro = face.glyph_index('€').unwrap().0;
        assert!(cmap.contains(&format!("<{euro:04X}> <20AC>")));
        // Unknown characters fall back to "?".
        let q = face.glyph_index('?').unwrap().0;
        assert_eq!(
            String::from_utf8(encode(&face, "\u{10FFFD}")).unwrap(),
            format!("<{q:04X}>")
        );
    }
}
