//! Dev utility: list the streams of a PDF (images / forms / content) with
//! size, filters and image dimensions — to see what makes a page heavy.
//!
//!     cargo run --release -p pdf_pdfium --example plan_objects -- file.pdf
fn main() {
    let path = std::env::args().nth(1).expect("usage: plan_objects file.pdf");
    let doc = lopdf::Document::load(&path).expect("load");
    let mut rows: Vec<(usize, String)> = Vec::new();
    for (id, obj) in &doc.objects {
        let Ok(s) = obj.as_stream() else { continue };
        let d = &s.dict;
        let name = |k: &[u8]| {
            d.get(k)
                .and_then(|o| o.as_name())
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default()
        };
        let filt = match d.get(b"Filter") {
            Ok(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
            Ok(lopdf::Object::Array(a)) => a
                .iter()
                .filter_map(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .collect::<Vec<_>>()
                .join("+"),
            _ => String::new(),
        };
        let int = |k: &[u8]| d.get(k).and_then(|o| o.as_i64()).unwrap_or(0);
        rows.push((
            s.content.len(),
            format!(
                "{:?} sub={:<6} filter={:<14} len={:>9} {}x{} bpc={} smask={} cs={}",
                id,
                name(b"Subtype"),
                filt,
                s.content.len(),
                int(b"Width"),
                int(b"Height"),
                int(b"BitsPerComponent"),
                d.has(b"SMask"),
                format!("{:?}", d.get(b"ColorSpace").ok()).chars().take(40).collect::<String>()
            ),
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, r) in rows.iter().take(15) {
        println!("{r}");
    }
}
