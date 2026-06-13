//! Dev utility: reproduce the desktop OCR pipeline on a real PDF.
//!
//!     cargo run -p pdf_pdfium --example ocr_probe -- file.pdf [page0 page1 ...]
//!
//! Renders each page at 300 DPI, greyscales it, runs the system Tesseract
//! exactly as apps/desktop-tauri/src-tauri/src/ocr.rs does, and reports how
//! many words survived the confidence filter. Use to tell render failures
//! apart from recognition failures.

use std::path::PathBuf;
use std::process::Command;

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{PageIndex, Rotation, ZoomLevel};

const OCR_DPI: u32 = 300;
const MIN_CONFIDENCE: f32 = 30.0;

fn find_tesseract() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TESSERACT") {
        return Some(PathBuf::from(p));
    }
    for base in [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|l| format!("{l}\\Programs")),
    ]
    .into_iter()
    .flatten()
    {
        let c = PathBuf::from(base)
            .join("Tesseract-OCR")
            .join("tesseract.exe");
        if c.exists() {
            return Some(c);
        }
    }
    Some(PathBuf::from("tesseract.exe"))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: ocr_probe file.pdf [pages...]");
    let lang = std::env::var("OCR_LANG").unwrap_or_else(|_| "eng".into());
    let want: Vec<u32> = args.filter_map(|s| s.parse().ok()).collect();

    let tesseract = find_tesseract().expect("tesseract");
    println!("tesseract: {}", tesseract.display());
    println!("lang: {lang}");

    let bytes = std::fs::read(&path).expect("read");
    println!("file: {} ({} MB)", path, bytes.len() / 1_048_576);
    let engine = PdfiumEngine::new().expect("pdfium");
    let id = engine.open_bytes(bytes).expect("open");
    let n = engine.page_count(id).unwrap();
    println!("pages: {n}");

    let pages: Vec<u32> = if want.is_empty() { vec![0, 1, 2] } else { want };
    let full_flow = std::env::var("FULL_FLOW").is_ok();
    // (page, words, img_w, img_h) collected for the full-flow inject test.
    let mut collected: Vec<(u32, Vec<pdf_mutate::OcrWord>, u32, u32)> = Vec::new();

    for p in pages.iter().copied().filter(|&p| p < n) {
        let extracted = engine.extract_text(id, PageIndex(p)).unwrap_or_default();
        let t = std::time::Instant::now();
        let r = match engine.render(RenderRequest {
            document: id,
            page: PageIndex(p),
            zoom: ZoomLevel(OCR_DPI as f32 / 72.0),
            rotation: Rotation::R0,
        }) {
            Ok(r) => r,
            Err(e) => {
                println!("page {p}: RENDER FAILED: {e}");
                continue;
            }
        };
        let render_t = t.elapsed();

        // rgba -> gray (Rec.601), same as ocr.rs
        let mut gray = image::GrayImage::new(r.width_px, r.height_px);
        let mut nonwhite = 0u64;
        for (i, px) in r.rgba.chunks_exact(4).enumerate() {
            let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as u8;
            if l < 250 {
                nonwhite += 1;
            }
            gray.as_mut()[i] = l;
        }
        let total = (r.width_px as u64) * (r.height_px as u64);
        let png = std::env::temp_dir().join(format!("ocr-probe-{p}.png"));
        image::save_buffer(
            &png,
            gray.as_raw(),
            gray.width(),
            gray.height(),
            image::ExtendedColorType::L8,
        )
        .unwrap();

        let mut cmd = Command::new(&tesseract);
        cmd.arg(&png)
            .arg("stdout")
            .args(["-l", &lang])
            .args(["--psm", "3"])
            .args(["--dpi", &OCR_DPI.to_string()]);
        if let Ok(td) = std::env::var("TESSDATA_DIR") {
            cmd.arg("--tessdata-dir").arg(td);
        }
        cmd.args(["-c", "tessedit_create_tsv=1"]);
        let out = cmd.output().expect("tesseract spawn");
        if !out.status.success() {
            println!(
                "page {p}: TESSERACT FAILED: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            continue;
        }
        let tsv = String::from_utf8_lossy(&out.stdout);
        let mut words = 0u32;
        let mut sample: Vec<String> = Vec::new();
        let mut ocr_words: Vec<pdf_mutate::OcrWord> = Vec::new();
        for line in tsv.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 12 || cols[0] != "5" {
                continue;
            }
            let conf: f32 = cols[10].parse().unwrap_or(-1.0);
            let word = cols[11].trim();
            if conf < MIN_CONFIDENCE || word.is_empty() {
                continue;
            }
            words += 1;
            if sample.len() < 12 {
                sample.push(word.to_string());
            }
            if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                cols[6].parse::<f32>(),
                cols[7].parse::<f32>(),
                cols[8].parse::<f32>(),
                cols[9].parse::<f32>(),
            ) {
                ocr_words.push(pdf_mutate::OcrWord {
                    text: word.to_string(),
                    x,
                    y,
                    w,
                    h,
                });
            }
        }
        if full_flow && !ocr_words.is_empty() {
            collected.push((p, ocr_words, r.width_px, r.height_px));
        }
        println!(
            "page {p}: {}x{} render={render_t:?} nonwhite={:.1}% existing_text={}ch words={words} png={}",
            r.width_px,
            r.height_px,
            nonwhite as f64 / total as f64 * 100.0,
            extracted.trim().len(),
            png.display()
        );
        println!("         sample: {}", sample.join(" "));
    }

    if full_flow && !collected.is_empty() {
        println!("\n=== FULL FLOW: inject -> save -> reopen -> page_text_layout ===");
        let bytes = std::fs::read(&path).expect("read");
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).expect("from_bytes");
        for (page, words, iw, ih) in &collected {
            mdoc.add_ocr_text_layer(*page, words, *iw, *ih)
                .expect("add layer");
        }
        let out = mdoc.to_bytes().expect("to_bytes");
        println!("saved OCR'd doc: {} MB", out.len() / 1_048_576);
        let reid = engine.open_bytes(out).expect("reopen");
        for (page, words, _, _) in &collected {
            let sz = engine.page_size(reid, PageIndex(*page)).unwrap();
            let runs = engine
                .page_text_layout(reid, PageIndex(*page))
                .unwrap_or_default();
            let text = engine
                .extract_text(reid, PageIndex(*page))
                .unwrap_or_default();
            let joined: String = runs
                .iter()
                .map(|r| r.text.clone())
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "page {page}: page_size {:.1}x{:.1}pt, injected {} words -> {} text_runs, extract_text {}ch",
                sz.width_pt, sz.height_pt,
                words.len(),
                runs.len(),
                text.trim().len()
            );
            // Detect horizontal overlap between consecutive same-line runs
            // (what makes selection/highlight boxes look wrong).
            let mut overlaps = 0;
            let mut worst = 0.0f32;
            let mut example = String::new();
            for w in runs.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                let same_line =
                    (a.rect_pt.y0 - b.rect_pt.y0).abs() < (a.rect_pt.y1 - a.rect_pt.y0) * 0.5;
                if a.text.trim().is_empty() || b.text.trim().is_empty() {
                    continue;
                }
                let ov = a.rect_pt.x1 - b.rect_pt.x0; // >0 means b starts before a ends
                if same_line && ov > 0.5 {
                    overlaps += 1;
                    if ov > worst {
                        worst = ov;
                        example =
                            format!("{:?}/{:?} overlap={ov:.1}pt", a.text.trim(), b.text.trim());
                    }
                }
            }
            println!("         overlapping run pairs: {overlaps}  worst: {example}");
            println!(
                "         runs: {}",
                joined.chars().take(80).collect::<String>()
            );
        }
        engine.close(reid).unwrap();
    }
    engine.close(id).unwrap();
}
