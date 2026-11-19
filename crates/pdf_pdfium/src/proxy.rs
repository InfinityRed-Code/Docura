//! Render proxies for documents with huge raster images (scanned CAD plans).
//!
//! A scanned A0 plan at 720 dpi is a single ~33 700 × 23 800 px RGB image —
//! 2.4 GB once inflated. PDFium copes badly with that:
//!  - it refuses to inflate more than 1 GiB from one Flate stream, so
//!    everything past the first GiB of pixel rows renders **black**;
//!  - it does not cache such an image, so *every* render — thumbnail, fit
//!    page or 400 % — inflates it again (≈ 2–3 s each).
//!
//! For such documents the engine renders from derived copies while the
//! original bytes stay untouched for saving, signing and mutations:
//!  - `low`:  each huge image box-downsampled to ≈ 16 MP. Full-page renders
//!    up to [`RenderProxies::low_scale`] (fit page, thumbnails, normal
//!    reading) are fast.
//!  - `levels`: a pyramid of tiled copies — full resolution plus halved
//!    ones down towards `low` — where each huge image is a form XObject of
//!    1024-px-wide tiles. Every tile stays far below PDFium's limit, a
//!    clipped (zoomed-in) render only decodes the tiles it shows, and the
//!    engine picks the coarsest level that is still sharp at the zoom, so a
//!    region never decodes much more than ~4× its own pixels.
//!
//! Decoding is streamed (inflate + PNG predictor, one row at a time), tiles
//! are deflated in parallel, and results are cached by image content so a
//! reload after an edit (annotation, rotation, …) is instant.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// Images whose decoded size exceeds this get proxied.
const HUGE_DECODED_BYTES: usize = 96 << 20;
/// Pixel budget of a downsampled (`low`) image.
const LOW_TARGET_PX: usize = 16_000_000;
/// Tile width (and the height of a decoded band) in pixels.
const TILE: usize = 1024;
/// A pyramid level is only worth building when the next coarser copy is at
/// least this much smaller (linear).
const MIN_LEVEL_STEP: f64 = 1.6;
/// Decoded images kept for instant reloads.
const CACHE_ENTRIES: usize = 6;

pub(crate) struct RenderProxies {
    /// Document with every huge image downsampled.
    pub low: Vec<u8>,
    /// Zoom (1.0 = 72 dpi) up to which `low` is as sharp as the screen.
    pub low_scale: f32,
    /// Tiled documents from the coarsest to full resolution, each with the
    /// zoom up to which it is as sharp as the screen.
    pub levels: Vec<(f32, Vec<u8>)>,
}

struct Huge {
    id: ObjectId,
    w: usize,
    h: usize,
    comps: usize,
    png: bool,
    key: u64,
}

struct Processed {
    /// Downsampling factor of `low`.
    factor: usize,
    low_w: usize,
    low_h: usize,
    /// zlib-compressed `low_w × low_h × comps` samples.
    low: Vec<u8>,
    /// Tiled copies, finest first (`levels[0].f == 1`).
    levels: Vec<Level>,
}

/// One tiled copy of an image, downsampled by `f` (1 = full resolution).
struct Level {
    f: usize,
    w: usize,
    h: usize,
    tiles: Vec<Tile>,
}

