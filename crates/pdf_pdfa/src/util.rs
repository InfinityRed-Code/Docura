//! Small helpers on top of lopdf.

use std::collections::BTreeSet;
use std::io::Write;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// Follow references (bounded, cycles end in `Null`).
pub fn deref<'a>(doc: &'a Document, mut o: &'a Object) -> &'a Object {
    for _ in 0..32 {
        match o {
            Object::Reference(id) => match doc.objects.get(id) {
                Some(t) => o = t,
                None => return &Object::Null,
            },
            _ => return o,
        }
    }
    &Object::Null
}

/// Dictionary of a dictionary or stream object (after dereferencing).
pub fn dict_of<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match deref(doc, o) {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

pub fn get<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    d.get(key).ok().map(|o| deref(doc, o)).filter(|o| !matches!(o, Object::Null))
}

pub fn get_dict<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    d.get(key).ok().and_then(|o| dict_of(doc, o))
}

pub fn get_name<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    match get(doc, d, key)? {
        Object::Name(n) => Some(n),
        _ => None,
    }
}

pub fn get_num(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<f64> {
    num(get(doc, d, key)?)
}

pub fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

pub fn get_array<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Vec<Object>> {
    match get(doc, d, key)? {
        Object::Array(a) => Some(a),
        _ => None,
    }
}

/// Mutable dictionary of object `id` (dictionary or stream).
pub fn dict_mut(doc: &mut Document, id: ObjectId) -> Option<&mut Dictionary> {
    match doc.objects.get_mut(&id)? {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&mut s.dict),
        _ => None,
    }
}

pub fn catalog_id(doc: &Document) -> Option<ObjectId> {
    doc.trailer.get(b"Root").ok().and_then(|o| o.as_reference().ok())
}

pub fn catalog(doc: &Document) -> Option<&Dictionary> {
    let id = catalog_id(doc)?;
    match doc.objects.get(&id)? {
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

pub fn catalog_mut(doc: &mut Document) -> Option<&mut Dictionary> {
    let id = catalog_id(doc)?;
    dict_mut(doc, id)
}

/// Page object ids in document order.
pub fn pages(doc: &Document) -> Vec<ObjectId> {
    doc.page_iter().collect()
}

/// Inheritable page attribute (Resources, MediaBox, CropBox, Rotate).
pub fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut cur = Some(page);
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        let d = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => d,
            _ => break,
        };
        if let Ok(v) = d.get(key) {
            return Some(v.clone());
        }
        cur = d.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
    }
    None
}

/// Page box as [x0, y0, x1, y1] (normalised), MediaBox fallback Letter.
pub fn page_box(doc: &Document, page: ObjectId, key: &[u8]) -> [f64; 4] {
    let get_box = |k: &[u8]| -> Option<[f64; 4]> {
        let o = inherited(doc, page, k)?;
        let a = match deref(doc, &o) {
            Object::Array(a) => a.clone(),
            _ => return None,
        };
        if a.len() != 4 {
            return None;
        }
        let v: Vec<f64> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
        if v.len() != 4 {
            return None;
        }
        Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
    };
    let media = get_box(b"MediaBox").unwrap_or([0.0, 0.0, 612.0, 792.0]);
    if key == b"MediaBox" {
        return media;
    }
    match get_box(key) {
        Some(b) => [b[0].max(media[0]), b[1].max(media[1]), b[2].min(media[2]), b[3].min(media[3])],
        None => media,
    }
}

pub fn page_rotation(doc: &Document, page: ObjectId) -> i64 {
    inherited(doc, page, b"Rotate")
        .and_then(|o| num(deref(doc, &o)))
        .map(|r| ((r as i64 % 360) + 360) % 360)
        .unwrap_or(0)
}

/// Make sure the page has its own /Resources dictionary (copying inherited
/// entries) and return where it lives.
pub fn ensure_page_resources(doc: &mut Document, page: ObjectId) -> ResLoc {
    let own = doc
        .objects
        .get(&page)
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Resources").ok().cloned());
    match own {
        Some(Object::Reference(id)) if matches!(doc.objects.get(&id), Some(Object::Dictionary(_))) => {
            ResLoc::Obj(id)
        }
        Some(Object::Dictionary(_)) => ResLoc::Inline(page),
        _ => {
            let inherited = inherited(doc, page, b"Resources")
                .and_then(|o| dict_of(doc, &o).cloned())
                .unwrap_or_default();
            if let Some(d) = dict_mut(doc, page) {
                d.set("Resources", Object::Dictionary(inherited));
            }
            ResLoc::Inline(page)
        }
    }
}

