//! Image streams that are larger than they have to be.
//!
//! Two independent steps, both on images with 8 bits per component that
//! are stored without loss (Flate, LZW, RunLength, uncompressed):
//!
//! - **Photographs become JPEG** ([`Options::recompress_images`]). Only
//!   images that look like a photograph are touched: few flat areas, more
//!   than 256 colours, no mask, no colour key, default `Decode`, Gray, RGB
//!   or CMYK samples. Line art, screenshots, text-like, small, masked,
//!   indexed and 1-bit images stay exactly as they are. The pixel
//!   dimensions never change and the colour space stays what it was (CMYK
//!   is written as a CMYK JPEG, not converted to RGB). Every result is
//!   decoded again and compared with the original: it is only used when
//!   its SSIM is at least [`SSIM_MIN`], its PSNR at least [`PSNR_MIN`]
//!   and the stream at least a fifth smaller; otherwise the next quality step is tried and in the end the
//!   original kept.
//! - **Everything else is packed tighter without loss**
//!   ([`Options::reduce_size`]): Flate with the PNG predictor chosen per
//!   row, used when that is smaller than the stream of the input.
//!
//! Images that are JPEG, JPEG 2000, CCITT or JBIG2 already are never
//! decoded: a second lossy pass would cost quality for little gain.
//!
//! [`Options::recompress_images`]: crate::Options::recompress_images
//! [`Options::reduce_size`]: crate::Options::reduce_size

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::util;
use crate::Ctx;

/// Lowest structural similarity (SSIM of the luminance, mean over 8×8
/// windows) a recompressed image may have against its original.
pub const SSIM_MIN: f64 = 0.98;

/// Lowest peak signal-to-noise ratio in dB, over the samples of all
/// channels, a recompressed image may have. The SSIM looks at brightness
/// only; this bound keeps the colour close as well.
pub const PSNR_MIN: f64 = 34.0;

/// JPEG qualities (IJG scale) in the order they are tried. The colour
/// channels keep their full resolution (4:4:4) at every step.
/// Ghostscript's default is about quality 55 with the colour at half
/// resolution (4:2:0), so every step is finer than what Ghostscript does.
pub const JPEG_QUALITIES: [u8; 4] = [80, 85, 90, 95];

/// A JPEG has to be at most this share of the lossless stream to be used.
const JPEG_GAIN: f64 = 0.8;
/// A lossless repack has to be at most this share of the stream.
const PACK_GAIN: f64 = 0.95;
/// Images below this stream size or edge length are left alone.
const MIN_BYTES: usize = 8 * 1024;
const MIN_EDGE: usize = 32;
/// Share of pixels equal to their right and lower neighbour from which an
/// image counts as drawn (line art, screenshot, text) rather than
/// photographed.
const FLAT_SHARE: f64 = 0.5;
/// An image with at most this many colours (grey levels for one
/// channel) is a palette image.
const PALETTE_COLOURS: usize = 256;
const PALETTE_GREYS: usize = 64;

const LOSSLESS_FILTERS: [&[u8]; 5] = [b"FlateDecode", b"LZWDecode", b"RunLengthDecode", b"ASCIIHexDecode", b"ASCII85Decode"];

/// Decoded samples of an image, 8 bits per component.
struct Raw {
    width: usize,
    height: usize,
    comps: usize,
    data: Vec<u8>,
}

enum Outcome {
    /// Re-encoded as JPEG, with its similarity to the original.
    Jpeg(Stream, Similarity),
    /// Same samples, smaller stream.
    Packed(Stream),
}

pub fn run(ctx: &mut Ctx) {
    let (lossy, lossless) = (ctx.opts.recompress_images, ctx.opts.reduce_size);
    if !lossy && !lossless {
        return;
    }
    let masks = mask_streams(&ctx.doc);
    // (Images of pages that were rendered are still there, but unused.)
    let used = util::reachable(&ctx.doc);
    let mut jobs: Vec<ObjectId> = Vec::new();
    for (id, o) in &ctx.doc.objects {
        if let Object::Stream(s) = o {
            if is_image(&s.dict) && s.content.len() >= MIN_BYTES && used.contains(id) {
                jobs.push(*id);
            }
        }
    }
    let results = convert_all(&ctx.doc, &jobs, &masks, lossy, lossless);
    let (mut count, mut saved) = (0usize, 0usize);
    let mut worst = Similarity { ssim: 1.0, psnr: f64::INFINITY };
    for (id, old_len, outcome) in results {
        let stream = match outcome {
            Outcome::Jpeg(s, similarity) => {
                count += 1;
                saved += old_len - s.content.len();
                worst.ssim = worst.ssim.min(similarity.ssim);
                worst.psnr = worst.psnr.min(similarity.psnr);
                s
            }
            Outcome::Packed(s) => s,
        };
        ctx.doc.objects.insert(id, Object::Stream(stream));
    }
    if count > 0 {
        ctx.report.note("images_recompressed", format!("{count} ({saved} bytes, SSIM {:.3}, PSNR {:.1} dB)", worst.ssim, worst.psnr));
    }
}

