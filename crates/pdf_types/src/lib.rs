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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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

/// One character with its loose bounding box in our top-origin pt
/// convention. Used for pattern matching that needs to map text matches
/// back to page regions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharBox {
    pub ch: char,
    pub rect_pt: RectPt,
}

/// One optional-content group (layer) declared in the catalog's
/// /OCProperties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInfo {
    pub name: String,
    /// True when the default viewing configuration lists the OCG in /OFF —
    /// i.e. the layer's content exists in the file but is not displayed.
    pub hidden: bool,
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
    /// Text styling of a /FreeText annotation, so the UI can reopen it in
    /// the editor. `None` for every other subtype.
    #[serde(default)]
    pub freetext: Option<FreeTextStyle>,
    /// /Subj: what the annotation is ("Stern", "Wolke") where the subtype
    /// alone does not say it (a /Polygon can be any of several shapes).
    #[serde(default)]
    pub subject: Option<String>,
}

/// Styling of a /FreeText annotation as written by this app (other writers'
/// boxes fall back to what their /DA string carries).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeTextStyle {
    pub font_size: f32,
    /// RGB text color 0..1.
    pub text_color: [f32; 3],
    /// Background fill; `None` = transparent.
    pub bg_color: Option<[f32; 3]>,
    /// Logical family: "Helv" | "Times" | "Cour", or a bundled family id
    /// (`pdf_fonts`, e.g. "Roboto").
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    /// Viewer rotation (quarter turns CW) the text was written upright for.
    pub rotation_quadrant: u8,
}

/// One editable line of text that is part of the page content itself (not
/// an annotation): the page objects it is made of plus enough styling to
/// reopen it in the text editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    /// Indices of the page objects forming this line.
    pub objects: Vec<u32>,
    pub text: String,
    /// Bounding box in displayed top-origin points.
    pub rect_pt: RectPt,
    /// Start of the baseline in displayed top-origin points.
    pub origin_x: f32,
    pub origin_y: f32,
    /// Reading direction on the displayed page in quarter turns clockwise
    /// (0 = left to right, 1 = top to bottom, …).
    pub dir: u8,
    pub font_size: f32,
    /// Closest standard family ("Helv" | "Times" | "Cour"), or the bundled
    /// family (`pdf_fonts`) the line is written with.
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    pub color: [f32; 3],
    /// Drawn with an invisible render mode (OCR layer over a scan).
    pub invisible: bool,
    /// The font carries no usable Unicode mapping: `text` is not what the
    /// glyphs show (it is blanked), so the line can only be retyped.
    #[serde(default)]
    pub unreadable: bool,
    /// Installed font file the line's font was identified as; edits embed
    /// it so the new text looks like the old.
    #[serde(default)]
    pub font_file: Option<String>,
    /// Family name of that font ("Consolas"), for displaying the editor.
    #[serde(default)]
    pub font_name: Option<String>,
}

/// Replace the page objects of one [`TextBlock`] with new text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextReplace {
    pub objects: Vec<u32>,
    /// New text; empty = just remove the old text.
    pub text: String,
    pub font_size: f32,
    pub font_family: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    pub color: [f32; 3],
    /// Write the text with this installed font file (from
    /// [`TextBlock::font_file`]) instead of a standard font.
    #[serde(default)]
    pub font_file: Option<String>,
    /// Paint this color over the old text's box first — for scans, where
    /// the visible glyphs are pixels of the page image.
    #[serde(default)]
    pub cover: Option<[f32; 3]>,
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

/// A 3D annotation (`/Subtype /3D`, or a RichMedia annotation with a 3D
/// asset) on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeDAnnot {
    /// Same id as `AnnotationInfo::id` (`/NM`, else the object id).
    pub id: String,
    pub page: u32,
    /// Annotation rectangle, top-origin view space like `AnnotationInfo`.
    pub rect_pt: RectPt,
    /// "3D" or "RichMedia".
    pub subtype: String,
    /// Model format: "U3D", "PRC" or what the file declares.
    pub format: String,
    /// Size of the decoded model data in bytes.
    pub model_size: usize,
    /// The annotation has a normal appearance (poster image) to show while
    /// inactive.
    pub has_poster: bool,
    /// `/3DA /A`: "PO" (page opened), "PV" (page visible), "XA" (explicit).
    pub activation: String,
    /// `/3DA /TB`: show a toolbar when active.
    pub toolbar: bool,
    /// `/3DA /NP`: show the model tree when active.
    pub model_tree: bool,
    /// Preset views (`/VA`), plus a view given only in `/3DV`.
    pub views: Vec<ThreeDView>,
    /// Index into `views` of the initial view; `None` = the model's own.
    pub default_view: Option<u32>,
    /// `/Contents`, if any.
    pub contents: Option<String>,
}

/// A 3D view dictionary (ISO 32000 13.6.4).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreeDView {
    /// `/XN`: name shown to the user.
    pub name: String,
    /// `/IN`.
    #[serde(default)]
    pub internal_name: Option<String>,
    /// `/C2W` (with `/MS /M`): camera-to-world matrix, columns x (left of
    /// the screen), y (up), z (view direction) and position.
    #[serde(default)]
    pub c2w: Option<[f32; 12]>,
    /// `/U3DPath` (with `/MS /U3D`): view node names in the U3D model.
    #[serde(default)]
    pub u3d_path: Vec<String>,
    /// `/CO`: distance along the view direction to the center of orbit.
    #[serde(default)]
    pub center_of_orbit: Option<f32>,
    /// `/P /Subtype /O`.
    #[serde(default)]
    pub orthographic: bool,
    /// `/P /FOV` in degrees (perspective).
    #[serde(default)]
    pub fov: Option<f32>,
    /// `/P /PS`: what the FOV circle spans: "W", "H", "Min", "Max" or a
    /// diameter in points.
    #[serde(default)]
    pub fov_span: Option<String>,
    /// `/P /OS` (orthographic scale).
    #[serde(default)]
    pub ortho_scale: Option<f32>,
    /// `/P /OB` (orthographic binding): "W", "H", "Min", "Max", "Absolute".
    #[serde(default)]
    pub ortho_binding: Option<String>,
    /// `/BG /C`.
    #[serde(default)]
    pub background: Option<[f32; 3]>,
    /// `/RM /Subtype` ("Solid", "Wireframe", "Transparent", …).
    #[serde(default)]
    pub render_mode: Option<String>,
    /// `/RM /AC` (edge color).
    #[serde(default)]
    pub render_aux_color: Option<[f32; 3]>,
    /// `/LS /Subtype` ("Artwork", "White", "CAD", "Headlamp", …).
    #[serde(default)]
    pub lighting: Option<String>,
    /// `/SA`.
    #[serde(default)]
    pub cross_sections: Vec<ThreeDCrossSection>,
    /// `/NA` (node states).
    #[serde(default)]
    pub nodes: Vec<ThreeDNodeState>,
}

/// `/SA` entry: a cutting plane.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreeDCrossSection {
    pub center: [f32; 3],
    /// Rotations about x, y, z in degrees; exactly one is `None` (the
    /// axis the plane starts perpendicular to).
    pub orientation: [Option<f32>; 3],
    pub plane_opacity: f32,
    pub plane_color: [f32; 3],
    pub plane_visible: bool,
    pub intersection_visible: bool,
    pub intersection_color: [f32; 3],
}

/// `/NA` entry: state of one node in a view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreeDNodeState {
    pub name: String,
    pub visible: Option<bool>,
    pub opacity: Option<f32>,
    /// 3x4 transform (same layout as `ThreeDView::c2w`).
    pub matrix: Option<[f32; 12]>,
}
