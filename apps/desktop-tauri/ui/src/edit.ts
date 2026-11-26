import { invoke } from "@tauri-apps/api/core";
import { pdfaErrorText, pdfaLang, syncLevelSelect } from "./pdfa";
import { open, save, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import type {
  DocSnapshot, InsertWhere, MergeFile, OpenedDocument, RibbonTab,
} from "./types";
import { state, makeMdrag, makePdrag } from "./state";
import { t } from "./i18n";
import { dom, setStatus } from "./dom";
import { reflow } from "./viewer";
import {
  applyDocSnapshot, applyOpenedDoc, armThumbObserver, clearSelection,
  confirmDiscardChanges, docBaseName, fileNameFromPath, handleThumbSelect,
  refreshSelectionUi,
} from "./doc";
import { disarmAllTools } from "./annot";

// ---------- Page operations ----------

function pagesLabel(pages: number[]): string {
  return pages.length === 1
    ? t("page.one", { n: pages[0] + 1 })
    : t("page.range", { count: pages.length, from: pages[0] + 1, to: pages[pages.length - 1] + 1 });
}

export async function deleteSelectedPages() {
  if (!state.currentDoc || state.selectedPages.size === 0) return;
  const pages = [...state.selectedPages].sort((a, b) => a - b);
  if (pages.length >= state.pageCount) {
    setStatus(t("st.cantDeleteAll"));
    return;
  }
  const label = pagesLabel(pages);
  const ok = await confirmDialog(t("dlg.deleteMsg", { label }), {
    title: t("dlg.deleteTitle"),
    kind: "warning",
    okLabel: t("btn.delete"),
    cancelLabel: t("tip.cancel"),
  });
  if (!ok) return;
  setStatus(t("st.deleting", { label }));
  try {
    const snap = await invoke<DocSnapshot>("delete_pages", { document: state.currentDoc, pages });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(t("st.deleted", { label }));
  } catch (e) {
    setStatus(t("st.deleteError", { e: String(e) }));
  }
}

export async function extractSelectedPages() {
  if (!state.currentDoc || state.selectedPages.size === 0) return;
  const pages = [...state.selectedPages].sort((a, b) => a - b);
  const suffix = pages.length === 1
    ? `page_${pages[0] + 1}`
    : `pages_${pages[0] + 1}-${pages[pages.length - 1] + 1}`;
  const path = await save({
    title: t("dlg.extractTitle"),
    defaultPath: `${docBaseName()}_${suffix}.pdf`,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!path) return;
  const label = pagesLabel(pages);
  setStatus(t("st.extracting", { label }));
  try {
    await invoke("extract_pages", { document: state.currentDoc, pages, path });
    setStatus(t("st.extracted", { label, path }));
  } catch (e) {
    setStatus(t("st.extractError", { e: String(e) }));
  }
}

export async function rotateSelectionOrCurrent(degrees: number) {
  if (!state.currentDoc) return;
  const pages = state.selectedPages.size > 0
    ? [...state.selectedPages].sort((a, b) => a - b)
    : [state.currentPage];
  const label = pagesLabel(pages);
  setStatus(t("st.rotating", { label }));
  try {
    const snap = await invoke<DocSnapshot>("rotate_pages", {
      document: state.currentDoc,
      pages,
      degrees,
    });
    applyDocSnapshot(snap);
    // Rotation keeps the page order, so the selection stays meaningful.
    refreshSelectionUi();
    setStatus(t("st.rotated", { label, deg: degrees }));
  } catch (e) {
    setStatus(t("st.rotateError", { e: String(e) }));
  }
}

// ---------- Print ----------

export async function triggerPrint() {
  if (!state.currentDoc) return;
  setStatus(t("st.printDialog"));
  try {
    // Opens the native Windows print dialog (printer selection, copies, …) on
    // the Rust side and rasterises the PDF onto the chosen printer.
    const printed = await invoke<boolean>("print_document", { document: state.currentDoc });
    setStatus(printed ? t("st.printSent") : t("st.printCancelled"));
  } catch (e) {
    setStatus(t("st.printError", { e: String(e) }));
  }
}

// ---------- Ribbon ----------

export function setRibbonTab(tab: RibbonTab) {
  const changed = state.ribbonTab !== tab;
  state.ribbonTab = tab;
  dom.ribbonBody.dataset.active = tab;
  dom.ribbonBody.querySelectorAll<HTMLElement>(":scope > [data-ribbon]").forEach((el) => {
    el.classList.toggle("offTab", el.dataset.ribbon !== tab);
  });
  document.querySelectorAll(".ribbonTab[data-ribbon-tab]").forEach((el) => {
    const on = (el as HTMLElement).dataset.ribbonTab === tab;
    el.classList.toggle("active", on);
    el.setAttribute("aria-selected", String(on));
  });
  // A tool armed on one tab must never keep acting on another (e.g. the
  // eraser deleting annotations while the user is on "Start").
  if (changed) disarmAllTools();
  if (state.mergeMode && tab !== "organize") exitMergeMode();
  if (tab !== "convert" && document.body.classList.contains("convert")) exitConvertMode();
  const wasOrganize = document.body.classList.contains("organize");
  document.body.classList.toggle("organize", tab === "organize");
  if (tab === "organize") {
    armThumbObserver();
  } else if (wasOrganize) {
    // Page selection only exists in the page grid.
    clearSelection();
    if (state.currentDoc) requestAnimationFrame(() => reflow());
  }
}

// ---------- Watermark ----------

export function setWmMode(mode: "text" | "image") {
  state.wmMode = mode;
  dom.wmTabText.classList.toggle("active", mode === "text");
  dom.wmTabImage.classList.toggle("active", mode === "image");
  dom.wmSectionText.classList.toggle("hidden", mode !== "text");
  dom.wmSectionImage.classList.toggle("hidden", mode !== "image");
}

async function pickImageWatermark() {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: t("lbl.images"), extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
    title: t("dlg.pickWmImage"),
  });
  if (picked && typeof picked === "string") {
    (document.getElementById("iwPath") as HTMLInputElement).value = picked;
  }
}

