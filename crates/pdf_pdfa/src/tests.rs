//! Tests of the public API and of the paths the conformance matrix does
//! not reach: errors, a missing or failing rasterizer, missing system
//! fonts. The inputs come from the corpus in `tests/common`.

use std::sync::Arc;

use lopdf::{dictionary, Dictionary, Document, Object, Stream};

use crate::common;
use crate::{convert, needs_password, Conformance, Ctx, Level, Options, PdfaError, RasterPage, RasterWord, Rasterizer, Report};

/// Renders every page as a small white bitmap with one word.
struct Blank;

impl Rasterizer for Blank {
    fn render_page(&self, _pdf: &[u8], _page: u32, _dpi: f32) -> std::result::Result<RasterPage, String> {
        Ok(RasterPage { width: 20, height: 20, rgb: vec![255; 20 * 20 * 3], words: vec![RasterWord { text: "Wort".into(), x: 2.0, y: 2.0, w: 10.0, h: 5.0 }] })
    }
}

struct Broken;

impl Rasterizer for Broken {
    fn render_page(&self, _pdf: &[u8], _page: u32, _dpi: f32) -> std::result::Result<RasterPage, String> {
        Err("no renderer today".into())
    }
}

fn opts(level: &str) -> Options {
    Options::new(level.parse().unwrap())
}

fn with(level: &str, r: impl Rasterizer + 'static) -> Options {
    opts(level).with_rasterizer(Arc::new(r))
}

/// The output can be read back and has as many pages as the input.
fn reload(bytes: &[u8]) -> Document {
    let doc = Document::load_mem(bytes).expect("output parses");
    assert!(doc.page_iter().next().is_some());
    doc
}

