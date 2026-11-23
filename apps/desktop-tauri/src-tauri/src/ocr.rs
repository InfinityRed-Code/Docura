//! OCR integration (milestone v1.0).
//!
//! Tesseract is driven as an external CLI, like Ghostscript (PDF/A) and
//! LibreOffice (Office→PDF). The Windows installer ships it (with `deu` and
//! `eng`) in `<install dir>\tesseract`, which is preferred; a `TESSERACT`
//! override, PATH and standard system installs are the fallbacks.
//! Recognition runs on page bitmaps rendered by pdfium at a fixed DPI; the
//! TSV word boxes are mapped back into PDF user space and injected as an
//! invisible text layer via `pdf_mutate::add_ocr_text_layer`.
//!
//! Further language packs are downloaded from the `tessdata_fast` repository
//! into the app config dir and passed to Tesseract via `--tessdata-dir`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

use image::GrayImage;
use parking_lot::Mutex;
use pdf_core::{PdfEngine, RenderRequest};
use pdf_types::{DocumentId, PageIndex, Rotation, ZoomLevel};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::{no_window, AppState};

const OCR_DPI: u32 = 300;
/// Upper bound on parallel Tesseract processes. Each holds a full 300-DPI page
/// bitmap in RAM, so cap concurrency to keep the memory spike bounded even on
/// many-core machines.
const OCR_MAX_WORKERS: usize = 6;
/// Words below this Tesseract confidence (0-100) are dropped.
const MIN_CONFIDENCE: f32 = 30.0;
/// Pages whose existing text layer exceeds this many characters are
/// considered "already OCRed / born digital" and skipped.
const TEXT_PAGE_THRESHOLD: usize = 50;

// ---------------------------------------------------------------- detection

#[derive(Serialize)]
pub struct TesseractInfo {
    available: bool,
    path: Option<String>,
    version: Option<String>,
    /// Installed language codes (system tessdata + downloaded packs).
    languages: Vec<String>,
}

/// Directories holding a Tesseract bundled with the app: `tesseract\` next to
/// the executable (installed by nsis/hooks.nsi). Debug builds also accept the
/// copy staged by scripts/fetch-tesseract.ps1, so `tauri dev` and the tests
/// run against the bundled build.
fn bundled_tesseract_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("tesseract")))
        {
            v.push(dir);
        }
        #[cfg(debug_assertions)]
        v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../vendor/tesseract"));
    }
    v
}

/// The bundled Tesseract's tessdata dir if `tesseract` is a bundled binary.
fn bundled_tessdata(tesseract: &Path) -> Option<PathBuf> {
    let dir = tesseract.parent()?;
    bundled_tesseract_dirs()
        .iter()
        .any(|b| b == dir)
        .then(|| dir.join("tessdata"))
}

/// `Command` for a Tesseract binary. A bundled Tesseract always reads its own
/// models: a TESSDATA_PREFIX inherited from some system install would
/// otherwise point it at that installation's tessdata.
fn tesseract_command(tesseract: &str) -> Command {
    let mut cmd = Command::new(tesseract);
    if let Some(tessdata) = bundled_tessdata(Path::new(tesseract)) {
        cmd.env("TESSDATA_PREFIX", tessdata);
    }
    cmd
}

fn tesseract_candidates() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("TESSERACT") {
        v.push(PathBuf::from(p));
    }
    #[cfg(target_os = "windows")]
    {
        v.extend(
            bundled_tesseract_dirs()
                .into_iter()
                .map(|d| d.join("tesseract.exe")),
        );
        v.push(PathBuf::from("tesseract.exe"));
        for base in [
            std::env::var("ProgramFiles").ok(),
            std::env::var("ProgramFiles(x86)").ok(),
            std::env::var("LOCALAPPDATA")
                .ok()
                .map(|l| format!("{l}\\Programs")),
            std::env::var("LOCALAPPDATA").ok(),
        ]
        .into_iter()
        .flatten()
        {
            v.push(PathBuf::from(base).join("Tesseract-OCR").join("tesseract.exe"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        v.push(PathBuf::from("tesseract"));
    }
    v
}

fn probe_tesseract(path: &Path) -> Option<(String, String)> {
    let out = no_window(Command::new(path).arg("--version")).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // First line: "tesseract v5.3.3.20231005" or "tesseract 5.4.1".
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next().unwrap_or("").trim();
    let ver = first
        .strip_prefix("tesseract")
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .filter(|s| !s.is_empty())?;
    Some((path.to_string_lossy().to_string(), ver))
}

pub(crate) fn find_tesseract() -> Option<String> {
    tesseract_candidates()
        .iter()
        .find_map(|c| probe_tesseract(c).map(|(p, _)| p))
}

/// Directory for language packs downloaded by the app.
fn user_tessdata_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("tessdata"))
        .map_err(|e| e.to_string())
}

