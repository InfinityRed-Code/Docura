//! Optional content (layers).
//!
//! PDF/A-1 forbids optional content: the catalog entry goes, and XObjects /
//! annotations in groups that are hidden by default are removed so the page
//! keeps showing what it showed before. Later parts allow it with
//! restrictions on the configuration dictionaries.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::util::{self, deref, dict_mut};
use crate::Ctx;

pub fn run(ctx: &mut Ctx) {
    let doc = &mut ctx.doc;
    let Some(cat_id) = util::catalog_id(doc) else { return };
    let ocp = match util::catalog(doc).and_then(|c| c.get(b"OCProperties").ok()).map(|o| deref(doc, o).clone()) {
        Some(Object::Dictionary(d)) => d,
        _ => return,
    };
    let all: Vec<Object> = util::get_array(doc, &ocp, b"OCGs").cloned().unwrap_or_default();
    if !ctx.level.allows_optional_content() {
        let hidden = hidden_groups(doc, &ocp);
        if let Some(c) = dict_mut(doc, cat_id) {
            c.remove(b"OCProperties");
        }
        let removed = drop_hidden(doc, &hidden);
        ctx.report.note("layers_flattened", format!("{removed}"));
        return;
    }
    // Parts 2–4: Name in every configuration (unique), no AS, complete Order.
    let mut ocp = ocp;
    let mut used_names: BTreeSet<Vec<u8>> = BTreeSet::new();
    let fix_config = |doc: &mut Document, cfg: &Object, idx: usize, used: &mut BTreeSet<Vec<u8>>| -> Object {
        let (mut d, id) = match cfg {
            Object::Reference(r) => match doc.objects.get(r) {
                Some(Object::Dictionary(d)) => (d.clone(), Some(*r)),
                _ => (Dictionary::new(), None),
            },
            Object::Dictionary(d) => (d.clone(), None),
            _ => (Dictionary::new(), None),
        };
        d.remove(b"AS");
        let mut name = d.get(b"Name").ok().and_then(util::decode_text).unwrap_or_default();
        if name.is_empty() {
            name = if idx == 0 { "Default".to_string() } else { format!("Config {idx}") };
        }
        let mut unique = name.clone();
        let mut n = 2;
        while used.contains(unique.as_bytes()) {
            unique = format!("{name} ({n})");
            n += 1;
        }
        used.insert(unique.as_bytes().to_vec());
        d.set("Name", util::text_string(&unique));
        if let Ok(order) = d.get(b"Order").map(|o| deref(doc, o).clone()) {
            if let Object::Array(mut order) = order {
                let mut present = BTreeSet::new();
                collect_refs(doc, &Object::Array(order.clone()), &mut present, 0);
                for g in &all {
                    if let Object::Reference(r) = g {
                        if !present.contains(r) {
                            order.push(g.clone());
                        }
                    }
                }
                d.set("Order", Object::Array(order));
            }
        }
        match id {
            Some(r) => {
                doc.objects.insert(r, Object::Dictionary(d));
                Object::Reference(r)
            }
            None => Object::Dictionary(d),
        }
    };
    if let Ok(dcfg) = ocp.get(b"D").cloned() {
        let n = fix_config(doc, &dcfg, 0, &mut used_names);
        ocp.set("D", n);
    } else {
        let n = fix_config(doc, &Object::Dictionary(Dictionary::new()), 0, &mut used_names);
        ocp.set("D", n);
    }
    if let Ok(Object::Array(cfgs)) = ocp.get(b"Configs").map(|o| deref(doc, o).clone()) {
        let mut v = Vec::new();
        for (i, c) in cfgs.iter().enumerate() {
            v.push(fix_config(doc, c, i + 1, &mut used_names));
        }
        ocp.set("Configs", Object::Array(v));
    }
    let ocp_ref = util::catalog(doc).and_then(|c| c.get(b"OCProperties").ok().cloned());
    match ocp_ref {
        Some(Object::Reference(r)) => {
            doc.objects.insert(r, Object::Dictionary(ocp));
        }
        _ => {
            if let Some(c) = dict_mut(doc, cat_id) {
                c.set("OCProperties", Object::Dictionary(ocp));
            }
        }
    }
}