async function runWatermark() {
  if (!state.currentDoc) return;
  if (state.wmMode === "text") {
    const text = (document.getElementById("wmText") as HTMLInputElement).value.trim();
    if (!text) { setStatus(t("st.wmEmpty")); return; }
    const args = {
      text,
      font_size: parseFloat((document.getElementById("wmFontSize") as HTMLInputElement).value) || 72,
      rotation_deg: parseFloat((document.getElementById("wmRot") as HTMLInputElement).value) || 0,
      opacity: parseFloat((document.getElementById("wmOpacity") as HTMLInputElement).value) || 0.25,
      gray: parseFloat((document.getElementById("wmGray") as HTMLInputElement).value),
    };
    if (!Number.isFinite(args.gray)) args.gray = 0.5;
    dom.wmOverlay.classList.add("hidden");
    setStatus(t("st.wmApplying"));
    try {
      const snap = await invoke<DocSnapshot>("add_watermark", { document: state.currentDoc, args });
      applyDocSnapshot(snap);
      setStatus(t("st.wmApplied"));
    } catch (e) {
      setStatus(t("st.wmError", { e: String(e) }));
    }
  } else {
    const image_path = (document.getElementById("iwPath") as HTMLInputElement).value.trim();
    if (!image_path) { setStatus(t("st.pickImageFirst")); return; }
    const args = {
      image_path,
      scale_frac: parseFloat((document.getElementById("iwScale") as HTMLInputElement).value) || 0.4,
      rotation_deg: parseFloat((document.getElementById("iwRot") as HTMLInputElement).value) || 0,
      opacity: parseFloat((document.getElementById("iwOpacity") as HTMLInputElement).value) || 0.5,
      anchor: (document.getElementById("iwAnchor") as HTMLSelectElement).value || "center",
      margin_pt: parseFloat((document.getElementById("iwMargin") as HTMLInputElement).value) || 36,
    };
    dom.wmOverlay.classList.add("hidden");
    setStatus(t("st.wmApplying"));
    try {
      const snap = await invoke<DocSnapshot>("add_image_watermark", { document: state.currentDoc, args });
      applyDocSnapshot(snap);
      setStatus(t("st.wmApplied"));
    } catch (e) {
      setStatus(t("st.wmError", { e: String(e) }));
    }
  }
}

async function runHeaderFooter() {
  if (!state.currentDoc) return;
  const v = (id: string) => (document.getElementById(id) as HTMLInputElement).value;
  const args = {
    header_left: v("hfHL"),
    header_center: v("hfHC"),
    header_right: v("hfHR"),
    footer_left: v("hfFL"),
    footer_center: v("hfFC"),
    footer_right: v("hfFR"),
    font_size: parseFloat(v("hfFontSize")) || 10,
    margin_pt: parseFloat(v("hfMargin")) || 24,
    first_page_number: parseInt(v("hfFirst"), 10) || 1,
    gray: parseFloat(v("hfGray")),
  };
  if (!Number.isFinite(args.gray)) args.gray = 0;
  const anyText = [args.header_left, args.header_center, args.header_right, args.footer_left, args.footer_center, args.footer_right].some((s) => s.trim() !== "");
  if (!anyText) { setStatus(t("st.hfEmpty")); return; }
  dom.hfOverlay.classList.add("hidden");
  setStatus(t("st.hfApplying"));
  try {
    const snap = await invoke<DocSnapshot>("add_header_footer", { document: state.currentDoc, args });
    applyDocSnapshot(snap);
    setStatus(t("st.hfApplied"));
  } catch (e) {
    setStatus(t("st.hfError", { e: String(e) }));
  }
}

