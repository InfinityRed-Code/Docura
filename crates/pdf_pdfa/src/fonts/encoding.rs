//! Simple-font encodings and glyph names.

use lopdf::{Dictionary, Document, Object};

use super::tables;
use crate::util::{self, deref};

/// Glyph name → Unicode text, per the Adobe Glyph List conventions
/// (AGL entries, uniXXXX sequences, uXXXX[XX], suffixes, ligatures).
pub fn glyph_unicode(name: &str) -> Option<String> {
    if name.is_empty() || name == ".notdef" {
        return None;
    }
    let base = name.split('.').next().unwrap_or(name);
    if base.is_empty() {
        return None;
    }
    if base.contains('_') {
        let parts: Option<Vec<String>> = base.split('_').map(single).collect();
        return parts.map(|p| p.concat()).filter(|s| !s.is_empty());
    }
    single(base)
}

fn single(name: &str) -> Option<String> {
    if let Ok(i) = tables::AGL.binary_search_by(|(k, _)| k.cmp(&name)) {
        return char::from_u32(tables::AGL[i].1).map(String::from);
    }
    if let Some(hex) = name.strip_prefix("uni") {
        if hex.len() >= 4 && hex.len() % 4 == 0 && hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()) {
            let units: Vec<u16> = (0..hex.len()).step_by(4).filter_map(|i| u16::from_str_radix(&hex[i..i + 4], 16).ok()).collect();
            let s = String::from_utf16(&units).ok()?;
            return Some(s).filter(|s| !s.is_empty());
        }
    }
    if let Some(hex) = name.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).map(String::from);
        }
    }
    // Names like "g123" / "cid123" / "a12" carry no meaning.
    None
}

pub fn dingbat_unicode(name: &str) -> Option<String> {
    tables::DINGBATS
        .binary_search_by(|(k, _)| k.cmp(&name))
        .ok()
        .and_then(|i| char::from_u32(tables::DINGBATS[i].1))
        .map(String::from)
}

/// Named base encoding table.
pub fn named(name: &[u8]) -> Option<&'static [&'static str; 256]> {
    Some(match name {
        b"StandardEncoding" => &tables::STANDARD_ENCODING,
        b"WinAnsiEncoding" => &tables::WIN_ANSI_ENCODING,
        b"MacRomanEncoding" => &tables::MAC_ROMAN_ENCODING,
        b"MacExpertEncoding" => &tables::MAC_EXPERT_ENCODING,
        _ => return None,
    })
}

/// The 14 standard fonts, canonical names.
pub const STD14: [&str; 14] = [
    "Courier", "Courier-Bold", "Courier-BoldOblique", "Courier-Oblique", "Helvetica", "Helvetica-Bold",
    "Helvetica-BoldOblique", "Helvetica-Oblique", "Symbol", "Times-Bold", "Times-BoldItalic", "Times-Italic",
    "Times-Roman", "ZapfDingbats",
];

/// Strip a subset tag ("ABCDEF+Name").
pub fn strip_subset(name: &str) -> &str {
    match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()) => rest,
        _ => name,
    }
}

/// Map a BaseFont name to one of the standard 14 (also the usual aliases
/// such as Arial, TimesNewRoman, CourierNew and their style suffixes).
pub fn std14_name(base: &str) -> Option<&'static str> {
    let n = strip_subset(base);
    if let Some(s) = STD14.iter().find(|s| **s == n) {
        return Some(s);
    }
    let lower = n.to_ascii_lowercase().replace([' ', '_'], "");
    let (fam, style) = match lower.split_once([',', '-']) {
        Some((f, s)) => (f.to_string(), s.to_string()),
        None => (lower.clone(), String::new()),
    };
    let fam = fam.trim_end_matches("mt").trim_end_matches("ps").to_string();
    let bold = style.contains("bold") || fam.ends_with("bold");
    let italic = style.contains("italic") || style.contains("oblique") || fam.ends_with("italic");
    let fam = fam.trim_end_matches("bolditalic").trim_end_matches("bold").trim_end_matches("italic");
    let pick = |b: [&'static str; 4]| -> &'static str {
        match (bold, italic) {
            (false, false) => b[0],
            (true, false) => b[1],
            (false, true) => b[2],
            (true, true) => b[3],
        }
    };
    Some(match fam {
        "helvetica" | "arial" => pick(["Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique"]),
        "times" | "timesroman" | "timesnewroman" => pick(["Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic"]),
        "courier" | "couriernew" => pick(["Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique"]),
        "symbol" => "Symbol",
        "zapfdingbats" | "dingbats" => "ZapfDingbats",
        _ => return None,
    })
}

/// Standard-14 AFM width of glyph `name`.
pub fn std14_width(font: &str, name: &str) -> Option<u16> {
    if font.starts_with("Courier") {
        return Some(600);
    }
    let t = tables::std14_widths(font)?;
    t.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| t[i].1)
}

