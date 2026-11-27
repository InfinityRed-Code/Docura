//! Structural PDF mutation via `lopdf`.
//!
//! Pdfium is used for rendering and text extraction. For page-level edits
//! (delete, insert, reorder, rotate persistent, merge, split, extract) the
//! byte stream has to be rewritten — lopdf owns that side of the pipeline.
//!
//! Usage pattern: load bytes into `MutableDoc`, apply mutations, serialise
//! back to bytes, reload pdfium so rendering sees the new state.

use std::io::Cursor;

use lopdf::{Document, Object, ObjectId};
use pdf_types::RectPt;

mod annotations;
mod encryption;
mod form;
mod form_ap;
mod images;
mod inspect;
mod ocr;
mod overlay;
mod read;
mod truetype;
mod redact;
mod std_metrics;
mod threed;
mod xfdf;

pub use annotations::{
    FreeTextAnnotation, InkAnnotation, LineEnding, MarkupAnnotation, MarkupKind,
    ShapeAnnotation, ShapeKind, StampAnnotation, TextNoteAnnotation,
};
pub use encryption::{encrypt_pdf, is_encrypted_pdf, probably_encrypted, PermissionOptions};
pub use form_ap::auto_size_single;
pub use images::images_to_pdf;
pub use threed::{c2w_look_at, ThreeDAnnotation};
pub use ocr::OcrWord;
pub use overlay::{HeaderFooterOptions, ImageAnchor, ImageWatermarkOptions, WatermarkOptions};

pub type MutateResult<T> = Result<T, MutateError>;

#[derive(Debug, thiserror::Error)]
pub enum MutateError {
    #[error("parse: {0}")]
    Parse(String),
    #[error("serialize: {0}")]
    Serialize(String),
    #[error("page {0} out of range (doc has {1} pages)")]
    PageOutOfRange(u32, u32),
    #[error("cannot delete the only remaining page")]
    LastPage,
    #[error("crypt: {0}")]
    Crypt(String),
}

pub struct MutableDoc {
    inner: Document,
}

pub(crate) fn resolve_media_box(doc: &Document, page_id: ObjectId) -> Option<[f32; 4]> {
    resolve_page_box(doc, page_id, b"MediaBox")
}

/// The page region viewers display and pdfium renders: CropBox ∩ MediaBox,
/// falling back to the MediaBox. All top-origin UI coordinates are relative
/// to this box — pages with print marks / scan offsets have a non-zero
/// origin here.
pub(crate) fn resolve_view_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    let media = resolve_media_box(doc, page_id).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    match resolve_page_box(doc, page_id, b"CropBox") {
        Some(c) => {
            let b = [
                c[0].max(media[0]),
                c[1].max(media[1]),
                c[2].min(media[2]),
                c[3].min(media[3]),
            ];
            if b[2] > b[0] && b[3] > b[1] { b } else { media }
        }
        None => media,
    }
}

/// Upper bound for /Parent chain walks. Real page trees are a handful of
/// levels deep; the cap only exists so a malformed /Parent cycle cannot hang.
const MAX_TREE_DEPTH: usize = 256;

/// Page-tree attributes a page inherits from its /Pages ancestors
/// (PDF 32000-1 §7.7.3.4).
const INHERITABLE_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Value of `key` on the page or the nearest /Pages ancestor carrying it.
pub(crate) fn inherited_attr(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut cur = page_id;
    for _ in 0..MAX_TREE_DEPTH {
        let d = doc.get_dictionary(cur).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(v.clone());
        }
        cur = d.get(b"Parent").and_then(|o| o.as_reference()).ok()?;
    }
    None
}

/// Copy inherited page attributes onto the page dictionary itself, so the
/// page keeps them when it is re-parented or its /Pages ancestors are pruned.
pub(crate) fn materialize_inherited_attrs(doc: &mut Document, page_id: ObjectId) {
    for key in INHERITABLE_PAGE_KEYS {
        let has_own = doc
            .get_dictionary(page_id)
            .map(|d| d.has(key))
            .unwrap_or(true);
        if has_own {
            continue;
        }
        if let Some(v) = inherited_attr(doc, page_id, key) {
            if let Ok(d) = doc.get_dictionary_mut(page_id) {
                d.set(key.to_vec(), v);
            }
        }
    }
}

/// Drop every object no longer reachable from the trailer. lopdf writes all
/// objects it holds, so unlinked content (e.g. a redacted page's images or
/// form XObjects) would otherwise still be in the saved file.
pub(crate) fn prune_unreachable(doc: &mut Document) {
    fn collect(obj: &Object, out: &mut Vec<ObjectId>) {
        match obj {
            Object::Reference(id) => out.push(*id),
            Object::Array(a) => a.iter().for_each(|o| collect(o, out)),
            Object::Dictionary(d) => d.iter().for_each(|(_, o)| collect(o, out)),
            Object::Stream(s) => s.dict.iter().for_each(|(_, o)| collect(o, out)),
            _ => {}
        }
    }
    let mut seen: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    let mut stack: Vec<ObjectId> = Vec::new();
    for (_, o) in doc.trailer.iter() {
        collect(o, &mut stack);
    }
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(o) = doc.objects.get(&id) {
            collect(o, &mut stack);
        }
    }
    doc.objects.retain(|id, _| seen.contains(id));
}

