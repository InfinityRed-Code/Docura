use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use parking_lot::Mutex;

mod vprinter;

use pdf_cache::{CacheKey, CachedImage, RenderCache};
use pdf_core::{CoreError, PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{
    AnnotationInfo, AttachmentInfo, DocMetadata, DocPermissions, DocumentId, FormFieldInfo,
    OutlineNode, PageIndex, PageLink, PageSize, RecentFile, RectPt, Rotation, SearchHit,
    SearchQuery, TextRun, ZoomLevel,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;

const UNDO_CAP: usize = 50;

#[derive(Default)]
struct DocUndo {
    undo: VecDeque<Vec<u8>>,
    redo: VecDeque<Vec<u8>>,
}

#[derive(Default)]
struct UndoStacks {
    map: HashMap<DocumentId, DocUndo>,
}

impl UndoStacks {
    fn entry(&mut self, id: DocumentId) -> &mut DocUndo {
        self.map.entry(id).or_default()
    }
    fn record(&mut self, id: DocumentId, pre: Vec<u8>) {
        let e = self.entry(id);
        e.undo.push_back(pre);
        while e.undo.len() > UNDO_CAP {
            e.undo.pop_front();
        }
        e.redo.clear();
    }
    fn pop_undo(&mut self, id: DocumentId, current: Vec<u8>) -> Option<Vec<u8>> {
        let e = self.entry(id);
        let prev = e.undo.pop_back()?;
        e.redo.push_back(current);
        while e.redo.len() > UNDO_CAP {
            e.redo.pop_front();
        }
        Some(prev)
    }
    fn pop_redo(&mut self, id: DocumentId, current: Vec<u8>) -> Option<Vec<u8>> {
        let e = self.entry(id);
        let next = e.redo.pop_back()?;
        e.undo.push_back(current);
        while e.undo.len() > UNDO_CAP {
            e.undo.pop_front();
        }
        Some(next)
    }
    fn clear_doc(&mut self, id: DocumentId) {
        self.map.remove(&id);
    }
    fn lengths(&self, id: DocumentId) -> (usize, usize) {
        self.map
            .get(&id)
            .map(|e| (e.undo.len(), e.redo.len()))
            .unwrap_or((0, 0))
    }
}

/// Protection settings of an open document. The resident engine copy is
/// always plaintext (decrypted on open); these settings are re-applied by
/// `save_pdf_as` when the file is written to disk.
#[derive(Clone)]
struct DocCrypto {
    /// User password (empty = open without prompt, restrictions only).
    user_password: String,
    /// Owner password; falls back to the user password when empty.
    owner_password: String,
    permissions: pdf_mutate::PermissionOptions,
}

struct AppState {
    engine: Arc<dyn PdfEngine>,
    cache: Mutex<RenderCache>,
    settings_path: Mutex<Option<PathBuf>>,
    undo: Mutex<UndoStacks>,
    doc_crypto: Mutex<HashMap<DocumentId, DocCrypto>>,
}

#[derive(Serialize)]
struct OpenedDocument {
    id: DocumentId,
    page_count: u32,
    page_sizes: Vec<PageSize>,
    outline: Vec<OutlineNode>,
    /// True when the document was password-protected / permission-restricted.
    /// The protection is re-applied on save.
    protected: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value")]
enum OpenError {
    PasswordRequired,
    PasswordIncorrect,
    Other(String),
}

impl From<CoreError> for OpenError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::PasswordRequired => OpenError::PasswordRequired,
            CoreError::PasswordIncorrect => OpenError::PasswordIncorrect,
            other => OpenError::Other(other.to_string()),
        }
    }
}

