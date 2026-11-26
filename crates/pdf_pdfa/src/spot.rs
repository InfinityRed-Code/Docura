//! Spot colours: Separation and DeviceN colour spaces.
//!
//! PDF/A-2 and later (ISO 19005-2, 6.2.4.4) want every spot colorant of a
//! DeviceN space described in its `Colorants` dictionary, and every
//! Separation with the same name to use the same alternate space and tint
//! transform. PDF/A-1 only limits DeviceN to eight colorants; pages using
//! more are left to the rasterizer (see [`page_has_wide_devicen`]).

use std::collections::BTreeMap;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::util::{self, deref, num};
use crate::Ctx;

/// Colorant name → (alternate space, tint transform), shared by every
/// Separation of that name.
type Canon = BTreeMap<Vec<u8>, (Object, Object)>;

pub fn run(ctx: &mut Ctx) {
    if ctx.level.part() < 2 {
        return;
    }
    let doc = &mut ctx.doc;
    let mut canon = collect(doc);
    if canon.is_empty() && !has_devicen(doc) {
        return;
    }
    for (alt, tint) in canon.values_mut() {
        *alt = shared(doc, alt.clone());
        *tint = shared(doc, tint.clone());
    }
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut added = 0;
    for id in ids {
        // Taken out of the map so the walk can read and add other objects.
        let Some(mut o) = doc.objects.remove(&id) else { continue };
        rewrite(&mut o, doc, &mut canon, &mut added, 0);
        doc.objects.insert(id, o);
    }
    if added > 0 {
        ctx.report.note("colorants_described", added.to_string());
    }
}

fn is_name(o: &Object, n: &[u8]) -> bool {
    matches!(o, Object::Name(x) if x == n)
}

fn separation_name(a: &[Object]) -> Option<&[u8]> {
    // (An array of four colorant names is not a colour space.)
    if a.len() == 4 && is_name(&a[0], b"Separation") && !matches!(a[3], Object::Name(_)) {
        if let Object::Name(n) = &a[1] {
            return Some(n);
        }
    }
    None
}

fn is_devicen(a: &[Object]) -> bool {
    (a.len() == 4 || a.len() == 5) && is_name(&a[0], b"DeviceN") && matches!(a[1], Object::Array(_) | Object::Reference(_))
}

/// A value that several colour spaces can point at: names and references
/// stay, anything else becomes an indirect object.
fn shared(doc: &mut Document, o: Object) -> Object {
    match o {
        Object::Name(_) | Object::Reference(_) => o,
        other => Object::Reference(doc.add_object(other)),
    }
}

fn visit<'a>(o: &'a Object, f: &mut dyn FnMut(&'a [Object]), depth: u32) {
    if depth > 32 {
        return;
    }
    match o {
        Object::Array(a) => {
            f(a);
            a.iter().for_each(|x| visit(x, f, depth + 1));
        }
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| visit(v, f, depth + 1)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| visit(v, f, depth + 1)),
        _ => {}
    }
}

/// First definition of every Separation colorant, in object order.
fn collect(doc: &Document) -> Canon {
    let mut canon = Canon::new();
    for o in doc.objects.values() {
        visit(
            o,
            &mut |a| {
                if let Some(n) = separation_name(a) {
                    canon.entry(n.to_vec()).or_insert_with(|| (a[2].clone(), a[3].clone()));
                }
            },
            0,
        );
    }
    canon
}

fn has_devicen(doc: &Document) -> bool {
    let mut found = false;
    for o in doc.objects.values() {
        visit(o, &mut |a| found |= is_devicen(a), 0);
        if found {
            break;
        }
    }
    found
}

fn rewrite(o: &mut Object, doc: &mut Document, canon: &mut Canon, added: &mut usize, depth: u32) {
    if depth > 32 {
        return;
    }
    match o {
        Object::Array(a) => {
            if let Some(n) = separation_name(a).map(|n| n.to_vec()) {
                if let Some((alt, tint)) = canon.get(&n) {
                    a[2] = alt.clone();
                    a[3] = tint.clone();
                }
                return;
            }
            if is_devicen(a) {
                *added += describe_colorants(a, doc, canon);
            }
            a.iter_mut().for_each(|x| rewrite(x, doc, canon, added, depth + 1));
        }
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| rewrite(v, doc, canon, added, depth + 1)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| rewrite(v, doc, canon, added, depth + 1)),
        _ => {}
    }
}

