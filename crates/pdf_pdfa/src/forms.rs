//! Interactive forms: stay fillable, but with valid appearances and no
//! NeedAppearances / XFA.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

use crate::annots::{field_type, rect_of};
use crate::util::{self, deref, dict_mut};
use crate::{ap, Ctx};

pub fn run(ctx: &mut Ctx) {
    let doc = &mut ctx.doc;
    let Some(cat_id) = util::catalog_id(doc) else { return };
    let af = util::catalog(doc).and_then(|c| c.get(b"AcroForm").ok().cloned());
    let (mut afd, af_id) = match af {
        Some(Object::Reference(id)) => match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => (d.clone(), Some(id)),
            _ => return,
        },
        Some(Object::Dictionary(d)) => (d, None),
        _ => return,
    };
    let need_appearances = matches!(afd.get(b"NeedAppearances"), Ok(Object::Boolean(true)));
    afd.remove(b"NeedAppearances");
    if afd.remove(b"XFA").is_some() {
        ctx.report.note("xfa_removed", "");
    }
    match af_id {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(afd));
        }
        None => {
            // An indirect AcroForm is what form editors expect (the app's
            // own form filling does not handle an inline one).
            let id = doc.add_object(Object::Dictionary(afd));
            if let Some(c) = dict_mut(doc, cat_id) {
                c.set("AcroForm", Object::Reference(id));
            }
        }
    }

    // Widgets without appearance get one (buttons drawn with paths; text
    // fields get theirs in the font pass, which can embed fonts).
    let mut generated = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let d = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Widget".as_slice()) => d.clone(),
            _ => continue,
        };
        let has_ap = util::get_dict(doc, &d, b"AP").map(|a| a.has(b"N")).unwrap_or(false);
        let ft = field_type(doc, &d);
        if has_ap && !(need_appearances && ft.as_deref() == Some(b"Btn".as_slice()) && !state_ok(doc, &d)) {
            continue;
        }
        let Some(rect) = rect_of(doc, &d) else { continue };
        if rect[0] == rect[2] && rect[1] == rect[3] {
            continue;
        }
        let n = match ft.as_deref() {
            Some(b"Btn") => button_appearance(doc, &d, rect),
            Some(b"Tx") | Some(b"Ch") => {
                // Filled in by fonts::field_appearances.
                continue;
            }
            _ => {
                let s = ap::empty_form(rect);
                Object::Reference(doc.add_object(Object::Stream(s)))
            }
        };
        if let Some(dd) = dict_mut(doc, id) {
            dd.set("AP", dictionary! { "N" => n });
            generated += 1;
        }
    }
    if generated > 0 {
        ctx.report.note("appearances_generated", format!("{generated}"));
    }
}

/// The widget's /AS names an existing state.
fn state_ok(doc: &Document, d: &Dictionary) -> bool {
    let n = util::get_dict(doc, d, b"AP").and_then(|a| a.get(b"N").ok()).map(|n| deref(doc, n));
    match (n, d.get(b"AS").ok().and_then(|o| o.as_name().ok())) {
        (Some(Object::Dictionary(sd)), Some(st)) => sd.has(st),
        (Some(Object::Dictionary(_)), None) => true,
        _ => true,
    }
}

/// On/Off appearances for check boxes and radio buttons (a check mark or
/// dot drawn with paths, no font needed).
fn button_appearance(doc: &mut Document, d: &Dictionary, rect: [f64; 4]) -> Object {
    let flags = util::get_num(doc, d, b"Ff").unwrap_or(0.0) as i64;
    let radio = flags & (1 << 15) != 0;
    let push = flags & (1 << 16) != 0;
    let on_name = d
        .get(b"AS")
        .ok()
        .and_then(|o| o.as_name().ok())
        .filter(|n| *n != b"Off")
        .map(|n| n.to_vec())
        .unwrap_or_else(|| b"Yes".to_vec());
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    let s = w.min(h);
    let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
    let mut on = Vec::new();
    use std::io::Write;
    if push {
        let empty = ap::empty_form(rect);
        let id = doc.add_object(Object::Stream(empty));
        let mut sd = Dictionary::new();
        sd.set(on_name, Object::Reference(id));
        return Object::Dictionary(sd);
    }
    if radio {
        let r = s * 0.25;
        let k = 0.5523 * r;
        let _ = write!(
            on,
            "q 0 g {} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c f Q\n",
            cx + r, cy,
            cx + r, cy + k, cx + k, cy + r, cx, cy + r,
            cx - k, cy + r, cx - r, cy + k, cx - r, cy,
            cx - r, cy - k, cx - k, cy - r, cx, cy - r,
            cx + k, cy - r, cx + r, cy - k, cx + r, cy
        );
    } else {
        let _ = write!(
            on,
            "q 0 G {} w 1 J 1 j {} {} m {} {} l {} {} l S Q\n",
            (s * 0.1).max(0.5),
            cx - s * 0.3,
            cy,
            cx - s * 0.08,
            cy - s * 0.25,
            cx + s * 0.32,
            cy + s * 0.28
        );
    }
    let on_id = doc.add_object(Object::Stream(ap::form(rect, on, None)));
    let off_id = doc.add_object(Object::Stream(ap::empty_form(rect)));
    let mut sd = Dictionary::new();
    sd.set(on_name, Object::Reference(on_id));
    sd.set("Off", Object::Reference(off_id));
    Object::Dictionary(sd)
}