/// code → glyph name for a simple font dictionary. `builtin` is the font
/// program's own encoding (Type1/CFF), used when the dictionary does not
/// name a base encoding.
pub fn simple_encoding(doc: &Document, font: &Dictionary, builtin: Option<&[Option<String>; 256]>) -> [Option<String>; 256] {
    let mut names: [Option<String>; 256] = std::array::from_fn(|_| None);
    let base_name = |o: Option<&Object>| -> Option<Vec<u8>> {
        match o.map(|x| deref(doc, x)) {
            Some(Object::Name(n)) => Some(n.clone()),
            _ => None,
        }
    };
    let enc = font.get(b"Encoding").ok().map(|o| deref(doc, o));
    let base = match enc {
        Some(Object::Name(n)) => Some(n.clone()),
        Some(Object::Dictionary(d)) => base_name(d.get(b"BaseEncoding").ok()),
        _ => None,
    };
    let base_font = util::get_name(doc, font, b"BaseFont").map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
    let std = std14_name(&base_font);
    let symbolic_std: Option<&[&str; 256]> = match std {
        Some("Symbol") => Some(&tables::SYMBOL_ENCODING),
        Some("ZapfDingbats") => Some(&tables::ZAPF_DINGBATS_ENCODING),
        _ => None,
    };
    let fill = |names: &mut [Option<String>; 256], t: &[&str; 256]| {
        for (i, n) in t.iter().enumerate() {
            names[i] = if n.is_empty() { None } else { Some(n.to_string()) };
        }
    };
    match base.as_deref().and_then(named) {
        Some(t) => fill(&mut names, t),
        None => {
            if let Some(b) = builtin {
                names = b.clone();
            } else if let Some(t) = symbolic_std {
                fill(&mut names, t);
            } else {
                fill(&mut names, &tables::STANDARD_ENCODING);
            }
        }
    }
    if let Some(Object::Dictionary(d)) = enc {
        if let Some(diffs) = util::get_array(doc, d, b"Differences") {
            let mut code: i64 = 0;
            for item in diffs {
                match deref(doc, item) {
                    Object::Integer(c) => code = *c,
                    Object::Real(c) => code = *c as i64,
                    Object::Name(n) => {
                        if (0..256).contains(&code) {
                            names[code as usize] = Some(String::from_utf8_lossy(n).to_string());
                        }
                        code += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    names
}

/// MacRoman code of a glyph name (for (1,0) cmap lookups).
pub fn mac_roman_code(name: &str) -> Option<u8> {
    tables::MAC_ROMAN_ENCODING.iter().position(|n| *n == name).map(|p| p as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn glyph_names() {
        assert_eq!(glyph_unicode("A").as_deref(), Some("A"));
        assert_eq!(glyph_unicode("adieresis").as_deref(), Some("ä"));
        assert_eq!(glyph_unicode("uni20AC").as_deref(), Some("€"));
        assert_eq!(glyph_unicode("uni00410042").as_deref(), Some("AB"));
        assert_eq!(glyph_unicode("u1F600").as_deref(), Some("\u{1F600}"));
        assert_eq!(glyph_unicode("f_i").as_deref(), Some("fi"));
        assert_eq!(glyph_unicode("a.sc").as_deref(), Some("a"));
        assert_eq!(glyph_unicode("g123"), None);
        assert_eq!(glyph_unicode(".notdef"), None);
        assert_eq!(glyph_unicode("uni00e4"), None);
        assert_eq!(dingbat_unicode("a20").as_deref(), Some("\u{2714}"));
    }

    #[test]
    fn std14() {
        assert_eq!(std14_name("Helvetica"), Some("Helvetica"));
        assert_eq!(std14_name("ABCDEF+Arial,Bold"), Some("Helvetica-Bold"));
        assert_eq!(std14_name("Arial-BoldItalicMT"), Some("Helvetica-BoldOblique"));
        assert_eq!(std14_name("ArialMT"), Some("Helvetica"));
        assert_eq!(std14_name("TimesNewRomanPS-BoldMT"), Some("Times-Bold"));
        assert_eq!(std14_name("Times New Roman,Italic"), Some("Times-Italic"));
        assert_eq!(std14_name("CourierNew"), Some("Courier"));
        assert_eq!(std14_name("Calibri"), None);
        assert_eq!(std14_width("Helvetica", "A"), Some(667));
        assert_eq!(std14_width("Courier-Bold", "A"), Some(600));
        assert_eq!(std14_width("Helvetica", "nonexistent"), None);
        assert_eq!(strip_subset("ABCDEF+Foo"), "Foo");
        assert_eq!(strip_subset("Abcdef+Foo"), "Abcdef+Foo");
        assert_eq!(mac_roman_code("A"), Some(65));
    }

    #[test]
    fn encodings_with_differences() {
        let doc = Document::with_version("1.7");
        let f = dictionary! {
            "BaseFont" => "Helvetica",
            "Encoding" => dictionary! { "BaseEncoding" => "WinAnsiEncoding", "Differences" => vec![65.into(), Object::Name(b"Euro".to_vec()), Object::Name(b"B".to_vec())] },
        };
        let e = simple_encoding(&doc, &f, None);
        assert_eq!(e[65].as_deref(), Some("Euro"));
        assert_eq!(e[66].as_deref(), Some("B"));
        assert_eq!(e[0xE4].as_deref(), Some("adieresis"));
        let sym = dictionary! { "BaseFont" => "Symbol" };
        assert_eq!(simple_encoding(&doc, &sym, None)[0x61].as_deref(), Some("alpha"));
        let std = dictionary! { "BaseFont" => "Foo" };
        assert_eq!(simple_encoding(&doc, &std, None)[0x27].as_deref(), Some("quoteright"));
        assert!(named(b"Nope").is_none());
    }
}