fn system_langs(tesseract: &str) -> Vec<String> {
    let Ok(out) = no_window(tesseract_command(tesseract).arg("--list-langs")).output() else {
        return Vec::new();
    };
    // First line is a header ("List of available languages in ...").
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && *l != "osd")
        .collect()
}

fn user_langs(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".traineddata").map(|s| s.to_string())
        })
        .filter(|l| *l != "osd")
        .collect()
}

#[tauri::command]
pub async fn check_tesseract(app: tauri::AppHandle) -> TesseractInfo {
    tokio::task::spawn_blocking(move || {
        for c in tesseract_candidates() {
            if let Some((path, version)) = probe_tesseract(&c) {
                let mut langs = system_langs(&path);
                if let Ok(dir) = user_tessdata_dir(&app) {
                    langs.extend(user_langs(&dir));
                }
                langs.sort();
                langs.dedup();
                return TesseractInfo {
                    available: true,
                    path: Some(path),
                    version: Some(version),
                    languages: langs,
                };
            }
        }
        TesseractInfo { available: false, path: None, version: None, languages: Vec::new() }
    })
    .await
    .unwrap_or(TesseractInfo { available: false, path: None, version: None, languages: Vec::new() })
}

// ----------------------------------------------------------- language packs

/// Download a `tessdata_fast` language pack into the app's tessdata dir.
#[tauri::command]
pub async fn download_ocr_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    if !lang
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        || lang.len() < 3
        || lang.len() > 24
    {
        return Err(format!("invalid language code: {lang}"));
    }
    let dir = user_tessdata_dir(&app)?;
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let url = format!(
            "https://github.com/tesseract-ocr/tessdata_fast/raw/main/{lang}.traineddata"
        );
        let resp = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(120))
            .call()
            .map_err(|e| format!("OCR_DOWNLOAD_FAILED:{e}"))?;
        let mut bytes: Vec<u8> = Vec::new();
        resp.into_reader()
            .take(100 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("OCR_DOWNLOAD_FAILED:{e}"))?;
        if bytes.len() < 64 * 1024 {
            return Err("OCR_DOWNLOAD_FAILED:file too small".into());
        }
        let tmp = dir.join(format!("{lang}.traineddata.part"));
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        let dest = dir.join(format!("{lang}.traineddata"));
        std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
        tracing::info!("ocr: downloaded {lang} ({} KB)", bytes.len() / 1024);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

use std::io::Read;

/// Resolve how to pass the requested languages to Tesseract. Plus-joined
/// combos ("deu+eng") are supported. If any language only exists as a
/// downloaded pack, run with `--tessdata-dir` and copy missing system
/// languages over so combos across both locations work.
fn resolve_tessdata(
    app: &tauri::AppHandle,
    tesseract: &str,
    lang: &str,
) -> Result<Option<PathBuf>, String> {
    let wanted: Vec<&str> = lang.split('+').filter(|s| !s.is_empty()).collect();
    if wanted.is_empty() {
        return Err("OCR_LANG_MISSING:".into());
    }
    let sys = system_langs(tesseract);
    if wanted.iter().all(|w| sys.iter().any(|s| s == w)) {
        return Ok(None);
    }
    let dir = user_tessdata_dir(app)?;
    let user = user_langs(&dir);
    for w in &wanted {
        if user.iter().any(|u| u == *w) {
            continue;
        }
        if sys.iter().any(|s| s == *w) {
            // Copy the system pack next to the downloaded ones.
            let sys_dir = Path::new(tesseract)
                .parent()
                .map(|p| p.join("tessdata"))
                .ok_or_else(|| format!("OCR_LANG_MISSING:{w}"))?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::copy(
                sys_dir.join(format!("{w}.traineddata")),
                dir.join(format!("{w}.traineddata")),
            )
            .map_err(|_| format!("OCR_LANG_MISSING:{w}"))?;
        } else {
            return Err(format!("OCR_LANG_MISSING:{w}"));
        }
    }
    Ok(Some(dir))
}

// ------------------------------------------------------------ preprocessing

