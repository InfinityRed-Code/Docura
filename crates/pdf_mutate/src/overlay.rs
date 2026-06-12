//! Text overlay injection (watermarks, headers, footers, page numbers).
//!
//! Appends a content stream to each selected page that draws text on top of
//! existing content. Uses the 14 standard PDF fonts (Helvetica) with
//! WinAnsiEncoding so ASCII + Latin-1 (including German umlauts) renders
//! without embedded font data. Opacity handled via ExtGState `/ca`.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::{MutateError, MutateResult};

#[derive(Clone, Copy, Debug)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub struct WatermarkOptions {
    pub font_size: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
    pub gray: f32,
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            font_size: 72.0,
            rotation_deg: 45.0,
            opacity: 0.25,
            gray: 0.5,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HeaderFooterOptions {
    pub header_left: String,
    pub header_center: String,
    pub header_right: String,
    pub footer_left: String,
    pub footer_center: String,
    pub footer_right: String,
    pub font_size: f32,
    pub margin_pt: f32,
    pub first_page_number: u32,
    pub gray: f32,
}

#[derive(Clone, Copy, Debug)]
struct TextItem<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    angle_deg: f32,
    font_size: f32,
    align: HAlign,
}

pub(crate) struct OverlayContext {
    font_id: ObjectId,
    gstate_id: Option<ObjectId>,
    font_alias: &'static [u8],
    gs_alias: &'static [u8],
    gray: f32,
}

impl OverlayContext {
    fn new(doc: &mut Document, opacity: f32, gray: f32) -> Self {
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let gstate_id = if (opacity - 1.0).abs() > 0.001 {
            Some(doc.add_object(dictionary! {
                "Type" => "ExtGState",
                "ca" => Object::Real(opacity),
                "CA" => Object::Real(opacity),
            }))
        } else {
            None
        };
        Self {
            font_id,
            gstate_id,
            font_alias: b"FRPDF",
            gs_alias: b"GRPDF",
            gray: gray.clamp(0.0, 1.0),
        }
    }
}

