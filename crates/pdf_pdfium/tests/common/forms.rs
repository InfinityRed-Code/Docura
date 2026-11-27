//! Synthetic AcroForm documents for the form scaling tests (issue #28).
//!
//! One page with every field kind the app fills in. Next to each field the
//! page content itself draws a hairline frame on the widget rectangle and a
//! label in the field's font size, so a rendered page shows at a glance
//! whether a field (or an overlay on top of it) sits on its frame and
//! whether its text has the size of the label.

#![allow(dead_code)]

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};

/// Geometry of the test page.
#[derive(Debug, Clone, Copy)]
pub struct PageSpec {
    /// Page /Rotate in degrees.
    pub rotate: i64,
    /// Lower-left corner of the MediaBox.
    pub origin: (f32, f32),
    /// CropBox inset from the MediaBox on every side (0 = no CropBox).
    pub crop_inset: f32,
    /// /UserUnit (1 = absent). The layout shrinks by it, so the page keeps
    /// its displayed size.
    pub user_unit: f32,
    /// Write appearance streams, as form authoring tools do. Without them
    /// the document asks for /NeedAppearances.
    pub appearances: bool,
}

impl Default for PageSpec {
    fn default() -> Self {
        PageSpec { rotate: 0, origin: (0.0, 0.0), crop_inset: 0.0, user_unit: 1.0, appearances: true }
    }
}

/// Layout size of the page in points (A4).
pub const PAGE_W: f32 = 595.0;
pub const PAGE_H: f32 = 842.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Text,
    Multiline,
    Check,
    Radio,
    Combo,
    List,
    Signature,
}

/// One field of the test page, in layout coordinates (bottom-left origin,
/// before origin offset and user unit).
#[derive(Debug, Clone)]
pub struct Field {
    pub name: &'static str,
    pub kind: Kind,
    /// x0, y0, x1, y1.
    pub rect: [f32; 4],
    /// /DA font size; 0 = auto.
    pub size: f32,
    /// Font size the authoring tool chose for the appearance.
    pub ap_size: f32,
    pub value: &'static str,
    /// /Q alignment.
    pub q: i64,
}

pub fn fields() -> Vec<Field> {
    let f = |name, kind, rect, size, ap_size, value, q| Field { name, kind, rect, size, ap_size, value, q };
    vec![
        f("text12", Kind::Text, [150.0, 760.0, 420.0, 782.0], 12.0, 12.0, "Hamburg 12 pt", 0),
        f("auto", Kind::Text, [150.0, 715.0, 420.0, 745.0], 0.0, 20.0, "Auto size", 0),
        f("big24", Kind::Text, [150.0, 665.0, 420.0, 700.0], 24.0, 24.0, "Big 24 pt", 0),
        f("small6", Kind::Text, [150.0, 640.0, 420.0, 652.0], 6.0, 6.0, "Small 6 pt", 0),
        f("right", Kind::Text, [150.0, 605.0, 420.0, 627.0], 12.0, 12.0, "1.234,56", 2),
        f("multi10", Kind::Multiline, [150.0, 520.0, 420.0, 590.0], 10.0, 10.0, "Line one\nLine two\nLine three", 0),
        f("multiauto", Kind::Multiline, [150.0, 445.0, 420.0, 505.0], 0.0, 12.0, "Auto one\nAuto two", 0),
        f("check_on", Kind::Check, [150.0, 410.0, 166.0, 426.0], 0.0, 0.0, "Yes", 0),
        f("check_off", Kind::Check, [190.0, 410.0, 206.0, 426.0], 0.0, 0.0, "Off", 0),
        f("big_check", Kind::Check, [230.0, 396.0, 270.0, 436.0], 0.0, 0.0, "Yes", 0),
        f("combo", Kind::Combo, [150.0, 330.0, 320.0, 350.0], 11.0, 11.0, "Gruen", 0),
        f("list", Kind::List, [150.0, 250.0, 320.0, 315.0], 10.0, 10.0, "Blau", 0),
        f("sig", Kind::Signature, [150.0, 170.0, 380.0, 230.0], 0.0, 0.0, "", 0),
    ]
}

/// The three widgets of the radio group "choice" (the second one is on).
pub fn radio_rects() -> [[f32; 4]; 3] {
    [[150.0, 370.0, 166.0, 386.0], [190.0, 370.0, 206.0, 386.0], [230.0, 370.0, 246.0, 386.0]]
}

pub const CHOICES: [&str; 4] = ["Rot", "Gruen", "Blau", "Gelb"];

fn num(v: f32) -> Object {
    Object::Real(v)
}

