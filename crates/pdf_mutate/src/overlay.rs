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
    /// Unique per call: a page watermarked twice with different opacities
    /// must not have the second ExtGState replace the first one's.
    gs_alias: Vec<u8>,
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
            gs_alias: format!("GRPDF{}", gstate_id.map_or(0, |id| id.0)).into_bytes(),
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
            // No own /Resources: the page inherits them from a /Pages
            // ancestor. Start from a copy of those — an empty dictionary on
            // the page would shadow them and drop every font/image it uses.
            let inherited = match crate::inherited_attr(doc, page_id, b"Resources") {
                Some(Object::Dictionary(d)) => d,
                Some(Object::Reference(r)) => doc.get_dictionary(r).cloned().unwrap_or_default(),
                _ => Dictionary::new(),
            };
            let rid = doc.add_object(Object::Dictionary(inherited));
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

/// The page as displayed: the view box (CropBox ∩ MediaBox, possibly not at
/// the origin) with its persistent /Rotate applied. Overlays are laid out in
/// this top-origin space — so a header sits at the top of what the reader
/// sees, also on rotated (e.g. landscape-scanned) pages — and then mapped
/// into PDF user space with `to_pdf`, turning text by `rot` to stay upright.
struct PageLayout {
    vb: [f32; 4],
    rot: i32,
    /// Displayed width/height in points.
    w: f32,
    h: f32,
}

impl PageLayout {
    fn of(doc: &Document, page_id: ObjectId) -> Self {
        let vb = crate::resolve_view_box(doc, page_id);
        let rot = crate::page_rotation(doc, page_id);
        let (bw, bh) = (vb[2] - vb[0], vb[3] - vb[1]);
        let (w, h) = if rot == 90 || rot == 270 { (bh, bw) } else { (bw, bh) };
        Self { vb, rot, w, h }
    }

    fn to_pdf(&self, x: f32, y: f32) -> (f32, f32) {
        crate::point_to_pdf_space(x, y, self.vb, self.rot)
    }
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
        buf.extend_from_slice(&ctx.gs_alias);
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
    // /Contents may be: missing, a Stream (direct or indirect), an array.
    let current = doc
        .get_dictionary(page_id)
        .map_err(|e| MutateError::Parse(e.to_string()))?
        .get(b"Contents")
        .ok()
        .cloned();

