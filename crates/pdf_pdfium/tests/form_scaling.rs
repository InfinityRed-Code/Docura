//! Form fields stay on the page (issue #28): the reported widget rectangles
//! match what PDFium paints for every page geometry, a filled-in field is
//! drawn with the font size of its /DA at every zoom, and the widgets are
//! part of region renders as well as of whole-page renders.

mod common {
    pub mod forms;
}

use common::forms::{self, PageSpec};
use pdf_core::{PdfEngine, RenderRequest, RenderedPage};
use pdf_mutate::MutableDoc;
use pdf_pdfium::PdfiumEngine;
use pdf_types::{DocumentId, FieldTextStyle, FormFieldInfo, PageIndex, RectPt, Rotation, ZoomLevel};

/// One engine for all tests, one test at a time: PDFium is not thread-safe
/// and the test harness runs the tests of a file in parallel.
fn engine() -> std::sync::MutexGuard<'static, PdfiumEngine> {
    static ENGINE: std::sync::OnceLock<std::sync::Mutex<PdfiumEngine>> = std::sync::OnceLock::new();
    ENGINE
        .get_or_init(|| std::sync::Mutex::new(PdfiumEngine::new().expect("pdfium.dll")))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn render(engine: &PdfiumEngine, doc: DocumentId, zoom: f32) -> RenderedPage {
    engine
        .render(RenderRequest { document: doc, page: PageIndex(0), zoom: ZoomLevel(zoom), rotation: Rotation::R0 })
        .expect("render")
}

fn px(img: &RenderedPage, x: i64, y: i64) -> [u8; 3] {
    if x < 0 || y < 0 || x >= img.width_px as i64 || y >= img.height_px as i64 {
        return [255; 3];
    }
    let o = ((y as u32 * img.width_px + x as u32) * 4) as usize;
    [img.rgba[o], img.rgba[o + 1], img.rgba[o + 2]]
}

/// The frames the test page draws on every widget rectangle: a red
/// hairline, pale where it is thinner than a pixel.
fn is_frame(p: [u8; 3]) -> bool {
    p[0] as i32 - p[1] as i32 > 20 && p[0] as i32 - p[2] as i32 > 20
}

/// Black text, including its anti-aliased edge.
fn is_ink(p: [u8; 3]) -> bool {
    p[0] < 150 && p[1] < 150 && p[2] < 150
}

/// Share of an edge (a row or column of the bitmap, give or take `slack`
/// pixels across) that is covered by the frame colour.
fn frame_cover(img: &RenderedPage, horizontal: bool, at: f32, from: f32, to: f32, slack: i64) -> f32 {
    let (a, b) = (from.round() as i64 + 3, to.round() as i64 - 3);
    if b - a < 4 {
        // Too short to tell the edge from its corners.
        return 1.0;
    }
    let mut hit = 0;
    for t in a..b {
        let found = (-slack..=slack).any(|d| {
            let c = at.round() as i64 + d;
            is_frame(if horizontal { px(img, t, c) } else { px(img, c, t) })
        });
        hit += found as i32;
    }
    hit as f32 / (b - a).max(1) as f32
}

/// Bounding box of the dark pixels inside `r` (displayed points) at `zoom`,
/// `inset` pixels in from its edges: (x0, y0, x1, y1) in pixels.
fn ink_box(img: &RenderedPage, r: &RectPt, zoom: f32, inset: i64) -> Option<(i64, i64, i64, i64)> {
    let (x0, y0) = ((r.x0 * zoom).round() as i64 + inset, (r.y0 * zoom).round() as i64 + inset);
    let (x1, y1) = ((r.x1 * zoom).round() as i64 - inset, (r.y1 * zoom).round() as i64 - inset);
    let mut b: Option<(i64, i64, i64, i64)> = None;
    for y in y0..y1 {
        for x in x0..x1 {
            if is_ink(px(img, x, y)) {
                let e = b.get_or_insert((x, y, x, y));
                *e = (e.0.min(x), e.1.min(y), e.2.max(x), e.3.max(y));
            }
        }
    }
    b
}

fn field<'a>(fields: &'a [FormFieldInfo], id: &str) -> &'a FormFieldInfo {
    fields.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("field {id}"))
}

fn style(engine: &PdfiumEngine, doc: DocumentId, f: &FormFieldInfo) -> FieldTextStyle {
    let bytes = engine.document_bytes(doc).unwrap();
    let styles = MutableDoc::from_bytes(&bytes).unwrap().form_text_styles();
    styles.get(&(f.page, f.annot_index)).cloned().unwrap_or_else(|| panic!("no style for {}", f.id))
}

/// Sets a field the way the app does: mutate the bytes, reload the engine.
fn set(engine: &PdfiumEngine, doc: DocumentId, name: &str, value: &str) {
    let bytes = engine.document_bytes(doc).unwrap();
    let mut m = MutableDoc::from_bytes(&bytes).unwrap();
    m.set_form_field(name, value).unwrap();
    engine.reload_bytes(doc, m.to_bytes().unwrap()).unwrap();
}

