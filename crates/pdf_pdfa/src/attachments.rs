//! Embedded files per level: removed (part 1), converted to PDF/A or
//! removed (parts 2 and 4), kept and associated (parts 3 and 4f).

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::level::AttachmentPolicy;
use crate::util::{self, dict_mut};
use crate::{Ctx, PdfaError, Result};

/// Recursion guard for embedded PDFs that embed PDFs.
const MAX_DEPTH: u8 = 3;

pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "xml" => "text/xml",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "eml" => "message/rfc822",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// File specification dictionaries that carry embedded files.
fn filespecs(doc: &Document) -> Vec<ObjectId> {
    let mut v = Vec::new();
    for (&id, o) in &doc.objects {
        if let Object::Dictionary(d) = o {
            if d.has(b"EF") {
                v.push(id);
            }
        }
    }
    v
}

pub fn run(ctx: &mut Ctx) -> Result<()> {
    let policy = ctx.level.attachments();
    // Direct filespec dictionaries (in name trees / annotations) become
    // indirect so they can be edited in place.
    make_filespecs_indirect(&mut ctx.doc);
    let specs = filespecs(&ctx.doc);
    match policy {
        AttachmentPolicy::Forbidden => {
            let had_tree = remove_embedded_tree(&mut ctx.doc);
            for id in &specs {
                if let Some(d) = dict_mut(&mut ctx.doc, *id) {
                    d.remove(b"EF");
                    d.remove(b"RF");
                }
            }
            if had_tree || !specs.is_empty() {
                ctx.report.note("attachments_removed", format!("{}", specs.len()));
            }
            if let Some(c) = util::catalog_mut(&mut ctx.doc) {
                c.remove(b"AF");
            }
        }
        AttachmentPolicy::PdfaOnly => {
            let target = ctx.level.attachment_level();
            let mut dropped: BTreeSet<ObjectId> = BTreeSet::new();
            let mut converted = 0;
            let depth = ctx.opts.depth;
            for id in &specs {
                let data = embedded_data(&ctx.doc, *id);
                let ok = match data {
                    Some((sid, bytes)) if bytes.starts_with(b"%PDF") || bytes.windows(5).take(1024).any(|w| w == b"%PDF-") => {
                        if depth >= MAX_DEPTH {
                            false
                        } else {
                            let mut sub = ctx.opts.clone();
                            sub.level = target;
                            sub.depth = depth + 1;
                            match crate::convert(&bytes, &sub) {
                                Ok(c) => {
                                    replace_stream(&mut ctx.doc, sid, &c.bytes);
                                    converted += 1;
                                    true
                                }
                                Err(_) => false,
                            }
                        }
                    }
                    _ => false,
                };
                if ok {
                    fix_filespec(&mut ctx.doc, *id, ctx.level.part() == 4);
                } else {
                    dropped.insert(*id);
                }
            }
            if !dropped.is_empty() {
                drop_specs(&mut ctx.doc, &dropped);
                ctx.report.note("attachments_removed", format!("{}", dropped.len()));
            }
            if converted > 0 {
                ctx.report.note("attachments_converted", format!("{converted}"));
            }
            if ctx.level.part() == 4 {
                let specs = filespecs(&ctx.doc);
                associate(&mut ctx.doc, &specs);
            }
        }
        AttachmentPolicy::Any => {
            for id in &specs {
                fix_filespec(&mut ctx.doc, *id, true);
            }
            associate(&mut ctx.doc, &specs);
            if ctx.level.part() == 4 {
                // 4f: the EmbeddedFiles name tree must exist and hold them.
                ensure_in_tree(&mut ctx.doc, &specs);
                if specs.is_empty() {
                    return Err(PdfaError::AttachmentRequired);
                }
            }
        }
    }
    Ok(())
}

fn make_filespecs_indirect(doc: &mut Document) {
    // In the EmbeddedFiles name tree.
    if let Some(tree) = embedded_tree(doc) {
        let leaves = util::name_tree_leaves(doc, &tree);
        let mut changed = false;
        let mut new_leaves = Vec::new();
        for (k, v) in leaves {
            if let Object::Dictionary(d) = v {
                let id = doc.add_object(Object::Dictionary(d));
                new_leaves.push((k, Object::Reference(id)));
                changed = true;
            } else {
                new_leaves.push((k, v));
            }
        }
        if changed {
            set_embedded_tree(doc, Some(new_leaves));
        }
    }
    // In FileAttachment annotations.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let fs = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => match d.get(b"FS") {
                Ok(Object::Dictionary(fs)) => Some(fs.clone()),
                _ => None,
            },
            _ => None,
        };
        if let Some(fs) = fs {
            let fid = doc.add_object(Object::Dictionary(fs));
            if let Some(d) = dict_mut(doc, id) {
                d.set("FS", Object::Reference(fid));
            }
        }
    }
}