    let new_contents = match current {
        None => {
            let stream_id =
                doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content)));
            Object::Array(vec![Object::Reference(stream_id)])
        }
        Some(existing) => {
            // Isolate the existing page content's graphics state so the
            // appended content draws in the page's default user space. A
            // content stream that leaves a non-identity CTM active (e.g.
            // Canva exports that never restore a page-level `cm`) would
            // otherwise transform the appended text/overlay far off-page —
            // making OCR text unsearchable and unselectable. Wrap the prior
            // content in `q`/`Q`: the leading save runs before it, the
            // matching restore prefixes our own stream.
            let save_id =
                doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"q\n".to_vec())));
            let mut restored = Vec::with_capacity(content.len() + 2);
            restored.extend_from_slice(b"Q\n");
            restored.extend_from_slice(&content);
            let stream_id =
                doc.add_object(Object::Stream(Stream::new(Dictionary::new(), restored)));
            let mut arr = vec![Object::Reference(save_id)];
            match existing {
                // /Contents may be an indirect *array* of streams; splice its
                // entries — an array nested in the array is not a stream and
                // viewers would drop the original page content.
                Object::Reference(r) => match doc.get_object(r) {
                    Ok(Object::Array(a)) => arr.extend(a.iter().cloned()),
                    _ => arr.push(Object::Reference(r)),
                },
                Object::Array(a) => arr.extend(a),
                other => {
                    // Inline stream — promote to indirect.
                    let promoted = doc.add_object(other);
                    arr.push(Object::Reference(promoted));
                }
            }
            arr.push(Object::Reference(stream_id));
            Object::Array(arr)
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
        register_alias(doc, rid, b"ExtGState", &ctx.gs_alias, gs)?;
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
        let pl = PageLayout::of(doc, page_id);
        let (x, y) = pl.to_pdf(pl.w / 2.0, pl.h / 2.0);
        let item = TextItem {
            text,
            x,
            y,
            angle_deg: opts.rotation_deg + pl.rot as f32,
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
        // Baselines in the displayed page (top-origin): header `margin` below
        // the top edge, footer `margin` above the bottom edge.
        let pl = PageLayout::of(doc, page_id);
        let (w, h) = (pl.w, pl.h);
        let page_num = first_num + idx as u32;
        let mut items_owned: Vec<(String, HAlign, f32, f32)> = Vec::new();
        let hy = margin;
        let fy = h - margin;
        let slots: [(&str, HAlign, f32, f32); 6] = [
            (&opts.header_left, HAlign::Left, margin, hy),
            (&opts.header_center, HAlign::Center, w / 2.0, hy),
            (&opts.header_right, HAlign::Right, w - margin, hy),
            (&opts.footer_left, HAlign::Left, margin, fy),
            (&opts.footer_center, HAlign::Center, w / 2.0, fy),
            (&opts.footer_right, HAlign::Right, w - margin, fy),
        ];
        for (tpl, align, x, y) in slots {
            if tpl.is_empty() {
                continue;
            }
            let s = render_slot(tpl, page_num, total);
            let (px, py) = pl.to_pdf(x, y);
            items_owned.push((s, align, px, py));
        }
        let items: Vec<TextItem<'_>> = items_owned
            .iter()
            .map(|(s, align, x, y)| TextItem {
                text: s.as_str(),
                x: *x,
                y: *y,
                angle_deg: pl.rot as f32,
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

/// Embed the watermark image as an image XObject; returns (id, width, height).
/// Images with an alpha channel keep their transparency through an /SMask —
/// flattening to RGB would paint a transparent logo's background (usually
/// black) as an opaque box. Opaque images are stored as JPEG.
fn embed_watermark_image(doc: &mut Document, bytes: &[u8]) -> MutateResult<(ObjectId, u32, u32)> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| MutateError::Parse(format!("image decode: {e}")))?;
    let (w, h) = (img.width(), img.height());
    if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let n = (w as usize) * (h as usize);
        let mut rgb: Vec<u8> = Vec::with_capacity(n * 3);
        let mut alpha: Vec<u8> = Vec::with_capacity(n);
        for px in rgba.pixels() {
            rgb.extend_from_slice(&[px[0], px[1], px[2]]);
            alpha.push(px[3]);
        }
        let mut smask = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => w as i64,
                "Height" => h as i64,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            alpha,
        );
        let _ = smask.compress();
        let smask_id = doc.add_object(Object::Stream(smask));
        let mut img_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => w as i64,
                "Height" => h as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "SMask" => Object::Reference(smask_id),
            },
            rgb,
        );
        let _ = img_stream.compress();
        return Ok((doc.add_object(Object::Stream(img_stream)), w, h));
    }
    let rgb = img.to_rgb8();
    let mut jpeg: Vec<u8> = Vec::with_capacity((w as usize) * (h as usize));
    {
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
        encoder
            .encode(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| MutateError::Parse(format!("jpeg encode: {e}")))?;
    }
    let image_dict = lopdf::dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => w as i64,
        "Height" => h as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        "Filter" => "DCTDecode",
    };
    Ok((doc.add_object(Object::Stream(Stream::new(image_dict, jpeg))), w, h))
}