/// Factors of the tiled pyramid for an image whose `low` copy is `factor`×
/// smaller: full resolution plus every power of two that still leaves a
/// worthwhile step to `low`.
fn level_factors(factor: usize) -> Vec<usize> {
    let mut v = vec![1];
    let mut f = 2;
    while factor as f64 / f as f64 >= MIN_LEVEL_STEP {
        v.push(f);
        f *= 2;
    }
    v
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

struct Tile {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    /// zlib-compressed samples.
    data: Vec<u8>,
}

static CACHE: Mutex<Vec<(u64, Arc<Processed>)>> = Mutex::new(Vec::new());

/// Build render proxies for `bytes`, or `None` when the document has no
/// image that needs one (the common case — costs one lopdf parse).
pub(crate) fn build(bytes: &[u8]) -> Option<RenderProxies> {
    let t0 = std::time::Instant::now();
    let doc = Document::load_mem(bytes).ok()?;
    if doc.trailer.has(b"Encrypt") {
        // Streams are encrypted; the app flattens protected documents to a
        // plaintext copy and reloads, which builds the proxies then.
        return None;
    }
    let huge = find_huge(&doc);
    if huge.is_empty() {
        return None;
    }

    // Decode every huge image (in parallel — they are independent).
    let processed: Vec<Option<Arc<Processed>>> = std::thread::scope(|s| {
        let handles: Vec<_> = huge
            .iter()
            .map(|hg| {
                let doc = &doc;
                s.spawn(move || cached_or_process(doc, hg))
            })
            .collect();
        handles.into_iter().map(|h| h.join().ok().flatten()).collect()
    });

    let pairs: Vec<(&Huge, Arc<Processed>)> =
        huge.iter().zip(processed).filter_map(|(hg, p)| p.map(|p| (hg, p))).collect();
    if pairs.is_empty() {
        return None;
    }
    // Zoom at which a copy `long` pixels on its long side is 1:1 on screen.
    // Without a usable MediaBox, scale relative to the largest `low` copy.
    let page_long = max_page_long_side(&doc)
        .unwrap_or_else(|| pairs.iter().map(|(_, p)| p.low_w.max(p.low_h)).max().unwrap_or(1) as f32);
    let scale = |long: usize| long as f32 / page_long;

    let mut low_doc = doc.clone();
    let mut low_scale = f32::INFINITY;
    for (hg, p) in &pairs {
        replace_low(&mut low_doc, hg, p)?;
        low_scale = low_scale.min(scale(p.low_w.max(p.low_h)));
    }
    let mut low = Vec::new();
    low_doc.save_to(&mut low).ok()?;
    drop(low_doc);

    // Pyramid level j holds every image at the largest of its own factors
    // that is ≤ 2^j (its `low` copy where that is finer than its tiles).
    let n_levels = pairs.iter().map(|(_, p)| p.levels.len()).max().unwrap_or(1);
    let mut levels = Vec::with_capacity(n_levels);
    for j in (0..n_levels).rev() {
        let target = 1usize << j;
        let mut d = doc.clone();
        let mut max_zoom = f32::INFINITY;
        for (hg, p) in &pairs {
            let lvl = p.levels.iter().rev().find(|l| l.f <= target)?;
            if p.factor <= target && p.factor > lvl.f {
                replace_low(&mut d, hg, p)?;
                max_zoom = max_zoom.min(scale(p.low_w.max(p.low_h)));
            } else {
                replace_tiles(&mut d, hg, lvl)?;
                max_zoom = max_zoom.min(scale(lvl.w.max(lvl.h)));
            }
        }
        flatten_annotations(&mut d);
        let mut bytes = Vec::new();
        d.save_to(&mut bytes).ok()?;
        levels.push((max_zoom, bytes));
    }
    tracing::info!(
        "render proxies: {} huge image(s), low_scale={low_scale:.2}, low={:.1}MB, levels [{}] in {:?}",
        pairs.len(),
        low.len() as f64 / 1048576.0,
        levels
            .iter()
            .map(|(z, b)| format!("≤{z:.2}: {:.1}MB", b.len() as f64 / 1048576.0))
            .collect::<Vec<_>>()
            .join(", "),
        t0.elapsed()
    );
    Some(RenderProxies { low, low_scale, levels })
}

fn cached_or_process(doc: &Document, hg: &Huge) -> Option<Arc<Processed>> {
    if let Some(p) = CACHE.lock().ok()?.iter().find(|(k, _)| *k == hg.key).map(|(_, p)| p.clone()) {
        return Some(p);
    }
    let stream = doc.get_object(hg.id).ok()?.as_stream().ok()?;
    let t = std::time::Instant::now();
    match process(&stream.content, hg) {
        Ok(p) => {
            tracing::info!(
                "render proxies: decoded {:?} {}x{} → low {}x{}, levels {:?} in {:?}",
                hg.id,
                hg.w,
                hg.h,
                p.low_w,
                p.low_h,
                p.levels.iter().map(|l| (l.f, l.tiles.len())).collect::<Vec<_>>(),
                t.elapsed()
            );
            let p = Arc::new(p);
            if let Ok(mut c) = CACHE.lock() {
                c.retain(|(k, _)| *k != hg.key);
                c.push((hg.key, p.clone()));
                while c.len() > CACHE_ENTRIES {
                    c.remove(0);
                }
            }
            Some(p)
        }
        Err(e) => {
            tracing::warn!("render proxies: cannot decode {:?}: {e}", hg.id);
            None
        }
    }
}

// ---------- Detection ----------

fn name_of(o: &Object) -> Option<&[u8]> {
    o.as_name().ok()
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(o),
        _ => o,
    }
}

fn components(doc: &Document, cs: &Object) -> Option<usize> {
    match resolve(doc, cs) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(1),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        },
        Object::Array(a) => {
            let family = name_of(a.first()?)?;
            match family {
                b"ICCBased" => {
                    let s = resolve(doc, a.get(1)?).as_stream().ok()?;
                    s.dict.get(b"N").ok()?.as_i64().ok().map(|n| n as usize)
                }
                b"CalRGB" => Some(3),
                b"CalGray" => Some(1),
                _ => None,
            }
        }
        _ => None,
    }
}