pub(crate) fn ensure_own_resources(doc: &mut Document, page_id: ObjectId) -> MutateResult<ObjectId> {
    // Returns the ObjectId of a Resources dictionary owned by this page.
    // If Resources is a direct dict on the page, promote to an indirect object
    // so we can share updates via object id.
    let existing = doc
        .get_dictionary(page_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Resources")
        .ok()
        .cloned();

    match existing {
        Some(Object::Reference(rid)) => Ok(rid),
        Some(Object::Dictionary(d)) => {
            let rid = doc.add_object(Object::Dictionary(d));
            if let Ok(pd) = doc.get_dictionary_mut(page_id) {
                pd.set("Resources", Object::Reference(rid));
            }
            Ok(rid)
        }
        _ => {
            // No resources at all — inherit-or-empty. Start with empty.
            let rid = doc.add_object(Object::Dictionary(Dictionary::new()));
            if let Ok(pd) = doc.get_dictionary_mut(page_id) {
                pd.set("Resources", Object::Reference(rid));
            }
            Ok(rid)
        }
    }
}

pub(crate) fn register_alias(
    doc: &mut Document,
    resources_id: ObjectId,
    sub_key: &[u8],
    alias: &[u8],
    target: ObjectId,
) -> MutateResult<()> {
    // Make Resources[sub_key][alias] = target (indirect reference).
    let sub_obj = doc
        .get_dictionary(resources_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(sub_key)
        .ok()
        .cloned();

    match sub_obj {
        Some(Object::Reference(sid)) => {
            if let Ok(sd) = doc.get_dictionary_mut(sid) {
                sd.set(alias.to_vec(), Object::Reference(target));
            }
        }
        Some(Object::Dictionary(d)) => {
            let mut d = d;
            d.set(alias.to_vec(), Object::Reference(target));
            if let Ok(rd) = doc.get_dictionary_mut(resources_id) {
                rd.set(String::from_utf8_lossy(sub_key).to_string(), Object::Dictionary(d));
            }
        }
        _ => {
            let mut d = Dictionary::new();
            d.set(alias.to_vec(), Object::Reference(target));
            if let Ok(rd) = doc.get_dictionary_mut(resources_id) {
                rd.set(String::from_utf8_lossy(sub_key).to_string(), Object::Dictionary(d));
            }
        }
    }
    Ok(())
}

fn get_page_box(doc: &Document, page_id: ObjectId) -> (f32, f32) {
    // Returns (width, height) of the displayed page region in points.
    let vb = crate::resolve_view_box(doc, page_id);
    ((vb[2] - vb[0]).abs(), (vb[3] - vb[1]).abs())
}

/// Origin (lower-left) of the displayed page region in PDF user space.
/// Overlay content placed relative to the visible page must add this.
fn get_page_origin(doc: &Document, page_id: ObjectId) -> (f32, f32) {
    let vb = crate::resolve_view_box(doc, page_id);
    (vb[0], vb[1])
}

fn page_rotation(doc: &Document, page_id: ObjectId) -> i32 {
    let mut cur = page_id;
    loop {
        if let Ok(d) = doc.get_dictionary(cur) {
            if let Ok(r) = d.get(b"Rotate").and_then(|o| o.as_i64()) {
                return ((r % 360 + 360) % 360) as i32;
            }
            if let Ok(parent) = d.get(b"Parent").and_then(|o| o.as_reference()) {
                cur = parent;
                continue;
            }
        }
        break;
    }
    0
}

// Helvetica character widths (in 1/1000 em units) for the WinAnsi range we use.
// Approximate via a compact table; missing chars fall back to 500.
fn helvetica_width(byte: u8) -> u16 {
    // Source: Adobe Helvetica AFM metrics, WinAnsiEncoding subset.
    const T: [(u8, u16); 96] = [
        (0x20, 278), (0x21, 278), (0x22, 355), (0x23, 556), (0x24, 556), (0x25, 889),
        (0x26, 667), (0x27, 191), (0x28, 333), (0x29, 333), (0x2A, 389), (0x2B, 584),
        (0x2C, 278), (0x2D, 333), (0x2E, 278), (0x2F, 278), (0x30, 556), (0x31, 556),
        (0x32, 556), (0x33, 556), (0x34, 556), (0x35, 556), (0x36, 556), (0x37, 556),
        (0x38, 556), (0x39, 556), (0x3A, 278), (0x3B, 278), (0x3C, 584), (0x3D, 584),
        (0x3E, 584), (0x3F, 556), (0x40, 1015), (0x41, 667), (0x42, 667), (0x43, 722),
        (0x44, 722), (0x45, 667), (0x46, 611), (0x47, 778), (0x48, 722), (0x49, 278),
        (0x4A, 500), (0x4B, 667), (0x4C, 556), (0x4D, 833), (0x4E, 722), (0x4F, 778),
        (0x50, 667), (0x51, 778), (0x52, 722), (0x53, 667), (0x54, 611), (0x55, 722),
        (0x56, 667), (0x57, 944), (0x58, 667), (0x59, 667), (0x5A, 611), (0x5B, 278),
        (0x5C, 278), (0x5D, 278), (0x5E, 469), (0x5F, 556), (0x60, 333), (0x61, 556),
        (0x62, 556), (0x63, 500), (0x64, 556), (0x65, 556), (0x66, 278), (0x67, 556),
        (0x68, 556), (0x69, 222), (0x6A, 222), (0x6B, 500), (0x6C, 222), (0x6D, 833),
        (0x6E, 556), (0x6F, 556), (0x70, 556), (0x71, 556), (0x72, 333), (0x73, 500),
        (0x74, 278), (0x75, 556), (0x76, 500), (0x77, 722), (0x78, 500), (0x79, 500),
        (0x7A, 500), (0x7B, 334), (0x7C, 260), (0x7D, 334), (0x7E, 584), (0x7F, 500),
    ];
    if (0x20..=0x7F).contains(&byte) {
        T[(byte - 0x20) as usize].1
    } else if byte >= 0xA0 {
        // Latin-1 upper half: approximate with average widths.
        match byte {
            0xC4 | 0xD6 | 0xDC => 667, // Ä Ö Ü
            0xE4 | 0xF6 | 0xFC => 556, // ä ö ü
            0xDF => 611,               // ß
            _ => 556,
        }
    } else {
        500
    }
}

pub(crate) fn text_width_pt(bytes: &[u8], font_size: f32) -> f32 {
    let sum: u32 = bytes.iter().map(|b| helvetica_width(*b) as u32).sum();
    (sum as f32) * font_size / 1000.0
}

pub(crate) fn to_winansi_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let cp = c as u32;
        if cp <= 0x7F {
            out.push(cp as u8);
        } else if (0xA0..=0xFF).contains(&cp) {
            out.push(cp as u8);
        } else {
            let b: u8 = match c {
                '€' => 0x80, '‚' => 0x82, 'ƒ' => 0x83, '„' => 0x84,
                '…' => 0x85, '†' => 0x86, '‡' => 0x87, 'ˆ' => 0x88,
                '‰' => 0x89, 'Š' => 0x8A, '‹' => 0x8B, 'Œ' => 0x8C,
                'Ž' => 0x8E, '\u{2018}' => 0x91, '\u{2019}' => 0x92,
                '\u{201C}' => 0x93, '\u{201D}' => 0x94, '•' => 0x95,
                '–' => 0x96, '—' => 0x97, '˜' => 0x98, '™' => 0x99,
                'š' => 0x9A, '›' => 0x9B, 'œ' => 0x9C, 'ž' => 0x9E,
                'Ÿ' => 0x9F,
                _ => b'?',
            };
            out.push(b);
        }
    }
    out
}