#[test]
fn widget_rectangles_sit_on_their_frames_in_every_page_geometry() {
    let engine = engine();
    for (name, spec) in forms::variants() {
        let doc = engine.open_bytes(forms::form_pdf(spec)).unwrap();
        let fields = engine.list_form_fields(doc).unwrap();
        assert_eq!(fields.len(), forms::fields().len() + 3, "{name}: field count");
        for zoom in [0.5_f32, 1.0, 2.0, 4.0] {
            let img = render(&engine, doc, zoom);
            // The frame is 0.5 units wide and centred on the rectangle edge.
            let slack = (zoom * 0.75).ceil() as i64 + 1;
            for f in &fields {
                let r = &f.rect_pt;
                let (x0, y0, x1, y1) = (r.x0 * zoom, r.y0 * zoom, r.x1 * zoom, r.y1 * zoom);
                let edges = [
                    ("top", frame_cover(&img, true, y0, x0, x1, slack)),
                    ("bottom", frame_cover(&img, true, y1, x0, x1, slack)),
                    ("left", frame_cover(&img, false, x0, y0, y1, slack)),
                    ("right", frame_cover(&img, false, x1, y0, y1, slack)),
                ];
                for (edge, cover) in edges {
                    assert!(
                        cover > 0.8,
                        "{name} @{zoom}: {} edge of {} is off its frame (cover {cover:.2}, rect {r:?})",
                        edge, f.id
                    );
                }
            }
        }
        engine.close(doc).unwrap();
    }
}

#[test]
fn filled_in_text_has_the_field_font_size_at_every_zoom() {
    let engine = engine();
    let doc = engine.open_bytes(forms::form_pdf(PageSpec::default())).unwrap();
    set(&engine, doc, "big24", "HHHH");
    set(&engine, doc, "text12", "HHHH");
    set(&engine, doc, "auto", "HHHH");
    let fields = engine.list_form_fields(doc).unwrap();
    assert_eq!(field(&fields, "big24").value.as_deref(), Some("HHHH"));

    let big = style(&engine, doc, field(&fields, "big24"));
    assert_eq!(big.font_size_pt, 24.0);
    assert!(!big.auto_size);
    assert_eq!(style(&engine, doc, field(&fields, "text12")).font_size_pt, 12.0);
    // Auto size: the 30 pt high field is filled, (30 - 2) / 1.15.
    let auto = style(&engine, doc, field(&fields, "auto"));
    assert!(auto.auto_size);
    assert_eq!(auto.font_size_pt, pdf_mutate::auto_size_single(30.0, 0.0, 1000.0, 1.0));
    // Fields that were not touched keep the size of their own appearance.
    assert_eq!(style(&engine, doc, field(&fields, "small6")).font_size_pt, 6.0);
    assert_eq!(style(&engine, doc, field(&fields, "multiauto")).font_size_pt, 12.0);

    // Capital H is 0.718 em high: the ink must scale with size and zoom.
    for zoom in [0.5_f32, 1.0, 2.0, 4.0] {
        let img = render(&engine, doc, zoom);
        for (id, size) in [("big24", 24.0), ("text12", 12.0), ("auto", auto.font_size_pt)] {
            if size * zoom < 10.0 {
                // Too few pixels for a measurement.
                continue;
            }
            let f = field(&fields, id);
            let b = ink_box(&img, &f.rect_pt, zoom, 2).unwrap_or_else(|| panic!("{id} @{zoom}: no text drawn"));
            let got = (b.3 - b.1 + 1) as f32;
            let want = 0.718 * size * zoom;
            assert!((got - want).abs() <= 1.5 + 0.03 * want, "{id} @{zoom}: text {got} px high, expected {want:.1}");
            // Vertically centred in the field.
            let mid = (b.1 + b.3 + 1) as f32 / 2.0;
            let field_mid = (f.rect_pt.y0 + f.rect_pt.y1) * zoom / 2.0;
            assert!((mid - field_mid).abs() <= 1.5 + 0.02 * want, "{id} @{zoom}: text centre {mid}, field centre {field_mid}");
        }
    }
}

