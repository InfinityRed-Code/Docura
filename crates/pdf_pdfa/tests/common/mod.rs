//! Shared test support: a synthetic corpus covering the hard PDF/A cases,
//! a PDFium-backed rasterizer and the veraPDF runner.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use pdf_core::{PdfEngine, RenderRequest};
use pdf_types::{PageIndex, Rotation, ZoomLevel};

// ---------------------------------------------------------------- builder

pub struct B {
    pub doc: Document,
    pages: ObjectId,
    kids: Vec<Object>,
}

impl B {
    pub fn new() -> B {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        B { doc, pages, kids: Vec::new() }
    }

    pub fn page(&mut self, content: &[u8], resources: Dictionary, extra: Dictionary) -> ObjectId {
        let c = self.doc.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let mut d = dictionary! {
            "Type" => "Page",
            "Parent" => self.pages,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => c,
            "Resources" => resources,
        };
        // Not `extend`: that merges arrays (a second MediaBox would be appended).
        for (k, v) in extra.iter() {
            d.set(k.clone(), v.clone());
        }
        let id = self.doc.add_object(d);
        self.kids.push(id.into());
        id
    }

    pub fn finish_with(mut self, catalog_extra: Dictionary) -> Vec<u8> {
        let n = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => self.kids.clone(), "Count" => n }),
        );
        let mut cat = dictionary! { "Type" => "Catalog", "Pages" => self.pages };
        cat.extend(&catalog_extra);
        let cid = self.doc.add_object(cat);
        self.doc.trailer.set("Root", cid);
        let info = self.doc.add_object(dictionary! {
            "Title" => Object::string_literal("Corpus"),
            "Author" => Object::string_literal("Docura tests"),
            "CreationDate" => Object::string_literal("D:20240102030405+01'00'"),
        });
        self.doc.trailer.set("Info", info);
        let mut out = Vec::new();
        self.doc.save_to(&mut out).unwrap();
        out
    }

    pub fn finish(self) -> Vec<u8> {
        self.finish_with(Dictionary::new())
    }
}

pub fn helv(b: &mut B) -> ObjectId {
    b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" })
}

fn font_res(f: ObjectId) -> Dictionary {
    dictionary! { "Font" => dictionary! { "F1" => f } }
}

const TEXT: &[u8] = b"BT /F1 18 Tf 72 760 Td (PDF/A Testdokument \xc4\xd6\xdc \xe4\xf6\xfc \xdf \x80) Tj ET\n";

// ---------------------------------------------------------------- corpus

/// Expected outcome of converting a corpus file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    Ok,
    PasswordRequired,
    /// Only for 4f: no attachment present.
    AttachmentRequiredFor4f,
}

pub struct Case {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub expect: Expect,
}

pub fn corpus() -> Vec<Case> {
    let mut v = vec![
        Case { name: "std14_unembedded", bytes: std14_unembedded(), expect: Expect::Ok },
        Case { name: "truetype_unembedded", bytes: truetype_unembedded(), expect: Expect::Ok },
        Case { name: "type3_font", bytes: type3_font(), expect: Expect::Ok },
        Case { name: "cid_unembedded", bytes: cid_unembedded(), expect: Expect::Ok },
        Case { name: "embedded_bad_widths", bytes: embedded_bad_widths(), expect: Expect::Ok },
        Case { name: "transparency", bytes: transparency(), expect: Expect::Ok },
        Case { name: "device_colors", bytes: device_colors(), expect: Expect::Ok },
        Case { name: "jpx_image", bytes: jpx_image(), expect: Expect::Ok },
        Case { name: "app_annotations", bytes: app_annotations(), expect: Expect::Ok },
        Case { name: "raw_annotations", bytes: raw_annotations(), expect: Expect::Ok },
        Case { name: "forms", bytes: forms(), expect: Expect::Ok },
        Case { name: "attachments", bytes: attachments(), expect: Expect::Ok },
        Case { name: "encrypted_owner", bytes: encrypted(""), expect: Expect::Ok },
        Case { name: "encrypted_user", bytes: encrypted("secret"), expect: Expect::PasswordRequired },
        Case { name: "javascript_actions", bytes: javascript_actions(), expect: Expect::Ok },
        Case { name: "rotated_pages", bytes: rotated_pages(), expect: Expect::Ok },
        Case { name: "ocr_layer", bytes: ocr_layer(), expect: Expect::Ok },
        Case { name: "optional_content", bytes: optional_content(), expect: Expect::Ok },
        Case { name: "legacy_misc", bytes: legacy_misc(), expect: Expect::Ok },
        Case { name: "tagged_input", bytes: tagged_input(), expect: Expect::Ok },
        Case { name: "cff_type1_missing_glyphs", bytes: cff_type1_missing_glyphs(), expect: Expect::Ok },
        Case { name: "vertical_cjk", bytes: vertical_cjk(), expect: Expect::Ok },
        Case { name: "spot_colors", bytes: spot_colors(), expect: Expect::Ok },
        Case { name: "page_boxes", bytes: page_boxes(), expect: Expect::Ok },
    ];
    for c in &mut v {
        if c.expect == Expect::Ok && c.name != "attachments" {
            c.expect = Expect::AttachmentRequiredFor4f;
        }
    }
    v
}

pub fn std14_unembedded() -> Vec<u8> {
    let mut b = B::new();
    let mut fonts = Dictionary::new();
    let names = ["Helvetica", "Helvetica-Bold", "Times-Roman", "Times-Italic", "Courier", "Symbol", "ZapfDingbats"];
    let mut c = Vec::new();
    for (i, n) in names.iter().enumerate() {
        let mut fd = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => Object::Name(n.as_bytes().to_vec()) };
        if !matches!(*n, "Symbol" | "ZapfDingbats") {
            fd.set("Encoding", "WinAnsiEncoding");
        }
        let id = b.doc.add_object(fd);
        fonts.set(format!("F{i}"), id);
        let text: &[u8] = match *n {
            "Symbol" => b"abgd \x61\x62",
            "ZapfDingbats" => b"3456 ",
            _ => b"Gr\xfc\xdfe aus Stuttgart \x96 100\x80",
        };
        c.extend_from_slice(format!("BT /F{i} 14 Tf 72 {} Td (", 760 - 30 * i).as_bytes());
        c.extend_from_slice(text);
        c.extend_from_slice(b") Tj ET\n");
    }
    // Invisible text (render mode 3) with an unembedded font.
    c.extend_from_slice(b"BT 3 Tr /F0 10 Tf 72 400 Td (unsichtbar) Tj ET\n");
    b.page(&c, dictionary! { "Font" => fonts }, Dictionary::new());
    b.finish()
}

pub fn truetype_unembedded() -> Vec<u8> {
    let mut b = B::new();
    let widths: Vec<Object> = (32..=255).map(|_| Object::Integer(556)).collect();
    let fd = b.doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "Arial,Bold", "Flags" => 32,
        "FontBBox" => vec![(-600).into(), (-300).into(), 2000.into(), 1000.into()],
        "ItalicAngle" => 0, "Ascent" => 905, "Descent" => (-212), "CapHeight" => 716, "StemV" => 80,
    });
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Arial,Bold",
        "FirstChar" => 32, "LastChar" => 255, "Widths" => widths,
        "Encoding" => "WinAnsiEncoding", "FontDescriptor" => fd,
    });
    let calibri = b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Calibri", "Encoding" => "WinAnsiEncoding" });
    let res = dictionary! { "Font" => dictionary! { "F1" => f, "F2" => calibri } };
    b.page(b"BT /F1 16 Tf 72 760 Td (Arial Bold mit festen Breiten) Tj ET BT /F2 12 Tf 72 700 Td (Calibri ohne Breiten) Tj ET", res, Dictionary::new());
    b.finish()
}

