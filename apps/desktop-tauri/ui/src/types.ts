export type PageSize = { width_pt: number; height_pt: number };
export type RectPt = { x0: number; y0: number; x1: number; y1: number };
export type OutlineNode = {
  title: string;
  page: number | null;
  children: OutlineNode[];
};
export type OpenedDocument = {
  id: string;
  page_count: number;
  page_sizes: PageSize[];
  outline: OutlineNode[];
  /** Password/permission protection detected on open; re-applied on save. */
  protected: boolean;
  /** Huge scanned images: zoom above which sharp detail comes from
   * `render_region` (the whole-page render is a downsampled proxy). */
  detail_scale?: number | null;
};
export type Rendered = { width: number; height: number; data_url: string };
export type SearchHit = { page: number; rects_pt: RectPt[]; snippet: string };
export type TextRun = { text: string; rect_pt: RectPt };
export type RecentFile = { path: string; opened_at_unix: number };
export type LinkTarget = { kind: "Page"; value: number } | { kind: "Uri"; value: string };
export type PageLink = { rect_pt: RectPt; target: LinkTarget };
export type AttachmentInfo = { index: number; name: string; size: number };
export type DocMetadata = {
  title: string | null;
  author: string | null;
  subject: string | null;
  keywords: string | null;
  creator: string | null;
  producer: string | null;
  creation_date: string | null;
  modification_date: string | null;
};
export type DocPermissions = {
  security_handler: string;
  can_print_high_quality: boolean;
  can_print_low_quality: boolean;
  can_modify_content: boolean;
  can_extract_text_and_graphics: boolean;
  can_annotate: boolean;
  can_fill_forms: boolean;
  can_assemble: boolean;
};

export type FitMode = "custom" | "width" | "page" | "actual";
export type ViewMode = "single" | "continuous" | "double";
export type RibbonTab = "reader" | "comment" | "organize" | "edit" | "sign" | "convert" | "protect";
export type InsertWhere = "before" | "after" | "end";
export type MergeFile = { path: string; name: string };
export type MarkupKind = "highlight" | "underline" | "strikeout" | "squiggly";
export type ShapeKindUi =
  | "rect" | "circle" | "line" | "arrow" | "arrow2"
  | "triangle" | "diamond" | "hexagon" | "star" | "cloud" | "callout" | "check"
  | "polygon" | "polyline"
  | "freetext" | "ink" | "stamp" | "region" | "note" | "redact" | "model3d";
// Click-to-place Fill & Sign marks (signature/initials reuse the stamp tool).
export type SignTool = "check" | "cross" | "dot" | "date" | "text";
export type SignatureEntry = { kind: string; path: string };
// Digital signatures (PAdES).
export type SignatureStatus = "valid" | "invalid" | "unsupported";
export type SignatureReport = {
  field: string | null;
  signer_cn: string | null;
  signing_time: string | null;
  status: SignatureStatus;
  digest_matches: boolean;
  covers_whole_document: boolean;
  has_timestamp: boolean;
  detail: string;
};
export type WinCertEntry = { thumbprint: string; subject: string; has_private_key: boolean };
export type FormFieldKind =
  | "text" | "checkbox" | "radio" | "combobox"
  | "listbox" | "pushbutton" | "signature" | "unknown";

export type Slot = {
  wrap: HTMLDivElement;
  img: HTMLImageElement;
  selectLayer: HTMLDivElement;
  highlightLayer: HTMLDivElement;
  linkLayer: HTMLDivElement;
  noteLayer: HTMLDivElement;
  redactLayer: HTMLDivElement;
  drawLayer: SVGSVGElement;
  formLayer: HTMLDivElement;
  /** 3D annotations: posters, activation badges, the live 3D view. */
  threeDLayer: HTMLDivElement;
  placeholder: HTMLDivElement;
  loaded: boolean;
  renderingZoom: number | null;
  lastZoom: number | null;
  /** Zoom / rotation / content generation of the bitmap in `img` — reused
   * across zoom steps when the (capped) render zoom doesn't change. */
  baseZoom: number | null;
  baseRot: number | null;
  baseGen: number;
  /** Sharp render of the visible window at the current zoom, overlaid on
   * the (capped, upscaled) page bitmap. */
  detail: HTMLImageElement | null;
  detailKey: string | null;
  /** Content generation + rotation the text layer was built for, and the
   * fixed size it was laid out at; zooming only rescales it. */
  textKey: string | null;
  textW: number;
  textH: number;
};

export type DocSnapshot = {
  page_count: number;
  page_sizes: PageSize[];
  outline: OutlineNode[];
  detail_scale?: number | null;
};

export type OpenErr = {
  kind: "PasswordRequired" | "PasswordIncorrect" | "Other";
  value?: string;
};
export type UndoStatus = { undo: number; redo: number };

