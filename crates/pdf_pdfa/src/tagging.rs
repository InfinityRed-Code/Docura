//! Logical structure for level a (PDF/A-1a, -2a, -3a).
//!
//! Documents that are already tagged keep their structure; the role map is
//! repaired so every type resolves to a standard one. Untagged documents
//! are tagged automatically from their content: each run of text objects
//! becomes a paragraph (`P`), each image a `Figure` with alternate text,
//! and vector graphics or form XObjects become artifacts. That is what
//! automatic taggers do; it is a sound structure, not a semantic analysis.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

use crate::content::{self, Operand};
use crate::level::Level;
use crate::util::{self, deref, dict_mut};
use crate::{Ctx, Result};

/// Standard structure types of PDF 1.4 (part 1) and ISO 32000-1 (2, 3).
fn standard(level: Level, t: &[u8]) -> bool {
    const BASE: [&[u8]; 42] = [
        b"Document", b"Part", b"Art", b"Sect", b"Div", b"BlockQuote", b"Caption", b"TOC", b"TOCI", b"Index",
        b"NonStruct", b"Private", b"P", b"H", b"H1", b"H2", b"H3", b"H4", b"H5", b"H6", b"L", b"LI", b"Lbl",
        b"LBody", b"Table", b"TR", b"TH", b"TD", b"Span", b"Quote", b"Note", b"Reference", b"BibEntry", b"Code",
        b"Link", b"Figure", b"Formula", b"Form", b"Ruby", b"RB", b"RT", b"RP",
    ];
    const PDF17: [&[u8]; 7] = [b"THead", b"TBody", b"TFoot", b"Annot", b"Warichu", b"WT", b"WP"];
    if level.part() == 1 {
        BASE[..38].contains(&t)
    } else {
        BASE.contains(&t) || PDF17.contains(&t)
    }
}

pub fn run(ctx: &mut Ctx) -> Result<()> {
    if !ctx.level.needs_structure() {
        return Ok(());
    }
    let level = ctx.level;
    let lang = ctx.opts.lang.clone();
    let tagged = {
        let doc = &ctx.doc;
        let cat = util::catalog(doc);
        let marked = cat
            .and_then(|c| util::get_dict(doc, c, b"MarkInfo"))
            .map(|m| matches!(util::get(doc, m, b"Marked"), Some(Object::Boolean(true))))
            .unwrap_or(false);
        let has_tree = cat.map(|c| util::get_dict(doc, c, b"StructTreeRoot").is_some()).unwrap_or(false);
        marked && has_tree
    };
    if tagged {
        repair_role_map(&mut ctx.doc, level);
    } else {
        if let Some(c) = util::catalog_mut(&mut ctx.doc) {
            c.remove(b"StructTreeRoot");
        }
        auto_tag(ctx)?;
        ctx.report.note("tags_generated", "");
    }
    let doc = &mut ctx.doc;
    if let Some(c) = util::catalog_mut(doc) {
        c.set("MarkInfo", dictionary! { "Marked" => true });
        let lang_ok = c.get(b"Lang").ok().and_then(util::decode_text).map(|l| valid_lang(&l)).unwrap_or(false);
        if !lang_ok {
            match lang.as_deref().filter(|l| valid_lang(l)) {
                Some(l) => c.set("Lang", util::text_string(l)),
                None => {
                    c.remove(b"Lang");
                }
            }
        }
    }
    // Invalid Lang values elsewhere (structure elements) go.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(d) = dict_mut(doc, id) {
            if let Ok(l) = d.get(b"Lang") {
                if !util::decode_text(l).map(|s| valid_lang(&s)).unwrap_or(false) {
                    d.remove(b"Lang");
                }
            }
        }
    }
    Ok(())
}

