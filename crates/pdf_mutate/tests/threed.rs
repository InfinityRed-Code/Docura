//! 3D annotations: write → read round trip, view parsing, and what page
//! operations do with the embedded model.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_3d::u3d::{ExportMesh, ExportScene, IDENTITY};
use pdf_mutate::{MutableDoc, ThreeDAnnotation};
use pdf_types::{RectPt, ThreeDView};

fn blank_pdf(pages: usize) -> Vec<u8> {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let kids: Vec<Object> = (0..pages)
        .map(|_| {
            let c = d.add_object(Stream::new(dictionary! {}, b"0 0 1 rg 10 10 50 50 re f".to_vec()));
            d.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 600.into(), 400.into()],
                "Contents" => c,
            })
            .into()
        })
        .collect();
    d.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 }));
    let cat = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", cat);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

fn tetra_u3d() -> Vec<u8> {
    pdf_3d::u3d::write(&ExportScene {
        groups: Vec::new(),
        meshes: vec![ExportMesh {
            name: "Teil".into(),
            parent: None,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            normals: vec![],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
            color: [0.8, 0.2, 0.2, 1.0],
            transform: IDENTITY,
        }],
        units_m: Some(0.001),
    })
}

fn png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(8, 6, image::Rgb([200, 210, 220]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn view(name: &str) -> ThreeDView {
    ThreeDView {
        name: name.into(),
        internal_name: Some(name.into()),
        c2w: Some(pdf_mutate::c2w_look_at([0.5, -5.0, 0.5], [0.5, 0.0, 0.5], [0.0, 0.0, 1.0])),
        center_of_orbit: Some(5.0),
        fov: Some(30.0),
        fov_span: Some("Min".into()),
        background: Some([1.0, 1.0, 1.0]),
        render_mode: Some("Solid".into()),
        lighting: Some("CAD".into()),
        ..Default::default()
    }
}

fn with_3d(pages: usize, page: u32) -> Vec<u8> {
    let mut doc = MutableDoc::from_bytes(&blank_pdf(pages)).unwrap();
    doc.add_3d_annotation(
        page,
        &ThreeDAnnotation {
            rect: RectPt { x0: 100.0, y0: 50.0, x1: 400.0, y1: 250.0 },
            u3d: tetra_u3d(),
            views: vec![view("Iso"), view("Vorne")],
            default_view: 1,
            poster: png(),
            name: "teil.stl".into(),
            activation: "XA".into(),
        },
    )
    .unwrap();
    doc.to_bytes().unwrap()
}

#[test]
fn written_annotation_reads_back() {
    let bytes = with_3d(1, 0);
    let doc = MutableDoc::from_bytes(&bytes).unwrap();
    let list = doc.list_3d();
    assert_eq!(list.len(), 1);
    let a = &list[0];
    assert_eq!(a.subtype, "3D");
    assert_eq!(a.format, "U3D");
    assert!(a.has_poster);
    assert_eq!(a.activation, "XA");
    assert_eq!(a.default_view, Some(1));
    assert_eq!(a.views.len(), 2);
    assert_eq!(a.views[1].name, "Vorne");
    assert_eq!(a.views[0].c2w.unwrap()[10], -5.0);
    assert_eq!(a.views[0].lighting.as_deref(), Some("CAD"));
    assert_eq!(a.views[0].fov_span.as_deref(), Some("Min"));
    assert_eq!(a.contents.as_deref(), Some("teil.stl"));
    // Top-origin rect as placed.
    assert!((a.rect_pt.x0 - 100.0).abs() < 0.01 && (a.rect_pt.y1 - 250.0).abs() < 0.01, "{:?}", a.rect_pt);

    // The model comes back byte-identical and decodes.
    let (format, model) = doc.model_3d(0, &a.id).unwrap();
    assert_eq!(format, "U3D");
    assert_eq!(model, tetra_u3d());
    let scene = pdf_3d::u3d::parse(&model).unwrap();
    assert_eq!(pdf_3d::render::prepare(&scene).triangle_count(), 4);

    // The comment list sees it as a "3D" annotation, and it can be deleted.
    let annots = doc.list_annotations();
    assert!(annots.iter().any(|x| x.subtype == "3D" && x.id == a.id));
    let mut doc = doc;
    assert!(doc.delete_annotation(0, &a.id).unwrap());
    assert!(doc.list_3d().is_empty());
}

#[test]
fn page_operations_keep_the_model() {
    let bytes = with_3d(3, 1);
    let doc = MutableDoc::from_bytes(&bytes).unwrap();
    // Extract the page with the model.
    let extracted = MutableDoc::from_bytes(&doc.extract_to_bytes(&[1]).unwrap()).unwrap();
    let list = extracted.list_3d();
    assert_eq!(list.len(), 1);
    assert_eq!(extracted.model_3d(0, &list[0].id).unwrap().1, tetra_u3d());
    // Insert it into another document.
    let mut other = MutableDoc::from_bytes(&blank_pdf(2)).unwrap();
    other.insert_pdf(&bytes, Some(&[1]), 1).unwrap();
    let other = MutableDoc::from_bytes(&other.to_bytes().unwrap()).unwrap();
    let list = other.list_3d();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].page, 1);
    assert_eq!(other.model_3d(1, &list[0].id).unwrap().1, tetra_u3d());
    // Rotating the page keeps it, with the rect turned like the page.
    let mut rotated = MutableDoc::from_bytes(&bytes).unwrap();
    rotated.rotate_pages(&[1], 90).unwrap();
    let rotated = MutableDoc::from_bytes(&rotated.to_bytes().unwrap()).unwrap();
    assert_eq!(rotated.list_3d().len(), 1);
}

