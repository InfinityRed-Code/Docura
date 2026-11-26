import { invoke } from "@tauri-apps/api/core";
import { isBuiltin, pdfaErrorText, pdfaLang, pdfaNotesText, type PdfaSaved } from "./pdfa";
import { open, save, message, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import type {
  AttachmentInfo, DocMetadata, DocPermissions, DocSnapshot,
  OpenErr, OpenedDocument, OutlineNode, RecentFile, Rendered, UndoStatus,
} from "./types";
import { state, callbacks } from "./state";
import { dom, setStatus } from "./dom";
import { t } from "./i18n";
import { refreshIcons } from "./icons";
import {
  invalidateRenders, navigateTo, rebuildSlots, refreshWindowTitle,
  scheduleRender, setWindowTitle, updateChrome,
} from "./viewer";

const MAX_RENDER_DIST = 6;

export function fileNameFromPath(p: string): string {
  const m = p.replace(/\\/g, "/").split("/").pop() ?? p;
  return m || p;
}

/** File name of the open document without the .pdf extension. */
export function docBaseName(): string {
  return state.currentDocPath
    ? fileNameFromPath(state.currentDocPath).replace(/\.pdf$/i, "")
    : "document";
}

// ---------- Password prompt ----------
//
// window.prompt() is not supported inside the Tauri WebView, so password
// entry runs through a promise-based modal (markup in template.ts).

let pwResolve: ((value: string | null) => void) | null = null;

function settlePw(value: string | null) {
  document.getElementById("pwOverlay")!.classList.add("hidden");
  const r = pwResolve;
  pwResolve = null;
  r?.(value);
}

/** Show the password modal. Resolves to the input or `null` on cancel. */
export function promptPassword(message: string): Promise<string | null> {
  const input = document.getElementById("pwInput") as HTMLInputElement;
  // A pending prompt is cancelled by the new one.
  if (pwResolve) settlePw(null);
  document.getElementById("pwMessage")!.textContent = message;
  input.value = "";
  document.getElementById("pwOverlay")!.classList.remove("hidden");
  input.focus();
  return new Promise((resolve) => {
    pwResolve = resolve;
  });
}

function initPasswordModal() {
  const pwInput = document.getElementById("pwInput") as HTMLInputElement;
  document.getElementById("pwOk")!.addEventListener("click", () => settlePw(pwInput.value));
  document.getElementById("pwCancel")!.addEventListener("click", () => settlePw(null));
  document.getElementById("pwClose")!.addEventListener("click", () => settlePw(null));
  pwInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") settlePw(pwInput.value);
  });
  document.getElementById("pwOverlay")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("pwOverlay")) settlePw(null);
  });
}

// ---------- Unsaved changes ----------

export function markDirty() {
  if (!state.currentDoc || state.dirty) return;
  state.dirty = true;
  refreshWindowTitle();
}

function markClean() {
  state.dirty = false;
  refreshWindowTitle();
}

/**
 * Ask what to do with unsaved changes before the current document is
 * replaced or the app closes. Resolves `true` when it is fine to continue
 * (saved, discarded, or nothing to save) and `false` when the user cancelled.
 */
export async function confirmDiscardChanges(): Promise<boolean> {
  if (!state.currentDoc || !state.dirty) return true;
  const name = state.currentDocPath && !state.docIsTemp
    ? fileNameFromPath(state.currentDocPath)
    : docBaseName();
  const labels = { yes: t("dirty.save"), no: t("dirty.discard"), cancel: t("dirty.cancel") };
  let res: string;
  try {
    res = await message(t("dirty.msg", { name }), {
      title: t("dirty.title"),
      kind: "warning",
      buttons: labels,
    });
  } catch {
    // Dialog unavailable — fall back to a plain yes/no "discard?".
    return await confirmDialog(t("dirty.msg", { name }), { title: t("dirty.title"), kind: "warning" })
      .then((ok) => !ok ? true : saveDocument());
  }
  if (res === labels.yes || res === "Yes") return await saveDocument();
  if (res === labels.no || res === "No") return true;
  return false;
}

// ---------- Save ----------

export function hasInteractiveFormFields(): boolean {
  return state.formFields.some(
    (f) => !f.readonly && (f.kind === "text" || f.kind === "checkbox" ||
      f.kind === "radio" || f.kind === "combobox" || f.kind === "listbox" ||
      f.kind === "signature"),
  );
}

