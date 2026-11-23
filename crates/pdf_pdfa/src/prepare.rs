//! Loading and document-level clean-up before the real passes.

use lopdf::{Document, Object, ObjectId};

use crate::util::{self, deref, dict_mut};
use crate::{Ctx, PdfaError, Result};

/// Parse `input`; owner-password-only files are decrypted on load,
/// user-password files fail with [`PdfaError::PasswordRequired`].
pub fn load(input: &[u8]) -> Result<Document> {
    let mut doc = Document::load_mem(input).map_err(|e| {
        if matches!(e, lopdf::Error::Decryption(_)) {
            PdfaError::PasswordRequired
        } else {
            PdfaError::Parse(e.to_string())
        }
    })?;
    if doc.trailer.get(b"Encrypt").is_ok() {
        if doc.encryption_state.is_none() {
            return Err(PdfaError::PasswordRequired);
        }
        if let Ok(Object::Reference(id)) = doc.trailer.get(b"Encrypt").cloned() {
            doc.objects.remove(&id);
        }
        doc.trailer.remove(b"Encrypt");
        doc.encryption_state = None;
    }
    if doc.page_iter().next().is_none() {
        return Err(PdfaError::Parse("the document has no pages".into()));
    }
    Ok(doc)
}

pub fn run(ctx: &mut Ctx) -> Result<()> {
    let level = ctx.level;
    let doc = &mut ctx.doc;
    for k in [b"Prev".as_slice(), b"XRefStm", b"Encrypt"] {
        doc.trailer.remove(k);
    }

    let cat_id = util::catalog_id(doc).ok_or_else(|| PdfaError::Parse("no document catalog".into()))?;
    if let Some(cat) = dict_mut(doc, cat_id) {
        for k in [b"NeedsRendering".as_slice(), b"Requirements", b"Version", b"Perms"] {
            cat.remove(k);
        }
        if level.part() == 4 {
            cat.remove(b"PieceInfo");
        }
    }
    // Names: AlternatePresentations.
    let names = util::catalog(doc)
        .and_then(|c| c.get(b"Names").ok().cloned());
    if let Some(names) = names {
        match names {
            Object::Reference(id) => {
                if let Some(d) = dict_mut(doc, id) {
                    d.remove(b"AlternatePresentations");
                }
            }
            Object::Dictionary(mut d) => {
                d.remove(b"AlternatePresentations");
                if let Some(c) = dict_mut(doc, cat_id) {
                    c.set("Names", Object::Dictionary(d));
                }
            }
            _ => {}
        }
    }

    for page in util::pages(doc) {
        if let Some(d) = dict_mut(doc, page) {
            d.remove(b"PresSteps");
            if level.part() == 4 {
                d.remove(b"PieceInfo");
            }
        }
    }
    if level.part() == 4 {
        let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
        for id in ids {
            if let Some(d) = dict_mut(doc, id) {
                d.remove(b"PieceInfo");
            }
        }
    }

    make_fonts_indirect(doc);
    remove_signatures(ctx);
    Ok(())
}

/// Direct font dictionaries inside resources become indirect objects so the
/// font passes can address every font by object id.
fn make_fonts_indirect(doc: &mut Document) {
    for loc in util::resource_locations(doc) {
        let fonts = match util::res_ref(doc, loc).and_then(|r| r.get(b"Font").ok()).cloned() {
            Some(f) => f,
            None => continue,
        };
        let (mut fdict, fonts_ref) = match fonts {
            Object::Dictionary(d) => (d, None),
            Object::Reference(id) => match doc.objects.get(&id) {
                Some(Object::Dictionary(d)) => (d.clone(), Some(id)),
                _ => continue,
            },
            _ => continue,
        };
        let mut changed = false;
        let keys: Vec<Vec<u8>> = fdict.iter().map(|(k, _)| k.clone()).collect();
        for k in keys {
            if let Ok(Object::Dictionary(fd)) = fdict.get(&k) {
                let fd = fd.clone();
                let id = doc.add_object(Object::Dictionary(fd));
                fdict.set(k, Object::Reference(id));
                changed = true;
            }
        }
        if !changed {
            continue;
        }
        match fonts_ref {
            Some(id) => {
                doc.objects.insert(id, Object::Dictionary(fdict));
            }
            None => {
                if let Some(r) = util::res_mut(doc, loc) {
                    r.set("Font", Object::Dictionary(fdict));
                }
            }
        }
    }
    // ExtGState /Font [font size] entries.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let font = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => match d.get(b"Font") {
                Ok(Object::Array(a)) if matches!(a.first(), Some(Object::Dictionary(_))) => a.clone(),
                _ => continue,
            },
            _ => continue,
        };
        if let Some(Object::Dictionary(fd)) = font.first().cloned() {
            let fid = doc.add_object(Object::Dictionary(fd));
            let mut arr = font;
            arr[0] = Object::Reference(fid);
            if let Some(d) = dict_mut(doc, id) {
                d.set("Font", Object::Array(arr));
            }
        }
    }
}

/// Any rewrite invalidates digital signatures; keep the fields but drop the
/// signature values (PDF/A also restricts what signature dictionaries may
/// contain).
fn remove_signatures(ctx: &mut Ctx) {
    let doc = &mut ctx.doc;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut removed = 0;
    for id in ids {
        let is_sig_field = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => {
                let ft = d.get(b"FT").ok().map(|o| deref(doc, o));
                matches!(ft, Some(Object::Name(n)) if n == b"Sig") && d.has(b"V")
            }
            _ => false,
        };
        if is_sig_field {
            if let Some(d) = dict_mut(doc, id) {
                d.remove(b"V");
                d.remove(b"Lock");
                d.remove(b"SV");
                removed += 1;
            }
        }
    }
    if let Some(af) = util::catalog(doc).and_then(|c| c.get(b"AcroForm").ok().cloned()) {
        match af {
            Object::Reference(id) => {
                if let Some(d) = dict_mut(doc, id) {
                    d.remove(b"SigFlags");
                }
            }
            Object::Dictionary(mut d) => {
                d.remove(b"SigFlags");
                if let Some(c) = util::catalog_mut(doc) {
                    c.set("AcroForm", Object::Dictionary(d));
                }
            }
            _ => {}
        }
    }
    if removed > 0 {
        ctx.report.note("signatures_removed", format!("{removed}"));
    }
}
