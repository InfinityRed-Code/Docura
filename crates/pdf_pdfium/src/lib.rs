//! Concrete `PdfEngine` implementation backed by `pdfium-render`.
//!
//! PDFium is not thread-safe. The `Pdfium` binding is leaked to `'static`
//! and every call is serialised through an internal `Mutex`, so the engine
//! can be shared across async handlers while the native work still runs
//! one call at a time.
//!
//! Performance strategy: parse each PDF **once** on open, keep the
//! `PdfDocument` resident (lifetime extended to `'static` by leaking the
//! underlying byte buffer), and reuse it for every later render / text /
//! link / search / attachment call. Closing a doc reclaims the bytes.
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
    AttachmentInfo, CharBox, DocMetadata, DocPermissions, DocumentId, FormFieldInfo, FormFieldKind,
    LinkTarget, OutlineNode, PageIndex, PageLink, PageSize, RectPt, Rotation, SearchHit,
    SearchQuery, TextRun,
};
use pdfium_render::prelude::*;

#[cfg(windows)]
mod print_win;
mod fontmatch;
mod forms;
mod notes;
mod proxy;
mod textedit;

/// Control characters PDFium emits for glyphs without a Unicode mapping
/// (e.g. U+0002), plus the non-characters U+FFFE/U+FFFF. They are invalid
/// in XML exports and useless for display/selection. Tab/CR/LF stay —
/// they carry line structure in extracted text.
fn is_garbage_char(c: char) -> bool {
    (c < '\u{20}' && c != '\t' && c != '\r' && c != '\n') || c == '\u{FFFE}' || c == '\u{FFFF}'
}

struct SyncPdfium(Pdfium);

// SAFETY: All calls into PDFium are serialised through `PdfiumEngine::inner`.
// Nothing touches `Pdfium` without first holding that mutex, so exposing a
// `&Pdfium` across threads is safe in this crate's usage.
unsafe impl Sync for SyncPdfium {}
unsafe impl Send for SyncPdfium {}

static PDFIUM: OnceLock<SyncPdfium> = OnceLock::new();

fn shared_pdfium() -> CoreResult<&'static Pdfium> {
    if let Some(p) = PDFIUM.get() {
        return Ok(&p.0);
    }
    let bindings = bind_to_bundled().or_else(|_| {
        Pdfium::bind_to_system_library()
            .map_err(|e| CoreError::Engine(format!("pdfium system bind: {e}")))
    })?;
    let _ = PDFIUM.set(SyncPdfium(Pdfium::new(bindings)));
    Ok(&PDFIUM.get().expect("pdfium initialised").0)
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
            candidates.push(exe_dir.join("resources"));
            candidates.push(exe_dir.join("resources/vendor/pdfium"));
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

/// A `PdfDocument` together with the leaked byte buffer it borrows.
/// Dropping it tears down the handle before reclaiming the bytes.
struct LoadedDoc {
    doc: Option<PdfDocument<'static>>,
    bytes_raw: *mut [u8],
}

impl LoadedDoc {
    fn load(bytes: Vec<u8>, password: Option<&str>) -> Result<Self, PdfiumError> {
        let pdfium = shared_pdfium()
            .map_err(|_| PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::Unknown))?;
        let bytes_raw: *mut [u8] = Box::into_raw(bytes.into_boxed_slice());
        // SAFETY: `bytes_raw` stays alive until this LoadedDoc is dropped,
        // which first drops the document borrowing it.
        let bytes_static: &'static [u8] = unsafe { &*bytes_raw };
        match pdfium.load_pdf_from_byte_slice(bytes_static, password) {
            Ok(d) => Ok(Self { doc: Some(d), bytes_raw }),
            Err(e) => {
                // SAFETY: nothing borrows the buffer any more.
                unsafe { drop(Box::from_raw(bytes_raw)) };
                Err(e)
            }
        }
    }
}

impl Drop for LoadedDoc {
    fn drop(&mut self) {
        self.doc.take();
        // SAFETY: allocated by `Box::into_raw` in `load`; the document that
        // borrowed it was dropped just above.
        unsafe { drop(Box::from_raw(self.bytes_raw)) };
    }
}

/// Tiled render copies of a document with huge raster images (see
/// `proxy`): used for renders above `low_scale`, coarsest level first.
struct HiDoc {
    /// `(max_zoom, doc)`: the zoom up to which the level is screen-sharp.
    levels: Vec<(f32, LoadedDoc)>,
    low_scale: f32,
}

impl HiDoc {
    /// Coarsest level that is still sharp at `zoom` (else the finest).
    fn level_for(&self, zoom: f32) -> Option<(usize, &LoadedDoc)> {
        self.levels
            .iter()
            .enumerate()
            .find(|(_, (max, _))| zoom <= max * 1.01)
            .or_else(|| self.levels.iter().enumerate().next_back())
            .map(|(i, (_, d))| (i, d))
    }
}

