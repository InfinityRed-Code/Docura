//! Docura PDF — Slint reader (test-slint branch).
//!
//! A native Slint frontend that drives `pdf_core`/`pdf_pdfium` directly (no
//! webview, no IPC), built to compare against the Tauri + WebView2 app on
//! open speed, CPU and RAM — including while features are exercised.
//!
//! Features: virtualised continuous scroll with lazy threaded rendering,
//! zoom, full-text search (hit navigation + on-page highlight) and a lazy
//! thumbnail rail.
//!
//! Benchmark mode: pass a PDF path as the first CLI argument to open it on
//! startup and log timings to stdout + `%TEMP%/docura-slint-bench.log`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{DocumentId, PageIndex, Rotation, SearchQuery, ZoomLevel};
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

slint::include_modules!();

const ZOOM_MIN: f32 = 0.5;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_STEP: f32 = 1.25;
const DEFAULT_ZOOM: f32 = 1.5;
const THUMB_ZOOM: f32 = 0.2;
const ROW_PAD: f32 = 14.0;

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

struct Hit {
    page: u32,
    rects: Vec<(f32, f32, f32, f32)>,
}

/// Send + Sync state shared with worker threads. Holds no Slint models (those
/// live on the UI thread and are reached via the window handle).
struct Shared {
    engine: Arc<dyn PdfEngine>,
    doc: Option<DocumentId>,
    page_pt: Vec<(f32, f32)>,
    /// Cumulative top edge of each page in display px (for scroll-to-page).
    offsets: Vec<f32>,
    zoom: f32,
    /// Page rotation in quarter turns (0..=3), applied to render + layout.
    rot_q: u8,
    /// Page index at the top of the viewport (tracked while scrolling).
    current_page: usize,
    requested: HashSet<usize>,
    requested_thumbs: HashSet<usize>,
    hits: Vec<Hit>,
    hit_idx: usize,
    open_start: Option<Instant>,
    first_render_logged: bool,
}

impl Shared {
    fn rotation(&self) -> Rotation {
        Rotation::from_quarter_turns(self.rot_q)
    }
    /// True when the current rotation swaps page width and height.
    fn rotated_90(&self) -> bool {
        self.rot_q % 2 == 1
    }
}

