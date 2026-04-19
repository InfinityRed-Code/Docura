//! Concrete `PdfEngine` implementation backed by `pdfium-render`.
//!
//! PDFium is not thread-safe. We serialise every call through a single
//! process-wide mutex and expose the engine as `Send + Sync` so the Tauri
//! app can share it across async handlers; the actual native work still
//! runs one call at a time.
//!
//! Binding strategy: look for a bundled `pdfium.dll` / `libpdfium.so` in
//! `./vendor/pdfium`, `./`, or `../vendor/pdfium`; fall back to the system
//! library.
//!
//! Coordinate convention on the outward API: all `RectPt` values are in
//! PDF points with a **top-left origin** (y grows downward). PDFium's
//! native coord system is bottom-left; conversion happens here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;
use pdf_core::{CoreError, CoreResult, PdfEngine, RenderRequest, RenderedPage};
use pdf_types::{
    DocumentId, OutlineNode, PageIndex, PageSize, RectPt, SearchHit, SearchQuery, TextRun,
};
use pdfium_render::prelude::*;

struct SharedPdfium(Mutex<Pdfium>);

unsafe impl Send for SharedPdfium {}
unsafe impl Sync for SharedPdfium {}

static PDFIUM: OnceLock<SharedPdfium> = OnceLock::new();

fn shared_pdfium() -> CoreResult<&'static SharedPdfium> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let bindings = bind_to_bundled().or_else(|_| {
        Pdfium::bind_to_system_library()
            .map_err(|e| CoreError::Engine(format!("pdfium system bind: {e}")))
    })?;
    let wrapped = SharedPdfium(Mutex::new(Pdfium::new(bindings)));
    let _ = PDFIUM.set(wrapped);
    Ok(PDFIUM.get().expect("pdfium initialised"))
}

fn bind_to_bundled() -> CoreResult<Box<dyn PdfiumLibraryBindings>> {
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("./vendor/pdfium"),
        PathBuf::from("."),
        PathBuf::from("../vendor/pdfium"),
        PathBuf::from("../../vendor/pdfium"),
        PathBuf::from("../../../vendor/pdfium"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.insert(0, exe_dir.to_path_buf());
            candidates.push(exe_dir.join("../../../vendor/pdfium"));
            candidates.push(exe_dir.join("../../../../vendor/pdfium"));
        }
    }
    for dir in &candidates {
        let path = Pdfium::pdfium_platform_library_name_at_path(dir);
        if let Ok(b) = Pdfium::bind_to_library(&path) {
            tracing::info!("bound to PDFium at {}", path.display());
            return Ok(b);
        }
    }
    Err(CoreError::Engine(
        "PDFium not found; place pdfium.dll in vendor/pdfium or next to the executable".into(),
    ))
}

pub struct PdfiumEngine {
    docs: Mutex<HashMap<DocumentId, Vec<u8>>>,
}

impl PdfiumEngine {
    pub fn new() -> CoreResult<Self> {
        shared_pdfium()?;
        Ok(Self {
            docs: Mutex::new(HashMap::new()),
        })
    }

    fn bytes_for(&self, id: DocumentId) -> CoreResult<Vec<u8>> {
        self.docs
            .lock()
            .get(&id)
            .cloned()
            .ok_or(CoreError::DocumentNotLoaded)
    }
}

fn rect_to_top_origin(rect: PdfRect, page_height_pt: f32) -> RectPt {
    let left = rect.left().value;
    let right = rect.right().value;
    let top_bl = rect.top().value;
    let bottom_bl = rect.bottom().value;
    RectPt {
        x0: left,
        y0: page_height_pt - top_bl,
        x1: right,
        y1: page_height_pt - bottom_bl,
    }
}

impl PdfEngine for PdfiumEngine {
    fn open_bytes(&self, bytes: Vec<u8>) -> CoreResult<DocumentId> {
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        drop(pdfium);
        let id = DocumentId::new();
        self.docs.lock().insert(id, bytes);
        Ok(id)
    }

    fn close(&self, doc: DocumentId) -> CoreResult<()> {
        self.docs.lock().remove(&doc);
        Ok(())
    }

    fn page_count(&self, doc: DocumentId) -> CoreResult<u32> {
        let bytes = self.bytes_for(doc)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        Ok(document.pages().len() as u32)
    }

    fn page_size(&self, doc: DocumentId, page: PageIndex) -> CoreResult<PageSize> {
        let bytes = self.bytes_for(doc)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        Ok(PageSize {
            width_pt: p.width().value,
            height_pt: p.height().value,
        })
    }

    fn render(&self, req: RenderRequest) -> CoreResult<RenderedPage> {
        let bytes = self.bytes_for(req.document)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        let page = document
            .pages()
            .get(req.page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let config = PdfRenderConfig::new().scale_page_by_factor(req.zoom.0);
        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| CoreError::Engine(format!("render: {e}")))?;
        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        let rgba = bitmap.as_rgba_bytes();
        Ok(RenderedPage {
            width_px: width,
            height_px: height,
            rgba,
        })
    }

    fn extract_text(&self, doc: DocumentId, page: PageIndex) -> CoreResult<String> {
        let bytes = self.bytes_for(doc)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        let page = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let text = page
            .text()
            .map_err(|e| CoreError::Engine(format!("text: {e}")))?;
        Ok(text.all())
    }

