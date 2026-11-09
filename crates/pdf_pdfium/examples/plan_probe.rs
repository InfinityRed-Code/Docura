//! Dev utility: profile heavy (CAD-plan) pages — page load vs. rasterisation
//! at several scales and render flags, plus the lopdf parse the mutation
//! pipeline does.
//!
//!     cargo run --release -p pdf_pdfium --example plan_probe -- file.pdf [out.png]

use pdfium_render::prelude::*;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: plan_probe file.pdf [out.png]");
    let out_png = args.next();
    let bytes = std::fs::read(&path).expect("read");

    let t = Instant::now();
    let lo = lopdf::Document::load_mem(&bytes);
    println!(
        "lopdf parse: {:?} ({} objects, ok={})",
        t.elapsed(),
        lo.as_ref().map(|d| d.objects.len()).unwrap_or(0),
        lo.is_ok()
    );

    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./vendor/pdfium"))
            .expect("bind pdfium"),
    );
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).expect("open");
    let page_idx = 0u16;

    for &(label, scale, aa) in &[
        ("thumb 0.05", 0.05f32, true),
        ("0.25", 0.25, true),
        ("0.5", 0.5, true),
        ("1.0", 1.0, true),
        ("1.0 noAA", 1.0, false),
        ("2.0", 2.0, true),
    ] {
        let t = Instant::now();
        let page = doc.pages().get(page_idx).expect("page");
        let t_load = t.elapsed();
        let mut cfg = PdfRenderConfig::new().scale_page_by_factor(scale);
        if !aa {
            cfg = cfg
                .set_path_smoothing(false)
                .set_image_smoothing(false)
                .set_text_smoothing(false);
        }
        let t = Instant::now();
        let bmp = page.render_with_config(&cfg).expect("render");
        let t_render = t.elapsed();
        println!(
            "{label:>10}: load {t_load:?}  render {}x{} {t_render:?}  objects={}",
            bmp.width(),
            bmp.height(),
            page.objects().len()
        );
    }

    // Same page object, second render (is parsed content reused?).
    let page = doc.pages().get(page_idx).expect("page");
    let cfg = PdfRenderConfig::new().scale_page_by_factor(0.5);
    let t = Instant::now();
    let _ = page.render_with_config(&cfg).unwrap();
    let first = t.elapsed();
    let t = Instant::now();
    let _ = page.render_with_config(&cfg).unwrap();
    println!("same page handle 0.5: first {first:?}, second {:?}", t.elapsed());

    let ann = page.annotations();
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for a in ann.iter() {
        *kinds.entry(format!("{:?}", a.annotation_type())).or_default() += 1;
    }
    println!("annotations: {kinds:?}");

    if let Some(out) = out_png {
        let img = page
            .render_with_config(&PdfRenderConfig::new().scale_page_by_factor(0.3))
            .unwrap()
            .as_image();
        img.save(&out).unwrap();
        println!("wrote {out}");
    }
}
