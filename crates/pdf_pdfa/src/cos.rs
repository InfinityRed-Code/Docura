//! Low-level object rules: stream filters, external streams, number and
//! string limits, undefined content operators.

use lopdf::{Object, ObjectId};

use crate::content::{self, Operand};
use crate::level::Level;
use crate::util;
use crate::Ctx;

const STD_FILTERS: [&[u8]; 9] = [
    b"ASCIIHexDecode", b"ASCII85Decode", b"LZWDecode", b"FlateDecode", b"RunLengthDecode", b"CCITTFaxDecode",
    b"JBIG2Decode", b"DCTDecode", b"JPXDecode",
];

pub fn run(ctx: &mut Ctx) {
    let level = ctx.level;
    let doc = &mut ctx.doc;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in &ids {
        let Some(Object::Stream(s)) = doc.objects.get_mut(id) else { continue };
        s.dict.remove(b"F");
        s.dict.remove(b"FFilter");
        s.dict.remove(b"FDecodeParms");
        let filters: Vec<Vec<u8>> = match s.dict.get(b"Filter") {
            Ok(Object::Name(n)) => vec![n.clone()],
            Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_name().ok().map(|n| n.to_vec())).collect(),
            _ => Vec::new(),
        };
        if filters.is_empty() {
            continue;
        }
        // LZW is forbidden and Crypt has no meaning without encryption:
        // decode and store such streams with Flate.
        let needs_recode = filters.iter().any(|f| f == b"LZWDecode" || f == b"Crypt" || !STD_FILTERS.contains(&f.as_slice()));
        if needs_recode {
            if let Some(data) = util::stream_data(s) {
                let dict = s.dict.clone();
                *s = util::flate_stream(dict, &data);
                continue;
            }
        }
        // Not decodable here (an image codec follows): drop the Crypt entry.
        let has_crypt = filters.iter().any(|f| f == b"Crypt");
        if has_crypt {
            let kept: Vec<Object> = filters.iter().filter(|f| f.as_slice() != b"Crypt").map(|f| Object::Name(f.clone())).collect();
            match kept.len() {
                0 => {
                    s.dict.remove(b"Filter");
                }
                1 => s.dict.set("Filter", kept[0].clone()),
                _ => s.dict.set("Filter", Object::Array(kept)),
            }
            s.dict.remove(b"DecodeParms");
        }
    }
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(o) = doc.objects.get_mut(&id) {
            clamp(o, level, 0);
        }
    }
    clamp_trailer(&mut doc.trailer, level);
    strip_undefined_operators(ctx);
}

fn clamp_trailer(t: &mut lopdf::Dictionary, level: Level) {
    for (_, v) in t.iter_mut() {
        clamp(v, level, 0);
    }
}

/// Keep numbers and strings inside the implementation limits.
fn clamp(o: &mut Object, level: Level, depth: u32) {
    if depth > 64 {
        return;
    }
    match o {
        Object::Integer(i) => {
            *i = (*i).clamp(i32::MIN as i64, i32::MAX as i64);
        }
        Object::Real(r) => {
            let max = level.max_real() as f32;
            if !r.is_finite() {
                *r = 0.0;
            } else if r.abs() > max {
                *r = r.signum() * max;
            } else if r.abs() < 1.175e-38 && *r != 0.0 {
                *r = 0.0;
            }
        }
        Object::String(s, _) => {
            if s.len() > level.max_string_len() {
                s.truncate(level.max_string_len());
            }
        }
        Object::Array(a) => a.iter_mut().for_each(|x| clamp(x, level, depth + 1)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| clamp(v, level, depth + 1)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| clamp(v, level, depth + 1)),
        _ => {}
    }
}

fn undefined(op: &content::Op) -> bool {
    op.inline.is_none() && (!content::is_known_operator(op.name) || op.name == b"BX" || op.name == b"EX")
}

/// A real operand beyond the limit of the level, also inside arrays and
/// dictionaries.
fn over_limit(o: &Operand, max: f64) -> bool {
    match o {
        Operand::Real(r) => r.abs() > max,
        Operand::Array(a) => a.iter().any(|x| over_limit(x, max)),
        Operand::Dict(d) => d.iter().any(|(_, v)| over_limit(v, max)),
        _ => false,
    }
}

