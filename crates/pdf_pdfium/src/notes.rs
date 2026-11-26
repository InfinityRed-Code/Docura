//! Sticky-note icons (Text annotations).
//!
//! A text annotation behaves as if its NoRotate flag were always set
//! (ISO 32000-1 §12.5.6.4): the upper-left corner of its /Rect stays pinned
//! to the page and the icon is drawn upright from there, whatever the page's
//! /Rotate and the view rotation are. PDFium ignores that (and the explicit
//! NoRotate flag): it turns the icon with the page and lets it grow from the
//! turned rectangle, so on a page with /Rotate 90 the icon lies on its side,
//! left of the pinned corner.
//!
//! The renderer therefore hides the text annotations while PDFium draws the
//! page and paints the icons itself afterwards: a one-page icon PDF, drawn
//! by PDFium onto the same bitmap at the pinned corner. This happens for
//! every rotation, so a note looks the same on every page. Like Acrobat, the
//! icon is chosen by /Name and coloured by /C; an appearance stream of the
//! annotation is not used. Nothing of this reaches the file: the hidden flag
//! is taken back as soon as the page is drawn.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::os::raw::{c_int, c_uint};

use pdfium_render::prelude::*;

use crate::LoadedDoc;

const SUBTYPE_TEXT: c_int = 1;
const FLAG_HIDDEN: c_int = 1 << 1;
const FLAG_NO_VIEW: c_int = 1 << 5;
/// Side of the icon drawing in its own page units.
const ICON_BOX: f32 = 20.0;
/// Acrobat's default note colour, for a note without a usable /C.
const DEFAULT_COLOR: [u8; 3] = [255, 209, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Icon {
    Note,
    Comment,
    Insert,
}

impl Icon {
    /// The drawing for a /Name. The standard names without a drawing of
    /// their own (Key, Help, NewParagraph, Paragraph) and unknown ones show
    /// the note sheet, the default of the specification.
    fn from_name(name: &str) -> Icon {
        match name {
            "Comment" => Icon::Comment,
            "Insert" => Icon::Insert,
            _ => Icon::Note,
        }
    }
}

type IconKey = (Icon, [u8; 3], u8);

/// Icon pages by drawing, colour and opacity, shared by all documents.
#[derive(Default)]
pub(crate) struct IconCache {
    pages: HashMap<IconKey, LoadedDoc>,
}

impl IconCache {
    fn get(&mut self, key: IconKey) -> Option<&PdfDocument<'static>> {
        // Colours are picked freely; a cache that has grown this far is
        // simply rebuilt (an icon page is a few hundred bytes).
        if self.pages.len() > 256 && !self.pages.contains_key(&key) {
            self.pages.clear();
        }
        let page = match self.pages.entry(key) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(LoadedDoc::load(icon_pdf(key.0, key.1, key.2), None).ok()?),
        };
        page.doc.as_ref()
    }
}

/// /C of the text annotations PDFium does not report a colour for (those
/// with an appearance stream), by page and position in /Annots. Read from
/// the file once, when the first such note is drawn.
pub(crate) type NoteColors = HashMap<(u32, u32), [u8; 3]>;

pub(crate) fn read_note_colors(bytes: &[u8]) -> NoteColors {
    let mut out = NoteColors::new();
    // Encrypted files do not load here; their notes keep the default colour.
    let Ok(doc) = lopdf::Document::load_mem(bytes) else { return out };
    let resolve = |o: &lopdf::Object| match o {
        lopdf::Object::Reference(r) => doc.get_object(*r).ok().cloned(),
        other => Some(other.clone()),
    };
    let num = |o: &lopdf::Object| match o {
        lopdf::Object::Integer(i) => Some(*i as f32),
        lopdf::Object::Real(r) => Some(*r),
        _ => None,
    };
    for (page_idx, page_id) in doc.page_iter().enumerate() {
        let Some(annots) = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|p| p.get(b"Annots").ok())
            .and_then(resolve)
        else {
            continue;
        };
        let Ok(annots) = annots.as_array() else { continue };
        for (i, entry) in annots.iter().enumerate() {
            let Some(lopdf::Object::Dictionary(d)) = resolve(entry) else { continue };
            if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Text".as_slice()) {
                continue;
            }
            let Some(c) = d.get(b"C").ok().and_then(resolve) else { continue };
            let Ok(c) = c.as_array() else { continue };
            let v: Vec<f32> = c.iter().filter_map(num).collect();
            let rgb = match v.as_slice() {
                [g] => [*g, *g, *g],
                [r, g, b] => [*r, *g, *b],
                [c, m, y, k] => [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)],
                _ => continue,
            };
            out.insert((page_idx as u32, i as u32), rgb.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    out
}