pub fn type3_font() -> Vec<u8> {
    let mut b = B::new();
    let sq = b.doc.add_object(Stream::new(Dictionary::new(), b"1000 0 0 0 750 750 d1 0 0 750 750 re f".to_vec()));
    let tri = b.doc.add_object(Stream::new(Dictionary::new(), b"1000 0 0 0 750 750 d1 0 0 m 750 0 l 375 750 l f".to_vec()));
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 750.into(), 750.into()],
        "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
        "CharProcs" => dictionary! { "square" => sq, "triangle" => tri },
        "Encoding" => dictionary! { "Type" => "Encoding", "Differences" => vec![65.into(), Object::Name(b"square".to_vec()), Object::Name(b"triangle".to_vec()), Object::Name(b"missing".to_vec())] },
        "FirstChar" => 65, "LastChar" => 67,
        "Widths" => vec![1000.into(), 1000.into(), 1000.into()],
        "Resources" => Dictionary::new(),
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABCABC) Tj ET", font_res(f), Dictionary::new());
    b.finish()
}

pub fn cid_unembedded() -> Vec<u8> {
    let mut b = B::new();
    let tu = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /X def\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n3 beginbfchar <0001> <0048> <0002> <0069> <0003> <4E2D> endbfchar\nendcmap end end";
    let tus = b.doc.add_object(Stream::new(Dictionary::new(), tu.to_vec()));
    let fd = b.doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "MSGothic", "Flags" => 4,
        "FontBBox" => vec![0.into(), (-140).into(), 1000.into(), 860.into()],
        "ItalicAngle" => 0, "Ascent" => 860, "Descent" => (-140), "CapHeight" => 700, "StemV" => 80,
    });
    let cf = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "MSGothic",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => fd, "DW" => 1000,
    });
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "MSGothic", "Encoding" => "Identity-H",
        "DescendantFonts" => vec![cf.into()], "ToUnicode" => tus,
    });
    b.page(b"BT /F1 20 Tf 72 700 Td <000100020003> Tj ET", font_res(f), Dictionary::new());
    b.finish()
}

/// A simple TrueType font embedded from Roboto, with deliberately wrong
/// widths and one code that maps to no glyph.
pub fn embedded_bad_widths() -> Vec<u8> {
    let mut b = B::new();
    let data = pdf_fonts::family("Roboto").unwrap().face(false, false).to_vec();
    let mut ff = Stream::new(dictionary! { "Length1" => data.len() as i64 }, data);
    let _ = ff.compress();
    let ffid = b.doc.add_object(ff);
    let fd = b.doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "Roboto-Regular", "Flags" => 32,
        "FontBBox" => vec![(-700).into(), (-300).into(), 2000.into(), 1000.into()],
        "ItalicAngle" => 0, "Ascent" => 928, "Descent" => (-244), "CapHeight" => 711, "StemV" => 80,
        "FontFile2" => ffid,
    });
    let widths: Vec<Object> = (32..=126).map(|_| Object::Integer(500)).collect();
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Roboto-Regular",
        "FirstChar" => 32, "LastChar" => 126, "Widths" => widths,
        "Encoding" => "WinAnsiEncoding", "FontDescriptor" => fd,
    });
    b.page(b"BT /F1 16 Tf 72 760 Td (Breiten passen nicht zum Font) Tj ET BT /F1 16 Tf 72 700 Td <01> Tj ET", font_res(f), Dictionary::new());
    b.finish()
}

fn rgba_png(w: u32, h: u32) -> Vec<u8> {
    let mut img = image::RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 128, if (x + y) % 2 == 0 { 255 } else { 120 }]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

pub fn transparency() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let gs = b.doc.add_object(dictionary! { "Type" => "ExtGState", "ca" => 0.5, "CA" => 0.5, "BM" => "Multiply" });
    // Image with soft mask.
    let mask = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        vec![255, 128, 64, 0],
    ));
    let img = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "SMask" => mask },
        vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
    ));
    let res = dictionary! {
        "Font" => dictionary! { "F1" => f },
        "ExtGState" => dictionary! { "G1" => gs },
        "XObject" => dictionary! { "Im1" => img },
    };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"q /G1 gs 1 0 0 rg 72 500 300 150 re f Q q 200 0 0 200 300 200 cm /Im1 Do Q\n");
    b.page(&c, res, dictionary! { "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceRGB" } });
    // A second, opaque page that must stay vector in PDF/A-1.
    let f2 = helv(&mut b);
    b.page(TEXT, font_res(f2), Dictionary::new());
    b.finish()
}

pub fn device_colors() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let cmyk_img = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceCMYK", "BitsPerComponent" => 8 },
        vec![255, 0, 0, 0, 0, 0, 0, 255],
    ));
    let sep = b.doc.add_object(Object::Array(vec![
        Object::Name(b"Separation".to_vec()),
        Object::Name(b"HKS 43".to_vec()),
        Object::Name(b"DeviceCMYK".to_vec()),
        Object::Dictionary(dictionary! { "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "C0" => vec![0.into(), 0.into(), 0.into(), 0.into()], "C1" => vec![1.into(), 0.5.into(), 0.into(), 0.into()], "N" => 1 }),
    ]));
    let form = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()] },
        b"0 0 0 1 k 0 0 100 100 re f".to_vec(),
    ));
    let res = dictionary! {
        "Font" => dictionary! { "F1" => f },
        "XObject" => dictionary! { "Im1" => cmyk_img, "Fm1" => form },
        "ColorSpace" => dictionary! { "CS1" => sep },
    };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"0 1 1 0 k 72 600 100 100 re f 0.5 g 200 600 100 100 re f 0 0 1 rg 330 600 100 100 re f /CS1 cs 0.7 scn 72 450 100 100 re f q 200 0 0 100 72 300 cm /Im1 Do Q q 1 0 0 1 300 300 cm /Fm1 Do Q 0 1 0 0 K 4 w 72 250 m 500 250 l S\n");
    b.page(&c, res, Dictionary::new());
    b.finish()
}

pub fn jpx_image() -> Vec<u8> {
    let jp2 = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jp2")).unwrap();
    let mut b = B::new();
    let f = helv(&mut b);
    let mut s = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 16, "Height" => 16, "BitsPerComponent" => 8, "Filter" => "JPXDecode" },
        jp2,
    );
    s.allows_compression = false;
    let img = b.doc.add_object(s);
    let res = dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Im1" => img } };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"q 200 0 0 200 72 400 cm /Im1 Do Q\n");
    b.page(&c, res, Dictionary::new());
    b.finish()
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> pdf_types::RectPt {
    pdf_types::RectPt { x0, y0, x1, y1 }
}