pub fn add_image_watermark(
    doc: &mut Document,
    image_bytes: &[u8],
    opts: &ImageWatermarkOptions,
) -> MutateResult<()> {
    if image_bytes.is_empty() {
        return Err(MutateError::Parse("image_watermark: empty input".into()));
    }
    let (image_id, iw, ih) = embed_watermark_image(doc, image_bytes)?;

    let gstate_id = if (opts.opacity - 1.0).abs() > 0.001 {
        Some(doc.add_object(dictionary! {
            "Type" => "ExtGState",
            "ca" => Object::Real(opts.opacity.clamp(0.0, 1.0)),
            "CA" => Object::Real(opts.opacity.clamp(0.0, 1.0)),
        }))
    } else {
        None
    };

    // Unique per call: a second image watermark must not re-point the first
    // one's resource names at the new image (or its opacity state).
    let img_alias = format!("ImRPDF{}", image_id.0).into_bytes();
    let gs_alias = format!("GIRPDF{}", gstate_id.map_or(0, |id| id.0)).into_bytes();
    let scale = opts.scale_frac.clamp(0.01, 2.0);
    let margin = opts.margin_pt.max(0.0);

    let pages: Vec<ObjectId> = doc.page_iter().collect();
    for page_id in pages {
        let pl = PageLayout::of(doc, page_id);
        let (pw, ph) = (pl.w, pl.h);
        let target_w = pw * scale;
        let target_h = target_w * (ih as f32) / (iw as f32);

        // Anchor in the displayed page (top-origin), then map to PDF space.
        let (ax, ay) = match opts.anchor {
            ImageAnchor::Center => (pw / 2.0, ph / 2.0),
            ImageAnchor::TopLeft => (margin + target_w / 2.0, margin + target_h / 2.0),
            ImageAnchor::TopRight => (pw - margin - target_w / 2.0, margin + target_h / 2.0),
            ImageAnchor::BottomLeft => (margin + target_w / 2.0, ph - margin - target_h / 2.0),
            ImageAnchor::BottomRight => (pw - margin - target_w / 2.0, ph - margin - target_h / 2.0),
        };
        let (cx, cy) = pl.to_pdf(ax, ay);

        let theta = (opts.rotation_deg + pl.rot as f32).to_radians();
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
            stream.extend_from_slice(&gs_alias);
            stream.extend_from_slice(b" gs\n");
        }
        let _ = write!(
            &mut stream,
            "{:.5} {:.5} {:.5} {:.5} {:.3} {:.3} cm\n/{} Do\nQ\n",
            a, b, cc, d, e, f,
            std::str::from_utf8(&img_alias).unwrap_or("Im"),
        );

        let rid = ensure_own_resources(doc, page_id)?;
        register_alias(doc, rid, b"XObject", &img_alias, image_id)?;
        if let Some(gs) = gstate_id {
            register_alias(doc, rid, b"ExtGState", &gs_alias, gs)?;
        }
        append_content_stream(doc, page_id, stream)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One 612×792 page. `inherit_resources`: the page has no /Resources and
    /// inherits a font from the /Pages root. `rotate`: page /Rotate.
    fn doc(inherit_resources: bool, rotate: i64) -> (Document, ObjectId) {
        let mut d = Document::with_version("1.5");
        let pages_id = d.new_object_id();
        let font_id = d.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Times-Roman",
        });
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Rotate" => rotate,
        };
        let res = dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font_id) }),
        };
        if !inherit_resources {
            page.set("Resources", Object::Dictionary(res.clone()));
        }
        let page_id = d.add_object(Object::Dictionary(page));
        let mut pages = dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1,
        };
        if inherit_resources {
            pages.set("Resources", Object::Dictionary(res));
        }
        d.objects.insert(pages_id, Object::Dictionary(pages));
        let cat_id = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
        d.trailer.set("Root", Object::Reference(cat_id));
        (d, page_id)
    }

    fn page_resources(d: &Document, page_id: ObjectId) -> Dictionary {
        let rid = d.get_dictionary(page_id).unwrap().get(b"Resources").unwrap().as_reference().unwrap();
        d.get_dictionary(rid).unwrap().clone()
    }

    fn sub_dict(d: &Document, res: &Dictionary, key: &[u8]) -> Dictionary {
        match res.get(key).unwrap() {
            Object::Reference(r) => d.get_dictionary(*r).unwrap().clone(),
            o => o.as_dict().unwrap().clone(),
        }
    }

    fn png(rgba: bool) -> Vec<u8> {
        let img = if rgba {
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 0])))
        } else {
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 2, image::Rgb([255, 0, 0])))
        };
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn watermark_keeps_inherited_resources() {
        let (mut d, page_id) = doc(true, 0);
        add_text_watermark(&mut d, "DRAFT", &WatermarkOptions::default()).unwrap();
        let fonts = sub_dict(&d, &page_resources(&d, page_id), b"Font");
        assert!(fonts.has(b"F1"), "inherited font shadowed: {fonts:?}");
        assert!(fonts.has(b"FRPDF"));
    }

    #[test]
    fn second_image_watermark_keeps_the_first_image() {
        let (mut d, page_id) = doc(false, 0);
        let opts = ImageWatermarkOptions::default();
        add_image_watermark(&mut d, &png(false), &opts).unwrap();
        add_image_watermark(&mut d, &png(false), &opts).unwrap();
        let res = page_resources(&d, page_id);
        let xobjects = sub_dict(&d, &res, b"XObject");
        let targets: std::collections::HashSet<ObjectId> =
            xobjects.iter().map(|(_, v)| v.as_reference().unwrap()).collect();
        assert_eq!(targets.len(), 2, "both watermarks must keep their own image: {xobjects:?}");
        assert_eq!(sub_dict(&d, &res, b"ExtGState").len(), 2);
    }

    #[test]
    fn transparent_png_watermark_keeps_alpha() {
        let (mut d, page_id) = doc(false, 0);
        add_image_watermark(&mut d, &png(true), &ImageWatermarkOptions::default()).unwrap();
        let xobjects = sub_dict(&d, &page_resources(&d, page_id), b"XObject");
        let (_, img_ref) = xobjects.iter().next().unwrap();
        let img = d.get_object(img_ref.as_reference().unwrap()).unwrap().as_stream().unwrap();
        assert!(img.dict.has(b"SMask"), "alpha dropped: {:?}", img.dict);
    }

    #[test]
    fn indirect_contents_array_is_spliced() {
        let (mut d, page_id) = doc(false, 0);
        let s = d.add_object(Object::Stream(Stream::new(Dictionary::new(), b"0 0 m".to_vec())));
        let arr = d.add_object(Object::Array(vec![Object::Reference(s)]));
        d.get_dictionary_mut(page_id).unwrap().set("Contents", Object::Reference(arr));
        add_text_watermark(&mut d, "DRAFT", &WatermarkOptions::default()).unwrap();
        let contents = d.get_dictionary(page_id).unwrap().get(b"Contents").unwrap().as_array().unwrap();
        for c in contents {
            let id = c.as_reference().unwrap();
            assert!(d.get_object(id).unwrap().as_stream().is_ok(), "non-stream in /Contents: {id:?}");
        }
        assert!(contents.contains(&Object::Reference(s)));
    }

    #[test]
    fn header_sits_at_displayed_top_of_rotated_page() {
        let (mut d, page_id) = doc(false, 90);
        let opts = HeaderFooterOptions {
            header_left: "H".into(),
            margin_pt: 24.0,
            font_size: 10.0,
            ..Default::default()
        };
        add_header_footer(&mut d, &opts).unwrap();
        let content = d.get_page_content(page_id).unwrap();
        let s = String::from_utf8_lossy(&content);
        let tm: Vec<f32> = s
            .lines()
            .find(|l| l.ends_with(" Tm"))
            .unwrap()
            .split_whitespace()
            .take(6)
            .map(|v| v.parse().unwrap())
            .collect();
        // /Rotate 90 shows PDF x=0 as the top edge and PDF y=0 as the left
        // edge: the header-left baseline starts at x = margin, y = margin,
        // with text running along +y so it reads upright.
        assert!(tm[0].abs() < 1e-3 && (tm[1] - 1.0).abs() < 1e-3, "matrix {tm:?}");
        assert!((tm[4] - 24.0).abs() < 0.01 && (tm[5] - 24.0).abs() < 0.01, "origin {tm:?}");
    }
}
