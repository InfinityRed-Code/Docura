//! A page-size image must not be held in memory as a whole.
//!
//! Scanned or rendered plans are often one image per page: a few megabytes
//! in the file, gigabytes once decoded. The converter decodes and packs
//! such an image row by row. This test converts a synthetic one and
//! measures, with a counting allocator, how much memory the conversion
//! holds at its peak: it has to stay well below the size of the samples,
//! and the samples have to come out unchanged.
//!
//! One test function only: the allocator counts for the whole process.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use lopdf::{dictionary, Dictionary, Document, Object};
use pdf_pdfa::{convert, Options};

static HELD: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;

fn grown(bytes: usize) {
    let now = HELD.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            grown(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        HELD.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let q = System.realloc(p, layout, new_size);
        if !q.is_null() {
            HELD.fetch_sub(layout.size(), Ordering::Relaxed);
            grown(new_size);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const W: usize = 12_000;
const H: usize = 8_000;

/// Row `y` of a drawing: white with a frame, a grid of lines and a band of
/// hatching, one byte per pixel.
fn row(y: usize) -> Vec<u8> {
    let mut line = vec![255u8; W];
    if y < 40 || y >= H - 40 || y % 500 < 3 {
        line.fill(0);
        return line;
    }
    for x in (0..W).step_by(400) {
        line[x..x + 3].fill(0);
    }
    line[..40].fill(0);
    line[W - 40..].fill(0);
    if (3000..3400).contains(&y) {
        for x in (2000 + y % 16..6000).step_by(16) {
            line[x] = 90;
        }
    }
    line
}

#[test]
fn a_page_size_image_is_never_held_as_a_whole() {
    // The image as renderers store it: Flate with the PNG "Up" filter on
    // every row.
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut above = vec![0u8; W];
    for y in 0..H {
        let line = row(y);
        let mut filtered = vec![2u8];
        for (v, a) in line.iter().zip(&above) {
            filtered.push(v.wrapping_sub(*a));
        }
        encoder.write_all(&filtered).unwrap();
        above = line;
    }
    let stored = encoder.finish().unwrap();
    let mut b = common::B::new();
    let parms = dictionary! { "Predictor" => 15, "Colors" => 1, "Columns" => W as i64 };
    let image = b.doc.add_object(common::image_stream(W, H, Object::Name(b"DeviceGray".to_vec()), dictionary! { "Filter" => "FlateDecode", "DecodeParms" => parms }, stored));
    b.page(b"q 595 0 0 397 0 0 cm /Im1 Do Q", dictionary! { "XObject" => dictionary! { "Im1" => image } }, Dictionary::new());
    let input = b.finish();
    let samples = W * H;
    assert!(input.len() < samples / 50, "the file is small: {} bytes", input.len());

    // The limit of the test is far below the image, as the default
    // (512 MiB) is far below a page-size image.
    let mut opts = Options::new("2b".parse().unwrap());
    opts.max_image_bytes = 8 << 20;
    let before = HELD.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let out = convert(&input, &opts).unwrap();
    let peak = PEAK.load(Ordering::Relaxed) - before;
    println!("peak memory of the conversion: {peak} bytes for {samples} bytes of samples");
    assert!(peak < samples / 2, "the conversion held {peak} bytes at its peak; the image has {samples} bytes of samples");

    // Packed, and every sample as it was.
    assert!(out.bytes.len() < input.len(), "{} bytes became {}", input.len(), out.bytes.len());
    let banded = the_image(&out.bytes);
    let data = banded.decompressed_content().unwrap();
    assert_eq!(data.len(), samples);
    for (y, line) in data.chunks(W).enumerate() {
        assert!(line == row(y), "row {y} differs");
    }
    drop(data);

    // With the default limit the same image is worked on in memory: the
    // counter does see the difference.
    let before = HELD.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let in_memory = convert(&input, &Options::new("2b".parse().unwrap())).unwrap();
    assert!(PEAK.load(Ordering::Relaxed) - before > samples, "the counting allocator is not counting");

    // Both ways store the image as the same stream, byte for byte. The
    // files as a whole are not compared: each conversion writes its own
    // dates and document ID, which sit in the compressed object and
    // cross-reference streams, so the file sizes may differ by a few bytes.
    let whole = the_image(&in_memory.bytes);
    assert_eq!(whole.dict.get(b"Filter").ok(), banded.dict.get(b"Filter").ok());
    assert_eq!(whole.dict.get(b"DecodeParms").ok(), banded.dict.get(b"DecodeParms").ok());
    assert!(whole.content == banded.content, "the stored image differs: {} bytes in memory, {} bytes in bands", whole.content.len(), banded.content.len());
}

/// The one image of a converted file.
fn the_image(pdf: &[u8]) -> lopdf::Stream {
    let doc = Document::load_mem(pdf).unwrap();
    let mut images = doc.objects.into_values().filter_map(|o| match o {
        Object::Stream(s) if matches!(s.dict.get(b"Subtype"), Ok(Object::Name(n)) if n == b"Image") => Some(s),
        _ => None,
    });
    let image = images.next().expect("an image");
    assert!(images.next().is_none(), "more than one image");
    image
}
