//! AcroForm field value writes via lopdf.
//!
//! Strategy: locate the field by its fully-qualified name (PDF 1.7 §12.7.3.2 —
//! parent /T concatenated with `.` separator), set /V, mirror /AS on the
//! widget kids when the field is a Btn (checkbox / radio), then set
//! /AcroForm/NeedAppearances so PDFium / Acrobat regenerate the appearance
//! stream on next render.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use crate::{MutateError, MutateResult};

/// Bound for /Parent chain walks (real forms nest a few levels at most).
const MAX_FIELD_DEPTH: usize = 64;

fn get_acroform_id(doc: &Document) -> MutateResult<ObjectId> {
    let root_id = doc
        .trailer
        .get(b"Root")
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .as_reference()
        .map_err(|e| MutateError::Parse(e.to_string()))?;
    let af = doc
        .get_dictionary(root_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"AcroForm")
        .map_err(|_| MutateError::Parse("no /AcroForm in catalog".into()))?;
    match af {
        Object::Reference(r) => Ok(*r),
        Object::Dictionary(_) => Err(MutateError::Parse(
            "AcroForm is inline; not supported".into(),
        )),
        _ => Err(MutateError::Parse("AcroForm has unexpected type".into())),
    }
}

fn decode_pdf_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let mut units: Vec<u16> = Vec::with_capacity((bytes.len() - 2) / 2);
        let mut i = 2;
        while i + 1 < bytes.len() {
            units.push(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn read_t(doc: &Document, dict: &lopdf::Dictionary) -> Option<String> {
    let obj = dict.get(b"T").ok()?;
    let resolved = match obj {
        Object::Reference(r) => doc.get_object(*r).ok()?,
        other => other,
    };
    let s = resolved.as_str().ok()?;
    Some(decode_pdf_text(s))
}

/// Recursively collect (field_object_id, fully_qualified_name) for every
/// terminal field in an /AcroForm /Fields tree. Terminal = no /Kids that are
/// fields (kids that are widget annotations are siblings of the value).
fn collect_terminal_fields(
    doc: &Document,
    field_id: ObjectId,
    parent_name: &str,
    out: &mut Vec<(ObjectId, String)>,
    visited: &mut HashSet<ObjectId>,
) {
    // A /Kids cycle in a malformed form would otherwise recurse until the
    // stack overflows (an abort, not a recoverable error).
    if !visited.insert(field_id) {
        return;
    }
    let Ok(d) = doc.get_dictionary(field_id) else {
        return;
    };
    let local = read_t(doc, d).unwrap_or_default();
    let full = if local.is_empty() {
        parent_name.to_string()
    } else if parent_name.is_empty() {
        local
    } else {
        format!("{parent_name}.{local}")
    };
    let kids = d.get(b"Kids").ok().and_then(|o| match o {
        Object::Array(a) => Some(a.clone()),
        Object::Reference(r) => doc.get_object(*r).ok().and_then(|x| x.as_array().ok().cloned()),
        _ => None,
    });
    if let Some(kids) = kids {
        // Are kids fields (have /T or no /Subtype) or widget annotations only?
        let mut has_field_kids = false;
        let kid_ids: Vec<ObjectId> = kids
            .iter()
            .filter_map(|o| o.as_reference().ok())
            .collect();
        for &kid in &kid_ids {
            if let Ok(kd) = doc.get_dictionary(kid) {
                let is_widget = kd
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .map(|n| n == b"Widget")
                    .unwrap_or(false);
                let has_t = kd.get(b"T").is_ok();
                if !is_widget && has_t {
                    has_field_kids = true;
                    break;
                }
            }
        }
        if has_field_kids {
            for &kid in &kid_ids {
                collect_terminal_fields(doc, kid, &full, out, visited);
            }
            return;
        }
    }
    out.push((field_id, full));
}

fn pdf_string_text(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), lopdf::StringFormat::Literal)
    } else {
        // UTF-16BE with BOM.
        let mut bytes = Vec::with_capacity(2 + s.len() * 2);
        bytes.push(0xFE);
        bytes.push(0xFF);
        for u in s.encode_utf16() {
            bytes.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(bytes, lopdf::StringFormat::Hexadecimal)
    }
}

