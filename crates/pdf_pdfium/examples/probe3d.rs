//! Dev probe: what does the current pipeline do with 3D PDFs?
//!
//!     cargo run -p pdf_pdfium --example probe3d -- out_dir file.pdf...
//!
//! For every file: PDFium annotation types + a PNG of page 1, the lopdf
//! annotation list (comment panel source) and whether a lopdf save keeps the
//! 3D stream. Also writes `<name>.poster.pdf`, a copy whose 3D annotation got
//! a real /AP /N poster, to check that PDFium draws posters.

use lopdf::{dictionary, Object, Stream};
use pdfium_render::prelude::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let out_dir =
        std::path::PathBuf::from(args.next().expect("usage: probe3d out_dir file.pdf..."));
    std::fs::create_dir_all(&out_dir).unwrap();
    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            "./vendor/pdfium",
        ))
        .expect("bind pdfium"),
    );
    for path in args {
        let path = std::path::PathBuf::from(path);
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).expect("read");
        println!("=== {stem} ({} bytes)", bytes.len());
        probe_pdfium(&pdfium, &bytes, &out_dir.join(format!("{stem}.png")));

        let mut doc = pdf_mutate::MutableDoc::from_bytes(&bytes).expect("lopdf");
        for a in doc.list_annotations() {
            println!(
                "  list_annotations: page={} subtype={} rect=({:.0},{:.0},{:.0},{:.0}) contents={:?}",
                a.page, a.subtype, a.rect_pt.x0, a.rect_pt.y0, a.rect_pt.x1, a.rect_pt.y1, a.contents
            );
        }
        let saved = doc.to_bytes().expect("save");
        let kept = lopdf::Document::load_mem(&saved)
            .map(|d| {
                d.objects.values().any(|o| {
                    o.as_stream()
                        .ok()
                        .and_then(|s| s.dict.get(b"Type").ok())
                        .and_then(|t| t.as_name().ok())
                        == Some(b"3D".as_slice())
                })
            })
            .unwrap_or(false);
        println!("  lopdf save keeps /Type /3D stream: {kept}");

        // Poster variant.
        let mut lo = lopdf::Document::load_mem(&bytes).unwrap();
        let annots: Vec<lopdf::ObjectId> = lo
            .objects
            .iter()
            .filter(|(_, o)| {
                o.as_dict()
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok())
                    .and_then(|s| s.as_name().ok())
                    == Some(b"3D".as_slice())
            })
            .map(|(id, _)| *id)
            .collect();
        for id in annots {
            // Pull the model out of /3DD (FlateDecode or raw) and save it.
            let model = lo
                .get_dictionary(id)
                .unwrap()
                .get(b"3DD")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|r| lo.get_object(r).ok())
                .and_then(|o| o.as_stream().ok())
                .map(|s| {
                    let sub = s
                        .dict
                        .get(b"Subtype")
                        .ok()
                        .and_then(|n| n.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string());
                    (
                        sub,
                        s.decompressed_content()
                            .unwrap_or_else(|_| s.content.clone()),
                    )
                });
            if let Some((sub, bytes)) = model {
                let ext = if sub.as_deref() == Some("PRC") {
                    "prc"
                } else {
                    "u3d"
                };
                let path = out_dir.join(format!("{stem}.{ext}"));
                std::fs::write(&path, &bytes).unwrap();
                println!(
                    "  /3DD /Subtype {sub:?}: {} bytes -> {}",
                    bytes.len(),
                    path.display()
                );
            }
            let rect: Vec<f32> = lo
                .get_dictionary(id)
                .unwrap()
                .get(b"Rect")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|o| {
                    o.as_float()
                        .or_else(|_| o.as_i64().map(|i| i as f32))
                        .unwrap()
                })
                .collect();
            let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
            let content = format!(
                "0.85 0.9 1 rg 0 0 {w} {h} re f 0 0 0.6 RG 4 w 0 0 m {w} {h} l {w} 0 m 0 {h} l S"
            );
            let form = lo.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                },
                content.into_bytes(),
            ));
            let d = lo.get_dictionary_mut(id).unwrap();
            d.set("AP", dictionary! { "N" => Object::Reference(form) });
        }
        let poster = out_dir.join(format!("{stem}.poster.pdf"));
        lo.save(&poster).unwrap();
        let pb = std::fs::read(&poster).unwrap();
        println!("  poster variant:");
        probe_pdfium(&pdfium, &pb, &out_dir.join(format!("{stem}.poster.png")));
    }
}

fn probe_pdfium(pdfium: &Pdfium, bytes: &[u8], png: &std::path::Path) {
    let doc = match pdfium.load_pdf_from_byte_slice(bytes, None) {
        Ok(d) => d,
        Err(e) => {
            println!("  pdfium open FAILED: {e:?}");
            return;
        }
    };
    let page = doc.pages().get(0).expect("page");
    let kinds: Vec<String> = page
        .annotations()
        .iter()
        .map(|a| format!("{:?}", a.annotation_type()))
        .collect();
    println!("  pdfium annotations: {kinds:?}");
    let img = page
        .render_with_config(&PdfRenderConfig::new().scale_page_by_factor(1.0))
        .expect("render")
        .as_image();
    let rgb = img.to_rgb8();
    let non_white = rgb.pixels().filter(|p| p.0 != [255, 255, 255]).count();
    println!(
        "  pdfium render {}x{}: {} non-white pixels",
        rgb.width(),
        rgb.height(),
        non_white
    );
    rgb.save(png).unwrap();
}
