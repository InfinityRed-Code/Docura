//! CMaps: parsing embedded encoding CMaps and ToUnicode maps, splitting
//! strings into character codes, writing new CMaps.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::content::{self, Operand};

/// A character code: value and byte length.
pub type Code = (u32, u8);

#[derive(Clone, Debug, Default)]
pub struct CMap {
    /// (low, high) byte strings of equal length.
    pub codespace: Vec<(Vec<u8>, Vec<u8>)>,
    /// (low, high, byte length, first CID)
    pub cid_ranges: Vec<(u32, u32, u8, u32)>,
    /// Code → Unicode text (bfchar / bfrange).
    pub bf: BTreeMap<Code, String>,
    pub wmode: u8,
    pub usecmap: Option<Vec<u8>>,
}

fn be(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |a, &x| (a << 8) | x as u32)
}

/// UTF-16BE bytes → String (lossy for unpaired surrogates).
pub fn utf16be(b: &[u8]) -> String {
    let units: Vec<u16> = b.chunks(2).map(|c| if c.len() == 2 { u16::from_be_bytes([c[0], c[1]]) } else { c[0] as u16 }).collect();
    String::from_utf16_lossy(&units)
}

impl CMap {
    pub fn identity() -> CMap {
        CMap {
            codespace: vec![(vec![0, 0], vec![0xFF, 0xFF])],
            cid_ranges: vec![(0, 0xFFFF, 2, 0)],
            ..Default::default()
        }
    }

    /// One-byte codes 0..255 (simple fonts).
    pub fn one_byte() -> CMap {
        CMap { codespace: vec![(vec![0], vec![0xFF])], ..Default::default() }
    }

