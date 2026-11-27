//! AcroForm field value writes via lopdf.
//!
//! Strategy: locate the field by its fully-qualified name (PDF 1.7 §12.7.3.2 —
//! parent /T concatenated with `.` separator), set /V, then bring what the
//! widgets show in line with it: text and choice fields get a new appearance
//! stream (`form_ap`), check boxes and radio buttons switch their /AS.
//! /NeedAppearances is only asked for where no appearance could be written:
//! with it set, viewers discard the appearances of every field of the
//! document, including the untouched ones.

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

/// Redraws the text or choice field `field` on all its widgets. A widget
/// whose appearance cannot be written loses its stale one and leaves the
/// drawing to the viewer.
fn refresh_text_appearances(doc: &mut Document, field: ObjectId) {
    for w in widget_kids(doc, field) {
        if let Err(e) = crate::form_ap::write_appearance(doc, w) {
            tracing::warn!("form: no appearance for widget {w:?}: {e}");
            if let Ok(d) = doc.get_dictionary_mut(w) {
                d.remove(b"AP");
            }
            set_need_appearances(doc);
        }
    }
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
            if let Ok(d) = doc.get_dictionary_mut(target) {
                d.set("V", v);
                // /I lists the selected options of a choice field and must
                // not contradict /V.
                d.remove(b"I");
            }
            refresh_text_appearances(doc, target);
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
            let drawn = checkbox_on_name(doc, chosen);
            if drawn.is_none() {
                // Nothing to show for the chosen state: the viewer draws it.
                set_need_appearances(doc);
            }
            let on_name = drawn.unwrap_or_else(|| idx.to_string().into_bytes());
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
            let drawn: Option<Vec<u8>> = widgets.iter().find_map(|&w| checkbox_on_name(doc, w));
            if drawn.is_none() {
                // No appearance for the checked state: the viewer draws it.
                set_need_appearances(doc);
            }
            let on_name = drawn.unwrap_or_else(|| b"Yes".to_vec());
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
                if let Ok(d) = doc.get_dictionary_mut(fid) {
                    if let Some(dv) = dv {
                        d.set("V", dv);
                    } else {
                        d.remove(b"V");
                    }
                    d.remove(b"I");
                }
                refresh_text_appearances(doc, fid);
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

    /// One text field "Name" (200 x 22) with its widget merged in, a
    /// regular /Helv in the form's /DR and `extra` on top of the basics.
    fn text_doc(da: &str, extra: lopdf::Dictionary) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.5");
        let pages = doc.new_object_id();
        let helv = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let mut field = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("Name"),
            "Rect" => vec![100.into(), 700.into(), 300.into(), 722.into()],
            "DA" => Object::string_literal(da),
        };
        for (k, v) in extra.iter() {
            field.set(k.clone(), v.clone());
        }
        let field = doc.add_object(field);
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Annots" => vec![field.into()],
        });
        doc.objects.insert(pages, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }));
        let af = doc.add_object(dictionary! {
            "Fields" => vec![field.into()],
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => helv } },
        });
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "AcroForm" => af });
        doc.trailer.set("Root", cat);
        (doc, field)
    }

    /// The widget's normal appearance: (stream dictionary, content).
    fn appearance(doc: &Document, widget: ObjectId) -> (lopdf::Dictionary, String) {
        let ap = doc.get_dictionary(widget).unwrap().get(b"AP").unwrap().as_dict().unwrap();
        let n = ap.get(b"N").unwrap().as_reference().unwrap();
        let s = doc.get_object(n).unwrap().as_stream().unwrap();
        (s.dict.clone(), String::from_utf8_lossy(&s.content).into_owned())
    }

    fn need_appearances(doc: &Document) -> bool {
        let af = get_acroform_id(doc).unwrap();
        doc.get_dictionary(af).unwrap().has(b"NeedAppearances")
    }

    #[test]
    fn text_value_is_drawn_with_the_font_size_of_the_field() {
        let (mut doc, field) = text_doc("/Helv 12 Tf 0 0 1 rg", dictionary! {});
        set_field_value(&mut doc, "Name", "Hamburg").unwrap();
        let (dict, content) = appearance(&doc, field);
        assert!(content.contains("/Helv 12 Tf 0 0 1 rg"), "{content}");
        // Left aligned at the 2 pt padding, capitals centred in the 22 pt box.
        assert!(content.contains("1 0 0 1 2 6.692 Tm (Hamburg) Tj"), "{content}");
        assert_eq!(dict.get(b"BBox").unwrap().as_array().unwrap().len(), 4);
        // The other fields of a document keep their appearances: no
        // /NeedAppearances that would have every viewer redraw them.
        assert!(!need_appearances(&doc));
        let styles = crate::form_ap::text_styles(&doc);
        let st = &styles[&(0, 0)];
        assert_eq!((st.font_size_pt, st.auto_size, st.align, st.padding_pt, st.turns), (12.0, false, 0, 2.0, 0));
        assert_eq!(st.color, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn auto_size_fills_the_field_and_shrinks_for_long_values() {
        let (mut doc, field) = text_doc("/Helv 0 Tf 0 g", dictionary! {});
        set_field_value(&mut doc, "Name", "Hi").unwrap();
        // (22 - 2) / 1.15 = 17.39.
        assert!(appearance(&doc, field).1.contains("/Helv 17.39 Tf"), "{}", appearance(&doc, field).1);
        assert_eq!(crate::form_ap::text_styles(&doc)[&(0, 0)].font_size_pt, 17.39);
        assert!(crate::form_ap::text_styles(&doc)[&(0, 0)].auto_size);

        set_field_value(&mut doc, "Name", &"W".repeat(40)).unwrap();
        // 40 capital W are 37.76 em wide: 196 pt / 37.76.
        let size = crate::form_ap::text_styles(&doc)[&(0, 0)].font_size_pt;
        assert_eq!(size, 5.19);
        assert!(appearance(&doc, field).1.contains("/Helv 5.19 Tf"));
    }

    #[test]
    fn alignment_border_and_background_are_part_of_the_appearance() {
        let extra = dictionary! {
            "Q" => 2,
            "MK" => dictionary! {
                "BC" => vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)],
                "BG" => vec![Object::Real(0.9)],
            },
            "BS" => dictionary! { "W" => 2 },
        };
        let (mut doc, field) = text_doc("/Helv 10 Tf 0 g", extra);
        set_field_value(&mut doc, "Name", "WW").unwrap();
        let (_, content) = appearance(&doc, field);
        assert!(content.contains("q 0.9 g 0 0 200 22 re f Q"), "{content}");
        assert!(content.contains("q 1 0 0 RG 2 w 1 1 198 20 re S Q"), "{content}");
        // Right aligned: 200 - padding (2 x border) - 2 x 0.944 em x 10.
        assert!(content.contains("1 0 0 1 177.12 7.41 Tm (WW) Tj"), "{content}");
        let st = &crate::form_ap::text_styles(&doc)[&(0, 0)];
        assert_eq!((st.align, st.padding_pt), (2, 4.0));
    }

    #[test]
    fn multiline_text_wraps_and_keeps_line_breaks() {
        let extra = dictionary! { "Ff" => 1 << 12, "Rect" => vec![100.into(), 600.into(), 160.into(), 700.into()] };
        let (mut doc, field) = text_doc("/Helv 10 Tf 0 g", extra);
        set_field_value(&mut doc, "Name", "one two three four\nfive").unwrap();
        let (_, content) = appearance(&doc, field);
        let lines: Vec<&str> = content.lines().filter(|l| l.ends_with("Tj")).collect();
        // 56 pt of room: "one two" (34 pt) fits, "one two three" does not.
        let texts: Vec<&str> = lines.iter().map(|l| &l[l.find('(').unwrap()..]).collect();
        assert_eq!(texts, ["(one two) Tj", "(three four) Tj", "(five) Tj"], "{content}");
        // 11.5 pt from baseline to baseline (leading 1.15), first one 0.92
        // em below the padding.
        let y = |l: &str| l.split(' ').nth(5).unwrap().parse::<f32>().unwrap();
        assert!((y(lines[0]) - (100.0 - 2.0 - 9.2)).abs() < 0.01, "{content}");
        assert!((y(lines[0]) - y(lines[1]) - 11.5).abs() < 0.01, "{content}");
    }

    #[test]
    fn rotated_widget_gets_a_turned_appearance() {
        // /MK /R 90 in a 22 x 200 rectangle: the text box is 200 x 22.
        let extra = dictionary! {
            "Rect" => vec![100.into(), 500.into(), 122.into(), 700.into()],
            "MK" => dictionary! { "R" => 90 },
        };
        let (mut doc, field) = text_doc("/Helv 12 Tf 0 g", extra);
        set_field_value(&mut doc, "Name", "Up").unwrap();
        let (dict, content) = appearance(&doc, field);
        let nums = |key: &[u8]| -> Vec<f32> {
            dict.get(key).unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect()
        };
        assert_eq!(nums(b"BBox"), [0.0, 0.0, 200.0, 22.0]);
        assert_eq!(nums(b"Matrix"), [0.0, 1.0, -1.0, 0.0, 22.0, 0.0]);
        assert!(content.contains("1 0 0 1 2 6.692 Tm (Up) Tj"), "{content}");
        // On an unrotated page such a text reads bottom to top.
        assert_eq!(crate::form_ap::text_styles(&doc)[&(0, 0)].turns, 3);
    }

    #[test]
    fn text_outside_winansi_is_written_with_an_embedded_font() {
        let (mut doc, field) = text_doc("/Helv 12 Tf 0 g", dictionary! {});
        set_field_value(&mut doc, "Name", "Łódź").unwrap();
        let (dict, content) = appearance(&doc, field);
        let fonts = dict.get(b"Resources").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap();
        let (_, font) = fonts.iter().next().unwrap();
        let font = doc.get_dictionary(font.as_reference().unwrap()).unwrap();
        assert_eq!(font.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
        // Glyph ids as a hex string, still 12 pt.
        assert!(content.contains(" 12 Tf") && content.contains("> Tj"), "{content}");
        assert_eq!(crate::form_ap::text_styles(&doc)[&(0, 0)].font_size_pt, 12.0);
    }

    #[test]
    fn reset_redraws_the_default_value() {
        let (mut doc, field) = text_doc("/Helv 9 Tf 0 g", dictionary! { "DV" => Object::string_literal("default") });
        set_field_value(&mut doc, "Name", "typed").unwrap();
        reset_form(&mut doc).unwrap();
        let (_, content) = appearance(&doc, field);
        assert!(content.contains("(default) Tj") && content.contains("/Helv 9 Tf"), "{content}");
        assert!(!need_appearances(&doc));
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
