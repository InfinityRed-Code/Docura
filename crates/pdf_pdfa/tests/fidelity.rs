//! Size reduction must not change what a reader sees or extracts.
//!
//! Every corpus file, and every PDF in the directories below, is converted
//! three times:
//!
//! 1. plain: `reduce_size` and `recompress_images` off,
//! 2. lossless: `reduce_size` on (font programs reduced, private
//!    application data dropped, images packed tighter without loss),
//! 3. default: `recompress_images` on as well (photographs as JPEG).
//!
//! PDFium then has to
//!
//! - extract the same text with the same positions from every page of all
//!   three (tolerance 0.01 pt),
//! - render every page of the lossless output to the same bitmap as the
//!   plain one, pixel for pixel (108 dpi),
//! - render every page of the default output to the same bitmap too,
//!   unless the conversion reports recompressed images. Those documents
//!   are measured instead: each page against the plain one, SSIM of the
//!   luminance and PSNR over RGB, and both have to stay above the bounds
//!   the converter itself uses per image ([`SSIM_MIN`], [`PSNR_MIN`]).
//!   The numbers are printed per document.
//!
//! Real documents: `PDFA_FIDELITY_DIRS` (separated by `;`), by default the
//! PDFs a local Ghostscript installation ships (`doc`, `examples`). Where
//! neither exists (CI) only the corpus is compared, and the test says so.
//!
//! One test function only: PDFium is not safe to drive from parallel tests.

mod common;

use std::path::{Path, PathBuf};

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfa::{convert, psnr, ssim, Converted, Options, PSNR_MIN, SSIM_MIN};
use pdf_types::{PageIndex, Rotation, ZoomLevel};

const ZOOM: f32 = 1.5;
const POSITION_TOLERANCE_PT: f32 = 0.01;

fn collect_pdfs(p: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(dir) = std::fs::read_dir(p) {
        let mut entries: Vec<PathBuf> = Vec::new();
        for e in dir.flatten() {
            entries.push(e.path());
        }
        entries.sort();
        for e in entries {
            collect_pdfs(&e, out);
        }
    } else if let Some(ext) = p.extension() {
        if ext.eq_ignore_ascii_case("pdf") {
            out.push(p.to_path_buf());
        }
    }
}

fn real_documents() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(v) = std::env::var("PDFA_FIDELITY_DIRS") {
        for d in v.split(';') {
            dirs.push(PathBuf::from(d));
        }
    } else {
        for base in [r"C:\Program Files\gs", "/usr/share/doc/ghostscript", "/usr/share/ghostscript"] {
            if let Ok(rd) = std::fs::read_dir(base) {
                for e in rd.flatten() {
                    dirs.push(e.path().join("doc"));
                    dirs.push(e.path().join("examples"));
                }
            }
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        collect_pdfs(&d, &mut out);
    }
    out
}

struct Page {
    text: Vec<(String, [f32; 4])>,
    size: (u32, u32),
    rgba: Vec<u8>,
}

fn pages(engine: &pdf_pdfium::PdfiumEngine, pdf: &[u8]) -> Vec<Page> {
    let doc = engine.open_bytes(pdf.to_vec()).expect("PDFium opens the output");
    let n = engine.page_count(doc).expect("page count");
    let mut out = Vec::new();
    for i in 0..n {
        let mut text = Vec::new();
        for r in engine.page_text_layout(doc, PageIndex(i)).unwrap_or_default() {
            text.push((r.text, [r.rect_pt.x0, r.rect_pt.y0, r.rect_pt.x1, r.rect_pt.y1]));
        }
        let r = engine.render(RenderRequest { document: doc, page: PageIndex(i), zoom: ZoomLevel(ZOOM), rotation: Rotation::R0 }).expect("render");
        out.push(Page { text, size: (r.width_px, r.height_px), rgba: r.rgba });
    }
    let _ = engine.close(doc);
    out
}

/// Index of the first text run that differs (text or position).
fn first_text_difference(a: &Page, b: &Page) -> Option<usize> {
    for i in 0..a.text.len().max(b.text.len()) {
        let (Some(x), Some(y)) = (a.text.get(i), b.text.get(i)) else { return Some(i) };
        if x.0 != y.0 {
            return Some(i);
        }
        for k in 0..4 {
            if (x.1[k] - y.1[k]).abs() > POSITION_TOLERANCE_PT {
                return Some(i);
            }
        }
    }
    None
}

