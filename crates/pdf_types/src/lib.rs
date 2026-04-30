use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub Uuid);

impl DocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PageIndex(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoomLevel(pub f32);

impl ZoomLevel {
    pub const MIN: f32 = 0.1;
    pub const MAX: f32 = 16.0;

    pub fn clamp(value: f32) -> Self {
        Self(value.clamp(Self::MIN, Self::MAX))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub term: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RectPt {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PageSize {
    pub width_pt: f32,
    pub height_pt: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub page: PageIndex,
    pub rects_pt: Vec<RectPt>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineNode {
    pub title: String,
    pub page: Option<PageIndex>,
    pub children: Vec<OutlineNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    pub rect_pt: RectPt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: PathBuf,
    pub opened_at_unix: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn from_quarter_turns(q: u8) -> Self {
        match q % 4 {
            0 => Self::R0,
            1 => Self::R90,
            2 => Self::R180,
            _ => Self::R270,
        }
    }

    pub fn quarter_turns(self) -> u8 {
        match self {
            Self::R0 => 0,
            Self::R90 => 1,
            Self::R180 => 2,
            Self::R270 => 3,
        }
    }
}

impl Default for Rotation {
    fn default() -> Self {
        Self::R0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum LinkTarget {
    Page(PageIndex),
    Uri(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    pub rect_pt: RectPt,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub index: u32,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationInfo {
    /// Stable id from /NM, fallback to lopdf object id.
    pub id: String,
    /// 0-indexed page number.
    pub page: u32,
    /// PDF /Subtype name, e.g. "Highlight", "Text", "Square", "Ink", "FreeText", "Stamp".
    pub subtype: String,
    /// Bounding rect in our top-origin RectPt convention.
    pub rect_pt: RectPt,
    /// RGB color in 0..1 if /C is present.
    pub color: Option<[f32; 3]>,
    /// /CA opacity (0..1). Defaults to 1.0 when absent.
    pub opacity: f32,
    /// PDF /F bit 2 (Print flag).
    pub print: bool,
    /// /Contents text (decoded UTF-16BE or PDFDocEncoding).
    pub contents: Option<String>,
    /// /T author / popup title.
    pub author: Option<String>,
    /// /M raw modified-date string (PDF "D:YYYYMMDD..." form).
    pub modified: Option<String>,
    /// /Name (icon hint) for sticky notes / stamps.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormFieldKind {
    Text,
    Checkbox,
    Radio,
    ComboBox,
    ListBox,
    PushButton,
    Signature,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldInfo {
    /// Fully-qualified field name (the value used for /V binding).
    pub id: String,
    pub page: u32,
    /// Widget rect in our top-origin RectPt convention.
    pub rect_pt: RectPt,
    pub kind: FormFieldKind,
    /// Current value (text content, "Yes"/"Off" for checkbox, export value for radio).
    pub value: Option<String>,
    /// Choice options for ComboBox / ListBox.
    pub options: Option<Vec<String>>,
    /// Sibling export value (for radio buttons — every radio in a group shares an /id but different /AS).
    pub export_value: Option<String>,
    pub readonly: bool,
    pub required: bool,
    pub multiline: bool,
    pub password: bool,
    pub max_len: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocPermissions {
    pub security_handler: String,
    pub can_print_high_quality: bool,
    pub can_print_low_quality: bool,
    pub can_modify_content: bool,
    pub can_extract_text_and_graphics: bool,
    pub can_annotate: bool,
    pub can_fill_forms: bool,
    pub can_assemble: bool,
}