// ---------- Split ----------
//
// window.prompt() does not work inside the WebView, so splitting runs
// through its own small dialog: "every N pages" (the common case) or free
// page ranges. Both are turned into the range spec the backend expects.

function splitEl<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

/** Parse "1-3, 4, 5-9" into [from, to] pairs (1-based, inclusive). */
function parseRanges(spec: string, total: number): [number, number][] | null {
  const parts = spec.split(/[,;]/).map((s) => s.trim()).filter(Boolean);
  if (parts.length === 0) return null;
  const out: [number, number][] = [];
  for (const p of parts) {
    const m = p.match(/^(\d+)\s*(?:-\s*(\d+))?$/);
    if (!m) return null;
    const a = parseInt(m[1], 10);
    const b = m[2] ? parseInt(m[2], 10) : a;
    if (a < 1 || b < a || b > total) return null;
    out.push([a, b]);
  }
  return out;
}

function splitPlan(): [number, number][] | null {
  const total = state.pageCount;
  if (splitEl<HTMLInputElement>("splitModeEvery").checked) {
    const n = parseInt(splitEl<HTMLInputElement>("splitEvery").value, 10);
    if (!Number.isFinite(n) || n < 1) return null;
    const out: [number, number][] = [];
    for (let a = 1; a <= total; a += n) out.push([a, Math.min(total, a + n - 1)]);
    return out;
  }
  return parseRanges(splitEl<HTMLInputElement>("splitRanges").value, total);
}

function updateSplitPreview() {
  const plan = splitPlan();
  const preview = splitEl<HTMLSpanElement>("splitPreview");
  const commit = splitEl<HTMLButtonElement>("splitCommit");
  const ranges = splitEl<HTMLInputElement>("splitRanges");
  const every = splitEl<HTMLInputElement>("splitEvery");
  const rangesMode = splitEl<HTMLInputElement>("splitModeRanges").checked;
  ranges.disabled = !rangesMode;
  every.disabled = rangesMode;
  commit.disabled = !plan || plan.length < 1;
  if (!plan) {
    preview.textContent = t("split.invalid", { n: state.pageCount });
    preview.style.color = "var(--danger)";
    return;
  }
  const fmt = ([a, b]: [number, number]) => (a === b ? `${a}` : `${a}–${b}`);
  const shown = plan.slice(0, 6).map(fmt).join(", ") + (plan.length > 6 ? ", …" : "");
  preview.textContent = t("split.preview", { n: plan.length, parts: shown });
  preview.style.color = "";
}

function openSplitDialog() {
  if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
  const half = Math.max(1, Math.ceil(state.pageCount / 2));
  const ranges = splitEl<HTMLInputElement>("splitRanges");
  ranges.value = state.pageCount > 1 ? `1-${half}, ${half + 1}-${state.pageCount}` : "1";
  ranges.placeholder = `1-3, 4, 5-${state.pageCount}`;
  // The label carries an example with the real page count.
  const lbl = ranges.closest("label");
  if (lbl?.firstChild) lbl.firstChild.textContent = t("split.rangesLabel", { n: state.pageCount });
  splitEl<HTMLInputElement>("splitEvery").max = String(state.pageCount);
  updateSplitPreview();
  splitEl<HTMLDivElement>("splitOverlay").classList.remove("hidden");
}

function closeSplitDialog() {
  splitEl<HTMLDivElement>("splitOverlay").classList.add("hidden");
}

async function runSplit() {
  const plan = splitPlan();
  if (!state.currentDoc || !plan) return;
  const spec = plan.map(([a, b]) => (a === b ? `${a}` : `${a}-${b}`)).join(",");
  const outDir = await open({ directory: true, title: t("split.outDir") });
  if (!outDir || typeof outDir !== "string") return;
  closeSplitDialog();
  setStatus(t("st.splitting"));
  try {
    const res = await invoke<{ files: string[] }>("split_pdf", {
      document: state.currentDoc,
      ranges: spec,
      outDir,
      baseName: docBaseName(),
    });
    setStatus(t("st.split", { n: res.files.length, dir: outDir }));
  } catch (e) {
    setStatus(t("st.splitError", { e: String(e) }));
  }
}

// ---------- Convert pane (PDF → PDF/A, batch) ----------

export function enterConvertMode() {
  if (state.ribbonTab !== "convert") setRibbonTab("convert");
  document.body.classList.add("convert");
  syncLevelSelect(dom.convertLevelEl);
  if (dom.pdfaLevelEl && dom.pdfaLevelEl.value !== "off") {
    dom.convertLevelEl.value = dom.pdfaLevelEl.value;
  }
  renderConvertList();
  updateConvertOutLabel();
}
function exitConvertMode() {
  document.body.classList.remove("convert");
  state.convertFiles = [];
  renderConvertList();
  if (state.currentDoc) requestAnimationFrame(() => reflow());
}

