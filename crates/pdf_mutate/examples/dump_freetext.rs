// Quick debug helper. Build a one-page PDF, add a FreeText annotation, dump bytes.
// Run: `cargo run -p pdf_mutate --example dump_freetext > /tmp/ft.pdf`

use std::io::Write;

use lopdf::dictionary;

fn main() {
    let mut bytes: Vec<u8> = Vec::new();

    // Build a tiny one-page PDF in-memory with lopdf.
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let resources_id = doc.add_object(lopdf::Dictionary::new());

    let content_id = doc.add_object(lopdf::Object::Stream(lopdf::Stream::new(
        lopdf::Dictionary::new(),
        b"q\nQ\n".to_vec(),
    )));

    let page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources_id,
        "Contents" => content_id,
    };
    doc.objects.insert(page_id, lopdf::Object::Dictionary(page_dict));

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects
        .insert(pages_id, lopdf::Object::Dictionary(pages_dict));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.compress();

    let mut buf: Vec<u8> = Vec::new();
    doc.save_to(&mut std::io::Cursor::new(&mut buf)).unwrap();
    bytes = buf;

    // Now mutate via pdf_mutate.
    let mut mdoc = pdf_mutate::MutableDoc::from_bytes(&bytes).unwrap();
    let annot = pdf_mutate::FreeTextAnnotation {
        rect: pdf_types::RectPt {
            x0: 100.0,
            y0: 100.0,
            x1: 320.0,
            y1: 140.0,
        },
        text: "Hello world".into(),
        color: [0.0, 0.0, 0.0],
        bg_color: [1.0, 1.0, 1.0, 1.0],
        font_size: 14.0,
        font_family: "Helv".into(),
        rotation_quadrant: 0,
        opacity: 1.0,
        author: None,
        print: true,
    };
    mdoc.add_freetext_annotation(0, &annot).unwrap();
    let out = mdoc.to_bytes().unwrap();
    let _ = std::io::stdout().lock().write_all(&out);
}