export async function confirmFormsLostInPdfa(level: string): Promise<boolean> {
  if (!hasInteractiveFormFields()) return true;
  return await confirmDialog(t("dlg.pdfaFormMsg", { level }), {
    title: t("dlg.pdfaFormTitle"),
    kind: "warning",
  });
}

async function writePlain(dest: string): Promise<boolean> {
  setStatus(t("st.saving"));
  try {
    await invoke("save_pdf_as", { document: state.currentDoc, path: dest });
    setStatus(t("st.saved", { path: dest }));
    return true;
  } catch (e) {
    setStatus(t("st.saveError", { e: String(e) }));
    return false;
  }
}

/** Save over the original file (Ctrl+S). Documents that only exist in a
 * temp file (merge result, images → PDF, …) ask for a destination. */
export async function saveDocument(): Promise<boolean> {
  if (!state.currentDoc) return false;
  if (!state.currentDocPath || state.docIsTemp) return await saveDocumentAs();
  const ok = await writePlain(state.currentDocPath);
  if (ok) markClean();
  return ok;
}

/** "Save as" (Ctrl+Shift+S). Honors the default PDF/A level from Settings. */
export async function saveDocumentAs(): Promise<boolean> {
  if (!state.currentDoc) return false;
  const suggestion = state.currentDocPath && !state.docIsTemp
    ? state.currentDocPath
    : `${docBaseName()}.pdf`;
  const dest = await save({
    defaultPath: suggestion,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!dest || typeof dest !== "string") return false;
  const level = dom.pdfaLevelEl?.value ?? "off";
  // Ghostscript flattens forms; the built-in converter keeps them fillable.
  if (level !== "off" && !isBuiltin() && hasInteractiveFormFields()) {
    const ok = await writePlain(dest);
    if (ok) {
      setStatus(t("st.pdfaSkipped", { path: dest }));
      adoptSavedPath(dest);
    }
    return ok;
  }
  if (level !== "off") {
    setStatus(t("st.savingPdfa", { level }));
    try {
      const res = await invoke<PdfaSaved>("save_as_pdfa", {
        document: state.currentDoc,
        args: { path: dest, level, lang: pdfaLang() },
      });
      const label = res.label;
      const notes = pdfaNotesText(res.notes);
      // PDF/A forbids encryption — flag the dropped protection.
      let suffix = "";
      try {
        const sec = await invoke<{ protected: boolean }>("get_pdf_security", {
          document: state.currentDoc,
        });
        if (sec.protected) suffix = ` · ${t("st.pdfaNoProtection")}`;
      } catch {}
      if (notes) suffix += ` · ${notes}`;
      setStatus(t("st.savedPdfa", { label, path: dest }) + suffix);
      // The PDF/A copy differs from the working document, so the working
      // document keeps its own path; the changes count as saved though.
      markClean();
      return true;
    } catch (e) {
      setStatus(t("st.pdfaError", { e: pdfaErrorText(e) }));
      return false;
    }
  }
  const ok = await writePlain(dest);
  if (ok) adoptSavedPath(dest);
  return ok;
}

/** After "Save as", the open document now *is* that file. */
function adoptSavedPath(dest: string) {
  state.currentDocPath = dest;
  state.docIsTemp = false;
  setWindowTitle(fileNameFromPath(dest));
  markClean();
  void invoke("push_recent", { path: dest }).catch(() => {});
}

// ---------- Open ----------

export async function openWithDialog() {
  const path = await open({
    multiple: false,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!path || typeof path !== "string") return;
  await openByPath(path);
}

export function applyOpenedDoc(doc: OpenedDocument, label: string, opts: { temp?: boolean } = {}) {
  try {
    const prior = state.currentDoc;
    state.currentDoc = doc.id;
    state.currentDocPath = /\.pdf$/i.test(label) ? label : null;
    state.docIsTemp = !!opts.temp;
    // A temp document (merge result, conversion) has never been saved.
    state.dirty = !!opts.temp;
    state.contentGen++;
    clearSelection();
    if (prior && prior !== doc.id) {
      invoke("close_pdf", { document: prior }).catch(() => {});
    }
    state.pageCount = doc.page_count;
    state.pageSizes = doc.page_sizes;
    state.outline = doc.outline;
    state.detailScale = doc.detail_scale ?? null;
    state.currentPage = 0;
    state.hits = [];
    state.currentHit = -1;
    state.docRotation = 0;
    state.pageRotation.clear();
    state.textRunsCache.clear();
    state.linksCache.clear();
    state.formFields = [];
    state.formFieldsByPage.clear();
    state.invalidFieldIds.clear();
    dom.searchCount.textContent = "";
    renderOutline();
    document.getElementById("emptyState")?.classList.add("hidden");
    rebuildSlots();
    if (state.viewMode === "single") navigateTo(0, { recordHistory: false });
    else scheduleRender(0);
    renderThumbsShell();
    void loadAttachments();
    const name = fileNameFromPath(label);
    const base = t("st.opened", { name, n: state.pageCount });
    setStatus(doc.protected ? `${base} · ${t("st.protectedSuffix")}` : base);
    updateChrome();
    setWindowTitle(name);
    // Decoupled listeners (e.g. the OCR "scanned document?" banner).
    document.dispatchEvent(new CustomEvent("docura:doc-opened"));
    void refreshUndoUi();
    void callbacks.refreshFormFields();
    document.dispatchEvent(new CustomEvent("rustpdf:doc-changed"));
  } catch (e) {
    setStatus(t("st.error", { e: String(e) }));
  }
}

export async function openByPath(path: string) {
  if (!(await confirmDiscardChanges())) return;
  const t0 = performance.now();
  setStatus(t("st.opening", { name: fileNameFromPath(path) }));
  let doc: OpenedDocument | null = null;
  let password: string | null = null;
  let attempts = 0;
  while (doc === null) {
    try {
      doc = await invoke<OpenedDocument>("open_pdf_path", { path, password });
    } catch (e) {
      const err = e as OpenErr | string;
      const kind = typeof err === "object" ? err.kind : "Other";
      if (kind === "PasswordRequired" || kind === "PasswordIncorrect") {
        if (attempts >= 3) { setStatus(t("pw.tooMany")); return; }
        const prompt = kind === "PasswordIncorrect" ? t("pw.incorrect") : t("pw.required");
        const input = await promptPassword(prompt);
        if (input === null) { setStatus(t("pw.cancelled")); return; }
        password = input; attempts += 1; continue;
      }
      const msg = typeof err === "string" ? err : err.value ?? "unknown error";
      setStatus(t("st.error", { e: msg })); return;
    }
  }
  console.info(`open_pdf_path round-trip: ${(performance.now() - t0).toFixed(1)}ms`);
  applyOpenedDoc(doc, path);
  try { await invoke("push_recent", { path }); } catch {}
}

// ---------- Outline ----------

export function renderOutline() {
  dom.outlinePanel.innerHTML = "";
  if (state.outline.length === 0) {
    const empty = document.createElement("div");
    empty.className = "attachEmpty";
    empty.textContent = t("outline.none");
    dom.outlinePanel.appendChild(empty);
    return;
  }
  const tree = buildOutlineTree(state.outline);
  dom.outlinePanel.appendChild(tree);
}

function buildOutlineTree(nodes: OutlineNode[]): HTMLElement {
  const wrap = document.createElement("div");
  for (const n of nodes) {
    const item = document.createElement("div");
    item.className = "outlineItem";
    item.textContent = n.title || t("outline.untitled");
    item.title = n.title;
    if (n.page !== null) {
      const page = n.page;
      item.addEventListener("click", () => navigateTo(page));
    }
    wrap.appendChild(item);
    if (n.children.length > 0) {
      const sub = buildOutlineTree(n.children);
      sub.className = "outlineChildren";
      wrap.appendChild(sub);
    }
  }
  return wrap;
}

// ---------- Thumbs ----------

export function renderThumbsShell() {
  dom.thumbsPanel.innerHTML = "";
  state.thumbEls.length = 0;
  state.thumbLoaded.clear();
  state.thumbInFlight.clear();
  if (state.thumbObserver) {
    state.thumbObserver.disconnect();
    state.thumbObserver = null;
  }
  state.thumbWraps.length = 0;
  for (let i = 0; i < state.pageCount; i++) {
    const wrap = document.createElement("div");
    wrap.className = "thumb";
    wrap.dataset.page = String(i);
    wrap.draggable = false;
    const img = document.createElement("img");
    img.className = "thumbImg";
    img.dataset.page = String(i);
    img.draggable = false;
    img.alt = t("page.one", { n: i + 1 });
    const sz = state.pageSizes[i];
    if (sz) {
      const ratio = sz.height_pt / Math.max(sz.width_pt, 1);
      wrap.style.width = "170px";
      img.style.width = "170px";
      img.style.height = `${Math.round(170 * ratio)}px`;
    }
    const label = document.createElement("div");
    label.className = "thumbLabel";
    label.textContent = `${i + 1}`;
    wrap.appendChild(img);
    wrap.appendChild(label);
    dom.thumbsPanel.appendChild(wrap);
    state.thumbEls.push(img);
    state.thumbWraps.push(wrap);
  }
  if (!dom.sidebarEl.classList.contains("hidden") || document.body.classList.contains("organize")) {
    const activeTab = document.querySelector(".tab.active") as HTMLElement | null;
    if (activeTab?.dataset.tab === "thumbs" || document.body.classList.contains("organize")) armThumbObserver();
  }
  refreshSelectionUi();
  updateChrome();
}

export function armThumbObserver() {
  if (state.thumbObserver || state.thumbEls.length === 0) return;
  state.thumbObserver = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        const i = parseInt((e.target as HTMLElement).dataset.page || "-1", 10);
        if (i < 0) continue;
        loadThumb(i);
      }
    },
    { root: dom.thumbsPanel, rootMargin: "150px 0px", threshold: 0.01 },
  );
  for (const img of state.thumbEls) state.thumbObserver.observe(img);
}