#[test]
fn redaction_removes_the_model() {
    let bytes = with_3d(1, 0);
    let mut doc = MutableDoc::from_bytes(&bytes).unwrap();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[255u8; 3 * 4 * 4], 4, 4, image::ExtendedColorType::Rgb8)
        .unwrap();
    doc.replace_page_with_jpeg(0, jpeg, 4, 4, 600.0, 400.0).unwrap();
    let out = doc.to_bytes().unwrap();
    let d = Document::load_mem(&out).unwrap();
    let has_3d_stream = d.objects.values().any(|o| {
        o.as_stream().ok().and_then(|s| s.dict.get(b"Type").ok()).and_then(|t| t.as_name().ok()) == Some(b"3D".as_slice())
    });
    assert!(!has_3d_stream, "redaction must drop the 3D model data");
}

/// Files written by other tools: /3DV by name, /MS /U3D views, a 3D
/// reference dictionary, a RichMedia annotation and a PRC stream.
#[test]
fn foreign_structures_are_understood() {
    let mut d = Document::load_mem(&blank_pdf(1)).unwrap();
    let page = *d.get_pages().get(&1).unwrap();
    let u3d = d.add_object(Stream::new(
        dictionary! {
            "Type" => "3D", "Subtype" => "U3D",
            "VA" => vec![
                Object::Dictionary(dictionary! { "XN" => Object::string_literal("A"), "IN" => Object::string_literal("a"), "MS" => "U3D", "U3DPath" => Object::string_literal("DefaultView") }),
                Object::Dictionary(dictionary! { "XN" => Object::string_literal("B"), "IN" => Object::string_literal("b"),
                    "P" => dictionary! { "Subtype" => "O", "OS" => 2.0, "OB" => "W" },
                    "SA" => vec![Object::Dictionary(dictionary! { "O" => vec![Object::Null, 0.into(), 45.into()], "C" => vec![1.into(), 2.into(), 3.into()] })],
                    "NA" => vec![Object::Dictionary(dictionary! { "N" => Object::string_literal("Teil"), "V" => false, "O" => 0.5 })],
                }),
            ],
        },
        tetra_u3d(),
    ));
    let reference = d.add_object(dictionary! { "Type" => "3DRef", "3DD" => u3d });
    let a1 = d.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "3D", "Rect" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "3DD" => reference, "3DV" => Object::string_literal("b"), "NM" => Object::string_literal("ref"),
        "3DA" => dictionary! { "A" => "PV", "NP" => true },
    });
    let ef = d.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, tetra_u3d()));
    let spec = d.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("model.u3d"), "EF" => dictionary! { "F" => ef } });
    let a2 = d.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "RichMedia", "Rect" => vec![100.into(), 0.into(), 200.into(), 100.into()],
        "NM" => Object::string_literal("rm"),
        "RichMediaContent" => dictionary! {
            "Assets" => dictionary! { "Names" => vec![Object::string_literal("model.u3d"), spec.into()] },
            "Configurations" => vec![Object::Dictionary(dictionary! { "Subtype" => "3D", "Instances" => vec![Object::Dictionary(dictionary! { "Subtype" => "3D", "Asset" => spec })] })],
        },
    });
    let prc = d.add_object(Stream::new(dictionary! { "Type" => "3D", "Subtype" => "PRC" }, b"PRC\0data".to_vec()));
    let a3 = d.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "3D", "Rect" => vec![200.into(), 0.into(), 300.into(), 100.into()],
        "3DD" => prc, "NM" => Object::string_literal("prc"),
    });
    d.get_dictionary_mut(page).unwrap().set("Annots", vec![a1.into(), a2.into(), a3.into()]);
    let mut bytes = Vec::new();
    d.save_to(&mut bytes).unwrap();

    let doc = MutableDoc::from_bytes(&bytes).unwrap();
    let list = doc.list_3d();
    assert_eq!(list.len(), 3, "{list:?}");
    let r = list.iter().find(|a| a.id == "ref").unwrap();
    assert_eq!(r.format, "U3D");
    assert_eq!(r.default_view, Some(1), "/3DV by internal name");
    assert_eq!(r.activation, "PV");
    assert!(r.model_tree);
    assert!(!r.has_poster);
    assert_eq!(r.views[0].u3d_path, ["DefaultView"]);
    let b = &r.views[1];
    assert!(b.orthographic);
    assert_eq!((b.ortho_scale, b.ortho_binding.as_deref()), (Some(2.0), Some("W")));
    assert_eq!(b.cross_sections[0].orientation, [None, Some(0.0), Some(45.0)]);
    assert_eq!(b.cross_sections[0].center, [1.0, 2.0, 3.0]);
    assert_eq!(b.nodes[0].visible, Some(false));
    assert_eq!(b.nodes[0].opacity, Some(0.5));
    let rm = list.iter().find(|a| a.id == "rm").unwrap();
    assert_eq!((rm.subtype.as_str(), rm.format.as_str()), ("RichMedia", "U3D"));
    assert_eq!(doc.model_3d(0, "rm").unwrap().1, tetra_u3d());
    let p = list.iter().find(|a| a.id == "prc").unwrap();
    assert_eq!(p.format, "PRC");
}

