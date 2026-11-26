//! Sticky-note icons (issue #15): on every combination of page /Rotate and
//! view rotation the icon is drawn upright, growing right and down from the
//! clicked point. Covers notes as this app writes them, notes with an
//! explicit NoRotate flag and Acrobat-style notes that carry an appearance
//! stream, the tiled render path, and that rendering leaves the document
//! untouched.
//!
//! One test function only: PDFium tests of one binary must not run in
//! parallel.

use lopdf::{Dictionary, Document, Object, Stream};
use pdf_core::{PdfEngine, RenderRequest, RenderedPage};
use pdf_mutate::{MutableDoc, TextNoteAnnotation};
use pdf_pdfium::PdfiumEngine;
use pdf_types::{DocumentId, PageIndex, Rotation, ZoomLevel};

const ZOOM: f32 = 2.0;
/// Icon side in pixels: the 22 pt square the app writes, at `ZOOM`.
const SIDE: i32 = 44;
/// Unrotated page size; the MediaBox does not start at the origin.
const PAGE: (f32, f32) = (400.0, 600.0);

/// Clicked points, in displayed page space (top-left origin, /Rotate
/// applied, view rotation 0).
const PLAIN: (f32, f32) = (120.0, 150.0);
const NO_ROTATE: (f32, f32) = (220.0, 150.0);
const ACROBAT: (f32, f32) = (120.0, 300.0);
const HIDDEN: (f32, f32) = (220.0, 300.0);

const RED: [f32; 3] = [0.9, 0.2, 0.1];
const GREEN: [f32; 3] = [0.1, 0.8, 0.2];

fn blank_page(rotate: i32) -> Vec<u8> {
    let mut d = Document::with_version("1.5");
    let pages_id = d.new_object_id();
    let page_id = d.new_object_id();
    let cat_id = d.new_object_id();
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set(
        "MediaBox",
        Object::Array(vec![10.into(), 20.into(), 410.into(), 620.into()]),
    );
    page.set("Rotate", Object::Integer(rotate as i64));
    d.objects.insert(page_id, Object::Dictionary(page));
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages.set("Count", Object::Integer(1));
    d.objects.insert(pages_id, Object::Dictionary(pages));
    let mut cat = Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    d.objects.insert(cat_id, Object::Dictionary(cat));
    d.trailer.set("Root", Object::Reference(cat_id));
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

fn annot_ids(doc: &Document) -> Vec<lopdf::ObjectId> {
    let page_id = doc.page_iter().next().unwrap();
    let annots = doc.get_dictionary(page_id).unwrap().get(b"Annots").unwrap();
    let annots = match annots {
        Object::Reference(r) => doc.get_object(*r).unwrap(),
        o => o,
    };
    annots.as_array().unwrap().iter().map(|o| o.as_reference().unwrap()).collect()
}

/// A page with /Rotate `rotate` and four notes placed through the app's own
/// writer: plain, explicit NoRotate, Acrobat-style (appearance stream that
/// fills its box blue) and hidden.
fn pdf_with_notes(rotate: i32) -> Vec<u8> {
    let mut m = MutableDoc::from_bytes(&blank_page(rotate)).unwrap();
    for (at, color, icon) in [
        (PLAIN, RED, None),
        (NO_ROTATE, RED, None),
        (ACROBAT, GREEN, Some("Comment")),
        (HIDDEN, RED, None),
    ] {
        m.add_text_note_annotation(
            0,
            &TextNoteAnnotation {
                x_pt: at.0,
                y_pt: at.1,
                contents: "Notiz".into(),
                color,
                icon: icon.map(str::to_string),
                author: None,
                print: true,
            },
        )
        .unwrap();
    }
    let mut d = Document::load_mem(&m.to_bytes().unwrap()).unwrap();
    let ids = annot_ids(&d);
    // Print | NoZoom | NoRotate, as Acrobat writes its notes.
    for id in [ids[1], ids[2]] {
        d.get_dictionary_mut(id).unwrap().set("F", Object::Integer(28));
    }
    let mut form = Dictionary::new();
    form.set("Type", Object::Name(b"XObject".to_vec()));
    form.set("Subtype", Object::Name(b"Form".to_vec()));
    form.set("BBox", Object::Array(vec![0.into(), 0.into(), 22.into(), 22.into()]));
    let form_id = d.add_object(Object::Stream(Stream::new(form, b"0 0 1 rg 0 0 22 22 re f".to_vec())));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    d.get_dictionary_mut(ids[2]).unwrap().set("AP", Object::Dictionary(ap));
    // Hidden.
    d.get_dictionary_mut(ids[3]).unwrap().set("F", Object::Integer(2));
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    out
}

/// Size of the page as displayed with /Rotate `page_rot` and `view` quarter
/// turns of view rotation.
fn shown_size(page_rot: i32, view: i32) -> (f32, f32) {
    if (page_rot / 90 + view) % 2 == 0 { PAGE } else { (PAGE.1, PAGE.0) }
}

/// Bitmap pixel of a point given in displayed page space (view rotation 0).
fn pixel(p: (f32, f32), page_rot: i32, view: i32) -> (i32, i32) {
    let (w, h) = shown_size(page_rot, 0);
    let q = match view {
        1 => (h - p.1, p.0),
        2 => (w - p.0, h - p.1),
        3 => (p.1, w - p.0),
        _ => p,
    };
    ((q.0 * ZOOM).round() as i32, (q.1 * ZOOM).round() as i32)
}

fn px(r: &RenderedPage, x: i32, y: i32) -> [u8; 3] {
    let o = ((y as u32 * r.width_px + x as u32) * 4) as usize;
    [r.rgba[o], r.rgba[o + 1], r.rgba[o + 2]]
}

fn is_white(c: [u8; 3]) -> bool {
    c.iter().all(|&v| v > 245)
}

/// RGB of the `SIDE` square at `at`, row by row.
fn patch(r: &RenderedPage, at: (i32, i32)) -> Vec<[u8; 3]> {
    let mut out = Vec::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            out.push(px(r, at.0 + x, at.1 + y));
        }
    }
    out
}