/// Number of differing pixels and the largest channel difference.
fn pixel_difference(a: &Page, b: &Page) -> (usize, u8) {
    if a.rgba == b.rgba {
        return (0, 0);
    }
    let (mut count, mut max) = (0, 0u8);
    for i in (0..a.rgba.len().min(b.rgba.len())).step_by(4) {
        let mut differs = false;
        for k in i..(i + 4).min(a.rgba.len()).min(b.rgba.len()) {
            let d = a.rgba[k].abs_diff(b.rgba[k]);
            max = max.max(d);
            differs |= d > 0;
        }
        count += differs as usize;
    }
    (count, max)
}

/// The colour channels of a rendered page and their luminance.
fn rgb_and_luminance(p: &Page) -> (Vec<u8>, Vec<u8>) {
    let (mut rgb, mut luma) = (Vec::with_capacity(p.rgba.len() / 4 * 3), Vec::with_capacity(p.rgba.len() / 4));
    for px in p.rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
        luma.push(((299 * px[0] as u32 + 587 * px[1] as u32 + 114 * px[2] as u32 + 500) / 1000) as u8);
    }
    (rgb, luma)
}

/// SSIM (luminance) and PSNR (RGB) of a page against its reference.
fn page_similarity(a: &Page, b: &Page) -> Option<(f64, f64)> {
    if a.size != b.size {
        return None;
    }
    let ((rgb_a, luma_a), (rgb_b, luma_b)) = (rgb_and_luminance(a), rgb_and_luminance(b));
    Some((ssim(&luma_a, &luma_b, a.size.0 as usize, a.size.1 as usize)?, psnr(&rgb_a, &rgb_b)))
}

/// Text of every page as in the reference; `true` when the page counts fit.
fn same_text(what: &str, reference: &[Page], other: &[Page], problems: &mut Vec<String>) -> bool {
    if reference.len() != other.len() {
        problems.push(format!("{what}: {} pages became {}", reference.len(), other.len()));
        return false;
    }
    for (i, (pa, pb)) in reference.iter().zip(other).enumerate() {
        if let Some(run) = first_text_difference(pa, pb) {
            problems.push(format!("{what} page {}: text differs at run {run}: {:?} became {:?}", i + 1, pa.text.get(run), pb.text.get(run)));
        }
    }
    true
}

fn same_pixels(what: &str, reference: &[Page], other: &[Page], problems: &mut Vec<String>) {
    for (i, (pa, pb)) in reference.iter().zip(other).enumerate() {
        let (pixels, largest) = pixel_difference(pa, pb);
        if pa.size != pb.size || pixels > 0 {
            problems.push(format!("{what} page {}: {:?} became {:?}, {pixels} pixels differ (largest difference {largest} of 255)", i + 1, pa.size, pb.size));
        }
    }
}

fn detail(c: &Converted, code: &str) -> Option<String> {
    c.report.notes.iter().find(|n| n.code == code).map(|n| n.detail.clone())
}