struct Note {
    /// Upper-left corner of /Rect in PDF user space: the pinned point.
    pin: (f32, f32),
    /// Side of the icon in points.
    side: f32,
    key: IconKey,
}

/// The visible text annotations of a page, hidden from PDFium for as long
/// as this value lives.
pub(crate) struct HiddenNotes<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    hidden: Vec<(FPDF_ANNOTATION, c_int)>,
    notes: Vec<Note>,
}

impl<'a> HiddenNotes<'a> {
    /// Hide the text annotations of `page`. `colors` supplies the colours
    /// PDFium does not report; it is only called when one is needed.
    pub(crate) fn hide<'c>(
        bindings: &'a dyn PdfiumLibraryBindings,
        page: FPDF_PAGE,
        page_index: u32,
        colors: impl Fn() -> &'c NoteColors,
    ) -> Self {
        let mut out = HiddenNotes { bindings, hidden: Vec::new(), notes: Vec::new() };        for i in 0..bindings.FPDFPage_GetAnnotCount(page).max(0) {
            let annot = bindings.FPDFPage_GetAnnot(page, i);
            if annot.is_null() {
                continue;
            }
            let flags = bindings.FPDFAnnot_GetFlags(annot);
            if bindings.FPDFAnnot_GetSubtype(annot) != SUBTYPE_TEXT
                || flags & (FLAG_HIDDEN | FLAG_NO_VIEW) != 0
            {
                bindings.FPDFPage_CloseAnnot(annot);
                continue;
            }
            let mut rect = FS_RECTF { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };
            let has_rect = bindings.is_true(bindings.FPDFAnnot_GetRect(annot, &mut rect));
            let (w, h) = ((rect.right - rect.left).abs(), (rect.top - rect.bottom).abs());
            if has_rect && w > 0.0 && h > 0.0 {
                let color = annot_color(bindings, annot)
                    .or_else(|| colors().get(&(page_index, i as u32)).copied())
                    .unwrap_or(DEFAULT_COLOR);
                let mut ca = 1.0f32;
                if !bindings.is_true(bindings.FPDFAnnot_GetNumberValue(annot, "CA", &mut ca)) {
                    ca = 1.0;
                }
                let alpha = (ca.clamp(0.0, 1.0) * 255.0).round() as u8;
                out.notes.push(Note {
                    pin: (rect.left.min(rect.right), rect.top.max(rect.bottom)),
                    side: w.min(h),
                    key: (Icon::from_name(&annot_name(bindings, annot)), color, alpha),
                });
            }
            // Hidden also keeps PDFium from generating an appearance (and
            // from rewriting /Rect) for the annotation in its copy.
            bindings.FPDFAnnot_SetFlags(annot, flags | FLAG_HIDDEN);
            out.hidden.push((annot, flags));
        }
        out
    }

    /// Paint the icons onto `bitmap`, which holds the page as drawn by
    /// `FPDF_RenderPageBitmap(bitmap, page, start_x, start_y, size_x,
    /// size_y, rotate, …)` at `zoom` pixels per point. `reverse_bytes`: the
    /// page was drawn with FPDF_REVERSE_BYTE_ORDER (RGB instead of BGR).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &self,
        icons: &mut IconCache,
        bitmap: FPDF_BITMAP,
        page: FPDF_PAGE,
        (start_x, start_y): (c_int, c_int),
        (size_x, size_y): (c_int, c_int),
        rotate: c_int,
        zoom: f32,
        reverse_bytes: bool,
    ) {
        const FPDF_REVERSE_BYTE_ORDER: c_int = 0x10;
        let flags = if reverse_bytes { FPDF_REVERSE_BYTE_ORDER } else { 0 };
        let b = self.bindings;
        let (bw, bh) = (b.FPDFBitmap_GetWidth(bitmap), b.FPDFBitmap_GetHeight(bitmap));
        for note in &self.notes {
            let (mut x, mut y) = (0, 0);
            b.FPDF_PageToDevice(
                page, start_x, start_y, size_x, size_y, rotate,
                note.pin.0 as f64, note.pin.1 as f64, &mut x, &mut y,
            );
            let side = ((note.side * zoom).round() as c_int).max(1);
            if x >= bw || y >= bh || x + side <= 0 || y + side <= 0 {
                continue;
            }
            let Some(doc) = icons.get(note.key) else { continue };
            let Ok(icon_page) = doc.pages().get(0) else { continue };
            b.FPDF_RenderPageBitmap(bitmap, b.get_handle_from_page(&icon_page), x, y, side, side, 0, flags);
        }
    }
}

