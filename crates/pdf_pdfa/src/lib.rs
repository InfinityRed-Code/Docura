//! PDF → PDF/A conversion in pure Rust (no Ghostscript).
//!
//! [`convert`] takes the bytes of any PDF and returns a file conforming to
//! the requested [`Level`] (PDF/A-1a … PDF/A-4f), or a [`PdfaError`] when
//! the input cannot be made conformant (for example an encrypted file whose
//! password is unknown). It never returns a file it knows to be
//! non-conformant.
//!
//! The pipeline works on the object level with `lopdf`: it keeps the page
//! content as it is wherever the standard allows it and only rewrites what
//! PDF/A forbids. Cases without a lossless fix (transparency in PDF/A-1,
//! JPEG 2000 in PDF/A-1, fonts that cannot be embedded) fall back to the
//! optional [`Rasterizer`] hook, which renders a page so it can be replaced
//! by an image with an invisible text layer.

mod actions;
mod annots;
mod ap;
mod attachments;
mod cmap;
mod color;
pub mod content;
mod cos;
mod flatten;
mod fonts;
mod forms;
mod icc;
mod level;
mod oc;
mod prepare;
mod tagging;
mod util;
mod write;
mod xmp;
mod xobjects;

use std::sync::Arc;

use lopdf::Document;

pub use level::{AttachmentPolicy, Conformance, Level, ALL_LEVELS};

/// A rendered page for the fallback paths.
#[derive(Clone, Debug)]
pub struct RasterPage {
    pub width: u32,
    pub height: u32,
    /// Packed RGB, 3 bytes per pixel, top row first.
    pub rgb: Vec<u8>,
    /// Text on the page with boxes in bitmap pixel coordinates (top-left
    /// origin), used for the invisible text layer.
    pub words: Vec<RasterWord>,
}

#[derive(Clone, Debug)]
pub struct RasterWord {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Renders pages of a PDF. Implemented with PDFium by the app.
pub trait Rasterizer: Send + Sync {
    /// Render page `page` (0-based) of `pdf` at `dpi` with annotations,
    /// as displayed (page /Rotate applied).
    fn render_page(&self, pdf: &[u8], page: u32, dpi: f32) -> std::result::Result<RasterPage, String>;
}

/// Conversion settings.
#[derive(Clone)]
pub struct Options {
    pub level: Level,
    /// Language for level a (`Lang` in the catalog) when the document has
    /// none, e.g. "de-DE".
    pub lang: Option<String>,
    /// Fallback renderer; without it the fallback cases fail with
    /// [`PdfaError::NeedsRasterizer`].
    pub rasterizer: Option<Arc<dyn Rasterizer>>,
    /// Resolution for flattened pages.
    pub flatten_dpi: f32,
    /// `Producer` / `pdf:Producer` value.
    pub producer: String,
    /// Directories searched for fonts to embed (system fonts). Empty means
    /// the platform defaults.
    pub font_dirs: Vec<std::path::PathBuf>,
    /// Nesting depth for embedded PDFs converted along the way.
    pub(crate) depth: u8,
}

impl Options {
    pub fn new(level: Level) -> Self {
        Options {
            level,
            lang: None,
            rasterizer: None,
            flatten_dpi: 300.0,
            producer: "Docura PDF".into(),
            font_dirs: Vec::new(),
            depth: 0,
        }
    }

    pub fn with_rasterizer(mut self, r: Arc<dyn Rasterizer>) -> Self {
        self.rasterizer = Some(r);
        self
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("level", &self.level)
            .field("lang", &self.lang)
            .field("rasterizer", &self.rasterizer.is_some())
            .field("flatten_dpi", &self.flatten_dpi)
            .finish()
    }
}

/// Something the conversion changed that the user may want to know about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Stable code for translation in the UI, e.g. `"attachments_removed"`.
    pub code: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub notes: Vec<Note>,
}

impl Report {
    pub(crate) fn note(&mut self, code: &'static str, detail: impl Into<String>) {
        let detail = detail.into();
        if !self.notes.iter().any(|n| n.code == code && n.detail == detail) {
            self.notes.push(Note { code, detail });
        }
    }

    pub fn has(&self, code: &str) -> bool {
        self.notes.iter().any(|n| n.code == code)
    }
}

/// Result of a successful conversion.
#[derive(Clone, Debug)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub level: Level,
    pub report: Report,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PdfaError {
    #[error("the PDF is password protected; open it with its password first")]
    PasswordRequired,
    #[error("cannot read the PDF: {0}")]
    Parse(String),
    #[error("cannot be made PDF/A conformant: {0}")]
    Unsupported(String),
    #[error("this page needs to be rendered to become conformant ({0}), but no renderer is available")]
    NeedsRasterizer(String),
    #[error("PDF/A-4f requires at least one embedded file")]
    AttachmentRequired,
    #[error("writing the PDF failed: {0}")]
    Write(String),
}

pub type Result<T> = std::result::Result<T, PdfaError>;

pub(crate) struct Ctx<'a> {
    pub doc: Document,
    pub level: Level,
    pub opts: &'a Options,
    pub report: Report,
    /// Input bytes (unencrypted view is `doc`); handed to the rasterizer.
    pub source: &'a [u8],
}

/// Convert `input` to the level in `opts`.
pub fn convert(input: &[u8], opts: &Options) -> Result<Converted> {
    let doc = prepare::load(input)?;
    let mut ctx = Ctx { doc, level: opts.level, opts, report: Report::default(), source: input };
    run_pipeline(&mut ctx)?;
    let bytes = write::save(&mut ctx)?;
    Ok(Converted { bytes, level: ctx.level, report: ctx.report })
}

fn run_pipeline(ctx: &mut Ctx) -> Result<()> {
    prepare::run(ctx)?;
    actions::run(ctx);
    attachments::run(ctx)?;
    forms::run(ctx);
    annots::run(ctx)?;
    oc::run(ctx);
    flatten::run(ctx)?;
    xobjects::run(ctx)?;
    fonts::run(ctx)?;
    color::run(ctx);
    tagging::run(ctx)?;
    cos::run(ctx);
    xmp::run(ctx);
    Ok(())
}

/// Quick check whether `input` is encrypted with a user password (the
/// converter cannot open it without that password).
pub fn needs_password(input: &[u8]) -> bool {
    matches!(prepare::load(input), Err(PdfaError::PasswordRequired))
}