/// Where a resource dictionary lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResLoc {
    /// The resource dictionary is the indirect object itself.
    Obj(ObjectId),
    /// The resource dictionary is the direct /Resources value of this object.
    Inline(ObjectId),
}

/// Every resource dictionary in the document.
pub fn resource_locations(doc: &Document) -> Vec<ResLoc> {
    let mut set = BTreeSet::new();
    for (&id, obj) in &doc.objects {
        let d = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        match d.get(b"Resources") {
            Ok(Object::Reference(r)) => {
                if matches!(doc.objects.get(r), Some(Object::Dictionary(_))) {
                    set.insert(ResLoc::Obj(*r));
                }
            }
            Ok(Object::Dictionary(_)) => {
                set.insert(ResLoc::Inline(id));
            }
            _ => {}
        }
    }
    set.into_iter().collect()
}

pub fn res_mut(doc: &mut Document, loc: ResLoc) -> Option<&mut Dictionary> {
    match loc {
        ResLoc::Obj(id) => dict_mut(doc, id),
        ResLoc::Inline(id) => dict_mut(doc, id)?.get_mut(b"Resources").ok()?.as_dict_mut().ok(),
    }
}

pub fn res_ref(doc: &Document, loc: ResLoc) -> Option<&Dictionary> {
    match loc {
        ResLoc::Obj(id) => doc.objects.get(&id)?.as_dict().ok(),
        ResLoc::Inline(id) => {
            let d = match doc.objects.get(&id)? {
                Object::Dictionary(d) => d,
                Object::Stream(s) => &s.dict,
                _ => return None,
            };
            d.get(b"Resources").ok()?.as_dict().ok()
        }
    }
}

pub fn flate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// A Flate-compressed stream with `dict` extended by Filter and Length.
pub fn flate_stream(mut dict: Dictionary, data: &[u8]) -> Stream {
    let z = flate(data);
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.remove(b"DecodeParms");
    let mut s = Stream::new(dict, z);
    s.allows_compression = false;
    s
}

/// Decoded stream data, or `None` when a filter is unsupported (DCT, JPX,
/// CCITT, JBIG2 stay encoded and are not needed decoded).
///
/// Handles filter chains with per-filter parameters, the ASCIIHex and
/// RunLength filters (lopdf decodes neither), the identity Crypt filter
/// and the TIFF and PNG predictors (lopdf knows no TIFF predictor and
/// assumes at least 8 bits per component for the PNG ones).
pub fn stream_data(s: &Stream) -> Option<Vec<u8>> {
    let filters: Vec<Vec<u8>> = match s.dict.get(b"Filter") {
        Err(_) => return Some(s.content.clone()),
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a.iter().map(|o| o.as_name().ok().map(|n| n.to_vec())).collect::<Option<_>>()?,
        Ok(_) => return None,
    };
    let parms: Vec<Option<Dictionary>> = match s.dict.get(b"DecodeParms") {
        Ok(Object::Dictionary(d)) => vec![Some(d.clone())],
        Ok(Object::Array(a)) => a.iter().map(|o| o.as_dict().ok().cloned()).collect(),
        _ => Vec::new(),
    };
    let mut data = s.content.clone();
    for (i, f) in filters.iter().enumerate() {
        data = match f.as_slice() {
            b"ASCIIHexDecode" => ascii_hex_decode(&data),
            b"RunLengthDecode" => run_length_decode(&data),
            b"Crypt" => data,
            b"FlateDecode" | b"LZWDecode" | b"ASCII85Decode" => {
                let mut d = Dictionary::new();
                d.set("Filter", Object::Name(f.clone()));
                let mut predictor = None;
                if let Some(Some(p)) = parms.get(i) {
                    let mut p = p.clone();
                    predictor = take_predictor(&mut p);
                    d.set("DecodeParms", Object::Dictionary(p));
                }
                let decoded = Stream::new(d, data).decompressed_content().ok()?;
                match predictor {
                    Some(p) => unpredict(&decoded, p)?,
                    None => decoded,
                }
            }
            _ => return None,
        };
    }
    Some(data)
}

/// A predictor of the Flate and LZW filters.
pub struct Predictor {
    /// 2: TIFF, 10–15: PNG.
    pub kind: i64,
    pub colors: usize,
    pub bits: usize,
    pub columns: usize,
}

