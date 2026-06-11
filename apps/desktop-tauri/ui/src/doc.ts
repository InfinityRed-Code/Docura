import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  AttachmentInfo, DocMetadata, DocPermissions, DocSnapshot,
  OpenErr, OpenedDocument, OutlineNode, RecentFile, Rendered, UndoStatus,
} from "./types";
import { state, callbacks } from "./state";
import { dom, setStatus } from "./dom";
import { t } from "./i18n";
import { refreshIcons } from "./icons";
import {
  applyViewMode, evictFarSlots, navigateTo, rebuildSlots, reflow,
  renderAllHighlights, rotatedSize, scheduleRender, setWindowTitle, updateChrome,
} from "./viewer";

const MAX_RENDER_DIST = 6;

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
    else if (e.key === "Escape") settlePw(null);
  });
  document.getElementById("pwOverlay")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("pwOverlay")) settlePw(null);
  });
}

// ---------- Open ----------

async function tryOpen(
  bytes: Uint8Array,
  password: string | null,
): Promise<OpenedDocument> {
  return await invoke<OpenedDocument>("open_pdf", {
    bytes: Array.from(bytes),
    password,
  });
}

export async function openFromBytes(bytes: Uint8Array, label: string) {
  setStatus(`Opening ${label}…`);
  let doc: OpenedDocument | null = null;
  let password: string | null = null;
  let attempts = 0;
  while (doc === null) {
    try {
      doc = await tryOpen(bytes, password);
    } catch (e) {
      const err = e as OpenErr | string;
      const kind = typeof err === "object" ? err.kind : "Other";
      if (kind === "PasswordRequired" || kind === "PasswordIncorrect") {
        if (attempts >= 3) {
          setStatus(t("pw.tooMany"));
          return;
        }
        const prompt = kind === "PasswordIncorrect" ? t("pw.incorrect") : t("pw.required");
        const input = await promptPassword(prompt);
        if (input === null) {
          setStatus(t("pw.cancelled"));
          return;
        }
        password = input;
        attempts += 1;
        continue;
      }
      const msg = typeof err === "string" ? err : err.value ?? "unknown error";
      setStatus(`Error: ${msg}`);
      return;
    }
  }
  applyOpenedDoc(doc, label);
}

export function applyOpenedDoc(doc: OpenedDocument, label: string) {
  try {
    const prior = state.currentDoc;
    state.currentDoc = doc.id;
    state.currentDocPath = label.endsWith(".pdf") ? label : null;
    clearSelection();
    if (prior && prior !== doc.id) {
      invoke("close_pdf", { document: prior }).catch(() => {});
    }
    state.pageCount = doc.page_count;
    state.pageSizes = doc.page_sizes;
    state.outline = doc.outline;
    state.currentPage = 0;
    state.hits = [];
    state.currentHit = -1;
    state.docRotation = 0;
    state.pageRotation.clear();
    state.textRunsCache.clear();
    state.linksCache.clear();
    state.formFields = [];
    state.formFieldsByPage.clear();
    dom.searchCount.textContent = "";
    renderOutline();
    rebuildSlots();
    if (state.viewMode === "single") navigateTo(0, { recordHistory: false });
    else scheduleRender(0);
    renderThumbsShell();
    void loadAttachments();
    setStatus(
      doc.protected
        ? `${label} — ${state.pageCount} pages · ${t("st.protectedSuffix")}`
        : `${label} — ${state.pageCount} pages`,
    );
    updateChrome();
    setWindowTitle(label.split(/[\\/]/).pop() ?? label);
    void refreshUndoUi();
    void callbacks.refreshFormFields();
    document.dispatchEvent(new CustomEvent("rustpdf:doc-changed"));
  } catch (e) {
    setStatus(`Error: ${e}`);
  }
}