impl Drop for HiddenNotes<'_> {
    fn drop(&mut self) {
        for (annot, flags) in self.hidden.drain(..) {
            self.bindings.FPDFAnnot_SetFlags(annot, flags);
            self.bindings.FPDFPage_CloseAnnot(annot);
        }
    }
}

/// /C as PDFium reports it. It does not for an annotation that has an
/// appearance stream.
fn annot_color(bindings: &dyn PdfiumLibraryBindings, annot: FPDF_ANNOTATION) -> Option<[u8; 3]> {
    if !bindings.is_true(bindings.FPDFAnnot_HasKey(annot, "C")) {
        return None;
    }
    let (mut r, mut g, mut b, mut a): (c_uint, c_uint, c_uint, c_uint) = (0, 0, 0, 0);
    // 0 = FPDFANNOT_COLORTYPE_Color (/C).
    bindings
        .is_true(bindings.FPDFAnnot_GetColor(annot, 0, &mut r, &mut g, &mut b, &mut a))
        .then(|| [r.min(255) as u8, g.min(255) as u8, b.min(255) as u8])
}

/// /Name of the annotation ("" when it has none).
fn annot_name(bindings: &dyn PdfiumLibraryBindings, annot: FPDF_ANNOTATION) -> String {
    let mut buf = [0u16; 32];
    let bytes = bindings.FPDFAnnot_GetStringValue(
        annot,
        "Name",
        buf.as_mut_ptr() as *mut FPDF_WCHAR,
        std::mem::size_of_val(&buf) as _,
    ) as usize;
    // Byte length including the terminator; a longer name is none of ours.
    if bytes < 2 || bytes > std::mem::size_of_val(&buf) {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..bytes / 2 - 1])
}

/// Content stream of an icon in a 20 x 20 box (PDF space, y up).
fn icon_content(icon: Icon, rgb: [u8; 3], alpha: u8) -> String {
    let c = |v: u8, f: f32| format!("{:.3}", v as f32 / 255.0 * f);
    let fill = format!("{} {} {} rg", c(rgb[0], 1.0), c(rgb[1], 1.0), c(rgb[2], 1.0));
    // Outline and details: the same hue, darker.
    let dark = format!("{} {} {}", c(rgb[0], 0.4), c(rgb[1], 0.4), c(rgb[2], 0.4));
    let gs = if alpha < 255 { "/G gs\n" } else { "" };
    let shape = match icon {
        // A sheet with its lower right corner folded and three lines.
        Icon::Note => {
            "2.5 17.5 m 17.5 17.5 l 17.5 7.5 l 12.5 2.5 l 2.5 2.5 l h B\n\
             17.5 7.5 m 12.5 7.5 l 12.5 2.5 l S\n\
             5.5 14 m 14.5 14 l 5.5 11 m 14.5 11 l 5.5 8 m 10 8 l S\n"
        }
        // A speech bubble with its tail at the lower left.
        Icon::Comment => {
            "5 17.5 m 15 17.5 l 16.4 17.5 17.5 16.4 17.5 15 c 17.5 9.5 l \
             17.5 8.1 16.4 7 15 7 c 9.5 7 l 4.5 2.5 l 5.5 7 l 5 7 l \
             3.6 7 2.5 8.1 2.5 9.5 c 2.5 15 l 2.5 16.4 3.6 17.5 5 17.5 c h B\n\
             5.5 14 m 14.5 14 l 5.5 10.5 m 12 10.5 l S\n"
        }
        // An insertion caret, pointing up.
        Icon::Insert => "10 17.5 m 17 2.5 l 3 2.5 l h B\n",
    };
    format!("{gs}1 w 1 j 1 J\n{fill}\n{dark} RG\n{shape}")
}

/// A one-page PDF showing the icon.
fn icon_pdf(icon: Icon, rgb: [u8; 3], alpha: u8) -> Vec<u8> {
    let content = icon_content(icon, rgb, alpha);
    let a = alpha as f32 / 255.0;
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {ICON_BOX} {ICON_BOX}] \
             /Resources << /ExtGState << /G << /ca {a:.3} /CA {a:.3} >> >> >> /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for o in offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}