/// Per-document state. Owns the leaked byte buffer and the resident
/// `PdfDocument`. Dropping this entry tears down the document handle and
/// reclaims the bytes.
///
/// For documents with huge raster images (scanned plans) `doc` is the
/// downsampled render proxy, `hi` the tiled resolution pyramid and `orig`
/// the untouched file bytes (what `document_bytes` hands out for saving,
/// signing and mutations).
struct DocEntry {
    doc: Option<PdfDocument<'static>>,
    bytes_raw: *mut [u8],
    orig: Option<Vec<u8>>,
    hi: Option<HiDoc>,
    page_count: u32,
    // Per-page sizes resolved lazily on first access. `None` = not yet
    // fetched. Opening a 500-page PDF no longer blocks on N sequential
    // page-size queries.
    sizes: Vec<Option<PageSize>>,
    // Outline built lazily on first request. `None` = not yet walked.
    outline: Option<Vec<OutlineNode>>,
    // Colours of the sticky notes PDFium reports none for; read from the
    // file when the first such note is drawn (see `notes`).
    note_colors: std::cell::OnceCell<notes::NoteColors>,
}

impl Drop for DocEntry {
    fn drop(&mut self) {
        // Tear down the PDFium handle before releasing its backing bytes.
        self.doc.take();
        // SAFETY: `bytes_raw` came from `Box::into_raw` on open and has not
        // been freed elsewhere. The `PdfDocument` that borrowed this slice
        // has just been dropped above.
        unsafe {
            drop(Box::from_raw(self.bytes_raw));
        }
    }
}

struct Inner {
    docs: HashMap<DocumentId, DocEntry>,
    note_icons: notes::IconCache,
}

// SAFETY: All access to `Inner` (and therefore to any `PdfDocument` it
// holds) is serialised through `PdfiumEngine::inner: Mutex<Inner>`. PDFium
// itself is not thread-safe, so only one thread ever touches these
// handles at a time. The raw `*mut [u8]` in `DocEntry` is not shared.
unsafe impl Send for Inner {}

pub struct PdfiumEngine {
    inner: Mutex<Inner>,
}

impl DocEntry {
    /// Load `bytes`; documents with huge raster images get render proxies
    /// (built before the engine lock is taken — it can take seconds).
    fn build(bytes: Vec<u8>, password: Option<&str>) -> Result<Self, PdfiumError> {
        let proxies = if password.is_none() { proxy::build(&bytes) } else { None };
        let (main, orig, hi) = match proxies {
            Some(p) => {
                let low = LoadedDoc::load(p.low, None);
                let levels: Result<Vec<(f32, LoadedDoc)>, PdfiumError> = p
                    .levels
                    .into_iter()
                    .map(|(z, b)| LoadedDoc::load(b, None).map(|d| (z, d)))
                    .collect();
                match (low, levels) {
                    (Ok(low), Ok(levels)) if !levels.is_empty() => (
                        low,
                        Some(bytes),
                        Some(HiDoc { levels, low_scale: p.low_scale }),
                    ),
                    (low, levels) => {
                        tracing::warn!(
                            "render proxies failed to load (low ok={}, levels ok={}); rendering the original",
                            low.is_ok(),
                            levels.is_ok()
                        );
                        (LoadedDoc::load(bytes, password)?, None, None)
                    }
                }
            }
            None => (LoadedDoc::load(bytes, password)?, None, None),
        };
        // Hand the document/buffer pair over to the entry, whose own Drop
        // frees them from now on.
        let mut main = std::mem::ManuallyDrop::new(main);
        let doc = main.doc.take();
        let page_count = doc.as_ref().map(|d| d.pages().len() as u32).unwrap_or(0);
        Ok(DocEntry {
            doc,
            bytes_raw: main.bytes_raw,
            orig,
            hi,
            page_count,
            sizes: vec![None; page_count as usize],
            outline: None,
            note_colors: std::cell::OnceCell::new(),
        })
    }

    /// The file as opened (not a render proxy).
    fn file_bytes(&self) -> &[u8] {
        match &self.orig {
            Some(orig) => orig,
            // SAFETY: `bytes_raw` is live until the entry is dropped.
            None => unsafe { &*self.bytes_raw },
        }
    }

    fn note_colors(&self) -> &notes::NoteColors {
        self.note_colors.get_or_init(|| notes::read_note_colors(self.file_bytes()))
    }

