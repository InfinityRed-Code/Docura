//! Thumbnail and page-render cache. Byte-budgeted LRU keyed by zoom/page.
//!
//! Entries hold pre-encoded JPEG data URLs of wildly different sizes (a dense
//! A4 page at 200% dwarfs a sparse one), so a fixed *entry count* is a poor
//! proxy for memory. The cache therefore evicts by an approximate byte budget
//! the user can tune in Settings.

use lru::LruCache;
use pdf_types::{DocumentId, PageIndex};

/// Smallest budget we accept, so the cache always holds at least the current
/// viewport's worth of pages even if the user sets something tiny.
const MIN_BUDGET_MB: u64 = 16;
const BYTES_PER_MB: usize = 1024 * 1024;

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

/// Approximate resident bytes of one entry: the encoded payload plus a flat
/// allowance for the key/struct/LRU node overhead.
fn entry_bytes(img: &CachedImage) -> usize {
    img.data_url.len()
        + std::mem::size_of::<CacheKey>()
        + std::mem::size_of::<CachedImage>()
}

pub struct RenderCache {
    entries: LruCache<CacheKey, CachedImage>,
    cur_bytes: usize,
    max_bytes: usize,
}

impl RenderCache {
    /// Create a cache holding at most ~`mb` megabytes of encoded images.
    pub fn with_budget_mb(mb: u64) -> Self {
        Self {
            entries: LruCache::unbounded(),
            cur_bytes: 0,
            max_bytes: Self::clamp_budget(mb),
        }
    }

    fn clamp_budget(mb: u64) -> usize {
        mb.max(MIN_BUDGET_MB) as usize * BYTES_PER_MB
    }

    /// Current budget in megabytes (clamped, as actually applied).
    pub fn budget_mb(&self) -> u64 {
        (self.max_bytes / BYTES_PER_MB) as u64
    }

    /// Resize the budget and evict down to it immediately.
    pub fn set_budget_mb(&mut self, mb: u64) {
        self.max_bytes = Self::clamp_budget(mb);
        self.evict();
    }

    pub fn get(&mut self, key: &CacheKey) -> Option<&CachedImage> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: CacheKey, image: CachedImage) {
        let added = entry_bytes(&image);
        if let Some(old) = self.entries.put(key, image) {
            // Replacing an existing key: drop its bytes from the running total.
            self.cur_bytes = self.cur_bytes.saturating_sub(entry_bytes(&old));
        }
        self.cur_bytes += added;
        self.evict();
    }

    /// Pop least-recently-used entries until under budget. Always keeps at
    /// least one entry so a single oversized page can't empty the cache.
    fn evict(&mut self) {
        while self.cur_bytes > self.max_bytes && self.entries.len() > 1 {
            match self.entries.pop_lru() {
                Some((_, old)) => {
                    self.cur_bytes = self.cur_bytes.saturating_sub(entry_bytes(&old));
                }
                None => break,
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.cur_bytes = 0;
    }

    pub fn remove_document(&mut self, doc: DocumentId) {
        let keys: Vec<CacheKey> = self
            .entries
            .iter()
            .filter_map(|(k, _)| if k.document == doc { Some(*k) } else { None })
            .collect();
        for k in keys {
            if let Some(old) = self.entries.pop(&k) {
                self.cur_bytes = self.cur_bytes.saturating_sub(entry_bytes(&old));
            }
        }
    }
}
