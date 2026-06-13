//! Docura PDF — Slint reader (test-slint branch).
//!
//! A functional, virtualised continuous-scroll PDF reader built to compare
//! against the Tauri + WebView2 frontend on open speed, CPU and RAM. It calls
//! `pdf_core`/`pdf_pdfium` directly (no IPC, no webview). Pages are laid out
//! at their real size up front (cheap `page_size` calls) and their bitmaps are
//! rendered lazily on worker threads as they scroll into view.
//!
//! Benchmark mode: pass a PDF path as the first CLI argument to open it on
//! startup and print timing to stdout, e.g.
//!
//!     cargo run --release -p desktop-slint -- C:\path\large-doc.pdf
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{DocumentId, PageIndex, Rotation, ZoomLevel};
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

slint::include_modules!();

/// Print a benchmark line and append it to a log file (release builds detach
/// stdout, so the file is the reliable channel).
fn bench_log(msg: String) {
    println!("{msg}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("docura-slint-bench.log"))
    {
        use std::io::Write;
        let _ = writeln!(f, "{msg}");
    }
}

const ZOOM_MIN: f32 = 0.5;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_STEP: f32 = 1.25;
const DEFAULT_ZOOM: f32 = 1.5;

struct AppState {
    engine: Arc<dyn PdfEngine>,
    doc: Option<DocumentId>,
    /// Per-page size in PDF points (width, height) — the layout source.
    page_pt: Vec<(f32, f32)>,
    zoom: f32,
    /// Pages already handed to a render thread (avoids re-rendering on every
    /// scroll-into-view).
    requested: HashSet<usize>,
    /// Set when a benchmark file is opened, to time first-page render.
    open_start: Option<Instant>,
    first_render_logged: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let engine = PdfiumEngine::new().expect("bind pdfium (vendor/pdfium/pdfium.dll)");

    let state = Rc::new(RefCell::new(AppState {
        engine: Arc::new(engine),
        doc: None,
        page_pt: Vec::new(),
        zoom: DEFAULT_ZOOM,
        requested: HashSet::new(),
        open_start: None,
        first_render_logged: false,
    }));

    let pages: Rc<VecModel<PageRow>> = Rc::new(VecModel::default());
    ui.set_pages(ModelRc::from(pages.clone()));

    ui.on_open_clicked({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            let ui = ui.unwrap();
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_title("Open PDF")
                .pick_file()
            else {
                return;
            };
            open_path(&state, &pages, &ui, &path);
        }
    });

    ui.on_zoom_in({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            let z = {
                let mut s = state.borrow_mut();
                s.zoom = (s.zoom * ZOOM_STEP).min(ZOOM_MAX);
                s.zoom
            };
            rebuild_rows(&state, &pages, z);
            update_info(&state, &ui.unwrap());
        }
    });

    ui.on_zoom_out({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            let z = {
                let mut s = state.borrow_mut();
                s.zoom = (s.zoom / ZOOM_STEP).max(ZOOM_MIN);
                s.zoom
            };
            rebuild_rows(&state, &pages, z);
            update_info(&state, &ui.unwrap());
        }
    });

    ui.on_request_render({
        let ui = ui.as_weak();
        let state = state.clone();
        move |i| {
            let i = i as usize;
            let (engine, doc, zoom, bench) = {
                let mut s = state.borrow_mut();
                if s.requested.contains(&i) {
                    return;
                }
                let Some(doc) = s.doc else { return };
                s.requested.insert(i);
                // Time to first painted page (whichever delegate realises first).
                let bench = if !s.first_render_logged {
                    s.first_render_logged = true;
                    s.open_start
                } else {
                    None
                };
                (s.engine.clone(), doc, s.zoom, bench)
            };
            let ui_weak = ui.clone();
            // Render off the UI thread; pdfium serialises internally.
            std::thread::spawn(move || {
                let req = RenderRequest {
                    document: doc,
                    page: PageIndex(i as u32),
                    zoom: ZoomLevel(zoom),
                    rotation: Rotation::R0,
                };
                let Ok(r) = engine.render(req) else { return };
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui_weak.upgrade() else { return };
                    let model = ui.get_pages();
                    let Some(vm) = model.as_any().downcast_ref::<VecModel<PageRow>>() else {
                        return;
                    };
                    let Some(mut row) = vm.row_data(i) else { return };
                    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(r.width_px, r.height_px);
                    buf.make_mut_bytes().copy_from_slice(&r.rgba);
                    row.img = Image::from_rgba8(buf);
                    vm.set_row_data(i, row);
                    if let Some(t0) = bench {
                        bench_log(format!("[bench] first page rendered: {:?}", t0.elapsed()));
                    }
                });
            });
        }
    });

    // Benchmark mode: open a file given on the command line. Safe to call
    // before `run()` — render threads post back into the loop once it starts.
    if let Some(arg) = std::env::args().nth(1) {
        let path = std::path::PathBuf::from(arg);
        open_path(&state, &pages, &ui, &path);
    }

    ui.run()?;
    Ok(())
}

/// Open a PDF: load metadata + per-page sizes (fast), reserve layout rows.
/// Bitmaps render lazily via `request-render`.
fn open_path(state: &Rc<RefCell<AppState>>, pages: &Rc<VecModel<PageRow>>, ui: &AppWindow, path: &Path) {
    let t0 = Instant::now();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            ui.set_info(format!("Read error: {e}").into());
            return;
        }
    };
    let read_done = t0.elapsed();

    let (engine, zoom) = {
        let s = state.borrow();
        (s.engine.clone(), s.zoom)
    };

    if let Some(d) = state.borrow_mut().doc.take() {
        let _ = engine.close(d);
    }

    let doc = match engine.open_bytes(bytes) {
        Ok(d) => d,
        Err(e) => {
            ui.set_info(format!("Open error: {e}").into());
            return;
        }
    };
    let count = engine.page_count(doc).unwrap_or(0);
    let mut page_pt = Vec::with_capacity(count as usize);
    for p in 0..count {
        let sz = engine.page_size(doc, PageIndex(p)).unwrap_or(pdf_types::PageSize {
            width_pt: 612.0,
            height_pt: 792.0,
        });
        page_pt.push((sz.width_pt, sz.height_pt));
    }
    let meta_done = t0.elapsed();

    {
        let mut s = state.borrow_mut();
        s.doc = Some(doc);
        s.page_pt = page_pt;
        s.requested.clear();
        s.open_start = Some(t0);
        s.first_render_logged = false;
    }
    rebuild_rows(state, pages, zoom);

    bench_log(format!(
        "[bench] {}: {} pages | read {:?} | meta(open+page_size x{}) {:?}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        count,
        read_done,
        count,
        meta_done,
    ));
    ui.set_has_doc(true);
    update_info(state, ui);
}

/// Rebuild every layout row for the current zoom (empty images), which makes
/// the visible delegates re-`init` and request fresh renders.
fn rebuild_rows(state: &Rc<RefCell<AppState>>, pages: &Rc<VecModel<PageRow>>, zoom: f32) {
    let rows: Vec<PageRow> = {
        let mut s = state.borrow_mut();
        s.requested.clear();
        s.page_pt
            .iter()
            .map(|(w, h)| PageRow {
                w: w * zoom,
                h: h * zoom,
                img: Image::default(),
            })
            .collect()
    };
    pages.set_vec(rows);
}

fn update_info(state: &Rc<RefCell<AppState>>, ui: &AppWindow) {
    let s = state.borrow();
    ui.set_info(
        format!(
            "{} pages · {}%",
            s.page_pt.len(),
            (s.zoom * 100.0).round() as i32
        )
        .into(),
    );
}