    /// Document to render at `zoom`: above the downsampled proxy's
    /// sharpness limit the coarsest pyramid level that is still sharp,
    /// else the resident document.
    fn render_doc(&self, zoom: f32) -> Option<&PdfDocument<'static>> {
        match self.hi_level(zoom) {
            Some((_, d)) => d.doc.as_ref(),
            None => self.doc.as_ref(),
        }
    }

    /// Pyramid level (index, doc) used at `zoom`, if any.
    fn hi_level(&self, zoom: f32) -> Option<(usize, &LoadedDoc)> {
        match &self.hi {
            Some(hi) if zoom > hi.low_scale * 1.01 => hi.level_for(zoom),
            _ => None,
        }
    }
}

impl PdfiumEngine {
    pub fn new() -> CoreResult<Self> {
        shared_pdfium()?;
        Ok(Self {
            inner: Mutex::new(Inner {
                docs: HashMap::new(),
                note_icons: notes::IconCache::default(),
            }),
        })
    }
}

/// How to map raw PDF user-space coordinates (what PDFium reports for text,
/// links and widgets) into the *displayed* top-origin space (what PDFium
/// renders and the UI works in). Two effects are folded in:
///  - the page bounding box (CropBox ∩ MediaBox) may not start at (0,0)
///    — print marks, scans;
///  - a persistent /Rotate turns the rendered bitmap while the reported
///    coordinates stay in the unrotated page space.
#[derive(Clone, Copy, Debug)]
struct PageView {
    left: f32,
    top: f32,
    /// Unrotated view-box size.
    w: f32,
    h: f32,
    /// Page /Rotate in degrees (0/90/180/270).
    rot: i32,
}

fn page_view(page: &PdfPage) -> PageView {
    let (left, top, w, h) = page
        .boundaries()
        .bounding()
        .map(|b| {
            (
                b.bounds.left().value,
                b.bounds.top().value,
                b.bounds.right().value - b.bounds.left().value,
                b.bounds.top().value - b.bounds.bottom().value,
            )
        })
        .unwrap_or((0.0, page.height().value, page.width().value, page.height().value));
    let rot = match page.rotation() {
        Ok(PdfPageRenderRotation::Degrees90) => 90,
        Ok(PdfPageRenderRotation::Degrees180) => 180,
        Ok(PdfPageRenderRotation::Degrees270) => 270,
        _ => 0,
    };
    PageView { left, top, w, h, rot }
}

fn rect_to_top_origin(rect: PdfRect, pv: &PageView) -> RectPt {
    // Unrotated top-origin coordinates relative to the view box…
    let x0 = rect.left().value - pv.left;
    let x1 = rect.right().value - pv.left;
    let y0 = pv.top - rect.top().value;
    let y1 = pv.top - rect.bottom().value;
    // …then rotate into display orientation.
    match pv.rot {
        90 => RectPt { x0: pv.h - y1, y0: x0, x1: pv.h - y0, y1: x1 },
        180 => RectPt { x0: pv.w - x1, y0: pv.h - y1, x1: pv.w - x0, y1: pv.h - y0 },
        270 => RectPt { x0: y0, y0: pv.w - x1, x1: y1, y1: pv.w - x0 },
        _ => RectPt { x0, y0, x1, y1 },
    }
}

fn password_error<E: std::fmt::Display>(e: E, had_password: bool) -> CoreError {
    let msg = e.to_string().to_ascii_lowercase();
    if msg.contains("password") {
        if had_password {
            CoreError::PasswordIncorrect
        } else {
            CoreError::PasswordRequired
        }
    } else {
        CoreError::Engine(format!("open: {e}"))
    }
}

impl PdfEngine for PdfiumEngine {
    fn open_bytes_with_password(
        &self,
        bytes: Vec<u8>,
        password: Option<String>,
    ) -> CoreResult<DocumentId> {
        shared_pdfium()?;
        let bytes_len = bytes.len();
        let t_load = std::time::Instant::now();
        let entry = DocEntry::build(bytes, password.as_deref())
            .map_err(|e| password_error(e, password.is_some()))?;
        tracing::info!(
            "pdfium::open bytes={:.1}MB load={:?} pages={} proxied={}",
            bytes_len as f64 / 1024.0 / 1024.0,
            t_load.elapsed(),
            entry.page_count,
            entry.hi.is_some()
        );
        let id = DocumentId::new();
        self.inner.lock().docs.insert(id, entry);
        Ok(id)
    }

    fn close(&self, doc: DocumentId) -> CoreResult<()> {
        self.inner.lock().docs.remove(&doc);
        Ok(())
    }

    fn page_count(&self, doc: DocumentId) -> CoreResult<u32> {
        let inner = self.inner.lock();
        let e = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        Ok(e.page_count)
    }