pub(crate) fn escape_pdf_string(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 4);
    out.push(b'(');
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            _ => out.push(b),
        }
    }
    out.push(b')');
    out
}

fn build_page_stream(items: &[TextItem<'_>], ctx: &OverlayContext) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"q\n");
    if ctx.gstate_id.is_some() {
        buf.extend_from_slice(b"/");
        buf.extend_from_slice(ctx.gs_alias);
        buf.extend_from_slice(b" gs\n");
    }
    let _ = write!(&mut buf, "{:.3} g\n", ctx.gray);
    buf.extend_from_slice(b"BT\n");
    buf.extend_from_slice(b"/");
    buf.extend_from_slice(ctx.font_alias);
    buf.extend_from_slice(b" 12 Tf\n"); // placeholder; real size set per item
    for it in items {
        let bytes = to_winansi_bytes(it.text);
        if bytes.is_empty() {
            continue;
        }
        let w = text_width_pt(&bytes, it.font_size);
        let dx = match it.align {
            HAlign::Left => 0.0,
            HAlign::Center => -w / 2.0,
            HAlign::Right => -w,
        };
        let theta = it.angle_deg.to_radians();
        let (s, c) = (theta.sin(), theta.cos());
        // Tm: [a b c d e f] where rotation then translation.
        let a = c;
        let b = s;
        let cc = -s;
        let d = c;
        // Align offset applied along rotated x axis.
        let e = it.x + dx * c;
        let f = it.y + dx * s;
        let _ = write!(
            &mut buf,
            "/{} {:.3} Tf\n{:.5} {:.5} {:.5} {:.5} {:.3} {:.3} Tm\n",
            std::str::from_utf8(ctx.font_alias).unwrap_or("F"),
            it.font_size,
            a,
            b,
            cc,
            d,
            e,
            f,
        );
        let esc = escape_pdf_string(&bytes);
        buf.extend_from_slice(&esc);
        buf.extend_from_slice(b" Tj\n");
    }
    buf.extend_from_slice(b"ET\nQ\n");
    buf
}

// Use std::io::Write for format! on Vec<u8>.
use std::io::Write;

pub(crate) fn append_content_stream(
    doc: &mut Document,
    page_id: ObjectId,
    content: Vec<u8>,
) -> MutateResult<()> {
    let stream_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content)));
    // /Contents may be: missing, a Stream (direct or indirect), an array.
    let current = doc
        .get_dictionary(page_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Contents")
        .ok()
        .cloned();

    let new_contents = match current {
        None => Object::Array(vec![Object::Reference(stream_id)]),
        Some(Object::Reference(r)) => {
            Object::Array(vec![Object::Reference(r), Object::Reference(stream_id)])
        }
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(stream_id));
            Object::Array(arr)
        }
        Some(other) => {
            // Inline stream — promote to indirect.
            let promoted = doc.add_object(other);
            Object::Array(vec![Object::Reference(promoted), Object::Reference(stream_id)])
        }
    };

    if let Ok(pd) = doc.get_dictionary_mut(page_id) {
        pd.set("Contents", new_contents);
    }
    Ok(())
}