/// Annotations as the app creates them (pdf_mutate).
pub fn app_annotations() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    b.page(TEXT, font_res(f), Dictionary::new());
    let bytes = b.finish();
    let mut doc = pdf_mutate::MutableDoc::from_bytes(&bytes).unwrap();
    use pdf_mutate::*;
    doc.add_markup_annotation(0, &MarkupAnnotation { kind: MarkupKind::Highlight, rects: vec![rect(72.0, 60.0, 300.0, 85.0)], color: [1.0, 1.0, 0.0], opacity: 0.5, contents: Some("Wichtig".into()), author: Some("Test".into()), print: true }).unwrap();
    doc.add_markup_annotation(0, &MarkupAnnotation { kind: MarkupKind::Underline, rects: vec![rect(72.0, 60.0, 300.0, 85.0)], color: [1.0, 0.0, 0.0], opacity: 1.0, contents: None, author: None, print: false }).unwrap();
    for (i, kind) in [ShapeKind::Square, ShapeKind::Circle, ShapeKind::Line { arrow_start: LineEnding::None, arrow_end: LineEnding::ClosedArrow }, ShapeKind::Polygon { cloudy: true }, ShapeKind::PolyLine { start: LineEnding::OpenArrow, end: LineEnding::None }].into_iter().enumerate() {
        let y = 150.0 + i as f32 * 80.0;
        doc.add_shape_annotation(0, &ShapeAnnotation {
            kind,
            rect: rect(72.0, y, 200.0, y + 60.0),
            vertices: vec![(250.0, y), (350.0, y + 10.0), (300.0, y + 60.0)],
            subject: Some("Form".into()),
            stroke_color: [0.0, 0.0, 1.0],
            fill_color: if i == 0 { Some([0.8, 0.9, 1.0]) } else { None },
            border_width: 2.0,
            opacity: if i == 1 { 0.6 } else { 1.0 },
            contents: None,
            author: None,
            print: true,
        }).unwrap();
    }
    doc.add_freetext_annotation(0, &FreeTextAnnotation { rect: rect(350.0, 150.0, 550.0, 220.0), text: "Freitext mit Umlauten: äöü ß €".into(), color: [0.0, 0.0, 0.0], bg_color: [1.0, 1.0, 0.8, 1.0], font_size: 12.0, font_family: "Helv".into(), bold: false, italic: false, rotation_quadrant: 0, opacity: 1.0, author: None, print: true }).unwrap();
    doc.add_freetext_annotation(0, &FreeTextAnnotation { rect: rect(350.0, 250.0, 550.0, 300.0), text: "Times kursiv".into(), color: [0.2, 0.2, 0.6], bg_color: [0.0, 0.0, 0.0, 0.0], font_size: 14.0, font_family: "Times".into(), bold: true, italic: true, rotation_quadrant: 1, opacity: 0.8, author: None, print: true }).unwrap();
    doc.add_ink_annotation(0, &InkAnnotation { strokes: vec![vec![(400.0, 400.0), (420.0, 430.0), (450.0, 410.0)]], color: [0.0, 0.5, 0.0], width: 2.0, opacity: 1.0, contents: None, author: None, print: true }).unwrap();
    doc.add_stamp_annotation(0, &StampAnnotation { rect: rect(350.0, 500.0, 500.0, 600.0), image: rgba_png(32, 16), rotation_quadrant: 0, opacity: 1.0, contents: Some("Stempel".into()), author: None, print: true }).unwrap();
    doc.add_text_note_annotation(0, &TextNoteAnnotation { x_pt: 520.0, y_pt: 60.0, contents: "Notiz".into(), color: [1.0, 0.8, 0.0], icon: Some("Comment".into()), author: Some("Test".into()), print: true }).unwrap();
    doc.add_text_watermark("ENTWURF", &WatermarkOptions::default()).unwrap();
    doc.to_bytes().unwrap()
}

/// Annotations as other producers write them: no appearance, hidden,
/// pop-ups, links with actions, forbidden types.
pub fn raw_annotations() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let page_id = b.doc.new_object_id();
    let mut annots = Vec::new();
    let mut add = |doc: &mut Document, d: Dictionary| -> ObjectId {
        let id = doc.add_object(d);
        annots.push(Object::Reference(id));
        id
    };
    let r = |a: i64, b2: i64, c: i64, d: i64| -> Object { vec![a.into(), b2.into(), c.into(), d.into()].into() };
    let sq = add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => r(72, 600, 200, 700), "C" => vec![1.into(), 0.into(), 0.into()], "IC" => vec![0.9.into()] });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Circle", "Rect" => r(220, 600, 320, 700), "C" => vec![0.into(), 0.into(), 1.into()], "F" => 0 });
    let note = add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Text", "Rect" => r(400, 700, 420, 720), "Contents" => Object::string_literal("Kommentar"), "C" => vec![1.into(), 1.into(), 0.into()] });
    let popup = add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Popup", "Rect" => r(420, 600, 560, 700), "Parent" => note, "F" => 2 });
    if let Ok(Object::Dictionary(d)) = b.doc.get_object_mut(note) {
        d.set("Popup", popup);
    }
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Highlight", "Rect" => r(72, 750, 300, 780), "QuadPoints" => vec![72.into(), 780.into(), 300.into(), 780.into(), 72.into(), 750.into(), 300.into(), 750.into()], "C" => vec![1.into(), 1.into(), 0.into()], "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "StrikeOut", "Rect" => r(72, 750, 300, 780), "QuadPoints" => vec![72.into(), 780.into(), 300.into(), 780.into(), 72.into(), 750.into(), 300.into(), 750.into()], "C" => vec![1.into(), 0.into(), 0.into()], "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Ink", "Rect" => r(72, 400, 200, 500), "InkList" => vec![Object::Array(vec![80.into(), 410.into(), 150.into(), 480.into(), 190.into(), 420.into()])], "C" => vec![0.into(), 0.into(), 0.into()], "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Line", "Rect" => r(72, 300, 300, 350), "L" => vec![72.into(), 300.into(), 300.into(), 350.into()], "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Polygon", "Rect" => r(320, 300, 450, 400), "Vertices" => vec![320.into(), 300.into(), 450.into(), 300.into(), 385.into(), 400.into()], "C" => vec![0.into(), 0.5.into(), 0.into()], "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "FreeText", "Rect" => r(320, 450, 500, 500), "Contents" => Object::string_literal("ohne AP"), "DA" => Object::string_literal("/Helv 12 Tf 0 g"), "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Text", "Rect" => r(500, 500, 520, 520), "Contents" => Object::string_literal("versteckt"), "F" => 2 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Movie", "Rect" => r(72, 100, 200, 200), "F" => 4 });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => r(72, 50, 200, 70), "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://example.com") }, "Border" => vec![0.into(), 0.into(), 0.into()] });
    add(&mut b.doc, dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => r(220, 50, 300, 70), "A" => dictionary! { "S" => "Launch", "F" => Object::string_literal("calc.exe") } });
    add(&mut b.doc, dictionary! { "Subtype" => "Square", "Rect" => r(0, 0, 0, 0) });
    let _ = sq;
    let c = b.doc.add_object(Stream::new(Dictionary::new(), TEXT.to_vec()));
    b.doc.objects.insert(page_id, Object::Dictionary(dictionary! {
        "Type" => "Page", "Parent" => b.pages, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => c, "Resources" => font_res(f), "Annots" => annots,
    }));
    b.kids.push(page_id.into());
    b.finish()
}