type State = Arc<Mutex<Shared>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let engine = PdfiumEngine::new().expect("bind pdfium (vendor/pdfium/pdfium.dll)");

    let state: State = Arc::new(Mutex::new(Shared {
        engine: Arc::new(engine),
        doc: None,
        page_pt: Vec::new(),
        offsets: Vec::new(),
        zoom: DEFAULT_ZOOM,
        rot_q: 0,
        current_page: 0,
        requested: HashSet::new(),
        requested_thumbs: HashSet::new(),
        hits: Vec::new(),
        hit_idx: 0,
        open_start: None,
        first_render_logged: false,
    }));

    let pages: Rc<VecModel<PageRow>> = Rc::new(VecModel::default());
    let thumbs: Rc<VecModel<Thumb>> = Rc::new(VecModel::default());
    ui.set_pages(ModelRc::from(pages.clone()));
    ui.set_thumbs(ModelRc::from(thumbs.clone()));

    ui.on_open_clicked({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        let thumbs = thumbs.clone();
        move || {
            let ui = ui.unwrap();
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_title("Open PDF")
                .pick_file()
            else {
                return;
            };
            open_path(&state, &pages, &thumbs, &ui, &path);
        }
    });

    ui.on_zoom_in({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            {
                let mut s = state.lock().unwrap();
                s.zoom = (s.zoom * ZOOM_STEP).min(ZOOM_MAX);
            }
            rebuild_rows(&state, &pages);
            update_info(&state, &ui.unwrap());
        }
    });

    ui.on_zoom_out({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            {
                let mut s = state.lock().unwrap();
                s.zoom = (s.zoom / ZOOM_STEP).max(ZOOM_MIN);
            }
            rebuild_rows(&state, &pages);
            update_info(&state, &ui.unwrap());
        }
    });

    ui.on_fit_width({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            let ui = ui.unwrap();
            fit(&state, &pages, &ui, false);
        }
    });

    ui.on_fit_page({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        move || {
            let ui = ui.unwrap();
            fit(&state, &pages, &ui, true);
        }
    });

    ui.on_rotate_left({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        let thumbs = thumbs.clone();
        move || {
            rotate(&state, &pages, &thumbs, &ui.unwrap(), 3);
        }
    });

    ui.on_rotate_right({
        let ui = ui.as_weak();
        let state = state.clone();
        let pages = pages.clone();
        let thumbs = thumbs.clone();
        move || {
            rotate(&state, &pages, &thumbs, &ui.unwrap(), 1);
        }
    });

    ui.on_goto_page({
        let ui = ui.as_weak();
        let state = state.clone();
        move |t| {
            let Ok(n) = t.trim().parse::<usize>() else { return };
            if n == 0 {
                return;
            }
            scroll_to_page(&state, &ui.unwrap(), n - 1);
        }
    });

    ui.on_toggle_thumbs({
        let ui = ui.as_weak();
        move || {
            let ui = ui.unwrap();
            ui.set_show_thumbs(!ui.get_show_thumbs());
        }
    });

    ui.on_thumb_clicked({
        let ui = ui.as_weak();
        let state = state.clone();
        move |i| {
            scroll_to_page(&state, &ui.unwrap(), i as usize);
        }
    });

    ui.on_request_render(make_render_cb(&ui, &state, RenderKind::Page));
    ui.on_request_thumb(make_render_cb(&ui, &state, RenderKind::Thumb));

    ui.on_scrolled({
        let ui = ui.as_weak();
        let state = state.clone();
        move |y| {
            let info = {
                let mut s = state.lock().unwrap();
                if s.offsets.is_empty() {
                    return;
                }
                let scroll = (-y) as f32;
                let page = s
                    .offsets
                    .partition_point(|&top| top <= scroll)
                    .saturating_sub(1);
                s.current_page = page;
                format!("page {} / {} · {}%", page + 1, s.page_pt.len(), (s.zoom * 100.0).round() as i32)
            };
            ui.unwrap().set_info(info.into());
        }
    });

    ui.on_search({
        let ui = ui.as_weak();
        let state = state.clone();
        move |term| {
            run_search(&state, &ui.unwrap(), term.to_string());
        }
    });

    ui.on_next_hit({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let next = {
                let mut s = state.lock().unwrap();
                if s.hits.is_empty() {
                    return;
                }
                s.hit_idx = (s.hit_idx + 1) % s.hits.len();
                s.hit_idx
            };
            goto_hit(&state, &ui.unwrap(), next);
        }
    });

    ui.on_prev_hit({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let prev = {
                let mut s = state.lock().unwrap();
                if s.hits.is_empty() {
                    return;
                }
                s.hit_idx = (s.hit_idx + s.hits.len() - 1) % s.hits.len();
                s.hit_idx
            };
            goto_hit(&state, &ui.unwrap(), prev);
        }
    });

    if let Some(arg) = std::env::args().nth(1) {
        let path = std::path::PathBuf::from(arg);
        open_path(&state, &pages, &thumbs, &ui, &path);
        // Optional benchmark search: second argument is the term.
        if let Some(term) = std::env::args().nth(2) {
            run_search(&state, &ui, term);
        }
    }

    ui.run()?;
    Ok(())
}

/// Full-text search on a worker thread; post hits back and jump to the first.
fn run_search(state: &State, ui: &AppWindow, term: String) {
    if term.trim().is_empty() {
        return;
    }
    let (engine, doc) = {
        let s = state.lock().unwrap();
        match s.doc {
            Some(d) => (s.engine.clone(), d),
            None => return,
        }
    };
    let ui_weak = ui.as_weak();
    let state2 = state.clone();
    ui.set_search_info("searching…".into());
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let query = SearchQuery { term, case_sensitive: false, whole_word: false };
        let hits = engine.search(doc, &query).unwrap_or_default();
        let elapsed = t0.elapsed();
        let total = hits.len();
        let mapped: Vec<Hit> = hits
            .iter()
            .map(|h| Hit {
                page: h.page.0,
                rects: h.rects_pt.iter().map(|r| (r.x0, r.y0, r.x1, r.y1)).collect(),
            })
            .collect();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            {
                let mut s = state2.lock().unwrap();
                s.hits = mapped;
                s.hit_idx = 0;
            }
            bench_log(format!("[bench] search: {total} hits in {elapsed:?}"));
            if total == 0 {
                ui.set_search_info("0 hits".into());
                ui.set_hl_page(-1);
            } else {
                goto_hit(&state2, &ui, 0);
            }
        });
    });
}

use std::rc::Rc;

enum RenderKind {
    Page,
    Thumb,
}