/// Scan cleanup before recognition: grayscale conversion happens at render
/// time; here we stretch contrast, despeckle and deskew.
pub(crate) fn preprocess_gray(img: GrayImage) -> GrayImage {
    let img = contrast_stretch(img);
    let img = median3(&img);
    let angle = estimate_skew_deg(&img);
    if angle.abs() > 0.3 {
        tracing::info!("ocr: deskew by {angle:.2}°");
        rotate_gray(&img, -angle)
    } else {
        img
    }
}

fn contrast_stretch(mut img: GrayImage) -> GrayImage {
    let mut hist = [0u32; 256];
    for p in img.pixels() {
        hist[p.0[0] as usize] += 1;
    }
    let total: u32 = hist.iter().sum();
    if total == 0 {
        return img;
    }
    let pct = |frac: f32| -> u8 {
        let target = (total as f32 * frac) as u32;
        let mut acc = 0u32;
        for (i, &c) in hist.iter().enumerate() {
            acc += c;
            if acc >= target {
                return i as u8;
            }
        }
        255
    };
    let lo = pct(0.01) as f32;
    let hi = pct(0.99) as f32;
    if hi - lo < 16.0 {
        return img;
    }
    let scale = 255.0 / (hi - lo);
    for p in img.pixels_mut() {
        p.0[0] = ((p.0[0] as f32 - lo) * scale).clamp(0.0, 255.0) as u8;
    }
    img
}

fn median3(img: &GrayImage) -> GrayImage {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return img.clone();
    }
    let mut out = img.clone();
    let src = img.as_raw();
    let stride = w as usize;
    for y in 1..(h - 1) as usize {
        for x in 1..(w - 1) as usize {
            let mut n = [
                src[(y - 1) * stride + x - 1],
                src[(y - 1) * stride + x],
                src[(y - 1) * stride + x + 1],
                src[y * stride + x - 1],
                src[y * stride + x],
                src[y * stride + x + 1],
                src[(y + 1) * stride + x - 1],
                src[(y + 1) * stride + x],
                src[(y + 1) * stride + x + 1],
            ];
            n.sort_unstable();
            out.as_mut()[y * stride + x] = n[4];
        }
    }
    out
}

