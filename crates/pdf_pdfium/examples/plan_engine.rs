//! Dev utility: time the engine path on heavy plan PDFs — open (incl. render
//! proxies), full-page renders at reading zooms, a zoomed-in region — and
//! write PNGs to eyeball the result.
//!
//!     cargo run --release -p pdf_pdfium --example plan_engine -- file.pdf outdir

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, ZoomLevel};
use std::time::Instant;

fn save(r: &pdf_core::RenderedPage, path: &str) {
    image::RgbaImage::from_raw(r.width_px, r.height_px, r.rgba.clone())
        .expect("rgba")
        .save(path)
        .expect("png");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: plan_engine file.pdf outdir");
    let out = args.next().expect("outdir");
    let bytes = std::fs::read(&path).expect("read");
    tracing_subscriber::fmt().with_env_filter("info").init();
    let engine = PdfiumEngine::new().expect("pdfium");

    let t = Instant::now();
    let id = engine.open_bytes(bytes.clone()).expect("open");
    println!("open (incl. proxies): {:?}  detail_scale={:?}", t.elapsed(), engine.detail_scale(id));
    let sz = engine.page_size(id, PageIndex(0)).unwrap();
    let req = |zoom: f32| RenderRequest {
        document: id,
        page: PageIndex(0),
        zoom: ZoomLevel(zoom),
        rotation: Rotation::R0,
    };
    for zoom in [0.05f32, 0.3, 1.0] {
        let t = Instant::now();
        let r = engine.render(req(zoom)).unwrap();
        println!("render zoom {zoom}: {}x{} in {:?}", r.width_px, r.height_px, t.elapsed());
        if zoom == 0.3 {
            save(&r, &format!("{out}/eng_full.png"));
        }
    }
    // Region at 400 % around the page centre, and one at the top edge (the
    // part PDFium used to leave black).
    for (name, fx, fy) in [("center", 0.5f32, 0.5f32), ("top", 0.3, 0.08)] {
        let zoom = 4.0;
        let (w, h) = (1600u32, 1000u32);
        let x = (sz.width_pt * zoom * fx) as i32 - w as i32 / 2;
        let y = (sz.height_pt * zoom * fy) as i32 - h as i32 / 2;
        let t = Instant::now();
        let r = engine.render_region(req(zoom), x.max(0), y.max(0), w, h).unwrap();
        println!("region {name} zoom {zoom}: {:?}", t.elapsed());
        save(&r, &format!("{out}/eng_region_{name}.png"));
        let t = Instant::now();
        let _ = engine.render_region(req(zoom), x.max(0) + 300, y.max(0), w, h).unwrap();
        println!("region {name} panned: {:?}", t.elapsed());
    }
    // Reload with identical bytes (what every edit does): proxy cache hit.
    let t = Instant::now();
    engine.reload_bytes(id, bytes).unwrap();
    println!("reload (cached proxies): {:?}", t.elapsed());
    let t = Instant::now();
    let _ = engine.render(req(0.3)).unwrap();
    println!("render after reload zoom 0.3: {:?}", t.elapsed());
}
