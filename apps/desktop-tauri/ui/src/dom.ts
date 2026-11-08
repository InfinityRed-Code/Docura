class DomRefs {
  // Reader chrome
  statusEl!: HTMLSpanElement;
  viewer!: HTMLDivElement;
  pageList!: HTMLDivElement;
  zoomSelect!: HTMLSelectElement;
  zoomCustomOpt!: HTMLOptionElement;
  viewModeSelect!: HTMLSelectElement;
  pageInput!: HTMLInputElement;
  pageTotal!: HTMLSpanElement;

  // Panels
  searchPanel!: HTMLDivElement;
  searchInput!: HTMLInputElement;
  searchCase!: HTMLInputElement;
  searchCount!: HTMLSpanElement;
  thumbsPanel!: HTMLDivElement;
  outlinePanel!: HTMLDivElement;
  recentMenu!: HTMLDivElement;
  dropOverlay!: HTMLDivElement;
  attachmentsPanel!: HTMLDivElement;
  propsOverlay!: HTMLDivElement;
  propsInfoPanel!: HTMLDivElement;
  propsPermsPanel!: HTMLDivElement;
  sidebarEl!: HTMLElement;
  ribbonBody!: HTMLDivElement;

  // Undo/Redo
  undoBtn!: HTMLButtonElement;
  redoBtn!: HTMLButtonElement;

  // Watermark / Header-Footer overlays
  wmOverlay!: HTMLDivElement;
  hfOverlay!: HTMLDivElement;
  wmSectionText!: HTMLDivElement;
  wmSectionImage!: HTMLDivElement;
  wmTabText!: HTMLButtonElement;
  wmTabImage!: HTMLButtonElement;

  // Convert pane
  convertPane!: HTMLDivElement;
  convertList!: HTMLDivElement;
  convertCommitBtn!: HTMLButtonElement;
  convertLevelEl!: HTMLSelectElement;
  convertOutLabel!: HTMLSpanElement;

  // Merge pane
  mergePane!: HTMLDivElement;
  mergeList!: HTMLDivElement;
  mergeCommitBtn!: HTMLButtonElement;

  // Settings / Ghostscript / Virtual printer
  settingsOverlay!: HTMLDivElement;
  gsStatusEl!: HTMLDivElement;
  gsInstallEl!: HTMLDivElement;
  pdfaLevelEl!: HTMLSelectElement;
  vpStatusEl!: HTMLDivElement;
  vpPathRowEl!: HTMLDivElement;
  vpOutRowEl!: HTMLDivElement;
  vpWarnEl!: HTMLDivElement;
  vpInstallBtn!: HTMLButtonElement;
  vpUninstallBtn!: HTMLButtonElement;
  vpOpenFolderBtn!: HTMLButtonElement;

  // Annotation tools
  annotColorInput!: HTMLInputElement;
  printToggleBtn!: HTMLButtonElement;

  // Comments
  commentPanelEl!: HTMLElement;
  commentListEl!: HTMLDivElement;
  commentKindFilter!: HTMLSelectElement;
  commentAuthorFilter!: HTMLInputElement;
  commentHasContent!: HTMLInputElement;

  // Forms
  formHintEl!: HTMLSpanElement;
  formPrevBtn!: HTMLButtonElement;
  formNextBtn!: HTMLButtonElement;
  formHighlightBtn!: HTMLButtonElement;
}

export const dom = new DomRefs();