/// Bounding box of the non-white pixels within `margin` pixels around the
/// `SIDE` square at `at`, relative to `at`.
fn ink_bbox(r: &RenderedPage, at: (i32, i32), margin: i32) -> Option<(i32, i32, i32, i32)> {
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for y in -margin..SIDE + margin {
        for x in -margin..SIDE + margin {
            let (ax, ay) = (at.0 + x, at.1 + y);
            if ax < 0 || ay < 0 || ax >= r.width_px as i32 || ay >= r.height_px as i32 {
                continue;
            }
            if !is_white(px(r, ax, ay)) {
                let c = b.get_or_insert((x, y, x, y));
                *c = (c.0.min(x), c.1.min(y), c.2.max(x), c.3.max(y));
            }
        }
    }
    b
}

/// Same pixel. PDFium's anti-aliasing varies in the last bit with the
/// position on the bitmap.
fn same(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= 2)
}

fn near(c: [u8; 3], want: [f32; 3]) -> bool {
    c.iter().zip(want).all(|(&v, w)| (v as f32 - w * 255.0).abs() < 12.0)
}

fn request(document: DocumentId, view: i32) -> RenderRequest {
    RenderRequest {
        document,
        page: PageIndex(0),
        zoom: ZoomLevel(ZOOM),
        rotation: Rotation::from_quarter_turns(view as u8),
    }
}