pub fn forms() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let zadb = b.doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "ZapfDingbats" });
    let page_id = b.doc.new_object_id();
    let tx_ap = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 20.into()], "Resources" => dictionary! { "Font" => dictionary! { "Helv" => f } } },
        b"/Tx BMC BT /Helv 12 Tf 0 g 2 5 Td (Max Mustermann) Tj ET EMC".to_vec(),
    ));
    let tx = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("name"),
        "V" => Object::string_literal("Max Mustermann"), "Rect" => vec![72.into(), 700.into(), 272.into(), 720.into()],
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"), "P" => page_id, "F" => 4,
        "AP" => dictionary! { "N" => tx_ap, "D" => tx_ap },
        "AA" => dictionary! { "K" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("AFNumber_Keystroke(2)") } },
    });
    let tx2 = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("ort"),
        "V" => Object::string_literal("Köln"), "Rect" => vec![72.into(), 650.into(), 272.into(), 670.into()],
        "DA" => Object::string_literal("/Helv 10 Tf 0 0 1 rg"), "P" => page_id, "F" => 4, "Q" => 1,
    });
    let on = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 15.into(), 15.into()], "Resources" => dictionary! { "Font" => dictionary! { "ZaDb" => zadb } } },
        b"q BT /ZaDb 12 Tf 0 g 2 3 Td (4) Tj ET Q".to_vec(),
    ));
    let off = b.doc.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 15.into(), 15.into()] }, Vec::new()));
    let cb = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("ok"),
        "V" => "Yes", "AS" => "Yes", "Rect" => vec![300.into(), 700.into(), 315.into(), 715.into()], "P" => page_id, "F" => 4,
        "AP" => dictionary! { "N" => dictionary! { "Yes" => on, "Off" => off }, "D" => dictionary! { "Yes" => on, "Off" => off } },
        "A" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") },
    });
    let cb2 = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("neu"),
        "V" => "Off", "AS" => "Off", "Rect" => vec![300.into(), 650.into(), 315.into(), 665.into()], "P" => page_id, "F" => 4,
    });
    let c = b.doc.add_object(Stream::new(Dictionary::new(), TEXT.to_vec()));
    b.doc.objects.insert(page_id, Object::Dictionary(dictionary! {
        "Type" => "Page", "Parent" => b.pages, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => c, "Resources" => font_res(f), "Annots" => vec![tx.into(), tx2.into(), cb.into(), cb2.into()],
    }));
    b.kids.push(page_id.into());
    let acro = dictionary! {
        "Fields" => vec![tx.into(), tx2.into(), cb.into(), cb2.into()],
        "NeedAppearances" => true,
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => f, "ZaDb" => zadb } },
        "XFA" => Object::string_literal("<xdp/>"),
    };
    b.finish_with(dictionary! { "AcroForm" => acro })
}

pub fn simple_pdf(text: &str) -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    b.page(format!("BT /F1 12 Tf 72 760 Td ({text}) Tj ET").as_bytes(), font_res(f), Dictionary::new());
    b.finish()
}

pub fn attachments() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let txt = b.doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"Hallo Anhang".to_vec()));
    let txt_fs = b.doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("notiz.txt"), "EF" => dictionary! { "F" => txt } });
    let pdf = b.doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile", "Subtype" => Object::Name(b"application/pdf".to_vec()) }, simple_pdf("Eingebettetes PDF")));
    let pdf_fs = b.doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("rechnung.pdf"), "UF" => Object::string_literal("rechnung.pdf"), "EF" => dictionary! { "F" => pdf } });
    let xml = b.doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"<invoice/>".to_vec()));
    let annot = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "FileAttachment", "Rect" => vec![500.into(), 700.into(), 520.into(), 720.into()],
        "FS" => dictionary! { "Type" => "Filespec", "F" => Object::string_literal("daten.xml"), "EF" => dictionary! { "F" => xml } },
        "Contents" => Object::string_literal("XML"), "F" => 4,
    });
    let names = b.doc.add_object(dictionary! { "Names" => vec![Object::string_literal("notiz.txt"), txt_fs.into(), Object::string_literal("rechnung.pdf"), pdf_fs.into()] });
    b.page(TEXT, font_res(f), dictionary! { "Annots" => vec![annot.into()] });
    b.finish_with(dictionary! { "Names" => dictionary! { "EmbeddedFiles" => names } })
}

pub fn encrypted(user: &str) -> Vec<u8> {
    let plain = simple_pdf("Verschluesselt");
    pdf_mutate::encrypt_pdf(&plain, user, "owner-secret", pdf_mutate::PermissionOptions::default()).unwrap()
}

pub fn javascript_actions() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let js = b.doc.add_object(dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert('x')") });
    let js_tree = b.doc.add_object(dictionary! { "Names" => vec![Object::string_literal("init"), js.into()] });
    let link = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![72.into(), 700.into(), 200.into(), 720.into()],
        "A" => dictionary! { "S" => "Named", "N" => "Print" }, "F" => 4,
    });
    let goto = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![72.into(), 650.into(), 200.into(), 670.into()],
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://example.com"), "Next" => js }, "F" => 4,
    });
    b.page(TEXT, font_res(f), dictionary! {
        "Annots" => vec![link.into(), goto.into()],
        "AA" => dictionary! { "O" => js },
    });
    b.finish_with(dictionary! {
        "OpenAction" => js,
        "AA" => dictionary! { "WC" => js },
        "Names" => dictionary! { "JavaScript" => js_tree },
    })
}

pub fn rotated_pages() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    for r in [0, 90, 180, 270] {
        b.page(format!("BT /F1 24 Tf 72 700 Td (Seite mit Rotate {r}) Tj ET 0 0 1 RG 4 w 50 50 495 742 re S").as_bytes(), font_res(f), dictionary! { "Rotate" => r });
    }
    b.finish()
}

pub fn ocr_layer() -> Vec<u8> {
    let png = {
        let mut img = image::RgbImage::new(200, 280);
        for p in img.pixels_mut() {
            *p = image::Rgb([250, 250, 245]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let bytes = pdf_mutate::images_to_pdf(&[png]).unwrap();
    let mut doc = pdf_mutate::MutableDoc::from_bytes(&bytes).unwrap();
    let words = vec![
        pdf_mutate::OcrWord { text: "Gescanntes".into(), x: 20.0, y: 30.0, w: 80.0, h: 12.0 },
        pdf_mutate::OcrWord { text: "Dokument".into(), x: 105.0, y: 30.0, w: 70.0, h: 12.0 },
        pdf_mutate::OcrWord { text: "Größe".into(), x: 20.0, y: 60.0, w: 50.0, h: 12.0 },
    ];
    doc.add_ocr_text_layer(0, &words, 200, 280).unwrap();
    doc.to_bytes().unwrap()
}

pub fn optional_content() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let on = b.doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Sichtbar") });
    let off = b.doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Ausgeblendet") });
    let hidden_form = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()], "OC" => off },
        b"1 0 0 rg 0 0 100 100 re f".to_vec(),
    ));
    let res = dictionary! {
        "Font" => dictionary! { "F1" => f },
        "XObject" => dictionary! { "Hid" => hidden_form },
        "Properties" => dictionary! { "MC0" => on, "MC1" => off },
    };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"/OC /MC0 BDC 0 0 1 rg 72 500 100 100 re f EMC q 1 0 0 1 300 500 cm /Hid Do Q\n");
    b.page(&c, res, Dictionary::new());
    let ocp = dictionary! {
        "OCGs" => vec![on.into(), off.into()],
        "D" => dictionary! { "Order" => vec![on.into()], "OFF" => vec![off.into()], "AS" => vec![Object::Dictionary(dictionary! { "Event" => "View", "OCGs" => vec![on.into()], "Category" => vec![Object::Name(b"View".to_vec())] })] },
        "Configs" => vec![Object::Dictionary(dictionary! { "Name" => Object::string_literal("Alles"), "ON" => vec![on.into(), off.into()] }), Object::Dictionary(Dictionary::new())],
    };
    b.finish_with(dictionary! { "OCProperties" => ocp })
}