/// Estimate page skew via projection profiles: shear dark pixels by a set
/// of candidate angles, pick the one with the highest row-sum variance
/// (sharpest text-line separation). Range ±5°, resolution 0.25°.
fn estimate_skew_deg(img: &GrayImage) -> f32 {
    let (w, h) = img.dimensions();
    // Work on a downscaled copy for speed.
    let scale = (1000.0 / w as f32).min(1.0);
    let (sw, sh) = (
        ((w as f32 * scale) as u32).max(1),
        ((h as f32 * scale) as u32).max(1),
    );
    let small = image::imageops::resize(img, sw, sh, image::imageops::FilterType::Triangle);

    // Otsu threshold.
    let mut hist = [0u32; 256];
    for p in small.pixels() {
        hist[p.0[0] as usize] += 1;
    }
    let total = (sw * sh) as f64;
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut sum_b, mut w_b, mut best_t, mut best_var) = (0f64, 0f64, 127u8, 0f64);
    for t in 0..256 {
        w_b += hist[t] as f64;
        if w_b == 0.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0.0 {
            break;
        }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b;
        let m_f = (sum_all - sum_b) / w_f;
        let var = w_b * w_f * (m_b - m_f) * (m_b - m_f);
        if var > best_var {
            best_var = var;
            best_t = t as u8;
        }
    }

    let mut dark: Vec<(f32, f32)> = Vec::new();
    for (x, y, p) in small.enumerate_pixels() {
        if p.0[0] < best_t {
            dark.push((x as f32, y as f32));
        }
    }
    if dark.len() < 100 {
        return 0.0;
    }
    // Cap the sample for speed; uniform stride keeps spatial coverage.
    if dark.len() > 60_000 {
        let stride = dark.len() / 60_000 + 1;
        dark = dark.into_iter().step_by(stride).collect();
    }

    let mut best_angle = 0.0f32;
    let mut best_score = f64::MIN;
    let mut angle = -5.0f32;
    while angle <= 5.0 {
        let tan = angle.to_radians().tan();
        let mut bins = vec![0u32; sh as usize + 1];
        for &(x, y) in &dark {
            let yy = (y - x * tan).round();
            if yy >= 0.0 && (yy as usize) < bins.len() {
                bins[yy as usize] += 1;
            }
        }
        let n = bins.len() as f64;
        let mean: f64 = bins.iter().map(|&b| b as f64).sum::<f64>() / n;
        let var: f64 = bins
            .iter()
            .map(|&b| {
                let d = b as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / n;
        if var > best_score {
            best_score = var;
            best_angle = angle;
        }
        angle += 0.25;
    }
    best_angle
}

/// Rotate around the image centre, bilinear sampling, white background.
fn rotate_gray(img: &GrayImage, angle_deg: f32) -> GrayImage {
    let (w, h) = img.dimensions();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let theta = angle_deg.to_radians();
    let (s, c) = (theta.sin(), theta.cos());
    let src = img.as_raw();
    let stride = w as usize;
    let mut out = GrayImage::from_pixel(w, h, image::Luma([255u8]));
    for y in 0..h {
        for x in 0..w {
            // Inverse mapping.
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let sx = c * dx + s * dy + cx;
            let sy = -s * dx + c * dy + cy;
            if sx < 0.0 || sy < 0.0 || sx >= (w - 1) as f32 || sy >= (h - 1) as f32 {
                continue;
            }
            let (x0, y0) = (sx as usize, sy as usize);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let p00 = src[y0 * stride + x0] as f32;
            let p01 = src[y0 * stride + x0 + 1] as f32;
            let p10 = src[(y0 + 1) * stride + x0] as f32;
            let p11 = src[(y0 + 1) * stride + x0 + 1] as f32;
            let v = p00 * (1.0 - fx) * (1.0 - fy)
                + p01 * fx * (1.0 - fy)
                + p10 * (1.0 - fx) * fy
                + p11 * fx * fy;
            out.as_mut()[(y * w + x) as usize] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

// ------------------------------------------------------------- recognition

fn rgba_to_gray(width: u32, height: u32, rgba: &[u8]) -> GrayImage {
    let mut g = GrayImage::new(width, height);
    for (i, px) in rgba.chunks_exact(4).enumerate() {
        // Rec. 601 luma.
        let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as u8;
        g.as_mut()[i] = l;
    }
    g
}

/// Run Tesseract on a grayscale page bitmap, return recognised words in
/// pixel coordinates (top-left origin).
fn run_tesseract_tsv(
    tesseract: &str,
    tessdata: Option<&Path>,
    lang: &str,
    img: &GrayImage,
    omp_limit: usize,
) -> Result<Vec<pdf_mutate::OcrWord>, String> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let png = std::env::temp_dir().join(format!(
        "docura-ocr-{}-{seq}.png",
        std::process::id()
    ));
    image::save_buffer(
        &png,
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::L8,
    )
    .map_err(|e| format!("write temp png: {e}"))?;

    let mut cmd = tesseract_command(tesseract);
    cmd.arg(&png)
        .arg("stdout")
        .args(["-l", lang])
        .args(["--psm", "3"])
        .args(["--dpi", &OCR_DPI.to_string()])
        // Cap Tesseract's own OpenMP pool so N parallel processes don't
        // oversubscribe the cores. One page → all cores; many pages → one
        // thread each (see ocr_pages_parallel).
        .env("OMP_THREAD_LIMIT", omp_limit.max(1).to_string());
    if let Some(dir) = tessdata {
        cmd.arg("--tessdata-dir").arg(dir);
    }
    // Request TSV via the config variable, not the `tsv` configfile: the
    // configfile lives in `<tessdata>/configs/tsv`, but a downloaded
    // language pack dir (passed as --tessdata-dir) has no `configs/`, so
    // the configfile silently falls back to plain-text output and the
    // parser below finds zero words. The `-c` form has no such dependency.
    let out = no_window(&mut cmd)
        .args(["-c", "tessedit_create_tsv=1"])
        .output();
    let _ = std::fs::remove_file(&png);
    let out = out.map_err(|e| format!("tesseract spawn: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tesseract: {}", err.trim()));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut words: Vec<pdf_mutate::OcrWord> = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let conf: f32 = cols[10].parse().unwrap_or(-1.0);
        let word = cols[11].trim();
        if conf < MIN_CONFIDENCE || word.is_empty() {
            continue;
        }
        let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
            cols[6].parse::<f32>(),
            cols[7].parse::<f32>(),
            cols[8].parse::<f32>(),
            cols[9].parse::<f32>(),
        ) else {
            continue;
        };
        words.push(pdf_mutate::OcrWord { text: word.to_string(), x, y, w, h });
    }
    Ok(words)
}

/// OCR one already-open page: render, preprocess, recognise.
#[allow(clippy::too_many_arguments)]
fn ocr_page(
    engine: &Arc<dyn PdfEngine>,
    document: DocumentId,
    page: u32,
    tesseract: &str,
    tessdata: Option<&Path>,
    lang: &str,
    preprocess: bool,
    omp_limit: usize,
) -> Result<(Vec<pdf_mutate::OcrWord>, u32, u32), String> {
    let r = engine
        .render(RenderRequest {
            document,
            page: PageIndex(page),
            zoom: ZoomLevel(OCR_DPI as f32 / 72.0),
            rotation: Rotation::R0,
        })
        .map_err(|e| e.to_string())?;
    let gray = rgba_to_gray(r.width_px, r.height_px, &r.rgba);
    let gray = if preprocess { preprocess_gray(gray) } else { gray };
    let words = run_tesseract_tsv(tesseract, tessdata, lang, &gray, omp_limit)?;
    Ok((words, gray.width(), gray.height()))
}

/// OCR many pages in parallel. PDFium rasterisation inside `ocr_page`
/// serialises on the engine's internal mutex, but the Tesseract processes are
/// independent and overlap across cores — the actual win over the old
/// page-by-page loop. Returns recognised pages (sorted) and the skip count.
#[allow(clippy::too_many_arguments)]
fn ocr_pages_parallel(
    engine: &Arc<dyn PdfEngine>,
    document: DocumentId,
    pages: &[u32],
    tesseract: &str,
    tessdata: Option<&Path>,
    lang: &str,
    preprocess: bool,
    skip_text_pages: bool,
    progress: &(dyn Fn(u32, u32, u32) + Sync),
) -> Result<(Vec<(u32, Vec<pdf_mutate::OcrWord>, u32, u32)>, u32), String> {
    let total = pages.len() as u32;
    if pages.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let workers = cores.min(pages.len()).min(OCR_MAX_WORKERS).max(1);
    // Split cores across workers for Tesseract's OpenMP pool: one page uses all
    // cores, many pages run one single-threaded process each.
    let omp_per_proc = (cores / workers).max(1);

    let next = AtomicUsize::new(0);
    let done = AtomicU32::new(0);
    let skipped = AtomicU32::new(0);
    let results: Mutex<Vec<(u32, Vec<pdf_mutate::OcrWord>, u32, u32)>> = Mutex::new(Vec::new());
    let first_err: Mutex<Option<String>> = Mutex::new(None);

    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                if first_err.lock().is_some() {
                    break;
                }
                let i = next.fetch_add(1, Relaxed);
                if i >= pages.len() {
                    break;
                }
                let page = pages[i];
                if skip_text_pages && page_has_text(engine, document, page) {
                    skipped.fetch_add(1, Relaxed);
                    progress(done.fetch_add(1, Relaxed) + 1, total, page);
                    continue;
                }
                match ocr_page(
                    engine, document, page, tesseract, tessdata, lang, preprocess, omp_per_proc,
                ) {
                    Ok((words, iw, ih)) => {
                        if !words.is_empty() {
                            results.lock().push((page, words, iw, ih));
                        }
                    }
                    Err(e) => {
                        *first_err.lock() = Some(e);
                    }
                }
                progress(done.fetch_add(1, Relaxed) + 1, total, page);
            });
        }
    });

    if let Some(e) = first_err.into_inner() {
        return Err(e);
    }
    let mut v = results.into_inner();
    v.sort_by_key(|r| r.0);
    Ok((v, skipped.load(Relaxed)))
}

