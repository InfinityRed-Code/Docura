//! Dev utility: print the editable text lines of a page next to the page's
//! plain extracted text.
//!
//!     cargo run -p pdf_pdfium --example dump_blocks -- file.pdf [page]

use pdf_core::PdfEngine;
use pdf_pdfium::PdfiumEngine;
use pdf_types::PageIndex;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: dump_blocks file.pdf [page]");
    let page: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let bytes = std::fs::read(&path).expect("read");
    let engine = PdfiumEngine::new().expect("pdfium");
    let id = engine.open_bytes(bytes).expect("open");

    let text = engine.extract_text(id, PageIndex(page)).unwrap();
    println!("--- extract_text ({} chars)", text.chars().count());
    println!("{}", text.chars().take(600).collect::<String>());

    let blocks = engine.page_text_blocks(id, PageIndex(page)).unwrap();
    println!("--- {} blocks", blocks.len());
    for b in blocks.iter().take(40) {
        println!(
            "{:>3} obj  {:>5.1}pt {:5} {}{}{}  {:?}",
            b.objects.len(),
            b.font_size,
            b.font_family,
            if b.bold { "B" } else { "-" },
            if b.italic { "I" } else { "-" },
            if b.invisible { "H" } else { "-" },
            b.text
        );
    }
}
