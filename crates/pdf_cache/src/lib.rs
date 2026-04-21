//! Thumbnail and page-render cache. LRU with zoom/page keyed entries.

use std::num::NonZeroUsize;

use lru::LruCache;
use pdf_types::{DocumentId, PageIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub document: DocumentId,
    pub page: PageIndex,
    pub zoom_milli: u32,
    pub rotation_q: u8,
}

#[derive(Debug, Clone)]
pub struct CachedImage {
    pub width: u32,
    pub height: u32,
    /// Pre-encoded payload ready to ship to the UI (e.g. a JPEG data URL).
    /// Storing encoded bytes — not raw RGBA — means cache hits skip the
    /// ~100ms JPEG re-encode and keep ~10x less memory resident.
    pub data_url: String,
}

pub struct RenderCache {
    entries: LruCache<CacheKey, CachedImage>,
}

impl RenderCache {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            entries: LruCache::new(capacity),
        }
    }

    pub fn get(&mut self, key: &CacheKey) -> Option<&CachedImage> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: CacheKey, image: CachedImage) {
        self.entries.put(key, image);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn remove_document(&mut self, doc: DocumentId) {
        let keys: Vec<CacheKey> = self
            .entries
            .iter()
            .filter_map(|(k, _)| if k.document == doc { Some(*k) } else { None })
            .collect();
        for k in keys {
            self.entries.pop(&k);
        }
    }
}