/// Old/odd constructs: LZW content, interpolated 16-bit image, transfer
/// functions, PostScript XObject, undefined operators, custom XMP, Info
/// with dates and non-standard keys.
pub fn legacy_misc() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let gs = b.doc.add_object(dictionary! { "Type" => "ExtGState", "TR" => "Identity", "RI" => "Bogus", "HT" => dictionary! { "HalftoneType" => 6, "Width" => 1, "Height" => 1 } });
    let img16 = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 16, "Interpolate" => true, "Intent" => "Weird" },
        vec![0xFF, 0xFF, 0x00, 0x00],
    ));
    let ps = b.doc.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "PS" }, b"showpage".to_vec()));
    let content = {
        let mut c = TEXT.to_vec();
        c.extend_from_slice(b"/G1 gs BX 1 2 foo EX q 100 0 0 50 72 400 cm /Im1 Do Q /PS1 Do 1 0 0 1 0 0 cm\n");
        c
    };
    let lzw = lzw_encode(&content);
    let cid = b.doc.add_object(Stream::new(dictionary! { "Filter" => "LZWDecode" }, lzw));
    let xmp = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<?xpacket begin='' id='W5M0MpCehiHzreSzNTczkc9d' bytes='300'?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description rdf:about='' xmlns:foo='http://example.com/foo/'><foo:bar>baz</foo:bar></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>".to_vec(),
    ));
    let page_id = b.doc.new_object_id();
    b.doc.objects.insert(page_id, Object::Dictionary(dictionary! {
        "Type" => "Page", "Parent" => b.pages, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => cid,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => f }, "ExtGState" => dictionary! { "G1" => gs }, "XObject" => dictionary! { "Im1" => img16, "PS1" => ps } },
        "PresSteps" => dictionary! {}, "Metadata" => xmp,
    }));
    b.kids.push(page_id.into());
    let mut out = b.finish_with(dictionary! { "Metadata" => xmp, "NeedsRendering" => false, "Version" => "1.4" });
    // Non-standard Info key and a Unicode title.
    let mut doc = Document::load_mem(&out).unwrap();
    let info = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(info) {
        d.set("Title", Object::String(vec![0xFE, 0xFF, 0x00, 0x54, 0x00, 0xFC, 0x00, 0x74], StringFormat::Hexadecimal));
        d.set("Firma", Object::string_literal("Softengine"));
        d.set("ModDate", Object::string_literal("nonsense"));
        d.set("Trapped", "False");
    }
    out.clear();
    doc.save_to(&mut out).unwrap();
    out
}

/// Already tagged input with a custom structure type in the role map.
pub fn tagged_input() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let page = b.page(
        b"/Heading <</MCID 0>> BDC BT /F1 20 Tf 72 760 Td (Titel) Tj ET EMC /P <</MCID 1>> BDC BT /F1 12 Tf 72 720 Td (Absatz) Tj ET EMC",
        font_res(f),
        dictionary! { "StructParents" => 0 },
    );
    let root = b.doc.new_object_id();
    let docel = b.doc.new_object_id();
    let h = b.doc.add_object(dictionary! { "Type" => "StructElem", "S" => "Heading", "P" => docel, "Pg" => page, "K" => 0 });
    let p = b.doc.add_object(dictionary! { "Type" => "StructElem", "S" => "P", "P" => docel, "Pg" => page, "K" => 1 });
    b.doc.objects.insert(docel, Object::Dictionary(dictionary! { "Type" => "StructElem", "S" => "Document", "P" => root, "K" => vec![h.into(), p.into()] }));
    let pt = b.doc.add_object(dictionary! { "Nums" => vec![0.into(), Object::Array(vec![h.into(), p.into()])] });
    b.doc.objects.insert(root, Object::Dictionary(dictionary! {
        "Type" => "StructTreeRoot", "K" => docel, "ParentTree" => pt, "ParentTreeNextKey" => 1,
        "RoleMap" => dictionary! { "Heading" => "Title", "Title" => "Heading", "P" => "Para" },
    }));
    b.finish_with(dictionary! {
        "StructTreeRoot" => root,
        "MarkInfo" => dictionary! { "Marked" => true },
        "Lang" => Object::string_literal("de_DE"),
    })
}

// ---------------------------------------------------------------- synthetic font programs

/// Type 2 charstring number.
fn cs_num(v: i32) -> Vec<u8> {
    if (-107..=107).contains(&v) {
        vec![(v + 139) as u8]
    } else if (108..=1131).contains(&v) {
        let x = v - 108;
        vec![(x / 256 + 247) as u8, (x % 256) as u8]
    } else if (-1131..=-108).contains(&v) {
        let x = -v - 108;
        vec![(x / 256 + 251) as u8, (x % 256) as u8]
    } else {
        vec![28, (v >> 8) as u8, v as u8]
    }
}

fn cff_index(items: &[Vec<u8>]) -> Vec<u8> {
    if items.is_empty() {
        return vec![0, 0];
    }
    let mut out = (items.len() as u16).to_be_bytes().to_vec();
    out.push(2);
    let mut off = 1u16;
    out.extend_from_slice(&off.to_be_bytes());
    for i in items {
        off += i.len() as u16;
        out.extend_from_slice(&off.to_be_bytes());
    }
    for i in items {
        out.extend_from_slice(i);
    }
    out
}

/// A complete, minimal name-keyed CFF font (Standard Encoding) with a box
/// glyph for each of `glyphs` (ASCII letters) and the given advance widths.
pub fn minimal_cff(name: &str, glyphs: &[(char, i32)]) -> Vec<u8> {
    let dict_int = |v: i32| -> Vec<u8> {
        let mut o = vec![29u8];
        o.extend_from_slice(&v.to_be_bytes());
        o
    };
    let header = vec![1u8, 0, 4, 1];
    let name_idx = cff_index(&[name.as_bytes().to_vec()]);
    // Charstrings: .notdef, then one box per glyph.
    let mut css: Vec<Vec<u8>> = Vec::new();
    let mut notdef = cs_num(500);
    notdef.push(14);
    css.push(notdef);
    for (_, w) in glyphs {
        let mut c = Vec::new();
        for v in [*w, 100, 0] {
            c.extend(cs_num(v));
        }
        c.push(21); // rmoveto
        for (x, y) in [(300, 0), (0, 600), (-300, 0)] {
            c.extend(cs_num(x));
            c.extend(cs_num(y));
            c.push(5); // rlineto
        }
        c.push(14); // endchar
        css.push(c);
    }
    let charstrings = cff_index(&css);
    let mut charset = vec![0u8];
    for (ch, _) in glyphs {
        charset.extend_from_slice(&(*ch as u16 - 31).to_be_bytes());
    }
    // Private: defaultWidthX 0, nominalWidthX 0.
    let mut private = dict_int(0);
    private.push(20);
    private.extend(dict_int(0));
    private.push(21);
    // Top DICT has a fixed size (5-byte operands), so offsets are known.
    let top_len = 4 * 5 + 1 + 6 + 6 + 11;
    let top_idx_len = 2 + 1 + 4 + top_len;
    let charset_off = header.len() + name_idx.len() + top_idx_len + 2 + 2;
    let cs_off = charset_off + charset.len();
    let priv_off = cs_off + charstrings.len();
    let mut top = Vec::new();
    for v in [0, 0, 1000, 700] {
        top.extend(dict_int(v));
    }
    top.push(5); // FontBBox
    top.extend(dict_int(charset_off as i32));
    top.push(15);
    top.extend(dict_int(cs_off as i32));
    top.push(17);
    top.extend(dict_int(private.len() as i32));
    top.extend(dict_int(priv_off as i32));
    top.push(18);
    assert_eq!(top.len(), top_len);
    let mut out = header;
    out.extend(name_idx);
    out.extend(cff_index(&[top]));
    out.extend_from_slice(&[0, 0]); // String INDEX
    out.extend_from_slice(&[0, 0]); // Global Subr INDEX
    out.extend(charset);
    out.extend(charstrings);
    out.extend(private);
    out
}

