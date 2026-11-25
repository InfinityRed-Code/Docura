//! Size reduction must not change what a reader sees or extracts.
//!
//! Every corpus file, and every PDF in the directories below, is converted
//! twice: with `reduce_size` off and on (font programs reduced, private
//! application data dropped). PDFium then has to
//!
//! - extract the same text with the same positions from every page, and
//! - render every page to the same bitmap.
//!
//! Tolerance: positions may differ by 0.01 pt; bitmaps (108 dpi) must be
//! identical pixel for pixel. The fonts keep their glyph ids, outlines,
//! metrics and hinting, so there is nothing that should move.
//!
//! Real documents: `PDFA_FIDELITY_DIRS` (separated by `;`), by default the
//! PDFs a local Ghostscript installation ships (`doc`, `examples`). Where
//! neither exists (CI) only the corpus is compared, and the test says so.
//!
//! One test function only: PDFium is not safe to drive from parallel tests.

mod common;

use std::path::{Path, PathBuf};

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfa::{convert, Options};
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
    let (mut files, mut page_total, mut runs_total, mut saved) = (0usize, 0usize, 0usize, 0i64);
    let (mut reduced_fonts, mut dropped_private) = (0usize, 0usize);
    for (name, bytes) in &inputs {
        for level in ["1b", "2b"] {
            let mut opts = Options::new(level.parse().unwrap()).with_rasterizer(raster.clone());
            opts.reduce_size = false;
            let before = match convert(bytes, &opts) {
                Ok(c) => c,
                Err(e) => {
                    problems.push(format!("{name} {level}: {e}"));
                    continue;
                }
            };
            opts.reduce_size = true;
            let after = convert(bytes, &opts).expect("the reduced conversion succeeds where the plain one does");
            files += 1;
            saved += before.bytes.len() as i64 - after.bytes.len() as i64;
            reduced_fonts += after.report.has("fonts_reduced") as usize;
            dropped_private += after.report.has("private_data_removed") as usize;
            if after.bytes.len() > before.bytes.len() + 64 {
                problems.push(format!("{name} {level}: reduced output is larger ({} > {})", after.bytes.len(), before.bytes.len()));
            }
            let (a, b) = (pages(&engine, &before.bytes), pages(&engine, &after.bytes));
            if a.len() != b.len() {
                problems.push(format!("{name} {level}: {} pages became {}", a.len(), b.len()));
                continue;
            }
            for i in 0..a.len() {
                let (pa, pb) = (&a[i], &b[i]);
                page_total += 1;
                runs_total += pa.text.len();
                if let Some(run) = first_text_difference(pa, pb) {
                    problems.push(format!("{name} {level} page {}: text differs at run {run}: {:?} became {:?}", i + 1, pa.text.get(run), pb.text.get(run)));
                }
                let (pixels, largest) = pixel_difference(pa, pb);
                if pa.size != pb.size || pixels > 0 {
                    problems.push(format!("{name} {level} page {}: {:?} became {:?}, {pixels} pixels differ (largest difference {largest} of 255)", i + 1, pa.size, pb.size));
                }
            }
        }
    }
    println!(
        "fidelity: {files} conversions of {} inputs ({} real documents), {page_total} pages, {runs_total} text runs; fonts reduced in {reduced_fonts}, private data dropped in {dropped_private}; {saved} bytes saved",
        inputs.len(),
        real.len()
    );
    assert!(problems.is_empty(), "size reduction changed the result:\n{}", problems.join("\n"));
    assert!(reduced_fonts > 0, "no font was reduced at all: the comparison would prove nothing");
}