fn find_huge(doc: &Document) -> Vec<Huge> {
    let mut out = Vec::new();
    for (&id, obj) in &doc.objects {
        let Ok(s) = obj.as_stream() else { continue };
        let d = &s.dict;
        if d.get(b"Subtype").ok().and_then(name_of) != Some(b"Image") {
            continue;
        }
        // Only plain Flate images with 8-bit samples; masks and unusual
        // encodings stay with PDFium.
        let flate = match d.get(b"Filter").map(|o| resolve(doc, o)) {
            Ok(Object::Name(n)) => n.as_slice() == b"FlateDecode",
            Ok(Object::Array(a)) => a.len() == 1 && a.first().and_then(name_of) == Some(b"FlateDecode"),
            _ => false,
        };
        if !flate
            || d.get(b"BitsPerComponent").and_then(|o| o.as_i64()).unwrap_or(0) != 8
            || d.get(b"ImageMask").and_then(|o| o.as_bool()).unwrap_or(false)
            || d.has(b"SMask")
            || d.has(b"Mask")
        {
            continue;
        }
        let (Ok(w), Ok(h)) = (
            d.get(b"Width").and_then(|o| o.as_i64()),
            d.get(b"Height").and_then(|o| o.as_i64()),
        ) else {
            continue;
        };
        if w <= 0 || h <= 0 {
            continue;
        }
        let (w, h) = (w as usize, h as usize);
        let Some(comps) = d.get(b"ColorSpace").ok().and_then(|cs| components(doc, cs)) else {
            continue;
        };
        if w.saturating_mul(h).saturating_mul(comps) <= HUGE_DECODED_BYTES {
            continue;
        }
        // Predictor: none or PNG (10–15). TIFF predictor 2 is rare for scans.
        let parms = match d.get(b"DecodeParms").map(|o| resolve(doc, o)) {
            Ok(Object::Dictionary(p)) => Some(p.clone()),
            Ok(Object::Array(a)) => a.first().and_then(|o| resolve(doc, o).as_dict().ok()).cloned(),
            _ => None,
        };
        let int = |k: &[u8], dflt: i64| {
            parms
                .as_ref()
                .and_then(|p| p.get(k).ok())
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(dflt)
        };
        let predictor = int(b"Predictor", 1);
        let png = predictor >= 10;
        if !(predictor == 1 || png)
            || (png
                && (int(b"Colors", 1) as usize != comps
                    || int(b"BitsPerComponent", 8) != 8
                    || int(b"Columns", 1) as usize != w))
        {
            continue;
        }
        let mut hasher = DefaultHasher::new();
        s.content.hash(&mut hasher);
        (w, h, comps, png).hash(&mut hasher);
        out.push(Huge { id, w, h, comps, png, key: hasher.finish() });
    }
    out
}

fn max_page_long_side(doc: &Document) -> Option<f32> {
    let mut best = 0.0f32;
    for (_, page) in doc.get_pages() {
        // /MediaBox is inheritable: walk up the page tree.
        let mut cur = Some(page);
        let mut depth = 0;
        while let Some(id) = cur {
            let Ok(d) = doc.get_dictionary(id) else { break };
            if let Ok(mb) = d.get(b"MediaBox") {
                if let Ok(a) = resolve(doc, mb).as_array() {
                    let v: Vec<f32> = a
                        .iter()
                        .filter_map(|o| match resolve(doc, o) {
                            Object::Integer(i) => Some(*i as f32),
                            Object::Real(r) => Some(*r),
                            _ => None,
                        })
                        .collect();
                    if v.len() == 4 {
                        best = best.max((v[2] - v[0]).abs()).max((v[3] - v[1]).abs());
                    }
                }
                break;
            }
            cur = d.get(b"Parent").and_then(|o| o.as_reference()).ok();
            depth += 1;
            if depth > 64 {
                break;
            }
        }
    }
    (best > 0.0).then_some(best)
}

// ---------- Decoding ----------

#[cfg(test)]
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let pa = (p - a as i16).abs();
    let pb = (p - b as i16).abs();
    let pc = (p - c as i16).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undo a PNG row filter in place (`prev` is the previous decoded row).
fn unfilter(kind: u8, row: &mut [u8], prev: &[u8], bpp: usize) -> Result<(), String> {
    match kind {
        0 => {}
        1 => {
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(row[i - bpp]);
            }
        }
        2 => {
            for (r, p) in row.iter_mut().zip(prev) {
                *r = r.wrapping_add(*p);
            }
        }
        3 => {
            for i in 0..row.len() {
                let left = if i >= bpp { row[i - bpp] as u16 } else { 0 };
                row[i] = row[i].wrapping_add(((left + prev[i] as u16) / 2) as u8);
            }
        }
        4 => {
            // First pixel: left and upper-left are 0, so Paeth picks "up".
            let lead = bpp.min(row.len());
            for i in 0..lead {
                row[i] = row[i].wrapping_add(prev[i]);
            }
            for i in bpp..row.len() {
                let a = row[i - bpp] as i16;
                let b = prev[i] as i16;
                let c = prev[i - bpp] as i16;
                // |p-a| = |b-c|, |p-b| = |a-c|, |p-c| = |a+b-2c| with p = a+b-c.
                let pa = (b - c).abs();
                let pb = (a - c).abs();
                let pc = (a + b - 2 * c).abs();
                let pred = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                row[i] = row[i].wrapping_add(pred as u8);
            }
        }
        other => return Err(format!("bad PNG filter type {other}")),
    }
    Ok(())
}