    fn page_text_layout(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<TextRun>> {
        let bytes = self.bytes_for(doc)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let page_h = p.height().value;
        let text = p
            .text()
            .map_err(|e| CoreError::Engine(format!("text: {e}")))?;
        let mut runs: Vec<TextRun> = Vec::new();
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            if c == '\0' {
                continue;
            }
            let Ok(rect) = ch.loose_bounds() else { continue };
            let rect = rect_to_top_origin(rect, page_h);
            if let Some(last) = runs.last_mut() {
                // Merge if on same line (similar y) and contiguous x.
                let same_line = (last.rect_pt.y0 - rect.y0).abs() < 1.5
                    && (last.rect_pt.y1 - rect.y1).abs() < 1.5;
                let adjacent = rect.x0 >= last.rect_pt.x0 - 1.0
                    && rect.x0 - last.rect_pt.x1 < (rect.y1 - rect.y0) * 0.6;
                if same_line && adjacent {
                    last.text.push(c);
                    last.rect_pt.x1 = rect.x1.max(last.rect_pt.x1);
                    last.rect_pt.y0 = rect.y0.min(last.rect_pt.y0);
                    last.rect_pt.y1 = rect.y1.max(last.rect_pt.y1);
                    continue;
                }
            }
            let mut s = String::new();
            s.push(c);
            runs.push(TextRun {
                text: s,
                rect_pt: rect,
            });
        }
        Ok(runs)
    }

    fn outline(&self, doc: DocumentId) -> CoreResult<Vec<OutlineNode>> {
        let bytes = self.bytes_for(doc)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;
        let bookmarks = document.bookmarks();
        let mut roots = Vec::new();
        for bm in bookmarks.iter() {
            roots.push(collect_bookmark(&bm, &document));
        }
        Ok(roots)
    }

    fn search(&self, doc: DocumentId, query: &SearchQuery) -> CoreResult<Vec<SearchHit>> {
        if query.term.is_empty() {
            return Ok(Vec::new());
        }
        let bytes = self.bytes_for(doc)?;
        let shared = shared_pdfium()?;
        let pdfium = shared.0.lock();
        let document = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| CoreError::Engine(format!("open: {e}")))?;

        let needle: String = if query.case_sensitive {
            query.term.clone()
        } else {
            query.term.to_lowercase()
        };

        let mut hits: Vec<SearchHit> = Vec::new();
        for (page_idx, p) in document.pages().iter().enumerate() {
            let page_h = p.height().value;
            let Ok(text) = p.text() else { continue };
            // Build haystack char-by-char with aligned bounds.
            let mut chars: Vec<(char, RectPt)> = Vec::new();
            for ch in text.chars().iter() {
                let Some(c) = ch.unicode_char() else { continue };
                if c == '\0' {
                    continue;
                }
                let Ok(r) = ch.loose_bounds() else { continue };
                chars.push((c, rect_to_top_origin(r, page_h)));
            }
            let haystack: String = chars
                .iter()
                .map(|(c, _)| if query.case_sensitive { *c } else { c.to_ascii_lowercase() })
                .collect();
            let needle_len_chars = needle.chars().count();
            // Simple substring scan with char-index tracking.
            let mut char_positions: Vec<usize> = Vec::with_capacity(haystack.len());
            for (i, _) in haystack.char_indices() {
                char_positions.push(i);
            }
            char_positions.push(haystack.len());
            let mut from = 0usize;
            while let Some(byte_idx) = haystack[from..].find(&needle) {
                let absolute = from + byte_idx;
                let char_start = char_positions.partition_point(|&b| b <= absolute) - 1;
                let char_end = (char_start + needle_len_chars).min(chars.len());
                let rects = merge_rects(&chars[char_start..char_end]);
                let snippet = snippet_around(&chars, char_start, char_end);
                hits.push(SearchHit {
                    page: PageIndex(page_idx as u32),
                    rects_pt: rects,
                    snippet,
                });
                from = absolute + needle.len();
                if hits.len() > 2000 {
                    return Ok(hits);
                }
            }
        }
        Ok(hits)
    }
}

fn collect_bookmark(bm: &PdfBookmark, document: &PdfDocument) -> OutlineNode {
    let title = bm.title().unwrap_or_default();
    let page = bm
        .destination()
        .and_then(|d| d.page_index().ok())
        .map(|idx| PageIndex(idx as u32));
    let mut children = Vec::new();
    for child in bm.iter_direct_children() {
        children.push(collect_bookmark(&child, document));
    }
    let _ = document;
    OutlineNode {
        title,
        page,
        children,
    }
}

fn merge_rects(chars: &[(char, RectPt)]) -> Vec<RectPt> {
    let mut out: Vec<RectPt> = Vec::new();
    for (_, r) in chars {
        if let Some(last) = out.last_mut() {
            let same_line = (last.y0 - r.y0).abs() < 1.5 && (last.y1 - r.y1).abs() < 1.5;
            if same_line {
                last.x0 = last.x0.min(r.x0);
                last.x1 = last.x1.max(r.x1);
                last.y0 = last.y0.min(r.y0);
                last.y1 = last.y1.max(r.y1);
                continue;
            }
        }
        out.push(*r);
    }
    out
}

fn snippet_around(chars: &[(char, RectPt)], start: usize, end: usize) -> String {
    let lo = start.saturating_sub(20);
    let hi = (end + 20).min(chars.len());
    chars[lo..hi].iter().map(|(c, _)| *c).collect()
}
