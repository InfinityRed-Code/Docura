//! images_to_pdf output opens in PDFium, has one page per image with the
//! image's pixel size, and renders the expected colors.
//!
//! Single #[test] fn on purpose — parallel test threads hang the shared
//! (non-thread-safe) PDFium binding.

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, ZoomLevel};

fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn jpeg(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .unwrap();
    out.into_inner()
}

#[test]
fn images_pdf_opens_and_renders() {
    let pdf = pdf_mutate::images_to_pdf(&[png(100, 80, [0, 0, 255]), jpeg(60, 90, [220, 20, 20])])
        .unwrap();

    let engine = PdfiumEngine::new().expect("pdfium.dll available");
    let id = engine.open_bytes(pdf).unwrap();
    assert_eq!(engine.page_count(id).unwrap(), 2);

    let s0 = engine.page_size(id, PageIndex(0)).unwrap();
    assert!((s0.width_pt - 100.0).abs() < 0.5 && (s0.height_pt - 80.0).abs() < 0.5);

    // Page 1 must render blue, page 2 red (center pixel).
    for (page, expect) in [(0u32, [0u8, 0, 255]), (1, [220, 20, 20])] {
        let r = engine
            .render(RenderRequest {
                document: id,
                page: PageIndex(page),
                zoom: ZoomLevel(1.0),
                rotation: Rotation::R0,
            })
            .unwrap();
        let o = (((r.height_px / 2) * r.width_px + r.width_px / 2) * 4) as usize;
        let px = &r.rgba[o..o + 3];
        for c in 0..3 {
            assert!(
                (px[c] as i32 - expect[c] as i32).abs() < 30,
                "page {page} pixel {px:?} != {expect:?}"
            );
        }
    }
    engine.close(id).unwrap();
}
