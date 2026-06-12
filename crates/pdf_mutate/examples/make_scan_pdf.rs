//! Dev utility: wrap one or more images into an image-only PDF (no text
//! layer) — a stand-in for a scanned document when testing OCR.
//!
//!     cargo run -p pdf_mutate --example make_scan_pdf -- out.pdf img1.png [img2 …]

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("usage: make_scan_pdf OUT.pdf IMG…");
    let images: Vec<Vec<u8>> = args.map(|p| std::fs::read(&p).expect(&p)).collect();
    let pdf = pdf_mutate::images_to_pdf(&images).expect("build pdf");
    std::fs::write(&out, pdf).expect("write pdf");
    println!("wrote {out} ({} page(s))", images.len());
}