/// Build a lazy-render callback for either the main page list or the thumbnail
/// rail. Renders on a worker thread and posts the bitmap back to the model.
fn make_render_cb(
    ui: &AppWindow,
    state: &State,
    kind: RenderKind,
) -> impl Fn(i32) {
    let ui = ui.as_weak();
    let state = state.clone();
    let is_thumb = matches!(kind, RenderKind::Thumb);
    move |i| {
        let i = i as usize;
        let (engine, doc, zoom, rotation, bench) = {
            let mut s = state.lock().unwrap();
            let rotation = s.rotation();
            let set = if is_thumb { &mut s.requested_thumbs } else { &mut s.requested };
            if set.contains(&i) {
                return;
            }
            set.insert(i);
            let Some(doc) = s.doc else { return };
            let zoom = if is_thumb { THUMB_ZOOM } else { s.zoom };
            let bench = if !is_thumb && !s.first_render_logged {
                s.first_render_logged = true;
                s.open_start
            } else {
                None
            };
            (s.engine.clone(), doc, zoom, rotation, bench)
        };
        let ui_weak = ui.clone();
        std::thread::spawn(move || {
            let req = RenderRequest {
                document: doc,
                page: PageIndex(i as u32),
                zoom: ZoomLevel(zoom),
                rotation,
            };
            let Ok(r) = engine.render(req) else { return };
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = ui_weak.upgrade() else { return };
                let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(r.width_px, r.height_px);
                buf.make_mut_bytes().copy_from_slice(&r.rgba);
                let img = Image::from_rgba8(buf);
                if is_thumb {
                    let model = ui.get_thumbs();
                    if let Some(vm) = model.as_any().downcast_ref::<VecModel<Thumb>>() {
                        if let Some(mut row) = vm.row_data(i) {
                            row.img = img;
                            vm.set_row_data(i, row);
                        }
                    }
                } else {
                    let model = ui.get_pages();
                    if let Some(vm) = model.as_any().downcast_ref::<VecModel<PageRow>>() {
                        if let Some(mut row) = vm.row_data(i) {
                            row.img = img;
                            vm.set_row_data(i, row);
                        }
                    }
                    if let Some(t0) = bench {
                        bench_log(format!("[bench] first page rendered: {:?}", t0.elapsed()));
                    }
                }
            });
        });
    }
}

fn open_path(
    state: &State,
    pages: &Rc<VecModel<PageRow>>,
    thumbs: &Rc<VecModel<Thumb>>,
    ui: &AppWindow,
    path: &Path,
) {
    let t0 = Instant::now();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            ui.set_info(format!("Read error: {e}").into());
            return;
        }
    };
    let read_done = t0.elapsed();

    let engine = state.lock().unwrap().engine.clone();
    if let Some(d) = state.lock().unwrap().doc.take() {
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
        let mut s = state.lock().unwrap();
        s.doc = Some(doc);
        s.page_pt = page_pt;
        s.requested.clear();
        s.requested_thumbs.clear();
        s.hits.clear();
        s.hit_idx = 0;
        s.rot_q = 0;
        s.current_page = 0;
        s.open_start = Some(t0);
        s.first_render_logged = false;
    }
    rebuild_rows(state, pages);

    // Build empty thumbnail rows (rendered lazily when the rail is shown).
    let thumb_rows: Vec<Thumb> = (0..count)
        .map(|p| Thumb { img: Image::default(), label: format!("{}", p + 1).into() })
        .collect();
    thumbs.set_vec(thumb_rows);

    bench_log(format!(
        "[bench] {}: {} pages | read {:?} | meta(open+page_size x{}) {:?}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        count,
        read_done,
        count,
        meta_done,
    ));
    ui.set_has_doc(true);
    ui.set_search_info("".into());
    ui.set_hl_page(-1);
    update_info(state, ui);
}

/// Rebuild page rows + cumulative offsets for the current zoom.
fn rebuild_rows(state: &State, pages: &Rc<VecModel<PageRow>>) {
    let rows: Vec<PageRow> = {
        let mut s = state.lock().unwrap();
        s.requested.clear();
        let zoom = s.zoom;
        let swap = s.rotated_90();
        let mut offsets = Vec::with_capacity(s.page_pt.len());
        let mut top = 0.0f32;
        let rows = s
            .page_pt
            .iter()
            .map(|(w, h)| {
                // Rotation by 90°/270° swaps the displayed width and height.
                let (dw, dh) = if swap { (h, w) } else { (w, h) };
                offsets.push(top);
                let rh = dh * zoom;
                top += rh + ROW_PAD;
                PageRow { w: dw * zoom, h: rh, img: Image::default() }
            })
            .collect();
        s.offsets = offsets;
        rows
    };
    pages.set_vec(rows);
}

