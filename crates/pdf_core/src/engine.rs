use crate::error::CoreResult;
use pdf_types::{
    AttachmentInfo, CharBox, DocMetadata, DocPermissions, DocumentId, FormFieldInfo, OutlineNode,
    PageIndex, PageLink, PageSize, Rotation, SearchHit, SearchQuery, TextBlock, TextReplace,
    TextRun, ZoomLevel,
};

#[derive(Debug, Clone, Copy)]
pub struct RenderRequest {
    pub document: DocumentId,
    pub page: PageIndex,
    pub zoom: ZoomLevel,
    pub rotation: Rotation,
}

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub width_px: u32,
    pub height_px: u32,
    pub rgba: Vec<u8>,
}

/// Abstraction over the concrete PDF backend. Implemented by `pdf_pdfium`.
pub trait PdfEngine: Send + Sync {
    fn open_bytes(&self, bytes: Vec<u8>) -> CoreResult<DocumentId> {
        self.open_bytes_with_password(bytes, None)
    }
    fn open_bytes_with_password(
        &self,
        bytes: Vec<u8>,
        password: Option<String>,
    ) -> CoreResult<DocumentId>;
    fn close(&self, doc: DocumentId) -> CoreResult<()>;
    fn page_count(&self, doc: DocumentId) -> CoreResult<u32>;
    fn page_size(&self, doc: DocumentId, page: PageIndex) -> CoreResult<PageSize>;
    fn render(&self, req: RenderRequest) -> CoreResult<RenderedPage>;
    /// Render only the `width × height` pixel rectangle at (`x`, `y`) of the
    /// page as it would appear rendered whole at `req.zoom` / `req.rotation`.
    /// Used for sharp detail at zoom levels where a full-page bitmap would be
    /// huge. Default: unsupported.
    fn render_region(
        &self,
        _req: RenderRequest,
        _x: i32,
        _y: i32,
        _width: u32,
        _height: u32,
    ) -> CoreResult<RenderedPage> {
        Err(crate::error::CoreError::Engine("region rendering is not supported".into()))
    }
    /// For documents rendered from a downsampled proxy (huge scanned
    /// images): the zoom up to which full-page renders are sharp. Above it
    /// callers should switch to `render_region`. `None` = no proxy.
    fn detail_scale(&self, _doc: DocumentId) -> Option<f32> {
        None
    }
    fn extract_text(&self, doc: DocumentId, page: PageIndex) -> CoreResult<String>;
    fn page_text_layout(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<TextRun>>;
    fn outline(&self, doc: DocumentId) -> CoreResult<Vec<OutlineNode>>;
    fn search(&self, doc: DocumentId, query: &SearchQuery) -> CoreResult<Vec<SearchHit>>;
    fn page_links(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<PageLink>>;
    fn list_attachments(&self, doc: DocumentId) -> CoreResult<Vec<AttachmentInfo>>;
    fn read_attachment(&self, doc: DocumentId, index: u32) -> CoreResult<Vec<u8>>;
    fn document_metadata(&self, doc: DocumentId) -> CoreResult<DocMetadata>;
    fn document_permissions(&self, doc: DocumentId) -> CoreResult<DocPermissions>;
    fn document_bytes(&self, doc: DocumentId) -> CoreResult<Vec<u8>>;
    /// Replace the resident document for `doc` with freshly parsed `bytes`.
    /// The `DocumentId` is preserved so callers (UI, cache) don't need to
    /// re-key. Used after structural mutations (delete/insert/reorder pages).
    fn reload_bytes(&self, doc: DocumentId, bytes: Vec<u8>) -> CoreResult<()>;
    /// True when the document carries a standard security handler
    /// (password protection / permission restrictions).
    fn is_protected(&self, _doc: DocumentId) -> CoreResult<bool> {
        Ok(false)
    }
    /// Re-serialise the resident document via the backend's own writer
    /// (pdfium `FPDF_SaveAsCopy`). Strips encryption — used to flatten a
    /// password-protected document to a plaintext working copy.
    fn save_copy_bytes(&self, _doc: DocumentId) -> CoreResult<Vec<u8>> {
        Err(crate::error::CoreError::Engine(
            "save_copy_bytes is not supported by this engine".into(),
        ))
    }
    /// Enumerate AcroForm fields. Returns empty Vec for documents without forms.
    fn list_form_fields(&self, doc: DocumentId) -> CoreResult<Vec<FormFieldInfo>>;
    /// Snippets of invisible text on a page — characters drawn with text
    /// rendering mode 3 (or invisible-with-clipping). Consecutive invisible
    /// characters are grouped into one snippet. Default: none found.
    fn page_hidden_text(&self, _doc: DocumentId, _page: PageIndex) -> CoreResult<Vec<String>> {
        Ok(Vec::new())
    }
    /// Every character on the page with its loose bounding box, in page
    /// order. Used to map regex matches over the page text back to regions.
    /// Default: none.
    fn page_char_boxes(&self, _doc: DocumentId, _page: PageIndex) -> CoreResult<Vec<CharBox>> {
        Ok(Vec::new())
    }
    /// Editable lines of text in the page content (see [`TextBlock`]).
    /// Default: none.
    fn page_text_blocks(&self, _doc: DocumentId, _page: PageIndex) -> CoreResult<Vec<TextBlock>> {
        Ok(Vec::new())
    }
    /// Replace the page objects of one text block with new text and return
    /// the resulting document bytes; the loaded document is left untouched
    /// (the caller reloads it). Default: unsupported.
    fn replace_page_text(
        &self,
        _doc: DocumentId,
        _page: PageIndex,
        _edit: &TextReplace,
    ) -> CoreResult<Vec<u8>> {
        Err(crate::error::CoreError::Engine(
            "text editing is not supported by this engine".into(),
        ))
    }
    /// Show the native OS print dialog and print the document. Returns `true`
    /// if the user printed, `false` if they cancelled. Default: unsupported.
    fn print_with_dialog(&self, _doc: DocumentId) -> CoreResult<bool> {
        Err(crate::error::CoreError::Engine(
            "printing is not supported on this platform".into(),
        ))
    }
}
