//! Minimal Type 1 font program reader: built-in encoding, glyph names and
//! advance widths (from hsbw/sbw), enough for the PDF/A glyph and width
//! checks.

use std::collections::BTreeMap;

pub struct Type1 {
    pub encoding: Option<[Option<String>; 256]>,
    /// Glyph name → advance width in glyph space (usually 1/1000 em).
    pub widths: BTreeMap<String, f64>,
    /// FontMatrix x scale relative to 0.001.
    pub scale: f64,
}

fn strip_pfb(data: &[u8]) -> Vec<u8> {
    if data.first() != Some(&0x80) {
        return data.to_vec();
    }
    let mut out = Vec::new();
    let mut p = 0;
    while p + 6 <= data.len() && data[p] == 0x80 {
        let kind = data[p + 1];
        if kind == 3 {
            break;
        }
        let len = u32::from_le_bytes([data[p + 2], data[p + 3], data[p + 4], data[p + 5]]) as usize;
        let s = p + 6;
        let e = (s + len).min(data.len());
        out.extend_from_slice(&data[s..e]);
        p = e;
    }
    out
}

fn decrypt(data: &[u8], mut r: u16, skip: usize) -> Vec<u8> {
    const C1: u16 = 52845;
    const C2: u16 = 22719;
    let mut out = Vec::with_capacity(data.len());
    for &c in data {
        let p = c ^ (r >> 8) as u8;
        r = (c as u16).wrapping_add(r).wrapping_mul(C1).wrapping_add(C2);
        out.push(p);
    }
    if out.len() > skip {
        out.drain(..skip);
    } else {
        out.clear();
    }
    out
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// Parse a FontFile stream. `len1` is /Length1 (clear-text part).
pub fn parse(raw: &[u8], len1: Option<usize>) -> Option<Type1> {
    let data = strip_pfb(raw);
    let eexec = find(&data, b"eexec")?;
    let clear_end = len1.filter(|l| *l <= data.len() && *l > eexec).unwrap_or(eexec + 5);
    let clear = &data[..clear_end];

    // FontMatrix.
    let mut scale = 1.0;
    if let Some(p) = find(clear, b"/FontMatrix") {
        let rest = &clear[p + 11..clear.len().min(p + 120)];
        if let Some(s) = rest.iter().position(|&c| c == b'[' || c == b'{') {
            let nums: Vec<f64> = String::from_utf8_lossy(&rest[s + 1..])
                .split(|c: char| c.is_whitespace() || c == ']' || c == '}')
                .filter_map(|t| t.parse::<f64>().ok())
                .take(1)
                .collect();
            if let Some(a) = nums.first() {
                if *a != 0.0 {
                    scale = a / 0.001;
                }
            }
        }
    }

    // Encoding.
    let encoding = if find(clear, b"/Encoding StandardEncoding").is_some() {
        None
    } else if let Some(p) = find(clear, b"/Encoding") {
        let mut enc: [Option<String>; 256] = std::array::from_fn(|_| None);
        let text = String::from_utf8_lossy(&clear[p..]);
        let mut toks = text.split_whitespace().peekable();
        let mut any = false;
        while let Some(t) = toks.next() {
            if (t == "readonly" || t == "def") && any {
                break;
            }
            if t == "dup" {
                let code = toks.next().and_then(|c| c.parse::<usize>().ok());
                let name = toks.next().and_then(|n| n.strip_prefix('/'));
                if let (Some(c), Some(n)) = (code, name) {
                    if c < 256 {
                        enc[c] = Some(n.to_string());
                        any = true;
                    }
                }
            }
            if t == "eexec" {
                break;
            }
        }
        if any {
            Some(enc)
        } else {
            None
        }
    } else {
        None
    };

    // Encrypted part (binary or hex).
    let mut start = eexec + 5;
    while start < data.len() && matches!(data[start], b' ' | b'\r' | b'\n' | b'\t') {
        start += 1;
    }
    let enc_part = &data[start..];
    let bin: Vec<u8> = if enc_part.len() >= 4 && enc_part[..4].iter().all(|&c| is_hex(c)) {
        let hex: Vec<u8> = enc_part.iter().copied().filter(|c| is_hex(*c)).collect();
        hex.chunks(2)
            .filter(|c| c.len() == 2)
            .filter_map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
            .collect()
    } else {
        enc_part.to_vec()
    };
    let private = decrypt(&bin, 55665, 4);
    let len_iv = find(&private, b"/lenIV")
        .and_then(|p| String::from_utf8_lossy(&private[p + 6..private.len().min(p + 16)]).split_whitespace().next().and_then(|t| t.parse::<i64>().ok()))
        .unwrap_or(4);

    let mut widths = BTreeMap::new();
    let cs = find(&private, b"/CharStrings")?;
    let mut p = cs + 12;
    // Each glyph: /name <len> RD|-| <binary> ND|-|
    while p < private.len() {
        // Next '/'.
        let Some(off) = private[p..].iter().position(|&c| c == b'/') else { break };
        p += off + 1;
        let ns = p;
        while p < private.len() && !matches!(private[p], b' ' | b'\r' | b'\n' | b'\t') {
            p += 1;
        }
        let name = String::from_utf8_lossy(&private[ns..p]).to_string();
        // Length.
        while p < private.len() && private[p] == b' ' {
            p += 1;
        }
        let ls = p;
        while p < private.len() && private[p].is_ascii_digit() {
            p += 1;
        }
        let Ok(len) = String::from_utf8_lossy(&private[ls..p]).parse::<usize>() else {
            if name == "end" || name.is_empty() {
                break;
            }
            continue;
        };
        // RD token then one space.
        while p < private.len() && private[p] == b' ' {
            p += 1;
        }
        while p < private.len() && private[p] != b' ' {
            p += 1;
        }
        p += 1;
        if p + len > private.len() {
            break;
        }
        let cstr = &private[p..p + len];
        p += len;
        let plain = if len_iv >= 0 { decrypt(cstr, 4330, len_iv as usize) } else { cstr.to_vec() };
        if let Some(w) = charstring_width(&plain) {
            widths.insert(name, w);
        }
    }
    Some(Type1 { encoding, widths, scale })
}

/// Width from the first hsbw / sbw command of a Type 1 charstring.
fn charstring_width(cs: &[u8]) -> Option<f64> {
    let mut stack: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let v = cs[i];
        match v {
            32..=246 => {
                stack.push(v as f64 - 139.0);
                i += 1;
            }
            247..=250 => {
                let w = cs.get(i + 1).copied()? as f64;
                stack.push((v as f64 - 247.0) * 256.0 + w + 108.0);
                i += 2;
            }
            251..=254 => {
                let w = cs.get(i + 1).copied()? as f64;
                stack.push(-(v as f64 - 251.0) * 256.0 - w - 108.0);
                i += 2;
            }
            255 => {
                let b = cs.get(i + 1..i + 5)?;
                stack.push(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64);
                i += 5;
            }
            13 => return stack.get(1).copied(), // hsbw: sbx wx
            12 => {
                let sub = *cs.get(i + 1)?;
                if sub == 7 {
                    return stack.get(2).copied(); // sbw: sbx sby wx wy
                }
                if sub == 12 {
                    // div
                    let b = stack.pop()?;
                    let a = stack.pop()?;
                    stack.push(if b != 0.0 { a / b } else { 0.0 });
                    i += 2;
                    continue;
                }
                return None;
            }
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encrypt(plain: &[u8], mut r: u16) -> Vec<u8> {
        let mut out = Vec::new();
        for &p in plain {
            let c = p ^ (r >> 8) as u8;
            r = (c as u16).wrapping_add(r).wrapping_mul(52845).wrapping_add(22719);
            out.push(c);
        }
        out
    }

    /// A tiny synthetic Type 1 font with two glyphs.
    pub fn sample() -> Vec<u8> {
        let mut clear = b"%!PS-AdobeFont-1.0: Test\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\ndup 65 /A put\ndup 66 /B put\nreadonly def\ncurrentfile eexec\n".to_vec();
        let mut private = b"\0\0\0\0dup /Private 8 dict dup begin /lenIV 4 def\n2 index /CharStrings 2 dict dup begin\n".to_vec();
        for (name, w) in [("A", 667i32), ("B", 300)] {
            // hsbw with sbx=0, wx=w: numbers then 13, endchar 14.
            let mut cs = vec![0u8; 4];
            cs.push(139); // 0
            cs.push(255);
            cs.extend_from_slice(&w.to_be_bytes());
            cs.push(13);
            cs.push(14);
            let enc = encrypt(&cs, 4330);
            private.extend_from_slice(format!("/{name} {} RD ", enc.len()).as_bytes());
            private.extend_from_slice(&enc);
            private.extend_from_slice(b" ND\n");
        }
        private.extend_from_slice(b"end\nend\n");
        clear.extend_from_slice(&encrypt(&private, 55665));
        clear
    }

    #[test]
    fn parses_synthetic_font() {
        let f = parse(&sample(), None).unwrap();
        assert_eq!(f.widths.get("A"), Some(&667.0));
        assert_eq!(f.widths.get("B"), Some(&300.0));
        assert_eq!(f.encoding.as_ref().unwrap()[65].as_deref(), Some("A"));
        assert_eq!(f.scale, 1.0);
        assert!(parse(b"no font here", None).is_none());
    }

    #[test]
    fn charstring_numbers() {
        assert_eq!(charstring_width(&[139, 247, 0, 13]), Some(108.0));
        assert_eq!(charstring_width(&[139, 139, 251, 0, 139, 12, 7]), Some(-108.0));
        assert_eq!(charstring_width(&[139, 200, 139, 12, 12, 13]), Some(0.0));
        assert_eq!(charstring_width(&[1]), None);
        let pfb = [0x80, 1, 3, 0, 0, 0, b'a', b'b', b'c', 0x80, 3];
        assert_eq!(strip_pfb(&pfb), b"abc");
    }
}
