//! Low-level object rules: stream filters, external streams, number and
//! string limits, undefined content operators.

use lopdf::{Object, ObjectId};

use crate::content;
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

/// Content streams may only use operators defined in ISO 32000; anything
/// else (also inside BX/EX) is removed. Streams are only rewritten when
/// needed.
fn strip_undefined_operators(ctx: &mut Ctx) {
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
    let mut fixed = 0;
    for id in targets {
        let Some(Object::Stream(s)) = doc.objects.get(&id) else { continue };
        let Some(data) = util::stream_data(s) else { continue };
        let mut bad = false;
        content::visit(&data, |op| {
            if op.inline.is_none() && (!content::is_known_operator(op.name) || op.name == b"BX" || op.name == b"EX") {
                bad = true;
            }
            !bad
        });
        if !bad {
            continue;
        }
        let mut out = Vec::with_capacity(data.len());
        content::visit(&data, |op| {
            let skip = op.inline.is_none() && (!content::is_known_operator(op.name) || op.name == b"BX" || op.name == b"EX");
            if !skip {
                out.extend_from_slice(&data[op.start..op.end]);
                out.push(b'\n');
            }
            true
        });
        let dict = s.dict.clone();
        doc.objects.insert(id, Object::Stream(util::flate_stream(dict, &out)));
        fixed += 1;
    }
    if fixed > 0 {
        ctx.report.note("operators_removed", format!("{fixed}"));
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