#[tauri::command]
async fn close_pdf(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<(), String> {
    let engine = state.engine.clone();
    state.cache.lock().remove_document(document);
    state.undo.lock().clear_doc(document);
    state.doc_crypto.lock().remove(&document);
    tokio::task::spawn_blocking(move || engine.close(document).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_pdf_path(
    state: tauri::State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Result<OpenedDocument, OpenError> {
    let t_read = std::time::Instant::now();
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| OpenError::Other(format!("read {path}: {e}")))?;
    tracing::info!("open_pdf_path: read {:.1}MB in {:?}", bytes.len() as f64 / 1024.0 / 1024.0, t_read.elapsed());
    open_pdf_inner(state, bytes, password).await
}

#[tauri::command]
async fn open_pdf(
    state: tauri::State<'_, AppState>,
    bytes: Vec<u8>,
    password: Option<String>,
) -> Result<OpenedDocument, OpenError> {
    open_pdf_inner(state, bytes, password).await
}

async fn open_pdf_inner(
    state: tauri::State<'_, AppState>,
    bytes: Vec<u8>,
    password: Option<String>,
) -> Result<OpenedDocument, OpenError> {
    let engine = state.engine.clone();
    let size_mb = bytes.len() as f64 / 1024.0 / 1024.0;
    let result = tokio::task::spawn_blocking(move || -> Result<(OpenedDocument, Option<DocCrypto>), OpenError> {
        let t0 = std::time::Instant::now();
        let id = engine.open_bytes_with_password(bytes, password.clone())?;
        // Protected document: flatten to a plaintext working copy so the
        // lopdf mutation pipeline and save work on readable bytes. The
        // protection settings are re-applied on save.
        let mut crypto: Option<DocCrypto> = None;
        if engine.is_protected(id).unwrap_or(false) {
            let perms = engine
                .document_permissions(id)
                .map(|p| pdf_mutate::PermissionOptions {
                    print: p.can_print_high_quality || p.can_print_low_quality,
                    modify: p.can_modify_content,
                    copy: p.can_extract_text_and_graphics,
                    annotate: p.can_annotate,
                })
                .unwrap_or_default();
            match engine
                .save_copy_bytes(id)
                .and_then(|flat| engine.reload_bytes(id, flat))
            {
                Ok(()) => {
                    crypto = Some(DocCrypto {
                        user_password: password.unwrap_or_default(),
                        owner_password: String::new(),
                        permissions: perms,
                    });
                    tracing::info!("open_pdf: flattened protected document, perms={perms:?}");
                }
                Err(e) => {
                    // Keep the password-opened original; saving keeps the
                    // original encrypted bytes, mutations are unavailable.
                    tracing::warn!("open_pdf: could not flatten protected document: {e}");
                }
            }
        }
        let t_parse = t0.elapsed();
        let t_pc0 = std::time::Instant::now();
        let page_count = engine
            .page_count(id)
            .map_err(|e| OpenError::Other(e.to_string()))?;
        let t_page_count = t_pc0.elapsed();
        let t_sz0 = std::time::Instant::now();
        let mut page_sizes = Vec::with_capacity(page_count as usize);
        for i in 0..page_count {
            let sz = engine
                .page_size(id, PageIndex(i))
                .map_err(|e| OpenError::Other(e.to_string()))?;
            page_sizes.push(sz);
        }
        let t_sizes = t_sz0.elapsed();
        let t_out0 = std::time::Instant::now();
        let outline = engine.outline(id).unwrap_or_default();
        let t_outline = t_out0.elapsed();
        tracing::info!(
            "open_pdf: size={:.1}MB pages={} parse={:?} page_count_call={:?} sizes_loop={:?} outline={:?} total={:?}",
            size_mb,
            page_count,
            t_parse,
            t_page_count,
            t_sizes,
            t_outline,
            t0.elapsed()
        );
        // `crypto` is set when the document was flattened; if flattening
        // failed the resident copy still reports as protected itself.
        let protected = crypto.is_some() || engine.is_protected(id).unwrap_or(false);
        Ok((
            OpenedDocument {
                id,
                page_count,
                page_sizes,
                outline,
                protected,
            },
            crypto,
        ))
    })
    .await
    .map_err(|e| OpenError::Other(e.to_string()))?;
    let (doc, crypto) = result?;
    if let Some(c) = crypto {
        state.doc_crypto.lock().insert(doc.id, c);
    }
    Ok(doc)
}

#[derive(Serialize)]
struct RenderedPayload {
    width: u32,
    height: u32,
    data_url: String,
}

fn make_cache_key(doc: DocumentId, page: u32, zoom: f32, rotation_q: u8) -> CacheKey {
    CacheKey {
        document: doc,
        page: PageIndex(page),
        zoom_milli: (zoom * 1000.0).round() as u32,
        rotation_q: rotation_q % 4,
    }
}

#[tauri::command]
async fn render_page(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
    zoom: f32,
    rotation: Option<u8>,
) -> Result<RenderedPayload, String> {
    render_internal(state, document, page, zoom, rotation, 82).await
}

async fn render_internal(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
    zoom: f32,
    rotation: Option<u8>,
    quality: u8,
) -> Result<RenderedPayload, String> {
    let rot_q = rotation.unwrap_or(0) % 4;
    let rotation = Rotation::from_quarter_turns(rot_q);
    let key = make_cache_key(document, page, zoom, rot_q);
    let t_total = std::time::Instant::now();
    if let Some(cached) = state.cache.lock().get(&key).cloned() {
        tracing::info!(
            "render_page HIT page={} zoom={} {}x{} total={:?}",
            page, zoom, cached.width, cached.height, t_total.elapsed()
        );
        return Ok(RenderedPayload {
            width: cached.width,
            height: cached.height,
            data_url: cached.data_url,
        });
    }
    let engine = state.engine.clone();
    let rendered = tokio::task::spawn_blocking(move || {
        let t0 = std::time::Instant::now();
        let r = engine
            .render(RenderRequest {
                document,
                page: PageIndex(page),
                zoom: ZoomLevel(zoom),
                rotation,
            })
            .map_err(|e| e.to_string());
        tracing::info!("pdfium_render page={} zoom={} t={:?}", page, zoom, t0.elapsed());
        r
    })
    .await
    .map_err(|e| e.to_string())??;
    let w = rendered.width_px;
    let h = rendered.height_px;
    let rgba = rendered.rgba;
    let t_enc0 = std::time::Instant::now();
    let data_url = tokio::task::spawn_blocking(move || encode_data_url(w, h, &rgba, quality))
        .await
        .map_err(|e| e.to_string())??;
    let enc_t = t_enc0.elapsed();
    let t_cache0 = std::time::Instant::now();
    state.cache.lock().insert(
        key,
        CachedImage {
            width: w,
            height: h,
            data_url: data_url.clone(),
        },
    );
    let t_cache = t_cache0.elapsed();
    tracing::info!(
        "render_page MISS page={} zoom={} q={} {}x{} encode={:?} cache_insert={:?} total={:?}",
        page, zoom, quality, w, h, enc_t, t_cache, t_total.elapsed()
    );
    Ok(RenderedPayload { width: w, height: h, data_url })
}

fn encode_data_url(width: u32, height: u32, rgba: &[u8], quality: u8) -> Result<String, String> {
    let jpg = encode_jpeg(width, height, rgba, quality)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
    Ok(format!("data:image/jpeg;base64,{b64}"))
}

#[tauri::command]
async fn render_thumbnail(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
    max_width: u32,
    page_width_pt: f32,
    rotation: Option<u8>,
) -> Result<RenderedPayload, String> {
    let pt = page_width_pt.max(1.0);
    let zoom = (max_width as f32 / pt).clamp(0.05, 1.0);
    render_internal(state, document, page, zoom, rotation, 60).await
}

#[tauri::command]
async fn page_text_layout_cmd(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
) -> Result<Vec<TextRun>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .page_text_layout(document, PageIndex(page))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn page_links_cmd(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
) -> Result<Vec<PageLink>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .page_links(document, PageIndex(page))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_attachments_cmd(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<Vec<AttachmentInfo>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine.list_attachments(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_attachment_cmd(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    index: u32,
    path: String,
) -> Result<(), String> {
    let engine = state.engine.clone();
    let data = tokio::task::spawn_blocking(move || {
        engine.read_attachment(document, index).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    tokio::task::spawn_blocking(move || std::fs::write(&path, &data).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_pdf_as(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    path: String,
) -> Result<(), String> {
    let engine = state.engine.clone();
    let crypto = state.doc_crypto.lock().get(&document).cloned();
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        match crypto {
            // Re-apply password protection unless the resident bytes are
            // still the original encrypted file (flatten failed on open).
            Some(c) if !pdf_mutate::probably_encrypted(&bytes) => {
                let enc = pdf_mutate::encrypt_pdf(
                    &bytes,
                    &c.user_password,
                    &c.owner_password,
                    c.permissions,
                )
                .map_err(|e| format!("encrypt: {e}"))?;
                tracing::info!("save_pdf_as: re-encrypted ({} -> {} bytes)", bytes.len(), enc.len());
                Ok(enc)
            }
            _ => Ok(bytes),
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| format!("write {path}: {e}"))?;
    tracing::info!("save_pdf_as: wrote {} bytes to {}", bytes.len(), path);
    Ok(())
}

#[derive(Serialize)]
struct PdfSecurityInfo {
    protected: bool,
    has_user_password: bool,
    permissions: pdf_mutate::PermissionOptions,
}

#[tauri::command]
fn get_pdf_security(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> PdfSecurityInfo {
    match state.doc_crypto.lock().get(&document) {
        Some(c) => PdfSecurityInfo {
            protected: true,
            has_user_password: !c.user_password.is_empty(),
            permissions: c.permissions,
        },
        None => PdfSecurityInfo {
            protected: false,
            has_user_password: false,
            permissions: pdf_mutate::PermissionOptions::default(),
        },
    }
}

#[tauri::command]
async fn set_pdf_protection(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    user_password: String,
    owner_password: String,
    permissions: pdf_mutate::PermissionOptions,
) -> Result<(), String> {
    if user_password.is_empty() && owner_password.is_empty() {
        return Err("a user or owner password is required".into());
    }
    // If flattening failed on open, the resident bytes are still encrypted;
    // `save_pdf_as` would silently keep the original protection and drop
    // these settings. Refuse instead of pretending it worked.
    let engine = state.engine.clone();
    let still_encrypted =
        tokio::task::spawn_blocking(move || engine.is_protected(document).unwrap_or(false))
            .await
            .map_err(|e| e.to_string())?;
    if still_encrypted {
        return Err(
            "the document could not be converted to an editable copy; its existing protection cannot be changed"
                .into(),
        );
    }
    state.doc_crypto.lock().insert(
        document,
        DocCrypto { user_password, owner_password, permissions },
    );
    Ok(())
}

#[tauri::command]
fn remove_pdf_protection(state: tauri::State<'_, AppState>, document: DocumentId) {
    state.doc_crypto.lock().remove(&document);
}

#[derive(Serialize)]
struct DocSnapshot {
    page_count: u32,
    page_sizes: Vec<PageSize>,
    outline: Vec<OutlineNode>,
}

async fn snapshot_doc(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<DocSnapshot, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<DocSnapshot, String> {
        let page_count = engine.page_count(document).map_err(|e| e.to_string())?;
        let mut page_sizes = Vec::with_capacity(page_count as usize);
        for i in 0..page_count {
            let sz = engine
                .page_size(document, PageIndex(i))
                .map_err(|e| e.to_string())?;
            page_sizes.push(sz);
        }
        let outline = engine.outline(document).unwrap_or_default();
        Ok(DocSnapshot {
            page_count,
            page_sizes,
            outline,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_page(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
) -> Result<DocSnapshot, String> {
    delete_pages_inner(state, document, vec![page]).await
}

#[tauri::command]
async fn delete_pages(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    pages: Vec<u32>,
) -> Result<DocSnapshot, String> {
    delete_pages_inner(state, document, pages).await
}

async fn delete_pages_inner(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    pages: Vec<u32>,
) -> Result<DocSnapshot, String> {
    let engine = state.engine.clone();
    let pages_for_task = pages.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let _new_count = mdoc.delete_pages(&pages_for_task).map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("delete_pages {:?}: {:?}", pages_for_task, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    // Invalidate render cache for this doc before reload so stale keys don't
    // resolve to removed pages.
    state.cache.lock().remove_document(document);

    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);

    snapshot_doc(state, document).await
}

#[tauri::command]
async fn extract_pages(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    pages: Vec<u32>,
    path: String,
) -> Result<(), String> {
    if pages.is_empty() {
        return Err("extract_pages: no pages given".to_string());
    }
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let out = mdoc.extract_to_bytes(&pages).map_err(|e| e.to_string())?;
        std::fs::write(&path, &out).map_err(|e| e.to_string())?;
        tracing::info!(
            "extract_pages {:?} → {} ({:.1}KB) in {:?}",
            pages,
            path,
            out.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(())
}

#[tauri::command]
async fn save_region(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    page: u32,
    rect: RectPt,
    path: String,
) -> Result<(), String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let out = mdoc
            .extract_region_to_bytes(page, rect)
            .map_err(|e| e.to_string())?;
        std::fs::write(&path, &out).map_err(|e| format!("write {path}: {e}"))?;
        tracing::info!(
            "save_region page={} rect={:?} → {} ({:.1}KB) in {:?}",
            page,
            rect,
            path,
            out.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn reorder_pages(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    order: Vec<u32>,
) -> Result<DocSnapshot, String> {
    if order.is_empty() {
        return Err("reorder_pages: empty order".to_string());
    }
    let engine = state.engine.clone();
    let order_for_task = order.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        mdoc.reorder_pages(&order_for_task).map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("reorder_pages n={}: {:?}", order_for_task.len(), t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);

    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);

    snapshot_doc(state, document).await
}

#[tauri::command]
async fn rotate_pages(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    pages: Vec<u32>,
    degrees: i32,
) -> Result<DocSnapshot, String> {
    if pages.is_empty() {
        return Err("rotate_pages: no pages given".to_string());
    }
    let engine = state.engine.clone();
    let pages_for_task = pages.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        mdoc.rotate_pages(&pages_for_task, degrees)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!(
            "rotate_pages {:?} deg={}: {:?}",
            pages_for_task,
            degrees,
            t.elapsed()
        );
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);

    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);

    snapshot_doc(state, document).await
}

#[tauri::command]
async fn insert_pdf_files(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    paths: Vec<String>,
    at_index: u32,
    src_pages: Option<Vec<u32>>,
) -> Result<DocSnapshot, String> {
    if paths.is_empty() {
        return Err("insert_pdf_files: no paths".to_string());
    }
    let mut sources: Vec<Vec<u8>> = Vec::with_capacity(paths.len());
    for p in &paths {
        let bytes = tokio::fs::read(p)
            .await
            .map_err(|e| format!("read {p}: {e}"))?;
        // lopdf cannot parse encrypted inputs — the merge would silently
        // produce a broken document.
        if pdf_mutate::is_encrypted_pdf(&bytes) {
            return Err(format!("ENCRYPTED_INPUT:{p}"));
        }
        sources.push(bytes);
    }
    let engine = state.engine.clone();
    let paths_len = paths.len();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let mut at = at_index;
        for (i, src) in sources.iter().enumerate() {
            let filter = if i == 0 { src_pages.as_deref() } else { None };
            let before = mdoc.page_count();
            let after = mdoc.insert_pdf(src, filter, at).map_err(|e| e.to_string())?;
            at += after.saturating_sub(before);
        }
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!(
            "insert_pdf_files n_files={} at={} total_bytes={} in {:?}",
            paths_len,
            at_index,
            out.len(),
            t.elapsed()
        );
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);

    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .reload_bytes(document, new_bytes)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);

    snapshot_doc(state, document).await
}

#[derive(Serialize)]
struct SplitResult {
    files: Vec<String>,
}

fn parse_ranges(spec: &str, page_count: u32) -> Result<Vec<Vec<u32>>, String> {
    let mut groups: Vec<Vec<u32>> = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let group = if let Some((a, b)) = part.split_once('-') {
            let a: u32 = a.trim().parse().map_err(|_| format!("bad range: {part}"))?;
            let b: u32 = b.trim().parse().map_err(|_| format!("bad range: {part}"))?;
            if a == 0 || b == 0 || a > b {
                return Err(format!("bad range: {part}"));
            }
            if b > page_count {
                return Err(format!("range {part} exceeds page count {page_count}"));
            }
            ((a - 1)..b).collect()
        } else {
            let v: u32 = part.parse().map_err(|_| format!("bad page: {part}"))?;
            if v == 0 || v > page_count {
                return Err(format!("page {part} out of range"));
            }
            vec![v - 1]
        };
        groups.push(group);
    }
    if groups.is_empty() {
        return Err("no ranges specified".into());
    }
    Ok(groups)
}

#[tauri::command]
async fn split_pdf(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    ranges: String,
    out_dir: String,
    base_name: String,
) -> Result<SplitResult, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<SplitResult, String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let page_count = mdoc.page_count();
        let groups = parse_ranges(&ranges, page_count)?;
        let dir = PathBuf::from(&out_dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {out_dir}: {e}"))?;
        let mut written: Vec<String> = Vec::with_capacity(groups.len());
        for (i, group) in groups.iter().enumerate() {
            let out_bytes = mdoc.extract_to_bytes(group).map_err(|e| e.to_string())?;
            let suffix = if group.len() == 1 {
                format!("page_{}", group[0] + 1)
            } else {
                format!("pages_{}-{}", group[0] + 1, group[group.len() - 1] + 1)
            };
            let name = format!("{base_name}_part{:02}_{suffix}.pdf", i + 1);
            let p = dir.join(&name);
            std::fs::write(&p, &out_bytes)
                .map_err(|e| format!("write {}: {e}", p.display()))?;
            written.push(p.to_string_lossy().to_string());
        }
        tracing::info!(
            "split_pdf: {} parts in {:?}",
            written.len(),
            t.elapsed()
        );
        Ok(SplitResult { files: written })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct MergeOpenedDocument {
    id: DocumentId,
    page_count: u32,
    page_sizes: Vec<PageSize>,
    outline: Vec<OutlineNode>,
    protected: bool,
    temp_path: String,
}

#[tauri::command]
async fn merge_files_and_open(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
) -> Result<MergeOpenedDocument, String> {
    if paths.len() < 2 {
        return Err("merge_files_and_open: need at least 2 files".to_string());
    }
    let mut sources: Vec<Vec<u8>> = Vec::with_capacity(paths.len());
    for p in &paths {
        let bytes = tokio::fs::read(p)
            .await
            .map_err(|e| format!("read {p}: {e}"))?;
        if pdf_mutate::is_encrypted_pdf(&bytes) {
            return Err(format!("ENCRYPTED_INPUT:{p}"));
        }
        sources.push(bytes);
    }
    let merged_bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let t = std::time::Instant::now();
        let mut iter = sources.into_iter();
        let base = iter.next().ok_or("empty sources")?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&base).map_err(|e| e.to_string())?;
        for src in iter {
            let at = mdoc.page_count();
            mdoc.insert_pdf(&src, None, at)
                .map_err(|e| e.to_string())?;
        }
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!(
            "merge_files_and_open: {} files -> {:.1}KB in {:?}",
            0, // len moved; printed via paths.len() below
            out.len() as f64 / 1024.0,
            t.elapsed()
        );
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())??;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("rustpdf-merged-{ts}.pdf"));
    tokio::fs::write(&tmp, &merged_bytes)
        .await
        .map_err(|e| format!("write temp {}: {e}", tmp.display()))?;
    let tmp_str = tmp.to_string_lossy().to_string();

    let opened = open_pdf_inner(state, merged_bytes, None)
        .await
        .map_err(|e| match e {
            OpenError::Other(s) => s,
            OpenError::PasswordRequired => "PasswordRequired".into(),
            OpenError::PasswordIncorrect => "PasswordIncorrect".into(),
        })?;
    Ok(MergeOpenedDocument {
        id: opened.id,
        page_count: opened.page_count,
        page_sizes: opened.page_sizes,
        outline: opened.outline,
        protected: false,
        temp_path: tmp_str,
    })
}

#[derive(Deserialize)]
struct WatermarkArgs {
    text: String,
    #[serde(default = "default_wm_font")]
    font_size: f32,
    #[serde(default = "default_wm_rot")]
    rotation_deg: f32,
    #[serde(default = "default_wm_opacity")]
    opacity: f32,
    #[serde(default = "default_wm_gray")]
    gray: f32,
}
fn default_wm_font() -> f32 { 72.0 }
fn default_wm_rot() -> f32 { 45.0 }
fn default_wm_opacity() -> f32 { 0.25 }
fn default_wm_gray() -> f32 { 0.5 }

#[tauri::command]
async fn add_watermark(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: WatermarkArgs,
) -> Result<DocSnapshot, String> {
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let opts = pdf_mutate::WatermarkOptions {
            font_size: args.font_size,
            rotation_deg: args.rotation_deg,
            opacity: args.opacity,
            gray: args.gray,
        };
        mdoc.add_text_watermark(&args.text, &opts)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_watermark: {:?}", t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct HeaderFooterArgs {
    #[serde(default)] header_left: String,
    #[serde(default)] header_center: String,
    #[serde(default)] header_right: String,
    #[serde(default)] footer_left: String,
    #[serde(default)] footer_center: String,
    #[serde(default)] footer_right: String,
    #[serde(default = "default_hf_font")]
    font_size: f32,
    #[serde(default = "default_hf_margin")]
    margin_pt: f32,
    #[serde(default = "default_hf_first")]
    first_page_number: u32,
    #[serde(default = "default_hf_gray")]
    gray: f32,
}
fn default_hf_font() -> f32 { 10.0 }
fn default_hf_margin() -> f32 { 24.0 }
fn default_hf_first() -> u32 { 1 }
fn default_hf_gray() -> f32 { 0.0 }

#[tauri::command]
async fn add_header_footer(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: HeaderFooterArgs,
) -> Result<DocSnapshot, String> {
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let opts = pdf_mutate::HeaderFooterOptions {
            header_left: args.header_left,
            header_center: args.header_center,
            header_right: args.header_right,
            footer_left: args.footer_left,
            footer_center: args.footer_center,
            footer_right: args.footer_right,
            font_size: args.font_size,
            margin_pt: args.margin_pt,
            first_page_number: args.first_page_number,
            gray: args.gray,
        };
        mdoc.add_header_footer(&opts).map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_header_footer: {:?}", t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct ImageWatermarkArgs {
    image_path: String,
    #[serde(default = "default_iw_scale")]
    scale_frac: f32,
    #[serde(default)]
    rotation_deg: f32,
    #[serde(default = "default_iw_opacity")]
    opacity: f32,
    #[serde(default = "default_iw_anchor")]
    anchor: String,
    #[serde(default = "default_iw_margin")]
    margin_pt: f32,
}
fn default_iw_scale() -> f32 { 0.4 }
fn default_iw_opacity() -> f32 { 0.5 }
fn default_iw_anchor() -> String { "center".into() }
fn default_iw_margin() -> f32 { 36.0 }

fn parse_image_anchor(s: &str) -> pdf_mutate::ImageAnchor {
    match s.to_ascii_lowercase().as_str() {
        "topleft" | "top_left" | "tl" => pdf_mutate::ImageAnchor::TopLeft,
        "topright" | "top_right" | "tr" => pdf_mutate::ImageAnchor::TopRight,
        "bottomleft" | "bottom_left" | "bl" => pdf_mutate::ImageAnchor::BottomLeft,
        "bottomright" | "bottom_right" | "br" => pdf_mutate::ImageAnchor::BottomRight,
        _ => pdf_mutate::ImageAnchor::Center,
    }
}

#[tauri::command]
async fn add_image_watermark(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: ImageWatermarkArgs,
) -> Result<DocSnapshot, String> {
    let img_bytes = tokio::fs::read(&args.image_path)
        .await
        .map_err(|e| format!("read {}: {e}", args.image_path))?;
    let engine = state.engine.clone();
    let anchor = parse_image_anchor(&args.anchor);
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let opts = pdf_mutate::ImageWatermarkOptions {
            scale_frac: args.scale_frac,
            rotation_deg: args.rotation_deg,
            opacity: args.opacity,
            anchor,
            margin_pt: args.margin_pt,
        };
        mdoc.add_image_watermark(&img_bytes, &opts)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_image_watermark: {:?}", t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct AnnotationArgs {
    page: u32,
    rects: Vec<RectPt>,
    /// `"highlight"` | `"underline"` | `"strikeout"`. Default highlight.
    #[serde(default = "default_markup_kind")]
    kind: String,
    /// RGB 0..1; default yellow.
    #[serde(default = "default_markup_color")]
    color: [f32; 3],
    #[serde(default = "default_markup_opacity")]
    opacity: f32,
    #[serde(default)]
    contents: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_print_flag")]
    print: bool,
}
fn default_markup_kind() -> String { "highlight".into() }
fn default_markup_color() -> [f32; 3] { [1.0, 0.95, 0.0] }
fn default_markup_opacity() -> f32 { 0.4 }
fn default_print_flag() -> bool { true }

fn parse_markup_kind(s: &str) -> pdf_mutate::MarkupKind {
    match s.to_ascii_lowercase().as_str() {
        "underline" => pdf_mutate::MarkupKind::Underline,
        "strikeout" | "strike" | "strikethrough" => pdf_mutate::MarkupKind::StrikeOut,
        _ => pdf_mutate::MarkupKind::Highlight,
    }
}

#[tauri::command]
async fn add_annotation(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: AnnotationArgs,
) -> Result<DocSnapshot, String> {
    if args.rects.is_empty() {
        return Err("add_annotation: no rects".into());
    }
    let engine = state.engine.clone();
    let kind = parse_markup_kind(&args.kind);
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let annot = pdf_mutate::MarkupAnnotation {
            kind,
            rects: args.rects,
            color: args.color,
            opacity: args.opacity,
            contents: args.contents,
            author: args.author,
            print: args.print,
        };
        mdoc.add_markup_annotation(args.page, &annot)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_annotation page={} kind={:?}: {:?}", args.page, kind, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct ShapeArgs {
    page: u32,
    /// `"rect"` | `"circle"` | `"line"` | `"arrow"`.
    kind: String,
    /// (x0, y0, x1, y1) in PDF points, top-origin.
    rect: RectPt,
    /// RGB stroke color 0..1.
    #[serde(default = "default_shape_stroke")]
    stroke: [f32; 3],
    /// Optional fill color (rect/circle only).
    #[serde(default)]
    fill: Option<[f32; 3]>,
    #[serde(default = "default_shape_border_width")]
    border_width: f32,
    #[serde(default = "default_shape_opacity")]
    opacity: f32,
    #[serde(default)]
    contents: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_print_flag")]
    print: bool,
}
fn default_shape_stroke() -> [f32; 3] { [0.85, 0.1, 0.1] }
fn default_shape_border_width() -> f32 { 1.5 }
fn default_shape_opacity() -> f32 { 1.0 }

#[tauri::command]
async fn add_shape(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: ShapeArgs,
) -> Result<DocSnapshot, String> {
    let kind = match args.kind.to_ascii_lowercase().as_str() {
        "rect" | "rectangle" | "square" => pdf_mutate::ShapeKind::Square,
        "circle" | "ellipse" | "oval" => pdf_mutate::ShapeKind::Circle,
        "line" => pdf_mutate::ShapeKind::Line {
            arrow_start: pdf_mutate::LineEnding::None,
            arrow_end: pdf_mutate::LineEnding::None,
        },
        "arrow" => pdf_mutate::ShapeKind::Line {
            arrow_start: pdf_mutate::LineEnding::None,
            arrow_end: pdf_mutate::LineEnding::OpenArrow,
        },
        other => return Err(format!("unknown shape kind: {other}")),
    };
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let annot = pdf_mutate::ShapeAnnotation {
            kind,
            rect: args.rect,
            stroke_color: args.stroke,
            fill_color: args.fill,
            border_width: args.border_width,
            opacity: args.opacity,
            contents: args.contents,
            author: args.author,
            print: args.print,
        };
        mdoc.add_shape_annotation(args.page, &annot)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_shape page={} kind={:?}: {:?}", args.page, kind, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct FreeTextArgs {
    page: u32,
    rect: RectPt,
    text: String,
    #[serde(default = "default_freetext_color")]
    color: [f32; 3],
    #[serde(default = "default_freetext_bg")]
    bg_color: [f32; 4],
    #[serde(default = "default_freetext_size")]
    font_size: f32,
    #[serde(default = "default_freetext_font")]
    font_family: String,
    #[serde(default)]
    rotation_quadrant: u8,
    #[serde(default = "default_shape_opacity")]
    opacity: f32,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_print_flag")]
    print: bool,
}
fn default_freetext_color() -> [f32; 3] { [0.0, 0.0, 0.0] }
fn default_freetext_size() -> f32 { 14.0 }
fn default_freetext_bg() -> [f32; 4] { [1.0, 1.0, 1.0, 1.0] }
fn default_freetext_font() -> String { "Helv".to_string() }

#[tauri::command]
async fn add_freetext(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: FreeTextArgs,
) -> Result<DocSnapshot, String> {
    if args.text.trim().is_empty() {
        return Err("freetext: empty text".into());
    }
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let annot = pdf_mutate::FreeTextAnnotation {
            rect: args.rect,
            text: args.text,
            color: args.color,
            bg_color: args.bg_color,
            font_size: args.font_size,
            font_family: args.font_family,
            rotation_quadrant: args.rotation_quadrant,
            opacity: args.opacity,
            author: args.author,
            print: args.print,
        };
        mdoc.add_freetext_annotation(args.page, &annot)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_freetext page={}: {:?}", args.page, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct InkArgs {
    page: u32,
    /// Each stroke is a flat list of x,y pairs in PDF points (top-origin).
    strokes: Vec<Vec<f32>>,
    #[serde(default = "default_ink_color")]
    color: [f32; 3],
    #[serde(default = "default_ink_width")]
    width: f32,
    #[serde(default = "default_shape_opacity")]
    opacity: f32,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    contents: Option<String>,
    #[serde(default = "default_print_flag")]
    print: bool,
}
fn default_ink_color() -> [f32; 3] { [0.95, 0.1, 0.1] }
fn default_ink_width() -> f32 { 1.6 }

#[tauri::command]
async fn add_ink(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: InkArgs,
) -> Result<DocSnapshot, String> {
    let strokes: Vec<Vec<(f32, f32)>> = args
        .strokes
        .into_iter()
        .map(|flat| {
            let mut s: Vec<(f32, f32)> = Vec::with_capacity(flat.len() / 2);
            for chunk in flat.chunks(2) {
                if chunk.len() == 2 {
                    s.push((chunk[0], chunk[1]));
                }
            }
            s
        })
        .filter(|s| s.len() >= 2)
        .collect();
    if strokes.is_empty() {
        return Err("ink: no usable strokes".into());
    }
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let annot = pdf_mutate::InkAnnotation {
            strokes,
            color: args.color,
            width: args.width,
            opacity: args.opacity,
            contents: args.contents,
            author: args.author,
            print: args.print,
        };
        mdoc.add_ink_annotation(args.page, &annot)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_ink page={}: {:?}", args.page, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct StampArgs {
    page: u32,
    rect: RectPt,
    image_path: String,
    #[serde(default = "default_shape_opacity")]
    opacity: f32,
    #[serde(default)]
    contents: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_print_flag")]
    print: bool,
}

#[tauri::command]
async fn add_stamp(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: StampArgs,
) -> Result<DocSnapshot, String> {
    let img_bytes = tokio::fs::read(&args.image_path)
        .await
        .map_err(|e| format!("read {}: {e}", args.image_path))?;
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let annot = pdf_mutate::StampAnnotation {
            rect: args.rect,
            image: img_bytes,
            opacity: args.opacity,
            contents: args.contents,
            author: args.author,
            print: args.print,
        };
        mdoc.add_stamp_annotation(args.page, &annot)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_stamp page={}: {:?}", args.page, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Deserialize)]
struct TextNoteArgs {
    page: u32,
    /// Anchor point (top-origin, PDF points) — top-left of the icon.
    x_pt: f32,
    y_pt: f32,
    contents: String,
    #[serde(default = "default_textnote_color")]
    color: [f32; 3],
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_print_flag")]
    print: bool,
}
fn default_textnote_color() -> [f32; 3] { [1.0, 0.93, 0.25] }

#[tauri::command]
async fn add_text_note(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: TextNoteArgs,
) -> Result<DocSnapshot, String> {
    if args.contents.trim().is_empty() {
        return Err("text note: empty contents".into());
    }
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let t = std::time::Instant::now();
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let annot = pdf_mutate::TextNoteAnnotation {
            x_pt: args.x_pt,
            y_pt: args.y_pt,
            contents: args.contents,
            color: args.color,
            icon: args.icon,
            author: args.author,
            print: args.print,
        };
        mdoc.add_text_note_annotation(args.page, &annot)
            .map_err(|e| e.to_string())?;
        let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
        tracing::info!("add_text_note page={}: {:?}", args.page, t.elapsed());
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[tauri::command]
async fn list_form_fields(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<Vec<FormFieldInfo>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.list_form_fields(document).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_form_field(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    name: String,
    value: String,
) -> Result<Option<FormFieldInfo>, String> {
    let engine = state.engine.clone();
    let name_for_mutate = name.clone();
    let (pre_bytes, new_bytes) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
            let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
            let mut mdoc =
                pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
            mdoc.set_form_field(&name_for_mutate, &value).map_err(|e| e.to_string())?;
            let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
            Ok((bytes, out))
        })
        .await
        .map_err(|e| e.to_string())??;
    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    // Return only the updated field. The UI keeps its current page bitmap and
    // patches the in-memory form state in place — no full document reload,
    // no image re-render flash.
    let engine = state.engine.clone();
    let updated = tokio::task::spawn_blocking(move || {
        engine
            .list_form_fields(document)
            .map(|fields| fields.into_iter().find(|f| f.id == name))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(updated)
}

#[tauri::command]
async fn reset_form(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<DocSnapshot, String> {
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
            let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
            let mut mdoc =
                pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
            mdoc.reset_form().map_err(|e| e.to_string())?;
            let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
            Ok((bytes, out))
        })
        .await
        .map_err(|e| e.to_string())??;
    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[tauri::command]
async fn list_annotations(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<Vec<AnnotationInfo>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<Vec<AnnotationInfo>, String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        Ok(mdoc.list_annotations())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_annotations_xfdf(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    path: String,
) -> Result<usize, String> {
    let engine = state.engine.clone();
    let xml = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
        mdoc.export_xfdf(None).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let len = xml.len();
    tokio::fs::write(&path, &xml)
        .await
        .map_err(|e| format!("write {path}: {e}"))?;
    Ok(len)
}

#[tauri::command]
async fn import_annotations_xfdf(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    path: String,
) -> Result<DocSnapshot, String> {
    let xml = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("read {path}: {e}"))?;
    let engine = state.engine.clone();
    let (pre_bytes, new_bytes, count) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>, usize), String> {
            let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
            let mut mdoc =
                pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
            let n = mdoc.import_xfdf(&xml).map_err(|e| e.to_string())?;
            let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
            Ok((bytes, out, n))
        })
        .await
        .map_err(|e| e.to_string())??;
    if count == 0 {
        return Err("xfdf: no importable annotations".into());
    }
    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[derive(Serialize)]
struct GhostscriptInfo {
    available: bool,
    path: Option<String>,
    version: Option<String>,
}

fn ghostscript_candidates() -> Vec<std::path::PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("GHOSTSCRIPT") {
        v.push(PathBuf::from(p));
    }
    #[cfg(target_os = "windows")]
    {
        v.push(PathBuf::from("gswin64c.exe"));
        v.push(PathBuf::from("gswin32c.exe"));
        v.push(PathBuf::from("gswin64c"));
        for base in [r"C:\Program Files\gs", r"C:\Program Files (x86)\gs"] {
            if let Ok(dir) = std::fs::read_dir(base) {
                for entry in dir.flatten() {
                    let p = entry.path().join("bin");
                    let exe64 = p.join("gswin64c.exe");
                    if exe64.exists() { v.push(exe64); }
                    let exe32 = p.join("gswin32c.exe");
                    if exe32.exists() { v.push(exe32); }
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        v.push(PathBuf::from("gs"));
    }
    v
}

fn probe_ghostscript(path: &Path) -> Option<(String, String)> {
    use std::process::Command;
    let out = Command::new(path).arg("--version").output().ok()?;
    if !out.status.success() { return None; }
    let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if ver.is_empty() { return None; }
    Some((path.to_string_lossy().to_string(), ver))
}

pub(crate) fn find_ghostscript() -> Option<String> {
    for c in ghostscript_candidates() {
        if let Some((p, _)) = probe_ghostscript(&c) {
            return Some(p);
        }
    }
    None
}

pub(crate) fn probe_ghostscript_version(path: &Path) -> Option<String> {
    probe_ghostscript(path).map(|(_, v)| v)
}

#[tauri::command]
async fn check_ghostscript() -> GhostscriptInfo {
    tokio::task::spawn_blocking(|| {
        for c in ghostscript_candidates() {
            if let Some((p, v)) = probe_ghostscript(&c) {
                return GhostscriptInfo { available: true, path: Some(p), version: Some(v) };
            }
        }
        GhostscriptInfo { available: false, path: None, version: None }
    })
    .await
    .unwrap_or(GhostscriptInfo { available: false, path: None, version: None })
}

#[derive(Deserialize)]
struct PdfaArgs {
    path: String,
    level: String, // "1b" | "2b" | "3b" | "4"
}

pub(crate) fn pdfa_flags_for(level: &str) -> Result<(&'static str, &'static str), String> {
    pdfa_flags(level)
}

fn pdfa_flags(level: &str) -> Result<(&'static str, &'static str), String> {
    // (PDFA value, compat label)
    match level {
        "1b" | "1B" | "1" => Ok(("1", "PDF/A-1b")),
        "2b" | "2B" | "2" => Ok(("2", "PDF/A-2b")),
        "3b" | "3B" | "3" => Ok(("3", "PDF/A-3b")),
        "4"              => Ok(("4", "PDF/A-4")),
        other => Err(format!("unsupported PDF/A level: {other}")),
    }
}

#[tauri::command]
async fn save_as_pdfa(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: PdfaArgs,
) -> Result<String, String> {
    let (pdfa_val, compat_label) = pdfa_flags(&args.level)?;
    let info = check_ghostscript().await;
    let gs_path = info.path.ok_or_else(|| {
        "Ghostscript not found. Install it from https://ghostscript.com/releases/gsdnld.html".to_string()
    })?;

    let engine = state.engine.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        engine.document_bytes(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let out_path = args.path.clone();
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        use std::io::Write;
        let t = std::time::Instant::now();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let src = std::env::temp_dir().join(format!("rustpdf-gs-src-{ts}.pdf"));
        {
            let mut f = std::fs::File::create(&src)
                .map_err(|e| format!("temp create {}: {e}", src.display()))?;
            f.write_all(&bytes).map_err(|e| format!("temp write: {e}"))?;
        }

        let mut cmd = std::process::Command::new(&gs_path);
        cmd.args([
            "-dBATCH",
            "-dNOPAUSE",
            "-dQUIET",
            "-sDEVICE=pdfwrite",
        ]);
        cmd.arg(format!("-dPDFA={pdfa_val}"));
        cmd.arg("-dPDFACompatibilityPolicy=1");
        cmd.arg("-sColorConversionStrategy=RGB");
        cmd.arg("-sProcessColorModel=DeviceRGB");
        cmd.arg(format!("-sOutputFile={out_path}"));
        cmd.arg(&src);

        let output = cmd.output().map_err(|e| format!("gs spawn: {e}"))?;
        let _ = std::fs::remove_file(&src);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("gs failed: {}", stderr.trim()));
        }
        tracing::info!("save_as_pdfa level={compat_label} in {:?}", t.elapsed());
        Ok(compat_label.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct ConvertResult {
    files: Vec<String>,
    failed: Vec<String>,
}

#[tauri::command]
async fn convert_files_to_pdfa(
    paths: Vec<String>,
    out_dir: String,
    level: String,
) -> Result<ConvertResult, String> {
    if paths.is_empty() {
        return Err("convert_files_to_pdfa: no input files".into());
    }
    let (pdfa_val, compat_label) = pdfa_flags(&level)?;
    let info = check_ghostscript().await;
    let gs_path = info.path.ok_or_else(|| {
        "Ghostscript not found. Install it from https://ghostscript.com/releases/gsdnld.html"
            .to_string()
    })?;
    let dir_str = out_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<ConvertResult, String> {
        let t = std::time::Instant::now();
        let dir = PathBuf::from(&dir_str);
        std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {dir_str}: {e}"))?;
        let mut written: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for p in paths {
            let src = PathBuf::from(&p);
            let stem = src
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("converted");
            let out = dir.join(format!("{stem}_pdfa-{level}.pdf"));
            let mut cmd = std::process::Command::new(&gs_path);
            cmd.args([
                "-dBATCH",
                "-dNOPAUSE",
                "-dQUIET",
                "-sDEVICE=pdfwrite",
            ]);
            cmd.arg(format!("-dPDFA={pdfa_val}"));
            cmd.arg("-dPDFACompatibilityPolicy=1");
            cmd.arg("-sColorConversionStrategy=RGB");
            cmd.arg("-sProcessColorModel=DeviceRGB");
            cmd.arg(format!("-sOutputFile={}", out.display()));
            cmd.arg(&src);
            match cmd.output() {
                Ok(o) if o.status.success() => {
                    written.push(out.to_string_lossy().to_string());
                }
                Ok(o) => {
                    tracing::warn!(
                        "convert_files_to_pdfa: {} failed: {}",
                        p,
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                    failed.push(p);
                }
                Err(e) => {
                    tracing::warn!("convert_files_to_pdfa: spawn {p}: {e}");
                    failed.push(p);
                }
            }
        }
        tracing::info!(
            "convert_files_to_pdfa: {} ok / {} failed → {} ({}) in {:?}",
            written.len(),
            failed.len(),
            dir_str,
            compat_label,
            t.elapsed()
        );
        Ok(ConvertResult { files: written, failed })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct UndoStatus {
    undo: usize,
    redo: usize,
}

#[tauri::command]
fn undo_status(state: tauri::State<'_, AppState>, document: DocumentId) -> UndoStatus {
    let (u, r) = state.undo.lock().lengths(document);
    UndoStatus { undo: u, redo: r }
}

async fn apply_swap(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    direction_redo: bool,
) -> Result<Option<DocSnapshot>, String> {
    let engine = state.engine.clone();
    let current = tokio::task::spawn_blocking(move || {
        engine.document_bytes(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let target = {
        let mut g = state.undo.lock();
        if direction_redo {
            g.pop_redo(document, current)
        } else {
            g.pop_undo(document, current)
        }
    };
    let Some(target) = target else {
        return Ok(None);
    };

    state.cache.lock().remove_document(document);

    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .reload_bytes(document, target)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    snapshot_doc(state, document).await.map(Some)
}

#[tauri::command]
async fn undo_doc(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<Option<DocSnapshot>, String> {
    apply_swap(state, document, false).await
}

#[tauri::command]
async fn redo_doc(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<Option<DocSnapshot>, String> {
    apply_swap(state, document, true).await
}

#[tauri::command]
/// Show the native OS print dialog and print the document. Returns `true` if
/// the user printed, `false` if they cancelled.
async fn print_document(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<bool, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.print_with_dialog(document).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn document_metadata_cmd(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<DocMetadata, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine.document_metadata(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn document_permissions_cmd(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<DocPermissions, String> {
    // The resident engine copy of a protected document is plaintext
    // (decrypted on open), so pdfium would report "Unprotected" with all
    // permissions granted. Report the tracked protection settings instead —
    // they are what ends up in the saved file.
    if let Some(c) = state.doc_crypto.lock().get(&document) {
        let p = c.permissions;
        return Ok(DocPermissions {
            security_handler: "AES-256 (applied on save)".to_string(),
            can_print_high_quality: p.print,
            can_print_low_quality: p.print,
            can_modify_content: p.modify,
            can_extract_text_and_graphics: p.copy,
            can_annotate: p.annotate,
            can_fill_forms: p.annotate,
            can_assemble: p.modify,
        });
    }
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine.document_permissions(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Color the native Windows title bar to match the app theme (Windows 11+,
/// DWM caption attributes; silently ignored on older Windows).
#[cfg(windows)]
#[tauri::command]
fn set_titlebar_theme(window: tauri::WebviewWindow, dark: bool) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    // COLORREF is 0x00BBGGRR. Values mirror the CSS vars in template.ts:
    // dark --bg #2b2b2c / --text #e4e4e4, light --bg #ffffff / --text #2c2e33.
    let (caption, text): (u32, u32) = if dark {
        (0x002C2B2B, 0x00E4E4E4)
    } else {
        (0x00FFFFFF, 0x00332E2C)
    };
    let dark_flag: i32 = if dark { 1 } else { 0 };
    unsafe {
        // Dark-mode hint first so hover/inactive states follow suit; the
        // explicit caption color is what actually matches the ribbon.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_flag as *const i32 as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        )
        .map_err(|e| format!("caption color: {e}"))?;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
fn set_titlebar_theme(_window: tauri::WebviewWindow, _dark: bool) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn open_external(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    let allowed = ["http://", "https://", "mailto:"];
    if !allowed.iter().any(|p| lower.starts_with(p)) {
        return Err(format!("scheme not allowed: {trimmed}"));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(trimmed, None::<&str>)
        .map_err(|e| e.to_string())
}

// ----------------- Saved signatures (Fill & Sign) -----------------

/// Reusable signatures (drawn or imported) live as PNG files under the app
/// config dir so they survive restarts and can be re-stamped onto any PDF.
fn signatures_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("signatures");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn sanitize_signature_kind(kind: &str) -> String {
    kind.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

#[derive(serde::Serialize)]
struct SignatureEntry {
    kind: String,
    path: String,
}

#[tauri::command]
fn save_signature(app: tauri::AppHandle, kind: String, data: Vec<u8>) -> Result<String, String> {
    let safe = sanitize_signature_kind(&kind);
    if safe.is_empty() {
        return Err("invalid signature name".into());
    }
    let path = signatures_dir(&app)?.join(format!("{safe}.png"));
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn list_signatures(app: tauri::AppHandle) -> Result<Vec<SignatureEntry>, String> {
    let dir = signatures_dir(&app)?;
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("png") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(SignatureEntry {
                        kind: stem.to_string(),
                        path: p.to_string_lossy().into_owned(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind));
    Ok(out)
}

#[tauri::command]
fn import_signature(
    app: tauri::AppHandle,
    kind: String,
    src_path: String,
) -> Result<String, String> {
    let safe = sanitize_signature_kind(&kind);
    if safe.is_empty() {
        return Err("invalid signature name".into());
    }
    // Copy the raw bytes verbatim; the stamp decoder sniffs the format by
    // content, so a non-PNG source keeps working (and PNG keeps its alpha).
    let dest = signatures_dir(&app)?.join(format!("{safe}.png"));
    std::fs::copy(&src_path, &dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}

#[tauri::command]
fn delete_signature(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    let safe = sanitize_signature_kind(&kind);
    if safe.is_empty() {
        return Err("invalid signature name".into());
    }
    let path = signatures_dir(&app)?.join(format!("{safe}.png"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ----------------- Digital signatures (PAdES) -----------------

#[derive(Deserialize, Default)]
struct DigitalSignMeta {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    contact: Option<String>,
    /// Optional RFC 3161 timestamp authority URL (PAdES-B-T).
    #[serde(default)]
    tsa_url: Option<String>,
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format a UTC PDF date string `D:YYYYMMDDHHmmSS+00'00'` from epoch seconds.
fn pdf_utc_date(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let sod = epoch.rem_euclid(86_400);
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("D:{year:04}{m:02}{d:02}{h:02}{mi:02}{s:02}+00'00'")
}

fn sign_options(meta: DigitalSignMeta, epoch: i64) -> pdf_sign::SignOptions {
    pdf_sign::SignOptions {
        reason: meta.reason,
        name: meta.name,
        location: meta.location,
        contact: meta.contact,
        pdf_date: pdf_utc_date(epoch),
        signing_time_epoch: epoch,
        ..Default::default()
    }
}

async fn apply_signed_bytes(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    pre_bytes: Vec<u8>,
    new_bytes: Vec<u8>,
) -> Result<DocSnapshot, String> {
    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    snapshot_doc(state, document).await
}

#[tauri::command]
async fn sign_pdf_pkcs12(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    pfx_path: String,
    password: String,
    meta: DigitalSignMeta,
) -> Result<DocSnapshot, String> {
    if state.doc_crypto.lock().contains_key(&document) {
        return Err("PROTECTED_DOC".into());
    }
    let pfx = tokio::fs::read(&pfx_path)
        .await
        .map_err(|e| format!("read {pfx_path}: {e}"))?;
    let engine = state.engine.clone();
    let epoch = now_epoch();
    let (pre, new) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let signer = pdf_sign::load_pkcs12(&pfx, &password).map_err(|e| e.to_string())?;
        let tsa = meta.tsa_url.clone().filter(|u| !u.is_empty()).map(pdf_sign::TsaClient::new);
        let opt = sign_options(meta, epoch);
        let out = pdf_sign::sign_pdf(
            &bytes,
            &signer,
            &opt,
            tsa.as_ref().map(|t| t as &dyn pdf_sign::TimestampClient),
        )
        .map_err(|e| e.to_string())?;
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;
    apply_signed_bytes(state, document, pre, new).await
}

#[tauri::command]
async fn verify_signatures(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<Vec<pdf_sign::SignatureReport>, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        pdf_sign::verify(&bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
#[tauri::command]
fn list_windows_certificates() -> Result<Vec<pdf_sign::windows::WinCertEntry>, String> {
    pdf_sign::windows::list_certificates().map_err(|e| e.to_string())
}

#[cfg(not(windows))]
#[tauri::command]
fn list_windows_certificates() -> Result<Vec<serde_json::Value>, String> {
    Err("Windows certificate store is only available on Windows".into())
}

#[cfg(windows)]
#[tauri::command]
async fn sign_pdf_windows_cert(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    thumbprint: String,
    meta: DigitalSignMeta,
) -> Result<DocSnapshot, String> {
    if state.doc_crypto.lock().contains_key(&document) {
        return Err("PROTECTED_DOC".into());
    }
    let engine = state.engine.clone();
    let epoch = now_epoch();
    let (pre, new) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), String> {
        let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
        let signer = pdf_sign::windows::WindowsSigner::from_thumbprint(&thumbprint)
            .map_err(|e| e.to_string())?;
        let tsa = meta.tsa_url.clone().filter(|u| !u.is_empty()).map(pdf_sign::TsaClient::new);
        let opt = sign_options(meta, epoch);
        let out = pdf_sign::sign_pdf(
            &bytes,
            &signer,
            &opt,
            tsa.as_ref().map(|t| t as &dyn pdf_sign::TimestampClient),
        )
        .map_err(|e| e.to_string())?;
        Ok((bytes, out))
    })
    .await
    .map_err(|e| e.to_string())??;
    apply_signed_bytes(state, document, pre, new).await
}

#[cfg(not(windows))]
#[tauri::command]
async fn sign_pdf_windows_cert(
    _state: tauri::State<'_, AppState>,
    _document: DocumentId,
    _thumbprint: String,
    _meta: DigitalSignMeta,
) -> Result<DocSnapshot, String> {
    Err("Windows certificate store is only available on Windows".into())
}

#[tauri::command]
async fn search_document(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    term: String,
    case_sensitive: bool,
) -> Result<Vec<SearchHit>, String> {
    let engine = state.engine.clone();
    let query = SearchQuery {
        term,
        case_sensitive,
        whole_word: false,
    };
    tokio::task::spawn_blocking(move || {
        engine.search(document, &query).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn encode_jpeg(width: u32, height: u32, rgba: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    // mozjpeg (libjpeg-turbo SIMD). PDFium writes opaque RGBA — feed JCS_EXT_RGBA
    // directly so the encoder drops alpha without an explicit compositing pass.
    let t_jpg = std::time::Instant::now();
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGBA);
    // JCP_FASTEST profile: baseline (no progressive), no trellis, no scan
    // optimization, no Huffman 2nd pass. ~2-3× faster than default at a
    // small filesize cost.
    comp.set_fastest_defaults();
    comp.set_optimize_coding(false);
    comp.set_size(width as usize, height as usize);
    comp.set_quality(quality as f32);
    let mut started = comp.start_compress(Vec::with_capacity(rgba.len() / 10))
        .map_err(|e| e.to_string())?;
    started.write_scanlines(rgba).map_err(|e| e.to_string())?;
    let out = started.finish().map_err(|e| e.to_string())?;
    tracing::info!(
        "encode_jpeg {}x{} q={} jpeg={:?} out={}KB",
        width, height, quality, t_jpg.elapsed(), out.len() / 1024
    );
    Ok(out)
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedSettings {
    recent_files: VecDeque<RecentFile>,
    /// `"off" | "1b" | "2b" | "3b" | "4"`. Defaults to `"off"` (plain PDF).
    #[serde(default)]
    default_pdfa_level: Option<String>,
}

fn load_settings(path: &Path) -> PersistedSettings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_settings(path: &Path, s: &PersistedSettings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(s)?;
    std::fs::write(path, data)
}

fn pdfa_pref_from_settings(s: &PersistedSettings) -> Option<String> {
    match s.default_pdfa_level.as_deref() {
        Some(v) if v != "off" && !v.is_empty() => Some(v.to_string()),
        _ => None,
    }
}

#[tauri::command]
fn get_default_pdfa(state: tauri::State<'_, AppState>) -> String {
    let guard = state.settings_path.lock();
    if let Some(path) = guard.as_ref() {
        let s = load_settings(path);
        return s.default_pdfa_level.unwrap_or_else(|| "off".to_string());
    }
    "off".to_string()
}

#[tauri::command]
fn set_default_pdfa(state: tauri::State<'_, AppState>, level: String) -> Result<(), String> {
    let allowed = ["off", "1b", "2b", "3b", "4"];
    if !allowed.contains(&level.as_str()) {
        return Err(format!("invalid pdfa level: {level}"));
    }
    let guard = state.settings_path.lock();
    let Some(cfg) = guard.as_ref() else {
        return Err("settings path unavailable".into());
    };
    let mut s = load_settings(cfg);
    s.default_pdfa_level = Some(level.clone());
    save_settings(cfg, &s).map_err(|e| e.to_string())?;
    vprinter::set_pdfa_default(pdfa_pref_from_settings(&s));
    Ok(())
}

#[tauri::command]
fn recent_files(state: tauri::State<'_, AppState>) -> Vec<RecentFile> {
    let guard = state.settings_path.lock();
    let Some(path) = guard.as_ref() else {
        return Vec::new();
    };
    load_settings(path).recent_files.into_iter().collect()
}

#[tauri::command]
fn push_recent(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let guard = state.settings_path.lock();
    let Some(cfg) = guard.as_ref() else {
        return Err("settings path unavailable".into());
    };
    let mut s = load_settings(cfg);
    let pb = PathBuf::from(&path);
    s.recent_files.retain(|e| e.path != pb);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    s.recent_files.push_front(RecentFile {
        path: pb,
        opened_at_unix: now,
    });
    while s.recent_files.len() > 20 {
        s.recent_files.pop_back();
    }
    save_settings(cfg, &s).map_err(|e| e.to_string())
}

pub fn run() {
    // Default to INFO so open_pdf / render timings show. Override via RUST_LOG.
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,desktop_tauri_lib=info,pdf_pdfium=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init()
        .ok();

    let engine: Arc<dyn PdfEngine> = match PdfiumEngine::new() {
        Ok(e) => Arc::new(e),
        Err(e) => {
            tracing::error!("pdfium unavailable at startup: {e}");
            Arc::new(NullEngine)
        }
    };

    let cache = RenderCache::new(NonZeroUsize::new(64).unwrap());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings_path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join("settings.json"));
            if let Some(path) = settings_path.as_ref() {
                let s = load_settings(path);
                vprinter::set_pdfa_default(pdfa_pref_from_settings(&s));
            }
            let state: tauri::State<'_, AppState> = app.state();
            *state.settings_path.lock() = settings_path;
            vprinter::start_spool_watcher(app.handle().clone());
            Ok(())
        })
        .manage(AppState {
            engine,
            cache: Mutex::new(cache),
            settings_path: Mutex::new(None),
            undo: Mutex::new(UndoStacks::default()),
            doc_crypto: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            open_pdf,
            open_pdf_path,
            close_pdf,
            render_page,
            render_thumbnail,
            page_text_layout_cmd,
            search_document,
            page_links_cmd,
            open_external,
            list_attachments_cmd,
            save_attachment_cmd,
            document_metadata_cmd,
            document_permissions_cmd,
            print_document,
            save_pdf_as,
            get_pdf_security,
            set_pdf_protection,
            remove_pdf_protection,
            set_titlebar_theme,
            delete_page,
            delete_pages,
            rotate_pages,
            reorder_pages,
            extract_pages,
            save_region,
            insert_pdf_files,
            merge_files_and_open,
            split_pdf,
            add_watermark,
            add_header_footer,
            add_image_watermark,
            add_annotation,
            add_shape,
            add_freetext,
            add_ink,
            add_stamp,
            add_text_note,
            save_signature,
            list_signatures,
            import_signature,
            delete_signature,
            sign_pdf_pkcs12,
            verify_signatures,
            list_windows_certificates,
            sign_pdf_windows_cert,
            list_form_fields,
            set_form_field,
            reset_form,
            list_annotations,
            export_annotations_xfdf,
            import_annotations_xfdf,
            check_ghostscript,
            save_as_pdfa,
            convert_files_to_pdfa,
            vprinter::virtual_printer_status,
            vprinter::install_virtual_printer,
            vprinter::uninstall_virtual_printer,
            vprinter::open_print_output_folder,
            undo_doc,
            redo_doc,
            undo_status,
            recent_files,
            push_recent,
            get_default_pdfa,
            set_default_pdfa
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

struct NullEngine;

impl PdfEngine for NullEngine {
    fn open_bytes_with_password(
        &self,
        _: Vec<u8>,
        _: Option<String>,
    ) -> pdf_core::CoreResult<DocumentId> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn close(&self, _: DocumentId) -> pdf_core::CoreResult<()> {
        Ok(())
    }
    fn page_count(&self, _: DocumentId) -> pdf_core::CoreResult<u32> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn page_size(&self, _: DocumentId, _: PageIndex) -> pdf_core::CoreResult<PageSize> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn render(&self, _: pdf_core::RenderRequest) -> pdf_core::CoreResult<pdf_core::RenderedPage> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn extract_text(&self, _: DocumentId, _: PageIndex) -> pdf_core::CoreResult<String> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn page_text_layout(&self, _: DocumentId, _: PageIndex) -> pdf_core::CoreResult<Vec<TextRun>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn outline(&self, _: DocumentId) -> pdf_core::CoreResult<Vec<OutlineNode>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn search(&self, _: DocumentId, _: &SearchQuery) -> pdf_core::CoreResult<Vec<SearchHit>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn page_links(&self, _: DocumentId, _: PageIndex) -> pdf_core::CoreResult<Vec<PageLink>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn list_attachments(&self, _: DocumentId) -> pdf_core::CoreResult<Vec<AttachmentInfo>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn read_attachment(&self, _: DocumentId, _: u32) -> pdf_core::CoreResult<Vec<u8>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn document_metadata(&self, _: DocumentId) -> pdf_core::CoreResult<DocMetadata> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn document_permissions(&self, _: DocumentId) -> pdf_core::CoreResult<DocPermissions> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn document_bytes(&self, _: DocumentId) -> pdf_core::CoreResult<Vec<u8>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn reload_bytes(&self, _: DocumentId, _: Vec<u8>) -> pdf_core::CoreResult<()> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
    fn list_form_fields(
        &self,
        _: DocumentId,
    ) -> pdf_core::CoreResult<Vec<pdf_types::FormFieldInfo>> {
        Err(pdf_core::CoreError::Engine("pdfium not loaded".into()))
    }
}