async function loadThumb(i: number) {
  if (!state.currentDoc) return;
  if (state.thumbLoaded.has(i) || state.thumbInFlight.has(i)) return;
  const doc = state.currentDoc;
  const gen = state.contentGen;
  const sz = state.pageSizes[i];
  if (!sz) return;
  state.thumbInFlight.add(i);
  try {
    const res = await invoke<Rendered>("render_thumbnail", {
      document: doc,
      page: i,
      maxWidth: 170,
      pageWidthPt: sz.width_pt,
    });
    // Drop thumbnails of a closed document or of a page order that has
    // since changed (delete / reorder / undo rebuild the thumb strip).
    if (doc !== state.currentDoc || gen !== state.contentGen || !state.thumbEls[i]) return;
    state.thumbEls[i].src = res.data_url;
    state.thumbLoaded.add(i);
  } catch {
  } finally {
    if (gen === state.contentGen) state.thumbInFlight.delete(i);
  }
}

// ---------- Attachments ----------

export async function loadAttachments() {
  dom.attachmentsPanel.innerHTML = "";
  if (!state.currentDoc) return;
  const doc = state.currentDoc;
  let list: AttachmentInfo[] = [];
  try {
    list = await invoke<AttachmentInfo[]>("list_attachments_cmd", { document: doc });
  } catch {
    list = [];
  }
  if (doc !== state.currentDoc) return;
  if (list.length === 0) {
    const empty = document.createElement("div");
    empty.className = "attachEmpty";
    empty.textContent = t("attach.none");
    dom.attachmentsPanel.appendChild(empty);
    return;
  }
  for (const a of list) {
    const row = document.createElement("div");
    row.className = "attachmentItem";
    const name = document.createElement("span");
    name.className = "name";
    name.title = a.name;
    name.textContent = a.name || t("attach.name", { n: a.index + 1 });
    const size = document.createElement("span");
    size.className = "size";
    size.textContent = formatSize(a.size);
    const btn = document.createElement("button");
    btn.textContent = t("attach.save");
    btn.addEventListener("click", async () => {
      const defaultName = a.name || `attachment-${a.index + 1}`;
      const dest = await save({ defaultPath: defaultName });
      if (!dest || typeof dest !== "string") return;
      try {
        await invoke("save_attachment_cmd", { document: doc, index: a.index, path: dest });
        setStatus(t("st.attachSaved", { path: dest }));
      } catch (e) {
        setStatus(t("st.saveError", { e: String(e) }));
      }
    });
    row.appendChild(name);
    row.appendChild(size);
    row.appendChild(btn);
    dom.attachmentsPanel.appendChild(row);
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

// ---------- Properties ----------

function propsMessage(panel: HTMLElement, text: string, error = false) {
  panel.innerHTML = "";
  const div = document.createElement("div");
  div.style.color = error ? "var(--danger)" : "var(--text-3)";
  div.textContent = text;
  panel.appendChild(div);
}

export async function openProperties() {
  if (!state.currentDoc) return;
  dom.propsOverlay.classList.remove("hidden");
  propsMessage(dom.propsInfoPanel, t("st.loading"));
  dom.propsPermsPanel.innerHTML = "";
  try {
    const [md, perms] = await Promise.all([
      invoke<DocMetadata>("document_metadata_cmd", { document: state.currentDoc }),
      invoke<DocPermissions>("document_permissions_cmd", { document: state.currentDoc }),
    ]);
    renderPropsInfo(md);
    renderPropsPerms(perms);
  } catch (e) {
    propsMessage(dom.propsInfoPanel, t("st.error", { e: String(e) }), true);
  }
}

function renderPropsInfo(md: DocMetadata) {
  const rows: [string, string | null][] = [
    [t("props.title"), md.title],
    [t("props.author"), md.author],
    [t("props.subject"), md.subject],
    [t("props.keywords"), md.keywords],
    [t("props.creator"), md.creator],
    [t("props.producer"), md.producer],
    [t("props.created"), md.creation_date],
    [t("props.modified"), md.modification_date],
  ];
  dom.propsInfoPanel.innerHTML = "";
  for (const [k, v] of rows) {
    const row = document.createElement("div");
    row.className = "propsRow";
    const key = document.createElement("div");
    key.className = "k";
    key.textContent = k;
    const val = document.createElement("div");
    val.className = "v" + (v ? "" : " no");
    val.textContent = v || "—";
    row.appendChild(key);
    row.appendChild(val);
    dom.propsInfoPanel.appendChild(row);
  }
}

function renderPropsPerms(p: DocPermissions) {
  const rows: [string, string | boolean][] = [
    [t("perm.handler"), p.security_handler],
    [t("perm.printHq"), p.can_print_high_quality],
    [t("perm.printLq"), p.can_print_low_quality],
    [t("perm.modifyContent"), p.can_modify_content],
    [t("perm.extract"), p.can_extract_text_and_graphics],
    [t("perm.annotateDoc"), p.can_annotate],
    [t("perm.fill"), p.can_fill_forms],
    [t("perm.assemble"), p.can_assemble],
  ];
  dom.propsPermsPanel.innerHTML = "";
  for (const [k, v] of rows) {
    const row = document.createElement("div");
    row.className = "propsRow";
    const key = document.createElement("div");
    key.className = "k";
    key.textContent = k;
    const val = document.createElement("div");
    if (typeof v === "boolean") {
      val.className = "v " + (v ? "yes" : "bad");
      val.textContent = v ? t("perm.allowed") : t("perm.denied");
    } else {
      val.className = "v";
      val.textContent = v;
    }
    row.appendChild(key);
    row.appendChild(val);
    dom.propsPermsPanel.appendChild(row);
  }
}

export function closeProperties() {
  dom.propsOverlay.classList.add("hidden");
}

// ---------- Doc snapshot / Undo / Redo ----------

export function applyDocSnapshot(snap: DocSnapshot) {
  const prevScrollTop = dom.viewer.scrollTop;
  const prevScrollLeft = dom.viewer.scrollLeft;
  const prevPageCount = state.pageCount;
  const prevPageSizes = state.pageSizes;
  const sameGeometry =
    prevPageCount === snap.page_count &&
    prevPageSizes.length === snap.page_sizes.length &&
    prevPageSizes.every(
      (p, i) =>
        p.width_pt === snap.page_sizes[i].width_pt &&
        p.height_pt === snap.page_sizes[i].height_pt,
    );

  // Page indices may now point at different pages (delete / reorder /
  // insert / undo): pending redaction marks would hit the wrong content.
  callbacks.discardPendingRedactions();
  state.contentGen++;
  state.pageCount = snap.page_count;
  state.pageSizes = snap.page_sizes;
  state.outline = snap.outline;
  state.detailScale = snap.detail_scale ?? null;
  if (state.currentPage >= state.pageCount) state.currentPage = Math.max(0, state.pageCount - 1);
  state.textRunsCache.clear();
  state.linksCache.clear();
  state.hits = [];
  state.currentHit = -1;
  dom.searchCount.textContent = "";
  renderOutline();
  if (sameGeometry) {
    // Supersede renders still in flight from before the edit — they would
    // paint (and mark loaded) a stale bitmap.
    invalidateRenders();
    dom.viewer.scrollTop = prevScrollTop;
    dom.viewer.scrollLeft = prevScrollLeft;
    for (let i = 0; i < state.slots.length; i++) {
      if (Math.abs(i - state.currentPage) <= MAX_RENDER_DIST) scheduleRender(i);
    }
  } else {
    state.pageRotation.clear();
    rebuildSlots();
    if (state.viewMode === "single") navigateTo(state.currentPage, { recordHistory: false });
    else scheduleRender(state.currentPage);
  }
  renderThumbsShell();
  updateChrome();
  markDirty();
  void refreshUndoUi();
  void callbacks.notifyAnnotationsChanged();
}

export async function refreshUndoUi() {
  if (!state.currentDoc) {
    dom.undoBtn.disabled = true;
    dom.redoBtn.disabled = true;
    return;
  }
  try {
    const s = await invoke<UndoStatus>("undo_status", { document: state.currentDoc });
    dom.undoBtn.disabled = s.undo === 0;
    dom.redoBtn.disabled = s.redo === 0;
  } catch {
    dom.undoBtn.disabled = true;
    dom.redoBtn.disabled = true;
  }
}

export async function doUndo() {
  if (!state.currentDoc || dom.undoBtn.disabled) return;
  setStatus(t("st.undoing"));
  try {
    const snap = await invoke<DocSnapshot | null>("undo_doc", { document: state.currentDoc });
    if (!snap) {
      setStatus(t("st.nothingUndo"));
      return;
    }
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(t("st.undone"));
  } catch (e) {
    setStatus(t("st.undoError", { e: String(e) }));
  }
}

export async function doRedo() {
  if (!state.currentDoc || dom.redoBtn.disabled) return;
  setStatus(t("st.redoing"));
  try {
    const snap = await invoke<DocSnapshot | null>("redo_doc", { document: state.currentDoc });
    if (!snap) {
      setStatus(t("st.nothingRedo"));
      return;
    }
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(t("st.redone"));
  } catch (e) {
    setStatus(t("st.redoError", { e: String(e) }));
  }
}

// ---------- Selection ----------

export function clearSelection() {
  state.selectedPages.clear();
  state.selectionAnchor = null;
  refreshSelectionUi();
}

export function refreshSelectionUi() {
  for (const w of state.thumbWraps) {
    const idx = parseInt(w.dataset.page || "-1", 10);
    w.classList.toggle("selected", state.selectedPages.has(idx));
  }
  const n = state.selectedPages.size;
  for (const id of ["deleteSelectionBtn", "extractSelectionBtn"]) {
    const btn = document.getElementById(id) as HTMLButtonElement | null;
    if (btn) btn.disabled = n === 0 || !state.currentDoc;
  }
  const hint = document.getElementById("organizeHint");
  if (hint) hint.textContent = n > 0 ? t("sel.count", { n }) : t("hint.organize");
}

export function handleThumbSelect(i: number, ev: MouseEvent | PointerEvent) {
  if (ev.shiftKey && state.selectionAnchor !== null) {
    const [a, b] = state.selectionAnchor <= i ? [state.selectionAnchor, i] : [i, state.selectionAnchor];
    state.selectedPages.clear();
    for (let k = a; k <= b; k++) state.selectedPages.add(k);
    navigateTo(i);
  } else if (ev.ctrlKey || ev.metaKey) {
    if (state.selectedPages.has(i)) state.selectedPages.delete(i);
    else state.selectedPages.add(i);
    state.selectionAnchor = i;
  } else {
    state.selectedPages.clear();
    state.selectedPages.add(i);
    state.selectionAnchor = i;
    navigateTo(i);
  }
  refreshSelectionUi();
}

// ---------- Welcome screen ----------

export async function renderWelcomeRecent() {
  const host = document.getElementById("welcomeRecent");
  if (!host) return;
  const list = await invoke<RecentFile[]>("recent_files").catch(() => [] as RecentFile[]);
  host.innerHTML = "";
  if (list.length === 0) {
    const div = document.createElement("div");
    div.className = "recentEmpty";
    div.textContent = t("welcome.noRecent");
    host.appendChild(div);
    return;
  }
  for (const f of list.slice(0, 6)) {
    const btn = document.createElement("button");
    btn.className = "recentItem";
    btn.title = f.path;
    const name = document.createElement("span");
    name.className = "recentName";
    name.textContent = fileNameFromPath(f.path);
    const path = document.createElement("span");
    path.className = "recentPath";
    path.textContent = f.path;
    btn.appendChild(name);
    btn.appendChild(path);
    btn.addEventListener("click", () => void openByPath(f.path));
    host.appendChild(btn);
  }
}

// ---------- Init ----------

export function initDoc() {
  initPasswordModal();

  // Open button
  document.getElementById("open")!.addEventListener("click", () => void openWithDialog());

  // Save / Save as
  document.getElementById("saveBtn")!.addEventListener("click", () => void saveDocument());
  document.getElementById("saveAsBtn")!.addEventListener("click", () => void saveDocumentAs());

  // Recent files dropdown
  document.getElementById("recentBtn")!.addEventListener("click", async (ev) => {
    ev.stopPropagation();
    if (!dom.recentMenu.classList.contains("hidden")) {
      dom.recentMenu.classList.add("hidden");
      return;
    }
    const list = await invoke<RecentFile[]>("recent_files").catch(() => [] as RecentFile[]);
    dom.recentMenu.innerHTML = "";
    if (list.length === 0) {
      const div = document.createElement("div");
      div.className = "empty";
      div.textContent = t("recent.none");
      dom.recentMenu.appendChild(div);
    } else {
      for (const f of list) {
        const div = document.createElement("div");
        div.className = "item";
        div.title = f.path;
        div.textContent = f.path;
        div.addEventListener("click", async () => {
          dom.recentMenu.classList.add("hidden");
          await openByPath(f.path);
        });
        dom.recentMenu.appendChild(div);
      }
    }
    dom.recentMenu.classList.remove("hidden");
  });
  document.addEventListener("click", () => dom.recentMenu.classList.add("hidden"));

  // Welcome screen: big "Open" + shortcuts to the file-less tools.
  document.getElementById("welcomeOpenBtn")!.addEventListener("click", () => void openWithDialog());
  const proxy = (from: string, to: string) =>
    document.getElementById(from)!.addEventListener("click", () => document.getElementById(to)?.click());
  proxy("welcomeMergeBtn", "mergePdfsBtn");
  proxy("welcomeImagesBtn", "imagesToPdfBtn");
  proxy("welcomeOfficeBtn", "officeToPdfBtn");
  void renderWelcomeRecent();

  // Sidebar tab switch
  document.querySelectorAll(".tab").forEach((el) => {
    el.addEventListener("click", () => {
      document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
      el.classList.add("active");
      const tab = (el as HTMLElement).dataset.tab;
      dom.thumbsPanel.classList.toggle("hidden", tab !== "thumbs");
      dom.outlinePanel.classList.toggle("hidden", tab !== "outline");
      dom.attachmentsPanel.classList.toggle("hidden", tab !== "attachments");
      if (tab === "thumbs" && !dom.sidebarEl.classList.contains("hidden")) armThumbObserver();
    });
  });

  // Sidebar-show event from viewer (avoid import cycle)
  document.addEventListener("rustpdf:arm-thumb-observer", () => armThumbObserver());

  // Undo/Redo buttons
  dom.undoBtn.addEventListener("click", () => void doUndo());
  dom.redoBtn.addEventListener("click", () => void doRedo());

  // Properties dialog
  document.getElementById("propsBtn")!.addEventListener("click", openProperties);
  document.getElementById("propsClose")!.addEventListener("click", closeProperties);
  dom.propsOverlay.addEventListener("click", (e) => {
    if (e.target === dom.propsOverlay) closeProperties();
  });
  document.querySelectorAll(".propsTab").forEach((el) => {
    el.addEventListener("click", () => {
      document.querySelectorAll(".propsTab").forEach((t) => t.classList.remove("active"));
      el.classList.add("active");
      const ptab = (el as HTMLElement).dataset.ptab;
      dom.propsInfoPanel.classList.toggle("hidden", ptab !== "info");
      dom.propsPermsPanel.classList.toggle("hidden", ptab !== "perms");
    });
  });

  // Keep the icons of dynamically built content in sync.
  refreshIcons();
}