fn is_image(d: &Dictionary) -> bool {
    matches!(d.get(b"Subtype"), Ok(Object::Name(n)) if n == b"Image")
}

/// Streams used as a soft mask or stencil mask of an image.
fn mask_streams(doc: &Document) -> BTreeSet<ObjectId> {
    let mut out = BTreeSet::new();
    for o in doc.objects.values() {
        if let Object::Stream(s) = o {
            for key in [b"SMask".as_slice(), b"Mask"] {
                if let Ok(Object::Reference(id)) = s.dict.get(key) {
                    out.insert(*id);
                }
            }
        }
    }
    out
}

/// Work through `jobs` on all cores; results in the order of `jobs`.
fn convert_all(doc: &Document, jobs: &[ObjectId], masks: &BTreeSet<ObjectId>, lossy: bool, lossless: bool) -> Vec<(ObjectId, usize, Outcome)> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, ObjectId, usize, Outcome)>> = Mutex::new(Vec::new());
    let mut threads = 1;
    if let Ok(n) = std::thread::available_parallelism() {
        threads = n.get().min(8);
    }
    std::thread::scope(|scope| {
        for _ in 0..threads.min(jobs.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(id) = jobs.get(i) else { break };
                let Some(Object::Stream(s)) = doc.objects.get(id) else { continue };
                if let Some(outcome) = convert(doc, s, lossy && !masks.contains(id), lossless) {
                    if let Ok(mut d) = done.lock() {
                        d.push((i, *id, s.content.len(), outcome));
                    }
                }
            });
        }
    });
    let mut results = done.into_inner().unwrap_or_default();
    results.sort_by_key(|r| r.0);
    let mut out = Vec::with_capacity(results.len());
    for (_, id, len, outcome) in results {
        out.push((id, len, outcome));
    }
    out
}

