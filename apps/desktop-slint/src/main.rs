//! Docura PDF — Slint frontend (test-slint branch).
//!
//! Proof-of-concept replacement for the Tauri + Vite UI: a native Slint
//! window that drives the existing `pdf_core`/`pdf_pdfium` engine directly
//! (no webview, no IPC). Implements the core viewer — open, render, page
//! navigation, zoom. The richer features of the Tauri app (OCR, annotations,
//! signing, forms, …) call into the same `pdf_*` crates and can be wired onto
//! this shell incrementally.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{DocumentId, PageIndex, Rotation, ZoomLevel};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

slint::include_modules!();

const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 6.0;
const ZOOM_STEP: f32 = 1.25;

struct AppState {
    engine: Arc<dyn PdfEngine>,
    doc: Option<DocumentId>,
    page: u32,
    page_count: u32,
    zoom: f32,
}

impl AppState {
    fn clamp_page(&mut self) {
        if self.page_count == 0 {
            self.page = 0;
        } else if self.page >= self.page_count {
            self.page = self.page_count - 1;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let engine = PdfiumEngine::new().expect("bind pdfium (vendor/pdfium/pdfium.dll)");

    let state = Rc::new(RefCell::new(AppState {
        engine: Arc::new(engine),
        doc: None,
        page: 0,
        page_count: 0,
        zoom: 1.0,
    }));

    ui.on_open_clicked({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let ui = ui.unwrap();
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_title("Open PDF")
                .pick_file()
            else {
                return;
            };
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    ui.set_status(format!("Read error: {e}").into());
                    return;
                }
            };
            {
                let mut s = state.borrow_mut();
                if let Some(d) = s.doc.take() {
                    let _ = s.engine.close(d);
                }
                match s.engine.open_bytes(bytes) {
                    Ok(doc) => {
                        s.page_count = s.engine.page_count(doc).unwrap_or(0);
                        s.doc = Some(doc);
                        s.page = 0;
                        s.zoom = 1.0;
                    }
                    Err(e) => {
                        ui.set_status(format!("Open error: {e}").into());
                        return;
                    }
                }
            }
            ui.set_has_doc(true);
            ui.set_status(
                format!("Opened {}", path.file_name().unwrap_or_default().to_string_lossy()).into(),
            );
            render_current(&state, &ui);
        }
    });

    ui.on_prev_clicked({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let ui = ui.unwrap();
            {
                let mut s = state.borrow_mut();
                if s.page > 0 {
                    s.page -= 1;
                } else {
                    return;
                }
            }
            render_current(&state, &ui);
        }
    });

    ui.on_next_clicked({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let ui = ui.unwrap();
            {
                let mut s = state.borrow_mut();
                if s.page + 1 < s.page_count {
                    s.page += 1;
                } else {
                    return;
                }
            }
            render_current(&state, &ui);
        }
    });

    ui.on_zoom_in({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let ui = ui.unwrap();
            {
                let mut s = state.borrow_mut();
                s.zoom = (s.zoom * ZOOM_STEP).min(ZOOM_MAX);
            }
            render_current(&state, &ui);
        }
    });

    ui.on_zoom_out({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let ui = ui.unwrap();
            {
                let mut s = state.borrow_mut();
                s.zoom = (s.zoom / ZOOM_STEP).max(ZOOM_MIN);
            }
            render_current(&state, &ui);
        }
    });

    ui.on_zoom_reset({
        let ui = ui.as_weak();
        let state = state.clone();
        move || {
            let ui = ui.unwrap();
            {
                let mut s = state.borrow_mut();
                s.zoom = 1.0;
            }
            render_current(&state, &ui);
        }
    });

    ui.run()?;
    Ok(())
}

/// Render the current page at the current zoom and push it to the window.
fn render_current(state: &Rc<RefCell<AppState>>, ui: &AppWindow) {
    let (engine, doc, page, page_count, zoom) = {
        let mut s = state.borrow_mut();
        s.clamp_page();
        match s.doc {
            Some(doc) => (s.engine.clone(), doc, s.page, s.page_count, s.zoom),
            None => return,
        }
    };

    let req = RenderRequest {
        document: doc,
        page: PageIndex(page),
        zoom: ZoomLevel(zoom),
        rotation: Rotation::R0,
    };
    match engine.render(req) {
        Ok(r) => {
            let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(r.width_px, r.height_px);
            // RenderedPage.rgba is tightly packed RGBA8, width*height*4 bytes.
            buf.make_mut_bytes().copy_from_slice(&r.rgba);
            ui.set_page_image(Image::from_rgba8(buf));
            ui.set_page_label(format!("{} / {}", page + 1, page_count).into());
            ui.set_zoom_label(format!("{}%", (zoom * 100.0).round() as i32).into());
        }
        Err(e) => ui.set_status(format!("Render error: {e}").into()),
    }
}
