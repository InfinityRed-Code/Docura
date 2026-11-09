//! Dev utility: decode the largest image of a PDF ourselves (lopdf, incl.
//! PNG predictor) and save a 1/N box-downsampled PNG — ground truth to
//! compare against PDFium's rendering of huge scans.
//!
//!     cargo run --release -p pdf_pdfium --example plan_decode -- file.pdf out.png [factor]
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: plan_decode file.pdf out.png [factor]");
    let out = args.next().expect("out.png");
    let f: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(32);
    let doc = lopdf::Document::load(&path).expect("load");
    let (id, s) = doc
        .objects
        .iter()
        .filter_map(|(id, o)| o.as_stream().ok().map(|s| (id, s)))
        .filter(|(_, s)| s.dict.get(b"Subtype").and_then(|o| o.as_name()).ok() == Some(b"Image"))
        .max_by_key(|(_, s)| s.content.len())
        .expect("no image");
    let w = s.dict.get(b"Width").unwrap().as_i64().unwrap() as usize;
    let h = s.dict.get(b"Height").unwrap().as_i64().unwrap() as usize;
    let t = std::time::Instant::now();
    let raw = s.decompressed_content().expect("decode");
    println!("{id:?}: {w}x{h}, decoded {} bytes in {:?}", raw.len(), t.elapsed());
    let (ow, oh) = (w / f, h / f);
    let mut img = image::RgbImage::new(ow as u32, oh as u32);
    for oy in 0..oh {
        for ox in 0..ow {
            let mut acc = [0u64; 3];
            for dy in 0..f {
                let row = (oy * f + dy) * w * 3;
                for dx in 0..f {
                    let i = row + (ox * f + dx) * 3;
                    acc[0] += raw[i] as u64;
                    acc[1] += raw[i + 1] as u64;
                    acc[2] += raw[i + 2] as u64;
                }
            }
            let n = (f * f) as u64;
            img.put_pixel(ox as u32, oy as u32, image::Rgb([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8]));
        }
    }
    img.save(&out).unwrap();
    println!("wrote {out} ({ow}x{oh})");
}