fn names_dict(doc: &Document) -> Option<Dictionary> {
    util::catalog(doc).and_then(|c| util::get_dict(doc, c, b"Names")).cloned()
}

fn embedded_tree(doc: &Document) -> Option<Object> {
    names_dict(doc).and_then(|n| n.get(b"EmbeddedFiles").ok().cloned())
}

/// Replace (Some) or remove (None) the EmbeddedFiles tree.
fn set_embedded_tree(doc: &mut Document, leaves: Option<Vec<(Vec<u8>, Object)>>) {
    let Some(cat_id) = util::catalog_id(doc) else { return };
    let names_ref = util::catalog(doc).and_then(|c| c.get(b"Names").ok().cloned());
    let mut names = names_dict(doc).unwrap_or_default();
    match leaves {
        Some(l) if !l.is_empty() => {
            let tree = util::make_name_tree(l);
            let tid = doc.add_object(Object::Dictionary(tree));
            names.set("EmbeddedFiles", Object::Reference(tid));
        }
        _ => {
            names.remove(b"EmbeddedFiles");
        }
    }
    match names_ref {
        Some(Object::Reference(r)) => {
            doc.objects.insert(r, Object::Dictionary(names));
        }
        _ => {
            if let Some(c) = dict_mut(doc, cat_id) {
                c.set("Names", Object::Dictionary(names));
            }
        }
    }
}

fn remove_embedded_tree(doc: &mut Document) -> bool {
    if embedded_tree(doc).is_some() {
        set_embedded_tree(doc, None);
        true
    } else {
        false
    }
}

/// (stream id, decoded bytes) of a filespec's embedded file.
fn embedded_data(doc: &Document, spec: ObjectId) -> Option<(ObjectId, Vec<u8>)> {
    let d = doc.objects.get(&spec)?.as_dict().ok()?;
    let ef = util::get_dict(doc, d, b"EF")?;
    let r = ef.get(b"UF").or_else(|_| ef.get(b"F")).ok()?.as_reference().ok()?;
    match doc.objects.get(&r)? {
        Object::Stream(s) => Some((r, util::stream_data(s)?)),
        _ => None,
    }
}

fn replace_stream(doc: &mut Document, sid: ObjectId, data: &[u8]) {
    let mut dict = match doc.objects.get(&sid) {
        Some(Object::Stream(s)) => s.dict.clone(),
        _ => Dictionary::new(),
    };
    dict.remove(b"DecodeParms");
    dict.remove(b"Length");
    if let Ok(Object::Dictionary(p)) = dict.get_mut(b"Params") {
        p.set("Size", data.len() as i64);
        p.remove(b"CheckSum");
    }
    doc.objects.insert(sid, Object::Stream(util::flate_stream(dict, data)));
}

/// Remove filespecs: from the name tree, FileAttachment annotations, /AF.
fn drop_specs(doc: &mut Document, dropped: &BTreeSet<ObjectId>) {
    if let Some(tree) = embedded_tree(doc) {
        let leaves: Vec<(Vec<u8>, Object)> = util::name_tree_leaves(doc, &tree)
            .into_iter()
            .filter(|(_, v)| !matches!(v, Object::Reference(r) if dropped.contains(r)))
            .collect();
        set_embedded_tree(doc, Some(leaves));
    }
    for page in util::pages(doc) {
        let annots = util::page_annots(doc, page);
        let n = annots.len();
        let kept: Vec<Object> = annots
            .into_iter()
            .filter(|a| {
                let fs = util::dict_of(doc, a).and_then(|d| d.get(b"FS").ok()).and_then(|f| f.as_reference().ok());
                !matches!(fs, Some(r) if dropped.contains(&r))
            })
            .collect();
        if kept.len() != n {
            util::set_page_annots(doc, page, kept);
        }
    }
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(d) = dict_mut(doc, id) {
            if let Ok(Object::Array(af)) = d.get(b"AF") {
                let kept: Vec<Object> = af.iter().filter(|o| !matches!(o, Object::Reference(r) if dropped.contains(r))).cloned().collect();
                if kept.is_empty() {
                    d.remove(b"AF");
                } else {
                    d.set("AF", Object::Array(kept));
                }
            }
        }
    }
    for id in dropped {
        doc.objects.remove(id);
    }
}