/// Scroll the page list so page `i` sits at the top of the viewport.
fn scroll_to_page(state: &State, ui: &AppWindow, i: usize) {
    let y = {
        let mut s = state.lock().unwrap();
        if s.offsets.is_empty() {
            return;
        }
        let i = i.min(s.offsets.len() - 1);
        s.current_page = i;
        -s.offsets[i]
    };
    ui.set_list_vp_y(y);
}

/// Fit the current page to the viewport: `to_page` false = fit width only,
/// true = fit the whole page (the smaller of the width/height ratios).
fn fit(state: &State, pages: &Rc<VecModel<PageRow>>, ui: &AppWindow, to_page: bool) {
    let view_w = ui.get_view_w();
    let view_h = ui.get_view_h();
    {
        let mut s = state.lock().unwrap();
        let idx = s.current_page.min(s.page_pt.len().saturating_sub(1));
        let Some(&(w_pt, h_pt)) = s.page_pt.get(idx) else { return };
        // Account for rotation and the per-row horizontal margin.
        let (eff_w, eff_h) = if s.rotated_90() { (h_pt, w_pt) } else { (w_pt, h_pt) };
        if eff_w <= 0.0 || eff_h <= 0.0 {
            return;
        }
        let zw = (view_w - 24.0).max(1.0) / eff_w;
        let zoom = if to_page {
            let zh = (view_h - 2.0 * ROW_PAD).max(1.0) / eff_h;
            zw.min(zh)
        } else {
            zw
        };
        s.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    }
    rebuild_rows(state, pages);
    let idx = state.lock().unwrap().current_page;
    scroll_to_page(state, ui, idx);
    update_info(state, ui);
}

/// Rotate all pages by `q` quarter turns clockwise, then re-layout. The search
/// overlay is cleared because hit rectangles are computed for the unrotated page.
fn rotate(
    state: &State,
    pages: &Rc<VecModel<PageRow>>,
    thumbs: &Rc<VecModel<Thumb>>,
    ui: &AppWindow,
    q: u8,
) {
    let idx = {
        let mut s = state.lock().unwrap();
        s.rot_q = (s.rot_q + q) % 4;
        s.requested_thumbs.clear();
        s.hits.clear();
        s.hit_idx = 0;
        s.current_page
    };
    // Drop cached thumbnails so they re-render at the new rotation.
    let count = thumbs.row_count();
    let thumb_rows: Vec<Thumb> = (0..count)
        .map(|p| Thumb { img: Image::default(), label: format!("{}", p + 1).into() })
        .collect();
    thumbs.set_vec(thumb_rows);
    rebuild_rows(state, pages);
    scroll_to_page(state, ui, idx);
    ui.set_hl_page(-1);
    ui.set_search_info("".into());
}

/// Jump to hit `idx`: scroll to its page and paint its rectangles.
fn goto_hit(state: &State, ui: &AppWindow, idx: usize) {
    let (page, rects, count, zoom, top) = {
        let s = state.lock().unwrap();
        let Some(h) = s.hits.get(idx) else { return };
        let top = s.offsets.get(h.page as usize).copied().unwrap_or(0.0);
        (h.page, h.rects.clone(), s.hits.len(), s.zoom, top)
    };
    let hl: Vec<HitRect> = rects
        .iter()
        .map(|(x0, y0, x1, y1)| HitRect {
            x: x0 * zoom,
            y: y0 * zoom,
            w: (x1 - x0) * zoom,
            h: (y1 - y0) * zoom,
        })
        .collect();
    ui.set_hl_page(page as i32);
    ui.set_hl_rects(ModelRc::from(Rc::new(VecModel::from(hl))));
    ui.set_search_info(format!("{} / {}", idx + 1, count).into());
    // Scroll the hit page near the top (small margin).
    ui.set_list_vp_y(-(top - 30.0).max(0.0));
}

fn update_info(state: &State, ui: &AppWindow) {
    let s = state.lock().unwrap();
    ui.set_info(format!("{} pages · {}%", s.page_pt.len(), (s.zoom * 100.0).round() as i32).into());
}
