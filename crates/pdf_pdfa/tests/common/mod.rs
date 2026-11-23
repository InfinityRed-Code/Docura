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
        d.extend(&extra);
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