/// A smaller stream for image `s`, when there is one.
fn convert(doc: &Document, s: &Stream, lossy: bool, lossless: bool) -> Option<Outcome> {
    let (comps, plain) = components(doc, s.dict.get(b"ColorSpace").ok()?)?;
    if !matches!(s.dict.get(b"BitsPerComponent"), Ok(Object::Integer(8))) {
        return None;
    }
    let width = usize::try_from(s.dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
    let height = usize::try_from(s.dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
    if width < MIN_EDGE || height < MIN_EDGE {
        return None;
    }
    let mut data = samples(s)?;
    let expected = width.checked_mul(height)?.checked_mul(comps)?;
    // (Up to two bytes of line end after the data are common.)
    if data.len() < expected || data.len() > expected + 2 {
        return None;
    }
    data.truncate(expected);
    let raw = Raw { width, height, comps, data };
    let old = s.content.len();
    if lossy && plain && !s.dict.has(b"SMask") && !s.dict.has(b"Mask") && default_decode(&s.dict) && photo_like(&raw) {
        if let Some((jpeg, similarity)) = best_jpeg(&raw, (old as f64 * JPEG_GAIN) as usize) {
            let mut dict = s.dict.clone();
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            dict.remove(b"DecodeParms");
            let mut stream = Stream::new(dict, jpeg);
            stream.allows_compression = false;
            return Some(Outcome::Jpeg(stream, similarity));
        }
    }
    if lossless {
        let packed = pack(&raw, s.dict.clone());
        // A Flate stream is only replaced for a real gain; everything
        // else (LZW, RunLength, uncompressed) as soon as this is smaller.
        let flate = matches!(s.dict.get(b"Filter"), Ok(Object::Name(n)) if n == b"FlateDecode");
        let limit = if flate { (old as f64 * PACK_GAIN) as usize } else { old };
        if packed.content.len() <= limit {
            return Some(Outcome::Packed(packed));
        }
    }
    None
}

/// Components per pixel of a colour space, and whether the samples are
/// plain Gray, RGB or CMYK values (what a JPEG can hold).
fn components(doc: &Document, cs: &Object) -> Option<(usize, bool)> {
    match util::deref(doc, cs) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" => Some((1, true)),
            b"DeviceRGB" => Some((3, true)),
            b"DeviceCMYK" => Some((4, true)),
            _ => None,
        },
        Object::Array(a) => {
            let Object::Name(kind) = util::deref(doc, a.first()?) else { return None };
            match kind.as_slice() {
                b"CalGray" => Some((1, true)),
                b"CalRGB" => Some((3, true)),
                b"Lab" => Some((3, false)),
                b"Indexed" | b"Separation" => Some((1, false)),
                b"DeviceN" => match util::deref(doc, a.get(1)?) {
                    Object::Array(names) if !names.is_empty() => Some((names.len(), false)),
                    _ => None,
                },
                b"ICCBased" => {
                    let Object::Stream(profile) = util::deref(doc, a.get(1)?) else { return None };
                    match profile.dict.get(b"N") {
                        Ok(Object::Integer(n @ (1 | 3 | 4))) => Some((*n as usize, true)),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// No `Decode` array, or the one that changes nothing.
fn default_decode(d: &Dictionary) -> bool {
    let Ok(decode) = d.get(b"Decode") else { return true };
    let Object::Array(a) = decode else { return false };
    for (i, v) in a.iter().enumerate() {
        if util::num(v) != Some((i % 2) as f64) {
            return false;
        }
    }
    true
}

/// The samples of a losslessly stored image. `None` for every other
/// filter (a JPEG is not decoded to be encoded again).
fn samples(s: &Stream) -> Option<Vec<u8>> {
    match s.dict.get(b"Filter") {
        Err(_) => {}
        Ok(Object::Name(n)) if LOSSLESS_FILTERS.contains(&n.as_slice()) => {}
        Ok(Object::Array(a)) => {
            for o in a {
                if !LOSSLESS_FILTERS.contains(&o.as_name().ok()?) {
                    return None;
                }
            }
        }
        Ok(_) => return None,
    }
    util::stream_data(s)
}

/// Does the image look photographed? Drawn images (line art, screenshots,
/// text) are mostly flat and gain little from JPEG but show its ringing;
/// palette images have too few colours to be a photograph.
fn photo_like(raw: &Raw) -> bool {
    let (w, h, n) = (raw.width, raw.height, raw.comps);
    let mut flat = 0usize;
    for y in 0..h - 1 {
        let line = &raw.data[y * w * n..(y + 1) * w * n];
        let below = &raw.data[(y + 1) * w * n..(y + 2) * w * n];
        for x in 0..w - 1 {
            let p = &line[x * n..(x + 1) * n];
            flat += (p == &line[(x + 1) * n..(x + 2) * n] && p == &below[x * n..(x + 1) * n]) as usize;
        }
    }
    if flat as f64 >= FLAT_SHARE * ((w - 1) * (h - 1)) as f64 {
        return false;
    }
    let limit = if n == 1 { PALETTE_GREYS } else { PALETTE_COLOURS };
    let mut colours = BTreeSet::new();
    for p in raw.data.chunks_exact(n) {
        let mut v = 0u32;
        for &c in p {
            v = v << 8 | c as u32;
        }
        colours.insert(v);
        if colours.len() > limit {
            return true;
        }
    }
    false
}

/// How close a recompressed image is to its original.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Similarity {
    /// Structural similarity of the luminance, see [`ssim`].
    pub ssim: f64,
    /// Peak signal-to-noise ratio over all samples, in dB.
    pub psnr: f64,
}

/// The first JPEG of [`JPEG_QUALITIES`] that is similar enough to `raw`
/// and at most `limit` bytes long.
fn best_jpeg(raw: &Raw, limit: usize) -> Option<(Vec<u8>, Similarity)> {
    let original = luminance(&raw.data, raw.comps);
    for quality in JPEG_QUALITIES {
        let jpeg = encode_jpeg(raw, quality)?;
        if jpeg.len() > limit {
            // The steps only get larger.
            return None;
        }
        let back = decode_jpeg(&jpeg, raw.comps)?;
        let similarity = Similarity { ssim: ssim(&original, &luminance(&back, raw.comps), raw.width, raw.height)?, psnr: psnr(&raw.data, &back) };
        if similarity.ssim >= SSIM_MIN && similarity.psnr >= PSNR_MIN {
            return Some((jpeg, similarity));
        }
    }
    None
}

/// Baseline JPEG with one interleaved scan, the standard Huffman tables
/// and no chroma subsampling: the form every decoder reads. Gray and RGB samples are stored
/// the usual way (RGB as YCbCr). CMYK samples are stored as YCCK with the
/// Adobe marker that says so, and in the polarity PDF expects (a sample
/// is the amount of ink, as in the lossless stream).
fn encode_jpeg(raw: &Raw, quality: u8) -> Option<Vec<u8>> {
    let (w, h) = (u16::try_from(raw.width).ok()?, u16::try_from(raw.height).ok()?);
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
    let result = match raw.comps {
        1 => encoder.encode(&raw.data, w, h, jpeg_encoder::ColorType::Luma),
        3 => encoder.encode(&raw.data, w, h, jpeg_encoder::ColorType::Rgb),
        _ => {
            // The encoder follows Photoshop and stores the complement;
            // complementing first makes it store the samples themselves.
            let mut inverted = raw.data.clone();
            for v in &mut inverted {
                *v = 255 - *v;
            }
            encoder.encode(&inverted, w, h, jpeg_encoder::ColorType::CmykAsYcck)
        }
    };
    result.ok()?;
    Some(out)
}

/// Samples of a JPEG written by [`encode_jpeg`], as a PDF reader gets them.
fn decode_jpeg(jpeg: &[u8], comps: usize) -> Option<Vec<u8>> {
    let space = match comps {
        1 => ColorSpace::Luma,
        3 => ColorSpace::RGB,
        _ => ColorSpace::YCCK,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(space);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(jpeg), options);
    let mut data = decoder.decode().ok()?;
    if comps == 4 {
        // YCCK → CMYK as libjpeg does it: ink = 255 − RGB, K unchanged.
        for p in data.chunks_exact_mut(4) {
            let (y, cb, cr) = (p[0] as f32, p[1] as f32 - 128.0, p[2] as f32 - 128.0);
            p[0] = 255 - (y + 1.402 * cr).round().clamp(0.0, 255.0) as u8;
            p[1] = 255 - (y - 0.344136 * cb - 0.714136 * cr).round().clamp(0.0, 255.0) as u8;
            p[2] = 255 - (y + 1.772 * cb).round().clamp(0.0, 255.0) as u8;
        }
    }
    Some(data)
}

/// What a viewer shows as brightness: the samples themselves for Gray,
/// Rec. 601 luma for RGB, and for CMYK the luma of the plain conversion
/// to RGB.
fn luminance(data: &[u8], comps: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / comps);
    for p in data.chunks_exact(comps) {
        let rgb = match comps {
            1 => [p[0] as u32; 3],
            3 => [p[0] as u32, p[1] as u32, p[2] as u32],
            _ => {
                let k = 255 - p[3] as u32;
                [(255 - p[0] as u32) * k / 255, (255 - p[1] as u32) * k / 255, (255 - p[2] as u32) * k / 255]
            }
        };
        out.push(((299 * rgb[0] + 587 * rgb[1] + 114 * rgb[2] + 500) / 1000) as u8);
    }
    out
}

/// Structural similarity (SSIM) of two 8-bit single-channel images of
/// `w` × `h` pixels: the mean over 8×8 windows placed every 4 pixels.
/// 1 is identical. `None` when the sizes do not fit.
pub fn ssim(a: &[u8], b: &[u8], w: usize, h: usize) -> Option<f64> {
    if a.len() != w * h || b.len() != a.len() || w < 8 || h < 8 {
        return None;
    }
    const C1: f64 = 6.5025; // (0.01 · 255)²
    const C2: f64 = 58.5225; // (0.03 · 255)²
    let (mut sum, mut count) = (0.0f64, 0usize);
    for y in (0..=h - 8).step_by(4) {
        for x in (0..=w - 8).step_by(4) {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for j in 0..8 {
                let start = (y + j) * w + x;
                for i in start..start + 8 {
                    let (p, q) = (a[i] as u32, b[i] as u32);
                    sa += p;
                    sb += q;
                    saa += p * p;
                    sbb += q * q;
                    sab += p * q;
                }
            }
            let (ma, mb) = (sa as f64 / 64.0, sb as f64 / 64.0);
            let (va, vb, cov) = (saa as f64 / 64.0 - ma * ma, sbb as f64 / 64.0 - mb * mb, sab as f64 / 64.0 - ma * mb);
            sum += ((2.0 * ma * mb + C1) * (2.0 * cov + C2)) / ((ma * ma + mb * mb + C1) * (va + vb + C2));
            count += 1;
        }
    }
    Some(sum / count as f64)
}

/// Peak signal-to-noise ratio of two sample arrays in dB (infinite when
/// they are equal).
pub fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mut sum = 0u64;
    for (x, y) in a.iter().zip(b) {
        let d = x.abs_diff(*y) as u64;
        sum += d * d;
    }
    if sum == 0 {
        return f64::INFINITY;
    }
    10.0 * (255.0 * 255.0 * a.len().min(b.len()) as f64 / sum as f64).log10()
}

/// One row through a PNG filter (0 None, 1 Sub, 2 Up, 3 Average, 4 Paeth)
/// with `n` bytes per pixel; returns the sum of the absolute differences.
fn png_filter(filter: u8, line: &[u8], above: &[u8], n: usize, out: &mut [u8]) -> u64 {
    match filter {
        0 => out.copy_from_slice(line),
        1 => {
            out[..n].copy_from_slice(&line[..n]);
            for i in n..line.len() {
                out[i] = line[i].wrapping_sub(line[i - n]);
            }
        }
        2 => {
            for i in 0..line.len() {
                out[i] = line[i].wrapping_sub(above[i]);
            }
        }
        3 => {
            for i in 0..n {
                out[i] = line[i].wrapping_sub(above[i] / 2);
            }
            for i in n..line.len() {
                out[i] = line[i].wrapping_sub(((line[i - n] as u16 + above[i] as u16) / 2) as u8);
            }
        }
        _ => {
            for i in 0..n {
                out[i] = line[i].wrapping_sub(above[i]);
            }
            for i in n..line.len() {
                out[i] = line[i].wrapping_sub(util::paeth(line[i - n], above[i], above[i - n]));
            }
        }
    }
    let mut cost = 0u64;
    for v in out.iter() {
        cost += (*v as i8).unsigned_abs() as u64;
    }
    cost
}

/// Flate with the PNG predictor, the filter of every row chosen by the
/// usual rule (smallest sum of absolute differences).
fn pack(raw: &Raw, mut dict: Dictionary) -> Stream {
    let (n, row) = (raw.comps, raw.width * raw.comps);
    let mut out = Vec::with_capacity(raw.data.len() + raw.height);
    let mut candidate = vec![0u8; row];
    let mut best = vec![0u8; row];
    let zero = vec![0u8; row];
    for y in 0..raw.height {
        let line = &raw.data[y * row..(y + 1) * row];
        let above = if y > 0 { &raw.data[(y - 1) * row..y * row] } else { &zero[..] };
        let (mut best_filter, mut best_cost) = (0u8, u64::MAX);
        // Up and Sub first: on drawn images most rows repeat the row
        // above or are one colour, and a filter that leaves only zeros
        // cannot be beaten.
        for filter in [2u8, 1, 0, 4, 3] {
            let cost = png_filter(filter, line, above, n, &mut candidate);
            if cost < best_cost {
                best_cost = cost;
                best_filter = filter;
                std::mem::swap(&mut candidate, &mut best);
            }
            if best_cost == 0 {
                break;
            }
        }
        out.push(best_filter);
        out.extend_from_slice(&best);
    }
    let mut parms = Dictionary::new();
    parms.set("Predictor", 15);
    parms.set("Colors", n as i64);
    parms.set("BitsPerComponent", 8);
    parms.set("Columns", raw.width as i64);
    let mut stream = util::flate_stream(std::mem::take(&mut dict), &out);
    stream.dict.set("DecodeParms", Object::Dictionary(parms));
    stream
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common;
    use lopdf::dictionary;

    const W: usize = 160;
    const H: usize = 120;

    fn name(s: &str) -> Object {
        Object::Name(s.as_bytes().to_vec())
    }

    /// The image streams of a converted file.
    fn images_of(bytes: &[u8]) -> Vec<Stream> {
        let doc = Document::load_mem(bytes).unwrap();
        let mut out = Vec::new();
        for o in doc.objects.values() {
            if let Object::Stream(s) = o {
                if is_image(&s.dict) {
                    out.push(s.clone());
                }
            }
        }
        out
    }

    fn filter(s: &Stream) -> &[u8] {
        match s.dict.get(b"Filter") {
            Ok(Object::Name(n)) => n,
            _ => b"",
        }
    }

    fn space(s: &Stream) -> &[u8] {
        match s.dict.get(b"ColorSpace") {
            Ok(Object::Name(n)) => n,
            _ => b"other",
        }
    }

    #[test]
    fn photographs_become_jpeg_and_the_rest_stays_lossless() {
        let input = common::images();
        let out = crate::convert(&input, &crate::Options::new("2b".parse().unwrap())).unwrap();
        let note = out.report.notes.iter().find(|n| n.code == "images_recompressed").expect("note");
        assert!(note.detail.starts_with("3 ("), "{}", note.detail);
        let images = images_of(&out.bytes);
        assert_eq!(images.len(), 8);
        let mut seen = Vec::new();
        for s in &images {
            let comps = match space(s) {
                b"DeviceGray" => 1,
                b"DeviceRGB" => 3,
                b"DeviceCMYK" => 4,
                _ => 0,
            };
            let masked = s.dict.has(b"SMask");
            let negative = s.dict.has(b"Decode");
            if filter(s) == b"DCTDecode" {
                // A photograph: same size, same colour space, close to the original.
                assert!(!masked && !negative && comps > 0);
                assert!(s.dict.get(b"DecodeParms").is_err());
                let original = common::photo(W, H, comps);
                let back = decode_jpeg(&s.content, comps).unwrap();
                let score = ssim(&luminance(&original, comps), &luminance(&back, comps), W, H).unwrap();
                assert!(score >= SSIM_MIN, "{comps} channels: SSIM {score}");
                assert!(psnr(&original, &back) > 35.0);
                assert!(s.content.len() * 4 < W * H * comps);
                seen.push(format!("jpeg{comps}"));
                continue;
            }
            // Everything else: the very same samples as in the input.
            assert_eq!(filter(s), b"FlateDecode");
            let data = samples(s).unwrap();
            let expected = if masked {
                seen.push("masked".to_string());
                common::photo(W, H, 3)
            } else if comps == 3 {
                seen.push("drawing".to_string());
                common::drawing(W, H)
            } else if comps == 0 || negative {
                seen.push(if negative { "negative" } else { "indexed" }.to_string());
                common::photo(W, H, 1)
            } else {
                seen.push("mask".to_string());
                continue;
            };
            assert_eq!(data, expected);
        }
        seen.sort();
        assert_eq!(seen, ["drawing", "indexed", "jpeg1", "jpeg3", "jpeg4", "mask", "masked", "negative"]);

        // Told to keep images lossless: no JPEG, every sample as it was.
        let mut opts = crate::Options::new("2b".parse().unwrap());
        opts.recompress_images = false;
        let lossless = crate::convert(&input, &opts).unwrap();
        assert!(!lossless.report.has("images_recompressed"));
        let mut photos = 0;
        for s in images_of(&lossless.bytes) {
            assert_eq!(filter(&s), b"FlateDecode");
            let data = samples(&s).unwrap();
            for comps in [1, 3, 4] {
                photos += (data == common::photo(W, H, comps)) as usize;
            }
        }
        // RGB, masked RGB, CMYK, Gray, indexed and negative.
        assert_eq!(photos, 6);
        assert!(lossless.bytes.len() > out.bytes.len());

        // Neither step: the streams of the input (LZW still has to go).
        opts.reduce_size = false;
        let plain = crate::convert(&input, &opts).unwrap();
        assert!(plain.bytes.len() > lossless.bytes.len());
        let unfiltered = images_of(&plain.bytes).iter().filter(|s| filter(s).is_empty()).count();
        assert_eq!(unfiltered, 5);

        // PDF/A-1 renders the page with the soft mask; the images of that
        // page are not worked on any more.
        struct Blank;
        impl crate::Rasterizer for Blank {
            fn render_page(&self, _: &[u8], _: u32, _: f32) -> std::result::Result<crate::RasterPage, String> {
                Ok(crate::RasterPage { width: 40, height: 40, rgb: vec![255; 4800], words: Vec::new() })
            }
        }
        let part1 = crate::convert(&input, &crate::Options::new("1b".parse().unwrap()).with_rasterizer(std::sync::Arc::new(Blank))).unwrap();
        assert!(part1.report.has("pages_rendered") && part1.report.has("images_recompressed"));
        let images = images_of(&part1.bytes);
        assert_eq!(images.iter().filter(|s| s.dict.has(b"SMask")).count(), 0);
        assert_eq!(images.iter().filter(|s| filter(s) == b"DCTDecode").count(), 4);
    }

    fn flate_image(space: Object, data: &[u8]) -> Stream {
        util::flate_stream(common::image_stream(W, H, space, Dictionary::new(), Vec::new()).dict, data)
    }

    #[test]
    fn images_that_are_left_alone() {
        let doc = Document::with_version("1.7");
        let photo = common::photo(W, H, 3);
        let good = flate_image(name("DeviceRGB"), &photo);
        assert!(matches!(convert(&doc, &good, true, true), Some(Outcome::Jpeg(..))));
        assert!(matches!(convert(&doc, &good, false, true), Some(Outcome::Packed(_))));
        assert!(convert(&doc, &good, false, false).is_none());

        let mut cases: Vec<(&str, Stream)> = Vec::new();
        let mut s = good.clone();
        s.dict.set("BitsPerComponent", 4);
        cases.push(("not 8 bit", s));
        let mut s = good.clone();
        s.dict.set("Width", 16);
        cases.push(("narrow", s));
        let mut s = good.clone();
        s.dict.set("Height", 16);
        cases.push(("low", s));
        let mut s = good.clone();
        s.dict.set("Height", H as i64 + 1);
        cases.push(("data too short", s));
        let mut s = good.clone();
        s.dict.set("Height", H as i64 - 1);
        cases.push(("data too long", s));
        let mut s = good.clone();
        s.dict.remove(b"ColorSpace");
        cases.push(("no colour space", s));
        let mut s = good.clone();
        s.dict.set("ColorSpace", "Pattern");
        cases.push(("pattern", s));
        let mut s = good.clone();
        s.dict.set("Filter", "DCTDecode");
        cases.push(("JPEG already", s));
        let mut s = good.clone();
        s.dict.set("Filter", vec![name("ASCII85Decode"), name("JPXDecode")]);
        cases.push(("JPEG 2000 in a chain", s));
        let mut s = good.clone();
        s.dict.set("Filter", 7);
        cases.push(("odd filter", s));
        let mut s = good.clone();
        s.dict.set("Filter", vec![Object::Integer(7)]);
        cases.push(("odd filter in a chain", s));
        for (why, s) in &cases {
            assert!(convert(&doc, s, true, true).is_none(), "{why}");
        }

        // Two bytes of line end after the samples are fine.
        let mut padded = photo.clone();
        padded.extend_from_slice(b"\r\n");
        assert!(matches!(convert(&doc, &flate_image(name("DeviceRGB"), &padded), true, true), Some(Outcome::Jpeg(..))));
        // An ASCII85 chain around Flate is lossless too.
        let mut chained = good.clone();
        chained.dict.set("Filter", vec![name("FlateDecode")]);
        assert!(matches!(convert(&doc, &chained, true, true), Some(Outcome::Jpeg(..))));

        // Pure noise: a JPEG that is close enough is not smaller, and no
        // predictor helps either.
        let mut noise = Vec::new();
        let mut seed = 7u32;
        for _ in 0..W * H * 3 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            noise.push((seed >> 24) as u8);
        }
        let raw = Raw { width: W, height: H, comps: 3, data: noise.clone() };
        assert!(photo_like(&raw));
        let (jpeg, similarity) = best_jpeg(&raw, usize::MAX).expect("the highest quality is close enough");
        assert!(similarity.ssim >= SSIM_MIN && similarity.psnr >= PSNR_MIN && jpeg.len() > noise.len() * 4 / 5, "{similarity:?} {}", jpeg.len());
        assert!(best_jpeg(&raw, 100).is_none(), "the limit ends the search");
        assert!(convert(&doc, &flate_image(name("DeviceRGB"), &noise), true, true).is_none());
        // Not photographs: flat, or few colours.
        let drawing = Raw { width: W, height: H, comps: 3, data: common::drawing(W, H) };
        assert!(!photo_like(&drawing));
        let mut two = Vec::new();
        for i in 0..W * H {
            two.push(if (i * 7 + i / W).is_multiple_of(3) { 0 } else { 255 });
        }
        assert!(!photo_like(&Raw { width: W, height: H, comps: 1, data: two }));
        // Wider than a JPEG can be.
        assert!(encode_jpeg(&Raw { width: 70000, height: 1, comps: 1, data: vec![0; 70000] }, 80).is_none());
        assert!(decode_jpeg(b"not a jpeg", 3).is_none());
    }

    #[test]
    fn colour_spaces_and_decode_arrays() {
        let mut doc = Document::with_version("1.7");
        let icc = |doc: &mut Document, n: i64| doc.add_object(Stream::new(dictionary! { "N" => n }, Vec::new()));
        let (icc3, icc4, icc2) = (icc(&mut doc, 3), icc(&mut doc, 4), icc(&mut doc, 2));
        let indirect = doc.add_object(Object::Array(vec![name("ICCBased"), icc3.into()]));
        let not_a_profile = doc.add_object(Dictionary::new());
        let cases: Vec<(Object, Option<(usize, bool)>)> = vec![
            (name("DeviceGray"), Some((1, true))),
            (name("DeviceRGB"), Some((3, true))),
            (name("DeviceCMYK"), Some((4, true))),
            (name("Pattern"), None),
            (Object::Integer(1), None),
            (Object::Array(Vec::new()), None),
            (Object::Array(vec![Object::Integer(1)]), None),
            (Object::Array(vec![name("CalGray"), Dictionary::new().into()]), Some((1, true))),
            (Object::Array(vec![name("CalRGB"), Dictionary::new().into()]), Some((3, true))),
            (Object::Array(vec![name("Lab"), Dictionary::new().into()]), Some((3, false))),
            (Object::Array(vec![name("Indexed"), name("DeviceRGB"), 1.into(), Object::string_literal("abcdef")]), Some((1, false))),
            (Object::Array(vec![name("Separation"), name("Gold"), name("DeviceCMYK"), Dictionary::new().into()]), Some((1, false))),
            (Object::Array(vec![name("DeviceN"), vec![name("A"), name("B")].into(), name("DeviceCMYK"), Dictionary::new().into()]), Some((2, false))),
            (Object::Array(vec![name("DeviceN"), Vec::<Object>::new().into()]), None),
            (Object::Array(vec![name("DeviceN")]), None),
            (indirect.into(), Some((3, true))),
            (Object::Array(vec![name("ICCBased"), icc4.into()]), Some((4, true))),
            (Object::Array(vec![name("ICCBased"), icc2.into()]), None),
            (Object::Array(vec![name("ICCBased"), not_a_profile.into()]), None),
            (Object::Array(vec![name("ICCBased")]), None),
            (Object::Array(vec![name("Pattern")]), None),
        ];
        for (space, expected) in cases {
            assert_eq!(components(&doc, &space), expected, "{space:?}");
        }

        assert!(default_decode(&Dictionary::new()));
        assert!(default_decode(&dictionary! { "Decode" => vec![0.into(), 1.into(), Object::Real(0.0), Object::Real(1.0)] }));
        assert!(!default_decode(&dictionary! { "Decode" => vec![1.into(), 0.into()] }));
        assert!(!default_decode(&dictionary! { "Decode" => "odd" }));
    }

    #[test]
    fn work_list() {
        // Masks are found whether they are soft masks or stencil masks.
        let mut doc = Document::with_version("1.7");
        let photo = common::photo(W, H, 1);
        let soft = doc.add_object(flate_image(name("DeviceGray"), &photo));
        let stencil = doc.add_object(flate_image(name("DeviceGray"), &photo));
        let mut a = flate_image(name("DeviceGray"), &photo);
        a.dict.set("SMask", soft);
        let a = doc.add_object(a);
        let mut b = flate_image(name("DeviceGray"), &photo);
        b.dict.set("Mask", stencil);
        b.dict.set("SMask", "None");
        let b = doc.add_object(b);
        let plain = doc.add_object(flate_image(name("DeviceGray"), &photo));
        let masks = mask_streams(&doc);
        assert_eq!(masks, BTreeSet::from([soft, stencil]));

        assert!(convert_all(&doc, &[], &masks, true, true).is_empty());
        // Jobs that are no stream are skipped; results keep the job order.
        let jobs = [plain, (9000, 0), stencil, a, b, soft];
        let results = convert_all(&doc, &jobs, &masks, true, true);
        let ids: Vec<ObjectId> = results.iter().map(|r| r.0).collect();
        assert_eq!(ids, [plain, stencil, a, b, soft]);
        for (id, old, outcome) in &results {
            assert_eq!(*old, doc.objects.get(id).unwrap().as_stream().unwrap().content.len());
            // Only the image that neither is a mask nor has one becomes a JPEG.
            assert_eq!(matches!(outcome, Outcome::Jpeg(..)), *id == plain, "{id:?}");
        }
    }

    #[test]
    fn similarity_measures() {
        let a = luminance(&common::photo(64, 48, 3), 3);
        assert_eq!(a.len(), 64 * 48);
        assert_eq!(ssim(&a, &a, 64, 48), Some(1.0));
        assert_eq!(psnr(&a, &a), f64::INFINITY);
        let mut b = a.clone();
        for (i, v) in b.iter_mut().enumerate() {
            *v = v.wrapping_add((i % 7) as u8 * 9);
        }
        let score = ssim(&a, &b, 64, 48).unwrap();
        assert!(score < 0.9 && score > 0.0, "{score}");
        assert!(psnr(&a, &b) < 30.0);
        // Sizes that do not fit.
        assert!(ssim(&a, &b, 64, 47).is_none());
        assert!(ssim(&a, &b[1..], 64, 48).is_none());
        assert!(ssim(&a[..28], &b[..28], 7, 4).is_none());
        assert!(ssim(&a[..28], &b[..28], 4, 7).is_none());
        // Luminance of the three kinds of samples.
        assert_eq!(luminance(&[7, 200], 1), [7, 200]);
        assert_eq!(luminance(&[255, 255, 255, 0, 0, 0, 255, 0, 0], 3), [255, 0, 76]);
        assert_eq!(luminance(&[0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0], 4), [255, 0, 179]);
        let s = Similarity { ssim: 0.99, psnr: 40.0 };
        let copy = s;
        assert!(copy == s && format!("{s:?}").contains("0.99"));
    }

    #[test]
    fn packing_keeps_every_sample() {
        for comps in [1usize, 3, 4] {
            let data = common::photo(W, H, comps);
            let packed = pack(&Raw { width: W, height: H, comps, data: data.clone() }, dictionary! { "Subtype" => "Image" });
            assert_eq!(filter(&packed), b"FlateDecode");
            assert_eq!(samples(&packed).unwrap(), data);
            assert!(packed.content.len() < util::flate(&data).len(), "{comps} channels");
        }
        // Flat rows take the filter that costs nothing.
        let flat = vec![200u8; W * H];
        let packed = pack(&Raw { width: W, height: H, comps: 1, data: flat.clone() }, Dictionary::new());
        assert_eq!(samples(&packed).unwrap(), flat);
    }
}