/// Names of a DeviceN space.
fn colorant_names(doc: &Document, a: &[Object]) -> Vec<Vec<u8>> {
    match deref(doc, &a[1]) {
        Object::Array(n) => n.iter().filter_map(|x| deref(doc, x).as_name().ok().map(|n| n.to_vec())).collect(),
        _ => Vec::new(),
    }
}

/// Add the missing `Colorants` entries of a DeviceN array. Returns how
/// many were added.
fn describe_colorants(a: &mut Vec<Object>, doc: &mut Document, canon: &mut Canon) -> usize {
    let names = colorant_names(doc, a);
    let (alt, tint) = (a[2].clone(), a[3].clone());
    // Attributes and Colorants may be direct or indirect.
    let attrs_ref = a.get(4).and_then(|o| o.as_reference().ok());
    let mut attrs = match a.get(4).map(|o| deref(doc, o)) {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    let col_ref = attrs.get(b"Colorants").ok().and_then(|o| o.as_reference().ok());
    let mut colorants = util::get_dict(doc, &attrs, b"Colorants").cloned().unwrap_or_default();
    let process: Vec<Vec<u8>> = util::get_dict(doc, &attrs, b"Process")
        .and_then(|p| util::get_array(doc, p, b"Components"))
        .map(|c| c.iter().filter_map(|x| deref(doc, x).as_name().ok().map(|n| n.to_vec())).collect())
        .unwrap_or_default();
    let mut added = 0;
    for (i, n) in names.iter().enumerate() {
        if n == b"None" || process.contains(n) || colorants.has(n) {
            continue;
        }
        if !canon.contains_key(n) {
            let derived = derive(doc, names.len(), i, &alt, &tint);
            canon.insert(n.clone(), derived);
        }
        let (c_alt, c_tint) = canon[n].clone();
        colorants.set(n.clone(), Object::Array(vec![Object::Name(b"Separation".to_vec()), Object::Name(n.clone()), c_alt, c_tint]));
        added += 1;
    }
    if added == 0 {
        return 0;
    }
    match col_ref {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(colorants));
        }
        None => attrs.set("Colorants", Object::Dictionary(colorants)),
    }
    match attrs_ref {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(attrs));
        }
        None => {
            if a.len() == 5 {
                a[4] = Object::Dictionary(attrs);
            } else {
                a.push(Object::Dictionary(attrs));
            }
        }
    }
    added
}

/// Number of components of an alternate colour space.
fn components(doc: &Document, cs: &Object) -> usize {
    match deref(doc, cs) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" => 1,
            b"DeviceCMYK" => 4,
            _ => 3,
        },
        Object::Array(a) => match a.first().map(|o| deref(doc, o)) {
            Some(Object::Name(n)) if n == b"CalGray" => 1,
            Some(Object::Name(n)) if n == b"ICCBased" => match a.get(1).map(|o| deref(doc, o)) {
                Some(Object::Stream(s)) => util::get_num(doc, &s.dict, b"N").unwrap_or(3.0) as usize,
                _ => 3,
            },
            _ => 3,
        },
        _ => 3,
    }
}

fn is_lab(doc: &Document, cs: &Object) -> bool {
    matches!(deref(doc, cs), Object::Array(a) if matches!(a.first().map(|o| deref(doc, o)), Some(Object::Name(n)) if n == b"Lab"))
}

/// Alternate space and tint transform for colorant `index` of a DeviceN
/// space with `n` colorants, derived from the DeviceN's own transform
/// (all other colorants at zero).
fn derive(doc: &mut Document, n: usize, index: usize, alt: &Object, tint: &Object) -> (Object, Object) {
    let function = match deref(doc, tint) {
        Object::Stream(s) => match util::get_num(doc, &s.dict, b"FunctionType").map(|v| v as i64) {
            Some(4) => calculator_for(doc, s, n, index),
            Some(0) => sampled_ends(doc, s, n, index).map(|(c0, c1)| Object::Dictionary(exponential(c0, c1))),
            _ => None,
        },
        _ => None,
    };
    let function = function.unwrap_or_else(|| {
        let m = components(doc, alt);
        let (c0, c1) = if is_lab(doc, alt) {
            (vec![100.0, 0.0, 0.0], vec![0.0, 0.0, 0.0])
        } else if m == 4 {
            (vec![0.0; 4], vec![0.0, 0.0, 0.0, 1.0])
        } else {
            (vec![1.0; m], vec![0.0; m])
        };
        Object::Dictionary(exponential(c0, c1))
    });
    let tint = Object::Reference(doc.add_object(function));
    (shared(doc, alt.clone()), tint)
}

