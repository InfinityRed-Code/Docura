//! Build a PDF from a list of raster images (one page per image).
//!
//! JPEG inputs are embedded as-is (DCTDecode, no recompression); everything
//! else (PNG, BMP, …) is decoded and stored as lossless Flate-compressed
//! RGB. Page size equals the pixel size at 72 dpi (1 px = 1 pt), matching
//! what Acrobat does for images without density metadata.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};

use crate::{MutateError, MutateResult};

struct EmbeddedImage {
    stream: Stream,
    width: u32,
    height: u32,
}

fn embed_image(bytes: &[u8]) -> MutateResult<EmbeddedImage> {
    let format = image::guess_format(bytes)
        .map_err(|e| MutateError::Parse(format!("image format: {e}")))?;
    let img = image::load_from_memory(bytes)
        .map_err(|e| MutateError::Parse(format!("image decode: {e}")))?;
    let (width, height) = (img.width(), img.height());

    // Plain RGB JPEG: keep the original bytes, no generation loss.
    if format == image::ImageFormat::Jpeg
        && matches!(img.color(), image::ColorType::Rgb8 | image::ColorType::L8)
    {
        let color: &str = if img.color() == image::ColorType::L8 {
            "DeviceGray"
        } else {
            "DeviceRGB"
        };
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => color,
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        };
        return Ok(EmbeddedImage {
            stream: Stream::new(dict, bytes.to_vec()).with_compression(false),
            width,
            height,
        });
    }

    // Everything else: lossless Flate-compressed RGB.
    let rgb = img.to_rgb8();
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
    };
    let mut stream = Stream::new(dict, rgb.into_raw());
    stream
        .compress()
        .map_err(|e| MutateError::Serialize(format!("flate: {e}")))?;
    Ok(EmbeddedImage {
        stream,
        width,
        height,
    })
}

/// Build a multi-page PDF, one page per input image. Returns the PDF bytes.
pub fn images_to_pdf(images: &[Vec<u8>]) -> MutateResult<Vec<u8>> {
    if images.is_empty() {
        return Err(MutateError::Parse("images_to_pdf: no images".into()));
    }
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(images.len());

    for bytes in images {
        let img = embed_image(bytes)?;
        let image_id = doc.add_object(Object::Stream(img.stream));
        let (w_pt, h_pt) = (img.width as f32, img.height as f32);

        let content = format!("q\n{w_pt:.2} 0 0 {h_pt:.2} 0 0 cm\n/Im0 Do\nQ\n");
        let content_id = doc.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            content.into_bytes(),
        )));

        let mut xobjects = Dictionary::new();
        xobjects.set("Im0", Object::Reference(image_id));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "XObject" => Object::Dictionary(xobjects),
            }),
            "MediaBox" => Object::Array(vec![
                0.into(),
                0.into(),
                Object::Real(w_pt),
                Object::Real(h_pt),
            ]),
        }));
        kids.push(Object::Reference(page_id));
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(images.len() as i64),
        }),
    );
    let cat_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat_id));
    doc.max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(1);

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| MutateError::Serialize(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_png(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(4, 6, image::Rgb([r, g, b]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn tiny_jpeg() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(8, 5, image::Rgb([200, 10, 10]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn builds_one_page_per_image() {
        let bytes = images_to_pdf(&[tiny_png(0, 0, 255), tiny_jpeg()]).unwrap();
        let doc = Document::load_mem(&bytes).unwrap();
        let pages: Vec<_> = doc.page_iter().collect();
        assert_eq!(pages.len(), 2);
        // Page 1 sized like the 4×6 PNG.
        let p0 = doc.get_dictionary(pages[0]).unwrap();
        let mb = p0.get(b"MediaBox").unwrap().as_array().unwrap();
        assert_eq!(mb[2].as_float().unwrap(), 4.0);
        assert_eq!(mb[3].as_float().unwrap(), 6.0);
        // JPEG page keeps DCTDecode.
        assert!(bytes.windows(9).any(|w| w == b"DCTDecode"));
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(images_to_pdf(&[]).is_err());
    }
}