/// /Type /Filespec, F + UF, AFRelationship, MIME subtype, Params/ModDate.
fn fix_filespec(doc: &mut Document, spec: ObjectId, associated: bool) {
    let Some(d) = doc.objects.get(&spec).and_then(|o| o.as_dict().ok()).cloned() else { return };
    let name = d
        .get(b"UF")
        .ok()
        .and_then(util::decode_text)
        .or_else(|| d.get(b"F").ok().and_then(util::decode_text))
        .unwrap_or_else(|| "attachment".to_string());
    let mut nd = d.clone();
    nd.set("Type", Object::Name(b"Filespec".to_vec()));
    nd.set("F", util::text_string(&name));
    nd.set("UF", util::text_string(&name));
    if associated && !matches!(nd.get(b"AFRelationship"), Ok(Object::Name(_))) {
        nd.set("AFRelationship", Object::Name(b"Unspecified".to_vec()));
    }
    // One stream for both F and UF.
    if let Some(ef) = util::get_dict(doc, &d, b"EF").cloned() {
        let sref = ef.get(b"F").or_else(|_| ef.get(b"UF")).ok().cloned();
        if let Some(Object::Reference(sid)) = sref {
            let mut efd = Dictionary::new();
            efd.set("F", Object::Reference(sid));
            efd.set("UF", Object::Reference(sid));
            nd.set("EF", Object::Dictionary(efd));
            let (pdf_now, _) = util::now_dates();
            if let Some(Object::Stream(s)) = doc.objects.get_mut(&sid) {
                let size = util::stream_data(s).map(|d| d.len() as i64);
                s.dict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
                let valid = matches!(s.dict.get(b"Subtype"), Ok(Object::Name(n)) if is_mime(n));
                if !valid {
                    s.dict.set("Subtype", Object::Name(mime_for(&name).as_bytes().to_vec()));
                }
                let mut params = match s.dict.get(b"Params") {
                    Ok(Object::Dictionary(p)) => p.clone(),
                    _ => Dictionary::new(),
                };
                if !params.has(b"ModDate") {
                    params.set("ModDate", Object::string_literal(pdf_now.as_str()));
                }
                if let (false, Some(size)) = (params.has(b"Size"), size) {
                    params.set("Size", size);
                }
                s.dict.set("Params", Object::Dictionary(params));
            }
        }
    }
    doc.objects.insert(spec, Object::Dictionary(nd));
}

fn is_mime(n: &[u8]) -> bool {
    let s = String::from_utf8_lossy(n);
    let mut parts = s.split('/');
    let ok = |p: Option<&str>| p.map(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_+.".contains(c))).unwrap_or(false);
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// List every filespec in the catalog's /AF array.
fn associate(doc: &mut Document, specs: &[ObjectId]) {
    if specs.is_empty() {
        return;
    }
    let mut af: Vec<Object> = util::catalog(doc)
        .and_then(|c| util::get_array(doc, c, b"AF"))
        .cloned()
        .unwrap_or_default();
    for s in specs {
        if !af.iter().any(|o| o == &Object::Reference(*s)) {
            af.push(Object::Reference(*s));
        }
    }
    // Make sure every filespec in /AF still exists.
    af.retain(|o| matches!(o, Object::Reference(r) if doc.objects.contains_key(r)));
    if let Some(c) = util::catalog_mut(doc) {
        c.set("AF", Object::Array(af));
    }
}

fn ensure_in_tree(doc: &mut Document, specs: &[ObjectId]) {
    let mut leaves = embedded_tree(doc).map(|t| util::name_tree_leaves(doc, &t)).unwrap_or_default();
    for s in specs {
        if !leaves.iter().any(|(_, v)| v == &Object::Reference(*s)) {
            let name = doc
                .objects
                .get(s)
                .and_then(|o| o.as_dict().ok())
                .and_then(|d| d.get(b"UF").ok())
                .and_then(util::decode_text)
                .unwrap_or_else(|| format!("attachment{}", s.0));
            let mut key = name.clone().into_bytes();
            let mut i = 2;
            while leaves.iter().any(|(k, _)| k == &key) {
                key = format!("{name} ({i})").into_bytes();
                i += 1;
            }
            leaves.push((key, Object::Reference(*s)));
        }
    }
    if !leaves.is_empty() {
        set_embedded_tree(doc, Some(leaves));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_guess() {
        assert_eq!(mime_for("a.PDF"), "application/pdf");
        assert_eq!(mime_for("invoice.xml"), "text/xml");
        assert_eq!(mime_for("x"), "application/octet-stream");
        assert!(is_mime(b"application/pdf"));
        assert!(is_mime(b"application/vnd.ms-excel"));
        assert!(!is_mime(b"pdf"));
        assert!(!is_mime(b"a/b/c"));
    }
}