/// Linear (exponent 1) function from `c0` at 0 to `c1` at 1.
fn exponential(c0: Vec<f64>, c1: Vec<f64>) -> Dictionary {
    let arr = |v: Vec<f64>| Object::Array(v.into_iter().map(|x| Object::Real(x as f32)).collect());
    dictionary! { "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "C0" => arr(c0), "C1" => arr(c1), "N" => 1 }
}

/// PostScript calculator function: push zeros for the other colorants,
/// then run the DeviceN's own code.
fn calculator_for(doc: &Document, s: &Stream, n: usize, index: usize) -> Option<Object> {
    let data = util::stream_data(s)?;
    let text = String::from_utf8_lossy(&data);
    let (open, close) = (text.find('{')?, text.rfind('}')?);
    if close <= open {
        return None;
    }
    let range = util::get_array(doc, &s.dict, b"Range")?.clone();
    Some(Object::Stream(Stream::new(
        dictionary! { "FunctionType" => 4, "Domain" => vec![0.into(), 1.into()], "Range" => range },
        calculator_code(&text[open + 1..close], n, index).into_bytes(),
    )))
}

/// `{ tint → the n inputs of the DeviceN code, then that code }`.
pub fn calculator_code(inner: &str, n: usize, index: usize) -> String {
    let mut code = String::from("{ ");
    for _ in 1..n {
        code.push_str("0 ");
    }
    if index > 0 && n > 1 {
        // The tint value sits at the bottom of the n values; bring it to
        // position `index`.
        code.push_str(&format!("{n} {index} roll "));
    }
    code.push_str(inner.trim());
    code.push_str(" }");
    code
}

/// Output of a sampled (type 0) function with all inputs 0, and with
/// input `index` at 1.
fn sampled_ends(doc: &Document, s: &Stream, n: usize, index: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    let d = &s.dict;
    let nums = |key: &[u8]| -> Option<Vec<f64>> { Some(util::get_array(doc, d, key)?.iter().filter_map(|x| num(deref(doc, x))).collect()) };
    let size: Vec<usize> = nums(b"Size")?.into_iter().map(|v| v as usize).collect();
    let range = nums(b"Range")?;
    let bps = util::get_num(doc, d, b"BitsPerSample")? as usize;
    let m = range.len() / 2;
    if size.len() != n || m == 0 || !matches!(bps, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) || size.contains(&0) {
        return None;
    }
    let decode = nums(b"Decode").filter(|v| v.len() == range.len()).unwrap_or_else(|| range.clone());
    let encode = nums(b"Encode").filter(|v| v.len() == 2 * n);
    let data = util::stream_data(s)?;
    let at = |inputs: &[f64]| -> Option<Vec<f64>> {
        let mut sample = 0usize;
        let mut stride = 1usize;
        for (j, x) in inputs.iter().enumerate() {
            let (e0, e1) = match &encode {
                Some(e) => (e[2 * j], e[2 * j + 1]),
                None => (0.0, (size[j] - 1) as f64),
            };
            let idx = (e0 + x * (e1 - e0)).round().clamp(0.0, (size[j] - 1) as f64) as usize;
            sample += idx * stride;
            stride *= size[j];
        }
        let max = ((1u64 << bps) - 1) as f64;
        (0..m)
            .map(|k| {
                let v = read_bits(&data, (sample * m + k) * bps, bps)? as f64;
                Some(decode[2 * k] + v / max * (decode[2 * k + 1] - decode[2 * k]))
            })
            .collect()
    };
    let zero = vec![0.0; n];
    let mut one = zero.clone();
    one[index] = 1.0;
    Some((at(&zero)?, at(&one)?))
}

fn read_bits(data: &[u8], bit: usize, count: usize) -> Option<u64> {
    let mut v = 0u64;
    for i in bit..bit + count {
        let byte = *data.get(i / 8)?;
        v = (v << 1) | ((byte >> (7 - i % 8)) & 1) as u64;
    }
    Some(v)
}

// ---------------------------------------------------------------- PDF/A-1

fn wide_space(doc: &Document, cs: &Object, max: usize, depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    match deref(doc, cs) {
        Object::Array(a) => match a.first().map(|o| deref(doc, o)) {
            Some(Object::Name(n)) if n == b"DeviceN" => a.len() >= 2 && colorant_names(doc, a).len() > max,
            Some(Object::Name(n)) if n == b"Indexed" || n == b"Pattern" => a.get(1).map(|b| wide_space(doc, b, max, depth + 1)).unwrap_or(false),
            _ => false,
        },
        _ => false,
    }
}

fn wide_in_resources(doc: &Document, res: &Dictionary, max: usize, depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    let space_of = |d: &Dictionary| d.get(b"ColorSpace").ok().map(|c| wide_space(doc, c, max, 0)).unwrap_or(false);
    if let Some(cs) = util::get_dict(doc, res, b"ColorSpace") {
        if cs.iter().any(|(_, v)| wide_space(doc, v, max, 0)) {
            return true;
        }
    }
    if let Some(sh) = util::get_dict(doc, res, b"Shading") {
        if sh.iter().any(|(_, v)| util::dict_of(doc, v).map(space_of).unwrap_or(false)) {
            return true;
        }
    }
    for key in [b"XObject".as_slice(), b"Pattern", b"Font"] {
        let Some(d) = util::get_dict(doc, res, key) else { continue };
        for (_, v) in d.iter() {
            let Some(x) = util::dict_of(doc, v) else { continue };
            if space_of(x) {
                return true;
            }
            if let Some(sh) = util::get_dict(doc, x, b"Shading") {
                if space_of(sh) {
                    return true;
                }
            }
            if let Some(r) = util::get_dict(doc, x, b"Resources") {
                if wide_in_resources(doc, r, max, depth + 1) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether the page's resources hold a DeviceN space with more than `max`
/// colorants (PDF/A-1 allows eight).
pub fn page_has_wide_devicen(doc: &Document, page: ObjectId, max: usize) -> bool {
    util::inherited(doc, page, b"Resources")
        .and_then(|o| util::dict_of(doc, &o).cloned())
        .map(|r| wide_in_resources(doc, &r, max, 0))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> Object {
        Object::Name(s.as_bytes().to_vec())
    }

    fn sep(n: &str, alt: &str, c1: f32) -> Object {
        Object::Array(vec![name("Separation"), name(n), name(alt), Object::Dictionary(dictionary! { "FunctionType" => 2, "N" => 1, "C1" => vec![Object::Real(c1)] })])
    }

    fn ps(inputs: usize, code: &str) -> Stream {
        let domain: Vec<Object> = (0..inputs).flat_map(|_| [Object::Integer(0), Object::Integer(1)]).collect();
        let range: Vec<Object> = (0..4).flat_map(|_| [Object::Integer(0), Object::Integer(1)]).collect();
        Stream::new(dictionary! { "FunctionType" => 4, "Domain" => domain, "Range" => range }, code.as_bytes().to_vec())
    }

    fn convert(doc: Document, level: &str) -> (Document, crate::Report) {
        let opts = crate::Options::new(level.parse().unwrap());
        let mut ctx = Ctx { doc, level: opts.level, opts: &opts, report: crate::Report::default(), source: &[] };
        run(&mut ctx);
        (ctx.doc, ctx.report)
    }

    #[test]
    fn calculator_prefix() {
        assert_eq!(calculator_code(" pop 0 0 ", 1, 0), "{ pop 0 0 }");
        assert_eq!(calculator_code("0 0", 2, 0), "{ 0 0 0 }");
        assert_eq!(calculator_code("0 0", 2, 1), "{ 0 2 1 roll 0 0 }");
        assert_eq!(calculator_code("x", 4, 2), "{ 0 0 0 4 2 roll x }");
    }

    #[test]
    fn bits() {
        assert_eq!(read_bits(&[0b1010_0000, 0xFF], 0, 4), Some(0b1010));
        assert_eq!(read_bits(&[0b1010_0000, 0xFF], 4, 8), Some(0x0F));
        assert_eq!(read_bits(&[0x12, 0x34], 0, 16), Some(0x1234));
        assert_eq!(read_bits(&[0x12], 4, 8), None);
    }

    #[test]
    fn same_name_separations_are_unified() {
        let mut doc = Document::with_version("1.7");
        let a = doc.add_object(sep("Spot", "DeviceCMYK", 0.1));
        let b = doc.add_object(sep("Spot", "DeviceRGB", 0.9));
        let other = doc.add_object(dictionary! { "CS" => sep("Other", "DeviceGray", 0.5), "Nested" => vec![name("Indexed"), sep("Spot", "DeviceGray", 0.3), 1.into(), Object::string_literal("ab")] });
        let (doc, report) = convert(doc, "2b");
        let arr = |id| doc.objects.get(&id).unwrap().as_array().unwrap().clone();
        assert_eq!(arr(a)[2], name("DeviceCMYK"));
        assert_eq!(arr(b)[2], name("DeviceCMYK"));
        assert!(matches!(arr(a)[3], Object::Reference(_)));
        assert_eq!(arr(a)[3], arr(b)[3]);
        let o = doc.objects.get(&other).unwrap().as_dict().unwrap();
        let nested = o.get(b"Nested").unwrap().as_array().unwrap()[1].as_array().unwrap().clone();
        assert_eq!((&nested[2], &nested[3]), (&arr(a)[2], &arr(a)[3]));
        assert_eq!(o.get(b"CS").unwrap().as_array().unwrap()[2], name("DeviceGray"));
        assert!(report.notes.is_empty());

        // PDF/A-1 has no such rule: untouched.
        let mut doc = Document::with_version("1.4");
        let a = doc.add_object(sep("Spot", "DeviceCMYK", 0.1));
        let (doc, _) = convert(doc, "1b");
        assert!(matches!(doc.objects.get(&a).unwrap().as_array().unwrap()[3], Object::Dictionary(_)));
        // Nothing to do at all.
        let mut doc = Document::with_version("1.7");
        doc.add_object(dictionary! { "A" => 1 });
        assert!(convert(doc, "3b").1.notes.is_empty());
    }

    #[test]
    fn devicen_gets_colorants() {
        let mut doc = Document::with_version("1.7");
        let existing = doc.add_object(sep("Gold", "DeviceCMYK", 0.2));
        let f = doc.add_object(ps(3, "{ pop pop pop 0 0 0 1 }"));
        // No attributes: Cyan and Gold are added, None is skipped.
        let plain = doc.add_object(Object::Array(vec![name("DeviceN"), Object::Array(vec![name("Cyan"), name("Gold"), name("None")]), name("DeviceCMYK"), f.into()]));
        // Indirect attributes and Colorants, Process covers Magenta.
        let col = doc.add_object(dictionary! { "Silver" => sep("Silver", "DeviceGray", 0.4) });
        let attrs = doc.add_object(dictionary! { "Colorants" => col, "Process" => dictionary! { "ColorSpace" => "DeviceCMYK", "Components" => vec![name("Cyan"), name("Magenta"), name("Yellow"), name("Black")] } });
        let f2 = doc.add_object(ps(3, "{ pop pop pop 0 0 0 0 }"));
        let full = doc.add_object(Object::Array(vec![name("DeviceN"), Object::Array(vec![name("Magenta"), name("Silver"), name("Bronze")]), name("DeviceCMYK"), f2.into(), attrs.into()]));
        // Direct, empty attributes.
        let direct = doc.add_object(Object::Array(vec![name("DeviceN"), Object::Array(vec![name("Bronze")]), name("DeviceCMYK"), f2.into(), Object::Dictionary(Dictionary::new())]));
        let (doc, report) = convert(doc, "2b");
        assert_eq!(report.notes[0].code, "colorants_described");
        assert_eq!(report.notes[0].detail, "4");

        let plain = doc.objects.get(&plain).unwrap().as_array().unwrap();
        assert_eq!(plain.len(), 5);
        let c = plain[4].as_dict().unwrap().get(b"Colorants").unwrap().as_dict().unwrap();
        assert!(c.has(b"Cyan") && c.has(b"Gold") && !c.has(b"None"));
        // Gold reuses the document's Separation.
        let gold = doc.objects.get(&existing).unwrap().as_array().unwrap();
        assert_eq!(c.get(b"Gold").unwrap().as_array().unwrap()[3], gold[3]);
        // Cyan is derived from the DeviceN transform (first of three inputs).
        let cyan = c.get(b"Cyan").unwrap().as_array().unwrap();
        let Object::Stream(cf) = deref(&doc, &cyan[3]) else { panic!("no calculator function") };
        assert_eq!(cf.content, b"{ 0 0 pop pop pop 0 0 0 1 }");
        assert_eq!(cf.dict.get(b"Domain").unwrap().as_array().unwrap().len(), 2);

        let c = doc.objects.get(&col).unwrap().as_dict().unwrap();
        assert!(c.has(b"Silver") && c.has(b"Bronze") && !c.has(b"Magenta"));
        let bronze = c.get(b"Bronze").unwrap().as_array().unwrap();
        let Object::Stream(bf) = deref(&doc, &bronze[3]) else { panic!() };
        assert_eq!(bf.content, b"{ 0 0 3 2 roll pop pop pop 0 0 0 0 }");
        assert!(doc.objects.get(&full).unwrap().as_array().unwrap()[4].as_reference().is_ok());
        // The second use of Bronze shares the first definition.
        let d = doc.objects.get(&direct).unwrap().as_array().unwrap()[4].as_dict().unwrap().get(b"Colorants").unwrap().as_dict().unwrap().get(b"Bronze").unwrap().as_array().unwrap().clone();
        assert_eq!(d[3], bronze[3]);
    }

    #[test]
    fn derived_transforms_for_other_function_types() {
        let mut doc = Document::with_version("1.7");
        // Sampled: 2 inputs (2×2 samples), 3 outputs, 8 bit.
        let samples = vec![255, 255, 255, 255, 0, 0, 0, 0, 255, 0, 0, 0];
        let f0 = doc.add_object(Stream::new(
            dictionary! { "FunctionType" => 0, "Domain" => vec![0.into(), 1.into(), 0.into(), 1.into()], "Range" => vec![0.into(), 1.into(), 0.into(), 1.into(), 0.into(), 1.into()], "Size" => vec![2.into(), 2.into()], "BitsPerSample" => 8 },
            samples,
        ));
        let (alt, tint) = derive(&mut doc, 2, 0, &name("DeviceRGB"), &Object::Reference(f0));
        assert_eq!(alt, name("DeviceRGB"));
        let d = deref(&doc, &tint).as_dict().unwrap().clone();
        assert_eq!(d.get(b"C0").unwrap(), &Object::Array(vec![Object::Real(1.0); 3]));
        assert_eq!(d.get(b"C1").unwrap(), &Object::Array(vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]));
        let (_, tint) = derive(&mut doc, 2, 1, &name("DeviceRGB"), &Object::Reference(f0));
        assert_eq!(deref(&doc, &tint).as_dict().unwrap().get(b"C1").unwrap(), &Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(1.0)]));
        // Wrong dimension → generic transform for the alternate space.
        let (_, tint) = derive(&mut doc, 3, 0, &name("DeviceRGB"), &Object::Reference(f0));
        assert_eq!(deref(&doc, &tint).as_dict().unwrap().get(b"C1").unwrap(), &Object::Array(vec![Object::Real(0.0); 3]));
        // Unknown function types and alternates.
        let lab = Object::Array(vec![name("Lab"), Object::Dictionary(Dictionary::new())]);
        let (alt, tint) = derive(&mut doc, 1, 0, &lab, &Object::Null);
        assert!(alt.as_reference().is_ok());
        assert_eq!(deref(&doc, &tint).as_dict().unwrap().get(b"C0").unwrap(), &Object::Array(vec![Object::Real(100.0), Object::Real(0.0), Object::Real(0.0)]));
        let (_, tint) = derive(&mut doc, 1, 0, &name("DeviceCMYK"), &Object::Null);
        assert_eq!(deref(&doc, &tint).as_dict().unwrap().get(b"C1").unwrap().as_array().unwrap().len(), 4);
        let f2 = doc.add_object(Stream::new(dictionary! { "FunctionType" => 3 }, Vec::new()));
        let (_, tint) = derive(&mut doc, 1, 0, &name("DeviceGray"), &Object::Reference(f2));
        assert_eq!(deref(&doc, &tint).as_dict().unwrap().get(b"C0").unwrap().as_array().unwrap().len(), 1);
        // A calculator function without braces or Range falls back too.
        let bad = doc.add_object(Stream::new(dictionary! { "FunctionType" => 4 }, b"} {".to_vec()));
        let (_, tint) = derive(&mut doc, 1, 0, &name("DeviceGray"), &Object::Reference(bad));
        assert!(deref(&doc, &tint).as_dict().is_ok());

        // Component counts.
        let icc = doc.add_object(Stream::new(dictionary! { "N" => 4 }, Vec::new()));
        assert_eq!(components(&doc, &Object::Array(vec![name("ICCBased"), icc.into()])), 4);
        assert_eq!(components(&doc, &Object::Array(vec![name("ICCBased"), Object::Null])), 3);
        assert_eq!(components(&doc, &Object::Array(vec![name("CalGray"), Object::Null])), 1);
        assert_eq!(components(&doc, &Object::Array(vec![name("CalRGB"), Object::Null])), 3);
        assert_eq!(components(&doc, &name("DeviceGray")), 1);
        assert_eq!(components(&doc, &Object::Null), 3);
    }

    #[test]
    fn sampled_function_options() {
        let doc = Document::with_version("1.7");
        // 1 input, 3 samples, 1 output, 16 bit, Encode reversed, Decode 0..2.
        let s = Stream::new(
            dictionary! { "FunctionType" => 0, "Range" => vec![0.into(), 2.into()], "Size" => vec![3.into()], "BitsPerSample" => 16, "Encode" => vec![2.into(), 0.into()], "Decode" => vec![0.into(), 2.into()] },
            vec![0, 0, 0x80, 0, 0xFF, 0xFF],
        );
        let (c0, c1) = sampled_ends(&doc, &s, 1, 0).unwrap();
        assert_eq!((c0[0], c1[0]), (2.0, 0.0));
        // Truncated data, bad bit depth, missing keys.
        let short = Stream::new(dictionary! { "Range" => vec![0.into(), 1.into()], "Size" => vec![2.into()], "BitsPerSample" => 8 }, vec![1]);
        assert!(sampled_ends(&doc, &short, 1, 0).is_none());
        let bits = Stream::new(dictionary! { "Range" => vec![0.into(), 1.into()], "Size" => vec![2.into()], "BitsPerSample" => 7 }, vec![1, 2]);
        assert!(sampled_ends(&doc, &bits, 1, 0).is_none());
        assert!(sampled_ends(&doc, &Stream::new(Dictionary::new(), Vec::new()), 1, 0).is_none());
    }

    #[test]
    fn wide_devicen_detection() {
        let mut doc = Document::with_version("1.4");
        let nine: Vec<Object> = (0..9).map(|i| name(&format!("S{i}"))).collect();
        let wide = doc.add_object(Object::Array(vec![name("DeviceN"), Object::Array(nine), name("DeviceCMYK"), Object::Null]));
        let narrow = Object::Array(vec![name("DeviceN"), Object::Array(vec![name("A")]), name("DeviceCMYK"), Object::Null]);
        let pages = doc.new_object_id();
        let page = |doc: &mut Document, res: Dictionary| doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "Resources" => res });
        let p_direct = page(&mut doc, dictionary! { "ColorSpace" => dictionary! { "A" => narrow.clone(), "B" => wide } });
        let p_indexed = page(&mut doc, dictionary! { "ColorSpace" => dictionary! { "I" => vec![name("Indexed"), wide.into(), 1.into(), Object::string_literal("x")] } });
        let img = doc.add_object(Stream::new(dictionary! { "Subtype" => "Image", "ColorSpace" => wide }, Vec::new()));
        let p_image = page(&mut doc, dictionary! { "XObject" => dictionary! { "Im" => img } });
        let form = doc.add_object(Stream::new(dictionary! { "Subtype" => "Form", "Resources" => dictionary! { "ColorSpace" => dictionary! { "B" => wide } } }, Vec::new()));
        let p_form = page(&mut doc, dictionary! { "XObject" => dictionary! { "Fm" => form } });
        let p_shading = page(&mut doc, dictionary! { "Shading" => dictionary! { "Sh" => dictionary! { "ShadingType" => 2, "ColorSpace" => wide } } });
        let p_pattern = page(&mut doc, dictionary! { "Pattern" => dictionary! { "P" => dictionary! { "PatternType" => 2, "Shading" => dictionary! { "ColorSpace" => wide } } } });
        let p_none = page(&mut doc, dictionary! { "ColorSpace" => dictionary! { "A" => narrow, "G" => "DeviceGray" }, "XObject" => dictionary! { "Fm" => Object::Null } });
        let p_empty = doc.add_object(dictionary! { "Type" => "Page" });
        for p in [p_direct, p_indexed, p_image, p_form, p_shading, p_pattern] {
            assert!(page_has_wide_devicen(&doc, p, 8), "{p:?}");
            assert!(!page_has_wide_devicen(&doc, p, 32));
        }
        assert!(!page_has_wide_devicen(&doc, p_none, 8));
        assert!(!page_has_wide_devicen(&doc, p_empty, 8));
    }
}
