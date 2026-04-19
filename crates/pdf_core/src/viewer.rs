use pdf_types::{DocumentId, PageIndex, ZoomLevel};

#[derive(Debug, Clone)]
pub struct ViewerState {
    pub document: Option<DocumentId>,
    pub page_count: u32,
    pub current_page: PageIndex,
    pub zoom: ZoomLevel,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            document: None,
            page_count: 0,
            current_page: PageIndex(0),
            zoom: ZoomLevel(1.0),
        }
    }
}

impl ViewerState {
    pub fn go_to(&mut self, page: PageIndex) {
        if page.0 < self.page_count {
            self.current_page = page;
        }
    }

    pub fn next_page(&mut self) {
        if self.current_page.0 + 1 < self.page_count {
            self.current_page.0 += 1;
        }
    }

    pub fn prev_page(&mut self) {
        if self.current_page.0 > 0 {
            self.current_page.0 -= 1;
        }
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = ZoomLevel::clamp(zoom);
    }
}