fn field_subtype(doc: &Document, field_id: ObjectId) -> Option<Vec<u8>> {
    // /FT may be inherited from parent. Bounded so a /Parent cycle can't hang.
    let mut cur = field_id;
    for _ in 0..MAX_FIELD_DEPTH {
        let d = doc.get_dictionary(cur).ok()?;
        if let Ok(ft) = d.get(b"FT").and_then(|o| o.as_name()) {
            return Some(ft.to_vec());
        }
        let parent = d.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        match parent {
            Some(p) if p != cur => cur = p,
            _ => return None,
        }
    }
    None
}

fn widget_kids(doc: &Document, field_id: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let Ok(d) = doc.get_dictionary(field_id) else {
        return out;
    };
    // If field IS the widget (no Kids, has /Subtype Widget), include itself.
    let is_self_widget = d
        .get(b"Subtype")
        .ok()
        .and_then(|s| s.as_name().ok())
        .map(|n| n == b"Widget")
        .unwrap_or(false);
    if is_self_widget {
        out.push(field_id);
    }
    if let Ok(kids) = d.get(b"Kids") {
        let arr = match kids {
            Object::Array(a) => a.clone(),
            Object::Reference(r) => doc
                .get_object(*r)
                .and_then(|o| o.as_array().map(|a| a.clone()))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for o in &arr {
            if let Ok(r) = o.as_reference() {
                if let Ok(kd) = doc.get_dictionary(r) {
                    let is_w = kd
                        .get(b"Subtype")
                        .ok()
                        .and_then(|s| s.as_name().ok())
                        .map(|n| n == b"Widget")
                        .unwrap_or(false);
                    if is_w {
                        out.push(r);
                    }
                }
            }
        }
    }
    out
}

/// Walk a Btn widget's /AP /N to find the "on" appearance name (anything
/// other than /Off). Used to set /AS when checking a checkbox.
fn checkbox_on_name(doc: &Document, widget_id: ObjectId) -> Option<Vec<u8>> {
    let d = doc.get_dictionary(widget_id).ok()?;
    let ap = d.get(b"AP").ok()?;
    let ap_d = match ap {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(r) => doc.get_dictionary(*r).ok().cloned()?,
        _ => return None,
    };
    let n = ap_d.get(b"N").ok()?;
    let n_d = match n {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(r) => doc.get_dictionary(*r).ok().cloned()?,
        _ => return None,
    };
    for (k, _) in n_d.iter() {
        if k != b"Off" {
            return Some(k.to_vec());
        }
    }
    None
}

fn set_need_appearances(doc: &mut Document) {
    if let Ok(af_id) = get_acroform_id(doc) {
        if let Ok(af) = doc.get_dictionary_mut(af_id) {
            af.set("NeedAppearances", Object::Boolean(true));
        }
    }
}

pub fn set_field_value(doc: &mut Document, name: &str, value: &str) -> MutateResult<()> {
    let af_id = get_acroform_id(doc)?;
    let fields_obj = doc
        .get_dictionary(af_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Fields")
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .clone();
    let root_field_ids: Vec<ObjectId> = match fields_obj {
        Object::Array(a) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
        Object::Reference(r) => doc
            .get_object(r)
            .and_then(|o| o.as_array().map(|a| a.clone()))
            .unwrap_or_default()
            .iter()
            .filter_map(|o| o.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    };
    let mut all: Vec<(ObjectId, String)> = Vec::new();
    let mut visited = HashSet::new();
    for &fid in &root_field_ids {
        collect_terminal_fields(doc, fid, "", &mut all, &mut visited);
    }
    let target = all
        .iter()
        .find(|(_, n)| n == name)
        .map(|(id, _)| *id)
        .ok_or_else(|| MutateError::Parse(format!("field not found: {name}")))?;

    let ft = field_subtype(doc, target).unwrap_or_default();
    match ft.as_slice() {
        b"Tx" | b"Ch" => {
            let v = pdf_string_text(value);
            let widgets = widget_kids(doc, target);
            if let Ok(d) = doc.get_dictionary_mut(target) {
                d.set("V", v);
                d.remove(b"AP"); // force regeneration on viewers that honor NeedAppearances
            }
            // The widget kid carries the visible /AP stream. Strip it so our
            // PDFium renderer (which doesn't run FFLDraw) doesn't keep
            // painting the stale appearance under the HTML overlay.
            for w in widgets {
                if w == target {
                    continue;
                }
                if let Ok(d) = doc.get_dictionary_mut(w) {
                    d.remove(b"AP");
                }
            }
        }
        b"Btn" if value.starts_with("radio:") => {
            // Radio group: "radio:<n>" turns on the n-th widget (PDFium's
            // index-in-group = /Kids order) and switches every sibling off.
            let idx: usize = value["radio:".len()..]
                .trim()
                .parse()
                .map_err(|_| MutateError::Parse(format!("bad radio value {value:?}")))?;
            let widgets = widget_kids(doc, target);
            let chosen = *widgets
                .get(idx)
                .ok_or_else(|| MutateError::Parse(format!("radio option {idx} not found in {name}")))?;
            let on_name = checkbox_on_name(doc, chosen).unwrap_or_else(|| idx.to_string().into_bytes());
            if let Ok(d) = doc.get_dictionary_mut(target) {
                d.set("V", Object::Name(on_name.clone()));
            }
            for w in widgets {
                // Widgets sharing the chosen on-state (radios in unison) turn
                // on together; everything else is off.
                let on = w == chosen || checkbox_on_name(doc, w).as_deref() == Some(on_name.as_slice());
                if let Ok(d) = doc.get_dictionary_mut(w) {
                    d.set(
                        "AS",
                        Object::Name(if on { on_name.clone() } else { b"Off".to_vec() }),
                    );
                }
            }
        }
        b"Btn" => {
            // Checkbox / radio. We treat "Yes"/"On"/"true" as check, anything else as off.
            let want_on = matches!(
                value.to_ascii_lowercase().as_str(),
                "yes" | "on" | "true" | "1"
            );
            let widgets = widget_kids(doc, target);
            let on_name: Option<Vec<u8>> = widgets
                .iter()
                .find_map(|&w| checkbox_on_name(doc, w))
                .or_else(|| Some(b"Yes".to_vec()));
            let on_name = on_name.unwrap_or_else(|| b"Yes".to_vec());
            let new_v = if want_on {
                Object::Name(on_name.clone())
            } else {
                Object::Name(b"Off".to_vec())
            };
            if let Ok(d) = doc.get_dictionary_mut(target) {
                d.set("V", new_v.clone());
            }
            for w in widgets {
                if let Ok(d) = doc.get_dictionary_mut(w) {
                    d.set("AS", new_v.clone());
                }
            }
        }
        b"Sig" => {
            return Err(MutateError::Parse("signature field not writable yet".into()));
        }
        other => {
            return Err(MutateError::Parse(format!(
                "unsupported field /FT {}",
                String::from_utf8_lossy(other)
            )));
        }
    }
    set_need_appearances(doc);
    Ok(())
}

pub fn reset_form(doc: &mut Document) -> MutateResult<()> {
    let af_id = get_acroform_id(doc)?;
    let fields_obj = doc
        .get_dictionary(af_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Fields")
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .clone();
    let root_field_ids: Vec<ObjectId> = match fields_obj {
        Object::Array(a) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
        Object::Reference(r) => doc
            .get_object(r)
            .and_then(|o| o.as_array().map(|a| a.clone()))
            .unwrap_or_default()
            .iter()
            .filter_map(|o| o.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    };
    let mut all: Vec<(ObjectId, String)> = Vec::new();
    let mut visited = HashSet::new();
    for &fid in &root_field_ids {
        collect_terminal_fields(doc, fid, "", &mut all, &mut visited);
    }
    for (fid, _) in all {
        let ft = field_subtype(doc, fid).unwrap_or_default();
        let dv = doc.get_dictionary(fid).ok().and_then(|d| d.get(b"DV").ok().cloned());
        match ft.as_slice() {
            b"Tx" | b"Ch" => {
                let widgets = widget_kids(doc, fid);
                if let Ok(d) = doc.get_dictionary_mut(fid) {
                    if let Some(dv) = dv {
                        d.set("V", dv);
                    } else {
                        d.remove(b"V");
                    }
                    d.remove(b"AP");
                }
                for w in widgets {
                    if w == fid {
                        continue;
                    }
                    if let Ok(d) = doc.get_dictionary_mut(w) {
                        d.remove(b"AP");
                    }
                }
            }
            b"Btn" => {
                let widgets = widget_kids(doc, fid);
                let new_v = dv.unwrap_or(Object::Name(b"Off".to_vec()));
                if let Ok(d) = doc.get_dictionary_mut(fid) {
                    d.set("V", new_v.clone());
                }
                for w in widgets {
                    if let Ok(d) = doc.get_dictionary_mut(w) {
                        d.set("AS", new_v.clone());
                    }
                }
            }
            _ => {}
        }
    }
    set_need_appearances(doc);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn kids_cycle_does_not_overflow_the_stack() {
        let mut d = Document::with_version("1.5");
        let a = d.new_object_id();
        let b = d.new_object_id();
        // Malformed: a and b are each other's kid and parent.
        d.objects.insert(a, Object::Dictionary(dictionary! {
            "T" => Object::string_literal("a"),
            "Parent" => Object::Reference(b),
            "Kids" => Object::Array(vec![Object::Reference(b), Object::Reference(b)]),
        }));
        d.objects.insert(b, Object::Dictionary(dictionary! {
            "T" => Object::string_literal("b"),
            "Parent" => Object::Reference(a),
            "Kids" => Object::Array(vec![Object::Reference(a), Object::Reference(a)]),
        }));
        let mut all = Vec::new();
        collect_terminal_fields(&d, a, "", &mut all, &mut HashSet::new());
        assert!(all.is_empty());
        assert_eq!(field_subtype(&d, b), None);
    }

    /// Radio field "Choice" with three widgets whose on-states are A, B, C.
    fn radio_doc() -> (Document, ObjectId, Vec<ObjectId>) {
        let mut doc = Document::with_version("1.5");
        let field = doc.new_object_id();
        let mut kids = Vec::new();
        for on in ["A", "B", "C"] {
            let n = doc.add_object(dictionary! {
                on => Object::Null,
                "Off" => Object::Null,
            });
            let ap = doc.add_object(dictionary! { "N" => Object::Reference(n) });
            kids.push(doc.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Widget",
                "Parent" => Object::Reference(field),
                "AP" => Object::Reference(ap),
                "AS" => if on == "A" { "A" } else { "Off" },
            }));
        }
        doc.objects.insert(field, Object::Dictionary(dictionary! {
            "FT" => "Btn",
            "Ff" => 1 << 15,
            "T" => Object::string_literal("Choice"),
            "V" => "A",
            "Kids" => Object::Array(kids.iter().map(|&k| Object::Reference(k)).collect()),
        }));
        let af = doc.add_object(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field)]),
        });
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "AcroForm" => Object::Reference(af) });
        doc.trailer.set("Root", Object::Reference(cat));
        (doc, field, kids)
    }

    fn as_of(doc: &Document, id: ObjectId) -> Vec<u8> {
        doc.get_dictionary(id).unwrap().get(b"AS").unwrap().as_name().unwrap().to_vec()
    }

    #[test]
    fn radio_selects_exactly_the_chosen_option() {
        let (mut doc, field, kids) = radio_doc();
        set_field_value(&mut doc, "Choice", "radio:2").unwrap();
        let v = doc.get_dictionary(field).unwrap().get(b"V").unwrap().as_name().unwrap().to_vec();
        assert_eq!(v, b"C");
        assert_eq!(as_of(&doc, kids[0]), b"Off");
        assert_eq!(as_of(&doc, kids[1]), b"Off");
        assert_eq!(as_of(&doc, kids[2]), b"C");
        assert!(set_field_value(&mut doc, "Choice", "radio:7").is_err());
    }

    #[test]
    fn reset_form_follows_indirect_fields_array() {
        let mut doc = Document::with_version("1.5");
        let field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("Name"),
            "V" => Object::string_literal("filled"),
        });
        // Catalog → AcroForm whose /Fields is an indirect array.
        let arr = doc.add_object(Object::Array(vec![Object::Reference(field)]));
        let af = doc.add_object(dictionary! { "Fields" => Object::Reference(arr) });
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "AcroForm" => Object::Reference(af) });
        doc.trailer.set("Root", Object::Reference(cat));
        reset_form(&mut doc).unwrap();
        assert!(!doc.get_dictionary(field).unwrap().has(b"V"), "field not reset");
    }
}
