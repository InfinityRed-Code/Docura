//! Dev utility: time open / page_size / page_text_layout / render on a PDF.
//!
//!     cargo run -p pdf_pdfium --example bench_text -- file.pdf [pages]

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, ZoomLevel};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: bench_text file.pdf [pages]");
    let probe: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);

    let bytes = std::fs::read(&path).expect("read");
    let engine = PdfiumEngine::new().expect("pdfium");

    let t0 = std::time::Instant::now();
    let id = engine.open_bytes(bytes).expect("open");
    println!("open: {:?}", t0.elapsed());

    let n = engine.page_count(id).unwrap();
    let t1 = std::time::Instant::now();
    for i in 0..n {
        engine.page_size(id, PageIndex(i)).unwrap();
    }
    println!("page_size x{n}: {:?}", t1.elapsed());

    for p in 0..probe.min(n) {
        let t = std::time::Instant::now();
        let r = engine
            .render(RenderRequest {
                document: id,
                page: PageIndex(p),
                zoom: ZoomLevel(1.0),
                rotation: Rotation::R0,
            })
            .unwrap();
        let t_render = t.elapsed();
        let t = std::time::Instant::now();
        let runs = engine.page_text_layout(id, PageIndex(p)).unwrap();
        let text = engine.extract_text(id, PageIndex(p)).unwrap_or_default();
        println!(
            "page {p}: render {}x{} {t_render:?}, text_layout {} runs {:?}, extract_text {} chars: {:?}",
            r.width_px,
            r.height_px,
            runs.len(),
            t.elapsed(),
            text.chars().count(),
            text.chars().take(60).collect::<String>()
        );
    }
    engine.close(id).unwrap();
}
