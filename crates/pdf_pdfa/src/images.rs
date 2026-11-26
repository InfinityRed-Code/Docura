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
//! **Memory.** A scanned or rendered plan can be one image of several
//! gigabytes of samples in a file of a few megabytes. Such an image is
//! never held as a whole: above [`Options::max_image_bytes`] of samples it
//! is decoded row by row and packed in bands on all cores, and it is not
//! considered for JPEG (measuring a JPEG needs the original and the decoded
//! result side by side). Smaller images are worked on in memory, several at
//! a time, as long as their samples together fit a budget of four times
//! that limit.
//!
//! [`Options::recompress_images`]: crate::Options::recompress_images
//! [`Options::reduce_size`]: crate::Options::reduce_size
//! [`Options::max_image_bytes`]: crate::Options::max_image_bytes

use std::collections::BTreeSet;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

use flate2::{Compress, Compression, FlushCompress, Status};
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

/// Default of [`Options::max_image_bytes`](crate::Options::max_image_bytes):
/// 512 MiB of samples, an A0 page scanned in colour at 300 dpi.
pub(crate) const LARGE: usize = 512 << 20;
/// Sample bytes packed as one piece: rows are filtered and compressed in
/// bands of about this size, each on its own core.
const BAND: usize = 4 << 20;

const LOSSLESS_FILTERS: [&[u8]; 5] = [b"FlateDecode", b"LZWDecode", b"RunLengthDecode", b"ASCIIHexDecode", b"ASCII85Decode"];

/// The memory the image work may hold at one time, shared by its threads.
struct Memory {
    /// Images with more sample bytes are worked on row by row.
    limit: usize,
    total: usize,
    free: Mutex<usize>,
    freed: Condvar,
}

impl Memory {
    fn new(limit: usize) -> Memory {
        let total = limit.saturating_mul(4);
        Memory { limit, total, free: Mutex::new(total), freed: Condvar::new() }
    }

    /// Wait until `bytes` are free and take them (at most everything, so
    /// the largest image runs alone). Returns what has to be given back.
    fn take(&self, bytes: usize) -> usize {
        let bytes = bytes.min(self.total);
        let Ok(mut free) = self.free.lock() else { return 0 };
        while *free < bytes {
            free = match self.freed.wait(free) {
                Ok(f) => f,
                Err(_) => return 0,
            };
        }
        *free -= bytes;
        bytes
    }

