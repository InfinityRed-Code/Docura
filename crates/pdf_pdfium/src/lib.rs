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
    AttachmentInfo, DocMetadata, DocPermissions, DocumentId, FormFieldInfo, FormFieldKind,
    LinkTarget, OutlineNode, PageIndex, PageLink, PageSize, RectPt, Rotation, SearchHit,
    SearchQuery, TextRun,
};
use pdfium_render::prelude::*;

#[cfg(windows)]
mod print_win;

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

/// Per-document state. Owns the leaked byte buffer and the resident
/// `PdfDocument`. Dropping this entry tears down the document handle and
/// reclaims the bytes.
struct DocEntry {
    doc: Option<PdfDocument<'static>>,
    bytes_raw: *mut [u8],
    page_count: u32,
    // Per-page sizes resolved lazily on first access. `None` = not yet
    // fetched. Opening a 500-page PDF no longer blocks on N sequential
    // page-size queries.
    sizes: Vec<Option<PageSize>>,
    // Outline built lazily on first request. `None` = not yet walked.
    outline: Option<Vec<OutlineNode>>,
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
}

// SAFETY: All access to `Inner` (and therefore to any `PdfDocument` it
// holds) is serialised through `PdfiumEngine::inner: Mutex<Inner>`. PDFium
// itself is not thread-safe, so only one thread ever touches these
// handles at a time. The raw `*mut [u8]` in `DocEntry` is not shared.
unsafe impl Send for Inner {}

pub struct PdfiumEngine {
    inner: Mutex<Inner>,
}

impl PdfiumEngine {
    pub fn new() -> CoreResult<Self> {
        shared_pdfium()?;
        Ok(Self {
            inner: Mutex::new(Inner {
                docs: HashMap::new(),
            }),
        })
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
        let pdfium = shared_pdfium()?;
        let bytes_len = bytes.len();
        // Leak bytes so the PdfDocument can borrow them for 'static.
        let bytes_box: Box<[u8]> = bytes.into_boxed_slice();
        let bytes_raw: *mut [u8] = Box::into_raw(bytes_box);
        // SAFETY: we just created `bytes_raw` and retain it until this
        // DocEntry is dropped.
        let bytes_static: &'static [u8] = unsafe { &*bytes_raw };

        let t_load = std::time::Instant::now();
        let document = match pdfium.load_pdf_from_byte_slice(bytes_static, password.as_deref()) {
            Ok(d) => d,
            Err(e) => {
                // SAFETY: bytes_raw still owns the leaked buffer, reclaim it.
                unsafe {
                    drop(Box::from_raw(bytes_raw));
                }
                return Err(password_error(e, password.is_some()));
            }
        };
        let load_t = t_load.elapsed();

        let t_pc = std::time::Instant::now();
        let page_count = document.pages().len() as u32;
        tracing::info!(
            "pdfium::open bytes={:.1}MB load={:?} page_count_enum={:?} pages={}",
            bytes_len as f64 / 1024.0 / 1024.0,
            load_t,
            t_pc.elapsed(),
            page_count
        );

        let id = DocumentId::new();
        let entry = DocEntry {
            doc: Some(document),
            bytes_raw,
            page_count,
            sizes: vec![None; page_count as usize],
            outline: None,
        };
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
        let t_lock = std::time::Instant::now();
        let inner = self.inner.lock();
        let lock_t = t_lock.elapsed();
        let entry = inner
            .docs
            .get(&req.document)
            .ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
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
            .rotate(rot, true);
        let t_render = std::time::Instant::now();
        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| CoreError::Engine(format!("render: {e}")))?;
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
        Ok(text.all())
    }

    fn page_text_layout(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<TextRun>> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
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
            let page_h = page.height().value;
            for annot in page.annotations().iter() {
                let Some(widget) = annot.as_widget_annotation() else { continue };
                let Some(field) = widget.form_field() else { continue };
                let Ok(bounds) = widget.bounds() else { continue };
                let rect_pt = RectPt {
                    x0: bounds.left().value,
                    y0: page_h - bounds.top().value,
                    x1: bounds.right().value,
                    y1: page_h - bounds.bottom().value,
                };
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
                });
            }
        }
        Ok(out)
    }

    fn reload_bytes(&self, doc: DocumentId, bytes: Vec<u8>) -> CoreResult<()> {
        let pdfium = shared_pdfium()?;
        let bytes_box: Box<[u8]> = bytes.into_boxed_slice();
        let bytes_raw: *mut [u8] = Box::into_raw(bytes_box);
        let bytes_static: &'static [u8] = unsafe { &*bytes_raw };
        let document = match pdfium.load_pdf_from_byte_slice(bytes_static, None) {
            Ok(d) => d,
            Err(e) => {
                unsafe { drop(Box::from_raw(bytes_raw)); }
                return Err(CoreError::Engine(format!("reload: {e}")));
            }
        };
        let page_count = document.pages().len() as u32;
        let mut inner = self.inner.lock();
        if !inner.docs.contains_key(&doc) {
            unsafe { drop(Box::from_raw(bytes_raw)); }
            return Err(CoreError::DocumentNotLoaded);
        }
        // Drop old entry (reclaims old bytes + PdfDocument) before inserting new.
        inner.docs.remove(&doc);
        let entry = DocEntry {
            doc: Some(document),
            bytes_raw,
            page_count,
            sizes: vec![None; page_count as usize],
            outline: None,
        };
        inner.docs.insert(doc, entry);
        Ok(())
    }

    fn document_permissions(&self, doc: DocumentId) -> CoreResult<DocPermissions> {
        let inner = self.inner.lock();
        let entry = inner.docs.get(&doc).ok_or(CoreError::DocumentNotLoaded)?;
        let document = entry.doc.as_ref().ok_or(CoreError::DocumentNotLoaded)?;
        let perms = document.permissions();
        let handler = perms
            .security_handler_revision()
            .map(|r| match r {
                PdfSecurityHandlerRevision::Unprotected => "Unprotected".to_string(),
                PdfSecurityHandlerRevision::Revision2 => "Standard (rev 2)".to_string(),
                PdfSecurityHandlerRevision::Revision3 => "Standard (rev 3)".to_string(),
                PdfSecurityHandlerRevision::Revision4 => "Standard (rev 4)".to_string(),
            })
            .unwrap_or_else(|_| "Unknown".to_string());
        Ok(DocPermissions {
            security_handler: handler,
            can_print_high_quality: perms.can_print_high_quality().unwrap_or(false),
            can_print_low_quality: perms.can_print_only_low_quality().unwrap_or(false),
            can_modify_content: perms.can_modify_document_content().unwrap_or(false),
            can_extract_text_and_graphics: perms.can_extract_text_and_graphics().unwrap_or(false),
            can_annotate: perms.can_add_or_modify_text_annotations().unwrap_or(false),
            can_fill_forms: perms
                .can_fill_existing_interactive_form_fields()
                .unwrap_or(false),
            can_assemble: perms.can_assemble_document().unwrap_or(false),
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
        let page_h = p.height().value;
        let mut out: Vec<PageLink> = Vec::new();
        for link in p.links().iter() {
            let Ok(rect) = link.rect() else { continue };
            let rect_pt = rect_to_top_origin(rect, page_h);
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

        let needle: String = if query.case_sensitive {
            query.term.clone()
        } else {
            query.term.to_lowercase()
        };

        let mut hits: Vec<SearchHit> = Vec::new();
        for (page_idx, p) in document.pages().iter().enumerate() {
            let page_h = p.height().value;
            let Ok(text) = p.text() else { continue };
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