#[test]
fn note_icons_stay_upright_at_the_clicked_point() {
    let engine = PdfiumEngine::new().expect("pdfium");
    let mut bad: Vec<String> = Vec::new();
    // Icon pixels of the unrotated page in the unrotated view, per note.
    let mut reference: Vec<Vec<[u8; 3]>> = Vec::new();

    for page_rot in [0, 90, 180, 270] {
        let bytes = pdf_with_notes(page_rot);
        let before = Document::load_mem(&bytes).unwrap();
        let id = engine.open_bytes(bytes).unwrap();
        for view in 0..4 {
            let ctx = format!("/Rotate {page_rot}, view {}°", view * 90);
            let r = engine.render(request(id, view)).unwrap();
            // NOTE_ICONS_DUMP=<dir> writes the renders as PNG, to look at.
            if let Some(dir) = std::env::var_os("NOTE_ICONS_DUMP") {
                let file = std::path::Path::new(&dir).join(format!("rotate{page_rot}-view{}.png", view * 90));
                image::save_buffer(file, &r.rgba, r.width_px, r.height_px, image::ColorType::Rgba8).unwrap();
            }
            let (w, h) = shown_size(page_rot, view);
            assert_eq!((r.width_px, r.height_px), ((w * ZOOM) as u32, (h * ZOOM) as u32), "{ctx}");

            for (n, (name, click)) in
                [("plain", PLAIN), ("NoRotate", NO_ROTATE), ("Acrobat", ACROBAT)].into_iter().enumerate()
            {
                let at = pixel(click, page_rot, view);
                // The icon fills the square right of and below the click…
                match ink_bbox(&r, at, 60) {
                    Some(b) if b.0 >= -1 && b.1 >= -1 && b.2 <= SIDE && b.3 <= SIDE
                        && b.0 <= 8 && b.1 <= 8 && b.2 >= SIDE - 9 && b.3 >= SIDE - 9 => {}
                    other => bad.push(format!("{ctx}: {name} note covers {other:?} from the click, want about (0, 0, {SIDE}, {SIDE})")),
                }
                // …and looks exactly as on the unrotated page.
                let p = patch(&r, at);
                if page_rot == 0 && view == 0 {
                    reference.push(p);
                } else if !p.iter().zip(&reference[n]).all(|(a, b)| same(*a, *b)) {
                    let off = p.iter().zip(&reference[n]).filter(|(a, b)| !same(**a, **b)).count();
                    bad.push(format!("{ctx}: {name} note differs from the upright icon in {off} pixels"));
                }
            }

            // A hidden note is not drawn.
            if let Some(b) = ink_bbox(&r, pixel(HIDDEN, page_rot, view), 30) {
                bad.push(format!("{ctx}: hidden note drawn at {b:?}"));
            }

            // Tiles show the same pixels as the whole page, also where a
            // tile border cuts through an icon.
            let at = pixel(PLAIN, page_rot, view);
            let (tx, ty, tw, th) = (at.0 + 17, at.1 + 9, 96u32, 80u32);
            let tile = engine.render_region(request(id, view), tx, ty, tw, th).unwrap();
            let tile_ok = (0..th as i32).all(|y| (0..tw as i32).all(|x| same(px(&tile, x, y), px(&r, tx + x, ty + y))));
            if !tile_ok {
                bad.push(format!("{ctx}: tile at ({tx}, {ty}) differs from the page render"));
            }
        }

        // Rendering changed nothing in the document PDFium holds: flags,
        // rectangles and appearances are as in the file.
        let after = Document::load_mem(&engine.save_copy_bytes(id).unwrap()).unwrap();
        let (ids_a, ids_b) = (annot_ids(&after), annot_ids(&before));
        assert_eq!(ids_a.len(), ids_b.len());
        for (i, (a, b)) in ids_a.iter().zip(&ids_b).enumerate() {
            let (a, b) = (after.get_dictionary(*a).unwrap(), before.get_dictionary(*b).unwrap());
            let rect = |d: &Dictionary| -> Vec<f32> {
                d.get(b"Rect").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect()
            };
            if a.get(b"F").unwrap().as_i64().unwrap() != b.get(b"F").unwrap().as_i64().unwrap()
                || rect(a) != rect(b)
                || a.has(b"AP") != b.has(b"AP")
            {
                bad.push(format!("/Rotate {page_rot}: note {i} changed by rendering: {a:?}"));
            }
        }
        engine.close(id).unwrap();
    }

    // The reference icons themselves: upright, in the colour of /C.
    let at = |fx: f32, fy: f32| ((fx * SIDE as f32) as usize) + ((fy * SIDE as f32) as usize) * SIDE as usize;
    for n in [0, 1] {
        let p = &reference[n];
        // Sheet body, the folded-away lower right corner, the lower left.
        if !near(p[at(0.5, 0.2)], RED) || !is_white(p[at(0.93, 0.93)]) || !near(p[at(0.2, 0.8)], RED) {
            bad.push(format!("note {n}: not an upright red note sheet"));
        }
    }
    let p = &reference[2];
    // Bubble body, nothing right of the tail, and no trace of the blue
    // appearance stream.
    if !near(p[at(0.5, 0.25)], GREEN) || !is_white(p[at(0.8, 0.85)]) || p.iter().any(|&c| near(c, [0.0, 0.0, 1.0])) {
        bad.push("Acrobat-style note: not an upright green speech bubble".into());
    }
    if reference[0] != reference[1] || reference[0] == reference[2] {
        bad.push("icons: NoRotate must not change the icon, /Name Comment must".into());
    }

    assert!(bad.is_empty(), "note icons off:\n{}", bad.join("\n"));
}