/// Take the predictor out of filter parameters (`None`: there is none).
pub fn take_predictor(p: &mut Dictionary) -> Option<Predictor> {
    let kind = p.get(b"Predictor").ok()?.as_i64().ok()?;
    p.remove(b"Predictor");
    if kind <= 1 {
        return None;
    }
    let mut values = [1i64, 8, 1];
    for (slot, key) in values.iter_mut().zip([b"Colors".as_slice(), b"BitsPerComponent", b"Columns"]) {
        if let Ok(Object::Integer(v)) = p.get(key) {
            *slot = (*v).clamp(1, 1 << 20);
        }
    }
    Some(Predictor { kind, colors: values[0] as usize, bits: values[1] as usize, columns: values[2] as usize })
}

/// Undo a TIFF (2) or PNG (10–15) predictor. `None` for a predictor this
/// does not know (TIFF below 8 bits per component, an invalid PNG row).
pub fn unpredict(data: &[u8], p: Predictor) -> Option<Vec<u8>> {
    let bpp = (p.colors * p.bits).div_ceil(8);
    let row = (p.colors * p.bits * p.columns).div_ceil(8);
    match p.kind {
        2 if p.bits == 8 => {
            let mut out = data.to_vec();
            for line in out.chunks_mut(row) {
                for i in bpp..line.len() {
                    line[i] = line[i].wrapping_add(line[i - bpp]);
                }
            }
            Some(out)
        }
        2 if p.bits == 16 => {
            let mut out = data.to_vec();
            for line in out.chunks_mut(row) {
                for i in (bpp..line.len().saturating_sub(1)).step_by(2) {
                    let sum = u16::from_be_bytes([line[i], line[i + 1]]).wrapping_add(u16::from_be_bytes([line[i - bpp], line[i - bpp + 1]]));
                    line[i..i + 2].copy_from_slice(&sum.to_be_bytes());
                }
            }
            Some(out)
        }
        10..=15 => {
            let mut out: Vec<u8> = Vec::with_capacity(data.len());
            for line in data.chunks(row + 1) {
                let start = out.len();
                let filter = line[0];
                for (i, &x) in line[1..].iter().enumerate() {
                    let a = if i >= bpp { out[start + i - bpp] } else { 0 };
                    let b = if start >= row { out[start + i - row] } else { 0 };
                    let c = if i >= bpp && start >= row { out[start + i - row - bpp] } else { 0 };
                    let predicted = match filter {
                        0 => 0,
                        1 => a,
                        2 => b,
                        3 => ((a as u16 + b as u16) / 2) as u8,
                        4 => paeth(a, b, c),
                        _ => return None,
                    };
                    out.push(x.wrapping_add(predicted));
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// The Paeth predictor of PNG: of left, above and upper left the value
/// closest to left + above − upper left.
pub fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let (pa, pb, pc) = ((p - a as i32).abs(), (p - b as i32).abs(), (p - c as i32).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// ASCIIHexDecode: pairs of hex digits, white space ignored, `>` ends the
/// data, a missing last digit counts as 0.
pub fn ascii_hex_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2);
    let mut high: Option<u8> = None;
    for &c in data {
        let v = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            b'>' => break,
            _ => continue,
        };
        match high.take() {
            Some(h) => out.push(h << 4 | v),
            None => high = Some(v),
        }
    }
    if let Some(h) = high {
        out.push(h << 4);
    }
    out
}

/// RunLengthDecode: a length byte below 128 copies the next n+1 bytes,
/// above 128 repeats the next byte 257-n times, 128 ends the data.
pub fn run_length_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let n = data[i] as usize;
        i += 1;
        if n == 128 {
            break;
        }
        if n < 128 {
            let end = (i + n + 1).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else if let Some(&b) = data.get(i) {
            out.extend(std::iter::repeat_n(b, 257 - n));
            i += 1;
        }
    }
    out
}

/// Decoded content of a page (all content streams joined by newlines).
pub fn page_content(doc: &Document, page: ObjectId) -> Vec<u8> {
    let mut out = Vec::new();
    let d = match doc.objects.get(&page).and_then(|o| o.as_dict().ok()) {
        Some(d) => d,
        None => return out,
    };
    let refs: Vec<Object> = match d.get(b"Contents") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(o @ Object::Reference(_)) => match deref(doc, o) {
            Object::Array(a) => a.clone(),
            _ => vec![o.clone()],
        },
        _ => return out,
    };
    for r in refs {
        if let Object::Stream(s) = deref(doc, &r) {
            if let Some(data) = stream_data(s) {
                out.extend_from_slice(&data);
                out.push(b'\n');
            }
        }
    }
    out
}

/// PDF text string (PDFDocEncoding when possible, else UTF-16BE).
pub fn text_string(s: &str) -> Object {
    lopdf::text_string(s)
}

pub fn decode_text(o: &Object) -> Option<String> {
    lopdf::decode_text_string(o).ok()
}

/// Annotation references of a page (resolved array).
pub fn page_annots(doc: &Document, page: ObjectId) -> Vec<Object> {
    let d = match doc.objects.get(&page).and_then(|o| o.as_dict().ok()) {
        Some(d) => d,
        None => return Vec::new(),
    };
    match d.get(b"Annots").ok().map(|o| deref(doc, o)) {
        Some(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// Set the page's /Annots to `annots` (removing the key when empty).
pub fn set_page_annots(doc: &mut Document, page: ObjectId, annots: Vec<Object>) {
    if let Some(d) = dict_mut(doc, page) {
        if annots.is_empty() {
            d.remove(b"Annots");
        } else {
            d.set("Annots", Object::Array(annots));
        }
    }
}

/// Walk a name tree, collecting (key, value) leaves.
pub fn name_tree_leaves(doc: &Document, root: &Object) -> Vec<(Vec<u8>, Object)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.clone(), 0)];
    while let Some((node, depth)) = stack.pop() {
        if depth > 32 {
            continue;
        }
        let Some(d) = dict_of(doc, &node) else { continue };
        if let Some(Object::Array(names)) = d.get(b"Names").ok().map(|o| deref(doc, o)) {
            for pair in names.chunks(2) {
                if let [k, v] = pair {
                    if let Object::String(s, _) = deref(doc, k) {
                        out.push((s.clone(), v.clone()));
                    }
                }
            }
        }
        if let Some(Object::Array(kids)) = d.get(b"Kids").ok().map(|o| deref(doc, o)) {
            for k in kids {
                stack.push((k.clone(), depth + 1));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Flat name tree object from sorted leaves.
pub fn make_name_tree(leaves: Vec<(Vec<u8>, Object)>) -> Dictionary {
    let mut sorted = leaves;
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut arr = Vec::with_capacity(sorted.len() * 2);
    for (k, v) in sorted {
        arr.push(Object::String(k, lopdf::StringFormat::Literal));
        arr.push(v);
    }
    let mut d = Dictionary::new();
    d.set("Names", Object::Array(arr));
    d
}

/// Current time as a PDF date and as an XMP date.
pub fn now_dates() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil(secs);
    (
        format!("D:{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}Z"),
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z"),
    )
}

/// Unix seconds → (year, month, day, hour, minute, second) in UTC.
pub fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, mi, s) = ((rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32);
    // Howard Hinnant's civil_from_days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d, h, mi, s)
}

/// Every object reachable from the trailer.
pub fn reachable(doc: &Document) -> BTreeSet<ObjectId> {
    let mut seen: BTreeSet<ObjectId> = BTreeSet::new();
    let mut stack: Vec<ObjectId> = Vec::new();
    fn refs(o: &Object, out: &mut Vec<ObjectId>) {
        match o {
            Object::Reference(id) => out.push(*id),
            Object::Array(a) => a.iter().for_each(|x| refs(x, out)),
            Object::Dictionary(d) => d.iter().for_each(|(_, v)| refs(v, out)),
            Object::Stream(s) => s.dict.iter().for_each(|(_, v)| refs(v, out)),
            _ => {}
        }
    }
    for (_, v) in doc.trailer.iter() {
        refs(v, &mut stack);
    }
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(o) = doc.objects.get(&id) {
            refs(o, &mut stack);
        }
    }
    seen
}

/// Remove every object not reachable from the trailer.
pub fn prune_unreachable(doc: &mut Document) {
    let seen = reachable(doc);
    doc.objects.retain(|id, _| seen.contains(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(951782400), (2000, 2, 29, 0, 0, 0));
        assert_eq!(civil(1795462245), (2026, 11, 23, 19, 30, 45));
        let (pdf, xmp) = now_dates();
        assert!(pdf.starts_with("D:20") && pdf.ends_with('Z'));
        assert!(xmp.contains('T') && xmp.ends_with('Z'));
    }

    #[test]
    fn name_tree_roundtrip() {
        let mut doc = Document::with_version("1.7");
        let leaf = doc.add_object(Object::Dictionary(make_name_tree(vec![
            (b"b".to_vec(), Object::Integer(2)),
            (b"a".to_vec(), Object::Integer(1)),
        ])));
        let mut root = Dictionary::new();
        root.set("Kids", Object::Array(vec![Object::Reference(leaf)]));
        let leaves = name_tree_leaves(&doc, &Object::Dictionary(root));
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].0, b"a");
    }

    #[test]
    fn predictors_are_undone() {
        use crate::common;
        use lopdf::dictionary;
        let (w, h) = (40usize, 15usize);
        let rgb = common::photo(w, h, 3);
        let stream = |filter: &str, parms: Dictionary, content: Vec<u8>| Stream::new(dictionary! { "Filter" => filter, "DecodeParms" => parms }, content);
        // PNG, every row filter.
        let png = stream("FlateDecode", dictionary! { "Predictor" => 15, "Colors" => 3, "Columns" => w as i64 }, flate(&common::png_predict(&rgb, w * 3, 3)));
        assert_eq!(stream_data(&png).unwrap(), rgb);
        // PNG over half rows of 4-bit components, as Illustrator writes CMYK.
        let cmyk = common::photo(w, h, 4);
        let half = stream("FlateDecode", dictionary! { "Predictor" => 12, "Colors" => 4, "BitsPerComponent" => 4, "Columns" => w as i64 }, flate(&common::png_predict(&cmyk, w * 2, 2)));
        assert_eq!(stream_data(&half).unwrap(), cmyk);
        // TIFF, 8 and 16 bits, behind LZW.
        let mut tiff = rgb.clone();
        for line in tiff.chunks_mut(w * 3) {
            for i in (3..line.len()).rev() {
                line[i] = line[i].wrapping_sub(line[i - 3]);
            }
        }
        let lzw = stream("LZWDecode", dictionary! { "Predictor" => 2, "Colors" => 3, "Columns" => w as i64 }, common::lzw_encode(&tiff));
        assert_eq!(stream_data(&lzw).unwrap(), rgb);
        let wide: Vec<u8> = common::photo(w, h, 2);
        let mut tiff16 = wide.clone();
        for line in tiff16.chunks_mut(w * 2) {
            for i in (2..line.len()).step_by(2).rev() {
                let d = u16::from_be_bytes([line[i], line[i + 1]]).wrapping_sub(u16::from_be_bytes([line[i - 2], line[i - 1]]));
                line[i..i + 2].copy_from_slice(&d.to_be_bytes());
            }
        }
        let deep = stream("FlateDecode", dictionary! { "Predictor" => 2, "BitsPerComponent" => 16, "Columns" => w as i64 }, flate(&tiff16));
        assert_eq!(stream_data(&deep).unwrap(), wide);
        // No predictor, and what cannot be undone.
        let none = stream("FlateDecode", dictionary! { "Predictor" => 1, "Columns" => 7 }, flate(&rgb));
        assert_eq!(stream_data(&none).unwrap(), rgb);
        let bits = stream("FlateDecode", dictionary! { "Predictor" => 2, "BitsPerComponent" => 4, "Columns" => w as i64 }, flate(&rgb));
        assert!(stream_data(&bits).is_none());
        let unknown = stream("FlateDecode", dictionary! { "Predictor" => 7 }, flate(&rgb));
        assert!(stream_data(&unknown).is_none());
        let bad_row = stream("FlateDecode", dictionary! { "Predictor" => 15, "Columns" => 2 }, flate(&[9, 1, 2]));
        assert!(stream_data(&bad_row).is_none());
        assert!(take_predictor(&mut Dictionary::new()).is_none());
        assert_eq!((paeth(10, 20, 30), paeth(30, 10, 20), paeth(10, 30, 20)), (10, 20, 20));
        assert_eq!(paeth(50, 60, 10), 60);
        assert_eq!(paeth(60, 50, 55), 55);
    }

    #[test]
    fn unreachable_objects() {
        let mut doc = Document::with_version("1.7");
        let leaf = doc.add_object(Object::Integer(1));
        let lost = doc.add_object(Object::Integer(2));
        let root = doc.add_object(Object::Array(vec![leaf.into()]));
        doc.trailer.set("Root", root);
        assert_eq!(reachable(&doc), BTreeSet::from([leaf, root]));
        prune_unreachable(&mut doc);
        assert!(!doc.objects.contains_key(&lost) && doc.objects.len() == 2);
    }
}