/// Persistent /Rotate of the page (inherited), normalised to 0/90/180/270.
pub(crate) fn page_rotation(doc: &Document, page_id: ObjectId) -> i32 {
    let rot = inherited_attr(doc, page_id, b"Rotate")
        .and_then(|o| o.as_i64().ok().or_else(|| o.as_float().ok().map(|f| f as i64)));
    match rot {
        Some(r) => ((r % 360 + 360) % 360) as i32,
        None => 0,
    }
}

/// Map a point from displayed top-origin space (UI / pdfium bitmap, rotation
/// applied) into PDF user space (unrotated, view-box offset added).
pub(crate) fn point_to_pdf_space(x: f32, y: f32, vb: [f32; 4], rot: i32) -> (f32, f32) {
    let w = vb[2] - vb[0];
    let h = vb[3] - vb[1];
    // Invert the display rotation first (top-origin, unrotated)…
    let (xu, yu) = match rot {
        90 => (y, h - x),
        180 => (w - x, h - y),
        270 => (w - y, x),
        _ => (x, y),
    };
    // …then convert top-origin → PDF bottom-origin with the box offset.
    (vb[0] + xu, vb[3] - yu)
}

pub(crate) fn resolve_page_box(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<[f32; 4]> {
    let mut cur = page_id;
    for _ in 0..MAX_TREE_DEPTH {
        if let Ok(d) = doc.get_dictionary(cur) {
            if let Ok(mb) = d.get(key) {
                // The box array may be an indirect object.
                let mb = match mb {
                    Object::Reference(r) => doc.get_object(*r).unwrap_or(mb),
                    other => other,
                };
                if let Ok(arr) = mb.as_array() {
                    if arr.len() == 4 {
                        let to_f = |o: &Object| {
                            o.as_float()
                                .ok()
                                .or_else(|| o.as_i64().ok().map(|i| i as f32))
                        };
                        let (x0, y0, x1, y1) = (
                            to_f(&arr[0]).unwrap_or(0.0),
                            to_f(&arr[1]).unwrap_or(0.0),
                            to_f(&arr[2]).unwrap_or(612.0),
                            to_f(&arr[3]).unwrap_or(792.0),
                        );
                        // A rectangle may name any two opposite corners
                        // (§7.9.5); normalise to [llx lly urx ury].
                        return Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]);
                    }
                }
            }
            if let Ok(parent) = d.get(b"Parent").and_then(|o| o.as_reference()) {
                cur = parent;
                continue;
            }
        }
        break;
    }
    None
}