export function initDom() {
  dom.statusEl = document.getElementById("status") as HTMLSpanElement;
  dom.viewer = document.getElementById("viewer") as HTMLDivElement;
  dom.pageList = document.getElementById("pageList") as HTMLDivElement;
  dom.zoomSelect = document.getElementById("zoomSelect") as HTMLSelectElement;
  dom.zoomCustomOpt = document.getElementById("zoomCustomOpt") as HTMLOptionElement;
  dom.viewModeSelect = document.getElementById("viewModeSelect") as HTMLSelectElement;
  dom.pageInput = document.getElementById("pageInput") as HTMLInputElement;
  dom.pageTotal = document.getElementById("pageTotal") as HTMLSpanElement;
  dom.searchPanel = document.getElementById("searchPanel") as HTMLDivElement;
  dom.searchInput = document.getElementById("searchInput") as HTMLInputElement;
  dom.searchCase = document.getElementById("searchCase") as HTMLInputElement;
  dom.searchCount = document.getElementById("searchCount") as HTMLSpanElement;
  dom.thumbsPanel = document.getElementById("thumbsPanel") as HTMLDivElement;
  dom.outlinePanel = document.getElementById("outlinePanel") as HTMLDivElement;
  dom.recentMenu = document.getElementById("recentMenu") as HTMLDivElement;
  dom.dropOverlay = document.getElementById("dropOverlay") as HTMLDivElement;
  dom.attachmentsPanel = document.getElementById("attachmentsPanel") as HTMLDivElement;
  dom.propsOverlay = document.getElementById("propsOverlay") as HTMLDivElement;
  dom.propsInfoPanel = document.getElementById("propsInfo") as HTMLDivElement;
  dom.propsPermsPanel = document.getElementById("propsPerms") as HTMLDivElement;
  dom.sidebarEl = document.getElementById("sidebar") as HTMLElement;
  dom.ribbonBody = document.querySelector(".ribbonBody") as HTMLDivElement;

  dom.undoBtn = document.getElementById("undoBtn") as HTMLButtonElement;
  dom.redoBtn = document.getElementById("redoBtn") as HTMLButtonElement;

  dom.wmOverlay = document.getElementById("wmOverlay") as HTMLDivElement;
  dom.hfOverlay = document.getElementById("hfOverlay") as HTMLDivElement;
  dom.wmSectionText = document.getElementById("wmSectionText") as HTMLDivElement;
  dom.wmSectionImage = document.getElementById("wmSectionImage") as HTMLDivElement;
  dom.wmTabText = document.getElementById("wmTabText") as HTMLButtonElement;
  dom.wmTabImage = document.getElementById("wmTabImage") as HTMLButtonElement;

  dom.convertPane = document.getElementById("convertPane") as HTMLDivElement;
  dom.convertList = document.getElementById("convertList") as HTMLDivElement;
  dom.convertCommitBtn = document.getElementById("convertCommitBtn") as HTMLButtonElement;
  dom.convertLevelEl = document.getElementById("convertLevel") as HTMLSelectElement;
  dom.convertOutLabel = document.getElementById("convertOutLabel") as HTMLSpanElement;

  dom.mergePane = document.getElementById("mergePane") as HTMLDivElement;
  dom.mergeList = document.getElementById("mergeList") as HTMLDivElement;
  dom.mergeCommitBtn = document.getElementById("mergeCommitBtn") as HTMLButtonElement;

  dom.settingsOverlay = document.getElementById("settingsOverlay") as HTMLDivElement;
  dom.gsStatusEl = document.getElementById("gsStatus") as HTMLDivElement;
  dom.gsInstallEl = document.getElementById("gsInstall") as HTMLDivElement;
  dom.pdfaLevelEl = document.getElementById("pdfaLevel") as HTMLSelectElement;
  dom.vpStatusEl = document.getElementById("vpStatus") as HTMLDivElement;
  dom.vpPathRowEl = document.getElementById("vpPathRow") as HTMLDivElement;
  dom.vpOutRowEl = document.getElementById("vpOutRow") as HTMLDivElement;
  dom.vpWarnEl = document.getElementById("vpWarn") as HTMLDivElement;
  dom.vpInstallBtn = document.getElementById("vpInstallBtn") as HTMLButtonElement;
  dom.vpUninstallBtn = document.getElementById("vpUninstallBtn") as HTMLButtonElement;
  dom.vpOpenFolderBtn = document.getElementById("vpOpenFolderBtn") as HTMLButtonElement;

  dom.annotColorInput = document.getElementById("annotColor") as HTMLInputElement;
  dom.printToggleBtn = document.getElementById("printToggleBtn") as HTMLButtonElement;

  dom.commentPanelEl = document.getElementById("commentPanel") as HTMLElement;
  dom.commentListEl = document.getElementById("commentList") as HTMLDivElement;
  dom.commentKindFilter = document.getElementById("commentKindFilter") as HTMLSelectElement;
  dom.commentAuthorFilter = document.getElementById("commentAuthorFilter") as HTMLInputElement;
  dom.commentHasContent = document.getElementById("commentHasContent") as HTMLInputElement;

  dom.formHintEl = document.getElementById("formHint") as HTMLSpanElement;
  dom.formPrevBtn = document.getElementById("formPrevBtn") as HTMLButtonElement;
  dom.formNextBtn = document.getElementById("formNextBtn") as HTMLButtonElement;
  dom.formHighlightBtn = document.getElementById("formHighlightBtn") as HTMLButtonElement;
}

export function setStatus(s: string) {
  dom.statusEl.textContent = s;
}
