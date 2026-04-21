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
}