/** One removable file tile for the merge / convert panes. */
function fileTile(f: MergeFile, i: number): HTMLElement {
  const tile = document.createElement("div");
  tile.className = "mergeTile";
  tile.dataset.idx = String(i);
  const idx = document.createElement("span");
  idx.className = "mergeIndex";
  idx.textContent = String(i + 1);
  const rm = document.createElement("button");
  rm.className = "mergeRemove";
  rm.title = t("tip.remove");
  rm.dataset.idx = String(i);
  rm.textContent = "✕";
  const name = document.createElement("div");
  name.className = "mergeName";
  name.textContent = f.name;
  const path = document.createElement("div");
  path.className = "mergePath";
  path.textContent = f.path;
  tile.append(idx, rm, name, path);
  return tile;
}

function emptyTile(text: string): HTMLElement {
  const empty = document.createElement("div");
  empty.className = "mergeEmpty";
  empty.textContent = text;
  return empty;
}

function renderConvertList() {
  dom.convertList.innerHTML = "";
  if (state.convertFiles.length === 0) {
    dom.convertList.appendChild(emptyTile(t("convert.empty")));
  } else {
    state.convertFiles.forEach((f, i) => dom.convertList.appendChild(fileTile(f, i)));
  }
  dom.convertCommitBtn.disabled = state.convertFiles.length === 0 || !state.convertOutDir;
}
function updateConvertOutLabel() {
  dom.convertOutLabel.textContent = state.convertOutDir
    ? t("convert.outputSet", { dir: state.convertOutDir })
    : t("convert.output");
}
function addConvertPaths(paths: string[]) {
  const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
  for (const p of pdfs) {
    if (state.convertFiles.some((f: MergeFile) => f.path === p)) continue;
    state.convertFiles.push({ path: p, name: fileNameFromPath(p) });
  }
  renderConvertList();
}

// ---------- Merge pane ----------

function enterMergeMode() {
  state.mergeMode = true;
  state.mergeFiles = [];
  document.body.classList.add("merge");
  setRibbonTab("organize");
  renderMergeList();
}
function exitMergeMode() {
  state.mergeMode = false;
  state.mergeFiles = [];
  document.body.classList.remove("merge", "mergeDrop");
  renderMergeList();
}
function renderMergeList() {
  dom.mergeList.innerHTML = "";
  if (state.mergeFiles.length === 0) {
    dom.mergeList.appendChild(emptyTile(t("merge.empty")));
  } else {
    state.mergeFiles.forEach((f, i) => dom.mergeList.appendChild(fileTile(f, i)));
  }
  dom.mergeCommitBtn.disabled = state.mergeFiles.length < 2;
}
export function addMergePaths(paths: string[]) {
  const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
  for (const p of pdfs) {
    if (state.mergeFiles.some((f: MergeFile) => f.path === p)) continue;
    state.mergeFiles.push({ path: p, name: fileNameFromPath(p) });
  }
  renderMergeList();
}

// ---------- Insert PDF (from another file) ----------

async function runInsertPdf(where: InsertWhere) {
  if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
  const picked = await open({
    multiple: true,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  if (paths.length === 0) return;
  const at = where === "end" ? state.pageCount
    : where === "before" ? state.currentPage
    : state.currentPage + 1;
  setStatus(t("st.inserting", { n: paths.length }));
  try {
    const snap = await invoke<DocSnapshot>("insert_pdf_files", {
      document: state.currentDoc, paths, atIndex: at,
    });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(t("st.inserted", { n: paths.length, pages: snap.page_count }));
  } catch (err) {
    setStatus(t("st.insertError", { e: fmtMutationError(err) }));
  }
}

/** Map backend error markers to localized messages. */
export function fmtMutationError(err: unknown): string {
  const s = String(err);
  const m = s.match(/ENCRYPTED_INPUT:(.+)$/);
  if (m) return t("st.encryptedInput", { file: m[1] });
  return s;
}

// ---------- Drag-to-reorder helpers ----------

function findMergeDropTarget(x: number, y: number): { el: HTMLElement; after: boolean } | null {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  const hit = el?.closest(".mergeTile") as HTMLElement | null;
  const all = Array.from(dom.mergeList.querySelectorAll<HTMLElement>(".mergeTile"));
  if (hit) {
    const r = hit.getBoundingClientRect();
    return { el: hit, after: x > r.left + r.width / 2 };
  }
  let best: HTMLElement | null = null;
  let bestRect: DOMRect | null = null;
  let bestDist = Infinity;
  for (const tile of all) {
    if (tile.classList.contains("dragging")) continue;
    const r = tile.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const dx = cx - x;
    const dy = cy - y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = tile; bestRect = r; }
  }
  if (!best || !bestRect) return null;
  return { el: best, after: x > bestRect.left + bestRect.width / 2 };
}

function resetMdrag() {
  if (state.mdrag.preview) state.mdrag.preview.remove();
  document.querySelectorAll(".mergeTile.dragging, .mergeTile.dropBefore, .mergeTile.dropAfter")
    .forEach((el) => el.classList.remove("dragging", "dropBefore", "dropAfter"));
  state.mdrag = makeMdrag();
}

function findDropTarget(x: number, y: number): { el: HTMLElement; after: boolean } | null {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  const hit = el?.closest(".thumb") as HTMLElement | null;
  if (hit) {
    const r = hit.getBoundingClientRect();
    return { el: hit, after: x > r.left + r.width / 2 };
  }
  let best: HTMLElement | null = null;
  let bestRect: DOMRect | null = null;
  let bestDist = Infinity;
  for (const w of state.thumbWraps) {
    if (w.classList.contains("dragging")) continue;
    const r = w.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const dx = cx - x;
    const dy = cy - y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = w; bestRect = r; }
  }
  if (!best || !bestRect) return null;
  return { el: best, after: x > bestRect.left + bestRect.width / 2 };
}

