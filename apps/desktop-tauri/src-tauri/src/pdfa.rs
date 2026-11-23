//! PDF/A export: the built-in converter (`pdf_pdfa`, no Ghostscript) and
//! the previous Ghostscript path, selectable in the settings.

use std::sync::{Arc, Mutex};

use pdf_core::{PdfEngine, RenderRequest};
use pdf_pdfa::{Level, Options, PdfaError};
use pdf_types::{PageIndex, Rotation, ZoomLevel};
use serde::Serialize;

/// Which converter produces PDF/A files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfaEngine {
    Builtin,
    Ghostscript,
}

impl PdfaEngine {
    pub fn parse(s: &str) -> Option<PdfaEngine> {
        match s {
            "builtin" => Some(PdfaEngine::Builtin),
            "ghostscript" => Some(PdfaEngine::Ghostscript),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            PdfaEngine::Builtin => "builtin",
            PdfaEngine::Ghostscript => "ghostscript",
        }
    }
}

static ENGINE: Mutex<PdfaEngine> = Mutex::new(PdfaEngine::Builtin);

pub fn engine() -> PdfaEngine {
    *ENGINE.lock().unwrap()
}

pub fn set_engine(e: PdfaEngine) {
    *ENGINE.lock().unwrap() = e;
}

/// Every level the settings accept ("off" = plain PDF).
pub const LEVEL_IDS: [&str; 12] = ["off", "1a", "1b", "2a", "2b", "2u", "3a", "3b", "3u", "4", "4e", "4f"];

/// Error text the UI translates: `PDFA_ERR:<code>:<detail>`.
pub fn ui_error(code: &str, detail: impl std::fmt::Display) -> String {
    format!("PDFA_ERR:{code}:{detail}")
}

pub fn map_error(e: &PdfaError) -> String {
    let code = match e {
        PdfaError::PasswordRequired => "password",
        PdfaError::Parse(_) => "parse",
        PdfaError::Unsupported(_) => "unsupported",
        PdfaError::NeedsRasterizer(_) => "render",
        PdfaError::AttachmentRequired => "attachment_required",
        PdfaError::Write(_) => "write",
    };
    ui_error(code, e)
}

#[derive(Serialize, Clone, Debug)]
pub struct NoteDto {
    pub code: String,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct PdfaSaved {
    pub label: String,
    pub engine: String,
    pub notes: Vec<NoteDto>,
}

/// Renders pages with PDFium for the converter's fallback paths
/// (PDF/A-1 transparency, JPEG 2000, unrepairable fonts).
pub struct PdfiumRasterizer {
    engine: pdf_pdfium::PdfiumEngine,
}

impl PdfiumRasterizer {
    pub fn new() -> Option<Arc<PdfiumRasterizer>> {
        pdf_pdfium::PdfiumEngine::new().ok().map(|engine| Arc::new(PdfiumRasterizer { engine }))
    }
}

impl pdf_pdfa::Rasterizer for PdfiumRasterizer {
    fn render_page(&self, pdf: &[u8], page: u32, dpi: f32) -> Result<pdf_pdfa::RasterPage, String> {
        let doc = self.engine.open_bytes(pdf.to_vec()).map_err(|e| e.to_string())?;
        let zoom = (dpi / 72.0).clamp(ZoomLevel::MIN, ZoomLevel::MAX);
        let rendered = self.engine.render(RenderRequest {
            document: doc,
            page: PageIndex(page),
            zoom: ZoomLevel(zoom),
            rotation: Rotation::R0,
        });
        let runs = self.engine.page_text_layout(doc, PageIndex(page)).unwrap_or_default();
        let _ = self.engine.close(doc);
        let r = rendered.map_err(|e| e.to_string())?;
        // RGBA over white (PDFium bitmaps carry alpha for transparent areas).
        let rgb: Vec<u8> = r
            .rgba
            .chunks(4)
            .flat_map(|p| {
                let a = p[3] as u32;
                let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
                [blend(p[0]), blend(p[1]), blend(p[2])]
            })
            .collect();
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

/// Convert with the built-in engine.
pub fn convert_builtin(bytes: &[u8], level: &str, lang: Option<String>) -> Result<(Vec<u8>, PdfaSaved), String> {
    let lvl: Level = level.parse().map_err(|e: String| ui_error("level", e))?;
    let mut opts = Options::new(lvl);
    opts.lang = lang.filter(|l| !l.is_empty());
    if let Some(r) = PdfiumRasterizer::new() {
        opts = opts.with_rasterizer(r);
    }
    let t = std::time::Instant::now();
    let out = pdf_pdfa::convert(bytes, &opts).map_err(|e| map_error(&e))?;
    tracing::info!("pdfa builtin {} in {:?} ({} → {} bytes)", lvl.label(), t.elapsed(), bytes.len(), out.bytes.len());
    let saved = PdfaSaved {
        label: lvl.label(),
        engine: PdfaEngine::Builtin.id().into(),
        notes: out.report.notes.iter().map(|n| NoteDto { code: n.code.into(), detail: n.detail.clone() }).collect(),
    };
    Ok((out.bytes, saved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn engine_and_levels() {
        assert_eq!(PdfaEngine::parse("builtin"), Some(PdfaEngine::Builtin));
        assert_eq!(PdfaEngine::parse("ghostscript").map(|e| e.id()), Some("ghostscript"));
        assert_eq!(PdfaEngine::parse("x"), None);
        for l in &LEVEL_IDS[1..] {
            assert!(l.parse::<Level>().is_ok(), "{l}");
        }
        assert_eq!(ui_error("password", "x"), "PDFA_ERR:password:x");
        assert!(map_error(&PdfaError::AttachmentRequired).starts_with("PDFA_ERR:attachment_required:"));
    }

    #[test]
    fn builtin_conversion_of_a_minimal_pdf() {
        let mut doc = lopdf::Document::with_version("1.4");
        let pages = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let content = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), b"BT /F1 12 Tf 72 700 Td (Hallo) Tj ET".to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content, "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        doc.objects.insert(pages, lopdf::Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", cat);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let (out, saved) = convert_builtin(&bytes, "2u", Some("de-DE".into())).unwrap();
        assert!(out.starts_with(b"%PDF-1.7"));
        assert_eq!(saved.label, "PDF/A-2u");
        assert!(saved.notes.iter().any(|n| n.code == "fonts_embedded"));
        assert!(convert_builtin(&bytes, "9x", None).unwrap_err().starts_with("PDFA_ERR:level:"));
        assert!(convert_builtin(b"not a pdf", "2b", None).unwrap_err().starts_with("PDFA_ERR:parse:"));
    }
}
