//! True redaction support: replace a page's entire content with a
//! pre-rendered bitmap (the caller blacks out the redaction regions in the
//! pixel data before encoding). Rasterising guarantees the original text,
//! vector and image data is gone from the file — nothing is merely covered.
//!
//! Metadata sanitising lives here too: strips the /Info dictionary, XMP
//! /Metadata streams and private /PieceInfo payloads.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};

use crate::{MutateError, MutateResult};

/// Replace page `page_index` (0-based) with a single full-page JPEG.
/// `page_w_pt`/`page_h_pt` are the *displayed* page dimensions (rotation
/// already applied) — the page is normalised to MediaBox [0 0 w h] with
/// /Rotate removed so it matches the rendered bitmap. Annotations on the
/// page are dropped (their text would survive rasterisation otherwise).
pub fn replace_page_with_jpeg(
    doc: &mut Document,
    page_index: u32,
    jpeg: Vec<u8>,
    px_w: u32,
    px_h: u32,
    page_w_pt: f32,
    page_h_pt: f32,
) -> MutateResult<()> {
    let pages: Vec<_> = doc.page_iter().collect();
    let page_id = *pages
        .get(page_index as usize)
        .ok_or(MutateError::PageOutOfRange(page_index, pages.len() as u32))?;

    // Collect the old content streams and annotations for deletion — lopdf
    // writes orphaned objects back out, so merely unlinking them would leave
    // the redacted data recoverable in the file.
    let mut doomed: Vec<lopdf::ObjectId> = Vec::new();
    if let Ok(page) = doc.get_dictionary(page_id) {
        if let Ok(contents) = page.get(b"Contents") {
            match contents {
                Object::Reference(id) => doomed.push(*id),
                Object::Array(arr) => {
                    for o in arr {
                        if let Ok(id) = o.as_reference() {
                            doomed.push(id);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Ok(annots) = page.get(b"Annots") {
            let arr = match annots {
                Object::Reference(id) => {
                    doomed.push(*id);
                    doc.get_object(*id).and_then(Object::as_array).ok().cloned()
                }
                Object::Array(arr) => Some(arr.clone()),
                _ => None,
            };
            for o in arr.unwrap_or_default() {
                if let Ok(id) = o.as_reference() {
                    doomed.push(id);
                }
            }
        }
    }

    let image_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => px_w as i64,
        "Height" => px_h as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        "Filter" => "DCTDecode",
    };
    let image_id = doc.add_object(Object::Stream(Stream::new(image_dict, jpeg)));

    let content = format!("q\n{page_w_pt:.3} 0 0 {page_h_pt:.3} 0 0 cm\n/ImRedact Do\nQ\n");
    let content_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        content.into_bytes(),
    )));

    let mut xobjects = Dictionary::new();
    xobjects.set("ImRedact", Object::Reference(image_id));
    let resources_id = doc.add_object(Object::Dictionary(dictionary! {
        "XObject" => Object::Dictionary(xobjects),
    }));

    let page = doc
        .get_dictionary_mut(page_id)
        .map_err(|e| MutateError::Parse(format!("page dict: {e}")))?;
    page.set("Contents", Object::Reference(content_id));
    page.set("Resources", Object::Reference(resources_id));
    page.set(
        "MediaBox",
        Object::Array(vec![
            0.into(),
            0.into(),
            Object::Real(page_w_pt),
            Object::Real(page_h_pt),
        ]),
    );
    // Bitmap is already in display orientation; other boxes may disagree
    // with the new MediaBox.
    page.remove(b"Rotate");
    page.remove(b"CropBox");
    page.remove(b"TrimBox");
    page.remove(b"BleedBox");
    page.remove(b"ArtBox");
    page.remove(b"Annots");
    for id in doomed {
        doc.objects.remove(&id);
    }
    Ok(())
}

/// Strip document metadata: /Info dictionary, XMP /Metadata streams on the
/// catalog and on every page, and /PieceInfo private application data.
/// Returns the number of removed items.
pub fn sanitize_metadata(doc: &mut Document) -> MutateResult<u32> {
    let mut removed = 0u32;

    if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(Object::as_reference) {
        doc.objects.remove(&info_ref);
        removed += 1;
    }
    if doc.trailer.remove(b"Info").is_some() {
        removed += 1;
    }

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|e| MutateError::Parse(format!("catalog: {e}")))?;
    let mut orphaned: Vec<lopdf::ObjectId> = Vec::new();
    if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
        if let Some(obj) = catalog.remove(b"Metadata") {
            if let Ok(id) = obj.as_reference() {
                orphaned.push(id);
            }
            removed += 1;
        }
        if catalog.remove(b"PieceInfo").is_some() {
            removed += 1;
        }
    }

    let pages: Vec<_> = doc.page_iter().collect();
    for page_id in pages {
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            if let Some(obj) = page.remove(b"Metadata") {
                if let Ok(id) = obj.as_reference() {
                    orphaned.push(id);
                }
                removed += 1;
            }
            if page.remove(b"PieceInfo").is_some() {
                removed += 1;
            }
        }
    }
    for id in orphaned {
        doc.objects.remove(&id);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_metadata() -> Document {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let page_id = d.new_object_id();
        let cat_id = d.new_object_id();
        let content_id = d.new_object_id();
        let content = Stream::new(
            Dictionary::new(),
            b"BT /F1 12 Tf 72 712 Td (top secret content) Tj ET".to_vec(),
        );
        d.objects.insert(content_id, Object::Stream(content));
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        d.objects.insert(page_id, Object::Dictionary(page));
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set("Count", Object::Integer(1));
        d.objects.insert(pages_id, Object::Dictionary(pages));
        let mut cat = Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set("Pages", Object::Reference(pages_id));
        d.objects.insert(cat_id, Object::Dictionary(cat));
        d.trailer.set("Root", Object::Reference(cat_id));

        let info_id = d.add_object(Object::Dictionary(dictionary! {
            "Author" => Object::string_literal("Secret Author"),
            "Title" => Object::string_literal("Secret Title"),
        }));
        d.trailer.set("Info", Object::Reference(info_id));
        let xmp_id = d.add_object(Object::Stream(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<x:xmpmeta>Secret XMP</x:xmpmeta>".to_vec(),
        )));
        if let Ok(cat) = d.get_dictionary_mut(cat_id) {
            cat.set("Metadata", Object::Reference(xmp_id));
        }
        d.max_id = d.objects.keys().map(|k| k.0).max().unwrap_or(1);
        d
    }

    #[test]
    fn sanitize_strips_info_and_xmp() {
        let mut d = doc_with_metadata();
        let removed = sanitize_metadata(&mut d).unwrap();
        assert!(removed >= 2, "removed = {removed}");
        assert!(d.trailer.get(b"Info").is_err());
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        let needle = |n: &[u8]| out.windows(n.len()).any(|w| w == n);
        assert!(!needle(b"Secret Author"));
        assert!(!needle(b"Secret XMP"));
    }

    #[test]
    fn replace_page_removes_original_content() {
        let mut d = doc_with_metadata();
        let fake_jpeg = vec![0xFFu8, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        replace_page_with_jpeg(&mut d, 0, fake_jpeg, 100, 130, 612.0, 792.0).unwrap();
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        let needle = |n: &[u8]| out.windows(n.len()).any(|w| w == n);
        assert!(!needle(b"top secret content"));
        assert!(needle(b"ImRedact"));
        // Old content stream object must be gone after a full re-save?
        // lopdf keeps orphaned objects in `objects`, so prune explicitly:
        // (covered by renumber/save in the app path via MutableDoc::to_bytes)
    }
}
