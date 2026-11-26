//! Convert one PDF to PDF/A from the command line and print what changed.
//!
//! ```text
//! cargo run --release -p pdf_pdfa --example pdfa_convert -- <in.pdf> <level> <out.pdf>
//! ```
//!
//! `level` is 1a, 1b, 2a, 2b, 2u, 3a, 3b, 3u, 4, 4e or 4f. PDFium renders
//! the pages that have no lossless fix, as in the app.

#[path = "../tests/common/mod.rs"]
mod common;

use pdf_pdfa::{convert, Level, Options};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, level, output] = args.as_slice() else {
        eprintln!("usage: pdfa_convert <in.pdf> <level> <out.pdf>");
        std::process::exit(2);
    };
    let level: Level = level.parse().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let bytes = std::fs::read(input).unwrap_or_else(|e| {
        eprintln!("{input}: {e}");
        std::process::exit(1);
    });
    let opts = Options::new(level).with_rasterizer(common::PdfiumRasterizer::new());
    let t = std::time::Instant::now();
    match convert(&bytes, &opts) {
        Ok(out) => {
            std::fs::write(output, &out.bytes).unwrap_or_else(|e| {
                eprintln!("{output}: {e}");
                std::process::exit(1);
            });
            println!("{} in {:.0} ms, {} → {} bytes", level.label(), t.elapsed().as_secs_f64() * 1000.0, bytes.len(), out.bytes.len());
            for n in &out.report.notes {
                println!("  {}: {}", n.code, n.detail);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
