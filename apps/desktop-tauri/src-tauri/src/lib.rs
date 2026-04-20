use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use parking_lot::Mutex;
use pdf_cache::{CacheKey, CachedImage, RenderCache};
use pdf_core::{CoreError, PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{
    AttachmentInfo, DocMetadata, DocPermissions, DocumentId, OutlineNode, PageIndex, PageLink,
    PageSize, RecentFile, Rotation, SearchHit, SearchQuery, TextRun, ZoomLevel,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;

struct AppState {
    engine: Arc<dyn PdfEngine>,
    cache: Mutex<RenderCache>,
    settings_path: Mutex<Option<PathBuf>>,
}

#[derive(Serialize)]
struct OpenedDocument {
    id: DocumentId,
    page_count: u32,
    page_sizes: Vec<PageSize>,
    outline: Vec<OutlineNode>,
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
    tokio::task::spawn_blocking(move || engine.close(document).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_pdf(
    state: tauri::State<'_, AppState>,
    bytes: Vec<u8>,
    password: Option<String>,
) -> Result<OpenedDocument, OpenError> {
    let engine = state.engine.clone();
    let size_mb = bytes.len() as f64 / 1024.0 / 1024.0;
    tokio::task::spawn_blocking(move || -> Result<OpenedDocument, OpenError> {
        let t0 = std::time::Instant::now();
        let id = engine.open_bytes_with_password(bytes, password)?;
        let t_parse = t0.elapsed();
        let page_count = engine
            .page_count(id)
            .map_err(|e| OpenError::Other(e.to_string()))?;
        let mut page_sizes = Vec::with_capacity(page_count as usize);
        for i in 0..page_count {
            let sz = engine
                .page_size(id, PageIndex(i))
                .map_err(|e| OpenError::Other(e.to_string()))?;
            page_sizes.push(sz);
        }
        let outline = engine.outline(id).unwrap_or_default();
        tracing::info!(
            "open_pdf: size={:.1}MB pages={} parse={:?} total={:?}",
            size_mb,
            page_count,
            t_parse,
            t0.elapsed()
        );
        Ok(OpenedDocument {
            id,
            page_count,
            page_sizes,
            outline,
        })
    })
    .await
    .map_err(|e| OpenError::Other(e.to_string()))?
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
    let rot_q = rotation.unwrap_or(0) % 4;
    let rotation = Rotation::from_quarter_turns(rot_q);
    let key = make_cache_key(document, page, zoom, rot_q);
    if let Some(cached) = state.cache.lock().get(&key).cloned() {
        return Ok(encode_payload(cached.width, cached.height, &cached.rgba)?);
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
        tracing::debug!("render page={} zoom={} t={:?}", page, zoom, t0.elapsed());
        r
    })
    .await
    .map_err(|e| e.to_string())??;
    state.cache.lock().insert(
        key,
        CachedImage {
            width: rendered.width_px,
            height: rendered.height_px,
            rgba: rendered.rgba.clone(),
        },
    );
    encode_payload(rendered.width_px, rendered.height_px, &rendered.rgba)
}

fn encode_payload(width: u32, height: u32, rgba: &[u8]) -> Result<RenderedPayload, String> {
    let png = encode_png(width, height, rgba)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(RenderedPayload {
        width,
        height,
        data_url: format!("data:image/png;base64,{b64}"),
    })
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
    render_page(state, document, page, zoom, rotation).await
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
async fn print_document(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<(), String> {
    let engine = state.engine.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        engine.document_bytes(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        sweep_old_print_tempfiles();
        let mut path = std::env::temp_dir();
        path.push(format!("rustpdf-print-{}.pdf", tempfile_suffix()));
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        shell_print(&path).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

static TEMPFILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn tempfile_suffix() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = TEMPFILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("{pid:x}-{t:x}-{c:x}")
}

fn sweep_old_print_tempfiles() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let now = SystemTime::now();
    let max_age = Duration::from_secs(60 * 60);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        if !(s.starts_with("rustpdf-print-") && s.ends_with(".pdf")) {
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mt| now.duration_since(mt).ok());
        if age.map(|a| a > max_age).unwrap_or(false) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(target_os = "windows")]
fn shell_print(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    let path_str = path.to_string_lossy().replace('\'', "''");
    let ps = format!(
        "Start-Process -FilePath '{}' -Verb Print -WindowStyle Hidden",
        path_str
    );
    let encoded = encode_powershell_command(&ps);
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn encode_powershell_command(cmd: &str) -> String {
    let utf16: Vec<u16> = cmd.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for u in utf16 {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}

#[cfg(not(target_os = "windows"))]
fn shell_print(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    Command::new("lp").arg(path).spawn().map(|_| ())
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
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine.document_permissions(document).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(rgba.len() / 4);
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedSettings {
    recent_files: VecDeque<RecentFile>,
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
    tracing_subscriber::fmt::try_init().ok();

    let engine: Arc<dyn PdfEngine> = match PdfiumEngine::new() {
        Ok(e) => Arc::new(e),
        Err(e) => {
            tracing::error!("pdfium unavailable at startup: {e}");
            Arc::new(NullEngine)
        }
    };

    let cache = RenderCache::new(NonZeroUsize::new(256).unwrap());

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
            let state: tauri::State<'_, AppState> = app.state();
            *state.settings_path.lock() = settings_path;
            Ok(())
        })
        .manage(AppState {
            engine,
            cache: Mutex::new(cache),
            settings_path: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            open_pdf,
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
            recent_files,
            push_recent
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
}
