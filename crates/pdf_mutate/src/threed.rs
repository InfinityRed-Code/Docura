//! 3D annotations (ISO 32000 13.6): reading the annotation, its views and
//! the embedded model, and writing a new 3D annotation with a U3D model,
//! preset views and a poster image.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use pdf_types::{RectPt, ThreeDAnnot, ThreeDCrossSection, ThreeDNodeState, ThreeDView};

use crate::annotations::{
    annot_id_string, append_annot_to_page, now_pdf_date, page_id_for, page_view, pdf_string_utf16be, rect_native,
};
use crate::read::{decode_pdf_string, read_rect_top, read_string_field};
use crate::{MutateError, MutateResult};

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(o),
        _ => o,
    }
}

fn get<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    d.get(key).ok().map(|o| deref(doc, o))
}

fn dict<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    match get(doc, d, key)? {
        Object::Dictionary(x) => Some(x),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

fn name(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    get(doc, d, key)?.as_name().ok().map(|n| String::from_utf8_lossy(n).into_owned())
}

fn num(o: &Object) -> Option<f32> {
    o.as_float().ok().or_else(|| o.as_i64().ok().map(|i| i as f32))
}

fn number(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<f32> {
    num(get(doc, d, key)?)
}

fn boolean(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<bool> {
    get(doc, d, key)?.as_bool().ok()
}

fn text(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    match get(doc, d, key)? {
        Object::String(s, _) => Some(decode_pdf_string(s)),
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => None,
    }
}

fn numbers<const N: usize>(doc: &Document, o: &Object) -> Option<[f32; N]> {
    let a = deref(doc, o).as_array().ok()?;
    if a.len() < N {
        return None;
    }
    let mut out = [0f32; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = num(deref(doc, &a[i]))?;
    }
    Some(out)
}

/// `[/DeviceRGB r g b]` or `[r g b]`.
fn rgb(doc: &Document, o: &Object) -> Option<[f32; 3]> {
    let a = deref(doc, o).as_array().ok()?;
    let vals: Vec<f32> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
    (vals.len() >= 3).then(|| [vals[0], vals[1], vals[2]])
}

fn read_view(doc: &Document, d: &Dictionary, index: usize) -> ThreeDView {
    let mut v = ThreeDView {
        name: text(doc, d, b"XN").unwrap_or_else(|| format!("{}", index + 1)),
        internal_name: text(doc, d, b"IN"),
        ..Default::default()
    };
    match name(doc, d, b"MS").as_deref() {
        Some("M") => v.c2w = get(doc, d, b"C2W").and_then(|o| numbers::<12>(doc, o)),
        Some("U3D") => {
            v.u3d_path = match get(doc, d, b"U3DPath") {
                Some(Object::Array(a)) => a
                    .iter()
                    .filter_map(|x| match deref(doc, x) {
                        Object::String(s, _) => Some(decode_pdf_string(s)),
                        _ => None,
                    })
                    .collect(),
                Some(Object::String(s, _)) => vec![decode_pdf_string(s)],
                _ => Vec::new(),
            }
        }
        _ => {}
    }
    v.center_of_orbit = number(doc, d, b"CO").filter(|c| c.is_finite() && *c >= 0.0);
    if let Some(p) = dict(doc, d, b"P") {
        v.orthographic = name(doc, p, b"Subtype").as_deref() == Some("O");
        v.fov = number(doc, p, b"FOV");
        v.fov_span = match get(doc, p, b"PS") {
            Some(Object::Name(n)) => Some(String::from_utf8_lossy(n).into_owned()),
            Some(o) => num(o).map(|x| x.to_string()),
            None => None,
        };
        v.ortho_scale = number(doc, p, b"OS");
        v.ortho_binding = name(doc, p, b"OB");
    }
    if let Some(bg) = dict(doc, d, b"BG") {
        v.background = Some(get(doc, bg, b"C").and_then(|c| rgb(doc, c)).unwrap_or([1.0, 1.0, 1.0]));
    }
    if let Some(rm) = dict(doc, d, b"RM") {
        v.render_mode = name(doc, rm, b"Subtype");
        v.render_aux_color = get(doc, rm, b"AC").and_then(|c| rgb(doc, c));
    }
    if let Some(ls) = dict(doc, d, b"LS") {
        v.lighting = name(doc, ls, b"Subtype");
    }
    if let Some(Object::Array(sa)) = get(doc, d, b"SA") {
        for s in sa.iter().filter_map(|x| deref(doc, x).as_dict().ok()) {
            let mut orientation = [None, Some(0.0), Some(0.0)];
            if let Some(Object::Array(o)) = get(doc, s, b"O") {
                for (i, slot) in orientation.iter_mut().enumerate() {
                    *slot = o.get(i).and_then(|x| num(deref(doc, x)));
                }
            }
            v.cross_sections.push(ThreeDCrossSection {
                center: get(doc, s, b"C").and_then(|c| numbers::<3>(doc, c)).unwrap_or_default(),
                orientation,
                plane_opacity: number(doc, s, b"PO").unwrap_or(0.5),
                plane_color: get(doc, s, b"PC").and_then(|c| rgb(doc, c)).unwrap_or([1.0, 1.0, 1.0]),
                plane_visible: boolean(doc, s, b"PV").unwrap_or(true),
                intersection_visible: boolean(doc, s, b"IV").unwrap_or(false),
                intersection_color: get(doc, s, b"IC").and_then(|c| rgb(doc, c)).unwrap_or([0.0, 1.0, 0.0]),
            });
        }
    }
    if let Some(Object::Array(na)) = get(doc, d, b"NA") {
        for n in na.iter().filter_map(|x| deref(doc, x).as_dict().ok()) {
            let Some(node) = text(doc, n, b"N") else { continue };
            v.nodes.push(ThreeDNodeState {
                name: node,
                visible: boolean(doc, n, b"V"),
                opacity: number(doc, n, b"O"),
                matrix: get(doc, n, b"M").and_then(|m| numbers::<12>(doc, m)),
            });
        }
    }
    v
}

/// The 3D stream of a 3D annotation (`/3DD` directly or via a 3D
/// reference dictionary).
fn stream_of<'a>(doc: &'a Document, annot: &'a Dictionary) -> Option<&'a Stream> {
    match get(doc, annot, b"3DD")? {
        Object::Stream(s) => Some(s),
        Object::Dictionary(r) => get(doc, r, b"3DD")?.as_stream().ok(),
        _ => None,
    }
}

/// RichMedia: the first 3D instance's asset (an embedded file stream) and
/// the content's views.
fn richmedia_asset<'a>(doc: &'a Document, annot: &'a Dictionary) -> Option<(&'a Stream, Option<String>)> {
    let content = dict(doc, annot, b"RichMediaContent")?;
    let Object::Array(configs) = get(doc, content, b"Configurations")? else { return None };
    for c in configs.iter().filter_map(|x| deref(doc, x).as_dict().ok()) {
        if name(doc, c, b"Subtype").as_deref() != Some("3D") {
            continue;
        }
        let Some(Object::Array(instances)) = get(doc, c, b"Instances") else { continue };
        for inst in instances.iter().filter_map(|x| deref(doc, x).as_dict().ok()) {
            let Some(spec) = dict(doc, inst, b"Asset") else { continue };
            let file_name = text(doc, spec, b"UF").or_else(|| text(doc, spec, b"F"));
            let ef = dict(doc, spec, b"EF")?;
            let stream = get(doc, ef, b"F").or_else(|| get(doc, ef, b"UF"))?.as_stream().ok()?;
            return Some((stream, file_name));
        }
    }
    None
}

fn stream_bytes(s: &Stream) -> Vec<u8> {
    if s.dict.get(b"Filter").is_ok() {
        s.decompressed_content().unwrap_or_else(|_| s.content.clone())
    } else {
        s.content.clone()
    }
}

/// Sniffs the model format from the data (U3D starts with "U3D\0", PRC
/// with "PRC").
fn sniff(bytes: &[u8], declared: Option<String>) -> String {
    if bytes.starts_with(b"U3D\0") {
        "U3D".into()
    } else if bytes.starts_with(b"PRC") {
        "PRC".into()
    } else if bytes.starts_with(b"glTF") || bytes.starts_with(b"{") {
        "glTF".into()
    } else {
        declared.unwrap_or_else(|| "unknown".into())
    }
}

/// Resolves a `/3DV` or `/DV` value against `views`; dictionaries not in
/// the list are appended.
fn resolve_view(doc: &Document, o: &Object, views: &mut Vec<ThreeDView>) -> Option<u32> {
    match deref(doc, o) {
        Object::Integer(i) => (*i >= 0 && (*i as usize) < views.len()).then_some(*i as u32),
        Object::Name(n) => match n.as_slice() {
            b"F" => (!views.is_empty()).then_some(0),
            b"L" => views.len().checked_sub(1).map(|i| i as u32),
            _ => None, // "D": the stream's default, handled by the caller
        },
        Object::String(s, _) => {
            let n = decode_pdf_string(s);
            views.iter().position(|v| v.internal_name.as_deref() == Some(n.as_str())).map(|i| i as u32)
        }
        Object::Dictionary(d) => {
            views.push(read_view(doc, d, views.len()));
            Some(views.len() as u32 - 1)
        }
        _ => None,
    }
}

fn is_3d(doc: &Document, d: &Dictionary) -> bool {
    match name(doc, d, b"Subtype").as_deref() {
        Some("3D") => true,
        Some("RichMedia") => richmedia_asset(doc, d).is_some(),
        _ => false,
    }
}

/// Every 3D annotation of the document (top-origin rects, like
/// `list_annotations`).
pub fn list_3d(doc: &Document) -> Vec<ThreeDAnnot> {
    let mut out = Vec::new();
    for (page_idx, page_id) in doc.page_iter().enumerate() {
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        let Some(Object::Array(annots)) = page.get(b"Annots").ok().map(|o| deref(doc, o)) else { continue };
        let (vb, rot) = page_view(doc, page_id);
        for entry in annots {
            let (oid, d) = match entry {
                Object::Reference(r) => match doc.get_dictionary(*r) {
                    Ok(d) => (Some(*r), d),
                    Err(_) => continue,
                },
                Object::Dictionary(d) => (None, d),
                _ => continue,
            };
            if !is_3d(doc, d) {
                continue;
            }
            let Some(rect_pt) = read_rect_top(doc, d, vb, rot) else { continue };
            if let Some(info) = describe(doc, d, page_idx as u32, rect_pt, annot_id_string(doc, d, oid)) {
                out.push(info);
            }
        }
    }
    out
}

fn describe(doc: &Document, d: &Dictionary, page: u32, rect_pt: RectPt, id: String) -> Option<ThreeDAnnot> {
    let subtype = name(doc, d, b"Subtype")?;
    let has_poster = dict(doc, d, b"AP")
        .and_then(|ap| get(doc, ap, b"N"))
        .is_some_and(|n| matches!(n, Object::Stream(s) if !s.content.is_empty()));
    let mut views = Vec::new();
    let default_view;
    let (bytes, declared, activation_dict) = if subtype == "3D" {
        let s = stream_of(doc, d)?;
        if let Some(Object::Array(va)) = get(doc, &s.dict, b"VA") {
            for (i, v) in va.iter().filter_map(|x| deref(doc, x).as_dict().ok()).enumerate() {
                views.push(read_view(doc, v, i));
            }
        }
        // Precedence: annotation /3DV, stream /DV, first preset view.
        let stream_dv = get(doc, &s.dict, b"DV").and_then(|o| resolve_view(doc, o, &mut views));
        default_view = d
            .get(b"3DV")
            .ok()
            .and_then(|o| resolve_view(doc, o, &mut views))
            .or(stream_dv)
            .or((!views.is_empty()).then_some(0));
        (stream_bytes(s), name(doc, &s.dict, b"Subtype"), dict(doc, d, b"3DA"))
    } else {
        let (s, file) = richmedia_asset(doc, d)?;
        if let Some(content) = dict(doc, d, b"RichMediaContent") {
            if let Some(Object::Array(va)) = get(doc, content, b"Views") {
                for (i, v) in va.iter().filter_map(|x| deref(doc, x).as_dict().ok()).enumerate() {
                    views.push(read_view(doc, v, i));
                }
            }
        }
        default_view = (!views.is_empty()).then_some(0);
        let ext = file.and_then(|f| f.rsplit('.').next().map(|e| e.to_ascii_uppercase()));
        let activation = dict(doc, d, b"RichMediaSettings").and_then(|s| dict(doc, s, b"Activation"));
        (stream_bytes(s), ext, activation)
    };
    let (activation, toolbar, model_tree) = match activation_dict {
        Some(a) => (
            name(doc, a, b"A").or_else(|| name(doc, a, b"Condition")).unwrap_or_else(|| "XA".into()),
            boolean(doc, a, b"TB").unwrap_or(true),
            boolean(doc, a, b"NP").unwrap_or(false),
        ),
        None => ("XA".into(), true, false),
    };
    Some(ThreeDAnnot {
        id,
        page,
        rect_pt,
        subtype,
        format: sniff(&bytes, declared),
        model_size: bytes.len(),
        has_poster,
        activation,
        toolbar,
        model_tree,
        views,
        default_view,
        contents: read_string_field(doc, d, b"Contents"),
    })
}

/// The decoded model data of the 3D annotation `id` on `page_index`.
pub fn model_3d(doc: &Document, page_index: u32, id: &str) -> MutateResult<(String, Vec<u8>)> {
    let page_id = page_id_for(doc, page_index)?;
    let page = doc.get_dictionary(page_id).map_err(|e| MutateError::Parse(e.to_string()))?;
    let Some(Object::Array(annots)) = page.get(b"Annots").ok().map(|o| deref(doc, o)) else {
        return Err(MutateError::Parse("page has no annotations".into()));
    };
    for entry in annots {
        let (oid, d) = match entry {
            Object::Reference(r) => match doc.get_dictionary(*r) {
                Ok(d) => (Some(*r), d),
                Err(_) => continue,
            },
            Object::Dictionary(d) => (None, d),
            _ => continue,
        };
        if annot_id_string(doc, d, oid) != id || !is_3d(doc, d) {
            continue;
        }
        let (bytes, declared) = if name(doc, d, b"Subtype").as_deref() == Some("3D") {
            let s = stream_of(doc, d).ok_or_else(|| MutateError::Parse("3D annotation without model".into()))?;
            (stream_bytes(s), name(doc, &s.dict, b"Subtype"))
        } else {
            let (s, _) = richmedia_asset(doc, d).ok_or_else(|| MutateError::Parse("RichMedia without 3D asset".into()))?;
            (stream_bytes(s), None)
        };
        return Ok((sniff(&bytes, declared), bytes));
    }
    Err(MutateError::Parse(format!("3D annotation {id} not found")))
}

/// A new 3D annotation.
#[derive(Debug, Clone)]
pub struct ThreeDAnnotation {
    /// Placement in top-origin view space.
    pub rect: RectPt,
    /// U3D model data.
    pub u3d: Vec<u8>,
    /// Preset views; `default_view` indexes into them.
    pub views: Vec<ThreeDView>,
    pub default_view: usize,
    /// Poster shown while the annotation is inactive (any common image
    /// format; stored as JPEG).
    pub poster: Vec<u8>,
    /// Shown as /Contents (e.g. the model's file name).
    pub name: String,
    /// "XA" (click to activate), "PO" or "PV".
    pub activation: String,
}

fn real(v: f32) -> Object {
    Object::Real(if v.is_finite() { v } else { 0.0 })
}

fn rgb_array(c: [f32; 3]) -> Object {
    Object::Array(vec![Object::Name(b"DeviceRGB".to_vec()), real(c[0]), real(c[1]), real(c[2])])
}

fn write_view(v: &ThreeDView) -> Dictionary {
    let mut d = dictionary! {
        "Type" => "3DView",
        "XN" => pdf_string_utf16be(&v.name),
    };
    if let Some(n) = &v.internal_name {
        d.set("IN", pdf_string_utf16be(n));
    }
    if let Some(m) = v.c2w {
        d.set("MS", "M");
        d.set("C2W", Object::Array(m.iter().map(|x| real(*x)).collect()));
        if let Some(co) = v.center_of_orbit {
            d.set("CO", real(co));
        }
    }
    let mut p = Dictionary::new();
    if v.orthographic {
        p.set("Subtype", "O");
        p.set("OS", real(v.ortho_scale.unwrap_or(1.0)));
        p.set("OB", Object::Name(v.ortho_binding.clone().unwrap_or_else(|| "Absolute".into()).into_bytes()));
    } else {
        p.set("Subtype", "P");
        p.set("FOV", real(v.fov.unwrap_or(30.0)));
        p.set("PS", Object::Name(v.fov_span.clone().unwrap_or_else(|| "Min".into()).into_bytes()));
    }
    d.set("P", p);
    d.set(
        "BG",
        dictionary! { "Type" => "3DBG", "Subtype" => "SC", "C" => Object::Array(v.background.unwrap_or([1.0; 3]).iter().map(|x| real(*x)).collect()) },
    );
    let mut rm = dictionary! {
        "Type" => "3DRenderMode",
        "Subtype" => Object::Name(v.render_mode.clone().unwrap_or_else(|| "Solid".into()).into_bytes()),
    };
    if let Some(ac) = v.render_aux_color {
        rm.set("AC", rgb_array(ac));
    }
    d.set("RM", rm);
    d.set(
        "LS",
        dictionary! { "Type" => "3DLightingScheme", "Subtype" => Object::Name(v.lighting.clone().unwrap_or_else(|| "CAD".into()).into_bytes()) },
    );
    d
}

/// Adds a 3D annotation with a U3D model to `page_index`.
pub fn add_3d_annotation(doc: &mut Document, page_index: u32, a: &ThreeDAnnotation) -> MutateResult<ObjectId> {
    use std::io::Write;
    if !a.u3d.starts_with(b"U3D\0") {
        return Err(MutateError::Parse("3D model is not U3D".into()));
    }
    let page_id = page_id_for(doc, page_index)?;
    let view = page_view(doc, page_id);
    let (llx, lly, urx, ury) = rect_native(&a.rect, view);
    let (w, h) = ((urx - llx).max(1.0), (ury - lly).max(1.0));

    // Poster image.
    let img = image::load_from_memory(&a.poster).map_err(|e| MutateError::Parse(format!("poster image: {e}")))?;
    let rgb = img.to_rgb8();
    let (iw, ih) = (rgb.width(), rgb.height());
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(rgb.as_raw(), iw, ih, image::ExtendedColorType::Rgb8)
        .map_err(|e| MutateError::Parse(format!("poster jpeg: {e}")))?;
    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => iw as i64, "Height" => ih as i64,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
        },
        jpeg,
    ));
    let mut content = Vec::new();
    let _ = write!(&mut content, "q\n{w:.4} 0 0 {h:.4} 0 0 cm\n/Im0 Do\nQ\n");
    let form_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "FormType" => 1,
            "BBox" => vec![real(0.0), real(0.0), real(w), real(h)],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
        },
        content,
    ));

    // 3D stream.
    let views: Vec<Object> = a.views.iter().map(|v| Object::Dictionary(write_view(v))).collect();
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "3D",
            "Subtype" => "U3D",
            "VA" => views,
            "DV" => a.default_view.min(a.views.len().saturating_sub(1)) as i64,
        },
        a.u3d.clone(),
    );
    let _ = stream.compress();
    let stream_id = doc.add_object(stream);

    let nm = crate::annotations::random_nm();
    let activation = match a.activation.as_str() {
        "PO" | "PV" => a.activation.clone(),
        _ => "XA".to_string(),
    };
    let annot = dictionary! {
        "Type" => "Annot",
        "Subtype" => "3D",
        "Rect" => vec![real(llx), real(lly), real(urx), real(ury)],
        "P" => page_id,
        "NM" => Object::string_literal(nm),
        "Contents" => pdf_string_utf16be(&a.name),
        "M" => Object::string_literal(now_pdf_date()),
        "F" => 4,
        "3DD" => stream_id,
        "3DV" => a.default_view.min(a.views.len().saturating_sub(1)) as i64,
        "3DA" => dictionary! {
            "A" => Object::Name(activation.into_bytes()),
            "D" => "PI",
            "TB" => true,
            "NP" => true,
        },
        "3DI" => true,
        "AP" => dictionary! { "N" => form_id },
    };
    let annot_id = doc.add_object(annot);
    append_annot_to_page(doc, page_id, annot_id)?;
    Ok(annot_id)
}
