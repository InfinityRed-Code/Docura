//! Content stream lexer.
//!
//! Streams are visited operator by operator without building a full
//! operation list, so huge drawings (construction plans with millions of
//! path operators) stay cheap in memory. Every operation carries the byte
//! span it occupies, which lets rewriters copy untouched operations
//! verbatim instead of re-serialising (and re-rounding) their operands.

use std::io::Write;

/// One operand.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Int(i64),
    Real(f64),
    Bool(bool),
    Null,
    /// Name without the leading slash, `#xx` escapes decoded.
    Name(Vec<u8>),
    /// Literal or hex string, decoded bytes.
    Str(Vec<u8>),
    Array(Vec<Operand>),
    Dict(Vec<(Vec<u8>, Operand)>),
}

impl Operand {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Operand::Int(i) => Some(*i as f64),
            Operand::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&[u8]> {
        match self {
            Operand::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn dict_get(&self, key: &[u8]) -> Option<&Operand> {
        match self {
            Operand::Dict(d) => d.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Inline image found between `BI` and `EI`.
#[derive(Clone, Debug)]
pub struct InlineImage {
    pub dict: Vec<(Vec<u8>, Operand)>,
    /// Byte range of the image data (after `ID` + one white-space byte).
    pub data: (usize, usize),
}

/// One operation as seen by a visitor.
pub struct Op<'a> {
    pub name: &'a [u8],
    pub args: &'a [Operand],
    /// Byte range from the first operand to the end of the operator.
    pub start: usize,
    pub end: usize,
    pub inline: Option<&'a InlineImage>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b'\0' | b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
}

fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn is_regular(b: u8) -> bool {
    !is_ws(b) && !is_delim(b)
}

struct Lexer<'a> {
    d: &'a [u8],
    p: usize,
}

enum Tok {
    Operand(Operand),
    ArrayStart,
    ArrayEnd,
    DictStart,
    DictEnd,
    Keyword(usize, usize),
    Eof,
}

impl<'a> Lexer<'a> {
    fn skip_ws(&mut self) {
        while self.p < self.d.len() {
            let b = self.d[self.p];
            if is_ws(b) {
                self.p += 1;
            } else if b == b'%' {
                while self.p < self.d.len() && self.d[self.p] != b'\n' && self.d[self.p] != b'\r' {
                    self.p += 1;
                }
            } else {
                break;
            }
        }
    }

    fn next(&mut self) -> Tok {
        self.skip_ws();
        let d = self.d;
        if self.p >= d.len() {
            return Tok::Eof;
        }
        let b = d[self.p];
        match b {
            b'/' => {
                self.p += 1;
                let s = self.p;
                while self.p < d.len() && is_regular(d[self.p]) {
                    self.p += 1;
                }
                Tok::Operand(Operand::Name(decode_name(&d[s..self.p])))
            }
            b'(' => Tok::Operand(Operand::Str(self.literal_string())),
            b'<' => {
                if d.get(self.p + 1) == Some(&b'<') {
                    self.p += 2;
                    Tok::DictStart
                } else {
                    Tok::Operand(Operand::Str(self.hex_string()))
                }
            }
            b'>' => {
                self.p += 1;
                if d.get(self.p) == Some(&b'>') {
                    self.p += 1;
                }
                Tok::DictEnd
            }
            b'[' => {
                self.p += 1;
                Tok::ArrayStart
            }
            b']' => {
                self.p += 1;
                Tok::ArrayEnd
            }
            b'{' | b'}' | b')' => {
                // Not valid in content streams; skip.
                self.p += 1;
                self.next()
            }
            _ => {
                let s = self.p;
                while self.p < d.len() && is_regular(d[self.p]) {
                    self.p += 1;
                }
                let word = &d[s..self.p];
                if let Some(n) = parse_number(word) {
                    return Tok::Operand(n);
                }
                match word {
                    b"true" => Tok::Operand(Operand::Bool(true)),
                    b"false" => Tok::Operand(Operand::Bool(false)),
                    b"null" => Tok::Operand(Operand::Null),
                    _ => Tok::Keyword(s, self.p),
                }
            }
        }
    }

    fn literal_string(&mut self) -> Vec<u8> {
        let d = self.d;
        self.p += 1; // (
        let mut out = Vec::new();
        let mut depth = 1;
        while self.p < d.len() {
            let b = d[self.p];
            self.p += 1;
            match b {
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b);
                }
                b'\\' => {
                    let Some(&e) = d.get(self.p) else { break };
                    self.p += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if d.get(self.p) == Some(&b'\n') {
                                self.p += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match d.get(self.p) {
                                    Some(&c @ b'0'..=b'7') => {
                                        v = v * 8 + (c - b'0') as u32;
                                        self.p += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push((v & 0xFF) as u8);
                        }
                        other => out.push(other),
                    }
                }
                _ => out.push(b),
            }
        }
        out
    }

    fn hex_string(&mut self) -> Vec<u8> {
        let d = self.d;
        self.p += 1; // <
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while self.p < d.len() {
            let b = d[self.p];
            self.p += 1;
            if b == b'>' {
                break;
            }
            let v = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => continue,
            };
            match hi.take() {
                None => hi = Some(v),
                Some(h) => out.push(h << 4 | v),
            }
        }
        if let Some(h) = hi {
            out.push(h << 4);
        }
        out
    }

    /// Parse one complete operand (array/dict aware). `first` is an already
    /// read token.
    fn operand_from(&mut self, first: Tok) -> Option<Operand> {
        match first {
            Tok::Operand(o) => Some(o),
            Tok::ArrayStart => {
                let mut v = Vec::new();
                loop {
                    match self.next() {
                        Tok::ArrayEnd | Tok::Eof => break,
                        Tok::Keyword(..) | Tok::DictEnd => continue,
                        t => {
                            if let Some(o) = self.operand_from(t) {
                                v.push(o);
                            }
                        }
                    }
                }
                Some(Operand::Array(v))
            }
            Tok::DictStart => {
                let mut v = Vec::new();
                loop {
                    match self.next() {
                        Tok::DictEnd | Tok::Eof => break,
                        Tok::Operand(Operand::Name(k)) => {
                            let t = self.next();
                            match t {
                                Tok::DictEnd | Tok::Eof => break,
                                t => {
                                    if let Some(o) = self.operand_from(t) {
                                        v.push((k, o));
                                    }
                                }
                            }
                        }
                        _ => continue,
                    }
                }
                Some(Operand::Dict(v))
            }
            _ => None,
        }
    }
}

fn decode_name(raw: &[u8]) -> Vec<u8> {
    if !raw.contains(&b'#') {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'#' && i + 2 < raw.len() {
            let h = std::str::from_utf8(&raw[i + 1..i + 3]).ok();
            if let Some(v) = h.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(raw[i]);
        i += 1;
    }
    out
}

fn parse_number(w: &[u8]) -> Option<Operand> {
    let first = *w.first()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    if !w.iter().all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.')) {
        return None;
    }
    let s = std::str::from_utf8(w).ok()?;
    if !s.contains('.') {
        if let Ok(i) = s.parse::<i64>() {
            return Some(Operand::Int(i));
        }
    }
    if let Ok(r) = s.parse::<f64>() {
        return Some(Operand::Real(r));
    }
    // Malformed numbers such as "--5" or "4.-2": salvage what parses.
    let cleaned: String = {
        let mut out = String::new();
        let mut seen_digit = false;
        let mut seen_dot = false;
        for (i, c) in s.chars().enumerate() {
            match c {
                '-' | '+' if !seen_digit && !seen_dot => {
                    if i == 0 || out.is_empty() {
                        out.push(c);
                    }
                }
                '.' if !seen_dot => {
                    seen_dot = true;
                    out.push(c);
                }
                '0'..='9' => {
                    seen_digit = true;
                    out.push(c);
                }
                _ => break,
            }
        }
        out
    };
    let cleaned = cleaned.trim_start_matches('+');
    Some(Operand::Real(cleaned.parse::<f64>().unwrap_or(0.0)))
}

/// Find where inline image data ends. Returns (data_end, after_EI).
fn inline_image_end(d: &[u8], start: usize, dict: &[(Vec<u8>, Operand)]) -> (usize, usize) {
    let get = |k1: &[u8], k2: &[u8]| {
        dict.iter()
            .find(|(k, _)| k == k1 || k == k2)
            .map(|(_, v)| v)
    };
    // PDF 2.0 /Length (or /L).
    if let Some(len) = get(b"L", b"Length").and_then(|v| v.as_f64()) {
        let end = (start + len.max(0.0) as usize).min(d.len());
        if let Some(after) = find_ei(d, end, true) {
            return (end, after);
        }
    }
    let filtered = get(b"F", b"Filter").is_some();
    if !filtered {
        let w = get(b"W", b"Width").and_then(|v| v.as_f64());
        let h = get(b"H", b"Height").and_then(|v| v.as_f64());
        let mask = matches!(get(b"IM", b"ImageMask"), Some(Operand::Bool(true)));
        let bpc = if mask {
            Some(1.0)
        } else {
            get(b"BPC", b"BitsPerComponent").and_then(|v| v.as_f64())
        };
        let comps = if mask {
            Some(1.0)
        } else {
            match get(b"CS", b"ColorSpace") {
                Some(Operand::Name(n)) => match n.as_slice() {
                    b"G" | b"DeviceGray" | b"CalGray" => Some(1.0),
                    b"RGB" | b"DeviceRGB" | b"CalRGB" => Some(3.0),
                    b"CMYK" | b"DeviceCMYK" => Some(4.0),
                    b"I" | b"Indexed" => Some(1.0),
                    _ => None,
                },
                Some(Operand::Array(a)) => match a.first().and_then(|o| o.as_name()) {
                    Some(b"I") | Some(b"Indexed") => Some(1.0),
                    _ => None,
                },
                None => None,
                _ => None,
            }
        };
        if let (Some(w), Some(h), Some(bpc), Some(c)) = (w, h, bpc, comps) {
            let row = ((w * c * bpc) / 8.0).ceil() as usize;
            let end = (start + row * h.max(0.0) as usize).min(d.len());
            if let Some(after) = find_ei(d, end, true) {
                return (end, after);
            }
        }
    }
    // Heuristic search: "EI" between white space, followed by something
    // that looks like content again.
    let mut i = start;
    while i + 2 <= d.len() {
        if d[i] == b'E'
            && d.get(i + 1) == Some(&b'I')
            && (i == start || is_ws(d[i - 1]))
            && d.get(i + 2).is_none_or(|&c| is_ws(c) || is_delim(c))
        {
            let tail = &d[i + 2..d.len().min(i + 40)];
            let plausible = tail
                .iter()
                .all(|&c| c == b'\n' || c == b'\r' || c == b'\t' || (32..127).contains(&c));
            if plausible {
                let mut end = i;
                if end > start && is_ws(d[end - 1]) {
                    end -= 1;
                }
                return (end, i + 2);
            }
        }
        i += 1;
    }
    (d.len(), d.len())
}

/// Look for `EI` right after `pos` (allowing white space). Returns the
/// position after `EI`.
fn find_ei(d: &[u8], pos: usize, lenient: bool) -> Option<usize> {
    let mut i = pos;
    while i < d.len() && is_ws(d[i]) {
        i += 1;
    }
    if d.get(i) == Some(&b'E') && d.get(i + 1) == Some(&b'I') {
        return Some(i + 2);
    }
    if lenient {
        // Some writers pad the data; search a little further.
        let lim = d.len().min(pos + 16);
        let mut j = pos;
        while j + 1 < lim {
            if d[j] == b'E' && d[j + 1] == b'I' && (j == 0 || is_ws(d[j - 1])) {
                return Some(j + 2);
            }
            j += 1;
        }
    }
    None
}

/// Visit every operation of a content stream. The visitor returns `false`
/// to stop early.
pub fn visit(data: &[u8], mut f: impl FnMut(&Op) -> bool) {
    let mut lx = Lexer { d: data, p: 0 };
    let mut args: Vec<Operand> = Vec::new();
    let mut op_start: Option<usize> = None;
    loop {
        lx.skip_ws();
        let tok_start = lx.p;
        let t = lx.next();
        match t {
            Tok::Eof => break,
            Tok::Keyword(s, e) => {
                let name = &data[s..e];
                let start = op_start.take().unwrap_or(s);
                if name == b"BI" {
                    // Inline image dictionary up to ID.
                    let mut dict = Vec::new();
                    loop {
                        match lx.next() {
                            Tok::Eof => break,
                            Tok::Keyword(ks, ke) if &data[ks..ke] == b"ID" => break,
                            Tok::Operand(Operand::Name(k)) => {
                                let vt = lx.next();
                                if let Tok::Keyword(ks, ke) = vt {
                                    if &data[ks..ke] == b"ID" {
                                        break;
                                    }
                                    continue;
                                }
                                if let Some(v) = lx.operand_from(vt) {
                                    dict.push((k, v));
                                }
                            }
                            _ => continue,
                        }
                    }
                    // Exactly one white-space byte after ID.
                    let data_start = (lx.p + 1).min(data.len());
                    let (data_end, after) = inline_image_end(data, data_start, &dict);
                    lx.p = after;
                    let img = InlineImage { dict, data: (data_start, data_end) };
                    let op = Op { name: b"BI", args: &[], start, end: after, inline: Some(&img) };
                    if !f(&op) {
                        return;
                    }
                    args.clear();
                    continue;
                }
                let op = Op { name, args: &args, start, end: e, inline: None };
                if !f(&op) {
                    return;
                }
                args.clear();
            }
            Tok::ArrayEnd | Tok::DictEnd => continue,
            t => {
                if op_start.is_none() {
                    op_start = Some(tok_start);
                }
                if let Some(o) = lx.operand_from(t) {
                    args.push(o);
                }
            }
        }
    }
}

/// Collect all operations (tests and small streams).
pub fn ops(data: &[u8]) -> Vec<(Vec<u8>, Vec<Operand>)> {
    let mut v = Vec::new();
    visit(data, |op| {
        v.push((op.name.to_vec(), op.args.to_vec()));
        true
    });
    v
}

// ---------- serialisation ----------

pub fn write_name(out: &mut Vec<u8>, name: &[u8]) {
    out.push(b'/');
    for &b in name {
        if b" \t\n\r\x0C()<>[]{}/%#".contains(&b) || !(33..=126).contains(&b) {
            let _ = write!(out, "#{b:02X}");
        } else {
            out.push(b);
        }
    }
}

pub fn write_real(out: &mut Vec<u8>, v: f64) {
    if !v.is_finite() {
        out.push(b'0');
        return;
    }
    if v == v.trunc() && v.abs() < 1e15 {
        let _ = write!(out, "{}", v as i64);
        return;
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    out.extend_from_slice(if s == "-0" { b"0" } else { s.as_bytes() });
}

pub fn write_string(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'<');
    for b in s {
        let _ = write!(out, "{b:02X}");
    }
    out.push(b'>');
}

pub fn write_operand(out: &mut Vec<u8>, o: &Operand) {
    match o {
        Operand::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Operand::Real(r) => write_real(out, *r),
        Operand::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Operand::Null => out.extend_from_slice(b"null"),
        Operand::Name(n) => write_name(out, n),
        Operand::Str(s) => write_string(out, s),
        Operand::Array(a) => {
            out.push(b'[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_operand(out, x);
            }
            out.push(b']');
        }
        Operand::Dict(d) => {
            out.extend_from_slice(b"<<");
            for (k, v) in d {
                write_name(out, k);
                out.push(b' ');
                write_operand(out, v);
            }
            out.extend_from_slice(b">>");
        }
    }
}

/// Write one operation (operands + operator) followed by a newline.
pub fn write_op(out: &mut Vec<u8>, name: &[u8], args: &[Operand]) {
    for a in args {
        write_operand(out, a);
        out.push(b' ');
    }
    out.extend_from_slice(name);
    out.push(b'\n');
}

/// All operators defined in ISO 32000-2 (PDF/A forbids anything else).
pub fn is_known_operator(op: &[u8]) -> bool {
    matches!(
        op,
        b"b" | b"B" | b"b*" | b"B*" | b"BDC" | b"BI" | b"BMC" | b"BT" | b"BX" | b"c" | b"cm"
            | b"CS" | b"cs" | b"d" | b"d0" | b"d1" | b"Do" | b"DP" | b"EI" | b"EMC" | b"ET"
            | b"EX" | b"f" | b"F" | b"f*" | b"G" | b"g" | b"gs" | b"h" | b"i" | b"ID" | b"j"
            | b"J" | b"K" | b"k" | b"l" | b"m" | b"M" | b"MP" | b"n" | b"q" | b"Q" | b"re"
            | b"RG" | b"rg" | b"ri" | b"s" | b"S" | b"SC" | b"sc" | b"SCN" | b"scn" | b"sh"
            | b"T*" | b"Tc" | b"Td" | b"TD" | b"Tf" | b"Tj" | b"TJ" | b"TL" | b"Tm" | b"Tr"
            | b"Ts" | b"Tw" | b"Tz" | b"v" | b"w" | b"W" | b"W*" | b"y" | b"'" | b"\""
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_basic_operations() {
        let src = b"q 1 0 0 1 72.5 -3 cm /F1 12 Tf (a\\(b\\)\\101\\\nc) Tj [<0041> -20 (x)] TJ Q % comment\n/GS0 gs";
        let o = ops(src);
        let names: Vec<_> = o.iter().map(|(n, _)| String::from_utf8_lossy(n).to_string()).collect();
        assert_eq!(names, ["q", "cm", "Tf", "Tj", "TJ", "Q", "gs"]);
        assert_eq!(o[1].1[4], Operand::Real(72.5));
        assert_eq!(o[1].1[5], Operand::Int(-3));
        assert_eq!(o[2].1[0], Operand::Name(b"F1".to_vec()));
        assert_eq!(o[3].1[0], Operand::Str(b"a(b)Ac".to_vec()));
        match &o[4].1[0] {
            Operand::Array(a) => {
                assert_eq!(a[0], Operand::Str(vec![0, 0x41]));
                assert_eq!(a[1], Operand::Int(-20));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lexes_dicts_names_and_odd_numbers() {
        let src = b"/Span <</ActualText (x) /MCID 3>> BDC EMC /A#20B BMC .5 -.25 4.-2 --3 0 0 m";
        let o = ops(src);
        assert_eq!(o[0].0, b"BDC");
        assert_eq!(o[0].1[1].dict_get(b"MCID"), Some(&Operand::Int(3)));
        assert_eq!(o[2].1[0], Operand::Name(b"A B".to_vec()));
        let m = &o[3].1;
        assert_eq!(m[0], Operand::Real(0.5));
        assert_eq!(m[1], Operand::Real(-0.25));
        assert_eq!(m[2], Operand::Real(4.0));
        assert_eq!(m[3], Operand::Real(-3.0));
        assert_eq!(o[3].0, b"m");
    }

    #[test]
    fn inline_images_by_size_and_heuristic() {
        // Unfiltered 2x2 RGB = 12 bytes containing "EI" inside the data.
        let mut src = b"q BI /W 2 /H 2 /BPC 8 /CS /RGB ID ".to_vec();
        src.extend_from_slice(b"EI EI EI EI");
        src.push(b'x');
        src.extend_from_slice(b"\nEI Q");
        let mut seen = Vec::new();
        visit(&src, |op| {
            if let Some(img) = op.inline {
                assert_eq!(img.data.1 - img.data.0, 12);
            }
            seen.push(op.name.to_vec());
            true
        });
        assert_eq!(seen, [b"q".to_vec(), b"BI".to_vec(), b"Q".to_vec()]);

        let src2 = b"BI /W 4 /H 4 /F /Fl ID \x78\x9c\x01\x02EI\x03 EI\nQ";
        let mut n = Vec::new();
        visit(src2, |op| {
            n.push(op.name.to_vec());
            true
        });
        assert_eq!(n, [b"BI".to_vec(), b"Q".to_vec()]);
    }

    #[test]
    fn spans_cover_operands() {
        let src = b"  1 2 m 3 4 l";
        let mut spans = Vec::new();
        visit(src, |op| {
            spans.push(&src[op.start..op.end]);
            true
        });
        assert_eq!(spans, [&b"1 2 m"[..], &b"3 4 l"[..]]);
    }

    #[test]
    fn serialisation() {
        let mut out = Vec::new();
        write_op(
            &mut out,
            b"BDC",
            &[
                Operand::Name(b"P".to_vec()),
                Operand::Dict(vec![(b"MCID".to_vec(), Operand::Int(1))]),
            ],
        );
        assert_eq!(out, b"/P <</MCID 1>> BDC\n");
        let mut r = Vec::new();
        write_real(&mut r, 1.5);
        r.push(b' ');
        write_real(&mut r, 2.0);
        r.push(b' ');
        write_real(&mut r, -0.0000001);
        r.push(b' ');
        write_real(&mut r, f64::NAN);
        assert_eq!(r, b"1.5 2 0 0");
        let mut n = Vec::new();
        write_name(&mut n, b"A B#");
        assert_eq!(n, b"/A#20B#23");
        let mut a = Vec::new();
        write_operand(&mut a, &Operand::Array(vec![Operand::Bool(true), Operand::Null, Operand::Str(b"A".to_vec())]));
        assert_eq!(a, b"[true null <41>]");
        assert!(is_known_operator(b"Tj") && !is_known_operator(b"foo"));
    }
}
