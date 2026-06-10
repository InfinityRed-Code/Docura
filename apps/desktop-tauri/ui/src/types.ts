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
export type RibbonTab = "reader" | "edit" | "forms" | "sign" | "convert" | "organize" | "protect";
export type EditMode = "select" | "delete";
export type InsertWhere = "before" | "after" | "end";
export type MergeFile = { path: string; name: string };
export type MarkupKind = "highlight" | "underline" | "strikeout";
export type ShapeKindUi =
  | "rect" | "circle" | "line" | "arrow"
  | "freetext" | "ink" | "stamp" | "region" | "note";
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
  drawLayer: SVGSVGElement;
  formLayer: HTMLDivElement;
  placeholder: HTMLDivElement;
  loaded: boolean;
  renderingZoom: number | null;
  lastZoom: number | null;
};

export type DocSnapshot = {
  page_count: number;
  page_sizes: PageSize[];
  outline: OutlineNode[];
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
}
