//! Dev utility: export a PDF's text layout as .docx (and .xlsx) the same
//! way the app command does — for inspecting exporter output.
//!
//!     cargo run -p pdf_pdfium --example dump_docx -- in.pdf out.docx

use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::PageIndex;

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("usage: dump_docx in.pdf out.docx");
    let output = args.next().unwrap_or_else(|| "out.docx".into());

    let engine = PdfiumEngine::new().expect("pdfium");
    let id = engine.open_bytes(std::fs::read(&input).expect("read")).expect("open");
    let n = engine.page_count(id).unwrap();
    let mut pages = Vec::new();
    for p in 0..n {
        pages.push(engine.page_text_layout(id, PageIndex(p)).unwrap());
    }
    let bytes = pdf_export::build_docx(&pages).expect("build docx");
    std::fs::write(&output, &bytes).expect("write");
    println!("{output}: {} pages, {} bytes", n, bytes.len());
    let xlsx = output.replace(".docx", ".xlsx");
    std::fs::write(&xlsx, pdf_export::build_xlsx(&pages).expect("xlsx")).expect("write xlsx");
    println!("{xlsx} written");
}
