//! Page boundaries: PDF/A-2 and -3 only allow boxes between 3 and 14400
//! units in each direction (ISO 19005-2, 6.1.13).
//!
//! Pages that are too large are scaled down (content, boxes, annotations
//! and destinations) and get a `UserUnit` that keeps their physical size.
//! Pages that are too small are widened. The optional boxes are clipped to
//! the MediaBox, and dropped where nothing valid is left.

use lopdf::{Document, Object, ObjectId, Stream};

use crate::content;
use crate::util::{self, deref, dict_mut, num};
use crate::Ctx;

pub const MIN: f64 = 3.0;
pub const MAX: f64 = 14400.0;
/// Target for the longer side of a page that is scaled down (a little
/// below the limit, numbers are stored as 32-bit reals).
const SCALE_TO: f64 = 14398.0;

const OPTIONAL_BOXES: [&[u8]; 4] = [b"CropBox", b"BleedBox", b"TrimBox", b"ArtBox"];

pub fn run(ctx: &mut Ctx) {
    if !ctx.level.limits_page_size() {
        return;
    }
    let mut scaled = Vec::new();
    let mut adjusted = Vec::new();
    for (i, page) in util::pages(&ctx.doc).into_iter().enumerate() {
        match fix_page(&mut ctx.doc, page) {
            Change::Scaled => scaled.push((i + 1).to_string()),
            Change::Boxes => adjusted.push((i + 1).to_string()),
            Change::None => {}
        }
    }
    if !scaled.is_empty() {
        ctx.report.note("pages_scaled", scaled.join(", "));
    }
    if !adjusted.is_empty() {
        ctx.report.note("page_boxes_adjusted", adjusted.join(", "));
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Change {
    None,
    Boxes,
    Scaled,
}

/// A box value as normalised corners.
fn read_box(doc: &Document, o: &Object) -> Option<[f64; 4]> {
    let Object::Array(a) = deref(doc, o) else { return None };
    if a.len() != 4 {
        return None;
    }
    let v: Vec<f64> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
    if v.len() != 4 || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

fn box_object(b: [f64; 4]) -> Object {
    Object::Array(b.iter().map(|v| real(*v)).collect())
}

fn real(v: f64) -> Object {
    if v.fract() == 0.0 && v.abs() < 1e9 {
        Object::Integer(v as i64)
    } else {
        Object::Real(v as f32)
    }
}

/// `b` clipped to `media`; `None` when less than the minimum is left.
pub fn clip(b: [f64; 4], media: [f64; 4]) -> Option<[f64; 4]> {
    let c = [b[0].max(media[0]), b[1].max(media[1]), b[2].min(media[2]), b[3].min(media[3])];
    (c[2] - c[0] >= MIN && c[3] - c[1] >= MIN).then_some(c)
}

/// Scale factor for a MediaBox, when it is larger than allowed.
pub fn scale_for(media: [f64; 4]) -> Option<f64> {
    let longest = (media[2] - media[0]).max(media[3] - media[1]);
    (longest > MAX).then(|| SCALE_TO / longest)
}

fn fix_page(doc: &mut Document, page: ObjectId) -> Change {
    let mut change = Change::None;
    let original = util::inherited(doc, page, b"MediaBox");
    let mut media = original.as_ref().and_then(|o| read_box(doc, o)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let had_valid_media = original.as_ref().and_then(|o| read_box(doc, o)).is_some();

    // Boxes as the page has them now (CropBox may be inherited).
    let mut boxes: Vec<(&[u8], Option<[f64; 4]>, bool)> = Vec::new();
    for key in OPTIONAL_BOXES {
        let own = doc.objects.get(&page).and_then(|o| o.as_dict().ok()).and_then(|d| d.get(key).ok()).cloned();
        let value = match (&own, key == b"CropBox") {
            (Some(o), _) => Some(o.clone()),
            (None, true) => util::inherited(doc, page, key),
            (None, false) => None,
        };
        let present = value.is_some();
        boxes.push((key, value.as_ref().and_then(|o| read_box(doc, o)), present));
    }

    let scale = scale_for(media);
    if let Some(s) = scale {
        for v in media.iter_mut() {
            *v *= s;
        }
        for b in boxes.iter_mut() {
            if let Some(bx) = b.1.as_mut() {
                for v in bx.iter_mut() {
                    *v *= s;
                }
            }
        }
        scale_content(doc, page, s);
        scale_annotations(doc, page, s);
        scale_destinations(doc, page, s);
        let unit = doc.objects.get(&page).and_then(|o| o.as_dict().ok()).and_then(|d| util::get_num(doc, d, b"UserUnit")).filter(|u| *u > 0.0).unwrap_or(1.0);
        if let Some(d) = dict_mut(doc, page) {
            d.set("UserUnit", Object::Real((unit / s) as f32));
        }
        change = Change::Scaled;
    }
    // Too small: widen to the minimum.
    let mut widened = false;
    if media[2] - media[0] < MIN {
        media[2] = media[0] + MIN;
        widened = true;
    }
    if media[3] - media[1] < MIN {
        media[3] = media[1] + MIN;
        widened = true;
    }
    if scale.is_some() || widened || !had_valid_media {
        if let Some(d) = dict_mut(doc, page) {
            d.set("MediaBox", box_object(media));
        }
        if change == Change::None && (widened || !had_valid_media) {
            change = Change::Boxes;
        }
    }

    for (key, value, present) in boxes {
        if !present {
            continue;
        }
        let fixed = value.and_then(|b| clip(b, media));
        let unchanged = scale.is_none() && fixed.is_some() && fixed == value;
        if unchanged {
            continue;
        }
        if let Some(d) = dict_mut(doc, page) {
            match fixed {
                Some(b) => d.set(key, box_object(b)),
                // An inherited CropBox would come back; override it.
                None if key == b"CropBox" => d.set(key, box_object(media)),
                None => {
                    d.remove(key);
                }
            }
        }
        if change == Change::None {
            change = Change::Boxes;
        }
    }
    change
}

/// Prepend a scaling matrix to the page content.
fn scale_content(doc: &mut Document, page: ObjectId, s: f64) {
    let mut c = Vec::new();
    content::write_real(&mut c, s);
    c.extend_from_slice(b" 0 0 ");
    content::write_real(&mut c, s);
    c.extend_from_slice(b" 0 0 cm\n");
    let pre = doc.add_object(Object::Stream(Stream::new(lopdf::Dictionary::new(), c)));
    let existing: Vec<Object> = match doc.objects.get(&page).and_then(|o| o.as_dict().ok()).and_then(|d| d.get(b"Contents").ok()) {
        Some(Object::Array(a)) => a.clone(),
        Some(o @ Object::Reference(_)) => match deref(doc, o) {
            Object::Array(a) => a.clone(),
            Object::Stream(_) => vec![o.clone()],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let mut all = vec![Object::Reference(pre)];
    all.extend(existing);
    if let Some(d) = dict_mut(doc, page) {
        d.set("Contents", Object::Array(all));
    }
}

fn scale_numbers(o: &mut Object, s: f64) {
    match o {
        Object::Integer(i) => *o = real(*i as f64 * s),
        Object::Real(r) => *o = Object::Real((*r as f64 * s) as f32),
        Object::Array(a) => a.iter_mut().for_each(|x| scale_numbers(x, s)),
        _ => {}
    }
}

/// Annotation geometry lives in page space; appearance streams follow
/// their /Rect by themselves.
fn scale_annotations(doc: &mut Document, page: ObjectId, s: f64) {
    const KEYS: [&[u8]; 7] = [b"Rect", b"QuadPoints", b"L", b"Vertices", b"InkList", b"CL", b"RD"];
    let annots = util::page_annots(doc, page);
    let mut direct = Vec::new();
    for a in annots {
        match a {
            Object::Reference(id) => {
                for key in KEYS {
                    let Some(v) = doc.objects.get(&id).and_then(|o| o.as_dict().ok()).and_then(|d| d.get(key).ok()).map(|o| deref(doc, o).clone()) else { continue };
                    let mut v = v;
                    scale_numbers(&mut v, s);
                    if let Some(d) = dict_mut(doc, id) {
                        d.set(key, v);
                    }
                }
                direct.push(Object::Reference(id));
            }
            Object::Dictionary(mut d) => {
                for key in KEYS {
                    if let Ok(v) = d.get_mut(key) {
                        scale_numbers(v, s);
                    }
                }
                direct.push(Object::Dictionary(d));
            }
            other => direct.push(other),
        }
    }
    util::set_page_annots(doc, page, direct);
}

/// Explicit destinations `[page /XYZ left top zoom]` etc. that point at
/// the page carry coordinates in its (now scaled) space.
fn scale_destinations(doc: &mut Document, page: ObjectId, s: f64) {
    fn walk(o: &mut Object, page: ObjectId, s: f64, depth: u32) {
        if depth > 32 {
            return;
        }
        match o {
            Object::Array(a) => {
                let is_dest = a.len() >= 2 && matches!(a[0], Object::Reference(p) if p == page) && matches!(&a[1], Object::Name(_));
                if is_dest {
                    let kind = match &a[1] {
                        Object::Name(n) => n.clone(),
                        _ => Vec::new(),
                    };
                    // The zoom factor of /XYZ stays as it is.
                    let coords = match kind.as_slice() {
                        b"XYZ" => 2,
                        b"FitH" | b"FitV" | b"FitBH" | b"FitBV" => 1,
                        b"FitR" => 4,
                        _ => 0,
                    };
                    for x in a.iter_mut().skip(2).take(coords) {
                        scale_numbers(x, s);
                    }
                } else {
                    a.iter_mut().for_each(|x| walk(x, page, s, depth + 1));
                }
            }
            Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| walk(v, page, s, depth + 1)),
            Object::Stream(st) => st.dict.iter_mut().for_each(|(_, v)| walk(v, page, s, depth + 1)),
            _ => {}
        }
    }
    for (id, o) in doc.objects.iter_mut() {
        if *id != page {
            walk(o, page, s, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn doc_with_page(extra: lopdf::Dictionary) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let c = doc.add_object(Stream::new(lopdf::Dictionary::new(), b"0 0 10 10 re f".to_vec()));
        let mut d = dictionary! { "Type" => "Page", "Parent" => pages, "Contents" => c };
        for (k, v) in extra.iter() {
            d.set(k.clone(), v.clone());
        }
        let page = doc.add_object(d);
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1, "CropBox" => vec![0.into(), 0.into(), 1.into(), 1.into()] }));
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", cat);
        (doc, page)
    }

    fn page_box(doc: &Document, page: ObjectId, key: &[u8]) -> Option<[f64; 4]> {
        doc.objects.get(&page)?.as_dict().ok()?.get(key).ok().and_then(|o| read_box(doc, o))
    }

    #[test]
    fn limits() {
        assert_eq!(clip([-10.0, -10.0, 700.0, 900.0], [0.0, 0.0, 595.0, 842.0]), Some([0.0, 0.0, 595.0, 842.0]));
        assert_eq!(clip([0.0, 0.0, 595.0, 2.0], [0.0, 0.0, 595.0, 842.0]), None);
        assert_eq!(clip([600.0, 0.0, 700.0, 100.0], [0.0, 0.0, 595.0, 842.0]), None);
        assert_eq!(scale_for([0.0, 0.0, 595.0, 842.0]), None);
        assert_eq!(scale_for([0.0, 0.0, 14400.0, 100.0]), None);
        let s = scale_for([0.0, 0.0, 28796.0, 100.0]).unwrap();
        assert!((s - 0.5).abs() < 1e-9);
        assert_eq!(real(3.0), Object::Integer(3));
        assert_eq!(real(1.5), Object::Real(1.5));
    }

    #[test]
    fn large_page_is_scaled_with_annotations_and_destinations() {
        let (mut doc, _) = doc_with_page(lopdf::Dictionary::new());
        let page = util::pages(&doc)[0];
        let annot = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => vec![1000.into(), 2000.into(), 3000.into(), 4000.into()], "InkList" => vec![Object::Array(vec![10.into(), 20.into()])] });
        let inline = Object::Dictionary(dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 28796.into(), 100.into()] });
        if let Some(d) = dict_mut(&mut doc, page) {
            d.set("MediaBox", vec![0.into(), 0.into(), 28796.into(), 20000.into()]);
            d.set("CropBox", vec![100.into(), 100.into(), 20000.into(), 20000.into()]);
            d.set("Annots", vec![annot.into(), inline, Object::Null]);
            d.set("UserUnit", 2);
        }
        let dest = doc.add_object(Object::Array(vec![page.into(), "XYZ".into(), 1000.into(), 2000.into(), 3.into()]));
        let fit = doc.add_object(dictionary! { "D" => vec![page.into(), "FitH".into(), 500.into()], "Other" => vec![Object::Reference((999, 0)), "XYZ".into(), 10.into()] });
        let fitr = doc.add_object(Object::Array(vec![page.into(), "FitR".into(), 2.into(), 4.into(), 6.into(), 8.into()]));
        assert_eq!(fix_page(&mut doc, page), Change::Scaled);
        assert_eq!(page_box(&doc, page, b"MediaBox"), Some([0.0, 0.0, 14398.0, 10000.0]));
        assert_eq!(page_box(&doc, page, b"CropBox"), Some([50.0, 50.0, 10000.0, 10000.0]));
        let d = doc.objects.get(&page).unwrap().as_dict().unwrap();
        assert_eq!(util::get_num(&doc, d, b"UserUnit"), Some(4.0));
        let first = match d.get(b"Contents").unwrap() {
            Object::Array(a) => a[0].clone(),
            _ => panic!("contents not an array"),
        };
        assert_eq!(a_len(d.get(b"Contents").unwrap()), 2);
        let Object::Stream(pre) = deref(&doc, &first) else { panic!() };
        assert_eq!(pre.content, b"0.5 0 0 0.5 0 0 cm\n");
        let a = doc.objects.get(&annot).unwrap().as_dict().unwrap();
        assert_eq!(read_box(&doc, a.get(b"Rect").unwrap()), Some([500.0, 1000.0, 1500.0, 2000.0]));
        assert_eq!(a.get(b"InkList").unwrap(), &Object::Array(vec![Object::Array(vec![5.into(), 10.into()])]));
        let annots = util::page_annots(&doc, page);
        assert_eq!(annots.len(), 3);
        let Object::Dictionary(l) = &annots[1] else { panic!() };
        assert_eq!(read_box(&doc, l.get(b"Rect").unwrap()), Some([0.0, 0.0, 14398.0, 50.0]));
        assert_eq!(doc.objects.get(&dest).unwrap(), &Object::Array(vec![page.into(), "XYZ".into(), 500.into(), 1000.into(), 3.into()]));
        let f = doc.objects.get(&fit).unwrap().as_dict().unwrap();
        assert_eq!(f.get(b"D").unwrap(), &Object::Array(vec![page.into(), "FitH".into(), 250.into()]));
        assert_eq!(f.get(b"Other").unwrap(), &Object::Array(vec![Object::Reference((999, 0)), "XYZ".into(), 10.into()]));
        assert_eq!(doc.objects.get(&fitr).unwrap(), &Object::Array(vec![page.into(), "FitR".into(), 1.into(), 2.into(), 3.into(), 4.into()]));
    }

    fn a_len(o: &Object) -> usize {
        match o {
            Object::Array(a) => a.len(),
            _ => 0,
        }
    }

    #[test]
    fn small_page_and_bad_boxes() {
        // 2×2 page: widened; the inherited 1×1 CropBox is overridden.
        let (mut doc, page) = doc_with_page(dictionary! { "MediaBox" => vec![0.into(), 0.into(), 2.into(), 2.into()], "ArtBox" => vec![0.into(), 0.into(), 1.into(), 1.into()] });
        assert_eq!(fix_page(&mut doc, page), Change::Boxes);
        assert_eq!(page_box(&doc, page, b"MediaBox"), Some([0.0, 0.0, 3.0, 3.0]));
        assert_eq!(page_box(&doc, page, b"CropBox"), Some([0.0, 0.0, 3.0, 3.0]));
        assert_eq!(page_box(&doc, page, b"ArtBox"), None);

        // Reversed corners and boxes beyond the MediaBox; no MediaBox at all.
        let (mut doc, page) = doc_with_page(dictionary! {
            "CropBox" => vec![700.into(), 900.into(), (-100).into(), (-100).into()],
            "TrimBox" => vec![10.into(), 10.into(), 500.into(), 500.into()],
            "BleedBox" => "nonsense",
        });
        assert_eq!(fix_page(&mut doc, page), Change::Boxes);
        assert_eq!(page_box(&doc, page, b"MediaBox"), Some([0.0, 0.0, 612.0, 792.0]));
        assert_eq!(page_box(&doc, page, b"CropBox"), Some([0.0, 0.0, 612.0, 792.0]));
        assert_eq!(page_box(&doc, page, b"TrimBox"), Some([10.0, 10.0, 500.0, 500.0]));
        assert!(!doc.objects.get(&page).unwrap().as_dict().unwrap().has(b"BleedBox"));

        // Nothing to do.
        let (mut doc, page) = doc_with_page(dictionary! { "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()], "CropBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] });
        assert_eq!(fix_page(&mut doc, page), Change::None);
    }

    #[test]
    fn contents_shapes() {
        // Contents as a referenced array, and missing contents.
        let (mut doc, page) = doc_with_page(lopdf::Dictionary::new());
        let s = doc.add_object(Stream::new(lopdf::Dictionary::new(), b"n".to_vec()));
        let arr = doc.add_object(Object::Array(vec![s.into()]));
        dict_mut(&mut doc, page).unwrap().set("Contents", arr);
        scale_content(&mut doc, page, 0.25);
        assert_eq!(a_len(doc.objects.get(&page).unwrap().as_dict().unwrap().get(b"Contents").unwrap()), 2);
        dict_mut(&mut doc, page).unwrap().remove(b"Contents");
        scale_content(&mut doc, page, 0.25);
        assert_eq!(a_len(doc.objects.get(&page).unwrap().as_dict().unwrap().get(b"Contents").unwrap()), 1);
        dict_mut(&mut doc, page).unwrap().set("Contents", Object::Reference((4242, 0)));
        scale_content(&mut doc, page, 0.25);
        assert_eq!(a_len(doc.objects.get(&page).unwrap().as_dict().unwrap().get(b"Contents").unwrap()), 1);
    }
}