fn page_has_text(engine: &Arc<dyn PdfEngine>, document: DocumentId, page: u32) -> bool {
    engine
        .extract_text(document, PageIndex(page))
        .map(|t| t.trim().len() > TEXT_PAGE_THRESHOLD)
        .unwrap_or(false)
}

// ----------------------------------------------------------------- commands

#[derive(Serialize, Clone)]
struct OcrProgress {
    current: u32,
    total: u32,
    page: u32,
    file: Option<String>,
}

#[derive(Serialize)]
pub struct OcrRunResult {
    snapshot: crate::DocSnapshot,
    pages_done: u32,
    pages_skipped: u32,
    words: u32,
}

#[derive(Deserialize)]
pub struct OcrArgs {
    /// Empty = all pages. 0-indexed.
    #[serde(default)]
    pages: Vec<u32>,
    lang: String,
    #[serde(default = "default_true")]
    preprocess: bool,
    /// Skip pages that already carry a text layer.
    #[serde(default = "default_true")]
    skip_text_pages: bool,
}
fn default_true() -> bool {
    true
}

/// Recognise text on the open document and inject the invisible OCR layer.
#[tauri::command]
pub async fn ocr_document(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    document: DocumentId,
    args: OcrArgs,
) -> Result<OcrRunResult, String> {
    let engine = state.engine.clone();
    let app2 = app.clone();
    let (pre_bytes, new_bytes, pages_done, pages_skipped, words_total) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>, u32, u32, u32), String> {
            let t = std::time::Instant::now();
            let tesseract = find_tesseract().ok_or("TESSERACT_MISSING")?;
            let tessdata = resolve_tessdata(&app2, &tesseract, &args.lang)?;

            let page_count = engine.page_count(document).map_err(|e| e.to_string())?;
            let pages: Vec<u32> = if args.pages.is_empty() {
                (0..page_count).collect()
            } else {
                args.pages.iter().copied().filter(|&p| p < page_count).collect()
            };

            let (results, skipped) = ocr_pages_parallel(
                &engine, document, &pages, &tesseract, tessdata.as_deref(),
                &args.lang, args.preprocess, args.skip_text_pages,
                &|done, total, page| {
                    let _ = app2.emit(
                        "ocr-progress",
                        OcrProgress { current: done, total, page: page + 1, file: None },
                    );
                },
            )?;
            if results.is_empty() {
                return Err("OCR_NO_TEXT_FOUND".into());
            }

            let bytes = engine.document_bytes(document).map_err(|e| e.to_string())?;
            let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
            let mut words_total = 0u32;
            for (page, words, iw, ih) in &results {
                mdoc.add_ocr_text_layer(*page, words, *iw, *ih)
                    .map_err(|e| e.to_string())?;
                words_total += words.len() as u32;
            }
            let out = mdoc.to_bytes().map_err(|e| e.to_string())?;
            tracing::info!(
                "ocr_document: {} pages ({} words, {} skipped) in {:?}",
                results.len(), words_total, skipped, t.elapsed()
            );
            Ok((bytes, out, results.len() as u32, skipped, words_total))
        })
        .await
        .map_err(|e| e.to_string())??;

    state.cache.lock().remove_document(document);
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine.reload_bytes(document, new_bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    state.undo.lock().record(document, pre_bytes);
    let snapshot = crate::snapshot_doc(state, document).await?;
    Ok(OcrRunResult { snapshot, pages_done, pages_skipped, words: words_total })
}