#[test]
fn reduced_output_reads_and_renders_like_the_unreduced_one() {
    let raster = common::PdfiumRasterizer::new();
    let engine = pdf_pdfium::PdfiumEngine::new().expect("pdfium");
    let mut inputs: Vec<(String, Vec<u8>)> = Vec::new();
    for c in common::corpus() {
        // (The password-protected file is refused, nothing to compare.)
        if c.expect != common::Expect::PasswordRequired {
            inputs.push((c.name.to_string(), c.bytes));
        }
    }
    let real = real_documents();
    if real.is_empty() {
        eprintln!("NOTE: no real documents found (set PDFA_FIDELITY_DIRS); comparing the corpus only");
    }
    for p in &real {
        inputs.push((p.display().to_string(), std::fs::read(p).expect("read")));
    }

    let mut problems: Vec<String> = Vec::new();
    let (mut files, mut page_total, mut runs_total) = (0usize, 0usize, 0usize);
    let (mut saved_lossless, mut saved_images) = (0i64, 0i64);
    let (mut reduced_fonts, mut dropped_private, mut recompressed) = (0usize, 0usize, 0usize);
    let mut table = String::from("| Dokument | Stufe | Bilder neu komprimiert (Ersparnis, schlechtestes Bild) | Seiten | Seite: SSIM min. | Seite: PSNR min. |\n|---|---|---|---|---|---|\n");
    for (name, bytes) in &inputs {
        for level in ["1b", "2b"] {
            let what = format!("{name} {level}");
            let mut opts = Options::new(level.parse().unwrap()).with_rasterizer(raster.clone());
            opts.reduce_size = false;
            opts.recompress_images = false;
            let plain = match convert(bytes, &opts) {
                Ok(c) => c,
                Err(e) => {
                    problems.push(format!("{what}: {e}"));
                    continue;
                }
            };
            opts.reduce_size = true;
            let lossless = convert(bytes, &opts).expect("the reduced conversion succeeds where the plain one does");
            opts.recompress_images = true;
            let default = convert(bytes, &opts).expect("the default conversion succeeds where the plain one does");
            files += 1;
            saved_lossless += plain.bytes.len() as i64 - lossless.bytes.len() as i64;
            saved_images += lossless.bytes.len() as i64 - default.bytes.len() as i64;
            reduced_fonts += lossless.report.has("fonts_reduced") as usize;
            dropped_private += lossless.report.has("private_data_removed") as usize;
            if lossless.bytes.len() > plain.bytes.len() + 64 {
                problems.push(format!("{what}: reduced output is larger ({} > {})", lossless.bytes.len(), plain.bytes.len()));
            }
            if default.bytes.len() > lossless.bytes.len() + 64 {
                problems.push(format!("{what}: output with recompressed images is larger ({} > {})", default.bytes.len(), lossless.bytes.len()));
            }
            assert!(!lossless.report.has("images_recompressed"), "{what}: images recompressed although switched off");

            let reference = pages(&engine, &plain.bytes);
            page_total += reference.len();
            for p in &reference {
                runs_total += p.text.len();
            }
            let reduced = pages(&engine, &lossless.bytes);
            if same_text(&format!("{what} (lossless)"), &reference, &reduced, &mut problems) {
                same_pixels(&format!("{what} (lossless)"), &reference, &reduced, &mut problems);
            }
            let with_jpeg = pages(&engine, &default.bytes);
            if !same_text(&what, &reference, &with_jpeg, &mut problems) {
                continue;
            }
            let Some(images) = detail(&default, "images_recompressed") else {
                same_pixels(&what, &reference, &with_jpeg, &mut problems);
                continue;
            };
            recompressed += 1;
            let (mut changed, mut worst_ssim, mut worst_psnr) = (0usize, 1.0f64, f64::INFINITY);
            for (i, (pa, pb)) in reference.iter().zip(&with_jpeg).enumerate() {
                if pa.rgba == pb.rgba {
                    continue;
                }
                changed += 1;
                match page_similarity(pa, pb) {
                    Some((s, p)) => {
                        worst_ssim = worst_ssim.min(s);
                        worst_psnr = worst_psnr.min(p);
                        if s < SSIM_MIN || p < PSNR_MIN {
                            problems.push(format!("{what} page {}: SSIM {s:.4}, PSNR {p:.1} dB against the lossless page (bounds {SSIM_MIN} and {PSNR_MIN} dB)", i + 1));
                        }
                    }
                    None => problems.push(format!("{what} page {}: {:?} became {:?}", i + 1, pa.size, pb.size)),
                }
            }
            let short = match Path::new(name).file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => name.clone(),
            };
            table.push_str(&format!("| {short} | {level} | {images} | {changed} von {} | {worst_ssim:.4} | {worst_psnr:.1} dB |\n", reference.len()));
        }
    }
    println!(
        "fidelity: {files} conversions of {} inputs ({} real documents), {page_total} pages, {runs_total} text runs; fonts reduced in {reduced_fonts}, private data dropped in {dropped_private}, images recompressed in {recompressed}; {saved_lossless} bytes saved without loss, {saved_images} more by recompressing images",
        inputs.len(),
        real.len()
    );
    println!("{table}");
    let out = common::workspace_root().join("target").join("pdfa-fidelity");
    let _ = std::fs::create_dir_all(&out);
    let _ = std::fs::write(out.join("images.md"), &table);
    assert!(problems.is_empty(), "size reduction changed the result:\n{}", problems.join("\n"));
    assert!(reduced_fonts > 0, "no font was reduced at all: the comparison would prove nothing");
    assert!(recompressed > 0, "no image was recompressed at all: the comparison would prove nothing");
}
