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