/// RFC 3066 style language tag (empty = unknown, allowed).
pub fn valid_lang(l: &str) -> bool {
    if l.is_empty() {
        return true;
    }
    let mut parts = l.split('-');
    let first = parts.next().unwrap_or("");
    let first_ok = (first.len() >= 1 && first.len() <= 8 && first.chars().all(|c| c.is_ascii_alphabetic())) || first == "i" || first == "x";
    first_ok && parts.all(|p| !p.is_empty() && p.len() <= 8 && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Every non-standard type maps (without cycles) to a standard one; no
/// standard type is remapped.
fn repair_role_map(doc: &mut Document, level: Level) {
    let Some(root_ref) = util::catalog(doc).and_then(|c| c.get(b"StructTreeRoot").ok().cloned()) else { return };
    let Some(root_id) = root_ref.as_reference().ok() else { return };
    let mut used: BTreeSet<Vec<u8>> = BTreeSet::new();
    for o in doc.objects.values() {
        if let Object::Dictionary(d) = o {
            let is_elem = d.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"StructElem".as_slice()) || (d.has(b"S") && d.has(b"P") && !d.has(b"Subtype"));
            if is_elem {
                if let Ok(Object::Name(s)) = d.get(b"S") {
                    used.insert(s.clone());
                }
            }
        }
    }
    let rm_obj = doc.objects.get(&root_id).and_then(|o| o.as_dict().ok()).and_then(|r| r.get(b"RoleMap").ok().cloned());
    let mut rm = match &rm_obj {
        Some(o) => util::dict_of(doc, o).cloned().unwrap_or_default(),
        None => Dictionary::new(),
    };
    // Drop remappings of standard types.
    let keys: Vec<Vec<u8>> = rm.iter().map(|(k, _)| k.clone()).collect();
    for k in &keys {
        if standard(level, k) {
            rm.remove(k);
        }
    }
    for t in &used {
        if standard(level, t) {
            continue;
        }
        // Follow the chain.
        let mut cur = t.clone();
        let mut seen = BTreeSet::new();
        let mut ok = false;
        while let Some(Object::Name(n)) = rm.get(&cur).ok().map(|o| deref(doc, o)) {
            if !seen.insert(cur.clone()) {
                break;
            }
            if standard(level, n) {
                ok = true;
                break;
            }
            cur = n.clone();
        }
        if !ok {
            rm.set(t.clone(), Object::Name(b"Span".to_vec()));
        }
    }
    if let Some(r) = dict_mut(doc, root_id) {
        r.set("RoleMap", Object::Dictionary(rm));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    P,
    Figure,
}

/// Rewrite one page's content with marked content; returns the new content
/// and the struct element kinds per MCID run: (kind, mcids).
fn tag_content(data: &[u8], images: &BTreeSet<Vec<u8>>) -> (Vec<u8>, Vec<(Kind, Vec<i64>)>) {
    let mut out = Vec::with_capacity(data.len() + data.len() / 8);
    let mut elems: Vec<(Kind, Vec<i64>)> = Vec::new();
    let mut mcid: i64 = 0;
    let mut in_text = false;
    let mut in_art = false;
    let mut last_was_text = false;
    let close_art = |out: &mut Vec<u8>, in_art: &mut bool| {
        if *in_art {
            out.extend_from_slice(b"EMC\n");
            *in_art = false;
        }
    };
    let mut depth_extra: i32 = 0; // existing marked content nesting
    content::visit(data, |op| {
        let raw = &data[op.start..op.end];
        if in_text {
            out.extend_from_slice(raw);
            out.push(b'\n');
            if op.name == b"ET" {
                out.extend_from_slice(b"EMC\n");
                in_text = false;
            }
            return true;
        }
        match op.name {
            b"BT" => {
                close_art(&mut out, &mut in_art);
                let _ = write_bdc(&mut out, b"P", mcid);
                if last_was_text {
                    if let Some(e) = elems.last_mut() {
                        e.1.push(mcid);
                    }
                } else {
                    elems.push((Kind::P, vec![mcid]));
                }
                mcid += 1;
                last_was_text = true;
                in_text = true;
                out.extend_from_slice(raw);
                out.push(b'\n');
            }
            b"Do" | b"BI" => {
                close_art(&mut out, &mut in_art);
                let is_image = op.name == b"BI" || op.args.first().and_then(|a| a.as_name()).map(|n| images.contains(n)).unwrap_or(false);
                if is_image {
                    let _ = write_bdc(&mut out, b"Figure", mcid);
                    elems.push((Kind::Figure, vec![mcid]));
                    mcid += 1;
                    last_was_text = false;
                } else {
                    out.extend_from_slice(b"/Artifact BMC\n");
                }
                out.extend_from_slice(raw);
                out.extend_from_slice(b"\nEMC\n");
            }
            b"sh" => {
                close_art(&mut out, &mut in_art);
                out.extend_from_slice(b"/Artifact BMC\n");
                out.extend_from_slice(raw);
                out.extend_from_slice(b"\nEMC\n");
                last_was_text = false;
            }
            b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re" | b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n" | b"W" | b"W*" => {
                if !in_art {
                    out.extend_from_slice(b"/Artifact BMC\n");
                    in_art = true;
                }
                last_was_text = false;
                out.extend_from_slice(raw);
                out.push(b'\n');
            }
            b"q" | b"Q" | b"BMC" | b"BDC" | b"EMC" | b"MP" | b"DP" => {
                close_art(&mut out, &mut in_art);
                if op.name == b"BDC" || op.name == b"BMC" {
                    depth_extra += 1;
                } else if op.name == b"EMC" {
                    if depth_extra == 0 {
                        // Unbalanced EMC in the source: drop it.
                        return true;
                    }
                    depth_extra -= 1;
                }
                // Marked content that carried MCIDs of an old, missing tree.
                if op.name == b"BDC" && op.args.get(1).map(|p| p.dict_get(b"MCID").is_some()).unwrap_or(false) {
                    if let Some(tag) = op.args.first().and_then(|a| a.as_name()) {
                        content::write_op(&mut out, b"BMC", &[Operand::Name(tag.to_vec())]);
                        return true;
                    }
                }
                out.extend_from_slice(raw);
                out.push(b'\n');
            }
            _ => {
                out.extend_from_slice(raw);
                out.push(b'\n');
            }
        }
        true
    });
    if in_text {
        out.extend_from_slice(b"ET\nEMC\n");
    }
    close_art(&mut out, &mut in_art);
    for _ in 0..depth_extra {
        out.extend_from_slice(b"EMC\n");
    }
    (out, elems)
}

fn write_bdc(out: &mut Vec<u8>, tag: &[u8], mcid: i64) -> std::io::Result<()> {
    content::write_op(out, b"BDC", &[Operand::Name(tag.to_vec()), Operand::Dict(vec![(b"MCID".to_vec(), Operand::Int(mcid))])]);
    Ok(())
}

fn auto_tag(ctx: &mut Ctx) -> Result<()> {
    let doc = &mut ctx.doc;
    let root_id = doc.new_object_id();
    let doc_elem = doc.new_object_id();
    let mut doc_kids: Vec<Object> = Vec::new();
    let mut nums: Vec<Object> = Vec::new();
    for (pi, page) in util::pages(doc).into_iter().enumerate() {
        let res = util::inherited(doc, page, b"Resources").and_then(|o| util::dict_of(doc, &o).cloned());
        let images: BTreeSet<Vec<u8>> = res
            .as_ref()
            .and_then(|r| util::get_dict(doc, r, b"XObject"))
            .map(|x| {
                x.iter()
                    .filter(|(_, v)| matches!(deref(doc, v), Object::Stream(s) if util::get_name(doc, &s.dict, b"Subtype") == Some(b"Image".as_slice())))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default();
        let data = util::page_content(doc, page);
        let (new, elems) = tag_content(&data, &images);
        let cid = doc.add_object(Object::Stream(util::flate_stream(Dictionary::new(), &new)));
        let mut parent_arr: BTreeMap<i64, ObjectId> = BTreeMap::new();
        for (kind, mcids) in elems {
            let k: Object = if mcids.len() == 1 { Object::Integer(mcids[0]) } else { Object::Array(mcids.iter().map(|m| Object::Integer(*m)).collect()) };
            let mut e = dictionary! {
                "Type" => "StructElem",
                "S" => Object::Name(match kind { Kind::P => b"P".to_vec(), Kind::Figure => b"Figure".to_vec() }),
                "P" => doc_elem,
                "Pg" => page,
                "K" => k,
            };
            if kind == Kind::Figure {
                e.set("Alt", util::text_string("Bild"));
            }
            let eid = doc.add_object(e);
            doc_kids.push(Object::Reference(eid));
            for m in mcids {
                parent_arr.insert(m, eid);
            }
        }
        let arr: Vec<Object> = parent_arr.values().map(|e| Object::Reference(*e)).collect();
        nums.push(Object::Integer(pi as i64));
        nums.push(Object::Array(arr));
        if let Some(d) = dict_mut(doc, page) {
            d.set("Contents", Object::Reference(cid));
            d.set("StructParents", pi as i64);
        }
    }
    let n_pages = nums.len() / 2;
    doc.objects.insert(
        doc_elem,
        Object::Dictionary(dictionary! { "Type" => "StructElem", "S" => "Document", "P" => root_id, "K" => doc_kids }),
    );
    let pt = doc.add_object(dictionary! { "Nums" => nums });
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => doc_elem,
            "ParentTree" => pt,
            "ParentTreeNextKey" => n_pages as i64,
            "RoleMap" => Dictionary::new(),
        }),
    );
    if let Some(c) = util::catalog_mut(doc) {
        c.set("StructTreeRoot", root_id);
    }
    // Pages must not point into a structure that no longer exists.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(d) = dict_mut(doc, id) {
            if d.get(b"Type").ok().and_then(|t| t.as_name().ok()) != Some(b"Page".as_slice()) {
                d.remove(b"StructParent");
                if d.has(b"StructParents") && d.get(b"Subtype").ok().and_then(|t| t.as_name().ok()) == Some(b"Form".as_slice()) {
                    d.remove(b"StructParents");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_text_images_and_artifacts() {
        let src = b"q 1 0 0 1 0 0 cm BT /F1 12 Tf (a) Tj ET BT (b) Tj ET Q 0 0 m 10 10 l S q 10 0 0 10 0 0 cm /Im1 Do Q /Fm1 Do /Span <</MCID 4>> BDC EMC EMC";
        let mut imgs = BTreeSet::new();
        imgs.insert(b"Im1".to_vec());
        let (out, elems) = tag_content(src, &imgs);
        let s = String::from_utf8(out.clone()).unwrap();
        assert_eq!(elems.len(), 2);
        assert!(matches!(elems[0].0, Kind::P) && elems[0].1 == vec![0, 1]);
        assert!(matches!(elems[1].0, Kind::Figure) && elems[1].1 == vec![2]);
        assert!(s.contains("/P <</MCID 0>> BDC\nBT"), "{s}");
        assert!(s.contains("/Artifact BMC\n0 0 m"), "{s}");
        assert!(s.contains("/Figure <</MCID 2>> BDC\n/Im1 Do\nEMC"), "{s}");
        assert!(s.contains("/Artifact BMC\n/Fm1 Do\nEMC"), "{s}");
        assert!(s.contains("/Span BMC"), "old MCIDs dropped: {s}");
        let bdc = s.matches("BDC").count() + s.matches("BMC").count();
        assert_eq!(bdc, s.matches("EMC").count(), "balanced: {s}");
        // Unterminated text object.
        let (o2, _) = tag_content(b"BT (x) Tj", &BTreeSet::new());
        assert!(String::from_utf8(o2).unwrap().ends_with("ET\nEMC\n"));
    }

    #[test]
    fn languages_and_types() {
        assert!(valid_lang("de-DE") && valid_lang("en") && valid_lang("") && valid_lang("x-klingon"));
        assert!(!valid_lang("de_DE") && !valid_lang("123") && !valid_lang("de-"));
        let l1: Level = "1a".parse().unwrap();
        let l2: Level = "2a".parse().unwrap();
        assert!(standard(l1, b"P") && standard(l2, b"THead") && !standard(l1, b"THead") && !standard(l2, b"Foo"));
    }

    #[test]
    fn role_map_repair() {
        let mut doc = Document::with_version("1.7");
        let root = doc.add_object(dictionary! {
            "Type" => "StructTreeRoot",
            "RoleMap" => dictionary! { "P" => "Para", "Heading" => "Title", "Title" => "Heading", "Fig" => "Figure" },
        });
        doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Heading", "P" => root });
        doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Fig", "P" => root });
        doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Custom", "P" => root });
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "StructTreeRoot" => root });
        doc.trailer.set("Root", cat);
        repair_role_map(&mut doc, "2a".parse().unwrap());
        let rm = doc.get_object(root).unwrap().as_dict().unwrap().get(b"RoleMap").unwrap().as_dict().unwrap();
        assert!(!rm.has(b"P"));
        assert_eq!(rm.get(b"Fig").unwrap().as_name().unwrap(), b"Figure");
        assert_eq!(rm.get(b"Heading").unwrap().as_name().unwrap(), b"Span");
        assert_eq!(rm.get(b"Custom").unwrap().as_name().unwrap(), b"Span");
    }
}