export async function openByPath(path: string) {
  const t0 = performance.now();
  setStatus(`Opening ${path}…`);
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
      setStatus(`Error: ${msg}`); return;
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
    dom.outlinePanel.innerHTML = '<div style="color:#777;padding:0.4rem;font-size:0.85rem;">No outline</div>';
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
    item.textContent = n.title || "(untitled)";
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
  if (!dom.sidebarEl.classList.contains("hidden")) {
    const activeTab = document.querySelector(".tab.active") as HTMLElement | null;
    if (activeTab?.dataset.tab === "thumbs") armThumbObserver();
  }
  refreshSelectionUi();
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
    if (doc !== state.currentDoc) return;
    state.thumbEls[i].src = res.data_url;
    state.thumbLoaded.add(i);
  } catch {
  } finally {
    state.thumbInFlight.delete(i);
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
    empty.textContent = "No attachments";
    dom.attachmentsPanel.appendChild(empty);
    return;
  }
  for (const a of list) {
    const row = document.createElement("div");
    row.className = "attachmentItem";
    const name = document.createElement("span");
    name.className = "name";
    name.title = a.name;
    name.textContent = a.name || `Attachment ${a.index + 1}`;
    const size = document.createElement("span");
    size.className = "size";
    size.textContent = formatSize(a.size);
    const btn = document.createElement("button");
    btn.textContent = "Save";
    btn.addEventListener("click", async () => {
      const defaultName = a.name || `attachment-${a.index + 1}`;
      const dest = await save({ defaultPath: defaultName });
      if (!dest || typeof dest !== "string") return;
      try {
        await invoke("save_attachment_cmd", { document: doc, index: a.index, path: dest });
        setStatus(`Saved attachment: ${dest}`);
      } catch (e) {
        setStatus(`Save error: ${e}`);
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

export async function openProperties() {
  if (!state.currentDoc) return;
  dom.propsOverlay.classList.remove("hidden");
  dom.propsInfoPanel.innerHTML = '<div style="color:#777;">Loading…</div>';
  dom.propsPermsPanel.innerHTML = "";
  try {
    const [md, perms] = await Promise.all([
      invoke<DocMetadata>("document_metadata_cmd", { document: state.currentDoc }),
      invoke<DocPermissions>("document_permissions_cmd", { document: state.currentDoc }),
    ]);
    renderPropsInfo(md);
    renderPropsPerms(perms);
  } catch (e) {
    dom.propsInfoPanel.innerHTML = `<div style="color:#e09f9f;">Error: ${e}</div>`;
  }
}

function renderPropsInfo(md: DocMetadata) {
  const rows: [string, string | null][] = [
    ["Title", md.title],
    ["Author", md.author],
    ["Subject", md.subject],
    ["Keywords", md.keywords],
    ["Creator", md.creator],
    ["Producer", md.producer],
    ["Created", md.creation_date],
    ["Modified", md.modification_date],
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
    ["Security handler", p.security_handler],
    ["Print (high quality)", p.can_print_high_quality],
    ["Print (low quality)", p.can_print_low_quality],
    ["Modify content", p.can_modify_content],
    ["Extract text/graphics", p.can_extract_text_and_graphics],
    ["Annotate", p.can_annotate],
    ["Fill forms", p.can_fill_forms],
    ["Assemble", p.can_assemble],
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
      val.textContent = v ? "Allowed" : "Denied";
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

  state.pageCount = snap.page_count;
  state.pageSizes = snap.page_sizes;
  state.outline = snap.outline;
  if (state.currentPage >= state.pageCount) state.currentPage = Math.max(0, state.pageCount - 1);
  state.textRunsCache.clear();
  state.linksCache.clear();
  state.hits = [];
  state.currentHit = -1;
  dom.searchCount.textContent = "";
  renderOutline();
  if (sameGeometry) {
    for (const s of state.slots) {
      s.loaded = false;
      s.lastZoom = null;
    }
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
  void refreshUndoUi();
  void callbacks.notifyAnnotationsChanged();
  // suppress unused-var warnings during ts-strict
  void rotatedSize; void applyViewMode; void evictFarSlots; void reflow; void renderAllHighlights;
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
  setStatus("Undo…");
  try {
    const snap = await invoke<DocSnapshot | null>("undo_doc", { document: state.currentDoc });
    if (!snap) {
      setStatus("Nothing to undo");
      return;
    }
    clearSelection();
    applyDocSnapshot(snap);
    setStatus("Undone");
  } catch (e) {
    setStatus(`Undo error: ${e}`);
  }
}

export async function doRedo() {
  if (!state.currentDoc || dom.redoBtn.disabled) return;
  setStatus("Redo…");
  try {
    const snap = await invoke<DocSnapshot | null>("redo_doc", { document: state.currentDoc });
    if (!snap) {
      setStatus("Nothing to redo");
      return;
    }
    clearSelection();
    applyDocSnapshot(snap);
    setStatus("Redone");
  } catch (e) {
    setStatus(`Redo error: ${e}`);
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
  const btn = document.getElementById("deleteSelectionBtn") as HTMLButtonElement | null;
  if (btn) {
    btn.disabled = state.selectedPages.size === 0;
    const label = state.selectedPages.size > 0
      ? `Delete ${state.selectedPages.size} page${state.selectedPages.size === 1 ? "" : "s"}`
      : "Delete selected";
    btn.innerHTML = `<i data-lucide="trash-2"></i> ${label}`;
  }
  const ex = document.getElementById("extractSelectionBtn") as HTMLButtonElement | null;
  if (ex) {
    ex.disabled = state.selectedPages.size === 0;
    const label = state.selectedPages.size > 0
      ? `Extract ${state.selectedPages.size} page${state.selectedPages.size === 1 ? "" : "s"}`
      : "Extract selected";
    ex.innerHTML = `<i data-lucide="file-output"></i> ${label}`;
  }
  refreshIcons();
  if (dom.modeHint) {
    const base = state.selectedPages.size === 0 ? "" : `${state.selectedPages.size} selected`;
    dom.modeHint.textContent = state.editMode === "delete"
      ? (base ? `⚠ Delete mode — ${base}` : "⚠ Delete mode")
      : base;
  }
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

// ---------- Init ----------

export function initDoc() {
  initPasswordModal();

  // Open button
  document.getElementById("open")!.addEventListener("click", async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path || typeof path !== "string") return;
    await openByPath(path);
  });

  // Recent files dropdown
  document.getElementById("recentBtn")!.addEventListener("click", async (ev) => {
    ev.stopPropagation();
    if (!dom.recentMenu.classList.contains("hidden")) {
      dom.recentMenu.classList.add("hidden");
      return;
    }
    const list = await invoke<RecentFile[]>("recent_files").catch(() => []);
    dom.recentMenu.innerHTML = "";
    if (list.length === 0) {
      const div = document.createElement("div");
      div.className = "empty";
      div.textContent = "No recent files";
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
}