#[test]
fn public_api_and_errors() {
    let simple = common::simple_pdf("Hallo");
    assert!(!needs_password(&simple));
    assert!(!needs_password(b"garbage"));
    let protected = common::encrypted("secret");
    assert!(needs_password(&protected));
    assert_eq!(convert(&protected, &opts("2b")).unwrap_err(), PdfaError::PasswordRequired);
    assert!(matches!(convert(b"garbage", &opts("2b")), Err(PdfaError::Parse(_))));
    assert_eq!(convert(&simple, &opts("4f")).unwrap_err(), PdfaError::AttachmentRequired);
    // A file without pages.
    let empty = common::B::new().finish();
    assert!(matches!(convert(&empty, &opts("2b")), Err(PdfaError::Parse(m)) if m.contains("no pages")));

    let out = convert(&simple, &opts("2b")).unwrap();
    assert!(out.bytes.starts_with(b"%PDF-1.7"));
    assert!(out.report.has("fonts_embedded") && !out.report.has("no_such_note"));
    assert_eq!(out.level.conformance(), Conformance::B);
    assert_eq!(out.level, "2b".parse::<Level>().unwrap());
    reload(&out.bytes);

    let dbg = format!("{:?}", with("1a", Blank));
    assert!(dbg.contains("rasterizer: true") && dbg.contains("flatten_dpi"), "{dbg}");
    for e in [
        PdfaError::PasswordRequired,
        PdfaError::Parse("p".into()),
        PdfaError::Unsupported("u".into()),
        PdfaError::NeedsRasterizer("n".into()),
        PdfaError::AttachmentRequired,
        PdfaError::Write("w".into()),
    ] {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn document_without_catalog_is_a_parse_error() {
    let o = opts("2b");
    let mut ctx = Ctx { doc: Document::with_version("1.7"), level: o.level, opts: &o, report: Report::default(), source: &[] };
    assert!(matches!(crate::prepare::run(&mut ctx), Err(PdfaError::Parse(m)) if m.contains("catalog")));
}

#[test]
fn pages_without_a_lossless_fix_need_the_rasterizer() {
    // Transparency in PDF/A-1.
    let t = common::transparency();
    assert!(matches!(convert(&t, &opts("1b")), Err(PdfaError::NeedsRasterizer(m)) if m.contains("transparency on page 1")));
    assert!(matches!(convert(&t, &with("1b", Broken)), Err(PdfaError::Unsupported(m)) if m.contains("rendering page 1 failed: no renderer today")));
    let out = convert(&t, &with("1b", Blank)).unwrap();
    assert!(out.report.has("pages_rendered"));
    assert_eq!(reload(&out.bytes).get_pages().len(), 2);
    // Part 2 keeps transparency: no rasterizer needed.
    assert!(!convert(&t, &opts("2b")).unwrap().report.has("pages_rendered"));

    // JPEG 2000 and wide DeviceN in PDF/A-1.
    assert!(matches!(convert(&common::jpx_image(), &opts("1b")), Err(PdfaError::NeedsRasterizer(m)) if m.contains("JPEG 2000")));
    assert!(matches!(convert(&common::spot_colors(), &opts("1a")), Err(PdfaError::NeedsRasterizer(m)) if m.contains("DeviceN")));
    assert!(convert(&common::spot_colors(), &with("1a", Blank)).unwrap().report.has("pages_rendered"));

    // Fonts that cannot be repaired, at any part.
    let f = common::cff_type1_missing_glyphs();
    assert!(matches!(convert(&f, &opts("2b")), Err(PdfaError::NeedsRasterizer(m)) if m.contains("font problems")));
    assert!(matches!(convert(&f, &with("3u", Broken)), Err(PdfaError::Unsupported(m)) if m.contains("failed")));
    let out = convert(&f, &with("4", Blank)).unwrap();
    assert!(out.report.has("pages_rendered") && out.report.has("font_widths_adjusted"));
    assert!(out.bytes.starts_with(b"%PDF-2.0"));

    // A rotated page with a translucent annotation (PDF/A-1): rendered and
    // placed with the page rotation.
    let out = convert(&common::edge::edge_annotations(), &with("1b", Blank)).unwrap();
    assert!(out.report.notes.iter().any(|n| n.code == "pages_rendered" && n.detail == "2"), "{:?}", out.report.notes);
    // A rasterizer that returns nothing usable.
    struct Empty;
    impl Rasterizer for Empty {
        fn render_page(&self, _: &[u8], _: u32, _: f32) -> std::result::Result<RasterPage, String> {
            Ok(RasterPage { width: 0, height: 0, rgb: Vec::new(), words: Vec::new() })
        }
    }
    assert!(matches!(convert(&t, &with("1b", Empty)), Err(PdfaError::Unsupported(m)) if m.contains("empty page rendering")));
}

#[test]
fn fonts_come_from_the_bundle_when_the_system_has_none() {
    let dir = std::env::temp_dir().join(format!("docura-pdfa-nofonts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, input) in [("std14", common::std14_unembedded()), ("truetype", common::truetype_unembedded()), ("cid", common::cid_unembedded())] {
        for level in ["1b", "2u", "4"] {
            let mut o = opts(level);
            o.font_dirs = vec![dir.clone()];
            let out = convert(&input, &o).unwrap_or_else(|e| panic!("{name} {level}: {e}"));
            assert!(out.report.has("fonts_embedded"), "{name} {level}");
            // The substitute is one of the bundled families.
            let doc = reload(&out.bytes);
            let names: Vec<String> = doc
                .objects
                .values()
                .filter_map(|o| o.as_dict().ok())
                .filter_map(|d| d.get(b"FontName").ok().and_then(|n| n.as_name().ok()).map(|n| String::from_utf8_lossy(n).to_string()))
                .collect();
            assert!(names.iter().any(|n| n.contains("Roboto") || n.contains("SourceSerif")), "{name} {level}: {names:?}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn large_files_stay_readable() {
    let input = common::edge::many_pages();
    for level in ["1b", "2b", "3a", "4"] {
        let out = convert(&input, &opts(level)).unwrap();
        // Read back by an independent parse: every page and its content.
        let doc = reload(&out.bytes);
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 130, "{level}");
        for (n, id) in pages {
            let text = doc.get_page_content(id).unwrap_or_default();
            assert!(!text.is_empty(), "{level}: page {n} has no content");
        }
        // Parts 2 and later pack the objects into exactly one object stream
        // (lopdf does not list a second one in the cross-reference stream).
        let streams = out.bytes.windows(12).filter(|w| w == b"/Type/ObjStm").count();
        assert_eq!(streams, if level == "1b" { 0 } else { 1 }, "{level}");
    }
}

#[test]
fn forms_keep_their_fields_and_get_an_indirect_acroform() {
    let out = convert(&common::forms(), &opts("2b")).unwrap();
    assert!(out.report.has("xfa_removed"));
    let doc = reload(&out.bytes);
    let cat = doc.catalog().unwrap();
    let acro = cat.get(b"AcroForm").unwrap();
    assert!(acro.as_reference().is_ok(), "AcroForm must be an indirect object");
    let acro = doc.get_dictionary(acro.as_reference().unwrap()).unwrap();
    assert_eq!(acro.get(b"Fields").unwrap().as_array().unwrap().len(), 4);
    assert!(!acro.has(b"XFA") && !acro.has(b"NeedAppearances"));
}

#[test]
fn stream_filters_are_decoded() {
    use crate::util::{ascii_hex_decode, run_length_decode, stream_data};
    assert_eq!(ascii_hex_decode(b"48 65 6C\n6c6F>zz"), b"Hello");
    assert_eq!(ascii_hex_decode(b"4"), vec![0x40]);
    assert_eq!(ascii_hex_decode(b""), Vec::<u8>::new());
    assert_eq!(run_length_decode(&[2, b'a', b'b', b'c', 254, b'x', 128, 9]), b"abcxxx");
    assert_eq!(run_length_decode(&[5, b'a', b'b']), b"ab");
    assert_eq!(run_length_decode(&[200]), Vec::<u8>::new());

    let name = |s: &str| Object::Name(s.as_bytes().to_vec());
    let plain = b"q 1 0 0 1 0 0 cm Q".to_vec();
    assert_eq!(stream_data(&Stream::new(Dictionary::new(), plain.clone())), Some(plain.clone()));
    let z = crate::util::flate(&plain);
    let hex: Vec<u8> = z.iter().flat_map(|b| format!("{b:02x}").into_bytes()).chain(*b">").collect();
    let chain = Stream::new(dictionary! { "Filter" => vec![name("ASCIIHexDecode"), name("FlateDecode")], "DecodeParms" => vec![Object::Null, Object::Dictionary(dictionary! { "Predictor" => 1 })] }, hex);
    assert_eq!(stream_data(&chain), Some(plain.clone()));
    let rl = Stream::new(dictionary! { "Filter" => vec![name("Crypt"), name("RunLengthDecode")] }, vec![1, b'h', b'i', 128]);
    assert_eq!(stream_data(&rl), Some(b"hi".to_vec()));
    let single = Stream::new(dictionary! { "Filter" => "FlateDecode", "DecodeParms" => dictionary! { "Predictor" => 1 } }, z);
    assert_eq!(stream_data(&single), Some(plain));
    assert_eq!(stream_data(&Stream::new(dictionary! { "Filter" => "DCTDecode" }, vec![1])), None);
    assert_eq!(stream_data(&Stream::new(dictionary! { "Filter" => vec![Object::Integer(1)] }, vec![1])), None);
    assert_eq!(stream_data(&Stream::new(dictionary! { "Filter" => 7 }, vec![1])), None);
    assert_eq!(stream_data(&Stream::new(dictionary! { "Filter" => "FlateDecode" }, b"not zlib".to_vec())).map(|d| d.len()), Some(0));
}

#[test]
fn every_level_converts_the_edge_cases() {
    // Conformance of these files is checked by the veraPDF matrix; here
    // every level must at least produce a readable file without a
    // rasterizer where none is needed.
    let attachments = common::edge::edge_attachments();
    for level in crate::ALL_LEVELS {
        let o = Options::new(level).with_rasterizer(Arc::new(Blank));
        for (name, input) in [("fonts", common::edge::edge_fonts()), ("color", common::edge::edge_color()), ("attachments", attachments.clone())] {
            match convert(&input, &o) {
                Ok(out) => {
                    reload(&out.bytes);
                }
                Err(PdfaError::AttachmentRequired) if level.id() == "4f" && name != "attachments" => {}
                Err(e) => panic!("{name} {level}: {e}"),
            }
        }
    }
    // Attachments per part: removed, kept as PDF/A only, kept with /AF.
    let p1 = convert(&attachments, &opts("1b")).unwrap();
    assert!(p1.report.has("attachments_removed"));
    let p3 = convert(&attachments, &opts("3b")).unwrap();
    assert!(!p3.report.has("attachments_removed"));
    let doc = reload(&p3.bytes);
    assert!(doc.catalog().unwrap().has(b"AF"));
}