#[test]
fn alignment_and_untouched_fields_survive_an_edit() {
    let engine = engine();
    let doc = engine.open_bytes(forms::form_pdf(PageSpec::default())).unwrap();
    let before = render(&engine, doc, 2.0);
    set(&engine, doc, "right", "HHHH");
    let fields = engine.list_form_fields(doc).unwrap();
    let after = render(&engine, doc, 2.0);

    // Right aligned (/Q 2): the text ends at the padding, 2 pt from the edge.
    let f = field(&fields, "right");
    assert_eq!(style(&engine, doc, f).align, 2);
    let b = ink_box(&after, &f.rect_pt, 2.0, 2).expect("text");
    let right_gap = f.rect_pt.x1 * 2.0 - (b.2 + 1) as f32;
    assert!((right_gap - 2.0 * 2.0).abs() <= 2.5, "gap to the right edge: {right_gap} px");

    // Every other field is drawn exactly as before: no /NeedAppearances that
    // would make the viewer redraw all of them.
    for other in fields.iter().filter(|o| o.id != "right") {
        let r = &other.rect_pt;
        for y in (r.y0 * 2.0) as i64..(r.y1 * 2.0) as i64 {
            for x in (r.x0 * 2.0) as i64..(r.x1 * 2.0) as i64 {
                assert_eq!(px(&before, x, y), px(&after, x, y), "{} changed at {x},{y}", other.id);
            }
        }
    }
    let bytes = engine.document_bytes(doc).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("NeedAppearances"));
}

#[test]
fn text_follows_the_page_rotation() {
    let engine = engine();
    for (rotate, turns) in [(90, 1u8), (180, 2), (270, 3)] {
        let doc = engine.open_bytes(forms::form_pdf(PageSpec { rotate, ..PageSpec::default() })).unwrap();
        set(&engine, doc, "big24", "HHHHHH");
        let fields = engine.list_form_fields(doc).unwrap();
        let f = field(&fields, "big24");
        assert_eq!(style(&engine, doc, f).turns, turns, "/Rotate {rotate}");
        let img = render(&engine, doc, 2.0);
        let b = ink_box(&img, &f.rect_pt, 2.0, 2).expect("text drawn inside the field");
        let (w, h) = ((b.2 - b.0) as f32, (b.3 - b.1) as f32);
        // The line runs along the long side of the field as displayed.
        if turns % 2 == 1 {
            assert!(h > 2.0 * w, "/Rotate {rotate}: text box {w} x {h}");
            assert!((w + 1.0 - 0.718 * 24.0 * 2.0).abs() <= 2.5, "/Rotate {rotate}: cap height {w}");
        } else {
            assert!(w > 2.0 * h, "/Rotate {rotate}: text box {w} x {h}");
        }
        engine.close(doc).unwrap();
    }
}

#[test]
fn region_renders_show_the_fields_like_the_whole_page() {
    let engine = engine();
    let doc = engine.open_bytes(forms::form_pdf(PageSpec::default())).unwrap();
    set(&engine, doc, "text12", "Region");
    let fields = engine.list_form_fields(doc).unwrap();
    let zoom = 4.0;
    let whole = render(&engine, doc, zoom);
    // Text field, check box and radio button: all widgets.
    for id in ["text12", "big24", "check_on", "big_check", "choice"] {
        let f = fields.iter().find(|f| f.id == id && (f.kind != pdf_types::FormFieldKind::Radio || f.value.as_deref() == Some("On"))).unwrap();
        let r = &f.rect_pt;
        let (x, y) = ((r.x0 * zoom) as i32 - 8, (r.y0 * zoom) as i32 - 8);
        let (w, h) = (((r.x1 - r.x0) * zoom) as u32 + 16, ((r.y1 - r.y0) * zoom) as u32 + 16);
        let req = RenderRequest { document: doc, page: PageIndex(0), zoom: ZoomLevel(zoom), rotation: Rotation::R0 };
        let tile = engine.render_region(req, x, y, w, h).expect("region");
        let mut ink = 0;
        let mut differing = 0;
        for ty in 0..h as i64 {
            for tx in 0..w as i64 {
                let a = px(&tile, tx, ty);
                let b = px(&whole, tx + x as i64, ty + y as i64);
                ink += is_ink(a) as u32;
                differing += (0..3).any(|c| (a[c] as i32 - b[c] as i32).abs() > 24) as u32;
            }
        }
        assert!(ink > 40, "{id}: the region render shows no field content ({ink} dark pixels)");
        assert!(differing * 200 < w * h, "{id}: region and page render differ in {differing} of {} pixels", w * h);
    }
}

#[test]
fn documents_without_appearances_are_still_drawn_and_measured() {
    let engine = engine();
    let spec = PageSpec { appearances: false, ..PageSpec::default() };
    let doc = engine.open_bytes(forms::form_pdf(spec)).unwrap();
    let fields = engine.list_form_fields(doc).unwrap();
    // No appearance to read the size from: the /DA size.
    assert_eq!(style(&engine, doc, field(&fields, "big24")).font_size_pt, 24.0);
    let img = render(&engine, doc, 2.0);
    let f = field(&fields, "big24");
    let b = ink_box(&img, &f.rect_pt, 2.0, 2).expect("PDFium draws the value");
    // "Big 24 pt": capitals and descenders, between cap height and 1 em.
    let h = (b.3 - b.1 + 1) as f32 / 2.0;
    assert!(h > 0.7 * 24.0 && h < 1.05 * 24.0, "text {h} pt high for a 24 pt field");
}