fn t1_encrypt(plain: &[u8], mut r: u16) -> Vec<u8> {
    let mut out = Vec::new();
    for &p in plain {
        let c = p ^ (r >> 8) as u8;
        r = (c as u16).wrapping_add(r).wrapping_mul(52845).wrapping_add(22719);
        out.push(c);
    }
    out
}

/// A minimal Type 1 font program; returns (data, Length1, Length2, Length3).
pub fn minimal_type1(name: &str, glyphs: &[(char, i32)]) -> (Vec<u8>, usize, usize, usize) {
    let mut clear = format!(
        "%!PS-AdobeFont-1.0: {name} 001.000\n11 dict begin\n/FontInfo 1 dict dup begin /FullName ({name}) readonly def end readonly def\n/FontName /{name} def\n/PaintType 0 def\n/FontType 1 def\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n/FontBBox {{0 0 1000 700}} readonly def\n/Encoding 256 array\n0 1 255 {{1 index exch /.notdef put}} for\n"
    )
    .into_bytes();
    for (c, _) in glyphs {
        clear.extend_from_slice(format!("dup {} /{} put\n", *c as u32, c).as_bytes());
    }
    clear.extend_from_slice(b"readonly def\ncurrentdict end\ncurrentfile eexec\n");
    let mut private = b"\0\0\0\0dup /Private 8 dict dup begin\n/RD {string currentfile exch readstring pop} executeonly def\n/ND {noaccess def} executeonly def\n/NP {noaccess put} executeonly def\n/BlueValues [] def\n/MinFeature {16 16} def\n/password 5839 def\n/lenIV 4 def\n/Subrs 0 array\nND\n".to_vec();
    private.extend_from_slice(format!("2 index /CharStrings {} dict dup begin\n", glyphs.len() + 1).as_bytes());
    let mut all: Vec<(String, i32)> = vec![(".notdef".into(), 500)];
    all.extend(glyphs.iter().map(|(c, w)| (c.to_string(), *w)));
    for (gname, w) in all {
        // 0 w hsbw; box; endchar
        let mut cs = vec![0u8; 4];
        cs.push(139);
        cs.push(255);
        cs.extend_from_slice(&w.to_be_bytes());
        cs.push(13);
        if gname != ".notdef" {
            cs.extend_from_slice(&[239, 139, 21]); // 100 0 rmoveto
            cs.extend_from_slice(&[247, 192, 6]); // 300 hlineto
            cs.extend_from_slice(&[248, 236, 7]); // 600 vlineto
            cs.extend_from_slice(&[251, 192, 6]); // -300 hlineto
            cs.push(9); // closepath
        }
        cs.push(14);
        let enc = t1_encrypt(&cs, 4330);
        private.extend_from_slice(format!("/{gname} {} RD ", enc.len()).as_bytes());
        private.extend_from_slice(&enc);
        private.extend_from_slice(b" ND\n");
    }
    private.extend_from_slice(b"end\nend\nreadonly put\nnoaccess put\ndup /FontName get exch definefont pop\nmark currentfile closefile\n");
    let enc = t1_encrypt(&private, 55665);
    let (l1, l2) = (clear.len(), enc.len());
    let mut out = clear;
    out.extend_from_slice(&enc);
    let mut trailer = Vec::new();
    for _ in 0..8 {
        trailer.extend_from_slice(&[b'0'; 64]);
        trailer.push(b'\n');
    }
    trailer.extend_from_slice(b"cleartomark\n");
    let l3 = trailer.len();
    out.extend_from_slice(&trailer);
    (out, l1, l2, l3)
}

fn simple_descriptor(b: &mut B, name: &str, key: &str, file: ObjectId) -> ObjectId {
    let mut fd = dictionary! {
        "Type" => "FontDescriptor", "FontName" => Object::Name(name.as_bytes().to_vec()), "Flags" => 32,
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 700.into()],
        "ItalicAngle" => 0, "Ascent" => 700, "Descent" => 0, "CapHeight" => 600, "StemV" => 80,
    };
    fd.set(key, file);
    b.doc.add_object(fd)
}

/// Embedded CFF and Type 1 fonts: complete programs whose /Widths are
/// wrong (stay embedded), a subset with a missing glyph and no system
/// replacement (page is rendered), and a subset of a common font with a
/// missing glyph that is installed on the system (replaced from there).
pub fn cff_type1_missing_glyphs() -> Vec<u8> {
    let mut b = B::new();
    let w3 = |w: i64| -> Vec<Object> { vec![w.into(), w.into(), w.into()] };
    // Page 1: complete CFF, /Widths disagree with the program.
    let cff = minimal_cff("DocuraCffOk", &[('A', 600), ('B', 700), ('C', 650)]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Subtype" => "Type1C" }, cff));
    let fd = simple_descriptor(&mut b, "DocuraCffOk", "FontFile3", ff);
    let f1 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "DocuraCffOk", "FirstChar" => 65, "LastChar" => 67,
        "Widths" => w3(500), "Encoding" => "WinAnsiEncoding", "FontDescriptor" => fd,
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABCCBA) Tj ET", font_res(f1), Dictionary::new());
    // Page 2: CFF subset without the glyph for "C", unknown font name.
    let cff = minimal_cff("AAAAAA+DocuraCffSubset", &[('A', 600), ('B', 700)]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Subtype" => "Type1C" }, cff));
    let fd = simple_descriptor(&mut b, "AAAAAA+DocuraCffSubset", "FontFile3", ff);
    let f2 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "AAAAAA+DocuraCffSubset", "FirstChar" => 65, "LastChar" => 67,
        "Widths" => vec![600.into(), 700.into(), 650.into()], "Encoding" => "WinAnsiEncoding", "FontDescriptor" => fd,
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABC) Tj ET", font_res(f2), Dictionary::new());
    // Page 3: Type 1 subset without the glyph for "C" whose font exists on
    // most systems (replaced from there; rendered where it does not).
    let (t1, l1, l2, l3) = minimal_type1("BBBBBB+ArialMT", &[('A', 600), ('B', 600)]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Length1" => l1 as i64, "Length2" => l2 as i64, "Length3" => l3 as i64 }, t1));
    let fd = simple_descriptor(&mut b, "BBBBBB+ArialMT", "FontFile", ff);
    let f3 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "BBBBBB+ArialMT", "FirstChar" => 65, "LastChar" => 67,
        "Widths" => w3(600), "Encoding" => "WinAnsiEncoding", "FontDescriptor" => fd,
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABC) Tj ET", font_res(f3), Dictionary::new());
    // Page 4: complete Type 1 with wrong widths (stays embedded).
    let (t1, l1, l2, l3) = minimal_type1("DocuraType1Ok", &[('A', 600), ('B', 700)]);
    let ff = b.doc.add_object(Stream::new(dictionary! { "Length1" => l1 as i64, "Length2" => l2 as i64, "Length3" => l3 as i64 }, t1));
    let fd = simple_descriptor(&mut b, "DocuraType1Ok", "FontFile", ff);
    let f4 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "DocuraType1Ok", "FirstChar" => 65, "LastChar" => 66,
        "Widths" => vec![500.into(), 500.into()], "FontDescriptor" => fd,
    });
    b.page(b"BT /F1 24 Tf 72 700 Td (ABBA) Tj ET", font_res(f4), Dictionary::new());
    b.finish()
}

