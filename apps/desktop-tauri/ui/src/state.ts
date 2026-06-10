import type {
  AnnotationInfo, DrawState, FitMode, FormFieldInfo, GhostscriptInfo,
  MDrag, MarkupKind, MergeFile, OutlineNode, PDrag, PageLink, PageSize,
  PanState, RibbonTab, EditMode, SearchHit, ShapeKindUi, SignTool,
  SignatureEntry, Slot, TextRun, ViewMode,
} from "./types";

export const state = {
  // Document
  currentDoc: null as string | null,
  currentDocPath: null as string | null,
  pageCount: 0,
  pageSizes: [] as PageSize[],
  outline: [] as OutlineNode[],
  currentPage: 0,

  // View
  ribbonTab: "reader" as RibbonTab,
  editMode: "select" as EditMode,
  zoom: 1.0,
  fitMode: "page" as FitMode,
  viewMode: "continuous" as ViewMode,
  docRotation: 0,
  pageRotation: new Map<number, number>(),
  darkMode: false,
  wheelZoomMode: false,
  panState: null as PanState | null,
  presentationActive: false,

  // Selection
  selectedPages: new Set<number>(),
  selectionAnchor: null as number | null,

  // Render pipeline
  slots: [] as Slot[],
  observer: null as IntersectionObserver | null,
  renderQueue: new Set<number>(),
  renderRunning: false,
  textRunsCache: new Map<number, TextRun[]>(),
  linksCache: new Map<number, PageLink[]>(),

  // Search
  hits: [] as SearchHit[],
  currentHit: -1,
  searchDebounce: null as number | null,

  // Thumbs
  thumbEls: [] as HTMLImageElement[],
  thumbWraps: [] as HTMLElement[],
  thumbObserver: null as IntersectionObserver | null,
  thumbLoaded: new Set<number>(),
  thumbInFlight: new Set<number>(),

  // Watermark
  wmMode: "text" as "text" | "image",

  // Convert pane
  convertFiles: [] as MergeFile[],
  convertOutDir: null as string | null,

  // Merge pane
  mergeMode: false,
  mergeFiles: [] as MergeFile[],
  mdrag: makeMdrag(),
  pdrag: makePdrag(),
  lastWasDrag: false,

  // Settings
  gsInfo: { available: false, path: null, version: null } as GhostscriptInfo,

  // Annotations
  annotTool: null as MarkupKind | null,
  shapeTool: null as ShapeKindUi | null,
  stampImagePath: null as string | null,
  printAnnotations: true,
  drawState: null as DrawState | null,
  cachedAnnotations: [] as AnnotationInfo[],

  // Forms
  formFields: [] as FormFieldInfo[],
  formFieldsByPage: new Map<number, FormFieldInfo[]>(),
  invalidFieldIds: new Set<string>(),

  // Fill & Sign
  signTool: null as SignTool | null,
  signatures: [] as SignatureEntry[],
};

export function makeMdrag(): MDrag {
  return {
    tracking: false, active: false, startX: 0, startY: 0,
    startIdx: -1, preview: null, target: null, after: false,
  };
}
export function makePdrag(): PDrag {
  return {
    tracking: false, active: false, startX: 0, startY: 0, startIdx: -1,
    pages: [], preview: null, target: null, after: false,
  };
}

// Callback registry for cross-module hooks (breaks import cycles).
// Set by the module that owns the implementation, called by consumers.
export const callbacks = {
  renderFormFor: (_idx: number) => {},
  renderNotesFor: (_idx: number) => {},
  notifyAnnotationsChanged: async () => {},
  refreshFormFields: async () => {},
  refreshComments: async () => {},
};