    fn page_size(&self, doc: DocumentId, page: PageIndex) -> CoreResult<PageSize> {
        let mut inner = self.inner.lock();
        let e = inner.docs.get_mut(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let idx = page.0 as usize;
        if idx >= e.sizes.len() {
            return Err(CoreError::PageOutOfRange(page.0, e.page_count));
        }
        if let Some(sz) = e.sizes[idx] {
            return Ok(sz);
        }
        let document = e.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|err| CoreError::Engine(format!("page: {err}")))?;
        let sz = PageSize {
            width_pt: p.width().value,
            height_pt: p.height().value,
        };
        e.sizes[idx] = Some(sz);
        Ok(sz)
    }

    fn render(&self, req: RenderRequest) -> CoreResult<RenderedPage> {
        proxy::full_speed();
        let t_lock = std::time::Instant::now();
        let mut inner = self.inner.lock();
        let lock_t = t_lock.elapsed();
        let Inner { docs, note_icons } = &mut *inner;
        let entry = docs.get(&req.document).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.render_doc(req.zoom.0).ok_or(CoreError::DocumentNotLoaded)?;
        let t_page = std::time::Instant::now();
        let page = document
            .pages()
            .get(req.page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let page_t = t_page.elapsed();
        let rot = match req.rotation {
            Rotation::R0 => PdfPageRenderRotation::None,
            Rotation::R90 => PdfPageRenderRotation::Degrees90,
            Rotation::R180 => PdfPageRenderRotation::Degrees180,
            Rotation::R270 => PdfPageRenderRotation::Degrees270,
        };
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(req.zoom.0)
            .rotate(rot, true)
            // The note icons are drawn into the same bitmap, in this order.
            .set_reverse_byte_order(true);
        let t_render = std::time::Instant::now();
        // Sticky-note icons are painted here, upright (see `notes`).
        let bindings = document.bindings();
        let page_handle = bindings.get_handle_from_page(&page);
        let sticky =
            notes::HiddenNotes::hide(bindings, page_handle, req.page.0, || entry.note_colors());
        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| CoreError::Engine(format!("render: {e}")))?;
        sticky.draw(
            note_icons,
            bindings.get_handle_from_bitmap(&bitmap),
            page_handle,
            (0, 0),
            (bitmap.width() as i32, bitmap.height() as i32),
            req.rotation as i32,
            req.zoom.0,
            true,
        );
        drop(sticky);
        let render_t = t_render.elapsed();
        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        let t_bytes = std::time::Instant::now();
        let rgba = bitmap.as_rgba_bytes();
        tracing::info!(
            "pdfium::render page={} zoom={:.2} {}x{} lock_wait={:?} page_open={:?} render={:?} rgba_copy={:?}",
            req.page.0, req.zoom.0, width, height,
            lock_t, page_t, render_t, t_bytes.elapsed()
        );
        Ok(RenderedPage {
            width_px: width,
            height_px: height,
            rgba,
        })
    }