#[derive(Deserialize)]
pub struct OcrBatchArgs {
    paths: Vec<String>,
    out_dir: String,
    lang: String,
    #[serde(default = "default_true")]
    preprocess: bool,
}

#[derive(Serialize)]
pub struct OcrBatchResult {
    files_done: u32,
    files_skipped: u32,
    words: u32,
}

/// OCR a set of PDF files from disk; writes `<name>_ocr.pdf` into `out_dir`.
/// Encrypted inputs and files where nothing was recognised are skipped.
#[tauri::command]
pub async fn ocr_files_batch(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    args: OcrBatchArgs,
) -> Result<OcrBatchResult, String> {
    if args.paths.is_empty() {
        return Err("ocr_files_batch: no files".into());
    }
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || -> Result<OcrBatchResult, String> {
        let tesseract = find_tesseract().ok_or("TESSERACT_MISSING")?;
        let tessdata = resolve_tessdata(&app, &tesseract, &args.lang)?;
        let out_dir = Path::new(&args.out_dir);
        std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

        let file_count = args.paths.len() as u32;
        let mut done = 0u32;
        let mut skipped = 0u32;
        let mut words_total = 0u32;
        for (fi, path) in args.paths.iter().enumerate() {
            let name = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            let Ok(bytes) = std::fs::read(path) else {
                skipped += 1;
                continue;
            };
            if pdf_mutate::probably_encrypted(&bytes) {
                tracing::info!("ocr batch: skipping encrypted {name}");
                skipped += 1;
                continue;
            }
            let Ok(doc) = engine.open_bytes(bytes.clone()) else {
                skipped += 1;
                continue;
            };
            let _ = app.emit(
                "ocr-progress",
                OcrProgress {
                    current: fi as u32 + 1,
                    total: file_count,
                    page: 0,
                    file: Some(name.clone()),
                },
            );
            let result = (|| -> Result<(Vec<u8>, u32), String> {
                let page_count = engine.page_count(doc).map_err(|e| e.to_string())?;
                let pages: Vec<u32> = (0..page_count).collect();
                let (results, _skipped) = ocr_pages_parallel(
                    &engine, doc, &pages, &tesseract, tessdata.as_deref(),
                    &args.lang, args.preprocess, true,
                    &|_, _, _| {},
                )?;
                if results.is_empty() {
                    return Err("nothing recognised".into());
                }
                let mut mdoc =
                    pdf_mutate::MutableDoc::from_bytes(&bytes).map_err(|e| e.to_string())?;
                let mut words = 0u32;
                for (page, w, iw, ih) in &results {
                    mdoc.add_ocr_text_layer(*page, w, *iw, *ih).map_err(|e| e.to_string())?;
                    words += w.len() as u32;
                }
                Ok((mdoc.to_bytes().map_err(|e| e.to_string())?, words))
            })();
            let _ = engine.close(doc);
            match result {
                Ok((out_bytes, words)) => {
                    let stem = Path::new(path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "document".into());
                    let dest = out_dir.join(format!("{stem}_ocr.pdf"));
                    std::fs::write(&dest, out_bytes)
                        .map_err(|e| format!("write {}: {e}", dest.display()))?;
                    done += 1;
                    words_total += words;
                }
                Err(e) => {
                    tracing::info!("ocr batch: {name}: {e}");
                    skipped += 1;
                }
            }
        }
        Ok(OcrBatchResult { files_done: done, files_skipped: skipped, words: words_total })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Heuristic used for the "scanned document — run OCR?" banner: the first
/// few pages carry (next to) no extractable text.
#[tauri::command]
pub async fn detect_needs_ocr(
    state: tauri::State<'_, AppState>,
    document: DocumentId,
) -> Result<bool, String> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        let Ok(page_count) = engine.page_count(document) else {
            return Ok(false);
        };
        if page_count == 0 {
            return Ok(false);
        }
        let sample = page_count.min(5);
        let mut chars = 0usize;
        for p in 0..sample {
            if let Ok(t) = engine.extract_text(document, PageIndex(p)) {
                chars += t.trim().len();
            }
            if chars > 20 {
                return Ok(false);
            }
        }
        Ok(chars <= 20)
    })
    .await
    .map_err(|e: tokio::task::JoinError| e.to_string())?
}