fn register_resources(doc: &mut Document, page_id: ObjectId, ctx: &OverlayContext) -> MutateResult<()> {
    let rid = ensure_own_resources(doc, page_id)?;
    register_alias(doc, rid, b"Font", ctx.font_alias, ctx.font_id)?;
    if let Some(gs) = ctx.gstate_id {
        register_alias(doc, rid, b"ExtGState", ctx.gs_alias, gs)?;
    }
    Ok(())
}

pub fn add_text_watermark(
    doc: &mut Document,
    text: &str,
    opts: &WatermarkOptions,
) -> MutateResult<()> {
    if text.is_empty() {
        return Err(MutateError::Parse("watermark: empty text".into()));
    }
    let ctx = OverlayContext::new(doc, opts.opacity, opts.gray);
    let pages: Vec<ObjectId> = doc.page_iter().collect();
    for page_id in pages {
        let (w, h) = get_page_box(doc, page_id);
        let (ox, oy) = get_page_origin(doc, page_id);
        let item = TextItem {
            text,
            x: ox + w / 2.0,
            y: oy + h / 2.0,
            angle_deg: opts.rotation_deg,
            font_size: opts.font_size,
            align: HAlign::Center,
        };
        let stream = build_page_stream(&[item], &ctx);
        register_resources(doc, page_id, &ctx)?;
        append_content_stream(doc, page_id, stream)?;
    }
    Ok(())
}

fn render_slot(template: &str, page_num: u32, total: u32) -> String {
    template
        .replace("{page}", &page_num.to_string())
        .replace("{total}", &total.to_string())
}

pub fn add_header_footer(
    doc: &mut Document,
    opts: &HeaderFooterOptions,
) -> MutateResult<()> {
    let has_any = !(opts.header_left.is_empty()
        && opts.header_center.is_empty()
        && opts.header_right.is_empty()
        && opts.footer_left.is_empty()
        && opts.footer_center.is_empty()
        && opts.footer_right.is_empty());
    if !has_any {
        return Err(MutateError::Parse("header_footer: all slots empty".into()));
    }
    let ctx = OverlayContext::new(doc, 1.0, opts.gray);
    let pages: Vec<ObjectId> = doc.page_iter().collect();
    let total = pages.len() as u32;
    let first_num = opts.first_page_number.max(1);
    let margin = if opts.margin_pt > 0.0 { opts.margin_pt } else { 24.0 };
    let fs = if opts.font_size > 0.0 { opts.font_size } else { 10.0 };
    for (idx, page_id) in pages.into_iter().enumerate() {
        let (w, h) = get_page_box(doc, page_id);
        let (ox, oy) = get_page_origin(doc, page_id);
        let rot = page_rotation(doc, page_id);
        // Place overlay in the page's unrotated coordinate space (MediaBox);
        // rotation is applied by viewers to the full page contents.
        let _ = rot;
        let page_num = first_num + idx as u32;
        let mut items_owned: Vec<(String, HAlign, f32, f32)> = Vec::new();
        let hy = oy + h - margin;
        let fy = oy + margin;
        let slots: [(&str, HAlign, f32, f32); 6] = [
            (&opts.header_left, HAlign::Left, ox + margin, hy),
            (&opts.header_center, HAlign::Center, ox + w / 2.0, hy),
            (&opts.header_right, HAlign::Right, ox + w - margin, hy),
            (&opts.footer_left, HAlign::Left, ox + margin, fy),
            (&opts.footer_center, HAlign::Center, ox + w / 2.0, fy),
            (&opts.footer_right, HAlign::Right, ox + w - margin, fy),
        ];
        for (tpl, align, x, y) in slots {
            if tpl.is_empty() {
                continue;
            }
            let s = render_slot(tpl, page_num, total);
            items_owned.push((s, align, x, y));
        }
        let items: Vec<TextItem<'_>> = items_owned
            .iter()
            .map(|(s, align, x, y)| TextItem {
                text: s.as_str(),
                x: *x,
                y: *y,
                angle_deg: 0.0,
                font_size: fs,
                align: *align,
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let stream = build_page_stream(&items, &ctx);
        register_resources(doc, page_id, &ctx)?;
        append_content_stream(doc, page_id, stream)?;
    }
    Ok(())
}

// ----------------- Image watermark -----------------

#[derive(Clone, Copy, Debug)]
pub enum ImageAnchor {
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug)]
pub struct ImageWatermarkOptions {
    pub scale_frac: f32,      // fraction of page width, 0.0-1.0
    pub rotation_deg: f32,
    pub opacity: f32,
    pub anchor: ImageAnchor,
    pub margin_pt: f32,       // inset from anchor edge (ignored for Center)
}

impl Default for ImageWatermarkOptions {
    fn default() -> Self {
        Self {
            scale_frac: 0.4,
            rotation_deg: 0.0,
            opacity: 0.5,
            anchor: ImageAnchor::Center,
            margin_pt: 36.0,
        }
    }
}

fn prepare_image_jpeg(bytes: &[u8]) -> MutateResult<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| MutateError::Parse(format!("image decode: {e}")))?;
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());
    let mut jpeg: Vec<u8> = Vec::with_capacity((w * h) as usize);
    {
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
        encoder
            .encode(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| MutateError::Parse(format!("jpeg encode: {e}")))?;
    }
    Ok((jpeg, w, h))
}

