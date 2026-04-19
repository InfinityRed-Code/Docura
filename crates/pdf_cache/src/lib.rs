//! Thumbnail and page-render cache. LRU with zoom/page keyed entries.

use std::collections::HashMap;
use std::num::NonZeroUsize;

use pdf_types::{DocumentId, PageIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub document: DocumentId,
    pub page: PageIndex,
    pub zoom_milli: u32,
}

#[derive(Debug, Clone)]
pub struct CachedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct RenderCache {
    capacity: NonZeroUsize,
    entries: HashMap<CacheKey, CachedImage>,
}

impl RenderCache {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<&CachedImage> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: CacheKey, image: CachedImage) {
        if self.entries.len() >= self.capacity.get() {
            if let Some(victim) = self.entries.keys().next().copied() {
                self.entries.remove(&victim);
            }
        }
        self.entries.insert(key, image);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