/// Ask Windows to run the calling thread at full speed. Decoding and
/// rendering a plan is work the user is waiting for, but while the window
/// isn't focused (switched away during a load, or ctrl+wheel over the
/// window on a second monitor) the scheduler treats the app as low-QoS and
/// parks its threads on efficiency cores at reduced clocks: 3-4x slower on
/// hybrid (P/E-core) CPUs.
#[cfg(windows)]
pub(crate) fn full_speed() {
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadInformation, ThreadPowerThrottling, THREAD_POWER_THROTTLING_CURRENT_VERSION,
        THREAD_POWER_THROTTLING_EXECUTION_SPEED, THREAD_POWER_THROTTLING_STATE,
    };
    let state = THREAD_POWER_THROTTLING_STATE {
        Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
        // Mechanism selected but off = opt out of throttling ("HighQoS").
        StateMask: 0,
    };
    // SAFETY: valid pseudo-handle and a correctly sized, initialised struct.
    // Best effort: older Windows versions reject the class, which is fine.
    unsafe {
        let _ = SetThreadInformation(
            GetCurrentThread(),
            ThreadPowerThrottling,
            &state as *const THREAD_POWER_THROTTLING_STATE as *const core::ffi::c_void,
            std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

#[cfg(not(windows))]
pub(crate) fn full_speed() {}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::with_capacity(data.len() / 8), flate2::Compression::fast());
    // Writing into a Vec cannot fail.
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

fn process(compressed: &[u8], hg: &Huge) -> Result<Processed, String> {
    process_with(compressed, hg, LOW_TARGET_PX)
}

fn process_with(compressed: &[u8], hg: &Huge, low_target: usize) -> Result<Processed, String> {
    full_speed();
    let (w, h, comps) = (hg.w, hg.h, hg.comps);
    let factor = (((w * h) as f64 / low_target as f64).sqrt().ceil() as usize).max(1);
    let (low_w, low_h) = (w.div_ceil(factor), h.div_ceil(factor));
    let fs = level_factors(factor);
    let row_len = w * comps;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    // Bands are a whole number of blocks of every reduction high, so each
    // band can be reduced on its own (in parallel with decoding the next).
    let max_f = *fs.last().expect("level 1 always exists");
    let unit = factor / gcd(factor, max_f) * max_f;
    let band_h = ((TILE / unit) * unit).max(unit);

    // Decoding (inflate + PNG unfilter) is inherently sequential; it runs
    // here while a worker reduces/tiles the previous band on all cores.
    std::thread::scope(|s| -> Result<Processed, String> {
        let (band_tx, band_rx) = std::sync::mpsc::sync_channel::<(Vec<u8>, usize, usize)>(1);
        let (free_tx, free_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let fs_worker = fs.clone();
        let worker = s.spawn(move || {
            full_speed();
            let mut low = Vec::with_capacity(low_w * low_h * comps);
            let mut tiles: Vec<Vec<Tile>> = fs_worker.iter().map(|_| Vec::new()).collect();
            for (band, y0, rows) in band_rx {
                let (l, t) =
                    reduce_band(&band[..rows * row_len], y0, rows, w, comps, factor, &fs_worker, threads);
                low.extend_from_slice(&l);
                for (acc, lt) in tiles.iter_mut().zip(t) {
                    acc.extend(lt);
                }
                let _ = free_tx.send(band);
            }
            (low, tiles)
        });

        let mut dec = flate2::read::ZlibDecoder::new(compressed);
        let mut raw = vec![0u8; row_len + hg.png as usize];
        let mut prev = vec![0u8; row_len];
        let mut band = vec![0u8; band_h * row_len];
        let (mut rows, mut y0) = (0usize, 0usize);
        let mut result = Ok(());
        for y in 0..h {
            if let Err(e) = dec.read_exact(&mut raw) {
                result = Err(format!("inflate row {y}: {e}"));
                break;
            }
            let dst = &mut band[rows * row_len..(rows + 1) * row_len];
            if hg.png {
                let (kind, data) = raw.split_first_mut().expect("row has filter byte");
                if let Err(e) = unfilter(*kind, data, &prev, comps) {
                    result = Err(e);
                    break;
                }
                prev.copy_from_slice(data);
                dst.copy_from_slice(data);
            } else {
                dst.copy_from_slice(&raw);
            }
            rows += 1;
            if rows == band_h || y + 1 == h {
                let next = free_rx.try_recv().unwrap_or_else(|_| vec![0u8; band_h * row_len]);
                let full = std::mem::replace(&mut band, next);
                if band_tx.send((full, y0, rows)).is_err() {
                    break;
                }
                y0 += rows;
                rows = 0;
            }
        }
        drop(band_tx);
        let (low, tiles) = worker.join().map_err(|_| "proxy worker panicked".to_string())?;
        result?;
        let levels = fs
            .iter()
            .zip(tiles)
            .map(|(&f, tiles)| Level { f, w: w.div_ceil(f), h: h.div_ceil(f), tiles })
            .collect();
        Ok(Processed { factor, low_w, low_h, low: zlib(&low), levels })
    })
}

/// Area-average `src` (`w` × `rows` pixels) over `k × k` blocks (partial
/// blocks at the right/bottom edge average what they cover). Output columns
/// are split across `threads`.
fn box_reduce(src: &[u8], w: usize, rows: usize, comps: usize, k: usize, threads: usize) -> (Vec<u8>, usize, usize) {
    let row_len = w * comps;
    let out_w = w.div_ceil(k);
    let out_rows = rows.div_ceil(k);
    if k == 1 {
        return (src[..rows * row_len].to_vec(), w, rows);
    }
    let per = out_w.div_ceil(threads).max(1);
    let parts: Vec<(usize, usize, Vec<u8>)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..out_w)
            .step_by(per)
            .map(|ox0| {
                s.spawn(move || {
                    full_speed();
                    let ox1 = (ox0 + per).min(out_w);
                    let mut out = vec![0u8; out_rows * (ox1 - ox0) * comps];
                    let mut acc = vec![0u32; (ox1 - ox0) * comps];
                    for ly in 0..out_rows {
                        let r0 = ly * k;
                        let r1 = (r0 + k).min(rows);
                        acc.fill(0);
                        for r in r0..r1 {
                            let row = &src[r * row_len..(r + 1) * row_len];
                            for ox in ox0..ox1 {
                                let x0 = ox * k;
                                let x1 = (x0 + k).min(w);
                                let a = &mut acc[(ox - ox0) * comps..(ox - ox0 + 1) * comps];
                                let px = &row[x0 * comps..x1 * comps];
                                if comps == 3 {
                                    let (mut s0, mut s1, mut s2) = (0u32, 0u32, 0u32);
                                    for p in px.chunks_exact(3) {
                                        s0 += p[0] as u32;
                                        s1 += p[1] as u32;
                                        s2 += p[2] as u32;
                                    }
                                    a[0] += s0;
                                    a[1] += s1;
                                    a[2] += s2;
                                } else {
                                    for p in px.chunks_exact(comps) {
                                        for c in 0..comps {
                                            a[c] += p[c] as u32;
                                        }
                                    }
                                }
                            }
                        }
                        let rows_in = (r1 - r0) as u32;
                        let base = ly * (ox1 - ox0) * comps;
                        for ox in ox0..ox1 {
                            let cols = ((ox * k + k).min(w) - ox * k) as u32;
                            let n = cols * rows_in;
                            for c in 0..comps {
                                out[base + (ox - ox0) * comps + c] =
                                    ((acc[(ox - ox0) * comps + c] + n / 2) / n) as u8;
                            }
                        }
                    }
                    (ox0, ox1, out)
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    // Stitch the column ranges back into row-major rows.
    let mut out = vec![0u8; out_rows * out_w * comps];
    for (ox0, ox1, part) in &parts {
        let pw = (ox1 - ox0) * comps;
        for ly in 0..out_rows {
            let dst = ly * out_w * comps + ox0 * comps;
            out[dst..dst + pw].copy_from_slice(&part[ly * pw..(ly + 1) * pw]);
        }
    }
    (out, out_w, out_rows)
}

/// Reduce one decoded band (rows `y0..y0 + rows` of the image): its rows of
/// the `low` copy and its tiles of every pyramid level in `fs` (finest
/// first, 1 then powers of two). Halved levels are computed from each
/// other; tiles are deflated across `threads`.
#[allow(clippy::too_many_arguments)]
fn reduce_band(
    band: &[u8],
    y0: usize,
    rows: usize,
    w: usize,
    comps: usize,
    factor: usize,
    fs: &[usize],
    threads: usize,
) -> (Vec<u8>, Vec<Vec<Tile>>) {
    // (factor, samples, width, rows) of every level of this band.
    let mut lv: Vec<(usize, std::borrow::Cow<[u8]>, usize, usize)> =
        vec![(1, std::borrow::Cow::Borrowed(band), w, rows)];
    for &f in &fs[1..] {
        let (pf, src, pw, pr) = lv.last().expect("level 1");
        let (out, ow, or) = box_reduce(src, *pw, *pr, comps, f / pf, threads);
        lv.push((f, std::borrow::Cow::Owned(out), ow, or));
    }
    let low = {
        let (lf, src, lw, lr) = lv.last().expect("level 1");
        if factor.is_multiple_of(*lf) {
            box_reduce(src, *lw, *lr, comps, factor / lf, threads).0
        } else {
            box_reduce(band, w, rows, comps, factor, threads).0
        }
    };

    // Tile jobs over all levels, split across threads.
    let jobs: Vec<(usize, usize)> = lv
        .iter()
        .enumerate()
        .flat_map(|(li, (_, _, lw, _))| (0..lw.div_ceil(TILE)).map(move |c| (li, c)))
        .collect();
    let per = jobs.len().div_ceil(threads).max(1);
    let mut tiles: Vec<Vec<Tile>> = lv.iter().map(|_| Vec::new()).collect();
    let done: Vec<Vec<(usize, Tile)>> = std::thread::scope(|s| {
        let lv = &lv;
        let handles: Vec<_> = jobs
            .chunks(per)
            .map(|chunk| {
                s.spawn(move || {
                    full_speed();
                    chunk
                        .iter()
                        .map(|&(li, c)| {
                            let (f, src, lw, lr) = &lv[li];
                            let row_len = lw * comps;
                            let x0 = c * TILE;
                            let tw = TILE.min(lw - x0);
                            let mut buf = Vec::with_capacity(tw * comps * lr);
                            for r in 0..*lr {
                                let start = r * row_len + x0 * comps;
                                buf.extend_from_slice(&src[start..start + tw * comps]);
                            }
                            (li, Tile { x: x0, y: y0 / f, w: tw, h: *lr, data: zlib(&buf) })
                        })
                        .collect()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    for (li, t) in done.into_iter().flatten() {
        tiles[li].push(t);
    }
    (low, tiles)
}

// ---------- Document rewriting ----------

/// Image dictionary entries that describe the samples themselves and must
/// not be copied onto a re-encoded replacement.
const SAMPLE_KEYS: &[&[u8]] = &[b"Length", b"Filter", b"DecodeParms", b"Width", b"Height", b"DL"];

fn sample_free_dict(orig: &Dictionary) -> Dictionary {
    let mut d = Dictionary::new();
    for (k, v) in orig.iter() {
        if !SAMPLE_KEYS.contains(&k.as_slice()) {
            d.set(k.clone(), v.clone());
        }
    }
    d
}

fn image_stream(base: &Dictionary, w: usize, h: usize, data: Vec<u8>) -> Stream {
    let mut d = base.clone();
    d.set("Width", w as i64);
    d.set("Height", h as i64);
    d.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    Stream::new(d, data).with_compression(false)
}

fn replace_low(doc: &mut Document, hg: &Huge, p: &Processed) -> Option<()> {
    let base = sample_free_dict(&doc.get_object(hg.id).ok()?.as_stream().ok()?.dict);
    doc.objects.insert(hg.id, Object::Stream(image_stream(&base, p.low_w, p.low_h, p.low.clone())));
    Some(())
}

/// Replace the image by a form XObject drawing `lvl`'s tiles over the
/// image's unit square.
fn replace_tiles(doc: &mut Document, hg: &Huge, lvl: &Level) -> Option<()> {
    let orig = doc.get_object(hg.id).ok()?.as_stream().ok()?.dict.clone();
    let mut base = sample_free_dict(&orig);
    // Optional-content membership belongs to the placed form, not each tile.
    let oc = base.remove(b"OC");
    base.remove(b"Metadata");
    base.remove(b"StructParent");
    let (fw, fh) = (lvl.w as f64, lvl.h as f64);
    let mut xobjects = Dictionary::new();
    let mut content = String::with_capacity(lvl.tiles.len() * 64);
    for (i, t) in lvl.tiles.iter().enumerate() {
        let tid = doc.add_object(Object::Stream(image_stream(&base, t.w, t.h, t.data.clone())));
        xobjects.set(format!("T{i}"), Object::Reference(tid));
        // Stretch inner tiles by half a pixel right/down: the next tile
        // (drawn later) covers the overlap, so no hairline seams appear.
        let ew = if t.x + t.w < lvl.w { 0.5 } else { 0.0 };
        let eh = if t.y + t.h < lvl.h { 0.5 } else { 0.0 };
        let sx = (t.w as f64 + ew) / fw;
        let sy = (t.h as f64 + eh) / fh;
        let tx = t.x as f64 / fw;
        // Image space: row 0 is the top edge (y = 1) of the unit square.
        let ty = 1.0 - (t.y as f64 + t.h as f64 + eh) / fh;
        content.push_str(&format!("q {sx:.9} 0 0 {sy:.9} {tx:.9} {ty:.9} cm /T{i} Do Q
"));
    }
    let mut res = Dictionary::new();
    res.set("XObject", Object::Dictionary(xobjects));
    let mut form = Dictionary::new();
    form.set("Type", Object::Name(b"XObject".to_vec()));
    form.set("Subtype", Object::Name(b"Form".to_vec()));
    form.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), 1.into(), 1.into()]),
    );
    form.set("Resources", Object::Dictionary(res));
    if let Some(oc) = oc {
        form.set("OC", oc);
    }
    doc.objects.insert(hg.id, Object::Stream(Stream::new(form, content.into_bytes())));
    Some(())
}

// ---------- Annotation flattening (high proxy only) ----------

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn nums(doc: &Document, o: &Object, n: usize) -> Option<Vec<f64>> {
    let a = resolve(doc, o).as_array().ok()?;
    let v: Vec<f64> = a.iter().filter_map(|x| num(resolve(doc, x))).collect();
    (v.len() == n).then_some(v)
}

/// Normal appearance stream of an annotation (honouring /AS for state dicts).
fn normal_appearance(doc: &Document, annot: &Dictionary) -> Option<ObjectId> {
    let ap = resolve(doc, annot.get(b"AP").ok()?).as_dict().ok()?;
    match ap.get(b"N").ok()? {
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Stream(_) => Some(*r),
            Object::Dictionary(states) => {
                let state = annot.get(b"AS").ok().and_then(name_of)?;
                states.get(state).ok()?.as_reference().ok()
            }
            _ => None,
        },
        Object::Dictionary(states) => {
            let state = annot.get(b"AS").ok().and_then(name_of)?;
            states.get(state).ok()?.as_reference().ok()
        }
        _ => None,
    }
}

/// Draw every visible annotation appearance into its page's content and
/// drop the annotation. PDFium only paints form widgets (e.g. a visible
/// signature) through its form layer, which clipped renders can't use —
/// so the render-only `high` proxy carries them as page content instead.
fn flatten_annotations(doc: &mut Document) {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in pages {
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        let annots: Vec<Object> = match page.get(b"Annots").map(|o| resolve(doc, o)) {
            Ok(Object::Array(a)) => a.clone(),
            _ => continue,
        };
        let mut keep = Vec::new();
        let mut draws: Vec<(ObjectId, [f64; 6])> = Vec::new();
        for a in annots {
            let Ok(ad) = resolve(doc, &a).as_dict() else {
                keep.push(a);
                continue;
            };
            let flags = ad.get(b"F").and_then(|o| o.as_i64()).unwrap_or(0);
            let hidden = flags & (2 | 32) != 0; // Hidden | NoView
            let placed = (|| {
                let ap = normal_appearance(doc, ad)?;
                let rect = nums(doc, ad.get(b"Rect").ok()?, 4)?;
                let form = doc.get_object(ap).ok()?.as_stream().ok()?;
                let bbox = nums(doc, form.dict.get(b"BBox").ok()?, 4)?;
                let m = form
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|o| nums(doc, o, 6))
                    .unwrap_or_else(|| vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                // PDF 32000 §12.5.5: map the transformed BBox onto /Rect.
                let pts = [(bbox[0], bbox[1]), (bbox[2], bbox[1]), (bbox[0], bbox[3]), (bbox[2], bbox[3])];
                let tx: Vec<(f64, f64)> = pts
                    .iter()
                    .map(|&(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]))
                    .collect();
                let (bx0, bx1) = tx.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
                let (by0, by1) = tx.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
                let (rx0, rx1) = (rect[0].min(rect[2]), rect[0].max(rect[2]));
                let (ry0, ry1) = (rect[1].min(rect[3]), rect[1].max(rect[3]));
                if bx1 - bx0 <= 0.0 || by1 - by0 <= 0.0 {
                    return None;
                }
                let sx = (rx1 - rx0) / (bx1 - bx0);
                let sy = (ry1 - ry0) / (by1 - by0);
                Some((ap, [sx, 0.0, 0.0, sy, rx0 - bx0 * sx, ry0 - by0 * sy]))
            })();
            match placed {
                Some(d) if !hidden => draws.push(d),
                Some(_) => {}
                None => keep.push(a),
            }
        }
        if draws.is_empty() {
            continue;
        }
        // Own (possibly inherited) resources + the appearance XObjects.
        let mut res = inherited_resources(doc, page_id).unwrap_or_default();
        let mut xobjs = res
            .get(b"XObject")
            .ok()
            .and_then(|o| resolve(doc, o).as_dict().ok().cloned())
            .unwrap_or_default();
        let mut content = String::from("Q\n");
        for (i, (ap, m)) in draws.iter().enumerate() {
            let name = format!("DocuraAP{i}");
            xobjs.set(name.as_bytes().to_vec(), Object::Reference(*ap));
            content.push_str(&format!(
                "q {} {} {} {} {} {} cm /{name} Do Q\n",
                m[0], m[1], m[2], m[3], m[4], m[5]
            ));
        }
        res.set("XObject", Object::Dictionary(xobjs));
        // Wrap the original content in q … Q so its graphics state can't
        // leak into the appended appearances.
        let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let close = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        let mut contents = vec![Object::Reference(open)];
        match page.get(b"Contents").map(|o| (o.clone(), resolve(doc, o).clone())) {
            Ok((_, Object::Array(a))) => contents.extend(a),
            Ok((orig, _)) => contents.push(orig),
            Err(_) => {}
        }
        contents.push(Object::Reference(close));
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            page.set("Contents", Object::Array(contents));
            page.set("Resources", Object::Dictionary(res));
            page.set("Annots", Object::Array(keep));
        }
    }
}

fn inherited_resources(doc: &Document, page_id: ObjectId) -> Option<Dictionary> {
    let mut cur = Some(page_id);
    let mut depth = 0;
    while let Some(id) = cur {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(r) = d.get(b"Resources") {
            return resolve(doc, r).as_dict().ok().cloned();
        }
        cur = d.get(b"Parent").and_then(|o| o.as_reference()).ok();
        depth += 1;
        if depth > 64 {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// A one-page PDF whose page is covered by a single `w × h` RGB image
    /// (PNG predictor, Up filter) with a diagonal gradient.
    fn huge_pdf(w: usize, h: usize) -> Vec<u8> {
        let mut raw = Vec::with_capacity((w * 3 + 1) * h);
        let mut prev = vec![0u8; w * 3];
        for y in 0..h {
            raw.push(2u8); // Up
            let row: Vec<u8> = (0..w * 3).map(|i| ((i / 3 + y) % 251) as u8).collect();
            raw.extend(row.iter().zip(&prev).map(|(c, p)| c.wrapping_sub(*p)));
            prev = row;
        }
        let mut doc = Document::with_version("1.7");
        let img = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => w as i64, "Height" => h as i64,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
                "Filter" => "FlateDecode",
                "DecodeParms" => dictionary! { "Predictor" => 15, "Colors" => 3, "Columns" => w as i64 },
            },
            zlib(&raw),
        ).with_compression(false));
        let content = doc.add_object(Stream::new(dictionary! {}, b"q 600 0 0 400 0 0 cm /Im0 Do Q".to_vec()));
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 400.into()],
            "Contents" => content,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => img } },
        });
        doc.objects.insert(pages, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }));
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", cat);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn small_documents_get_no_proxies() {
        assert!(build(&huge_pdf(64, 32)).is_none());
    }

    #[test]
    fn png_unfilter_round_trips_all_filter_types() {
        let prev = [10u8, 20, 30, 40, 50, 60];
        let orig = [15u8, 25, 35, 200, 210, 220];
        for kind in 0..=4u8 {
            // Encode with the same arithmetic the decoder inverts.
            let mut enc = orig;
            for i in (0..orig.len()).rev() {
                let left = if i >= 3 { orig[i - 3] } else { 0 };
                let ul = if i >= 3 { prev[i - 3] } else { 0 };
                let pred = match kind {
                    0 => 0,
                    1 => left,
                    2 => prev[i],
                    3 => ((left as u16 + prev[i] as u16) / 2) as u8,
                    _ => paeth(left, prev[i], ul),
                };
                enc[i] = orig[i].wrapping_sub(pred);
            }
            unfilter(kind, &mut enc, &prev, 3).unwrap();
            assert_eq!(enc, orig, "filter {kind}");
        }
    }

    #[test]
    fn process_tiles_cover_the_image_and_match_samples() {
        let (w, h) = (2500usize, 1300usize);
        let pdf = huge_pdf(w, h);
        let doc = Document::load_mem(&pdf).unwrap();
        let id = *doc
            .objects
            .iter()
            .find(|(_, o)| o.as_stream().map(|s| s.dict.has(b"Width")).unwrap_or(false))
            .unwrap()
            .0;
        let hg = Huge { id, w, h, comps: 3, png: true, key: 1 };
        let p = process(&doc.get_object(id).unwrap().as_stream().unwrap().content, &hg).unwrap();
        // Under the pixel budget: factor 1, a single full-resolution level.
        assert_eq!(p.levels.len(), 1);
        let tiles = &p.levels[0].tiles;
        // 3 columns × 2 rows of tiles; areas add up to the image.
        assert_eq!(tiles.len(), 6);
        assert_eq!(tiles.iter().map(|t| t.w * t.h).sum::<usize>(), w * h);
        // A tile decodes back to the original samples.
        let t = tiles.iter().find(|t| t.x == 1024 && t.y == 1024).unwrap();
        let mut px = Vec::new();
        flate2::read::ZlibDecoder::new(&t.data[..]).read_to_end(&mut px).unwrap();
        let (x, y) = (1024 + 7, 1024 + 5);
        let i = ((y - t.y) * t.w + (x - t.x)) * 3;
        assert_eq!(px[i], ((x + y) % 251) as u8);
        // Downsampled copy has the expected size.
        assert_eq!((p.low_w, p.low_h), (w, h)); // under the pixel budget: factor 1
    }

    #[test]
    fn tiled_form_replaces_the_image() {
        let (w, h) = (2500usize, 1300usize);
        let pdf = huge_pdf(w, h);
        let doc = Document::load_mem(&pdf).unwrap();
        let (&id, _) = doc
            .objects
            .iter()
            .find(|(_, o)| o.as_stream().map(|s| s.dict.has(b"Width")).unwrap_or(false))
            .unwrap();
        let hg = Huge { id, w, h, comps: 3, png: true, key: 2 };
        let p = process(&doc.get_object(id).unwrap().as_stream().unwrap().content, &hg).unwrap();
        let mut hi = doc.clone();
        replace_tiles(&mut hi, &hg, &p.levels[0]).unwrap();
        let form = hi.get_object(id).unwrap().as_stream().unwrap();
        assert_eq!(form.dict.get(b"Subtype").unwrap().as_name().unwrap(), b"Form");
        let xo = form.dict.get(b"Resources").unwrap().as_dict().unwrap().get(b"XObject").unwrap().as_dict().unwrap();
        assert_eq!(xo.len(), 6);
        let text = String::from_utf8(form.content.clone()).unwrap();
        // Top-left tile sits at the top of the unit square.
        let first = text.lines().next().unwrap();
        assert!(first.ends_with("cm /T0 Do Q"), "{first}");
        let nums: Vec<f64> = first.split_whitespace().skip(1).take(6).map(|v| v.parse().unwrap()).collect();
        assert!((nums[3] + nums[5] - 1.0).abs() < 1e-6, "top edge at y=1: {first}");
    }

    #[test]
    fn pyramid_levels_cover_the_image_and_average_it() {
        assert_eq!(level_factors(1), vec![1]);
        assert_eq!(level_factors(3), vec![1]);
        assert_eq!(level_factors(5), vec![1, 2]);
        assert_eq!(level_factors(8), vec![1, 2, 4]);
        assert_eq!(level_factors(13), vec![1, 2, 4, 8]);

        let (w, h) = (2500usize, 1300usize);
        let pdf = huge_pdf(w, h);
        let doc = Document::load_mem(&pdf).unwrap();
        let (&id, _) = doc
            .objects
            .iter()
            .find(|(_, o)| o.as_stream().map(|s| s.dict.has(b"Width")).unwrap_or(false))
            .unwrap();
        let hg = Huge { id, w, h, comps: 3, png: true, key: 3 };
        let content = doc.get_object(id).unwrap().as_stream().unwrap().content.clone();
        // A tiny pixel budget forces factor 8: levels 1, 2 and 4.
        let p = process_with(&content, &hg, w * h / 60).unwrap();
        assert_eq!(p.factor, 8);
        assert_eq!((p.low_w, p.low_h), (w.div_ceil(8), h.div_ceil(8)));
        assert_eq!(p.levels.iter().map(|l| l.f).collect::<Vec<_>>(), vec![1, 2, 4]);
        for l in &p.levels {
            assert_eq!((l.w, l.h), (w.div_ceil(l.f), h.div_ceil(l.f)), "level {}", l.f);
            assert_eq!(l.tiles.iter().map(|t| t.w * t.h).sum::<usize>(), l.w * l.h, "level {}", l.f);
            assert!(l.tiles.iter().all(|t| t.x + t.w <= l.w && t.y + t.h <= l.h));
        }
        // A level-2 pixel is the mean of its 2 × 2 source block.
        let l2 = &p.levels[1];
        let (lx, ly) = (600usize, 300usize);
        let t = l2.tiles.iter().find(|t| (t.x..t.x + t.w).contains(&lx) && (t.y..t.y + t.h).contains(&ly)).unwrap();
        let mut px = Vec::new();
        flate2::read::ZlibDecoder::new(&t.data[..]).read_to_end(&mut px).unwrap();
        let got = px[((ly - t.y) * t.w + (lx - t.x)) * 3] as u32;
        let src = |x: usize, y: usize| ((x + y) % 251) as u32;
        let (sx, sy) = (lx * 2, ly * 2);
        let want = (src(sx, sy) + src(sx + 1, sy) + src(sx, sy + 1) + src(sx + 1, sy + 1) + 2) / 4;
        assert_eq!(got, want);
    }
}