pub fn add_image_watermark(
    doc: &mut Document,
    image_bytes: &[u8],
    opts: &ImageWatermarkOptions,
) -> MutateResult<()> {
    if image_bytes.is_empty() {
        return Err(MutateError::Parse("image_watermark: empty input".into()));
    }
    let (jpeg, iw, ih) = prepare_image_jpeg(image_bytes)?;
    let image_dict = lopdf::dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => iw as i64,
        "Height" => ih as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        "Filter" => "DCTDecode",
    };
    let image_id = doc.add_object(Object::Stream(Stream::new(image_dict, jpeg)));

    let gstate_id = if (opts.opacity - 1.0).abs() > 0.001 {
        Some(doc.add_object(dictionary! {
            "Type" => "ExtGState",
            "ca" => Object::Real(opts.opacity.clamp(0.0, 1.0)),
            "CA" => Object::Real(opts.opacity.clamp(0.0, 1.0)),
        }))
    } else {
        None
    };

    let img_alias: &[u8] = b"ImRPDF";
    let gs_alias: &[u8] = b"GIRPDF";
    let scale = opts.scale_frac.clamp(0.01, 2.0);
    let margin = opts.margin_pt.max(0.0);

    let pages: Vec<ObjectId> = doc.page_iter().collect();
    for page_id in pages {
        let (pw, ph) = get_page_box(doc, page_id);
        let (ox, oy) = get_page_origin(doc, page_id);
        let target_w = pw * scale;
        let target_h = target_w * (ih as f32) / (iw as f32);

        let (cx, cy) = match opts.anchor {
            ImageAnchor::Center => (ox + pw / 2.0, oy + ph / 2.0),
            ImageAnchor::TopLeft => (ox + margin + target_w / 2.0, oy + ph - margin - target_h / 2.0),
            ImageAnchor::TopRight => (ox + pw - margin - target_w / 2.0, oy + ph - margin - target_h / 2.0),
            ImageAnchor::BottomLeft => (ox + margin + target_w / 2.0, oy + margin + target_h / 2.0),
            ImageAnchor::BottomRight => (ox + pw - margin - target_w / 2.0, oy + margin + target_h / 2.0),
        };

        let theta = opts.rotation_deg.to_radians();
        let (s, c) = (theta.sin(), theta.cos());
        // Image XObject draws into unit square (0..1, 0..1). cm maps that to
        // a rotated rectangle centered at (cx, cy) with size (target_w, target_h).
        let a = target_w * c;
        let b = target_w * s;
        let cc = -target_h * s;
        let d = target_h * c;
        let e = cx - (a + cc) * 0.5;
        let f = cy - (b + d) * 0.5;

        let mut stream: Vec<u8> = Vec::with_capacity(128);
        stream.extend_from_slice(b"q\n");
        if gstate_id.is_some() {
            stream.extend_from_slice(b"/");
            stream.extend_from_slice(gs_alias);
            stream.extend_from_slice(b" gs\n");
        }
        let _ = write!(
            &mut stream,
            "{:.5} {:.5} {:.5} {:.5} {:.3} {:.3} cm\n/{} Do\nQ\n",
            a, b, cc, d, e, f,
            std::str::from_utf8(img_alias).unwrap_or("Im"),
        );

        let rid = ensure_own_resources(doc, page_id)?;
        register_alias(doc, rid, b"XObject", img_alias, image_id)?;
        if let Some(gs) = gstate_id {
            register_alias(doc, rid, b"ExtGState", gs_alias, gs)?;
        }
        append_content_stream(doc, page_id, stream)?;
    }
    Ok(())
}