    fn extract_text(&self, doc: DocumentId, page: PageIndex) -> CoreResult<String> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let page = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let text = page
            .text()
            .map_err(|e| CoreError::Engine(format!("text: {e}")))?;
        // PDFium maps glyphs without a Unicode entry to C0 control chars
        // (e.g. U+0002) — strip them, keeping line structure.
        Ok(text.all().chars().filter(|&c| !is_garbage_char(c)).collect())
    }

    fn page_text_layout(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<TextRun>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let pv = page_view(&p);
        let text = p
            .text()
            .map_err(|e| CoreError::Engine(format!("text: {e}")))?;
        let mut runs: Vec<TextRun> = Vec::new();
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            if c == '\0' || is_garbage_char(c) {
                continue;
            }
            let Ok(rect) = ch.loose_bounds() else { continue };
            let rect = rect_to_top_origin(rect, &pv);
            if let Some(last) = runs.last_mut() {
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

    fn page_hidden_text(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<String>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let text = p
            .text()
            .map_err(|e| CoreError::Engine(format!("text: {e}")))?;
        let mut snippets: Vec<String> = Vec::new();
        let mut current = String::new();
        for ch in text.chars().iter() {
            let invisible = matches!(
                ch.render_mode(),
                Ok(PdfPageTextRenderMode::Invisible)
                    | Ok(PdfPageTextRenderMode::InvisibleClipping)
            );
            let c = ch
                .unicode_char()
                .filter(|&c| c != '\0' && !is_garbage_char(c));
            if invisible {
                if let Some(c) = c {
                    current.push(c);
                }
            } else if !current.trim().is_empty() {
                snippets.push(std::mem::take(&mut current).trim().to_string());
            } else {
                current.clear();
            }
        }
        if !current.trim().is_empty() {
            snippets.push(current.trim().to_string());
        }
        Ok(snippets)
    }

    fn page_char_boxes(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<CharBox>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let pv = page_view(&p);
        let text = p
            .text()
            .map_err(|e| CoreError::Engine(format!("text: {e}")))?;
        let mut out = Vec::new();
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            if c == '\0' || is_garbage_char(c) {
                continue;
            }
            let Ok(rect) = ch.loose_bounds() else { continue };
            out.push(CharBox {
                ch: c,
                rect_pt: rect_to_top_origin(rect, &pv),
            });
        }
        Ok(out)
    }

    fn page_text_blocks(
        &self,
        doc: DocumentId,
        page: PageIndex,
    ) -> CoreResult<Vec<pdf_types::TextBlock>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        // For proxied documents this is the downsampled render copy; its
        // page objects are the original's, only image data differs.
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        textedit::list(&p).map_err(|e| CoreError::Engine(format!("text blocks: {e}")))
    }

    fn replace_page_text(
        &self,
        doc: DocumentId,
        page: PageIndex,
        edit: &pdf_types::TextReplace,
    ) -> CoreResult<Vec<u8>> {
        let pdfium = shared_pdfium()?;
        // The lock is held throughout: PDFium is not thread-safe, even for
        // the private copy edited here.
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let bytes: &[u8] = match &entry.orig {
            Some(orig) => orig,
            // SAFETY: `bytes_raw` is live until the entry is dropped and we
            // hold the lock preventing that.
            None => unsafe { &*entry.bytes_raw },
        };
        let mut copy = pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| CoreError::Engine(format!("replace text: load: {e}")))?;
        textedit::replace(&mut copy, page.0 as u16, edit).map_err(CoreError::Engine)?;
        copy.save_to_bytes()
            .map_err(|e| CoreError::Engine(format!("replace text: save: {e}")))
    }

    fn outline(&self, doc: DocumentId) -> CoreResult<Vec<OutlineNode>> {
        let mut inner = self.inner.lock();
        let e = inner.docs.get_mut(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        if let Some(ref out) = e.outline {
            return Ok(out.clone());
        }
        let document = e.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let mut out = Vec::new();
        for bm in document.bookmarks().iter() {
            out.push(collect_bookmark(&bm, document));
        }
        e.outline = Some(out.clone());
        Ok(out)
    }

    fn list_attachments(&self, doc: DocumentId) -> CoreResult<Vec<AttachmentInfo>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let attachments = document.attachments();
        let mut out = Vec::new();
        let n = attachments.len();
        for i in 0..n {
            let Ok(att) = attachments.get(i) else {
                continue;
            };
            out.push(AttachmentInfo {
                index: i as u32,
                name: att.name(),
                size: att.len() as u64,
            });
        }
        Ok(out)
    }

    fn read_attachment(&self, doc: DocumentId, index: u32) -> CoreResult<Vec<u8>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let attachments = document.attachments();
        let att = attachments
            .get(index as u16)
            .map_err(|e| CoreError::Engine(format!("attachment: {e}")))?;
        att.save_to_bytes()
            .map_err(|e| CoreError::Engine(format!("attachment read: {e}")))
    }

    fn document_metadata(&self, doc: DocumentId) -> CoreResult<DocMetadata> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let md = document.metadata();
        let get = |t: PdfDocumentMetadataTagType| -> Option<String> {
            md.get(t)
                .map(|tag| tag.value().to_string())
                .filter(|s| !s.is_empty())
        };
        Ok(DocMetadata {
            title: get(PdfDocumentMetadataTagType::Title),
            author: get(PdfDocumentMetadataTagType::Author),
            subject: get(PdfDocumentMetadataTagType::Subject),
            keywords: get(PdfDocumentMetadataTagType::Keywords),
            creator: get(PdfDocumentMetadataTagType::Creator),
            producer: get(PdfDocumentMetadataTagType::Producer),
            creation_date: get(PdfDocumentMetadataTagType::CreationDate),
            modification_date: get(PdfDocumentMetadataTagType::ModificationDate),
        })
    }

    fn document_bytes(&self, doc: DocumentId) -> CoreResult<Vec<u8>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        if let Some(orig) = &entry.orig {
            return Ok(orig.clone());
        }
        // SAFETY: `bytes_raw` is live until the entry is dropped and we
        // hold the lock preventing that.
        let bytes = unsafe { &*entry.bytes_raw };
        Ok(bytes.to_vec())
    }

    #[cfg(windows)]
    fn print_with_dialog(&self, doc: DocumentId) -> CoreResult<bool> {
        print_win::print_document_dialog(self, doc)
    }

    fn list_form_fields(&self, doc: DocumentId) -> CoreResult<Vec<FormFieldInfo>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        if document.form().is_none() {
            return Ok(Vec::new());
        }
        let mut out: Vec<FormFieldInfo> = Vec::new();
        for (page_idx, page) in document.pages().iter().enumerate() {
            let pv = page_view(&page);
            let annots = page.annotations();
            for annot_index in 0..annots.len() {
                let Ok(annot) = annots.get(annot_index) else { continue };
                let Some(widget) = annot.as_widget_annotation() else { continue };
                let Some(field) = widget.form_field() else { continue };
                let Ok(bounds) = widget.bounds() else { continue };
                let rect_pt = rect_to_top_origin(bounds, &pv);
                let id = field.name().unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let readonly = field.is_read_only();
                let required = field.is_required();
                let (kind, value, options, export_value, multiline, password) =
                    map_field(field);
                out.push(FormFieldInfo {
                    id,
                    page: page_idx as u32,
                    rect_pt,
                    kind,
                    value,
                    options,
                    export_value,
                    readonly,
                    required,
                    multiline,
                    password,
                    max_len: None,
                    annot_index: annot_index as u32,
                    text_style: None,
                });
            }
        }
        Ok(out)
    }

    fn reload_bytes(&self, doc: DocumentId, bytes: Vec<u8>) -> CoreResult<()> {
        shared_pdfium()?;
        let entry =
            DocEntry::build(bytes, None).map_err(|e| CoreError::Engine(format!("reload: {e}")))?;
        let mut inner = self.inner.lock();
        if !inner.docs.contains_key(&doc) {
            return Err(CoreError::DocumentNotLoaded);
        }
        // Drop old entry (reclaims old bytes + PdfDocument) before inserting new.
        inner.docs.remove(&doc);
        inner.docs.insert(doc, entry);
        Ok(())
    }

    fn render_region(
        &self,
        req: RenderRequest,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> CoreResult<RenderedPage> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(CoreError::Engine(format!("bad region size {width}x{height}")));
        }
        proxy::full_speed();
        let mut inner = self.inner.lock();
        let Inner { docs, note_icons } = &mut *inner;
        let entry = docs.get(&req.document).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.render_doc(req.zoom.0).ok_or(CoreError::DocumentNotLoaded)?;
        let t = std::time::Instant::now();
        let page = document
            .pages()
            .get(req.page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let (rot, swap) = match req.rotation {
            Rotation::R0 => (0, false),
            Rotation::R90 => (1, true),
            Rotation::R180 => (2, false),
            Rotation::R270 => (3, true),
        };
        let (pw, ph) = (page.width().value, page.height().value);
        let (dw, dh) = if swap { (ph, pw) } else { (pw, ph) };
        let full_w = (dw * req.zoom.0).round() as i32;
        let full_h = (dh * req.zoom.0).round() as i32;
        let bindings = document.bindings();
        let bitmap = PdfBitmap::empty(width as i32, height as i32, PdfBitmapFormat::BGRA, bindings)
            .map_err(|e| CoreError::Engine(format!("bitmap: {e}")))?;
        let bmp = bindings.get_handle_from_bitmap(&bitmap);
        bindings.FPDFBitmap_FillRect(bmp, 0, 0, width as i32, height as i32, 0xFFFF_FFFF);
        // Draw the whole page shifted by (-x, -y): PDFium clips to the
        // bitmap and skips page objects (image tiles) outside it entirely.
        const FPDF_ANNOT: i32 = 0x01;
        let page_handle = bindings.get_handle_from_page(&page);
        // Sticky-note icons are painted here, upright (see `notes`).
        let sticky =
            notes::HiddenNotes::hide(bindings, page_handle, req.page.0, || entry.note_colors());
        bindings.FPDF_RenderPageBitmap(bmp, page_handle, -x, -y, full_w, full_h, rot, FPDF_ANNOT);
        // Form fields are not part of that: PDFium draws widgets only through
        // the form module (as `render` does via pdfium-render).
        if document.form().is_some() {
            forms::draw_widgets(bindings, document, bmp, page_handle, (-x, -y), (full_w, full_h), rot, FPDF_ANNOT);
        }
        sticky.draw(note_icons, bmp, page_handle, (-x, -y), (full_w, full_h), rot, req.zoom.0, false);
        drop(sticky);
        let rgba = bitmap.as_rgba_bytes();
        tracing::info!(
            "pdfium::render_region page={} zoom={:.2} at {x},{y} {width}x{height} of {full_w}x{full_h} level={:?} t={:?}",
            req.page.0,
            req.zoom.0,
            entry.hi_level(req.zoom.0).map(|(i, _)| i),
            t.elapsed()
        );
        Ok(RenderedPage { width_px: width, height_px: height, rgba })
    }

    fn detail_scale(&self, doc: DocumentId) -> Option<f32> {
        let inner = self.inner.lock();
        inner.docs.get(&doc)?.hi.as_ref().map(|h| h.low_scale)
    }

    fn is_protected(&self, doc: DocumentId) -> CoreResult<bool> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        // Raw binding: pdfium-render's PdfSecurityHandlerRevision enum lacks
        // revisions 5/6 (AES-256). -1 = no security handler.
        let bindings = document.bindings();
        let handle = bindings.get_handle_from_document(document);
        Ok(bindings.FPDF_GetSecurityHandlerRevision(handle) != -1)
    }

    fn save_copy_bytes(&self, doc: DocumentId) -> CoreResult<Vec<u8>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;

        // pdfium-render's `save_to_bytes` hardcodes flags = 0, which keeps
        // the security handler — the saved copy would still require the
        // password. Drop down to the raw binding with FPDF_REMOVE_SECURITY
        // so the copy is plaintext.
        const FPDF_REMOVE_SECURITY: FPDF_DWORD = 3;

        #[repr(C)]
        struct BufWriter {
            fw: FPDF_FILEWRITE,
            buf: Vec<u8>,
        }
        unsafe extern "C" fn write_block(
            pthis: *mut FPDF_FILEWRITE,
            data: *const std::os::raw::c_void,
            size: std::os::raw::c_ulong,
        ) -> std::os::raw::c_int {
            // SAFETY: `pthis` points at the `fw` field of a `BufWriter`
            // (repr(C), `fw` first), so the cast recovers the full struct.
            let w = pthis as *mut BufWriter;
            let slice = std::slice::from_raw_parts(data as *const u8, size as usize);
            (*w).buf.extend_from_slice(slice);
            1
        }

        let mut writer = BufWriter {
            fw: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(write_block),
            },
            buf: Vec::new(),
        };
        let bindings = document.bindings();
        let handle = bindings.get_handle_from_document(document);
        let ok = bindings.FPDF_SaveAsCopy(
            handle,
            &mut writer.fw as *mut FPDF_FILEWRITE,
            FPDF_REMOVE_SECURITY,
        );
        if bindings.is_true(ok) {
            Ok(writer.buf)
        } else {
            Err(CoreError::Engine("save_copy: FPDF_SaveAsCopy failed".into()))
        }
    }

    fn document_permissions(&self, doc: DocumentId) -> CoreResult<DocPermissions> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        // Raw bindings instead of pdfium-render's permission helpers: those
        // all go through a revision enum that lacks 5/6 (AES-256) and
        // therefore error out, which would read as "everything denied".
        let bindings = document.bindings();
        let handle = bindings.get_handle_from_document(document);
        let rev = bindings.FPDF_GetSecurityHandlerRevision(handle);
        let handler = match rev {
            -1 => "Unprotected".to_string(),
            2 => "Standard (rev 2)".to_string(),
            3 => "Standard (rev 3)".to_string(),
            4 => "Standard (rev 4)".to_string(),
            5 => "Standard (rev 5, AES-256)".to_string(),
            6 => "Standard (rev 6, AES-256)".to_string(),
            r => format!("Unknown (rev {r})"),
        };
        // /P bitfield, 1-based bit positions per PDF spec table 22.
        // Unprotected documents report 0xFFFF_FFFF (all allowed).
        let p = bindings.FPDF_GetDocPermissions(handle) as u64;
        let bit = |n: u32| p & (1u64 << (n - 1)) != 0;
        let legacy = rev <= 2; // rev 2: bits 9-12 unused, bit 3 covers all printing
        Ok(DocPermissions {
            security_handler: handler,
            // Bit 12 only refines bit 3: without bit 3 there is no printing.
            can_print_high_quality: bit(3) && (legacy || bit(12)),
            can_print_low_quality: bit(3),
            can_modify_content: bit(4),
            can_extract_text_and_graphics: bit(5),
            can_annotate: bit(6),
            // Bit 6 already allows filling; bit 9 allows it even without 6.
            can_fill_forms: bit(6) || (!legacy && bit(9)),
            can_assemble: if legacy { bit(4) } else { bit(11) },
        })
    }

    fn page_links(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<PageLink>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let p = document
            .pages()
            .get(page.0 as u16)
            .map_err(|e| CoreError::Engine(format!("page: {e}")))?;
        let pv = page_view(&p);
        let mut out: Vec<PageLink> = Vec::new();
        for link in p.links().iter() {
            let Ok(rect) = link.rect() else { continue };
            let rect_pt = rect_to_top_origin(rect, &pv);
            let target = match link.action() {
                Some(PdfAction::LocalDestination(a)) => a
                    .destination()
                    .ok()
                    .and_then(|d| d.page_index().ok())
                    .map(|idx| LinkTarget::Page(PageIndex(idx as u32))),
                Some(PdfAction::Uri(a)) => a.uri().ok().map(LinkTarget::Uri),
                _ => link
                    .destination()
                    .and_then(|d| d.page_index().ok())
                    .map(|idx| LinkTarget::Page(PageIndex(idx as u32))),
            };
            let Some(target) = target else { continue };
            out.push(PageLink { rect_pt, target });
        }
        Ok(out)
    }

    fn search(&self, doc: DocumentId, query: &SearchQuery) -> CoreResult<Vec<SearchHit>> {
        if query.term.is_empty() {
            return Ok(Vec::new());
        }
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;

        // Case folding must be identical on both sides and keep one char per
        // char (haystack offsets map back to `chars` by index), so fold per
        // char with `fold_case` — `to_lowercase` on the needle but ASCII-only
        // folding on the page text never matched e.g. "Über" against "über".
        let needle: String = if query.case_sensitive {
            query.term.clone()
        } else {
            query.term.chars().map(fold_case).collect()
        };

        let mut hits: Vec<SearchHit> = Vec::new();
        for (page_idx, p) in document.pages().iter().enumerate() {
            let pv = page_view(&p);
            let Ok(text) = p.text() else { continue };
            let mut chars: Vec<(char, RectPt)> = Vec::new();
            for ch in text.chars().iter() {
                let Some(c) = ch.unicode_char() else { continue };
                if c == '\0' {
                    continue;
                }
                let Ok(r) = ch.loose_bounds() else { continue };
                chars.push((c, rect_to_top_origin(r, &pv)));
            }
            let haystack: String = chars
                .iter()
                .map(|(c, _)| if query.case_sensitive { *c } else { fold_case(*c) })
                .collect();
            let needle_len_chars = needle.chars().count();
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

/// Lowercase `c` when that yields exactly one char (true for virtually all
/// letters, including umlauts); otherwise keep it, so text length in chars
/// is preserved.
fn fold_case(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
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

fn map_field(
    field: &PdfFormField<'_>,
) -> (
    FormFieldKind,
    Option<String>,
    Option<Vec<String>>,
    Option<String>,
    bool,
    bool,
) {
    match field.field_type() {
        PdfFormFieldType::Text => {
            let tf = field.as_text_field().expect("text field");
            (
                FormFieldKind::Text,
                tf.value(),
                None,
                None,
                tf.is_multiline(),
                tf.is_password(),
            )
        }
        PdfFormFieldType::Checkbox => {
            // pdfium-render 0.8 only honors "Yes" as the on-state name; PDFs
            // commonly use other names (e.g. "On", "Ja", "_3F"), so we read
            // the appearance state directly and treat anything other than
            // missing or "Off" as checked.
            let stream = field.appearance_stream().unwrap_or_default();
            let on = !stream.is_empty()
                && stream != "Off"
                && stream != "/Off";
            (
                FormFieldKind::Checkbox,
                Some(if on { "Yes".to_string() } else { "Off".to_string() }),
                None,
                None,
                false,
                false,
            )
        }
        PdfFormFieldType::RadioButton => {
            let rb = field.as_radio_button_field().expect("radio field");
            let stream = field.appearance_stream().unwrap_or_default();
            let on = !stream.is_empty()
                && stream != "Off"
                && stream != "/Off";
            let idx = rb.index_in_group();
            (
                FormFieldKind::Radio,
                Some(if on { "On".to_string() } else { "Off".to_string() }),
                None,
                Some(idx.to_string()),
                false,
                false,
            )
        }
        PdfFormFieldType::ComboBox => {
            let cb = field.as_combo_box_field().expect("combo field");
            let opts: Vec<String> = cb
                .options()
                .iter()
                .map(|o| o.label().cloned().unwrap_or_default())
                .collect();
            (
                FormFieldKind::ComboBox,
                cb.value(),
                Some(opts),
                None,
                false,
                false,
            )
        }
        PdfFormFieldType::ListBox => {
            let lb = field.as_list_box_field().expect("listbox field");
            let opts: Vec<String> = lb
                .options()
                .iter()
                .map(|o| o.label().cloned().unwrap_or_default())
                .collect();
            (
                FormFieldKind::ListBox,
                lb.value(),
                Some(opts),
                None,
                false,
                false,
            )
        }
        PdfFormFieldType::PushButton => (FormFieldKind::PushButton, None, None, None, false, false),
        PdfFormFieldType::Signature => (FormFieldKind::Signature, None, None, None, false, false),
        PdfFormFieldType::Unknown => (FormFieldKind::Unknown, None, None, None, false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::fold_case;

    #[test]
    fn fold_case_handles_umlauts_and_keeps_char_count() {
        let fold = |s: &str| s.chars().map(fold_case).collect::<String>();
        assert_eq!(fold("ÜBER Straße"), "über straße");
        // Multi-char lowercase mappings are left alone (1:1 char mapping).
        assert_eq!(fold("İ").chars().count(), 1);
    }
}