/// Vertical writing (WMode 1): an unembedded CJK font with Identity-V and
/// an embedded font used vertically.
pub fn vertical_cjk() -> Vec<u8> {
    let mut b = B::new();
    // Page 1: not embedded, vertical.
    let tu = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /X def\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n3 beginbfchar <0001> <65E5> <0002> <672C> <0003> <8A9E> endbfchar\nendcmap end end";
    let tus = b.doc.add_object(Stream::new(Dictionary::new(), tu.to_vec()));
    let fd = b.doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "MS-Mincho", "Flags" => 6,
        "FontBBox" => vec![0.into(), (-140).into(), 1000.into(), 860.into()],
        "ItalicAngle" => 0, "Ascent" => 860, "Descent" => (-140), "CapHeight" => 700, "StemV" => 80,
    });
    let cf = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "MS-Mincho",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Japan1"), "Supplement" => 2 },
        "FontDescriptor" => fd, "DW" => 1000, "DW2" => vec![880.into(), (-1000).into()],
    });
    let f = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "MS-Mincho", "Encoding" => "Identity-V",
        "DescendantFonts" => vec![cf.into()], "ToUnicode" => tus,
    });
    b.page(b"BT /F1 24 Tf 300 760 Td <000100020003> Tj ET", font_res(f), Dictionary::new());
    // Page 2: embedded TrueType font, vertical.
    let data = pdf_fonts::family("Roboto").unwrap().face(false, false).to_vec();
    let face = ttf_parser::Face::parse(&data, 0).unwrap();
    let upem = face.units_per_em() as f64;
    let mut codes = Vec::new();
    let mut w = Vec::new();
    for ch in "Vertikal".chars() {
        let g = face.glyph_index(ch).unwrap();
        codes.extend_from_slice(&g.0.to_be_bytes());
        let adv = (face.glyph_hor_advance(g).unwrap() as f64 * 1000.0 / upem).round() as i64;
        w.push(Object::Integer(g.0 as i64));
        w.push(Object::Array(vec![adv.into()]));
    }
    let mut ff = Stream::new(dictionary! { "Length1" => data.len() as i64 }, data.clone());
    let _ = ff.compress();
    let ffid = b.doc.add_object(ff);
    let fd = simple_descriptor(&mut b, "Roboto-Regular", "FontFile2", ffid);
    let cf = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "Roboto-Regular",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => fd, "W" => w, "DW2" => vec![880.into(), (-1000).into()], "CIDToGIDMap" => "Identity",
    });
    let f2 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Roboto-Regular", "Encoding" => "Identity-V",
        "DescendantFonts" => vec![cf.into()],
    });
    let mut c = b"BT /F1 24 Tf 300 760 Td <".to_vec();
    for x in &codes {
        c.extend_from_slice(format!("{x:02X}").as_bytes());
    }
    c.extend_from_slice(b"> Tj ET");
    b.page(&c, font_res(f2), Dictionary::new());
    b.finish()
}

fn ps_function(inputs: usize, code: &str) -> Stream {
    let mut domain = Vec::new();
    for _ in 0..inputs {
        domain.push(Object::Integer(0));
        domain.push(Object::Integer(1));
    }
    let range: Vec<Object> = (0..4).flat_map(|_| [Object::Integer(0), Object::Integer(1)]).collect();
    Stream::new(dictionary! { "FunctionType" => 4, "Domain" => domain, "Range" => range }, code.as_bytes().to_vec())
}

