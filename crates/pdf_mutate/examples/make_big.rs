//! Dev utility: generate a large many-page text PDF for manual testing.
//!
//!     cargo run -p pdf_mutate --example make_big -- [pages] [out.pdf]
//!
//! Each page carries ~40 lines of selectable Helvetica text.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};

fn main() {
    let mut args = std::env::args().skip(1);
    let pages: u32 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(800);
    let out = args.next().unwrap_or_else(|| "big.pdf".into());

    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let font_id = d.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }));

    let mut kids: Vec<Object> = Vec::with_capacity(pages as usize);
    for p in 0..pages {
        let mut content = String::from("BT /F1 11 Tf 56 760 Td 18 TL\n");
        for line in 0..40 {
            content.push_str(&format!(
                "(Seite {} Zeile {} - Lorem ipsum dolor sit amet, consetetur sadipscing elitr, kontakt{}@example.com Tel. 0171 2345{:04}) Tj T*\n",
                p + 1,
                line + 1,
                p,
                line
            ));
        }
        content.push_str("ET\n");
        let content_id = d.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            content.into_bytes(),
        )));
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        let page_id = d.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(fonts),
            }),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        }));
        kids.push(Object::Reference(page_id));
    }

    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(pages as i64),
        }),
    );
    let cat_id = d.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    d.trailer.set("Root", Object::Reference(cat_id));
    d.max_id = d.objects.keys().map(|k| k.0).max().unwrap_or(1);

    d.save(&out).expect("save");
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!("{out}: {pages} pages, {:.1} MB", size as f64 / 1e6);
}