/// Integers may be larger than reals (up to 2^31 - 1), so a real beyond the
/// limit is written as the nearest integer.
fn to_integer(o: &mut Operand, max: f64) {
    match o {
        Operand::Real(r) if r.abs() > max => *o = Operand::Int((r.round() as i64).clamp(i32::MIN as i64, i32::MAX as i64)),
        Operand::Array(a) => a.iter_mut().for_each(|x| to_integer(x, max)),
        Operand::Dict(d) => d.iter_mut().for_each(|(_, v)| to_integer(v, max)),
        _ => {}
    }
}

/// Content streams may only use operators defined in ISO 32000; anything
/// else (also inside BX/EX) is removed.
///
/// Real operands have to stay inside the limit of the level as well. PDF/A-1
/// allows ±32767 only, which a large drawing in a fine unit exceeds (an A0
/// plan in 1/720 inch reaches 33 700). Such an operand becomes the nearest
/// integer: it moves by at most half a unit of its own coordinate system,
/// and because that system has to be scaled down to fit a page, by less than
/// a quarter of a point on paper. Only the operations concerned are written
/// anew; everything else is copied byte for byte.
///
/// Streams are only rewritten when needed.
fn strip_undefined_operators(ctx: &mut Ctx) {
    let max = ctx.level.max_real();
    let doc = &mut ctx.doc;
    let mut targets: Vec<ObjectId> = Vec::new();
    for p in util::pages(doc) {
        if let Some(d) = doc.objects.get(&p).and_then(|o| o.as_dict().ok()) {
            match d.get(b"Contents") {
                Ok(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Array(a)) => targets.extend(a.iter().filter_map(|x| x.as_reference().ok())),
                    _ => targets.push(*r),
                },
                Ok(Object::Array(a)) => targets.extend(a.iter().filter_map(|x| x.as_reference().ok())),
                _ => {}
            }
        }
    }
    for (id, o) in &doc.objects {
        if let Object::Stream(s) = o {
            let is_form = s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok()) == Some(b"Form".as_slice());
            let is_pattern = s.dict.get(b"PatternType").ok().and_then(|n| n.as_i64().ok()) == Some(1);
            if is_form || is_pattern {
                targets.push(*id);
            }
        }
    }
    let (mut fixed, mut rounded) = (0, 0);
    for id in targets {
        let Some(Object::Stream(s)) = doc.objects.get(&id) else { continue };
        let Some(data) = util::stream_data(s) else { continue };
        let mut bad = false;
        content::visit(&data, |op| {
            bad = undefined(op) || op.args.iter().any(|a| over_limit(a, max));
            !bad
        });
        if !bad {
            continue;
        }
        let mut out = Vec::with_capacity(data.len());
        let (mut dropped, mut limited) = (false, false);
        content::visit(&data, |op| {
            if undefined(op) {
                dropped = true;
            } else if op.args.iter().any(|a| over_limit(a, max)) {
                let mut args = op.args.to_vec();
                args.iter_mut().for_each(|a| to_integer(a, max));
                content::write_op(&mut out, op.name, &args);
                limited = true;
            } else {
                out.extend_from_slice(&data[op.start..op.end]);
                out.push(b'\n');
            }
            true
        });
        let dict = s.dict.clone();
        doc.objects.insert(id, Object::Stream(util::flate_stream(dict, &out)));
        fixed += dropped as usize;
        rounded += limited as usize;
    }
    if fixed > 0 {
        ctx.report.note("operators_removed", format!("{fixed}"));
    }
    if rounded > 0 {
        ctx.report.note("numbers_rounded", format!("{rounded}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypt_filter_is_removed() {
        use lopdf::{dictionary, Dictionary, Document, Stream};
        let name = |s: &str| Object::Name(s.as_bytes().to_vec());
        let mut doc = Document::with_version("1.7");
        let mk = |filters: Vec<Object>| {
            let mut s = Stream::new(dictionary! { "Filter" => filters, "DecodeParms" => vec![Object::Null], "F" => 1, "FFilter" => "x", "FDecodeParms" => 2 }, b"data".to_vec());
            s.allows_compression = false;
            s
        };
        let jpeg = doc.add_object(mk(vec![name("Crypt"), name("DCTDecode")]));
        let two = doc.add_object(mk(vec![name("Crypt"), name("DCTDecode"), name("JPXDecode")]));
        let plain = doc.add_object(mk(vec![name("Crypt")]));
        let none = doc.add_object(Stream::new(Dictionary::new(), b"x".to_vec()));
        let o = crate::Options::new("2b".parse().unwrap());
        let mut ctx = Ctx { doc, level: o.level, opts: &o, report: crate::Report::default(), source: &[] };
        run(&mut ctx);
        let dict = |id| ctx.doc.objects.get(&id).unwrap().as_stream().unwrap().dict.clone();
        assert_eq!(dict(jpeg).get(b"Filter").unwrap(), &name("DCTDecode"));
        assert!(!dict(jpeg).has(b"DecodeParms") && !dict(jpeg).has(b"F") && !dict(jpeg).has(b"FFilter") && !dict(jpeg).has(b"FDecodeParms"));
        assert_eq!(dict(two).get(b"Filter").unwrap(), &Object::Array(vec![name("DCTDecode"), name("JPXDecode")]));
        // Decodable: stored with Flate instead.
        assert_eq!(dict(plain).get(b"Filter").unwrap(), &name("FlateDecode"));
        assert_eq!(util::stream_data(ctx.doc.objects.get(&plain).unwrap().as_stream().unwrap()).unwrap(), b"data");
        assert!(!dict(none).has(b"Filter"));
    }

    /// Coordinates of a large plan (A0 in 1/720 inch) exceed the PDF/A-1
    /// limit for real numbers.
    #[test]
    fn large_real_operands_become_integers_in_part_1() {
        use lopdf::{dictionary, Document, Stream};
        let src: &[u8] = b"0.1 0 0 0.1 0 0 cm 5.4 w 33704.4 23839.4 m 0.601563 -40000.5 l S\n[40000.5 3.25] 0 d /P <</X 50000.25 /Y [1.5]>> BDC EMC (a) Tj\n32767 0 m 32767.0 1.5 l 99999 2 l S";
        let convert = |level: &str, content: &[u8]| {
            let mut doc = Document::with_version("1.4");
            let form = doc.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Form" }, content.to_vec()));
            let o = crate::Options::new(level.parse().unwrap());
            let mut ctx = Ctx { doc, level: o.level, opts: &o, report: crate::Report::default(), source: &[] };
            run(&mut ctx);
            (util::stream_data(ctx.doc.objects.get(&form).unwrap().as_stream().unwrap()).unwrap(), ctx.report)
        };
        // Only the operations with a real beyond ±32767 are rewritten: the
        // number becomes the nearest integer, the rest keeps its value.
        let (out, report) = convert("1b", src);
        let expected: &[u8] = b"0.1 0 0 0.1 0 0 cm\n5.4 w\n33704 23839.4 m\n0.601563 -40001 l\nS\n[40001 3.25] 0 d\n/P <</X 50000/Y [1.5]>> BDC\nEMC\n(a) Tj\n32767 0 m\n32767.0 1.5 l\n99999 2 l\nS\n";
        assert_eq!(String::from_utf8_lossy(&out), String::from_utf8_lossy(expected));
        assert!(report.has("numbers_rounded") && !report.has("operators_removed"));
        let reals_beyond = |data: &[u8]| content::ops(data).iter().flat_map(|(_, args)| args.clone()).filter(|a| over_limit(a, 32767.0)).count();
        assert_eq!((reals_beyond(src), reals_beyond(&out)), (4, 0));
        // Later parts have no such limit: the stream stays as it is.
        let (out, report) = convert("2b", src);
        assert_eq!(out, src);
        assert!(!report.has("numbers_rounded"));
        // Together with an undefined operator both repairs are reported.
        let (out, report) = convert("1b", b"40000.5 0 m foo 1 2 l S");
        assert_eq!(out, b"40001 0 m\n1 2 l\nS\n");
        assert!(report.has("numbers_rounded") && report.has("operators_removed"));
        // Far beyond what an integer may hold as well.
        let mut huge = Operand::Real(-1e12);
        to_integer(&mut huge, 32767.0);
        assert_eq!(huge, Operand::Int(i32::MIN as i64));
    }

    #[test]
    fn clamps() {
        let l1: Level = "1b".parse().unwrap();
        let mut o = Object::Array(vec![
            Object::Real(40000.0),
            Object::Integer(1 << 40),
            Object::Real(f32::NAN),
            Object::Real(1e-40),
            Object::String(vec![0; 70000], lopdf::StringFormat::Literal),
        ]);
        clamp(&mut o, l1, 0);
        match o {
            Object::Array(a) => {
                assert_eq!(a[0], Object::Real(32767.0));
                assert_eq!(a[1], Object::Integer(i32::MAX as i64));
                assert_eq!(a[2], Object::Real(0.0));
                assert_eq!(a[3], Object::Real(0.0));
                assert!(matches!(&a[4], Object::String(s, _) if s.len() == 65535));
            }
            _ => unreachable!(),
        }
    }
}