    pub fn parse(data: &[u8]) -> CMap {
        let mut m = CMap::default();
        content::visit(data, |op| {
            let strs = |args: &[Operand]| -> Vec<Vec<u8>> {
                args.iter().filter_map(|a| if let Operand::Str(s) = a { Some(s.clone()) } else { None }).collect()
            };
            match op.name {
                b"endcodespacerange" => {
                    let s = strs(op.args);
                    for p in s.chunks(2) {
                        if p.len() == 2 && p[0].len() == p[1].len() && !p[0].is_empty() && p[0].len() <= 4 {
                            m.codespace.push((p[0].clone(), p[1].clone()));
                        }
                    }
                }
                b"endcidrange" => {
                    for t in op.args.chunks(3) {
                        if let [Operand::Str(lo), Operand::Str(hi), cid] = t {
                            if let Some(c) = cid.as_f64() {
                                if !lo.is_empty() && lo.len() <= 4 {
                                    m.cid_ranges.push((be(lo), be(hi), lo.len() as u8, c as u32));
                                }
                            }
                        }
                    }
                }
                b"endcidchar" => {
                    for t in op.args.chunks(2) {
                        if let [Operand::Str(c), cid] = t {
                            if let Some(v) = cid.as_f64() {
                                if !c.is_empty() && c.len() <= 4 {
                                    let k = be(c);
                                    m.cid_ranges.push((k, k, c.len() as u8, v as u32));
                                }
                            }
                        }
                    }
                }
                b"endbfchar" => {
                    for t in op.args.chunks(2) {
                        if let [Operand::Str(c), Operand::Str(u)] = t {
                            if !c.is_empty() && c.len() <= 4 {
                                m.bf.insert((be(c), c.len() as u8), utf16be(u));
                            }
                        }
                    }
                }
                b"endbfrange" => {
                    for t in op.args.chunks(3) {
                        if let [Operand::Str(lo), Operand::Str(hi), dst] = t {
                            if lo.is_empty() || lo.len() > 4 || lo.len() != hi.len() {
                                continue;
                            }
                            let (l, h, n) = (be(lo), be(hi), lo.len() as u8);
                            if h < l || h - l > 0xFFFF {
                                continue;
                            }
                            match dst {
                                Operand::Str(u) => {
                                    let base = utf16be(u);
                                    let mut chars: Vec<char> = base.chars().collect();
                                    for code in l..=h {
                                        m.bf.insert((code, n), chars.iter().collect());
                                        if let Some(last) = chars.last_mut() {
                                            *last = char::from_u32(*last as u32 + 1).unwrap_or(*last);
                                        }
                                    }
                                }
                                Operand::Array(a) => {
                                    for (i, item) in a.iter().enumerate() {
                                        if let Operand::Str(u) = item {
                                            let code = l + i as u32;
                                            if code > h {
                                                break;
                                            }
                                            m.bf.insert((code, n), utf16be(u));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                b"def" => {
                    if let [Operand::Name(k), v] = op.args {
                        if k == b"WMode" {
                            m.wmode = v.as_f64().unwrap_or(0.0) as u8;
                        }
                    }
                }
                b"usecmap" => {
                    if let Some(Operand::Name(n)) = op.args.first() {
                        m.usecmap = Some(n.clone());
                    }
                }
                _ => {}
            }
            true
        });
        if m.codespace.is_empty() {
            // Fall back to the code lengths used by the mappings.
            let mut lens: Vec<u8> = m.cid_ranges.iter().map(|r| r.2).chain(m.bf.keys().map(|k| k.1)).collect();
            lens.sort();
            lens.dedup();
            for l in lens {
                m.codespace.push((vec![0; l as usize], vec![0xFF; l as usize]));
            }
        }
        m
    }

    /// Split a string into codes per the codespace ranges.
    pub fn split(&self, s: &[u8]) -> Vec<Code> {
        let mut out = Vec::new();
        let mut i = 0;
        let min_len = self.codespace.iter().map(|r| r.0.len()).min().unwrap_or(1).max(1);
        while i < s.len() {
            let mut matched = None;
            for n in 1..=4usize {
                if i + n > s.len() {
                    break;
                }
                let b = &s[i..i + n];
                let hit = self.codespace.iter().any(|(lo, hi)| {
                    lo.len() == n && b.iter().zip(lo.iter().zip(hi.iter())).all(|(x, (l, h))| x >= l && x <= h)
                });
                if hit {
                    matched = Some(n);
                    break;
                }
            }
            let n = matched.unwrap_or(min_len).min(s.len() - i);
            out.push((be(&s[i..i + n]), n as u8));
            i += n;
        }
        out
    }

    pub fn cid(&self, code: Code) -> Option<u32> {
        // Later ranges override earlier ones (cidchar after cidrange).
        self.cid_ranges
            .iter()
            .rev()
            .find(|r| r.2 == code.1 && code.0 >= r.0 && code.0 <= r.1)
            .map(|r| r.3 + (code.0 - r.0))
    }
}

/// Predefined CMaps by name: Identity, the Unicode ones (code = UTF-16)
/// and code-length knowledge for the others.
pub fn predefined(name: &[u8]) -> Option<CMap> {
    let n = String::from_utf8_lossy(name);
    if n == "Identity-H" || n == "Identity-V" {
        let mut m = CMap::identity();
        m.wmode = n.ends_with("-V") as u8;
        return Some(m);
    }
    let mut m = CMap { wmode: n.ends_with("-V") as u8, ..Default::default() };
    if n.contains("UCS2") {
        m.codespace.push((vec![0, 0], vec![0xFF, 0xFF]));
    } else if n.contains("UTF16") {
        m.codespace.push((vec![0, 0], vec![0xD7, 0xFF]));
        m.codespace.push((vec![0xD8, 0, 0xDC, 0], vec![0xDB, 0xFF, 0xDF, 0xFF]));
        m.codespace.push((vec![0xE0, 0], vec![0xFF, 0xFF]));
    } else if n.contains("RKSJ") {
        m.codespace = vec![
            (vec![0x00], vec![0x80]),
            (vec![0x81, 0x40], vec![0x9F, 0xFC]),
            (vec![0xA0], vec![0xDF]),
            (vec![0xE0, 0x40], vec![0xFC, 0xFC]),
        ];
    } else if n.contains("EUC") || n.contains("GBK") || n.contains("GBpc") || n.contains("KSC") || n.contains("B5") || n.contains("ETen") || n.contains("HKscs") || n.contains("CNS") {
        m.codespace = vec![(vec![0x00], vec![0x80]), (vec![0x81, 0x40], vec![0xFE, 0xFE])];
    } else {
        m.codespace.push((vec![0, 0], vec![0xFF, 0xFF]));
    }
    Some(m)
}

/// Whether codes of this predefined CMap are UTF-16 Unicode values.
pub fn is_unicode_cmap(name: &[u8]) -> bool {
    let n = String::from_utf8_lossy(name);
    n.starts_with("Uni") && (n.contains("UCS2") || n.contains("UTF16"))
}

fn hex(out: &mut String, code: Code) {
    let _ = write!(out, "<{:0width$X}>", code.0, width = code.1 as usize * 2);
}

fn utf16_hex(out: &mut String, s: &str) {
    out.push('<');
    for u in s.encode_utf16() {
        let _ = write!(out, "{u:04X}");
    }
    out.push('>');
}

fn codespace_text(codespace: &[(Vec<u8>, Vec<u8>)]) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "{} begincodespacerange", codespace.len());
    for (lo, hi) in codespace {
        hex(&mut o, (be(lo), lo.len() as u8));
        o.push(' ');
        hex(&mut o, (be(hi), hi.len() as u8));
        o.push('\n');
    }
    o.push_str("endcodespacerange\n");
    o
}

/// A ToUnicode CMap for `map` (code → text).
pub fn write_to_unicode(map: &BTreeMap<Code, String>, codespace: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut o = String::new();
    o.push_str("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n");
    o.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    o.push_str("/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n");
    o.push_str(&codespace_text(codespace));
    let entries: Vec<(&Code, &String)> = map.iter().filter(|(_, v)| !v.is_empty()).collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(o, "{} beginbfchar", chunk.len());
        for (c, t) in chunk {
            hex(&mut o, **c);
            o.push(' ');
            utf16_hex(&mut o, t);
            o.push('\n');
        }
        o.push_str("endbfchar\n");
    }
    o.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    o.into_bytes()
}

/// An encoding CMap (code → CID), Adobe-Identity-0.
pub fn write_encoding(name: &str, map: &BTreeMap<Code, u32>, codespace: &[(Vec<u8>, Vec<u8>)], wmode: u8) -> Vec<u8> {
    let mut o = String::new();
    o.push_str("%!PS-Adobe-3.0 Resource-CMap\n/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n");
    o.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n");
    let _ = writeln!(o, "/CMapName /{name} def\n/CMapType 1 def\n/WMode {wmode} def");
    o.push_str(&codespace_text(codespace));
    let entries: Vec<(&Code, &u32)> = map.iter().collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(o, "{} begincidchar", chunk.len());
        for (c, cid) in chunk {
            hex(&mut o, **c);
            let _ = writeln!(o, " {cid}");
        }
        o.push_str("endcidchar\n");
    }
    o.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    o.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tounicode_and_split() {
        let src = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CMapName /X def /WMode 0 def
2 begincodespacerange <00> <7F> <8000> <FFFF> endcodespacerange
2 beginbfchar <41> <0041> <8001> <D83DDE00> endbfchar
1 beginbfrange <61> <63> <0061> endbfrange
1 beginbfrange <70> <71> [<0066006C> <0078>] endbfrange
1 begincidrange <8000> <80FF> 100 endcidrange
1 begincidchar <8005> 7 endcidchar
endcmap end end";
        let m = CMap::parse(src);
        assert_eq!(m.codespace.len(), 2);
        assert_eq!(m.split(b"A\x80\x01b"), vec![(0x41, 1), (0x8001, 2), (0x62, 1)]);
        assert_eq!(m.bf[&(0x8001, 2)], "\u{1F600}");
        assert_eq!(m.bf[&(0x63, 1)], "c");
        assert_eq!(m.bf[&(0x70, 1)], "fl");
        assert_eq!(m.bf[&(0x71, 1)], "x");
        assert_eq!(m.cid((0x8002, 2)), Some(102));
        assert_eq!(m.cid((0x8005, 2)), Some(7));
        assert_eq!(m.cid((0x41, 1)), None);
    }

    #[test]
    fn writers_roundtrip() {
        let mut map = BTreeMap::new();
        map.insert((0x41, 1), "A".to_string());
        map.insert((0x42, 1), "\u{1F600}".to_string());
        map.insert((0x43, 1), String::new());
        let tu = write_to_unicode(&map, &CMap::one_byte().codespace);
        let back = CMap::parse(&tu);
        assert_eq!(back.bf.len(), 2);
        assert_eq!(back.bf[&(0x42, 1)], "\u{1F600}");
        let mut enc = BTreeMap::new();
        enc.insert((0x20, 1), 3u32);
        let e = write_encoding("Test", &enc, &CMap::one_byte().codespace, 0);
        let eb = CMap::parse(&e);
        assert_eq!(eb.cid((0x20, 1)), Some(3));
        assert_eq!(eb.split(b"\x20\x21"), vec![(0x20, 1), (0x21, 1)]);
    }

    #[test]
    fn predefined_maps() {
        let id = predefined(b"Identity-H").unwrap();
        assert_eq!(id.split(b"\x00\x41\x01"), vec![(0x41, 2), (0x01, 1)]);
        assert_eq!(id.cid((0x41, 2)), Some(0x41));
        assert_eq!(predefined(b"Identity-V").unwrap().wmode, 1);
        let u = predefined(b"UniJIS-UTF16-H").unwrap();
        assert_eq!(u.split(&[0xD8, 0x3D, 0xDE, 0x00, 0x00, 0x41]), vec![(0xD83DDE00, 4), (0x41, 2)]);
        let r = predefined(b"90ms-RKSJ-H").unwrap();
        assert_eq!(r.split(b"A\x82\xa0"), vec![(0x41, 1), (0x82a0, 2)]);
        assert!(is_unicode_cmap(b"UniGB-UCS2-H") && !is_unicode_cmap(b"GBK-EUC-H"));
        assert_eq!(predefined(b"Foo").unwrap().codespace.len(), 1);
        assert_eq!(utf16be(&[0, 0x41, 0]), "A\0");
    }
}