#[tauri::command]
pub fn get_ocr_language(state: tauri::State<'_, AppState>) -> String {
    let guard = state.settings_path.lock();
    if let Some(path) = guard.as_ref() {
        let s = crate::load_settings(path);
        if let Some(l) = s.ocr_language {
            return l;
        }
    }
    "deu".to_string()
}

#[tauri::command]
pub fn set_ocr_language(state: tauri::State<'_, AppState>, lang: String) -> Result<(), String> {
    let guard = state.settings_path.lock();
    let Some(cfg) = guard.as_ref() else {
        return Err("settings path unavailable".into());
    };
    let mut s = crate::load_settings(cfg);
    s.ocr_language = Some(lang);
    crate::save_settings(cfg, &s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Document, Object};

    /// PDF with large visible Helvetica text — rendered it stands in for a
    /// clean scan.
    fn text_pdf(line1: &str, line2: &str) -> Vec<u8> {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let page_id = d.new_object_id();
        let cat_id = d.new_object_id();
        let font_id = d.new_object_id();
        let content_id = d.new_object_id();

        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type1".to_vec()));
        font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
        d.objects.insert(font_id, Object::Dictionary(font));

        let content = lopdf::Stream::new(
            Dictionary::new(),
            format!("BT /F1 36 Tf 72 700 Td ({line1}) Tj 0 -72 Td ({line2}) Tj ET").into_bytes(),
        );
        d.objects.insert(content_id, Object::Stream(content));

        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        let mut res = Dictionary::new();
        res.set("Font", Object::Dictionary(fonts));

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Dictionary(res));
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        d.objects.insert(page_id, Object::Dictionary(page));

        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set("Count", Object::Integer(1));
        d.objects.insert(pages_id, Object::Dictionary(pages));

        let mut cat = Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set("Pages", Object::Reference(pages_id));
        d.objects.insert(cat_id, Object::Dictionary(cat));
        d.trailer.set("Root", Object::Reference(cat_id));
        d.max_id = content_id.0;

        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        out
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn bundled_tesseract_is_preferred_and_reads_its_own_tessdata() {
        let exe = std::env::current_exe().unwrap();
        let bundled_dir = exe.parent().unwrap().join("tesseract");
        let bundled = bundled_dir.join("tesseract.exe");

        let candidates = tesseract_candidates();
        let pos = |p: &Path| candidates.iter().position(|c| c == p);
        let bundled_pos = pos(&bundled).expect("bundled path is a candidate");
        assert!(
            bundled_pos < pos(Path::new("tesseract.exe")).unwrap(),
            "bundled Tesseract must win over PATH: {candidates:?}"
        );

        assert_eq!(
            bundled_tessdata(Path::new(&bundled.to_string_lossy().to_string())),
            Some(bundled_dir.join("tessdata"))
        );
        assert_eq!(bundled_tessdata(Path::new("tesseract.exe")), None);
        assert_eq!(
            bundled_tessdata(Path::new(r"C:\Program Files\Tesseract-OCR\tesseract.exe")),
            None
        );
    }

    /// End-to-end against a real Tesseract install: render → preprocess →
    /// recognise → inject → extract. Skips when Tesseract is absent.
    #[test]
    fn tesseract_roundtrip() {
        let Some(tesseract) = find_tesseract() else {
            eprintln!("tesseract not installed — skipping e2e test");
            return;
        };
        let engine = pdf_pdfium::PdfiumEngine::new().expect("pdfium.dll available");
        let engine: Arc<dyn PdfEngine> = Arc::new(engine);

        let doc = engine
            .open_bytes(text_pdf("INVOICE 2026", "TOTAL 123,45 EUR"))
            .unwrap();
        let r = engine
            .render(RenderRequest {
                document: doc,
                page: PageIndex(0),
                zoom: ZoomLevel(OCR_DPI as f32 / 72.0),
                rotation: Rotation::R0,
            })
            .unwrap();
        let gray = preprocess_gray(rgba_to_gray(r.width_px, r.height_px, &r.rgba));
        let words = run_tesseract_tsv(&tesseract, None, "eng", &gray, 1).unwrap();
        engine.close(doc).unwrap();

        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert!(texts.contains(&"INVOICE"), "words: {texts:?}");
        assert!(texts.iter().any(|t| t.contains("123")), "words: {texts:?}");

        // The installer ships deu + eng; the app's default language is deu.
        if system_langs(&tesseract).iter().any(|l| l == "deu") {
            let de = run_tesseract_tsv(&tesseract, None, "deu+eng", &gray, 1).unwrap();
            assert!(
                de.iter().any(|w| w.text == "INVOICE"),
                "deu+eng words: {de:?}"
            );
        }
        // Boxes are inside the bitmap.
        for w in &words {
            assert!(w.x >= 0.0 && w.x + w.w <= gray.width() as f32 + 1.0);
            assert!(w.y >= 0.0 && w.y + w.h <= gray.height() as f32 + 1.0);
        }

        // Regression: a downloaded language pack is passed via
        // --tessdata-dir to a directory that has no `configs/` subdir.
        // TSV output must still come through (we request it with
        // `-c tessedit_create_tsv=1`, not the `tsv` configfile which lives
        // under configs/ and would silently fall back to plain text).
        let sys_pack = Path::new(&tesseract)
            .parent()
            .map(|p| p.join("tessdata").join("eng.traineddata"));
        if let Some(src) = sys_pack.filter(|p| p.exists()) {
            let tmp = std::env::temp_dir().join(format!("docura-ocr-test-td-{}", std::process::id()));
            std::fs::create_dir_all(&tmp).unwrap();
            std::fs::copy(&src, tmp.join("eng.traineddata")).unwrap();
            let td_words = run_tesseract_tsv(&tesseract, Some(&tmp), "eng", &gray, 1).unwrap();
            let _ = std::fs::remove_dir_all(&tmp);
            let td_texts: Vec<&str> = td_words.iter().map(|w| w.text.as_str()).collect();
            assert!(
                td_texts.contains(&"INVOICE"),
                "tessdata-dir TSV regression: {td_texts:?}"
            );
        }

        // Inject onto a blank page and verify the text extracts back.
        let blank = text_pdf(" ", " ");
        let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&blank).unwrap();
        mdoc.add_ocr_text_layer(0, &words, gray.width(), gray.height()).unwrap();
        let doc = engine.open_bytes(mdoc.to_bytes().unwrap()).unwrap();
        let text = engine.extract_text(doc, PageIndex(0)).unwrap();
        assert!(text.contains("INVOICE"), "extracted: {text:?}");
        engine.close(doc).unwrap();
    }
}