/// Spot colours: Separation and DeviceN with and without a Colorants
/// dictionary, the same spot name with different alternates, /All and
/// /None, and a DeviceN with more than eight components.
pub fn spot_colors() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let name = |s: &str| Object::Name(s.as_bytes().to_vec());
    let tint = |c1: [f32; 4]| -> Object {
        Object::Dictionary(dictionary! { "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "C0" => vec![0.into(), 0.into(), 0.into(), 0.into()], "C1" => c1.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(), "N" => 1 })
    };
    let sep_a = b.doc.add_object(Object::Array(vec![name("Separation"), name("HKS 43"), name("DeviceCMYK"), tint([1.0, 0.5, 0.0, 0.0])]));
    // Same colorant, other alternate space and tint transform.
    let sep_b = b.doc.add_object(Object::Array(vec![
        name("Separation"),
        name("HKS 43"),
        name("DeviceRGB"),
        Object::Dictionary(dictionary! { "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "C0" => vec![1.into(), 1.into(), 1.into()], "C1" => vec![0.into(), 0.3.into(), 0.8.into()], "N" => 1 }),
    ]));
    let sep_all = b.doc.add_object(Object::Array(vec![name("Separation"), name("All"), name("DeviceCMYK"), tint([1.0, 1.0, 1.0, 1.0])]));
    let sep_none = b.doc.add_object(Object::Array(vec![
        name("Separation"),
        name("None"),
        name("DeviceGray"),
        Object::Dictionary(dictionary! { "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "C0" => vec![1.into()], "C1" => vec![0.into()], "N" => 1 }),
    ]));
    // DeviceN without attributes.
    let fn2 = b.doc.add_object(ps_function(3, "{ pop 0 0 }"));
    let dn_plain = b.doc.add_object(Object::Array(vec![name("DeviceN"), Object::Array(vec![name("Cyan"), name("HKS 43"), name("None")]), name("DeviceCMYK"), fn2.into()]));
    // DeviceN (NChannel) with Colorants and Process.
    let fn2b = b.doc.add_object(ps_function(2, "{ exch 0 exch 0 exch 0 }"));
    let gold = Object::Array(vec![name("Separation"), name("Gold"), name("DeviceCMYK"), tint([0.0, 0.2, 0.8, 0.1])]);
    let dn_attr = b.doc.add_object(Object::Array(vec![
        name("DeviceN"),
        Object::Array(vec![name("Magenta"), name("Gold")]),
        name("DeviceCMYK"),
        fn2b.into(),
        Object::Dictionary(dictionary! {
            "Subtype" => "NChannel",
            "Colorants" => dictionary! { "Gold" => gold },
            "Process" => dictionary! { "ColorSpace" => "DeviceCMYK", "Components" => vec![name("Cyan"), name("Magenta"), name("Yellow"), name("Black")] },
        }),
    ]));
    // Nine components (PDF/A-1 allows eight).
    let fn9 = b.doc.add_object(ps_function(9, "{ pop pop pop pop pop }"));
    let nine: Vec<Object> = (1..=9).map(|i| name(&format!("Spot{i}"))).collect();
    let dn_nine = b.doc.add_object(Object::Array(vec![name("DeviceN"), Object::Array(nine), name("DeviceCMYK"), fn9.into()]));
    // Image in a Separation space.
    let img = b.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => sep_a, "BitsPerComponent" => 8 },
        vec![255, 64],
    ));
    let res = dictionary! {
        "Font" => dictionary! { "F1" => f },
        "XObject" => dictionary! { "Im1" => img },
        "ColorSpace" => dictionary! { "A" => sep_a, "B" => sep_b, "All" => sep_all, "None" => sep_none, "N2" => dn_plain, "N2a" => dn_attr, "N9" => dn_nine },
    };
    let mut c = TEXT.to_vec();
    c.extend_from_slice(b"/A cs 0.8 scn 72 600 80 80 re f /B cs 0.8 scn 172 600 80 80 re f /All cs 1 scn 272 600 80 80 re f /None cs 1 scn 372 600 80 80 re f\n");
    c.extend_from_slice(b"/N2 cs 0.5 0.9 0 scn 72 480 80 80 re f /N2a CS 0.3 0.7 SCN 4 w 172 480 80 80 re S /N9 cs 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 scn 272 480 80 80 re f\n");
    c.extend_from_slice(b"q 160 0 0 80 72 360 cm /Im1 Do Q\n");
    b.page(&c, res, Dictionary::new());
    b.finish()
}

/// Page boundaries outside what PDF/A allows (3 … 14400 units) and boxes
/// that do not fit their MediaBox.
pub fn page_boxes() -> Vec<u8> {
    let mut b = B::new();
    let f = helv(&mut b);
    let bx = |a: f32, c: f32, d: f32, e: f32| -> Object { vec![Object::Real(a), Object::Real(c), Object::Real(d), Object::Real(e)].into() };
    // Too large, with an annotation that has to move along.
    let link = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => bx(1000.0, 18000.0, 6000.0, 19000.0), "F" => 4,
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://example.com") }, "Border" => vec![0.into(), 0.into(), 0.into()],
    });
    b.page(
        b"BT /F1 600 Tf 1000 18000 Td (Riesenseite) Tj ET 0 0 1 RG 100 w 500 500 19000 19000 re S",
        font_res(f),
        dictionary! { "MediaBox" => bx(0.0, 0.0, 20000.0, 20000.0), "Annots" => vec![link.into()] },
    );
    // Too small.
    b.page(b"0 1 0 rg 0 0 2 2 re f", Dictionary::new(), dictionary! { "MediaBox" => bx(0.0, 0.0, 2.0, 2.0) });
    // Boxes larger than the MediaBox, reversed corners, UserUnit.
    b.page(
        TEXT,
        font_res(f),
        dictionary! {
            "CropBox" => bx(-100.0, -100.0, 700.0, 900.0), "TrimBox" => bx(-200.0, -200.0, 800.0, 1000.0),
            "ArtBox" => bx(500.0, 700.0, 100.0, 100.0), "BleedBox" => bx(0.0, 0.0, 30000.0, 30000.0), "UserUnit" => 2,
        },
    );
    // Negative origin and a CropBox flatter than three units.
    b.page(
        b"BT /F1 12 Tf -280 0 Td (Ursprung in der Mitte) Tj ET",
        font_res(f),
        dictionary! { "MediaBox" => bx(-300.0, -400.0, 295.0, 442.0), "CropBox" => bx(-300.0, 0.0, 295.0, 1.5) },
    );
    b.finish()
}

/// Plain (code-per-byte) LZW encoder (TIFF/PDF flavour, early change).
pub fn lzw_encode(data: &[u8]) -> Vec<u8> {
    let mut enc = weezl::encode::Encoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
    enc.encode(data).unwrap()
}

// ---------------------------------------------------------------- rasterizer

pub struct PdfiumRasterizer {
    engine: pdf_pdfium::PdfiumEngine,
}

impl PdfiumRasterizer {
    pub fn new() -> Arc<PdfiumRasterizer> {
        Arc::new(PdfiumRasterizer { engine: pdf_pdfium::PdfiumEngine::new().expect("pdfium") })
    }
}

impl pdf_pdfa::Rasterizer for PdfiumRasterizer {
    fn render_page(&self, pdf: &[u8], page: u32, dpi: f32) -> Result<pdf_pdfa::RasterPage, String> {
        let doc = self.engine.open_bytes(pdf.to_vec()).map_err(|e| e.to_string())?;
        let zoom = (dpi / 72.0).min(ZoomLevel::MAX);
        let r = self.engine.render(RenderRequest { document: doc, page: PageIndex(page), zoom: ZoomLevel(zoom), rotation: Rotation::R0 });
        let runs = self.engine.page_text_layout(doc, PageIndex(page)).unwrap_or_default();
        let _ = self.engine.close(doc);
        let r = r.map_err(|e| e.to_string())?;
        let rgb: Vec<u8> = r.rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let words = runs
            .into_iter()
            .map(|t| pdf_pdfa::RasterWord {
                text: t.text,
                x: t.rect_pt.x0 * zoom,
                y: t.rect_pt.y0 * zoom,
                w: (t.rect_pt.x1 - t.rect_pt.x0) * zoom,
                h: (t.rect_pt.y1 - t.rect_pt.y0) * zoom,
            })
            .collect();
        Ok(pdf_pdfa::RasterPage { width: r.width_px, height: r.height_px, rgb, words })
    }
}

// ---------------------------------------------------------------- veraPDF

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The veraPDF launcher, if staged (scripts/fetch-verapdf.ps1) or set via
/// the VERAPDF environment variable.
pub fn verapdf() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("VERAPDF") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let staged = workspace_root().join("vendor").join("verapdf").join(if cfg!(windows) { "verapdf.cmd" } else { "verapdf" });
    staged.exists().then_some(staged)
}

/// Validate every PDF in `dir` against `flavour` ("1b", "4f", …).
/// Returns file name → list of failed rule ids (empty = pass).
pub fn validate_dir(vera: &Path, dir: &Path, flavour: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(vera);
        c
    } else {
        std::process::Command::new(vera)
    };
    let flav = flavour.to_string();
    let out = cmd
        .arg("--flavour")
        .arg(&flav)
        .arg("--format")
        .arg("xml")
        .arg("--maxfailuresdisplayed")
        .arg("1")
        .arg(dir)
        .output()
        .expect("run veraPDF");
    let xml = String::from_utf8_lossy(&out.stdout).to_string();
    parse_report(&xml)
}

/// Minimal parsing of veraPDF's XML report.
pub fn parse_report(xml: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut res = std::collections::BTreeMap::new();
    for job in xml.split("<job>").skip(1) {
        let name = job
            .split("<item size=")
            .nth(1)
            .and_then(|s| s.split("<name>").nth(1))
            .and_then(|s| s.split("</name>").next())
            .map(|p| Path::new(p).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())
            .unwrap_or_default();
        let compliant = job.contains("isCompliant=\"true\"");
        let mut failed = Vec::new();
        if !compliant {
            for rule in job.split("<rule ").skip(1) {
                if rule.contains("status=\"failed\"") {
                    let attr = |k: &str| rule.split(&format!("{k}=\"")).nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
                    failed.push(format!("{}-{}", attr("clause"), attr("testNumber")));
                }
            }
            if failed.is_empty() {
                let err = job.split("<exceptionMessage>").nth(1).and_then(|s| s.split("</").next()).unwrap_or("not compliant");
                failed.push(err.to_string());
            }
        }
        res.insert(name, failed);
    }
    res
}