export type PanState = {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startScrollLeft: number;
  startScrollTop: number;
};

export type MDrag = {
  tracking: boolean; active: boolean; startX: number; startY: number;
  startIdx: number; preview: HTMLElement | null;
  target: HTMLElement | null; after: boolean;
};

export type PDrag = {
  tracking: boolean;
  active: boolean;
  startX: number;
  startY: number;
  startIdx: number;
  pages: number[];
  preview: HTMLElement | null;
  target: HTMLElement | null;
  after: boolean;
};

export type DrawState = {
  pageIdx: number;
  slot: Slot;
  startPx: { x: number; y: number };
  currentPx: { x: number; y: number };
  preview: SVGElement;
  pointerId: number;
  inkPoints?: { x: number; y: number }[];
};

export type GhostscriptInfo = {
  available: boolean;
  path: string | null;
  version: string | null;
};

export type VirtualPrinterInfo = {
  installed: boolean;
  printer_name: string;
  port_path: string;
  output_dir: string;
  ghostscript_available: boolean;
  ghostscript_version: string | null;
  supported: boolean;
};

export interface AnnotationInfo {
  id: string;
  page: number;
  subtype: string;
  rect_pt: RectPt;
  color: [number, number, number] | null;
  opacity: number;
  print: boolean;
  contents: string | null;
  author: string | null;
  modified: string | null;
  icon: string | null;
  /** Styling of a FreeText box (null for other subtypes). */
  freetext?: FreeTextStyle | null;
  /** /Subj: the shape's name ("Stern") where the subtype does not say it. */
  subject?: string | null;
}

export interface FreeTextStyle {
  font_size: number;
  text_color: [number, number, number];
  bg_color: [number, number, number] | null;
  font_family: string;
  bold: boolean;
  italic: boolean;
  rotation_quadrant: number;
}

/** One editable line of page text (see `page_text_blocks_cmd`). */
export interface TextBlock {
  objects: number[];
  text: string;
  rect_pt: RectPt;
  origin_x: number;
  origin_y: number;
  /** Reading direction on the displayed page, quarter turns clockwise. */
  dir: number;
  font_size: number;
  font_family: string;
  bold: boolean;
  italic: boolean;
  color: [number, number, number];
  invisible: boolean;
  /** The font has no Unicode mapping: `text` is empty, retype the line. */
  unreadable?: boolean;
  /** Installed font file identified as the line's font; edits embed it. */
  font_file?: string | null;
  /** Family name of that font, e.g. "Consolas". */
  font_name?: string | null;
}

export interface FormFieldInfo {
  id: string;
  page: number;
  rect_pt: RectPt;
  kind: FormFieldKind;
  value: string | null;
  options: string[] | null;
  export_value: string | null;
  readonly: boolean;
  required: boolean;
  multiline: boolean;
  password: boolean;
  max_len: number | null;
  /** Position of the widget in its page's /Annots. */
  annot_index?: number;
  /** How the field's appearance sets its text (text and choice fields). */
  text_style?: FieldTextStyle | null;
}

/** See pdf_types::FieldTextStyle. Sizes are in page points, like `rect_pt`. */
export interface FieldTextStyle {
  font_size_pt: number;
  auto_size: boolean;
  /** "Helv" | "Times" | "Cour" */
  font_family: string;
  bold: boolean;
  italic: boolean;
  color: [number, number, number];
  /** 0 = left, 1 = centred, 2 = right */
  align: number;
  padding_pt: number;
  /** Reading direction on the displayed page, quarter turns clockwise. */
  turns: number;
}

/** A PDF 3D view (ISO 32000 13.6.4), see pdf_types::ThreeDView. */
export type ThreeDView = {
  name: string;
  internal_name: string | null;
  c2w: number[] | null;
  u3d_path: string[];
  center_of_orbit: number | null;
  orthographic: boolean;
  fov: number | null;
  fov_span: string | null;
  ortho_scale: number | null;
  ortho_binding: string | null;
  background: [number, number, number] | null;
  render_mode: string | null;
  render_aux_color: [number, number, number] | null;
  lighting: string | null;
  cross_sections: {
    center: [number, number, number];
    orientation: [number | null, number | null, number | null];
    plane_opacity: number;
    plane_color: [number, number, number];
    plane_visible: boolean;
    intersection_visible: boolean;
    intersection_color: [number, number, number];
  }[];
  nodes: { name: string; visible: boolean | null; opacity: number | null; matrix: number[] | null }[];
};

/** A 3D annotation, see pdf_types::ThreeDAnnot. */
export type ThreeDAnnot = {
  id: string;
  page: number;
  rect_pt: RectPt;
  subtype: string;
  format: string;
  model_size: number;
  has_poster: boolean;
  activation: string;
  toolbar: boolean;
  model_tree: boolean;
  views: ThreeDView[];
  default_view: number | null;
  contents: string | null;
};