    fn give(&self, bytes: usize) {
        if let Ok(mut free) = self.free.lock() {
            *free += bytes;
        }
        self.freed.notify_all();
    }
}

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
    let results = convert_all(&ctx.doc, &jobs, &masks, lossy, lossless, ctx.opts.max_image_bytes);
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
/// `limit`: see [`Options::max_image_bytes`](crate::Options::max_image_bytes).
fn convert_all(doc: &Document, jobs: &[ObjectId], masks: &BTreeSet<ObjectId>, lossy: bool, lossless: bool, limit: usize) -> Vec<(ObjectId, usize, Outcome)> {
    let memory = Memory::new(limit);
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
                if let Some(outcome) = convert(doc, s, lossy && !masks.contains(id), lossless, &memory) {
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
fn convert(doc: &Document, s: &Stream, lossy: bool, lossless: bool, memory: &Memory) -> Option<Outcome> {
    let (comps, plain) = components(doc, s.dict.get(b"ColorSpace").ok()?)?;
    if !matches!(s.dict.get(b"BitsPerComponent"), Ok(Object::Integer(8))) {
        return None;
    }
    let width = usize::try_from(s.dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
    let height = usize::try_from(s.dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
    if width < MIN_EDGE || height < MIN_EDGE {
        return None;
    }
    let expected = width.checked_mul(height)?.checked_mul(comps)?;
    let size = Size { width, height, comps };
    if expected > memory.limit {
        return if lossless { convert_by_rows(s, size) } else { None };
    }
    // Samples, their brightness, a JPEG and what it decodes to: up to
    // four times the samples at once.
    let held = memory.take(expected.saturating_mul(4));
    let outcome = convert_in_memory(s, size, lossy && plain, lossless);
    memory.give(held);
    outcome
}

/// Pixel dimensions of an image.
#[derive(Clone, Copy)]
struct Size {
    width: usize,
    height: usize,
    comps: usize,
}

/// The largest stream a lossless repack of `s` may have to be used. A
/// Flate stream is only replaced for a real gain; everything else (LZW,
/// RunLength, uncompressed) as soon as the new one is smaller.
fn pack_limit(s: &Stream) -> usize {
    let old = s.content.len();
    if matches!(s.dict.get(b"Filter"), Ok(Object::Name(n)) if n == b"FlateDecode") {
        (old as f64 * PACK_GAIN) as usize
    } else {
        old
    }
}

/// The usual way: decode the image, look at it, store it as JPEG or pack it.
fn convert_in_memory(s: &Stream, size: Size, lossy: bool, lossless: bool) -> Option<Outcome> {
    let Size { width, height, comps } = size;
    let expected = width * height * comps;
    let mut data = samples(s)?;
    // (Up to two bytes of line end after the data are common.)
    if data.len() < expected || data.len() > expected + 2 {
        return None;
    }
    data.truncate(expected);
    let raw = Raw { width, height, comps, data };
    let old = s.content.len();
    if lossy && !s.dict.has(b"SMask") && !s.dict.has(b"Mask") && default_decode(&s.dict) && photo_like(&raw) {
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
        let packed = pack(&raw, s.dict.clone())?;
        if packed.content.len() <= pack_limit(s) {
            return Some(Outcome::Packed(packed));
        }
    }
    None
}

/// An image too large to hold: its rows are decoded one after the other
/// and packed as they come. Without loss, whatever the image shows.
fn convert_by_rows(s: &Stream, size: Size) -> Option<Outcome> {
    let mut rows = Rows::new(s, size)?;
    let packed = pack_rows(&mut |line| rows.next(line), size, s.dict.clone())?;
    if rows.at_end() && packed.content.len() <= pack_limit(s) {
        return Some(Outcome::Packed(packed));
    }
    None
}

/// The rows of an image that is stored uncompressed or with Flate, with or
/// without a predictor, read one at a time.
struct Rows<'a> {
    source: Box<dyn Read + 'a>,
    /// 1: none, 2: TIFF, 10 and above: PNG.
    predictor: i64,
    comps: usize,
    above: Vec<u8>,
}

impl<'a> Rows<'a> {
    /// `None` for any other way of storing an image (it is left alone).
    fn new(s: &'a Stream, size: Size) -> Option<Rows<'a>> {
        let filter = match s.dict.get(b"Filter") {
            Err(_) => None,
            Ok(Object::Name(n)) => Some(n.as_slice()),
            Ok(Object::Array(a)) if a.len() == 1 => Some(a[0].as_name().ok()?),
            Ok(_) => return None,
        };
        let mut predictor = 1;
        let source: Box<dyn Read + 'a> = match filter {
            None => Box::new(&s.content[..]),
            Some(b"FlateDecode") => {
                let parms = match s.dict.get(b"DecodeParms") {
                    Ok(Object::Dictionary(d)) => Some(d),
                    Ok(Object::Array(a)) => match a.first() {
                        Some(Object::Dictionary(d)) => Some(d),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(p) = parms {
                    if let Some(p) = util::take_predictor(&mut p.clone()) {
                        if p.bits != 8 || p.colors != size.comps || p.columns != size.width || !matches!(p.kind, 2 | 10..=15) {
                            return None;
                        }
                        predictor = p.kind;
                    }
                }
                Box::new(flate2::read::ZlibDecoder::new(&s.content[..]))
            }
            Some(_) => return None,
        };
        Some(Rows { source, predictor, comps: size.comps, above: vec![0; size.width * size.comps] })
    }

    /// The next row into `line`; `false` when the data ends or is invalid.
    fn next(&mut self, line: &mut [u8]) -> bool {
        let n = self.comps;
        let mut filter = [0u8];
        if self.predictor >= 10 && self.source.read_exact(&mut filter).is_err() {
            return false;
        }
        if self.source.read_exact(line).is_err() {
            return false;
        }
        if self.predictor == 2 {
            filter[0] = 1;
        }
        let above = &self.above;
        match filter[0] {
            0 => {}
            1 => {
                for i in n..line.len() {
                    line[i] = line[i].wrapping_add(line[i - n]);
                }
            }
            2 => {
                for (v, a) in line.iter_mut().zip(above) {
                    *v = v.wrapping_add(*a);
                }
            }
            3 => {
                for i in 0..n {
                    line[i] = line[i].wrapping_add(above[i] / 2);
                }
                for i in n..line.len() {
                    line[i] = line[i].wrapping_add(((line[i - n] as u16 + above[i] as u16) / 2) as u8);
                }
            }
            4 => {
                for i in 0..n {
                    line[i] = line[i].wrapping_add(above[i]);
                }
                for i in n..line.len() {
                    line[i] = line[i].wrapping_add(util::paeth(line[i - n], above[i], above[i - n]));
                }
            }
            _ => return false,
        }
        if self.predictor >= 10 {
            self.above.copy_from_slice(line);
        }
        true
    }

    /// Nothing follows the rows read so far (up to two bytes of line end
    /// are common).
    fn at_end(&mut self) -> bool {
        let mut rest = [0u8; 3];
        let mut n = 0;
        while n < rest.len() {
            match self.source.read(&mut rest[n..]) {
                Ok(0) | Err(_) => break,
                Ok(more) => n += more,
            }
        }
        n <= 2
    }
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
    let drawn = FLAT_SHARE * ((w - 1) * (h - 1)) as f64;
    for y in 0..h - 1 {
        let line = &raw.data[y * w * n..(y + 1) * w * n];
        let below = &raw.data[(y + 1) * w * n..(y + 2) * w * n];
        for x in 0..w - 1 {
            let p = &line[x * n..(x + 1) * n];
            flat += (p == &line[(x + 1) * n..(x + 2) * n] && p == &below[x * n..(x + 1) * n]) as usize;
        }
        // (The rest of the image cannot change that any more.)
        if flat as f64 >= drawn {
            return false;
        }
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

/// [`pack_rows`] for samples that are in memory.
fn pack(raw: &Raw, dict: Dictionary) -> Option<Stream> {
    let mut rows = raw.data.chunks_exact(raw.width * raw.comps);
    let mut next = |line: &mut [u8]| match rows.next() {
        Some(row) => {
            line.copy_from_slice(row);
            true
        }
        None => false,
    };
    pack_rows(&mut next, Size { width: raw.width, height: raw.height, comps: raw.comps }, dict)
}

/// Flate with the PNG predictor, the filter of every row chosen by the
/// usual rule (smallest sum of absolute differences).
///
/// `next` fills in one row of samples per call, from the top; `None` when
/// it runs out before the image is complete. The rows are taken in bands
/// of about [`BAND`] bytes, and with more than one band each is filtered
/// and compressed on its own core. Every band ends on a byte boundary
/// (a sync flush), so the bands one after the other are one Flate stream.
/// At no time more than one band per core is held, however large the image.
fn pack_rows(next: &mut dyn FnMut(&mut [u8]) -> bool, size: Size, mut dict: Dictionary) -> Option<Stream> {
    let (n, row) = (size.comps, size.width * size.comps);
    let per_band = (BAND / row).max(1);
    let bands = size.height.div_ceil(per_band);
    let mut threads = 0;
    if bands > 1 {
        threads = 1;
        if let Ok(cores) = std::thread::available_parallelism() {
            threads = cores.get().min(8).min(bands);
        }
    }
    // (band number, the row above the band, the rows of the band)
    let (jobs, queue) = std::sync::mpsc::sync_channel::<(usize, Vec<u8>, Vec<u8>)>(0);
    let queue = Mutex::new(queue);
    let done: Mutex<Vec<(usize, Option<Band>)>> = Mutex::new(Vec::new());
    let mut complete = true;
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let job = match queue.lock() {
                    Ok(q) => q.recv(),
                    Err(_) => break,
                };
                let Ok((i, above, band)) = job else { break };
                let packed = pack_band(&above, &band, row, n, i + 1 == bands);
                if let Ok(mut d) = done.lock() {
                    d.push((i, packed));
                }
            });
        }
        let mut above = vec![0u8; row];
        for i in 0..bands {
            let rows = per_band.min(size.height - i * per_band);
            let mut band = vec![0u8; rows * row];
            for line in band.chunks_exact_mut(row) {
                complete = complete && next(line);
            }
            if !complete {
                break;
            }
            let last = band[band.len() - row..].to_vec();
            let above = std::mem::replace(&mut above, last);
            if threads == 0 {
                let packed = pack_band(&above, &band, row, n, true);
                if let Ok(mut d) = done.lock() {
                    d.push((i, packed));
                }
            } else if jobs.send((i, above, band)).is_err() {
                break;
            }
        }
        drop(jobs);
    });
    let mut parts = done.into_inner().ok()?;
    if !complete || parts.len() != bands {
        return None;
    }
    parts.sort_by_key(|p| p.0);
    // zlib: header (deflate, 32 KiB window, default level), data, Adler-32.
    let mut out = vec![0x78, 0x9C];
    let mut check = 1u32;
    for (_, part) in parts {
        let part = part?;
        out.extend_from_slice(&part.data);
        check = adler_combine(check, part.check, part.len);
    }
    out.extend_from_slice(&check.to_be_bytes());
    let mut parms = Dictionary::new();
    parms.set("Predictor", 15);
    parms.set("Colors", n as i64);
    parms.set("BitsPerComponent", 8);
    parms.set("Columns", size.width as i64);
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("DecodeParms", Object::Dictionary(parms));
    let mut stream = Stream::new(dict, out);
    stream.allows_compression = false;
    Some(stream)
}

/// A band of rows, filtered and compressed.
struct Band {
    /// Raw deflate data, ending on a byte boundary.
    data: Vec<u8>,
    /// Adler-32 and length of the filtered rows.
    check: u32,
    len: usize,
}

/// Filter and compress the rows in `band`; `above` is the row before the
/// first one (zeros at the top of the image).
fn pack_band(above: &[u8], band: &[u8], row: usize, n: usize, last: bool) -> Option<Band> {
    let mut deflate = Compress::new(Compression::default(), false);
    let mut data = Vec::new();
    let mut check = adler2::Adler32::new();
    let mut candidate = vec![0u8; row];
    let mut best = vec![0u8; row];
    let mut above = above;
    for line in band.chunks_exact(row) {
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
        for piece in [&[best_filter][..], &best[..]] {
            check.write_slice(piece);
            deflate_into(&mut deflate, piece, &mut data, FlushCompress::None)?;
        }
        above = line;
    }
    deflate_into(&mut deflate, &[], &mut data, if last { FlushCompress::Finish } else { FlushCompress::Sync })?;
    data.shrink_to_fit();
    Some(Band { data, check: check.checksum(), len: band.len() + band.len() / row })
}

/// Compress all of `input` onto the end of `out` and carry out `flush`.
fn deflate_into(deflate: &mut Compress, mut input: &[u8], out: &mut Vec<u8>, flush: FlushCompress) -> Option<()> {
    loop {
        if out.capacity() - out.len() < 1024 {
            out.reserve(out.len().max(4096));
        }
        let before = deflate.total_in();
        let status = deflate.compress_vec(input, out, flush).ok()?;
        input = &input[(deflate.total_in() - before) as usize..];
        // A flush is complete when the compressor leaves room in `out`.
        let finished = match flush {
            FlushCompress::Finish => status == Status::StreamEnd,
            _ => input.is_empty() && out.len() < out.capacity(),
        };
        if finished {
            return Some(());
        }
    }
}

/// The Adler-32 of two pieces of data one after the other, from the
/// checksum of each and the length of the second (as zlib combines them).
fn adler_combine(first: u32, second: u32, second_len: usize) -> u32 {
    const BASE: u64 = 65521;
    let rem = second_len as u64 % BASE;
    let (a1, b1) = ((first & 0xFFFF) as u64, (first >> 16) as u64);
    let (a2, b2) = ((second & 0xFFFF) as u64, (second >> 16) as u64);
    let a = (a1 + a2 + BASE - 1) % BASE;
    let b = (rem * a1 + b1 + b2 + BASE - rem) % BASE;
    (b << 16 | a) as u32
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
        assert!(matches!(convert(&doc, &good, true, true, &Memory::new(LARGE)), Some(Outcome::Jpeg(..))));
        assert!(matches!(convert(&doc, &good, false, true, &Memory::new(LARGE)), Some(Outcome::Packed(_))));
        assert!(convert(&doc, &good, false, false, &Memory::new(LARGE)).is_none());

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
            assert!(convert(&doc, s, true, true, &Memory::new(LARGE)).is_none(), "{why}");
        }

        // Two bytes of line end after the samples are fine.
        let mut padded = photo.clone();
        padded.extend_from_slice(b"\r\n");
        assert!(matches!(convert(&doc, &flate_image(name("DeviceRGB"), &padded), true, true, &Memory::new(LARGE)), Some(Outcome::Jpeg(..))));
        // An ASCII85 chain around Flate is lossless too.
        let mut chained = good.clone();
        chained.dict.set("Filter", vec![name("FlateDecode")]);
        assert!(matches!(convert(&doc, &chained, true, true, &Memory::new(LARGE)), Some(Outcome::Jpeg(..))));

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
        assert!(convert(&doc, &flate_image(name("DeviceRGB"), &noise), true, true, &Memory::new(LARGE)).is_none());
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

        assert!(convert_all(&doc, &[], &masks, true, true, LARGE).is_empty());
        // Jobs that are no stream are skipped; results keep the job order.
        let jobs = [plain, (9000, 0), stencil, a, b, soft];
        let results = convert_all(&doc, &jobs, &masks, true, true, LARGE);
        let ids: Vec<ObjectId> = results.iter().map(|r| r.0).collect();
        assert_eq!(ids, [plain, stencil, a, b, soft]);
        for (id, old, outcome) in &results {
            assert_eq!(*old, doc.objects.get(id).unwrap().as_stream().unwrap().content.len());
            // Only the image that neither is a mask nor has one becomes a JPEG.
            assert_eq!(matches!(outcome, Outcome::Jpeg(..)), *id == plain, "{id:?}");
        }
    }

    /// What a zlib reader makes of a packed stream: the filtered rows, with
    /// the checksum verified.
    fn inflate(s: &Stream) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(&s.content[..]).read_to_end(&mut out).expect("a valid zlib stream");
        out
    }

    /// Images above the memory limit are decoded and packed row by row, in
    /// bands on several cores, and keep every sample.
    #[test]
    fn large_images_are_packed_row_by_row() {
        let doc = Document::with_version("1.7");
        // Three bands (the last one short): a drawing with a photograph
        // across the top, so that every PNG filter is chosen somewhere.
        let (w, h) = (1500usize, 2000usize);
        assert_eq!((h * w * 3).div_ceil(BAND / (w * 3) * (w * 3)), 3);
        let mut data = common::drawing(w, h);
        data[..w * 3 * 200].copy_from_slice(&common::photo(w, 200, 3));
        let size = Size { width: w, height: h, comps: 3 };
        let small = Memory::new(1 << 20);
        let image = |extra: Dictionary, content: Vec<u8>| common::image_stream(w, h, name("DeviceRGB"), extra, content);
        let zlib = |bytes: &[u8]| util::flate(bytes);
        let png = dictionary! { "Predictor" => 15, "Colors" => 3, "Columns" => w as i64 };
        let mut tiff = data.clone();
        for line in tiff.chunks_mut(w * 3) {
            for i in (3..line.len()).rev() {
                line[i] = line[i].wrapping_sub(line[i - 3]);
            }
        }
        let mut padded = data.clone();
        padded.extend_from_slice(b"\r\n");
        let stored: Vec<(&str, Stream)> = vec![
            ("uncompressed, with a line end", image(Dictionary::new(), padded)),
            ("Flate", image(dictionary! { "Filter" => "FlateDecode" }, zlib(&data))),
            ("Flate in a chain of one, parameters without predictor", image(dictionary! { "Filter" => vec![name("FlateDecode")], "DecodeParms" => vec![Object::Dictionary(dictionary! { "Columns" => 1 })] }, zlib(&data))),
            ("PNG predictor, all five filters", image(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => png.clone() }, zlib(&common::png_predict(&data, w * 3, 3)))),
            ("TIFF predictor", image(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => vec![Object::Dictionary(dictionary! { "Predictor" => 2, "Colors" => 3, "Columns" => w as i64 })] }, zlib(&tiff))),
        ];
        let mut packed_once = None;
        for (how, s) in &stored {
            let mut rows = Rows::new(s, size).expect(how);
            let packed = pack_rows(&mut |line| rows.next(line), size, s.dict.clone()).expect(how);
            assert!(rows.at_end(), "{how}");
            assert_eq!(inflate(&packed).len(), (w * 3 + 1) * h, "{how}");
            assert_eq!(samples(&packed).expect(how), data, "{how}");
            assert_eq!(filter(&packed), b"FlateDecode");
            // The same bytes as from the samples in memory.
            let in_memory = pack(&Raw { width: w, height: h, comps: 3, data: data.clone() }, s.dict.clone()).unwrap();
            assert_eq!(packed.content, in_memory.content, "{how}");
            packed_once = Some(packed);
        }
        // Through the front door: above the limit an image is packed, never
        // made a JPEG, and left alone when only JPEG was asked for.
        assert!(matches!(convert(&doc, &stored[0].1, true, true, &small), Some(Outcome::Packed(p)) if samples(&p).unwrap() == data));
        assert!(convert(&doc, &stored[0].1, true, false, &small).is_none());
        let photo = image_of(common::photo(W, H, 3));
        assert!(matches!(convert(&doc, &photo, true, true, &Memory::new(LARGE)), Some(Outcome::Jpeg(..))));
        assert!(matches!(convert(&doc, &photo, true, true, &Memory::new(W * H * 3 - 1)), Some(Outcome::Packed(_))));
        // What is packed already does not get smaller again.
        assert!(convert(&doc, &packed_once.unwrap(), true, true, &small).is_none());

        // Stored in a way that cannot be read row by row, or damaged: left alone.
        let mut cases: Vec<(&str, Stream)> = Vec::new();
        cases.push(("LZW", image(dictionary! { "Filter" => "LZWDecode" }, common::lzw_encode(&data[..w * 3 * 40]))));
        cases.push(("two filters", image(dictionary! { "Filter" => vec![name("ASCII85Decode"), name("FlateDecode")] }, zlib(&data))));
        cases.push(("odd filter", image(dictionary! { "Filter" => 7 }, zlib(&data))));
        cases.push(("odd filter in a chain", image(dictionary! { "Filter" => vec![Object::Integer(7)] }, zlib(&data))));
        cases.push(("predictor for other rows", image(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => dictionary! { "Predictor" => 15, "Colors" => 3, "Columns" => w as i64 + 1 } }, zlib(&data))));
        cases.push(("unknown predictor", image(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => dictionary! { "Predictor" => 3, "Colors" => 3, "Columns" => w as i64 } }, zlib(&data))));
        cases.push(("too short", image(Dictionary::new(), data[..data.len() - 1].to_vec())));
        let mut long = data.clone();
        long.extend_from_slice(b"abc");
        cases.push(("too long", image(Dictionary::new(), long)));
        let mut odd = common::png_predict(&data, w * 3, 3);
        odd[(w * 3 + 1) * 7] = 9;
        cases.push(("invalid PNG filter", image(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => png.clone() }, zlib(&odd))));
        cases.push(("PNG rows end early", image(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => png }, zlib(&common::png_predict(&data[..w * 3 * 50], w * 3, 3)))));
        let mut broken = zlib(&data);
        broken.truncate(broken.len() / 2);
        cases.push(("Flate data cut off", image(dictionary! { "Filter" => "FlateDecode" }, broken)));
        for (why, s) in &cases {
            assert!(convert(&doc, s, true, true, &small).is_none(), "{why}");
        }
    }

    fn image_of(samples: Vec<u8>) -> Stream {
        common::image_stream(W, H, name("DeviceRGB"), Dictionary::new(), samples)
    }

    #[test]
    fn memory_is_shared_between_the_threads() {
        let memory = Memory::new(100);
        // Never more than everything: the largest image runs alone.
        assert_eq!(memory.take(1000), 400);
        std::thread::scope(|scope| {
            let waiting = scope.spawn(|| {
                let got = memory.take(50);
                memory.give(got);
                got
            });
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert!(!waiting.is_finished(), "nothing is free yet");
            memory.give(400);
            assert_eq!(waiting.join().unwrap(), 50);
        });
        assert_eq!(*memory.free.lock().unwrap(), 400);
        // Several images in a small budget still all get their turn.
        let mut doc = Document::with_version("1.7");
        let mut jobs = Vec::new();
        for _ in 0..6 {
            jobs.push(doc.add_object(image_of(common::photo(W, H, 3))));
        }
        assert_eq!(convert_all(&doc, &jobs, &BTreeSet::new(), true, true, W * H * 3).len(), 6);
    }

    #[test]
    fn checksums_of_bands_combine() {
        let data = common::photo(400, 300, 1);
        let sum = |bytes: &[u8]| {
            let mut a = adler2::Adler32::new();
            a.write_slice(bytes);
            a.checksum()
        };
        for cut in [0, 1, 5000, 70_000, data.len()] {
            let (first, second) = data.split_at(cut);
            assert_eq!(adler_combine(sum(first), sum(second), second.len()), sum(&data), "{cut}");
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
            let packed = pack(&Raw { width: W, height: H, comps, data: data.clone() }, dictionary! { "Subtype" => "Image" }).unwrap();
            assert_eq!(filter(&packed), b"FlateDecode");
            assert_eq!(samples(&packed).unwrap(), data);
            assert!(packed.content.len() < util::flate(&data).len(), "{comps} channels");
        }
        // Flat rows take the filter that costs nothing.
        let flat = vec![200u8; W * H];
        let packed = pack(&Raw { width: W, height: H, comps: 1, data: flat.clone() }, Dictionary::new()).unwrap();
        assert_eq!(samples(&packed).unwrap(), flat);
    }
}
