//! Structural PDF mutation via `lopdf`.
//!
//! Pdfium is used for rendering and text extraction. For page-level edits
//! (delete, insert, reorder, rotate persistent, merge, split, extract) the
//! byte stream has to be rewritten — lopdf owns that side of the pipeline.
//!
//! Usage pattern: load bytes into `MutableDoc`, apply mutations, serialise
//! back to bytes, reload pdfium so rendering sees the new state.

use std::io::Cursor;

use lopdf::Document;

mod annotations;
mod overlay;

pub use annotations::{
    FreeTextAnnotation, InkAnnotation, LineEnding, MarkupAnnotation, MarkupKind,
    ShapeAnnotation, ShapeKind, StampAnnotation,
};
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
}

pub struct MutableDoc {
    inner: Document,
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
    /// `degrees` must be a multiple of 90 (positive = clockwise). Missing or
    /// inherited /Rotate is treated as 0. 0-indexed page list.
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
            let current = self
                .inner
                .get_dictionary(page_id)
                .ok()
                .and_then(|d| d.get(b"Rotate").ok())
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as i32;
            let new_rot = ((current + degrees) % 360 + 360) % 360;
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