impl MutableDoc {
    pub fn from_bytes(bytes: &[u8]) -> MutateResult<Self> {
        let t = std::time::Instant::now();
        let doc = Document::load_mem(bytes).map_err(|e| MutateError::Parse(e.to_string()))?;
        tracing::info!(
            "pdf_mutate::from_bytes: {:.1}KB parsed in {:?}",
            bytes.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(Self { inner: doc })
    }

    pub fn to_bytes(&mut self) -> MutateResult<Vec<u8>> {
        let t = std::time::Instant::now();
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut cur = Cursor::new(&mut buf);
            self.inner
                .save_to(&mut cur)
                .map_err(|e| MutateError::Serialize(e.to_string()))?;
        }
        tracing::info!(
            "pdf_mutate::to_bytes: {:.1}KB written in {:?}",
            buf.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(buf)
    }

    pub fn page_count(&self) -> u32 {
        self.inner.get_pages().len() as u32
    }

    /// Delete a single 0-indexed page. Returns new page count.
    pub fn delete_page(&mut self, page_index: u32) -> MutateResult<u32> {
        self.delete_pages(&[page_index])
    }

    /// Reorder pages according to `new_order`, a permutation of 0..page_count.
    /// Flattens the Pages tree into a single root Kids array.
    pub fn reorder_pages(&mut self, new_order: &[u32]) -> MutateResult<()> {
        let t = std::time::Instant::now();
        let total = self.page_count();
        if new_order.len() as u32 != total {
            return Err(MutateError::Parse(format!(
                "reorder: length {} != page count {total}",
                new_order.len()
            )));
        }
        let mut seen = vec![false; total as usize];
        for &i in new_order {
            if i >= total {
                return Err(MutateError::PageOutOfRange(i, total));
            }
            if seen[i as usize] {
                return Err(MutateError::Parse(format!("reorder: duplicate index {i}")));
            }
            seen[i as usize] = true;
        }

        let ordered_ids: Vec<lopdf::ObjectId> = self.inner.page_iter().collect();
        let root_id = self
            .inner
            .trailer
            .get(b"Root")
            .map_err(|e| MutateError::Parse(e.to_string()))?
            .as_reference()
            .map_err(|e| MutateError::Parse(e.to_string()))?;
        let pages_id = self
            .inner
            .get_dictionary(root_id)
            .map_err(|e| MutateError::Parse(e.to_string()))?
            .get(b"Pages")
            .map_err(|e| MutateError::Parse(e.to_string()))?
            .as_reference()
            .map_err(|e| MutateError::Parse(e.to_string()))?;

        for &pid in &ordered_ids {
            // Intermediate /Pages nodes are pruned below; keep what they passed down.
            materialize_inherited_attrs(&mut self.inner, pid);
            if let Ok(d) = self.inner.get_dictionary_mut(pid) {
                d.set("Parent", lopdf::Object::Reference(pages_id));
            }
        }
        let kids: Vec<lopdf::Object> = new_order
            .iter()
            .map(|&i| lopdf::Object::Reference(ordered_ids[i as usize]))
            .collect();
        let count = kids.len() as i64;
        if let Ok(d) = self.inner.get_dictionary_mut(pages_id) {
            d.set("Kids", lopdf::Object::Array(kids));
            d.set("Count", lopdf::Object::Integer(count));
        }
        self.inner.prune_objects();
        tracing::info!(
            "pdf_mutate::reorder_pages n={} in {:?}",
            new_order.len(),
            t.elapsed()
        );
        Ok(())
    }

    /// Rotate pages by `degrees` relative to their current /Rotate value.
    /// `degrees` must be a multiple of 90 (positive = clockwise). An inherited
    /// /Rotate counts as the current value; missing is 0. 0-indexed page list.
    pub fn rotate_pages(&mut self, page_indices: &[u32], degrees: i32) -> MutateResult<()> {
        let t = std::time::Instant::now();
        if page_indices.is_empty() {
            return Ok(());
        }
        if degrees % 90 != 0 {
            return Err(MutateError::Parse(format!(
                "rotation must be a multiple of 90, got {degrees}"
            )));
        }
        let total = self.page_count();
        let mut dedup: Vec<u32> = page_indices.to_vec();
        dedup.sort_unstable();
        dedup.dedup();
        if let Some(&max) = dedup.last() {
            if max >= total {
                return Err(MutateError::PageOutOfRange(max, total));
            }
        }
        let pages_map = self.inner.get_pages();
        for idx in &dedup {
            let page_num = idx + 1;
            let Some(&page_id) = pages_map.get(&page_num) else {
                continue;
            };
            // Relative to the effective rotation, which may be inherited from
            // a /Pages ancestor — setting it on the page overrides that value.
            let current = page_rotation(&self.inner, page_id);
            let new_rot = ((current + degrees % 360) % 360 + 360) % 360;
            if let Ok(d) = self.inner.get_dictionary_mut(page_id) {
                d.set("Rotate", lopdf::Object::Integer(new_rot as i64));
            }
        }
        tracing::info!(
            "pdf_mutate::rotate_pages n={} deg={} in {:?}",
            dedup.len(),
            degrees,
            t.elapsed()
        );
        Ok(())
    }

    /// Serialize a copy of the document containing only `page_indices`
    /// (0-indexed), preserving their original order. Duplicates ignored,
    /// input does not need to be sorted.
    pub fn extract_to_bytes(&self, page_indices: &[u32]) -> MutateResult<Vec<u8>> {
        let t = std::time::Instant::now();
        let total = self.page_count();
        if page_indices.is_empty() {
            return Err(MutateError::Parse("extract: empty page list".into()));
        }
        let mut keep: Vec<u32> = page_indices.to_vec();
        keep.sort_unstable();
        keep.dedup();
        if let Some(&max) = keep.last() {
            if max >= total {
                return Err(MutateError::PageOutOfRange(max, total));
            }
        }
        let keep_set: std::collections::HashSet<u32> = keep.iter().copied().collect();
        let delete_one_indexed: Vec<u32> = (0..total)
            .filter(|i| !keep_set.contains(i))
            .map(|i| i + 1)
            .collect();
        let mut copy = self.inner.clone();
        if !delete_one_indexed.is_empty() {
            copy.delete_pages(&delete_one_indexed);
            // lopdf only unlinks the page dictionaries; without this the
            // dropped pages' content and images would still be in the file.
            prune_unreachable(&mut copy);
        }
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut cur = Cursor::new(&mut buf);
            copy.save_to(&mut cur)
                .map_err(|e| MutateError::Serialize(e.to_string()))?;
        }
        tracing::info!(
            "pdf_mutate::extract_to_bytes n={} → {:.1}KB in {:?}",
            keep.len(),
            buf.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(buf)
    }

    /// Serialize a single-page PDF cropped to `rect` on `page_index`.
    /// `rect` is in our top-origin RectPt convention (PDF points on the page
    /// as displayed, /Rotate applied — same convention used for annotation
    /// rects). MediaBox + CropBox are rewritten to the region;
    /// any persistent /Rotate on the source page is preserved so the cropped
    /// page renders with the same orientation.
    pub fn extract_region_to_bytes(
        &self,
        page_index: u32,
        rect: RectPt,
    ) -> MutateResult<Vec<u8>> {
        let t = std::time::Instant::now();
        let total = self.page_count();
        if page_index >= total {
            return Err(MutateError::PageOutOfRange(page_index, total));
        }
        if !(rect.x1 > rect.x0) || !(rect.y1 > rect.y0) {
            return Err(MutateError::Parse(format!(
                "region: invalid rect {:?}",
                rect
            )));
        }
        let mut copy = self.inner.clone();
        let delete_one_indexed: Vec<u32> = (0..total)
            .filter(|&i| i != page_index)
            .map(|i| i + 1)
            .collect();
        if !delete_one_indexed.is_empty() {
            copy.delete_pages(&delete_one_indexed);
            prune_unreachable(&mut copy);
        }
        let page_id = copy
            .page_iter()
            .next()
            .ok_or_else(|| MutateError::Parse("region: no page after extract".into()))?;
        // UI rects are relative to the displayed region (CropBox ∩ MediaBox).
        let mb = resolve_view_box(&copy, page_id);
        let mb_left = mb[0];
        let mb_bottom = mb[1];
        let mb_right = mb[2];
        let mb_top = mb[3];
        // The rect is drawn on the displayed page, i.e. with /Rotate applied.
        let rot = page_rotation(&copy, page_id);
        let a = point_to_pdf_space(rect.x0, rect.y0, mb, rot);
        let b = point_to_pdf_space(rect.x1, rect.y1, mb, rot);
        let new_llx = a.0.min(b.0).max(mb_left).min(mb_right);
        let new_urx = a.0.max(b.0).max(mb_left).min(mb_right);
        let new_lly = a.1.min(b.1).max(mb_bottom).min(mb_top);
        let new_ury = a.1.max(b.1).max(mb_bottom).min(mb_top);
        if new_urx - new_llx < 1.0 || new_ury - new_lly < 1.0 {
            return Err(MutateError::Parse(
                "region: rect outside page bounds".into(),
            ));
        }
        let new_box = Object::Array(vec![
            Object::Real(new_llx),
            Object::Real(new_lly),
            Object::Real(new_urx),
            Object::Real(new_ury),
        ]);
        if let Ok(d) = copy.get_dictionary_mut(page_id) {
            d.set("MediaBox", new_box.clone());
            d.set("CropBox", new_box);
            d.remove(b"BleedBox");
            d.remove(b"TrimBox");
            d.remove(b"ArtBox");
        }
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut cur = Cursor::new(&mut buf);
            copy.save_to(&mut cur)
                .map_err(|e| MutateError::Serialize(e.to_string()))?;
        }
        tracing::info!(
            "pdf_mutate::extract_region_to_bytes page={} rect={:?} → {:.1}KB in {:?}",
            page_index,
            rect,
            buf.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(buf)
    }

    /// Delete many 0-indexed pages in one pass. Duplicates ignored, input
    /// does not need to be sorted. Returns new page count.
    pub fn delete_pages(&mut self, page_indices: &[u32]) -> MutateResult<u32> {
        let total = self.page_count();
        if page_indices.is_empty() {
            return Ok(total);
        }
        let mut dedup: Vec<u32> = page_indices.to_vec();
        dedup.sort_unstable();
        dedup.dedup();
        if let Some(&max) = dedup.last() {
            if max >= total {
                return Err(MutateError::PageOutOfRange(max, total));
            }
        }
        if dedup.len() as u32 >= total {
            return Err(MutateError::LastPage);
        }
        // lopdf uses 1-indexed page numbers.
        let one_indexed: Vec<u32> = dedup.iter().map(|p| p + 1).collect();
        self.inner.delete_pages(&one_indexed);
        // Drop the deleted pages' now-unreferenced content streams, images
        // and fonts, which lopdf would otherwise still write out.
        prune_unreachable(&mut self.inner);
        Ok(self.page_count())
    }

    /// Insert pages from another PDF at 0-indexed position `at_index` (0..=page_count).
    /// `src_pages = None` imports every page from source; otherwise only the listed
    /// 0-indexed source pages, preserving given order. Returns new page count.
    pub fn insert_pdf(
        &mut self,
        src_bytes: &[u8],
        src_pages: Option<&[u32]>,
        at_index: u32,
    ) -> MutateResult<u32> {
        let t = std::time::Instant::now();
        let total = self.page_count();
        if at_index > total {
            return Err(MutateError::PageOutOfRange(at_index, total));
        }
        let mut src =
            Document::load_mem(src_bytes).map_err(|e| MutateError::Parse(e.to_string()))?;
        let start_id = self.inner.max_id + 1;
        src.renumber_objects_with(start_id);
        let src_max = src.max_id;

        let src_page_ids: Vec<lopdf::ObjectId> = src.page_iter().collect();
        let src_count = src_page_ids.len() as u32;
        let keep_page_ids: Vec<lopdf::ObjectId> = match src_pages {
            None => src_page_ids.clone(),
            Some(indices) => {
                let mut v = Vec::with_capacity(indices.len());
                for &i in indices {
                    if i >= src_count {
                        return Err(MutateError::PageOutOfRange(i, src_count));
                    }
                    v.push(src_page_ids[i as usize]);
                }
                v
            }
        };
        if keep_page_ids.is_empty() {
            return Err(MutateError::Parse("insert_pdf: empty page list".into()));
        }

        let root_id = self
            .inner
            .trailer
            .get(b"Root")
            .map_err(|e| MutateError::Parse(e.to_string()))?
            .as_reference()
            .map_err(|e| MutateError::Parse(e.to_string()))?;
        let pages_id = self
            .inner
            .get_dictionary(root_id)
            .map_err(|e| MutateError::Parse(e.to_string()))?
            .get(b"Pages")
            .map_err(|e| MutateError::Parse(e.to_string()))?
            .as_reference()
            .map_err(|e| MutateError::Parse(e.to_string()))?;

        let keep_set: std::collections::HashSet<lopdf::ObjectId> =
            keep_page_ids.iter().copied().collect();
        // The source /Pages nodes are not copied, so pages must carry the
        // attributes (MediaBox, Resources, …) they inherited from them.
        for &pid in &keep_page_ids {
            materialize_inherited_attrs(&mut src, pid);
        }
        for (oid, obj) in src.objects.into_iter() {
            let typ = obj.type_name().unwrap_or(b"").to_vec();
            match typ.as_slice() {
                b"Catalog" | b"Pages" | b"Outlines" | b"Outline" => continue,
                b"Page" => {
                    if keep_set.contains(&oid) {
                        if let Ok(d) = obj.as_dict() {
                            let mut d = d.clone();
                            d.set("Parent", lopdf::Object::Reference(pages_id));
                            self.inner.objects.insert(oid, lopdf::Object::Dictionary(d));
                        }
                    }
                }
                _ => {
                    self.inner.objects.insert(oid, obj);
                }
            }
        }
        self.inner.max_id = self.inner.max_id.max(src_max);

        // Flatten existing Pages tree, splice new refs at `at_index`, rewrite root Kids.
        let ordered_ids: Vec<lopdf::ObjectId> = self.inner.page_iter().collect();
        for &pid in &ordered_ids {
            materialize_inherited_attrs(&mut self.inner, pid);
            if let Ok(d) = self.inner.get_dictionary_mut(pid) {
                d.set("Parent", lopdf::Object::Reference(pages_id));
            }
        }
        let mut flat: Vec<lopdf::Object> = ordered_ids
            .iter()
            .map(|&id| lopdf::Object::Reference(id))
            .collect();
        let new_refs: Vec<lopdf::Object> = keep_page_ids
            .iter()
            .map(|&id| lopdf::Object::Reference(id))
            .collect();
        let at = at_index as usize;
        flat.splice(at..at, new_refs);
        let new_count = flat.len() as i64;
        if let Ok(d) = self.inner.get_dictionary_mut(pages_id) {
            d.set("Kids", lopdf::Object::Array(flat));
            d.set("Count", lopdf::Object::Integer(new_count));
        }
        self.inner.prune_objects();
        tracing::info!(
            "pdf_mutate::insert_pdf n={} at={} total={} in {:?}",
            keep_page_ids.len(),
            at_index,
            new_count,
            t.elapsed()
        );
        Ok(new_count as u32)
    }

    /// Stamp a text watermark on every page.
    /// Append an invisible OCR text layer to a single 0-indexed page.
    /// Word boxes are pixel coordinates on the rendered page bitmap.
    pub fn add_ocr_text_layer(
        &mut self,
        page_index: u32,
        words: &[OcrWord],
        img_w: u32,
        img_h: u32,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        ocr::add_ocr_text_layer(&mut self.inner, page_index, words, img_w, img_h)?;
        tracing::info!(
            "pdf_mutate::add_ocr_text_layer page {} ({} words) in {:?}",
            page_index,
            words.len(),
            t.elapsed()
        );
        Ok(())
    }

    pub fn add_text_watermark(&mut self, text: &str, opts: &WatermarkOptions) -> MutateResult<()> {
        let t = std::time::Instant::now();
        overlay::add_text_watermark(&mut self.inner, text, opts)?;
        tracing::info!("pdf_mutate::add_text_watermark in {:?}", t.elapsed());
        Ok(())
    }

    /// Add headers / footers / page numbers on every page.
    /// `{page}` and `{total}` tokens in slot strings are replaced per page.
    pub fn add_header_footer(&mut self, opts: &HeaderFooterOptions) -> MutateResult<()> {
        let t = std::time::Instant::now();
        overlay::add_header_footer(&mut self.inner, opts)?;
        tracing::info!("pdf_mutate::add_header_footer in {:?}", t.elapsed());
        Ok(())
    }

    /// Add a text-markup annotation (highlight / underline / strikeout) to a
    /// single 0-indexed page. Rects are in our top-origin RectPt convention.
    pub fn add_markup_annotation(
        &mut self,
        page_index: u32,
        annot: &MarkupAnnotation,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        annotations::add_markup_annotation(&mut self.inner, page_index, annot)?;
        tracing::info!(
            "pdf_mutate::add_markup_annotation kind={:?} rects={} page={} in {:?}",
            annot.kind,
            annot.rects.len(),
            page_index,
            t.elapsed()
        );
        Ok(())
    }

    /// Add a shape annotation (rectangle / circle / line / arrow) to a single
    /// 0-indexed page. Geometry is in our top-origin RectPt convention.
    pub fn add_shape_annotation(
        &mut self,
        page_index: u32,
        annot: &ShapeAnnotation,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        annotations::add_shape_annotation(&mut self.inner, page_index, annot)?;
        tracing::info!(
            "pdf_mutate::add_shape_annotation kind={:?} page={} in {:?}",
            annot.kind,
            page_index,
            t.elapsed()
        );
        Ok(())
    }

    /// Add a /FreeText annotation (free-floating text on a page).
    pub fn add_freetext_annotation(
        &mut self,
        page_index: u32,
        annot: &FreeTextAnnotation,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        annotations::add_freetext_annotation(&mut self.inner, page_index, annot)?;
        tracing::info!(
            "pdf_mutate::add_freetext_annotation page={} in {:?}",
            page_index,
            t.elapsed()
        );
        Ok(())
    }

    /// Add an /Ink annotation (free-hand strokes).
    pub fn add_ink_annotation(
        &mut self,
        page_index: u32,
        annot: &InkAnnotation,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        annotations::add_ink_annotation(&mut self.inner, page_index, annot)?;
        tracing::info!(
            "pdf_mutate::add_ink_annotation strokes={} page={} in {:?}",
            annot.strokes.len(),
            page_index,
            t.elapsed()
        );
        Ok(())
    }

    /// Add a /Stamp annotation built from an image.
    pub fn add_stamp_annotation(
        &mut self,
        page_index: u32,
        annot: &StampAnnotation,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        annotations::add_stamp_annotation(&mut self.inner, page_index, annot)?;
        tracing::info!(
            "pdf_mutate::add_stamp_annotation page={} in {:?}",
            page_index,
            t.elapsed()
        );
        Ok(())
    }

    /// Set an AcroForm field's /V value by fully-qualified field name.
    /// For Btn (checkbox/radio) fields, also mirrors /AS on every widget kid.
    pub fn set_form_field(&mut self, name: &str, value: &str) -> MutateResult<()> {
        let t = std::time::Instant::now();
        form::set_field_value(&mut self.inner, name, value)?;
        tracing::info!(
            "pdf_mutate::set_form_field name={:?} value_len={} in {:?}",
            name,
            value.len(),
            t.elapsed()
        );
        Ok(())
    }

    /// How every text and choice field sets its text, keyed by page index
    /// and position in the page's /Annots (see `form_ap`).
    pub fn form_text_styles(&self) -> std::collections::HashMap<(u32, u32), pdf_types::FieldTextStyle> {
        form_ap::text_styles(&self.inner)
    }

    /// Reset every AcroForm field to its /DV (default value) or remove /V.
    pub fn reset_form(&mut self) -> MutateResult<()> {
        let t = std::time::Instant::now();
        form::reset_form(&mut self.inner)?;
        tracing::info!("pdf_mutate::reset_form in {:?}", t.elapsed());
        Ok(())
    }

    /// List every annotation present in the document, flat (one entry per
    /// annotation, page-ordered). Rects are returned in our top-origin
    /// convention.
    pub fn list_annotations(&self) -> Vec<pdf_types::AnnotationInfo> {
        read::list_annotations(&self.inner)
    }

    /// Every 3D annotation (incl. RichMedia with a 3D asset) with its views.
    pub fn list_3d(&self) -> Vec<pdf_types::ThreeDAnnot> {
        threed::list_3d(&self.inner)
    }

    /// Decoded model data (format, bytes) of a 3D annotation.
    pub fn model_3d(&self, page_index: u32, id: &str) -> MutateResult<(String, Vec<u8>)> {
        threed::model_3d(&self.inner, page_index, id)
    }

    /// Adds a 3D annotation with a U3D model, preset views and a poster.
    pub fn add_3d_annotation(&mut self, page_index: u32, annot: &ThreeDAnnotation) -> MutateResult<()> {
        threed::add_3d_annotation(&mut self.inner, page_index, annot).map(|_| ())
    }

    /// Remove the annotation with the given id (as reported by
    /// [`list_annotations`](Self::list_annotations)) from a 0-indexed page.
    /// Returns whether a matching annotation was found and removed.
    pub fn delete_annotation(&mut self, page_index: u32, id: &str) -> MutateResult<bool> {
        annotations::delete_annotation(&mut self.inner, page_index, id)
    }

    /// Redaction: replace a page's whole content with a pre-rendered JPEG
    /// (regions already blacked out in pixel data). Removes the original
    /// content streams and annotations from the file.
    pub fn replace_page_with_jpeg(
        &mut self,
        page_index: u32,
        jpeg: Vec<u8>,
        px_w: u32,
        px_h: u32,
        page_w_pt: f32,
        page_h_pt: f32,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        redact::replace_page_with_jpeg(
            &mut self.inner, page_index, jpeg, px_w, px_h, page_w_pt, page_h_pt,
        )?;
        tracing::info!("pdf_mutate::replace_page_with_jpeg page={} in {:?}", page_index, t.elapsed());
        Ok(())
    }

    /// Strip /Info, XMP /Metadata and /PieceInfo. Returns removed count.
    pub fn sanitize_metadata(&mut self) -> MutateResult<u32> {
        redact::sanitize_metadata(&mut self.inner)
    }

    /// List every optional-content group (layer) with its default-config
    /// visibility. Hidden layers carry content that is in the file but not
    /// displayed.
    pub fn list_layers(&self) -> Vec<pdf_types::LayerInfo> {
        inspect::list_layers(&self.inner)
    }

    /// Serialize all annotations to an XFDF (XML) document, suitable for
    /// import into Adobe Acrobat / Foxit. Returns the XML bytes.
    pub fn export_xfdf(&self, source_filename: Option<&str>) -> MutateResult<Vec<u8>> {
        xfdf::export(&self.inner, source_filename)
    }

    /// Parse an XFDF document and append its annotations to this PDF. Returns
    /// the count of annotations imported.
    pub fn import_xfdf(&mut self, xml: &[u8]) -> MutateResult<usize> {
        xfdf::import(&mut self.inner, xml)
    }

    /// Add a /Text (sticky note) annotation anchored at a single point.
    pub fn add_text_note_annotation(
        &mut self,
        page_index: u32,
        annot: &TextNoteAnnotation,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        annotations::add_text_note_annotation(&mut self.inner, page_index, annot)?;
        tracing::info!(
            "pdf_mutate::add_text_note_annotation page={} in {:?}",
            page_index,
            t.elapsed()
        );
        Ok(())
    }

    /// Stamp an image watermark on every page.
    pub fn add_image_watermark(
        &mut self,
        image_bytes: &[u8],
        opts: &ImageWatermarkOptions,
    ) -> MutateResult<()> {
        let t = std::time::Instant::now();
        overlay::add_image_watermark(&mut self.inner, image_bytes, opts)?;
        tracing::info!("pdf_mutate::add_image_watermark in {:?}", t.elapsed());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary};

    /// Two pages under an intermediate /Pages node that carries the
    /// inheritable MediaBox, Resources and Rotate; the pages have none.
    fn nested_tree_doc() -> Document {
        let mut d = Document::with_version("1.5");
        let root_id = d.new_object_id();
        let mid_id = d.new_object_id();
        let font_id = d.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let mut kids = Vec::new();
        for _ in 0..2 {
            let pid = d.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(mid_id),
            });
            kids.push(Object::Reference(pid));
        }
        d.objects.insert(
            mid_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Parent" => Object::Reference(root_id),
                "Kids" => Object::Array(kids),
                "Count" => 2,
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 300.into(), 400.into()]),
                "Rotate" => 90,
                "Resources" => Object::Dictionary(dictionary! {
                    "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font_id) }),
                }),
            }),
        );
        d.objects.insert(
            root_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(mid_id)]),
                "Count" => 2,
            }),
        );
        let cat_id = d.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(root_id),
        });
        d.trailer.set("Root", Object::Reference(cat_id));
        d
    }

    fn to_bytes(mut d: Document) -> Vec<u8> {
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        out
    }

    fn page_font_f1(doc: &Document, page_id: ObjectId) -> bool {
        let res = match inherited_attr(doc, page_id, b"Resources") {
            Some(Object::Dictionary(r)) => r,
            Some(Object::Reference(id)) => doc.get_dictionary(id).unwrap().clone(),
            _ => return false,
        };
        res.get(b"Font")
            .and_then(Object::as_dict)
            .map(|f| f.has(b"F1"))
            .unwrap_or(false)
    }

    #[test]
    fn reorder_keeps_attributes_inherited_from_pruned_pages_node() {
        let mut m = MutableDoc::from_bytes(&to_bytes(nested_tree_doc())).unwrap();
        m.reorder_pages(&[1, 0]).unwrap();
        let out = Document::load_mem(&m.to_bytes().unwrap()).unwrap();
        for pid in out.page_iter() {
            assert_eq!(resolve_media_box(&out, pid), Some([0.0, 0.0, 300.0, 400.0]));
            assert_eq!(page_rotation(&out, pid), 90);
            assert!(page_font_f1(&out, pid), "page lost its inherited /Resources");
        }
    }

    #[test]
    fn insert_pdf_keeps_attributes_inherited_in_source() {
        let src = to_bytes(nested_tree_doc());
        let mut m = MutableDoc::from_bytes(&src).unwrap();
        m.insert_pdf(&src, Some(&[0]), 2).unwrap();
        let out = Document::load_mem(&m.to_bytes().unwrap()).unwrap();
        let pages: Vec<_> = out.page_iter().collect();
        assert_eq!(pages.len(), 3);
        let inserted = pages[2];
        assert_eq!(resolve_media_box(&out, inserted), Some([0.0, 0.0, 300.0, 400.0]));
        assert_eq!(page_rotation(&out, inserted), 90);
        assert!(page_font_f1(&out, inserted));
    }

    #[test]
    fn rotate_is_relative_to_inherited_rotation() {
        let mut m = MutableDoc::from_bytes(&to_bytes(nested_tree_doc())).unwrap();
        m.rotate_pages(&[0], 90).unwrap();
        let pid = m.inner.page_iter().next().unwrap();
        // Inherited 90 + 90, not 0 + 90 (which would be a visual no-op).
        assert_eq!(page_rotation(&m.inner, pid), 180);
    }

    #[test]
    fn region_rect_is_taken_in_displayed_rotated_space() {
        let m = MutableDoc::from_bytes(&to_bytes(nested_tree_doc())).unwrap();
        // 300x400 page with inherited /Rotate 90 displays as 400x300; the
        // displayed top-left 100x50 corner is PDF-space [0 0 50 100].
        let rect = RectPt { x0: 0.0, y0: 0.0, x1: 100.0, y1: 50.0 };
        let out = Document::load_mem(&m.extract_region_to_bytes(0, rect).unwrap()).unwrap();
        let pid = out.page_iter().next().unwrap();
        assert_eq!(resolve_media_box(&out, pid), Some([0.0, 0.0, 50.0, 100.0]));
        assert_eq!(page_rotation(&out, pid), 90);
    }

    #[test]
    fn deleted_and_unextracted_pages_leave_no_content_behind() {
        let mut d = nested_tree_doc();
        let pages: Vec<ObjectId> = d.page_iter().collect();
        for (i, pid) in pages.iter().enumerate() {
            let content = format!("BT (PAGE-{i}-SECRET) Tj ET").into_bytes();
            let sid = d.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), content)));
            d.get_dictionary_mut(*pid).unwrap().set("Contents", Object::Reference(sid));
        }
        let bytes = to_bytes(d);
        let has = |hay: &[u8], n: &[u8]| hay.windows(n.len()).any(|w| w == n);

        let m = MutableDoc::from_bytes(&bytes).unwrap();
        let extracted = m.extract_to_bytes(&[0]).unwrap();
        assert!(has(&extracted, b"PAGE-0-SECRET"));
        assert!(!has(&extracted, b"PAGE-1-SECRET"), "unextracted page content leaked");
        let region = m
            .extract_region_to_bytes(0, RectPt { x0: 0.0, y0: 0.0, x1: 100.0, y1: 100.0 })
            .unwrap();
        assert!(!has(&region, b"PAGE-1-SECRET"), "region export leaked other pages");

        let mut m = MutableDoc::from_bytes(&bytes).unwrap();
        m.delete_page(1).unwrap();
        let out = m.to_bytes().unwrap();
        assert!(has(&out, b"PAGE-0-SECRET"));
        assert!(!has(&out, b"PAGE-1-SECRET"), "deleted page content still in file");
    }

    #[test]
    fn parent_cycle_and_inverted_box_are_handled() {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let page_id = d.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
        });
        let mut pages = Dictionary::new();
        pages.set("Type", "Pages");
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set("Count", 1);
        // Malformed: the root names itself as parent.
        pages.set("Parent", Object::Reference(pages_id));
        d.objects.insert(pages_id, Object::Dictionary(pages));
        // Terminates instead of spinning on the /Parent loop.
        assert_eq!(page_rotation(&d, page_id), 0);
        assert_eq!(resolve_media_box(&d, page_id), None);

        // Corners given in the "wrong" order are normalised.
        d.get_dictionary_mut(page_id).unwrap().set(
            "MediaBox",
            Object::Array(vec![612.into(), 792.into(), 0.into(), 0.into()]),
        );
        assert_eq!(resolve_media_box(&d, page_id), Some([0.0, 0.0, 612.0, 792.0]));
    }
}