function resetPdrag() {
  if (state.pdrag.preview) state.pdrag.preview.remove();
  document.body.classList.remove("pageDragging");
  state.thumbWraps.forEach((w) => w.classList.remove("dragging"));
  document.querySelectorAll(".thumb.dropBefore, .thumb.dropAfter").forEach((el) => {
    el.classList.remove("dropBefore");
    el.classList.remove("dropAfter");
  });
  state.pdrag = makePdrag();
}

/** Disable a button while an async action runs (guards double clicks). */
async function busy(btn: HTMLButtonElement, fn: () => Promise<void>) {
  if (btn.dataset.busy) return;
  btn.dataset.busy = "1";
  const wasDisabled = btn.disabled;
  btn.disabled = true;
  try {
    await fn();
  } finally {
    delete btn.dataset.busy;
    btn.disabled = wasDisabled;
  }
}

// ---------- Init ----------

export function initEdit() {
  // Print
  document.getElementById("printBtn")!.addEventListener("click", triggerPrint);

  // Ribbon tabs
  document.querySelectorAll(".ribbonTab[data-ribbon-tab]").forEach((el) => {
    el.addEventListener("click", () => {
      const tab = (el as HTMLElement).dataset.ribbonTab as RibbonTab;
      setRibbonTab(tab);
    });
  });
  // Initial paint: hide every group that isn't on the start tab.
  state.ribbonTab = "reader";
  setRibbonTab("reader");

  // Page actions (Pages tab)
  document.getElementById("deleteSelectionBtn")!.addEventListener("click", deleteSelectedPages);
  document.getElementById("extractSelectionBtn")!.addEventListener("click", extractSelectedPages);
  document.getElementById("rotateLeftBtn")!.addEventListener("click", () => void rotateSelectionOrCurrent(-90));
  document.getElementById("rotateRightBtn")!.addEventListener("click", () => void rotateSelectionOrCurrent(90));

  // Generic ribbon dropdowns: any `.menuTrigger` that sits in a `.menuHost`
  // toggles the `.popover` in that host. Clicking a [role=menuitem] or anywhere
  // outside closes open popovers; clicks on popover form controls keep it open.
  const closePopovers = (except?: Element | null) => {
    document.querySelectorAll<HTMLElement>(".menuHost > .popover:not(.hidden)").forEach((p) => {
      if (p === except) return;
      p.classList.add("hidden");
      p.closest(".menuHost")?.querySelector(".menuTrigger")?.setAttribute("aria-expanded", "false");
    });
  };
  document.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const trigger = target.closest(".menuTrigger") as HTMLElement | null;
    const pop = trigger?.closest(".menuHost")?.querySelector<HTMLElement>(":scope > .popover") ?? null;
    if (trigger && pop) {
      e.stopPropagation();
      const willOpen = pop.classList.contains("hidden");
      closePopovers(willOpen ? null : pop);
      pop.classList.toggle("hidden", !willOpen);
      trigger.setAttribute("aria-expanded", String(willOpen));
      return;
    }
    if (target.closest(".menuHost > .popover")) {
      if (target.closest('[role="menuitem"]')) closePopovers();
      return;
    }
    closePopovers();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && document.querySelector(".menuHost > .popover:not(.hidden)")) {
      e.stopPropagation();
      closePopovers();
    }
  }, true);
  document.getElementById("insertBeforeBtn")!.addEventListener("click", () => runInsertPdf("before"));
  document.getElementById("insertAfterBtn")!.addEventListener("click", () => runInsertPdf("after"));
  document.getElementById("insertEndBtn")!.addEventListener("click", () => runInsertPdf("end"));

  // Merge / split
  document.getElementById("mergePdfsBtn")!.addEventListener("click", () => enterMergeMode());
  document.getElementById("splitPdfBtn")!.addEventListener("click", openSplitDialog);
  document.getElementById("splitClose")!.addEventListener("click", closeSplitDialog);
  document.getElementById("splitCancel")!.addEventListener("click", closeSplitDialog);
  document.getElementById("splitCommit")!.addEventListener("click", () => void runSplit());
  for (const id of ["splitModeEvery", "splitModeRanges", "splitEvery", "splitRanges"]) {
    document.getElementById(id)!.addEventListener("input", updateSplitPreview);
    document.getElementById(id)!.addEventListener("change", updateSplitPreview);
  }

  // Watermark overlay
  dom.wmTabText.addEventListener("click", () => setWmMode("text"));
  dom.wmTabImage.addEventListener("click", () => setWmMode("image"));
  document.getElementById("watermarkBtn")!.addEventListener("click", () => {
    if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
    dom.wmOverlay.classList.remove("hidden");
    if (state.wmMode === "text") (document.getElementById("wmText") as HTMLInputElement).focus();
  });
  document.getElementById("wmClose")!.addEventListener("click", () => dom.wmOverlay.classList.add("hidden"));
  document.getElementById("wmCancel")!.addEventListener("click", () => dom.wmOverlay.classList.add("hidden"));
  document.getElementById("wmCommit")!.addEventListener("click", () => void runWatermark());
  document.getElementById("iwPick")!.addEventListener("click", () => void pickImageWatermark());

  // Header/footer overlay
  document.getElementById("headerFooterBtn")!.addEventListener("click", () => {
    if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
    dom.hfOverlay.classList.remove("hidden");
  });
  document.getElementById("hfClose")!.addEventListener("click", () => dom.hfOverlay.classList.add("hidden"));
  document.getElementById("hfCancel")!.addEventListener("click", () => dom.hfOverlay.classList.add("hidden"));
  document.getElementById("hfCommit")!.addEventListener("click", () => void runHeaderFooter());

  // Convert pane wiring
  document.getElementById("pdfaBatchBtn")!.addEventListener("click", enterConvertMode);
  document.getElementById("convertAddBtn")!.addEventListener("click", async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return;
    addConvertPaths(Array.isArray(picked) ? picked : [picked]);
  });
  document.getElementById("convertOutBtn")!.addEventListener("click", async () => {
    const picked = await open({ directory: true, multiple: false, title: t("tip.pickOutFolder") });
    if (!picked || typeof picked !== "string") return;
    state.convertOutDir = picked;
    updateConvertOutLabel();
    renderConvertList();
  });
  document.getElementById("convertCancelBtn")!.addEventListener("click", exitConvertMode);
  dom.convertList.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest(".mergeRemove") as HTMLElement | null;
    if (!btn) return;
    const idx = parseInt(btn.dataset.idx || "-1", 10);
    if (idx < 0) return;
    state.convertFiles.splice(idx, 1);
    renderConvertList();
  });
  dom.convertCommitBtn.addEventListener("click", () => void busy(dom.convertCommitBtn, async () => {
    if (state.convertFiles.length === 0 || !state.convertOutDir) return;
    const paths = state.convertFiles.map((f: MergeFile) => f.path);
    const level = dom.convertLevelEl.value;
    setStatus(t("st.converting", { n: paths.length, level }));
    try {
      type ConvertResult = { files: string[]; failed: string[]; errors: string[] };
      const res = await invoke<ConvertResult>("convert_files_to_pdfa", {
        paths,
        outDir: state.convertOutDir,
        level,
        lang: pdfaLang(),
      });
      const okCount = res.files.length;
      if (res.failed.length > 0) {
        const failed = res.failed
          .map((p, i) => `${fileNameFromPath(p)}${res.errors[i] ? ` (${pdfaErrorText(res.errors[i])})` : ""}`)
          .join(", ");
        setStatus(t("st.convertPartial", { ok: okCount, total: paths.length, failed }));
      } else {
        setStatus(t("st.converted", { n: okCount, dir: state.convertOutDir ?? "" }));
      }
      state.convertFiles = [];
      renderConvertList();
    } catch (e) {
      setStatus(t("st.convertError", { e: pdfaErrorText(e) }));
    }
  }));

  // Merge pane wiring
  document.getElementById("mergeAddBtn")!.addEventListener("click", async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    addMergePaths(paths);
  });
  document.getElementById("mergeCancelBtn")!.addEventListener("click", exitMergeMode);
  dom.mergeCommitBtn.addEventListener("click", () => void busy(dom.mergeCommitBtn, async () => {
    if (state.mergeFiles.length < 2) return;
    if (!(await confirmDiscardChanges())) return;
    const paths = state.mergeFiles.map((f: MergeFile) => f.path);
    setStatus(t("st.merging", { n: paths.length }));
    try {
      type MergeResult = OpenedDocument & { temp_path: string };
      const res = await invoke<MergeResult>("merge_files_and_open", { paths });
      exitMergeMode();
      applyOpenedDoc(res, res.temp_path, { temp: true });
      setRibbonTab("organize");
      setStatus(t("st.merged", { n: paths.length, pages: res.page_count }));
    } catch (err) {
      setStatus(t("st.mergeError", { e: fmtMutationError(err) }));
    }
  }));
  dom.mergeList.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest(".mergeRemove") as HTMLElement | null;
    if (!btn) return;
    const idx = parseInt(btn.dataset.idx || "-1", 10);
    if (idx < 0) return;
    state.mergeFiles.splice(idx, 1);
    renderMergeList();
  });

  // Merge-tile drag to reorder
  dom.mergeList.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const tile = (e.target as HTMLElement | null)?.closest(".mergeTile") as HTMLElement | null;
    if (!tile) return;
    if ((e.target as HTMLElement).closest(".mergeRemove")) return;
    const idx = parseInt(tile.dataset.idx || "-1", 10);
    if (idx < 0) return;
    state.mdrag = {
      tracking: true, active: false, startX: e.clientX, startY: e.clientY,
      startIdx: idx, preview: null, target: null, after: false,
    };
  });
  window.addEventListener("pointermove", (e) => {
    if (!state.mdrag.tracking) return;
    if (!state.mdrag.active) {
      const dx = e.clientX - state.mdrag.startX;
      const dy = e.clientY - state.mdrag.startY;
      if (dx * dx + dy * dy < 25) return;
      state.mdrag.active = true;
      const tile = dom.mergeList.querySelector<HTMLElement>(`.mergeTile[data-idx="${state.mdrag.startIdx}"]`);
      tile?.classList.add("dragging");
      const prev = document.createElement("div");
      prev.className = "dragPreview";
      prev.textContent = state.mergeFiles[state.mdrag.startIdx]?.name ?? "PDF";
      document.body.appendChild(prev);
      state.mdrag.preview = prev;
    }
    if (state.mdrag.preview) {
      state.mdrag.preview.style.left = `${e.clientX + 14}px`;
      state.mdrag.preview.style.top = `${e.clientY + 14}px`;
    }
    const hit = findMergeDropTarget(e.clientX, e.clientY);
    const target = hit?.el ?? null;
    if (state.mdrag.target && state.mdrag.target !== target) {
      state.mdrag.target.classList.remove("dropBefore", "dropAfter");
    }
    state.mdrag.target = target;
    if (hit && target) {
      state.mdrag.after = hit.after;
      target.classList.toggle("dropBefore", !state.mdrag.after);
      target.classList.toggle("dropAfter", state.mdrag.after);
    }
  });
  window.addEventListener("pointerup", () => {
    if (!state.mdrag.tracking) return;
    const wasActive = state.mdrag.active;
    const from = state.mdrag.startIdx;
    const target = state.mdrag.target;
    const after = state.mdrag.after;
    resetMdrag();
    if (!wasActive || from < 0 || !target) return;
    const toIdxRaw = parseInt(target.dataset.idx || "-1", 10);
    if (toIdxRaw < 0 || toIdxRaw === from) return;
    const moved = state.mergeFiles[from];
    state.mergeFiles.splice(from, 1);
    let insertAt = toIdxRaw;
    if (toIdxRaw > from) insertAt -= 1;
    if (after) insertAt += 1;
    insertAt = Math.max(0, Math.min(state.mergeFiles.length, insertAt));
    state.mergeFiles.splice(insertAt, 0, moved);
    renderMergeList();
  });
  window.addEventListener("pointercancel", resetMdrag);
  // No browser context menu (reload / inspect) on the app chrome — but keep
  // cut/copy/paste in text fields.
  window.addEventListener("contextmenu", (e) => {
    if (!(e.target as HTMLElement | null)?.closest("input, textarea")) e.preventDefault();
  });

  // Sidebar thumbnail click (outside the page grid): jump to that page.
  dom.thumbsPanel.addEventListener(
    "pointerdown",
    (e) => {
      if (state.ribbonTab === "organize") return;
      const thumb = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
      if (!thumb) return;
      const idx = parseInt(thumb.dataset.page || "-1", 10);
      if (idx < 0) return;
      e.preventDefault();
      e.stopPropagation();
      // Lazy route to avoid a viewer<->edit import cycle.
      document.dispatchEvent(new CustomEvent("rustpdf:goto-page", { detail: { page: idx } }));
    },
    true,
  );

  // Page grid (Pages tab): drag to reorder
  dom.thumbsPanel.addEventListener("pointerdown", (e) => {
    if (state.ribbonTab !== "organize") return;
    if (e.button !== 0) return;
    const thumb = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
    if (!thumb) return;
    const idx = parseInt(thumb.dataset.page || "-1", 10);
    if (idx < 0) return;
    state.pdrag = {
      tracking: true, active: false, startX: e.clientX, startY: e.clientY,
      startIdx: idx, pages: [], preview: null, target: null, after: false,
    };
  });
  window.addEventListener("pointermove", (e) => {
    if (!state.pdrag.tracking) return;
    if (!state.pdrag.active) {
      const dx = e.clientX - state.pdrag.startX;
      const dy = e.clientY - state.pdrag.startY;
      if (dx * dx + dy * dy < 25) return;
      state.pdrag.active = true;
      if (state.selectedPages.has(state.pdrag.startIdx) && state.selectedPages.size > 1) {
        state.pdrag.pages = [...state.selectedPages].sort((a, b) => a - b);
      } else {
        state.pdrag.pages = [state.pdrag.startIdx];
        state.selectedPages.clear();
        state.selectedPages.add(state.pdrag.startIdx);
        state.selectionAnchor = state.pdrag.startIdx;
        refreshSelectionUi();
      }
      const pageSet = new Set(state.pdrag.pages);
      state.thumbWraps.forEach((w, i) => {
        if (pageSet.has(i)) w.classList.add("dragging");
      });
      // Ghost: picture of the grabbed page + "move N pages" badge.
      const prev = document.createElement("div");
      prev.className = "dragPreview pageGhost";
      const src = state.thumbEls[state.pdrag.startIdx]?.getAttribute("src");
      if (src) {
        const img = document.createElement("img");
        img.src = src;
        img.alt = "";
        prev.appendChild(img);
      }
      const badge = document.createElement("span");
      badge.textContent = t("drag.move", { n: state.pdrag.pages.length });
      prev.appendChild(badge);
      document.body.appendChild(prev);
      document.body.classList.add("pageDragging");
      state.pdrag.preview = prev;
    }
    if (state.pdrag.preview) {
      state.pdrag.preview.style.left = `${e.clientX + 14}px`;
      state.pdrag.preview.style.top = `${e.clientY + 14}px`;
    }
    const hit = findDropTarget(e.clientX, e.clientY);
    // Hovering one of the moved pages themselves means "no move".
    const target = hit && !hit.el.classList.contains("dragging") ? hit.el : null;
    if (state.pdrag.target && state.pdrag.target !== target) {
      state.pdrag.target.classList.remove("dropBefore");
      state.pdrag.target.classList.remove("dropAfter");
    }
    state.pdrag.target = target;
    if (hit && target) {
      state.pdrag.after = hit.after;
      target.classList.toggle("dropBefore", !state.pdrag.after);
      target.classList.toggle("dropAfter", state.pdrag.after);
    }
  });
  window.addEventListener("pointerup", async () => {
    if (!state.pdrag.tracking) return;
    const wasActive = state.pdrag.active;
    const pages = state.pdrag.pages;
    const target = state.pdrag.target;
    const after = state.pdrag.after;
    resetPdrag();
    if (!wasActive) return;
    state.lastWasDrag = true;
    setTimeout(() => { state.lastWasDrag = false; }, 0);
    if (pages.length === 0) return;
    // Released over the moved pages themselves (or nowhere): nothing to do.
    if (!target) return;
    const dragSet = new Set(pages);
    const tIdx = parseInt(target.dataset.page || "-1", 10);
    if (tIdx < 0 || dragSet.has(tIdx)) return;
    const remaining = [...Array(state.pageCount).keys()].filter((i) => !dragSet.has(i));
    let insertAt = remaining.indexOf(tIdx);
    if (after) insertAt += 1;
    const newOrder = [
      ...remaining.slice(0, insertAt),
      ...pages,
      ...remaining.slice(insertAt),
    ];
    if (newOrder.length !== state.pageCount) return;
    if (newOrder.every((v, i) => v === i)) return;
    setStatus(t("st.reordering", { n: pages.length }));
    try {
      const snap = await invoke<DocSnapshot>("reorder_pages", {
        document: state.currentDoc,
        order: newOrder,
      });
      clearSelection();
      applyDocSnapshot(snap);
      setStatus(t("st.reordered", { n: pages.length }));
    } catch (err) {
      setStatus(t("st.reorderError", { e: String(err) }));
    }
  });
  window.addEventListener("pointercancel", () => resetPdrag());

  // Page grid click selection (suppressed if a drag just completed)
  dom.thumbsPanel.addEventListener("click", (e) => {
    if (state.ribbonTab !== "organize") return;
    if (state.lastWasDrag) return;
    const thumb = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
    if (!thumb) return;
    const idx = parseInt(thumb.dataset.page || "-1", 10);
    if (idx < 0) return;
    handleThumbSelect(idx, e);
  });
}