fn text(s: &str) -> Object {
    Object::String(s.replace('\n', "\r").into_bytes(), StringFormat::Literal)
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

/// Baseline offset that centres Helvetica caps of `size` in a box `h` high.
fn baseline(h: f32, size: f32) -> f32 {
    (h - size * 0.718) / 2.0
}

fn form_xobject(doc: &mut Document, w: f32, h: f32, content: String, font: Option<ObjectId>) -> ObjectId {
    let mut d = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => vec![num(0.0), num(0.0), num(w), num(h)],
    };
    if let Some(f) = font {
        d.set("Resources", dictionary! { "Font" => dictionary! { "Helv" => f } });
    }
    doc.add_object(Stream::new(d, content.into_bytes()))
}

/// Builds the test document. `k = 1 / user_unit` scales the whole layout.
pub fn form_pdf(spec: PageSpec) -> Vec<u8> {
    let k = 1.0 / spec.user_unit;
    let (ox, oy) = spec.origin;
    let place = |r: [f32; 4]| [ox + r[0] * k, oy + r[1] * k, ox + r[2] * k, oy + r[3] * k];
    let rect_obj = |r: [f32; 4]| Object::Array(r.iter().map(|v| num(*v)).collect());

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });

    let mut content = String::from("0.5 w 0.75 0.2 0.2 RG\n");
    let mut annots: Vec<Object> = Vec::new();
    let mut roots: Vec<Object> = Vec::new();

    let frame = |content: &mut String, r: [f32; 4], label: &str, size: f32| {
        let p = place(r);
        content.push_str(&format!("{} {} {} {} re S\n", p[0], p[1], p[2] - p[0], p[3] - p[1]));
        if !label.is_empty() {
            let s = if size > 0.0 { size } else { 9.0 } * k;
            let y = p[1] + baseline(p[3] - p[1], s);
            content.push_str(&format!(
                "BT /Helv {s} Tf 0 g {} {y} Td ({}) Tj ET\n",
                ox + 20.0 * k,
                escape(label)
            ));
        }
    };

    for f in fields() {
        let r = place(f.rect);
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let label = match f.kind {
            Kind::Text | Kind::Multiline | Kind::Combo | Kind::List if f.size > 0.0 => {
                format!("{} pt Hxg", f.size)
            }
            Kind::Text | Kind::Multiline => format!("auto ({} pt)", f.ap_size),
            _ => f.name.to_string(),
        };
        frame(&mut content, f.rect, &label, f.ap_size);

        let mut d = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => rect_obj(r),
            "P" => page_id,
            "F" => 4,
            "T" => Object::string_literal(f.name),
        };
        let da = format!("/Helv {} Tf 0 g", f.size * k);
        let size = f.ap_size * k;
        match f.kind {
            Kind::Text | Kind::Multiline | Kind::Combo | Kind::List => {
                d.set("FT", if matches!(f.kind, Kind::Combo | Kind::List) { "Ch" } else { "Tx" });
                d.set("DA", Object::string_literal(da));
                d.set("V", text(f.value));
                if f.q != 0 {
                    d.set("Q", f.q);
                }
                match f.kind {
                    Kind::Multiline => d.set("Ff", 1 << 12),
                    Kind::Combo => d.set("Ff", 1 << 17),
                    _ => {}
                }
                if matches!(f.kind, Kind::Combo | Kind::List) {
                    d.set("Opt", Object::Array(CHOICES.iter().map(|c| Object::string_literal(*c)).collect()));
                }
                if spec.appearances {
                    let pad = 2.0 * k;
                    let mut ap = format!("/Tx BMC q {pad} {pad} {} {} re W n BT /Helv {size} Tf 0 g\n", w - 2.0 * pad, h - 2.0 * pad);
                    match f.kind {
                        Kind::Multiline => {
                            ap.push_str(&format!("{} TL {pad} {} Td\n", size * 1.15, h - pad - size * 0.9));
                            for line in f.value.split('\n') {
                                ap.push_str(&format!("({}) Tj T*\n", escape(line)));
                            }
                        }
                        Kind::List => {
                            ap.push_str(&format!("{} TL {pad} {} Td\n", size * 1.15, h - pad - size * 0.9));
                            for line in CHOICES {
                                ap.push_str(&format!("({}) Tj T*\n", escape(line)));
                            }
                        }
                        _ => {
                            // Helvetica advances are close to 0.5 em on average;
                            // exact enough to right-align the test value.
                            let tw = f.value.len() as f32 * size * 0.5;
                            let x = if f.q == 2 { w - pad - tw } else { pad };
                            ap.push_str(&format!("{x} {} Td ({}) Tj\n", baseline(h, size), escape(f.value)));
                        }
                    }
                    ap.push_str("ET Q EMC");
                    let n = form_xobject(&mut doc, w, h, ap, Some(font));
                    d.set("AP", dictionary! { "N" => n });
                }
            }
            Kind::Check => {
                d.set("FT", "Btn");
                let on = f.value != "Off";
                d.set("V", if on { "Yes" } else { "Off" });
                d.set("AS", if on { "Yes" } else { "Off" });
                d.set("MK", dictionary! { "CA" => Object::string_literal("8") });
                // A cross from corner to corner: easy to measure.
                let yes = form_xobject(
                    &mut doc, w, h,
                    format!("q 0 G {} w {} {} m {} {} l {} {} m {} {} l S Q", 1.5 * k, 3.0 * k, 3.0 * k, w - 3.0 * k, h - 3.0 * k, 3.0 * k, h - 3.0 * k, w - 3.0 * k, 3.0 * k),
                    None,
                );
                let off = form_xobject(&mut doc, w, h, String::new(), None);
                d.set("AP", dictionary! { "N" => dictionary! { "Yes" => yes, "Off" => off } });
            }
            Kind::Signature => {
                d.set("FT", "Sig");
            }
            Kind::Radio => unreachable!(),
        }
        let id = doc.add_object(d);
        annots.push(id.into());
        roots.push(id.into());
    }

    // Radio group with three widgets; the second one is on.
    let group = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    for (i, rr) in radio_rects().iter().enumerate() {
        frame(&mut content, *rr, if i == 0 { "choice" } else { "" }, 0.0);
        let r = place(*rr);
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let name = format!("opt{i}");
        let c = w / 2.0;
        let on = form_xobject(
            &mut doc, w, h,
            format!("q 0 g {} {} {} {} re f Q", c - w * 0.25, c - w * 0.25, w * 0.5, w * 0.5),
            None,
        );
        let off = form_xobject(&mut doc, w, h, String::new(), None);
        let mut n = Dictionary::new();
        n.set(name.as_bytes().to_vec(), Object::Reference(on));
        n.set("Off", Object::Reference(off));
        let id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => rect_obj(r),
            "P" => page_id,
            "F" => 4,
            "Parent" => group,
            "AS" => if i == 1 { name.as_str() } else { "Off" },
            "AP" => dictionary! { "N" => n },
        });
        annots.push(id.into());
        kids.push(id.into());
    }
    doc.objects.insert(group, Object::Dictionary(dictionary! {
        "FT" => "Btn",
        "Ff" => (1 << 15) | (1 << 14),
        "T" => Object::string_literal("choice"),
        "V" => "opt1",
        "Kids" => kids,
    }));
    roots.push(group.into());

    // A ruler along the left and bottom edge (ticks every 50 pt).
    content.push_str("0 G 0.3 w\n");
    let mut t = 0.0;
    while t <= PAGE_H {
        content.push_str(&format!("{} {} m {} {} l S\n", ox, oy + t * k, ox + 8.0 * k, oy + t * k));
        if t <= PAGE_W {
            content.push_str(&format!("{} {} m {} {} l S\n", ox + t * k, oy, ox + t * k, oy + 8.0 * k));
        }
        t += 50.0;
    }

    let content_id = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
    let media = [ox, oy, ox + PAGE_W * k, oy + PAGE_H * k];
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => rect_obj(media),
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "Helv" => font } },
        "Annots" => annots,
    };
    if spec.crop_inset > 0.0 {
        let c = spec.crop_inset * k;
        page.set("CropBox", rect_obj([media[0] + c, media[1] + c, media[2] - c, media[3] - c]));
    }
    if spec.rotate != 0 {
        page.set("Rotate", spec.rotate);
    }
    if spec.user_unit != 1.0 {
        page.set("UserUnit", num(spec.user_unit));
    }
    doc.objects.insert(page_id, Object::Dictionary(page));
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
    }));
    let mut acro = dictionary! {
        "Fields" => roots,
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
    };
    if !spec.appearances {
        acro.set("NeedAppearances", true);
    }
    let acro_id = doc.add_object(acro);
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id, "AcroForm" => acro_id });
    doc.trailer.set("Root", cat);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save form pdf");
    out
}

/// The named page variants the tests and the sample generator cover.
pub fn variants() -> Vec<(&'static str, PageSpec)> {
    let d = PageSpec::default();
    vec![
        ("plain", d),
        ("rot90", PageSpec { rotate: 90, ..d }),
        ("rot180", PageSpec { rotate: 180, ..d }),
        ("rot270", PageSpec { rotate: 270, ..d }),
        ("offset", PageSpec { origin: (100.0, 200.0), ..d }),
        ("cropped", PageSpec { origin: (100.0, 200.0), crop_inset: 12.0, ..d }),
        ("cropped-rot90", PageSpec { origin: (100.0, 200.0), crop_inset: 12.0, rotate: 90, ..d }),
        ("userunit", PageSpec { user_unit: 2.0, ..d }),
        ("no-ap", PageSpec { appearances: false, ..d }),
    ]
}
