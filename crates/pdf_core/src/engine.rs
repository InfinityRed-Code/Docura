use crate::error::CoreResult;
use pdf_types::{
    AttachmentInfo, DocMetadata, DocPermissions, DocumentId, FormFieldInfo, OutlineNode, PageIndex,
    PageLink, PageSize, Rotation, SearchHit, SearchQuery, TextRun, ZoomLevel,
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
    /// Enumerate AcroForm fields. Returns empty Vec for documents without forms.
    fn list_form_fields(&self, doc: DocumentId) -> CoreResult<Vec<FormFieldInfo>>;
}
