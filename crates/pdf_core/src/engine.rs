use crate::error::CoreResult;
use pdf_types::{
    DocumentId, OutlineNode, PageIndex, PageSize, SearchHit, SearchQuery, TextRun, ZoomLevel,
};

#[derive(Debug, Clone, Copy)]
pub struct RenderRequest {
    pub document: DocumentId,
    pub page: PageIndex,
    pub zoom: ZoomLevel,
}

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub width_px: u32,
    pub height_px: u32,
    pub rgba: Vec<u8>,
}

/// Abstraction over the concrete PDF backend. Implemented by `pdf_pdfium`.
pub trait PdfEngine: Send + Sync {
    fn open_bytes(&self, bytes: Vec<u8>) -> CoreResult<DocumentId>;
    fn close(&self, doc: DocumentId) -> CoreResult<()>;
    fn page_count(&self, doc: DocumentId) -> CoreResult<u32>;
    fn page_size(&self, doc: DocumentId, page: PageIndex) -> CoreResult<PageSize>;
    fn render(&self, req: RenderRequest) -> CoreResult<RenderedPage>;
    fn extract_text(&self, doc: DocumentId, page: PageIndex) -> CoreResult<String>;
    fn page_text_layout(&self, doc: DocumentId, page: PageIndex) -> CoreResult<Vec<TextRun>>;
    fn outline(&self, doc: DocumentId) -> CoreResult<Vec<OutlineNode>>;
    fn search(&self, doc: DocumentId, query: &SearchQuery) -> CoreResult<Vec<SearchHit>>;
}
