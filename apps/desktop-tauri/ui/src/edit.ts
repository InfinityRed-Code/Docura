import { invoke } from "@tauri-apps/api/core";
import { open, save, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import type {
  DocSnapshot, EditMode, InsertWhere, MergeFile, OpenedDocument, RibbonTab,
} from "./types";
import { state, makeMdrag, makePdrag } from "./state";
import { t } from "./i18n";
import { dom, setStatus } from "./dom";
import { reflow } from "./viewer";
import {
  applyDocSnapshot, applyOpenedDoc, clearSelection, handleThumbSelect,
  refreshSelectionUi,
} from "./doc";
import { clearSignToolUi, setShapeTool } from "./annot";

// ---------- Page operations ----------

export async function deleteSelectedPages() {
  if (!state.currentDoc || state.selectedPages.size === 0) return;
  const pages = [...state.selectedPages].sort((a, b) => a - b);
  if (pages.length >= state.pageCount) {
    setStatus("Can't delete every page");
    return;
  }
  const label = pages.length === 1
    ? `page ${pages[0] + 1}`
    : `${pages.length} pages (${pages[0] + 1}–${pages[pages.length - 1] + 1})`;
  if (state.editMode === "select") {
    const ok = await confirmDialog(
      `Are you sure you want to delete all selected pages?\n\n(${label} — this can only be undone by not saving.)`,
      { title: "Delete pages", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (!ok) return;
  }
  setStatus(`Deleting ${label}…`);
  try {
    const snap = await invoke<DocSnapshot>("delete_pages", { document: state.currentDoc, pages });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(`Deleted ${label}`);
  } catch (e) {
    setStatus(`Delete error: ${e}`);
  }
}

export async function extractSelectedPages() {
  if (!state.currentDoc || state.selectedPages.size === 0) return;
  const pages = [...state.selectedPages].sort((a, b) => a - b);
  const base = state.currentDocPath
    ? state.currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "document";
  const suffix = pages.length === 1
    ? `page_${pages[0] + 1}`
    : `pages_${pages[0] + 1}-${pages[pages.length - 1] + 1}`;
  const defaultName = `${base}_${suffix}.pdf`;
  const path = await save({
    title: "Extract pages to…",
    defaultPath: defaultName,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!path) return;
  setStatus(`Extracting ${pages.length} page${pages.length === 1 ? "" : "s"}…`);
  try {
    await invoke("extract_pages", { document: state.currentDoc, pages, path });
    setStatus(`Extracted ${pages.length} page${pages.length === 1 ? "" : "s"} → ${path}`);
  } catch (e) {
    setStatus(`Extract error: ${e}`);
  }
}

export async function rotateSelectionOrCurrent(degrees: number) {
  if (!state.currentDoc) return;
  const pages = state.selectedPages.size > 0
    ? [...state.selectedPages].sort((a, b) => a - b)
    : [state.currentPage];
  const label = pages.length === 1
    ? `page ${pages[0] + 1}`
    : `${pages.length} pages`;
  setStatus(`Rotating ${label}…`);
  try {
    const snap = await invoke<DocSnapshot>("rotate_pages", {
      document: state.currentDoc,
      pages,
      degrees,
    });
    applyDocSnapshot(snap);
    refreshSelectionUi();
    setStatus(`Rotated ${label} by ${degrees}°`);
  } catch (e) {
    setStatus(`Rotate error: ${e}`);
  }
}

// ---------- Save / Print ----------

export async function triggerSaveAs() {
  if (!state.currentDoc) return;
  const suggestion = state.currentDocPath
    ? state.currentDocPath.replace(/\.pdf$/i, "") + "-copy.pdf"
    : "document.pdf";
  const dest = await save({
    defaultPath: suggestion,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!dest || typeof dest !== "string") return;
  const level = dom.pdfaLevelEl?.value ?? "off";
  if (level !== "off" && hasInteractiveFormFields()) {
    setStatus(`PDF/A skipped — document is a fillable form. Saved plain PDF: ${dest}`);
    try {
      await invoke("save_pdf_as", { document: state.currentDoc, path: dest });
    } catch (e) {
      setStatus(`Save error: ${e}`);
    }
    return;
  }
  if (level !== "off") {
    setStatus(`Saving as PDF/A-${level}…`);
    try {
      const label = await invoke<string>("save_as_pdfa", {
        document: state.currentDoc,
        args: { path: dest, level },
      });
      // PDF/A forbids encryption — flag the dropped protection.
      let suffix = "";
      try {
        const sec = await invoke<{ protected: boolean }>("get_pdf_security", {
          document: state.currentDoc,
        });
        if (sec.protected) suffix = ` · ${t("st.pdfaNoProtection")}`;
      } catch {}
      setStatus(`Saved as ${label}: ${dest}${suffix}`);
    } catch (e) {
      setStatus(`PDF/A save error: ${e}`);
    }
    return;
  }
  setStatus(`Saving to ${dest}…`);
  try {
    await invoke("save_pdf_as", { document: state.currentDoc, path: dest });
    setStatus(`Saved: ${dest}`);
  } catch (e) {
    setStatus(`Save error: ${e}`);
  }
}

export function hasInteractiveFormFields(): boolean {
  return state.formFields.some(
    (f) => !f.readonly && (f.kind === "text" || f.kind === "checkbox" ||
      f.kind === "radio" || f.kind === "combobox" || f.kind === "listbox" ||
      f.kind === "signature"),
  );
}

export async function confirmFormsLostInPdfa(level: string): Promise<boolean> {
  if (!hasInteractiveFormFields()) return true;
  return await confirmDialog(
    `This document contains interactive form fields. Saving as PDF/A-${level} flattens the form: field values are baked into static content and the document is no longer fillable.\n\nContinue and lose the interactive form?`,
    { title: "PDF/A would flatten the form", kind: "warning" },
  );
}

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
  state.ribbonTab = tab;
  dom.ribbonBody.dataset.active = tab;
  document.querySelectorAll(".ribbonTab[data-ribbon-tab]").forEach((el) => {
    el.classList.toggle("active", (el as HTMLElement).dataset.ribbonTab === tab);
  });
  const wasOrganize = document.body.classList.contains("organize");
  const wasConvert = document.body.classList.contains("convert");
  // Leaving the Sign tab disarms any signature/mark tool.
  if (tab !== "sign") { clearSignToolUi(); setShapeTool(null); }
  if (state.mergeMode && tab !== "organize") exitMergeMode();
  if (tab !== "convert" && wasConvert) exitConvertMode();
  if (tab === "convert" && !wasConvert) enterConvertMode();
  document.body.classList.toggle("organize", tab === "organize");
  if (tab === "organize") {
    document.getElementById("sidebar")!.classList.remove("hidden");
    void setEditMode("select");
  } else if (tab === "reader" || tab === "forms" || tab === "sign") {
    void setEditMode("select");
    clearSelection();
  }
  if ((wasOrganize || wasConvert) && tab !== "organize" && tab !== "convert" && state.currentDoc) {
    requestAnimationFrame(() => reflow());
  }
}

export async function setEditMode(mode: EditMode) {
  if (mode === "delete" && state.editMode !== "delete") {
    const ok = await confirmDialog(
      "When entering Delete mode, no further confirmation is needed to delete pages.\n\nMake sure to use with caution!",
      { title: "Enter Delete mode", kind: "warning", okLabel: "Enter Delete mode", cancelLabel: "Cancel" },
    );
    if (!ok) return;
  }
  state.editMode = mode;
  document.querySelectorAll(".modeBtn").forEach((el) => {
    const m = (el as HTMLElement).dataset.mode;
    el.classList.toggle("active", m === mode);
    el.classList.toggle("danger", m === mode && mode === "delete");
  });
  document.body.classList.toggle("edit-delete", mode === "delete");
  refreshSelectionUi();
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
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
    title: "Pick image for watermark",
  });
  if (picked && typeof picked === "string") {
    (document.getElementById("iwPath") as HTMLInputElement).value = picked;
  }
}

async function runWatermark() {
  if (!state.currentDoc) return;
  if (state.wmMode === "text") {
    const text = (document.getElementById("wmText") as HTMLInputElement).value.trim();
    if (!text) { setStatus("Watermark text is empty"); return; }
    const args = {
      text,
      font_size: parseFloat((document.getElementById("wmFontSize") as HTMLInputElement).value) || 72,
      rotation_deg: parseFloat((document.getElementById("wmRot") as HTMLInputElement).value) || 0,
      opacity: parseFloat((document.getElementById("wmOpacity") as HTMLInputElement).value) || 0.25,
      gray: parseFloat((document.getElementById("wmGray") as HTMLInputElement).value),
    };
    if (!Number.isFinite(args.gray)) args.gray = 0.5;
    dom.wmOverlay.classList.add("hidden");
    setStatus("Applying watermark…");
    try {
      const snap = await invoke<DocSnapshot>("add_watermark", { document: state.currentDoc, args });
      applyDocSnapshot(snap);
      setStatus("Watermark applied");
    } catch (e) {
      setStatus(`Watermark error: ${e}`);
    }
  } else {
    const image_path = (document.getElementById("iwPath") as HTMLInputElement).value.trim();
    if (!image_path) { setStatus("Pick an image first"); return; }
    const args = {
      image_path,
      scale_frac: parseFloat((document.getElementById("iwScale") as HTMLInputElement).value) || 0.4,
      rotation_deg: parseFloat((document.getElementById("iwRot") as HTMLInputElement).value) || 0,
      opacity: parseFloat((document.getElementById("iwOpacity") as HTMLInputElement).value) || 0.5,
      anchor: (document.getElementById("iwAnchor") as HTMLSelectElement).value || "center",
      margin_pt: parseFloat((document.getElementById("iwMargin") as HTMLInputElement).value) || 36,
    };
    dom.wmOverlay.classList.add("hidden");
    setStatus("Applying image watermark…");
    try {
      const snap = await invoke<DocSnapshot>("add_image_watermark", { document: state.currentDoc, args });
      applyDocSnapshot(snap);
      setStatus("Image watermark applied");
    } catch (e) {
      setStatus(`Image watermark error: ${e}`);
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
  if (!anyText) { setStatus("Fill at least one slot"); return; }
  dom.hfOverlay.classList.add("hidden");
  setStatus("Applying header/footer…");
  try {
    const snap = await invoke<DocSnapshot>("add_header_footer", { document: state.currentDoc, args });
    applyDocSnapshot(snap);
    setStatus("Header/footer applied");
  } catch (e) {
    setStatus(`Header/footer error: ${e}`);
  }
}

async function runSplitPdf() {
  if (!state.currentDoc) { setStatus("Open a PDF first"); return; }
  const suggestion = `1-${Math.min(state.pageCount, Math.max(1, Math.ceil(state.pageCount / 2)))},${Math.min(state.pageCount, Math.max(1, Math.ceil(state.pageCount / 2)) + 1)}-${state.pageCount}`;
  const spec = window.prompt(
    `Split into parts by page ranges (1-based, comma-separated).\nExamples:  1-3,4-6,7-${state.pageCount}    or    1,2-5,6`,
    suggestion,
  );
  if (!spec) return;
  const outDir = await open({ directory: true, title: "Output folder for split parts" });
  if (!outDir || typeof outDir !== "string") return;
  const baseName = state.currentDocPath
    ? state.currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "document";
  setStatus("Splitting…");
  try {
    const res = await invoke<{ files: string[] }>("split_pdf", {
      document: state.currentDoc,
      ranges: spec,
      outDir,
      baseName,
    });
    setStatus(`Split → ${res.files.length} file${res.files.length === 1 ? "" : "s"} in ${outDir}`);
  } catch (e) {
    setStatus(`Split error: ${e}`);
  }
}

// ---------- Convert pane (PDF → PDF/A) ----------

function enterConvertMode() {
  document.body.classList.add("convert");
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
}
function renderConvertList() {
  dom.convertList.innerHTML = "";
  if (state.convertFiles.length === 0) {
    const empty = document.createElement("div");
    empty.className = "mergeEmpty";
    empty.textContent = "No files yet. Click ＋ to add PDFs to convert.";
    dom.convertList.appendChild(empty);
  } else {
    state.convertFiles.forEach((f, i) => {
      const tile = document.createElement("div");
      tile.className = "mergeTile";
      tile.dataset.idx = String(i);
      tile.innerHTML = `
        <span class="mergeIndex">${i + 1}</span>
        <button class="mergeRemove" title="Remove" data-idx="${i}">✕</button>
        <div class="mergeName">${f.name}</div>
        <div class="mergePath">${f.path}</div>`;
      dom.convertList.appendChild(tile);
    });
  }
  dom.convertCommitBtn.disabled = state.convertFiles.length === 0 || !state.convertOutDir;
}
function updateConvertOutLabel() {
  dom.convertOutLabel.textContent = state.convertOutDir
    ? `Output: ${state.convertOutDir}`
    : "Output: (none chosen — pick an output folder)";
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
export function fileNameFromPath(p: string): string {
  const m = p.replace(/\\/g, "/").split("/").pop() ?? p;
  return m || p;
}
function renderMergeList() {
  dom.mergeList.innerHTML = "";
  if (state.mergeFiles.length === 0) {
    const empty = document.createElement("div");
    empty.className = "mergeEmpty";
    empty.textContent = "No files yet. Drop PDFs here or click ＋ Add files.";
    dom.mergeList.appendChild(empty);
  } else {
    state.mergeFiles.forEach((f, i) => {
      const tile = document.createElement("div");
      tile.className = "mergeTile";
      tile.dataset.idx = String(i);
      tile.innerHTML = `
        <span class="mergeIndex">${i + 1}</span>
        <button class="mergeRemove" title="Remove" data-idx="${i}">✕</button>
        <div class="mergeName">${f.name}</div>
        <div class="mergePath">${f.path}</div>`;
      dom.mergeList.appendChild(tile);
    });
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
  setInsertMenuOpen(false);
  if (!state.currentDoc) { setStatus("Open a PDF first"); return; }
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
  setStatus(`Inserting ${paths.length} file${paths.length === 1 ? "" : "s"}…`);
  try {
    const snap = await invoke<DocSnapshot>("insert_pdf_files", {
      document: state.currentDoc, paths, atIndex: at,
    });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(`Inserted ${paths.length} file${paths.length === 1 ? "" : "s"} (${snap.page_count} pages total)`);
  } catch (err) {
    setStatus(`Insert error: ${fmtMutationError(err)}`);
  }
}

/** Map backend error markers to localized messages. */
export function fmtMutationError(err: unknown): string {
  const s = String(err);
  const m = s.match(/ENCRYPTED_INPUT:(.+)$/);
  if (m) return t("st.encryptedInput", { file: m[1] });
  return s;
}

// ---------- Popovers ----------

function setRotateMenuOpen(open: boolean) {
  dom.rotateMenu.classList.toggle("hidden", !open);
  dom.rotateMenuBtn.setAttribute("aria-expanded", String(open));
}
function setInsertMenuOpen(open: boolean) {
  dom.insertMenu.classList.toggle("hidden", !open);
  dom.insertMenuBtn.setAttribute("aria-expanded", String(open));
}

function runRotate(deg: number) {
  setRotateMenuOpen(false);
  rotateSelectionOrCurrent(deg);
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
  for (const t of all) {
    if (t.classList.contains("dragging")) continue;
    const r = t.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const dx = cx - x;
    const dy = cy - y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = t; bestRect = r; }
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
  state.thumbWraps.forEach((w) => w.classList.remove("dragging"));
  document.querySelectorAll(".thumb.dropBefore, .thumb.dropAfter").forEach((el) => {
    el.classList.remove("dropBefore");
    el.classList.remove("dropAfter");
  });
  state.pdrag = makePdrag();
}

// ---------- Init ----------

export function initEdit() {
  // Save As / Print
  document.getElementById("saveAsBtn")!.addEventListener("click", triggerSaveAs);
  document.getElementById("printBtn")!.addEventListener("click", triggerPrint);

  // Ribbon tabs
  document.querySelectorAll(".ribbonTab[data-ribbon-tab]").forEach((el) => {
    el.addEventListener("click", () => {
      const t = (el as HTMLElement).dataset.ribbonTab as RibbonTab;
      setRibbonTab(t);
    });
  });
  document.querySelectorAll(".modeBtn").forEach((el) => {
    el.addEventListener("click", () => {
      const m = (el as HTMLElement).dataset.mode as EditMode;
      if (m) void setEditMode(m);
    });
  });

  // Edit-mode action buttons
  document.getElementById("deleteSelectionBtn")!.addEventListener("click", deleteSelectedPages);
  document.getElementById("extractSelectionBtn")!.addEventListener("click", extractSelectedPages);

  // Rotate menu
  dom.rotateMenuBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    setRotateMenuOpen(dom.rotateMenu.classList.contains("hidden"));
  });
  document.addEventListener("click", (e) => {
    if (!dom.rotateMenu.classList.contains("hidden")
      && !dom.rotateMenu.contains(e.target as Node)
      && e.target !== dom.rotateMenuBtn) {
      setRotateMenuOpen(false);
    }
  });
  document.getElementById("rotateLeftBtn")!.addEventListener("click", () => runRotate(-90));
  document.getElementById("rotateRightBtn")!.addEventListener("click", () => runRotate(90));
  document.getElementById("rotate180Btn")!.addEventListener("click", () => runRotate(180));

  // Insert menu
  dom.insertMenuBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    setInsertMenuOpen(dom.insertMenu.classList.contains("hidden"));
  });
  document.addEventListener("click", (e) => {
    if (!dom.insertMenu.classList.contains("hidden")
      && !dom.insertMenu.contains(e.target as Node)
      && e.target !== dom.insertMenuBtn) {
      setInsertMenuOpen(false);
    }
  });
  document.getElementById("insertBeforeBtn")!.addEventListener("click", () => runInsertPdf("before"));
  document.getElementById("insertAfterBtn")!.addEventListener("click", () => runInsertPdf("after"));
  document.getElementById("insertEndBtn")!.addEventListener("click", () => runInsertPdf("end"));

  // Merge / split
  document.getElementById("mergePdfsBtn")!.addEventListener("click", () => enterMergeMode());
  document.getElementById("splitPdfBtn")!.addEventListener("click", () => void runSplitPdf());

  // Watermark overlay
  dom.wmTabText.addEventListener("click", () => setWmMode("text"));
  dom.wmTabImage.addEventListener("click", () => setWmMode("image"));
  document.getElementById("watermarkBtn")!.addEventListener("click", () => {
    if (!state.currentDoc) { setStatus("Open a PDF first"); return; }
    dom.wmOverlay.classList.remove("hidden");
    if (state.wmMode === "text") (document.getElementById("wmText") as HTMLInputElement).focus();
  });
  document.getElementById("wmClose")!.addEventListener("click", () => dom.wmOverlay.classList.add("hidden"));
  document.getElementById("wmCancel")!.addEventListener("click", () => dom.wmOverlay.classList.add("hidden"));
  document.getElementById("wmCommit")!.addEventListener("click", () => void runWatermark());
  document.getElementById("iwPick")!.addEventListener("click", () => void pickImageWatermark());

  // Header/footer overlay
  document.getElementById("headerFooterBtn")!.addEventListener("click", () => {
    if (!state.currentDoc) { setStatus("Open a PDF first"); return; }
    dom.hfOverlay.classList.remove("hidden");
  });
  document.getElementById("hfClose")!.addEventListener("click", () => dom.hfOverlay.classList.add("hidden"));
  document.getElementById("hfCancel")!.addEventListener("click", () => dom.hfOverlay.classList.add("hidden"));
  document.getElementById("hfCommit")!.addEventListener("click", () => void runHeaderFooter());

  // Convert pane wiring
  document.getElementById("convertAddBtn")!.addEventListener("click", async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return;
    addConvertPaths(Array.isArray(picked) ? picked : [picked]);
  });
  document.getElementById("convertOutBtn")!.addEventListener("click", async () => {
    const picked = await open({ directory: true, multiple: false, title: "Pick output folder" });
    if (!picked || typeof picked !== "string") return;
    state.convertOutDir = picked;
    updateConvertOutLabel();
    renderConvertList();
  });
  document.getElementById("convertCancelBtn")!.addEventListener("click", () => setRibbonTab("reader"));
  dom.convertList.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest(".mergeRemove") as HTMLElement | null;
    if (!btn) return;
    const idx = parseInt(btn.dataset.idx || "-1", 10);
    if (idx < 0) return;
    state.convertFiles.splice(idx, 1);
    renderConvertList();
  });
  dom.convertCommitBtn.addEventListener("click", async () => {
    if (state.convertFiles.length === 0 || !state.convertOutDir) return;
    const paths = state.convertFiles.map((f: MergeFile) => f.path);
    const level = dom.convertLevelEl.value;
    setStatus(`Converting ${paths.length} file${paths.length === 1 ? "" : "s"} to PDF/A-${level}…`);
    try {
      type ConvertResult = { files: string[]; failed: string[] };
      const res = await invoke<ConvertResult>("convert_files_to_pdfa", {
        paths,
        outDir: state.convertOutDir,
        level,
      });
      const okCount = res.files.length;
      const failCount = res.failed.length;
      if (failCount > 0) {
        setStatus(`Converted ${okCount}/${paths.length}. Failed: ${res.failed.map(fileNameFromPath).join(", ")}`);
      } else {
        setStatus(`Converted ${okCount} file${okCount === 1 ? "" : "s"} → ${state.convertOutDir}`);
      }
      state.convertFiles = [];
      renderConvertList();
    } catch (e) {
      setStatus(`Convert error: ${e}`);
    }
  });

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
  dom.mergeCommitBtn.addEventListener("click", async () => {
    if (state.mergeFiles.length < 2) return;
    const paths = state.mergeFiles.map((f: MergeFile) => f.path);
    setStatus(`Merging ${paths.length} files…`);
    try {
      type MergeResult = OpenedDocument & { temp_path: string };
      const res = await invoke<MergeResult>("merge_files_and_open", { paths });
      const tmp = res.temp_path;
      exitMergeMode();
      applyOpenedDoc(res, tmp);
      setRibbonTab("organize");
      setStatus(`Merged ${paths.length} files into ${res.page_count} pages (temp: ${tmp})`);
    } catch (err) {
      setStatus(`Merge error: ${fmtMutationError(err)}`);
    }
  });
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
    const t = (e.target as HTMLElement | null)?.closest(".mergeTile") as HTMLElement | null;
    if (!t) return;
    if ((e.target as HTMLElement).closest(".mergeRemove")) return;
    const idx = parseInt(t.dataset.idx || "-1", 10);
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
      prev.textContent = state.mergeFiles[state.mdrag.startIdx]?.name ?? "file";
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
  window.addEventListener("contextmenu", (e) => e.preventDefault());

  // Thumb click (Edit / Reader)
  dom.thumbsPanel.addEventListener(
    "pointerdown",
    (e) => {
      const t = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
      if (!t) return;
      const idx = parseInt(t.dataset.page || "-1", 10);
      if (idx < 0) return;
      if (state.ribbonTab === "organize") return;
      e.preventDefault();
      e.stopPropagation();
      if (state.ribbonTab === "edit") {
        handleThumbSelect(idx, e);
      } else {
        // Lazy import to avoid cycle; navigateTo lives in viewer.
        document.dispatchEvent(new CustomEvent("rustpdf:goto-page", { detail: { page: idx } }));
      }
    },
    true,
  );

  // Organize-mode drag-to-reorder
  dom.thumbsPanel.addEventListener("pointerdown", (e) => {
    if (state.ribbonTab !== "organize") return;
    if (e.button !== 0) return;
    const t = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
    if (!t) return;
    const idx = parseInt(t.dataset.page || "-1", 10);
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
      const prev = document.createElement("div");
      prev.className = "dragPreview";
      prev.textContent = `Move ${state.pdrag.pages.length} page${state.pdrag.pages.length === 1 ? "" : "s"}`;
      document.body.appendChild(prev);
      state.pdrag.preview = prev;
    }
    if (state.pdrag.preview) {
      state.pdrag.preview.style.left = `${e.clientX + 14}px`;
      state.pdrag.preview.style.top = `${e.clientY + 14}px`;
    }
    const hit = findDropTarget(e.clientX, e.clientY);
    const target = hit?.el ?? null;
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
    const dragSet = new Set(pages);
    const remaining = [...Array(state.pageCount).keys()].filter((i) => !dragSet.has(i));
    let insertAt: number;
    if (!target) {
      insertAt = remaining.length;
    } else {
      const tIdx = parseInt(target.dataset.page || "-1", 10);
      if (dragSet.has(tIdx)) {
        insertAt = remaining.length;
      } else {
        insertAt = remaining.indexOf(tIdx);
        if (after) insertAt += 1;
      }
    }
    const newOrder = [
      ...remaining.slice(0, insertAt),
      ...pages,
      ...remaining.slice(insertAt),
    ];
    if (newOrder.length !== state.pageCount) return;
    if (newOrder.every((v, i) => v === i)) return;
    setStatus(`Reordering ${pages.length} page${pages.length === 1 ? "" : "s"}…`);
    try {
      const snap = await invoke<DocSnapshot>("reorder_pages", {
        document: state.currentDoc,
        order: newOrder,
      });
      clearSelection();
      applyDocSnapshot(snap);
      setStatus(`Reordered ${pages.length} page${pages.length === 1 ? "" : "s"}`);
    } catch (err) {
      setStatus(`Reorder error: ${err}`);
    }
  });
  window.addEventListener("pointercancel", () => resetPdrag());

  // Organize click selection (suppressed if a drag just completed)
  dom.thumbsPanel.addEventListener("click", (e) => {
    if (state.ribbonTab !== "organize") return;
    if (state.lastWasDrag) return;
    const t = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
    if (!t) return;
    const idx = parseInt(t.dataset.page || "-1", 10);
    if (idx < 0) return;
    handleThumbSelect(idx, e);
  });
}
