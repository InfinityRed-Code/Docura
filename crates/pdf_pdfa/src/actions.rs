//! Actions: JavaScript and other forbidden action types, additional actions.

use lopdf::{Document, Object, ObjectId};

use crate::level::Level;
use crate::util::{self, deref, dict_mut};
use crate::Ctx;

const NAMED_OK: [&[u8]; 4] = [b"NextPage", b"PrevPage", b"FirstPage", b"LastPage"];

fn allowed_type(level: Level, s: &[u8]) -> bool {
    let base: &[&[u8]] = match level.part() {
        1 => &[b"GoTo", b"GoToR", b"Thread", b"URI", b"Named", b"SubmitForm"],
        2 | 3 => &[b"GoTo", b"GoToR", b"GoToE", b"Thread", b"URI", b"Named", b"SubmitForm"],
        _ => &[b"GoTo", b"GoToR", b"GoToE", b"Thread", b"URI", b"Named", b"SubmitForm", b"GoToDp"],
    };
    base.contains(&s) || (level.allows_3d() && s == b"RichMediaExecute")
}

struct Stats {
    js: usize,
    other: usize,
}

/// Returns the sanitised action, or `None` when the action (head of the
/// chain) is forbidden. Forbidden actions further down a /Next chain are
/// dropped from the chain.
fn sanitize(doc: &mut Document, level: Level, action: &Object, st: &mut Stats, depth: u32) -> Option<Object> {
    if depth > 16 {
        return None;
    }
    let (dict, id) = match action {
        Object::Reference(id) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => (d.clone(), Some(*id)),
            _ => return None,
        },
        Object::Dictionary(d) => (d.clone(), None),
        // Destinations (arrays, names, strings) are fine where actions are
        // allowed to be destinations (OpenAction).
        other => return Some(other.clone()),
    };
    let s = match dict.get(b"S").ok().map(|o| deref(doc, o)) {
        Some(Object::Name(n)) => n.clone(),
        _ => return None,
    };
    if !allowed_type(level, &s) {
        if s == b"JavaScript" {
            st.js += 1;
        } else {
            st.other += 1;
        }
        return None;
    }
    if s == b"Named" {
        let ok = matches!(dict.get(b"N").ok().map(|o| deref(doc, o)), Some(Object::Name(n)) if NAMED_OK.contains(&n.as_slice()));
        if !ok {
            st.other += 1;
            return None;
        }
    }
    let mut dict = dict;
    if let Ok(next) = dict.get(b"Next").cloned() {
        let next_val = match &next {
            Object::Reference(r) => match doc.objects.get(r) {
                Some(Object::Array(a)) => Object::Array(a.clone()),
                _ => next.clone(),
            },
            _ => next.clone(),
        };
        let new_next = match next_val {
            Object::Array(items) => {
                let v: Vec<Object> = items
                    .iter()
                    .filter_map(|a| sanitize(doc, level, a, st, depth + 1))
                    .collect();
                if v.is_empty() {
                    None
                } else {
                    Some(Object::Array(v))
                }
            }
            other => sanitize(doc, level, &other, st, depth + 1),
        };
        match new_next {
            Some(n) => dict.set("Next", n),
            None => {
                dict.remove(b"Next");
            }
        }
    }
    match id {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(dict));
            Some(Object::Reference(id))
        }
        None => Some(Object::Dictionary(dict)),
    }
}

/// Sanitise the action stored at `key` in the dictionary of `owner`.
fn fix_key(doc: &mut Document, level: Level, owner: ObjectId, key: &[u8], st: &mut Stats) {
    let Some(a) = util::dict_mut(doc, owner).and_then(|d| d.get(key).ok().cloned()) else { return };
    let new = sanitize(doc, level, &a, st, 0);
    if let Some(d) = dict_mut(doc, owner) {
        match new {
            Some(n) => d.set(key.to_vec(), n),
            None => {
                d.remove(key);
            }
        }
    }
}