/// The /C2W convention of written views: columns left, up, view and the
/// position (a proper rotation), as media9 and Acrobat write it. The same
/// reference values are checked for the app's TypeScript camera math in
/// `apps/desktop-tauri/ui/tests/threed-camera.test.mjs`.
#[test]
fn c2w_convention_is_left_up_view() {
    let close = |a: &[f32], b: &[f32]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
    // media9's default camera: at -Y looking along +Y, Z up.
    let front = pdf_mutate::c2w_look_at([0.0, -10.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    assert!(close(&front, &[-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, -10.0, 0.0]), "{front:?}");
    // Top view with +Y up on screen.
    let top = pdf_mutate::c2w_look_at([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    assert!(close(&top, &[-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 5.0]), "{top:?}");
    // Any view: orthonormal, determinant +1, x = y x z, up column points up.
    let m = pdf_mutate::c2w_look_at([7.0, -4.0, 3.0], [1.0, 2.0, 0.5], [0.0, 0.0, 1.0]);
    let (x, y, z) = (&m[0..3], &m[3..6], &m[6..9]);
    let dot = |a: &[f32], b: &[f32]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: &[f32], b: &[f32]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    for v in [x, y, z] {
        assert!((dot(v, v) - 1.0).abs() < 1e-5);
    }
    assert!(dot(x, y).abs() < 1e-5 && dot(y, z).abs() < 1e-5 && dot(x, z).abs() < 1e-5);
    assert!(close(x, &cross(y, z)), "x = y x z (proper rotation)");
    assert!(y[2] > 0.0, "up column points up");

    // The matrix survives writing and reading unchanged.
    let doc = MutableDoc::from_bytes(&with_3d(1, 0)).unwrap();
    let read = doc.list_3d()[0].views[0].c2w.unwrap();
    assert!(close(&read, &[-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.5, -5.0, 0.5]), "{read:?}");
}

/// On a page with /Rotate the poster is turned into the appearance form so
/// it reads upright on screen, and the placement rect comes back as given.
#[test]
fn poster_is_upright_on_a_rotated_page() {
    let mut doc = MutableDoc::from_bytes(&blank_pdf(1)).unwrap();
    doc.rotate_pages(&[0], 90).unwrap();
    // The page is now 400 x 600 on screen; a 100 x 300 box in view space.
    let rect = RectPt { x0: 50.0, y0: 100.0, x1: 150.0, y1: 400.0 };
    doc.add_3d_annotation(
        0,
        &ThreeDAnnotation {
            rect,
            u3d: tetra_u3d(),
            views: vec![view("Iso")],
            default_view: 0,
            poster: png(),
            name: "teil.stl".into(),
            activation: "XA".into(),
        },
    )
    .unwrap();
    let bytes = doc.to_bytes().unwrap();
    let a = &MutableDoc::from_bytes(&bytes).unwrap().list_3d()[0];
    assert!((a.rect_pt.x0 - 50.0).abs() < 0.01 && (a.rect_pt.y1 - 400.0).abs() < 0.01, "{:?}", a.rect_pt);

    let d = Document::load_mem(&bytes).unwrap();
    let form = d
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .find(|s| s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok()) == Some(b"Form".as_slice()))
        .expect("poster form");
    // In page space the box is 300 wide and 100 high.
    let bbox: Vec<f32> = form.dict.get(b"BBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
    assert_eq!(bbox, vec![0.0, 0.0, 300.0, 100.0]);
    // The image (100 x 300 on screen) is turned a quarter into the form.
    let content = String::from_utf8(form.content.clone()).unwrap();
    assert_eq!(content, "q\n0.0000 100.0000 -300.0000 0.0000 300.0000 0.0000 cm\n/Im0 Do\nQ\n");
}

/// A model without preset views: no /VA, /DV or /3DV is written (an index
/// into an empty view array is invalid), and it reads back without a view.
#[test]
fn model_without_views_writes_no_view_entries() {
    let mut doc = MutableDoc::from_bytes(&blank_pdf(1)).unwrap();
    doc.add_3d_annotation(
        0,
        &ThreeDAnnotation {
            rect: RectPt { x0: 100.0, y0: 50.0, x1: 400.0, y1: 250.0 },
            u3d: tetra_u3d(),
            views: Vec::new(),
            default_view: 0,
            poster: png(),
            name: "teil.u3d".into(),
            activation: "PO".into(),
        },
    )
    .unwrap();
    let bytes = doc.to_bytes().unwrap();
    let a = &MutableDoc::from_bytes(&bytes).unwrap().list_3d()[0];
    assert!(a.views.is_empty());
    assert_eq!(a.default_view, None);
    assert_eq!(a.activation, "PO");
    let d = Document::load_mem(&bytes).unwrap();
    for o in d.objects.values() {
        let dict = match o {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        for key in [b"VA".as_slice(), b"DV", b"3DV"] {
            assert!(dict.get(key).is_err(), "{} written", String::from_utf8_lossy(key));
        }
    }
}
