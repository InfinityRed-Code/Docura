//! Dev utility: generate the app icons + UI assets from the CI logos.
//!
//!     cargo run -p pdf_mutate --example brand_icons
//!
//! Reads CI/Logo.svg and CI/Logo+Text.svg (vector art that includes a
//! white background rectangle as its first path). For the raster icons
//! the SVG is rendered and the background removed via border flood fill;
//! for the in-app assets the background path is stripped from the SVG
//! and the viewBox cropped to the content so the vector stays crisp.
//!
//! Writes:
//!   apps/desktop-tauri/src-tauri/icons/icon.png    (512², square padded)
//!   apps/desktop-tauri/src-tauri/icons/icon.ico    (256/128/64/48/32/16)
//!   apps/desktop-tauri/ui/src/assets/logo.svg      (cropped, transparent)
//!   apps/desktop-tauri/ui/src/assets/logo-text.svg (cropped, transparent)
//!
//! Run from the repo root.

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::imageops::FilterType;
use image::{DynamicImage, Rgba, RgbaImage};
use std::collections::VecDeque;

/// Remove the first all-white background path (the logos ship with a
/// full-canvas `#FEFEFE` rectangle as layer 1).
fn strip_background_path(svg: &str) -> String {
    if let Some(start) = svg.find("<path fill=\"#FEFEFE\"") {
        if let Some(end_rel) = svg[start..].find("/>") {
            let mut out = String::with_capacity(svg.len());
            out.push_str(&svg[..start]);
            out.push_str(&svg[start + end_rel + 2..]);
            return out;
        }
    }
    svg.to_string()
}

/// Render an SVG at its natural size (1 px per SVG unit).
fn render_svg(svg: &str) -> RgbaImage {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).expect("svg parse");
    let size = tree.size();
    let (w, h) = (size.width().ceil() as u32, size.height().ceil() as u32);
    let mut pm = resvg::tiny_skia::Pixmap::new(w, h).expect("pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pm.as_mut(),
    );
    let mut img = RgbaImage::new(w, h);
    for (i, p) in pm.pixels().iter().enumerate() {
        let c = p.demultiply();
        img.put_pixel(
            i as u32 % w,
            i as u32 / w,
            Rgba([c.red(), c.green(), c.blue(), c.alpha()]),
        );
    }
    img
}

/// Make every near-white pixel that is 4-connected to the border
/// transparent (safety net for any remaining white canvas).
fn knock_out_background(rgba: &mut RgbaImage) {
    let (w, h) = rgba.dimensions();
    let near_white =
        |p: &Rgba<u8>| p[3] == 0 || (p[0] >= 247 && p[1] >= 247 && p[2] >= 247);
    let mut visited = vec![false; (w * h) as usize];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    for x in 0..w {
        queue.push_back((x, 0));
        queue.push_back((x, h - 1));
    }
    for y in 0..h {
        queue.push_back((0, y));
        queue.push_back((w - 1, y));
    }
    while let Some((x, y)) = queue.pop_front() {
        let idx = (y * w + x) as usize;
        if visited[idx] {
            continue;
        }
        visited[idx] = true;
        if !near_white(rgba.get_pixel(x, y)) {
            continue;
        }
        rgba.get_pixel_mut(x, y).0[3] = 0;
        if x > 0 {
            queue.push_back((x - 1, y));
        }
        if x + 1 < w {
            queue.push_back((x + 1, y));
        }
        if y > 0 {
            queue.push_back((x, y - 1));
        }
        if y + 1 < h {
            queue.push_back((x, y + 1));
        }
    }
}

/// Bounding box of non-transparent pixels plus a relative margin.
fn content_bbox(img: &RgbaImage, margin_frac: f32) -> (u32, u32, u32, u32) {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 8 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    let m = ((x1 - x0).max(y1 - y0) as f32 * margin_frac) as u32;
    let x0 = x0.saturating_sub(m);
    let y0 = y0.saturating_sub(m);
    let x1 = (x1 + m).min(w - 1);
    let y1 = (y1 + m).min(h - 1);
    (x0, y0, x1 - x0 + 1, y1 - y0 + 1)
}

/// Pad to a centered square canvas (transparent).
fn pad_square(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let side = w.max(h);
    let mut out = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
    image::imageops::overlay(&mut out, img, ((side - w) / 2) as i64, ((side - h) / 2) as i64);
    out
}

fn resize(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::imageops::resize(img, w, h, FilterType::Lanczos3)
}

/// Set the SVG viewBox to the rendered content bbox so the vector asset
/// has no dead margin around the artwork.
fn rewrite_viewbox(svg: &str, bbox: (u32, u32, u32, u32)) -> String {
    let start = svg.find("viewBox=\"").expect("viewBox");
    let vstart = start + "viewBox=\"".len();
    let vend = vstart + svg[vstart..].find('"').expect("viewBox end");
    format!(
        "{}{} {} {} {}{}",
        &svg[..vstart],
        bbox.0,
        bbox.1,
        bbox.2,
        bbox.3,
        &svg[vend..]
    )
}

fn process(svg_path: &str, out_svg: &str) -> RgbaImage {
    let raw = std::fs::read_to_string(svg_path).expect(svg_path);
    let stripped = strip_background_path(&raw);
    let mut img = render_svg(&stripped);
    // Flood fill only as a fallback: when the background path was
    // stripped successfully the canvas is already transparent, and the
    // fill would chew into the white page edges of the artwork.
    let (w, h) = img.dimensions();
    let corners_clear = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
        .iter()
        .all(|&(x, y)| img.get_pixel(x, y)[3] == 0);
    if !corners_clear {
        knock_out_background(&mut img);
    }
    let bbox = content_bbox(&img, 0.02);
    std::fs::write(out_svg, rewrite_viewbox(&stripped, bbox)).expect(out_svg);
    image::imageops::crop_imm(&img, bbox.0, bbox.1, bbox.2, bbox.3).to_image()
}

fn main() {
    // Square app icon from the plain logo.
    let cut = process("CI/Logo.svg", "apps/desktop-tauri/ui/src/assets/logo.svg");
    let square = pad_square(&cut);

    resize(&square, 512, 512)
        .save("apps/desktop-tauri/src-tauri/icons/icon.png")
        .expect("write icon.png");

    let sizes = [256u32, 128, 64, 48, 32, 16];
    let frames: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| {
            let img = resize(&square, s, s);
            let mut png = Vec::new();
            DynamicImage::ImageRgba8(img)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .expect("png frame");
            png
        })
        .collect();
    let ico_frames: Vec<IcoFrame> = frames
        .iter()
        .zip(sizes.iter())
        .map(|(png, &s)| {
            IcoFrame::with_encoded(png.as_slice(), s, s, image::ExtendedColorType::Rgba8)
                .expect("ico frame")
        })
        .collect();
    let file = std::fs::File::create("apps/desktop-tauri/src-tauri/icons/icon.ico").expect("ico");
    IcoEncoder::new(file)
        .encode_images(&ico_frames)
        .expect("write icon.ico");

    // Logo with wordmark for the viewer empty state.
    let lt = process(
        "CI/Logo+Text.svg",
        "apps/desktop-tauri/ui/src/assets/logo-text.svg",
    );
    println!(
        "done: icon 512 + ico {:?}, logo {}x{}, logo-text {}x{}",
        sizes,
        cut.width(),
        cut.height(),
        lt.width(),
        lt.height()
    );
}