/// Sanitise an additional-actions dictionary of `owner`; `allowed_keys`
/// limits the triggers (None = all triggers allowed).
fn fix_aa(doc: &mut Document, level: Level, owner: ObjectId, allowed_keys: Option<&[&[u8]]>, st: &mut Stats) {
    let Some(aa) = util::dict_mut(doc, owner).and_then(|d| d.get(b"AA").ok().cloned()) else { return };
    let (mut aad, aa_id) = match &aa {
        Object::Reference(id) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => (d.clone(), Some(*id)),
            _ => {
                if let Some(d) = dict_mut(doc, owner) {
                    d.remove(b"AA");
                }
                return;
            }
        },
        Object::Dictionary(d) => (d.clone(), None),
        _ => {
            if let Some(d) = dict_mut(doc, owner) {
                d.remove(b"AA");
            }
            return;
        }
    };
    let keys: Vec<Vec<u8>> = aad.iter().map(|(k, _)| k.clone()).collect();
    for k in keys {
        if let Some(allowed) = allowed_keys {
            if !allowed.contains(&k.as_slice()) {
                let v = aad.get(&k).ok().cloned();
                if let Some(v) = v {
                    // Count what is dropped.
                    let _ = sanitize(doc, level, &v, st, 0);
                }
                aad.remove(&k);
                continue;
            }
        }
        let v = aad.get(&k).ok().cloned().unwrap_or(Object::Null);
        match sanitize(doc, level, &v, st, 0) {
            Some(n) => aad.set(k, n),
            None => {
                aad.remove(&k);
            }
        }
    }
    let empty = aad.is_empty();
    match aa_id {
        Some(id) if !empty => {
            doc.objects.insert(id, Object::Dictionary(aad));
        }
        _ => {
            if let Some(d) = dict_mut(doc, owner) {
                if empty {
                    d.remove(b"AA");
                } else {
                    d.set("AA", Object::Dictionary(aad));
                }
            }
        }
    }
}

pub fn run(ctx: &mut Ctx) {
    let level = ctx.level;
    let doc = &mut ctx.doc;
    let mut st = Stats { js: 0, other: 0 };

    // Document-level JavaScript.
    if let Some(cat_id) = util::catalog_id(doc) {
        let names = util::catalog(doc).and_then(|c| c.get(b"Names").ok().cloned());
        match names {
            Some(Object::Reference(id)) => {
                if let Some(d) = dict_mut(doc, id) {
                    if d.remove(b"JavaScript").is_some() {
                        st.js += 1;
                    }
                }
            }
            Some(Object::Dictionary(mut d)) => {
                if d.remove(b"JavaScript").is_some() {
                    st.js += 1;
                }
                if let Some(c) = dict_mut(doc, cat_id) {
                    c.set("Names", Object::Dictionary(d));
                }
            }
            _ => {}
        }
        fix_key(doc, level, cat_id, b"OpenAction", &mut st);
        // Catalog triggers (WC, WS, DS, WP, DP) are never allowed.
        let had = dict_mut(doc, cat_id).map(|c| c.remove(b"AA").is_some()).unwrap_or(false);
        if had {
            st.other += 1;
        }
    }

    // Every dictionary that can hold actions.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let (is_page, is_widget, is_field, is_annot, is_outline) = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => {
                let ty = d.get(b"Type").ok().and_then(|o| o.as_name().ok());
                let sub = d.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
                let is_page = ty == Some(b"Page".as_slice());
                let is_widget = sub == Some(b"Widget".as_slice());
                let is_field = d.has(b"FT") || (d.has(b"T") && (d.has(b"Kids") || d.has(b"Parent")) && !d.has(b"Subtype"));
                let is_annot = sub.is_some() && (ty == Some(b"Annot".as_slice()) || d.has(b"Rect"));
                let is_outline = d.has(b"Title") && (d.has(b"Parent") || d.has(b"Dest") || d.has(b"A"));
                (is_page, is_widget, is_field, is_annot, is_outline)
            }
            _ => continue,
        };
        if is_widget || is_field {
            if let Some(d) = dict_mut(doc, id) {
                let a = d.remove(b"A").is_some();
                let aa = d.remove(b"AA").is_some();
                if a || aa {
                    st.other += 1;
                }
            }
            continue;
        }
        if is_page {
            let had = dict_mut(doc, id).map(|d| d.remove(b"AA").is_some()).unwrap_or(false);
            if had {
                st.other += 1;
            }
            continue;
        }
        if is_annot {
            fix_key(doc, level, id, b"A", &mut st);
            if level.part() == 4 {
                fix_aa(doc, level, id, Some(&[b"E", b"X", b"D", b"U", b"Fo", b"Bl"]), &mut st);
            } else {
                fix_aa(doc, level, id, None, &mut st);
            }
            continue;
        }
        if is_outline {
            fix_key(doc, level, id, b"A", &mut st);
        }
    }

    if st.js > 0 {
        ctx.report.note("javascript_removed", format!("{}", st.js));
    }
    if st.other > 0 {
        ctx.report.note("actions_removed", format!("{}", st.other));
    }
}