fn collect_refs(doc: &Document, o: &Object, out: &mut BTreeSet<ObjectId>, depth: u32) {
    if depth > 16 {
        return;
    }
    match o {
        Object::Reference(r) => {
            if let Some(Object::Array(_)) = doc.objects.get(r) {
                collect_refs(doc, deref(doc, o), out, depth + 1);
            } else {
                out.insert(*r);
            }
        }
        Object::Array(a) => a.iter().for_each(|x| collect_refs(doc, x, out, depth + 1)),
        _ => {}
    }
}

/// OCGs that are off in the default configuration.
fn hidden_groups(doc: &Document, ocp: &Dictionary) -> BTreeSet<ObjectId> {
    let mut hidden = BTreeSet::new();
    let Some(d) = util::get_dict(doc, ocp, b"D") else { return hidden };
    let base_off = util::get_name(doc, d, b"BaseState") == Some(b"OFF".as_slice());
    let mut on = BTreeSet::new();
    if let Some(a) = util::get_array(doc, d, b"ON") {
        for x in a {
            if let Object::Reference(r) = x {
                on.insert(*r);
            }
        }
    }
    if let Some(a) = util::get_array(doc, d, b"OFF") {
        for x in a {
            if let Object::Reference(r) = x {
                hidden.insert(*r);
            }
        }
    }
    if base_off {
        if let Some(a) = util::get_array(doc, ocp, b"OCGs") {
            for x in a {
                if let Object::Reference(r) = x {
                    if !on.contains(r) {
                        hidden.insert(*r);
                    }
                }
            }
        }
    }
    hidden
}

/// Remove XObjects and annotations whose /OC is a hidden group (or a
/// membership dictionary listing only hidden groups). Returns the count.
fn drop_hidden(doc: &mut Document, hidden: &BTreeSet<ObjectId>) -> usize {
    let is_hidden = |doc: &Document, d: &Dictionary| -> bool {
        match d.get(b"OC") {
            Ok(Object::Reference(r)) if hidden.contains(r) => true,
            Ok(o) => match util::dict_of(doc, o) {
                Some(m) if m.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"OCMD".as_slice()) => {
                    let ocgs: Vec<ObjectId> = match m.get(b"OCGs") {
                        Ok(Object::Reference(r)) => vec![*r],
                        Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
                        _ => Vec::new(),
                    };
                    !ocgs.is_empty() && ocgs.iter().all(|g| hidden.contains(g))
                }
                _ => false,
            },
            Err(_) => false,
        }
    };
    let mut count = 0;
    // Annotations.
    for page in util::pages(doc) {
        let annots = util::page_annots(doc, page);
        let before = annots.len();
        let kept: Vec<Object> = annots
            .into_iter()
            .filter(|a| util::dict_of(doc, a).map(|d| !is_hidden(doc, d)).unwrap_or(true))
            .collect();
        count += before - kept.len();
        if kept.len() != before {
            util::set_page_annots(doc, page, kept);
        }
    }
    // XObjects in hidden groups become empty forms.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let hide = match doc.objects.get(&id) {
            Some(Object::Stream(s)) => is_hidden(doc, &s.dict),
            _ => false,
        };
        if hide {
            let bbox = match doc.objects.get(&id) {
                Some(Object::Stream(s)) => s.dict.get(b"BBox").ok().cloned(),
                _ => None,
            };
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"XObject".to_vec()));
            d.set("Subtype", Object::Name(b"Form".to_vec()));
            d.set("BBox", bbox.unwrap_or(Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()])));
            doc.objects.insert(id, Object::Stream(lopdf::Stream::new(d, Vec::new())));
            count += 1;
        }
    }
    // Remaining /OC keys are meaningless without OCProperties.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(d) = dict_mut(doc, id) {
            d.remove(b"OC");
        }
    }
    count
}
