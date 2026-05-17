import { invoke } from "@tauri-apps/api/core";
import { open, save, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import type {
  AnnotationInfo, DocSnapshot, DrawState, FormFieldInfo, MarkupKind,
  RectPt, ShapeKindUi, Slot,
} from "./types";
import { state, callbacks } from "./state";
import { dom, setStatus } from "./dom";
import { applyDocSnapshot } from "./doc";
import { effectiveRotation, navigateTo, rotateRectToSlot } from "./viewer";

// ---------- Annotation tools (markup + shapes) ----------

function setAnnotTool(k: MarkupKind | null) {
  state.annotTool = k;
  if (k !== null) state.shapeTool = null;
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.classList.toggle("active", b.dataset.annot === k);
  });
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.classList.toggle("active", false);
  });
  document.body.classList.toggle("annotMode", k !== null);
  document.body.classList.toggle("shapeMode", false);
}

function setShapeTool(k: ShapeKindUi | null) {
  state.shapeTool = k;
  if (k !== null) state.annotTool = null;
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.classList.toggle("active", b.dataset.shape === k);
  });
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.classList.toggle("active", false);
  });
  document.body.classList.toggle("shapeMode", k !== null);
  document.body.classList.toggle("annotMode", false);
}

function hexToRgb01(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return [1, 0.95, 0];
  const v = parseInt(m[1], 16);
  return [((v >> 16) & 0xff) / 255, ((v >> 8) & 0xff) / 255, (v & 0xff) / 255];
}

function pageOfRect(rect: DOMRect): { idx: number; slot: Slot } | null {
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;
  for (let i = 0; i < state.slots.length; i++) {
    const s = state.slots[i];
    if (!s || !s.loaded) continue;
    const bb = s.wrap.getBoundingClientRect();
    if (cx >= bb.left && cx <= bb.right && cy >= bb.top && cy <= bb.bottom) {
      return { idx: i, slot: s };
    }
  }
  return null;
}

function screenRectToPagePt(domRect: DOMRect, slot: Slot, pageIdx: number): RectPt | null {
  const sz = state.pageSizes[pageIdx];
  if (!sz) return null;
  const bb = slot.wrap.getBoundingClientRect();
  const imgW = bb.width;
  const imgH = bb.height;
  if (imgW <= 0 || imgH <= 0) return null;
  const left = domRect.left - bb.left;
  const top = domRect.top - bb.top;
  const right = left + domRect.width;
  const bottom = top + domRect.height;
  const rot = effectiveRotation(pageIdx);
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  const dh = rot === 1 || rot === 3 ? sz.width_pt : sz.height_pt;
  const sx = imgW / dw;
  const sy = imgH / dh;
  const nx0 = left / sx;
  const ny0 = top / sy;
  const nx1 = right / sx;
  const ny1 = bottom / sy;
  const w0 = sz.width_pt;
  const h0 = sz.height_pt;
  switch (rot) {
    case 0: return { x0: nx0, y0: ny0, x1: nx1, y1: ny1 };
    case 1: return { x0: ny0, y0: h0 - nx1, x1: ny1, y1: h0 - nx0 };
    case 2: return { x0: w0 - nx1, y0: h0 - ny1, x1: w0 - nx0, y1: h0 - ny0 };
    case 3: return { x0: w0 - ny1, y0: nx0, x1: w0 - ny0, y1: nx1 };
    default: return { x0: nx0, y0: ny0, x1: nx1, y1: ny1 };
  }
}

async function applyAnnotationFromSelection() {
  if (!state.annotTool || !state.currentDoc) return;
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
  const byPage = new Map<number, RectPt[]>();
  for (let i = 0; i < sel.rangeCount; i++) {
    const range = sel.getRangeAt(i);
    const rects = Array.from(range.getClientRects());
    for (const r of rects) {
      if (r.width < 1 || r.height < 1) continue;
      const hit = pageOfRect(r);
      if (!hit) continue;
      const pt = screenRectToPagePt(r, hit.slot, hit.idx);
      if (!pt) continue;
      let arr = byPage.get(hit.idx);
      if (!arr) { arr = []; byPage.set(hit.idx, arr); }
      arr.push(pt);
    }
  }
  if (byPage.size === 0) return;
  const color = hexToRgb01(dom.annotColorInput.value);
  const opacity = state.annotTool === "highlight" ? 0.4 : 1.0;
  const kind = state.annotTool;
  const pages = Array.from(byPage.keys()).sort((a, b) => a - b);
  setStatus(`Applying ${kind}…`);
  try {
    let snap: DocSnapshot | null = null;
    for (const page of pages) {
      const rects = byPage.get(page)!;
      snap = await invoke<DocSnapshot>("add_annotation", {
        document: state.currentDoc,
        args: { page, rects, kind, color, opacity, print: state.printAnnotations },
      });
    }
    sel.removeAllRanges();
    if (snap) applyDocSnapshot(snap);
    setStatus(`${kind} applied`);
  } catch (e) {
    setStatus(`${kind} error: ${e}`);
  }
}

// ---------- Shape drawing pipeline ----------

function imagePxFromEvent(slot: Slot, ev: PointerEvent): { x: number; y: number } {
  const bb = slot.wrap.getBoundingClientRect();
  return { x: ev.clientX - bb.left, y: ev.clientY - bb.top };
}

function ensureDrawLayerViewBox(slot: Slot) {
  const bb = slot.wrap.getBoundingClientRect();
  slot.drawLayer.setAttribute("viewBox", `0 0 ${bb.width} ${bb.height}`);
  slot.drawLayer.setAttribute("width", `${bb.width}`);
  slot.drawLayer.setAttribute("height", `${bb.height}`);
}

function makePreviewElement(kind: ShapeKindUi, color: string): SVGElement {
  const ns = "http://www.w3.org/2000/svg";
  if (kind === "rect" || kind === "freetext" || kind === "stamp" || kind === "region") {
    const el = document.createElementNS(ns, "rect");
    el.setAttribute("fill", kind === "region" ? "rgba(74,140,255,0.12)" : "none");
    el.setAttribute("stroke", kind === "region" ? "#4a8cff" : color);
    el.setAttribute("stroke-width", "2");
    if (kind !== "rect") el.setAttribute("stroke-dasharray", "5 4");
    return el;
  }
  if (kind === "circle") {
    const el = document.createElementNS(ns, "ellipse");
    el.setAttribute("fill", "none");
    el.setAttribute("stroke", color);
    el.setAttribute("stroke-width", "2");
    return el;
  }
  if (kind === "line") {
    const el = document.createElementNS(ns, "line");
    el.setAttribute("stroke", color);
    el.setAttribute("stroke-width", "2");
    return el;
  }
  if (kind === "arrow") {
    const g = document.createElementNS(ns, "g");
    const ln = document.createElementNS(ns, "line");
    ln.setAttribute("stroke", color);
    ln.setAttribute("stroke-width", "2");
    g.appendChild(ln);
    const head = document.createElementNS(ns, "polygon");
    head.setAttribute("fill", color);
    head.setAttribute("stroke", color);
    g.appendChild(head);
    return g;
  }
  const el = document.createElementNS(ns, "polyline");
  el.setAttribute("fill", "none");
  el.setAttribute("stroke", color);
  el.setAttribute("stroke-width", "2");
  el.setAttribute("stroke-linecap", "round");
  el.setAttribute("stroke-linejoin", "round");
  return el;
}

function updatePreview(kind: ShapeKindUi, el: SVGElement, s: { x: number; y: number }, c: { x: number; y: number }) {
  if (kind === "rect" || kind === "freetext" || kind === "stamp" || kind === "region") {
    const r = el as SVGRectElement;
    const x = Math.min(s.x, c.x), y = Math.min(s.y, c.y);
    const w = Math.abs(c.x - s.x), h = Math.abs(c.y - s.y);
    r.setAttribute("x", `${x}`); r.setAttribute("y", `${y}`);
    r.setAttribute("width", `${w}`); r.setAttribute("height", `${h}`);
  } else if (kind === "circle") {
    const e = el as SVGEllipseElement;
    const cx = (s.x + c.x) / 2, cy = (s.y + c.y) / 2;
    const rx = Math.abs(c.x - s.x) / 2, ry = Math.abs(c.y - s.y) / 2;
    e.setAttribute("cx", `${cx}`); e.setAttribute("cy", `${cy}`);
    e.setAttribute("rx", `${rx}`); e.setAttribute("ry", `${ry}`);
  } else if (kind === "line") {
    const ln = el as SVGLineElement;
    ln.setAttribute("x1", `${s.x}`); ln.setAttribute("y1", `${s.y}`);
    ln.setAttribute("x2", `${c.x}`); ln.setAttribute("y2", `${c.y}`);
  } else if (kind === "arrow") {
    const g = el as SVGGElement;
    const ln = g.firstElementChild as SVGLineElement;
    ln.setAttribute("x1", `${s.x}`); ln.setAttribute("y1", `${s.y}`);
    ln.setAttribute("x2", `${c.x}`); ln.setAttribute("y2", `${c.y}`);
    const head = g.lastElementChild as SVGPolygonElement;
    const dx = c.x - s.x, dy = c.y - s.y;
    const len = Math.hypot(dx, dy) || 1;
    const ux = dx / len, uy = dy / len;
    const size = 10;
    const tipX = c.x, tipY = c.y;
    const baseX = c.x - ux * size, baseY = c.y - uy * size;
    const px = -uy * size * 0.5, py = ux * size * 0.5;
    head.setAttribute(
      "points",
      `${tipX},${tipY} ${baseX + px},${baseY + py} ${baseX - px},${baseY - py}`,
    );
  }
  void s; void c;
}

function updateInkPreview(el: SVGElement, points: { x: number; y: number }[]) {
  const poly = el as SVGPolylineElement;
  poly.setAttribute("points", points.map((p) => `${p.x},${p.y}`).join(" "));
}

function imagePxToPagePt(slot: Slot, pageIdx: number, x: number, y: number): { x: number; y: number } | null {
  const sz = state.pageSizes[pageIdx];
  if (!sz) return null;
  const bb = slot.wrap.getBoundingClientRect();
  const imgW = bb.width, imgH = bb.height;
  if (imgW <= 0 || imgH <= 0) return null;
  const rot = effectiveRotation(pageIdx);
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  const dh = rot === 1 || rot === 3 ? sz.width_pt : sz.height_pt;
  const sx = imgW / dw, sy = imgH / dh;
  const nx = x / sx, ny = y / sy;
  const w0 = sz.width_pt, h0 = sz.height_pt;
  switch (rot) {
    case 0: return { x: nx, y: ny };
    case 1: return { x: ny, y: h0 - nx };
    case 2: return { x: w0 - nx, y: h0 - ny };
    case 3: return { x: w0 - ny, y: nx };
    default: return { x: nx, y: ny };
  }
}

function findSlotFromTarget(target: EventTarget | null): { slot: Slot; idx: number } | null {
  let node: HTMLElement | null = target as HTMLElement | null;
  while (node && node !== document.body) {
    if (node.classList && node.classList.contains("pageSlot")) {
      const idx = parseInt(node.dataset.page || "-1", 10);
      const s = state.slots[idx];
      if (s) return { slot: s, idx };
      return null;
    }
    node = node.parentElement;
  }
  return null;
}

function spawnFreeTextEditor(
  slot: Slot,
  pageIdx: number,
  rectPx: { x: number; y: number; w: number; h: number },
) {
  const ed = document.createElement("textarea");
  ed.style.position = "absolute";
  ed.style.left = `${rectPx.x}px`;
  ed.style.top = `${rectPx.y}px`;
  ed.style.width = `${rectPx.w}px`;
  ed.style.height = `${rectPx.h}px`;
  ed.style.border = "1px dashed #4a8cff";
  ed.style.background = "rgba(255,255,255,0.95)";
  ed.style.color = "#000";
  ed.style.font = "14px Helvetica, Arial, sans-serif";
  ed.style.padding = "2px 4px";
  ed.style.boxSizing = "border-box";
  ed.style.resize = "both";
  ed.style.zIndex = "20";
  slot.wrap.appendChild(ed);
  ed.focus();

  let committed = false;
  const cleanup = () => { ed.remove(); };
  const commit = async () => {
    if (committed) return;
    committed = true;
    const text = ed.value.trim();
    // Read the textarea geometry BEFORE detaching it. A removed element's
    // getBoundingClientRect collapses to all-zeros, which produces a 0-size
    // off-page rect and the freetext annotation never renders.
    const r = ed.getBoundingClientRect();
    const wrapBb = slot.wrap.getBoundingClientRect();
    cleanup();
    if (!text || !state.currentDoc) return;
    const left = r.left - wrapBb.left;
    const top = r.top - wrapBb.top;
    const right = left + r.width;
    const bottom = top + r.height;
    const tl = imagePxToPagePt(slot, pageIdx, left, top);
    const br = imagePxToPagePt(slot, pageIdx, right, bottom);
    if (!tl || !br) return;
    const rect: RectPt = {
      x0: Math.min(tl.x, br.x),
      y0: Math.min(tl.y, br.y),
      x1: Math.max(tl.x, br.x),
      y1: Math.max(tl.y, br.y),
    };
    setStatus("Adding text…");
    try {
      const fg = (document.getElementById("textFgColor") as HTMLInputElement | null)?.value || "#000000";
      const bg = (document.getElementById("textBgColor") as HTMLInputElement | null)?.value || "#ffffff";
      const bgTransparent = (document.getElementById("textBgTransparent") as HTMLInputElement | null)?.checked ?? false;
      const fontFamily = (document.getElementById("textFontFamily") as HTMLSelectElement | null)?.value || "Helv";
      const fontSize = parseFloat((document.getElementById("textFontSize") as HTMLInputElement | null)?.value || "14") || 14;
      const bgRgb = hexToRgb01(bg);
      const bg_color: [number, number, number, number] = [bgRgb[0], bgRgb[1], bgRgb[2], bgTransparent ? 0 : 1];
      const snap = await invoke<DocSnapshot>("add_freetext", {
        document: state.currentDoc,
        args: {
          page: pageIdx,
          rect,
          text,
          color: hexToRgb01(fg),
          bg_color,
          font_size: fontSize,
          font_family: fontFamily,
          rotation_quadrant: effectiveRotation(pageIdx),
          opacity: 1.0,
          print: state.printAnnotations,
        },
      });
      applyDocSnapshot(snap);
      setStatus("Text added");
    } catch (e) {
      setStatus(`text error: ${e}`);
    }
  };
  ed.addEventListener("blur", () => { void commit(); });
  ed.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { committed = true; cleanup(); }
    else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ed.blur(); }
  });
}

function spawnTextNoteEditor(
  slot: Slot,
  pageIdx: number,
  anchorPx: { x: number; y: number },
  anchorPt: { x: number; y: number },
) {
  const wrap = document.createElement("div");
  wrap.style.position = "absolute";
  wrap.style.left = `${anchorPx.x}px`;
  wrap.style.top = `${anchorPx.y}px`;
  wrap.style.width = "220px";
  wrap.style.zIndex = "20";
  wrap.style.background = "#fff8b0";
  wrap.style.color = "#000";
  wrap.style.border = "1px solid #b39a00";
  wrap.style.boxShadow = "2px 2px 6px rgba(0,0,0,0.35)";
  wrap.style.font = "13px Helvetica, Arial, sans-serif";

  const head = document.createElement("div");
  head.textContent = "📝 Sticky note";
  head.style.padding = "2px 6px";
  head.style.background = "#e6cf00";
  head.style.borderBottom = "1px solid #b39a00";
  head.style.fontSize = "12px";
  wrap.appendChild(head);

  const ed = document.createElement("textarea");
  ed.placeholder = "Type comment, Enter to save, Esc to cancel";
  ed.style.display = "block";
  ed.style.width = "100%";
  ed.style.height = "80px";
  ed.style.boxSizing = "border-box";
  ed.style.border = "0";
  ed.style.background = "transparent";
  ed.style.color = "#000";
  ed.style.font = "inherit";
  ed.style.padding = "4px 6px";
  ed.style.resize = "vertical";
  wrap.appendChild(ed);

  slot.wrap.appendChild(wrap);
  ed.focus();

  let committed = false;
  const cleanup = () => { wrap.remove(); };
  const commit = async () => {
    if (committed) return;
    committed = true;
    const text = ed.value.trim();
    cleanup();
    if (!text || !state.currentDoc) return;
    setStatus("Adding note…");
    try {
      const snap = await invoke<DocSnapshot>("add_text_note", {
        document: state.currentDoc,
        args: {
          page: pageIdx,
          x_pt: anchorPt.x,
          y_pt: anchorPt.y,
          contents: text,
          color: hexToRgb01(dom.annotColorInput.value),
          print: state.printAnnotations,
        },
      });
      applyDocSnapshot(snap);
      setStatus("Note added");
    } catch (e) {
      setStatus(`note error: ${e}`);
    }
  };
  ed.addEventListener("blur", () => { void commit(); });
  ed.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { committed = true; cleanup(); }
    else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ed.blur(); }
  });
}

// ---------- Comments ----------

const SUBTYPE_ICON: Record<string, string> = {
  Highlight: "🖍",
  Underline: "U̲",
  StrikeOut: "S̶",
  Squiggly: "〰",
  Square: "▭",
  Circle: "◯",
  Line: "／",
  Ink: "✎",
  FreeText: "T",
  Stamp: "🔖",
  Text: "📝",
};

function formatPdfDate(s: string | null): string {
  if (!s) return "";
  const m = /^D:(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})/.exec(s);
  if (!m) return s;
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}`;
}

function renderComments() {
  const kindF = dom.commentKindFilter.value;
  const authorF = dom.commentAuthorFilter.value.trim().toLowerCase();
  const requireBody = dom.commentHasContent.checked;
  const filtered = state.cachedAnnotations.filter((a) => {
    if (kindF && a.subtype !== kindF) return false;
    if (authorF && !(a.author || "").toLowerCase().includes(authorF)) return false;
    if (requireBody && !(a.contents && a.contents.trim())) return false;
    return true;
  });
  if (filtered.length === 0) {
    dom.commentListEl.innerHTML = `<div style="color:#777;padding:0.5rem;font-size:0.85rem;">No annotations${state.cachedAnnotations.length > 0 ? " match the filters" : ""}.</div>`;
    return;
  }
  const byPage = new Map<number, AnnotationInfo[]>();
  for (const a of filtered) {
    const list = byPage.get(a.page) ?? [];
    list.push(a);
    byPage.set(a.page, list);
  }
  const pages = Array.from(byPage.keys()).sort((a, b) => a - b);
  const out: string[] = [];
  for (const p of pages) {
    out.push(`<div class="commentGroup"><div class="commentGroupHead">Page ${p + 1}</div>`);
    for (const a of byPage.get(p)!) {
      const icon = SUBTYPE_ICON[a.subtype] ?? "•";
      const author = a.author ? `<span class="author">${escapeHtml(a.author)}</span>` : "";
      const date = a.modified ? `<span class="date">${escapeHtml(formatPdfDate(a.modified))}</span>` : "";
      const noprint = a.print ? "" : `<span class="nopaint">no-print</span>`;
      const body = a.contents ? `<div class="body">${escapeHtml(a.contents)}</div>` : "";
      out.push(
        `<div class="commentItem" data-page="${p}" data-id="${escapeHtml(a.id)}">` +
          `<div class="row1"><span class="kind">${icon} ${a.subtype}</span>${author}${noprint}${date}</div>` +
          body +
        `</div>`,
      );
    }
    out.push(`</div>`);
  }
  dom.commentListEl.innerHTML = out.join("");
  dom.commentListEl.querySelectorAll<HTMLDivElement>(".commentItem").forEach((el) => {
    el.addEventListener("click", () => {
      const p = parseInt(el.dataset.page || "0", 10);
      navigateTo(p, { recordHistory: true });
    });
  });
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[c]!);
}

async function refreshComments() {
  if (!state.currentDoc) {
    state.cachedAnnotations = [];
    renderComments();
    return;
  }
  try {
    state.cachedAnnotations = await invoke<AnnotationInfo[]>("list_annotations", { document: state.currentDoc });
  } catch (e) {
    setStatus(`comments error: ${e}`);
    state.cachedAnnotations = [];
  }
  renderComments();
}

function toggleCommentPanel() {
  const becomingVisible = dom.commentPanelEl.classList.contains("hidden");
  dom.commentPanelEl.classList.toggle("hidden");
  if (becomingVisible) void refreshComments();
}

// ---------- Forms ----------

function isFieldEmpty(f: FormFieldInfo): boolean {
  switch (f.kind) {
    case "text":
    case "combobox":
    case "listbox":
      return !f.value || f.value.length === 0;
    case "checkbox":
    case "radio":
      return (f.value ?? "Off") === "Off";
    case "signature":
      return true;
    default:
      return false;
  }
}

function validateForm() {
  state.invalidFieldIds.clear();
  const missing: FormFieldInfo[] = [];
  for (const f of state.formFields) {
    if (!f.required || f.readonly) continue;
    if (isFieldEmpty(f)) {
      missing.push(f);
      state.invalidFieldIds.add(f.id);
    }
  }
  for (let i = 0; i < state.slots.length; i++) renderFormFor(i);
  if (missing.length === 0) {
    const total = state.formFields.filter((f) => f.required && !f.readonly).length;
    setStatus(total === 0 ? "No required fields" : `All ${total} required fields filled`);
    return;
  }
  setStatus(
    `${missing.length} required field${missing.length > 1 ? "s" : ""} missing — first: ${missing[0].id}`,
  );
  const first = missing[0];
  const slot = state.slots[first.page];
  if (slot) {
    slot.wrap.scrollIntoView({ block: "center", behavior: "smooth" });
    const wrap = slot.formLayer.querySelector(
      `.formField[data-field-id="${CSS.escape(first.id)}"]`,
    ) as HTMLElement | null;
    const ed = wrap?.querySelector("input, textarea, select") as HTMLElement | null;
    ed?.focus();
  }
}

async function runResetForm() {
  if (!state.currentDoc) return;
  const ok = await confirmDialog(
    "Reset all form fields to their default values?",
    { title: "Reset form", kind: "warning" },
  );
  if (!ok) return;
  state.invalidFieldIds.clear();
  setStatus("Resetting form…");
  try {
    const snap = await invoke<DocSnapshot>("reset_form", { document: state.currentDoc });
    applyDocSnapshot(snap);
    setStatus("Form reset");
  } catch (e) {
    setStatus(`Reset error: ${e}`);
  }
}

async function refreshFormFields() {
  if (!state.currentDoc) {
    state.formFields = [];
    state.formFieldsByPage.clear();
    renderAllForms();
    return;
  }
  try {
    state.formFields = await invoke<FormFieldInfo[]>("list_form_fields", { document: state.currentDoc });
  } catch (e) {
    setStatus(`form fields error: ${e}`);
    state.formFields = [];
  }
  state.formFieldsByPage.clear();
  for (const f of state.formFields) {
    const list = state.formFieldsByPage.get(f.page) ?? [];
    list.push(f);
    state.formFieldsByPage.set(f.page, list);
  }
  for (const id of Array.from(state.invalidFieldIds)) {
    if (!state.formFields.some((f) => f.id === id)) state.invalidFieldIds.delete(id);
  }
  dom.formGroupEl.classList.toggle("hidden", state.formFields.length === 0);
  renderAllForms();
}

function renderAllForms() {
  const active = document.activeElement as HTMLElement | null;
  const activeWrap = active?.closest(".formField") as HTMLElement | null;
  const activeFieldId = activeWrap?.dataset.fieldId ?? null;
  let caretStart: number | null = null;
  let caretEnd: number | null = null;
  if (active && (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement)) {
    caretStart = active.selectionStart;
    caretEnd = active.selectionEnd;
  }
  for (let i = 0; i < state.slots.length; i++) renderFormFor(i);
  if (activeFieldId) {
    const wrap = document.querySelector(
      `.formField[data-field-id="${CSS.escape(activeFieldId)}"]`,
    ) as HTMLElement | null;
    const ed = wrap?.querySelector("input, textarea, select") as HTMLElement | null;
    if (ed) {
      ed.focus();
      if (
        caretStart !== null &&
        (ed instanceof HTMLInputElement || ed instanceof HTMLTextAreaElement)
      ) {
        try {
          ed.setSelectionRange(caretStart, caretEnd ?? caretStart);
        } catch {
          // Some input types reject setSelectionRange.
        }
      }
    }
  }
}

function renderFormFor(pageIdx: number) {
  const slot = state.slots[pageIdx];
  if (!slot) return;
  const list = state.formFieldsByPage.get(pageIdx) ?? [];
  const sz = state.pageSizes[pageIdx];
  if (!sz || !slot.loaded || slot.img.width === 0 || slot.img.height === 0) {
    slot.formLayer.innerHTML = "";
    return;
  }
  const existing = new Map<string, HTMLElement>();
  for (const w of Array.from(slot.formLayer.children) as HTMLElement[]) {
    const id = w.dataset.fieldId;
    if (id) existing.set(id, w);
    else w.remove();
  }
  const seen = new Set<string>();
  const rot = effectiveRotation(pageIdx);
  for (const f of list) {
    const box = rotateRectToSlot(f.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) continue;
    seen.add(f.id);
    let wrap = existing.get(f.id);
    if (!wrap) {
      wrap = buildFormFieldWrap(f);
      slot.formLayer.appendChild(wrap);
    } else {
      updateFormFieldWrap(wrap, f);
    }
    wrap.style.left = `${box.left}px`;
    wrap.style.top = `${box.top}px`;
    wrap.style.width = `${box.width}px`;
    wrap.style.height = `${box.height}px`;
  }
  for (const [id, w] of existing) {
    if (!seen.has(id)) w.remove();
  }
}

function buildFormFieldWrap(f: FormFieldInfo): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "formField";
  wrap.dataset.fieldId = f.id;
  wrap.dataset.kind = f.kind;
  if (f.readonly) wrap.classList.add("readonly");
  if (f.required) wrap.classList.add("required");
  if (state.invalidFieldIds.has(f.id)) wrap.classList.add("invalid");
  wrap.title = `${f.id}${f.required ? " *required" : ""}`;
  if (f.kind === "text") {
    const ed = f.multiline
      ? document.createElement("textarea")
      : document.createElement("input");
    if (!f.multiline) (ed as HTMLInputElement).type = f.password ? "password" : "text";
    ed.value = f.value ?? "";
    if (f.readonly) (ed as HTMLInputElement | HTMLTextAreaElement).readOnly = true;
    ed.addEventListener("blur", () => {
      if (wrap.classList.contains("readonly")) return;
      const v = (ed as HTMLInputElement | HTMLTextAreaElement).value;
      if (v === (wrap.dataset.lastValue ?? "")) return;
      wrap.dataset.lastValue = v;
      void writeFormField(f.id, v);
    });
    (ed as HTMLElement).addEventListener("keydown", (evt: Event) => {
      const e = evt as KeyboardEvent;
      if (e.key === "Enter" && !f.multiline) {
        e.preventDefault();
        (ed as HTMLInputElement).blur();
      }
      if (e.key === "Escape") {
        (ed as HTMLInputElement | HTMLTextAreaElement).value = wrap.dataset.lastValue ?? "";
        (ed as HTMLInputElement | HTMLTextAreaElement).blur();
      }
    });
    wrap.dataset.lastValue = f.value ?? "";
    wrap.appendChild(ed);
  } else if (f.kind === "checkbox" || f.kind === "radio") {
    wrap.classList.add("formCheck");
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = (f.value ?? "Off") !== "Off";
    cb.disabled = f.readonly;
    cb.addEventListener("change", () => {
      if (wrap.classList.contains("readonly")) return;
      const onValue = f.kind === "checkbox" ? "Yes" : "On";
      void writeFormField(f.id, cb.checked ? onValue : "Off");
    });
    wrap.appendChild(cb);
  } else if (f.kind === "combobox" || f.kind === "listbox") {
    const sel = document.createElement("select");
    if (f.kind === "listbox") sel.size = Math.max(2, f.options?.length ?? 2);
    const blank = document.createElement("option");
    blank.value = "";
    blank.textContent = "—";
    sel.appendChild(blank);
    for (const o of f.options ?? []) {
      const op = document.createElement("option");
      op.value = o;
      op.textContent = o;
      sel.appendChild(op);
    }
    sel.value = f.value ?? "";
    if (f.readonly) sel.disabled = true;
    sel.addEventListener("change", () => {
      if (wrap.classList.contains("readonly")) return;
      void writeFormField(f.id, sel.value);
    });
    wrap.appendChild(sel);
  } else if (f.kind === "signature") {
    wrap.classList.add("signature");
    wrap.textContent = "Signature";
  }
  return wrap;
}

function updateFormFieldWrap(wrap: HTMLElement, f: FormFieldInfo) {
  wrap.classList.toggle("readonly", f.readonly);
  wrap.classList.toggle("required", f.required);
  wrap.classList.toggle("invalid", state.invalidFieldIds.has(f.id));
  wrap.title = `${f.id}${f.required ? " *required" : ""}`;
  const active = document.activeElement;
  if (f.kind === "text") {
    const ed = wrap.querySelector("input, textarea") as
      | HTMLInputElement
      | HTMLTextAreaElement
      | null;
    if (ed && ed !== active) ed.value = f.value ?? "";
    wrap.dataset.lastValue = f.value ?? "";
    if (ed) ed.readOnly = f.readonly;
  } else if (f.kind === "checkbox" || f.kind === "radio") {
    const cb = wrap.querySelector("input[type=checkbox]") as HTMLInputElement | null;
    if (cb && cb !== active) cb.checked = (f.value ?? "Off") !== "Off";
    if (cb) cb.disabled = f.readonly;
  } else if (f.kind === "combobox" || f.kind === "listbox") {
    const sel = wrap.querySelector("select") as HTMLSelectElement | null;
    if (sel && sel !== active) sel.value = f.value ?? "";
    if (sel) sel.disabled = f.readonly;
  }
}

async function writeFormField(name: string, value: string) {
  if (!state.currentDoc) return;
  setStatus(`Setting ${name}…`);
  try {
    const updated = await invoke<FormFieldInfo | null>("set_form_field", {
      document: state.currentDoc,
      name,
      value,
    });
    if (updated) {
      const idx = state.formFields.findIndex((f) => f.id === name);
      if (idx >= 0) state.formFields[idx] = updated;
      const list = state.formFieldsByPage.get(updated.page);
      if (list) {
        const j = list.findIndex((f) => f.id === name);
        if (j >= 0) list[j] = updated;
      }
      if (state.invalidFieldIds.has(name) && !isFieldEmpty(updated)) {
        state.invalidFieldIds.delete(name);
      }
      renderFormFor(updated.page);
    }
    setStatus(`Field ${name} saved`);
  } catch (e) {
    setStatus(`form error: ${e}`);
  }
}

// ---------- Init ----------

export function initAnnot() {
  // Print-annotation toggle
  dom.printToggleBtn.addEventListener("click", () => {
    state.printAnnotations = !state.printAnnotations;
    dom.printToggleBtn.classList.toggle("active", state.printAnnotations);
    dom.printToggleBtn.title = state.printAnnotations
      ? "Include new annotations when printing (toggle)"
      : "Skip new annotations when printing (toggle)";
    setStatus(state.printAnnotations ? "New annotations will print" : "New annotations will NOT print");
  });

  // Markup tool buttons
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.addEventListener("click", () => {
      const k = b.dataset.annot as MarkupKind;
      setAnnotTool(state.annotTool === k ? null : k);
    });
  });

  // Shape tool buttons
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.addEventListener("click", async () => {
      const k = b.dataset.shape as ShapeKindUi;
      if (state.shapeTool === k) {
        setShapeTool(null);
        return;
      }
      if (k === "stamp") {
        const picked = await open({
          multiple: false,
          directory: false,
          filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
          title: "Pick stamp image",
        });
        if (!picked || typeof picked !== "string") return;
        state.stampImagePath = picked;
      }
      setShapeTool(k);
    });
  });

  // Markup applies on text-selection mouseup over the pageList
  document.addEventListener("mouseup", (ev) => {
    if (!state.annotTool) return;
    const t = ev.target as Node | null;
    if (!t || !dom.pageList.contains(t)) return;
    setTimeout(() => { void applyAnnotationFromSelection(); }, 0);
  });

  // Shape draw pipeline
  document.addEventListener("pointerdown", (ev) => {
    if (!state.shapeTool || !state.currentDoc) return;
    if (ev.button !== 0) return;
    const hit = findSlotFromTarget(ev.target);
    if (!hit) return;
    ev.preventDefault();
    ensureDrawLayerViewBox(hit.slot);
    const start = imagePxFromEvent(hit.slot, ev);
    const preview = makePreviewElement(state.shapeTool, dom.annotColorInput.value);
    hit.slot.drawLayer.appendChild(preview);
    state.drawState = {
      pageIdx: hit.idx,
      slot: hit.slot,
      startPx: start,
      currentPx: start,
      preview,
      pointerId: ev.pointerId,
    };
    if (state.shapeTool === "ink") {
      state.drawState.inkPoints = [start];
      updateInkPreview(preview, state.drawState.inkPoints);
    } else {
      updatePreview(state.shapeTool, preview, start, start);
    }
    hit.slot.drawLayer.setPointerCapture(ev.pointerId);
  });

  document.addEventListener("pointermove", (ev) => {
    if (!state.drawState || !state.shapeTool) return;
    if (ev.pointerId !== state.drawState.pointerId) return;
    const cur = imagePxFromEvent(state.drawState.slot, ev);
    state.drawState.currentPx = cur;
    if (state.shapeTool === "ink" && state.drawState.inkPoints) {
      const last = state.drawState.inkPoints[state.drawState.inkPoints.length - 1];
      if (Math.hypot(cur.x - last.x, cur.y - last.y) >= 1.0) {
        state.drawState.inkPoints.push(cur);
        updateInkPreview(state.drawState.preview, state.drawState.inkPoints);
      }
    } else {
      updatePreview(state.shapeTool, state.drawState.preview, state.drawState.startPx, cur);
    }
  });

  document.addEventListener("pointerup", async (ev) => {
    if (!state.drawState || !state.shapeTool || !state.currentDoc) return;
    if (ev.pointerId !== state.drawState.pointerId) return;
    const ds: DrawState = state.drawState;
    state.drawState = null;
    const tool = state.shapeTool;
    try {
      ds.slot.drawLayer.releasePointerCapture(ds.pointerId);
    } catch { /* ignore */ }
    ds.preview.remove();

    const sx = ds.startPx, cx = ds.currentPx;
    const dxPx = Math.abs(cx.x - sx.x), dyPx = Math.abs(cx.y - sx.y);

    if (tool === "ink") {
      const pts = ds.inkPoints || [];
      if (pts.length < 2) return;
      const stroke: number[] = [];
      for (const p of pts) {
        const pt = imagePxToPagePt(ds.slot, ds.pageIdx, p.x, p.y);
        if (!pt) continue;
        stroke.push(pt.x, pt.y);
      }
      if (stroke.length < 4) return;
      setStatus("Adding ink…");
      try {
        const snap = await invoke<DocSnapshot>("add_ink", {
          document: state.currentDoc,
          args: {
            page: ds.pageIdx,
            strokes: [stroke],
            color: hexToRgb01(dom.annotColorInput.value),
            width: 1.6,
            opacity: 1.0,
            print: state.printAnnotations,
          },
        });
        applyDocSnapshot(snap);
        setStatus("Ink added");
      } catch (e) {
        setStatus(`ink error: ${e}`);
      }
      return;
    }

    if (dxPx < 3 && dyPx < 3 && tool !== "freetext" && tool !== "note") return;

    const start = imagePxToPagePt(ds.slot, ds.pageIdx, sx.x, sx.y);
    const end = imagePxToPagePt(ds.slot, ds.pageIdx, cx.x, cx.y);
    if (!start || !end) return;

    if (tool === "note") {
      spawnTextNoteEditor(ds.slot, ds.pageIdx, sx, start);
      return;
    }

    if (tool === "region") {
      const rect: RectPt = {
        x0: Math.min(start.x, end.x),
        y0: Math.min(start.y, end.y),
        x1: Math.max(start.x, end.x),
        y1: Math.max(start.y, end.y),
      };
      if (rect.x1 - rect.x0 < 1 || rect.y1 - rect.y0 < 1) return;
      const base = state.currentDocPath
        ? state.currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
        : "document";
      const defaultName = `${base}_page_${ds.pageIdx + 1}_region.pdf`;
      const path = await save({
        title: "Save region as…",
        defaultPath: defaultName,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!path || typeof path !== "string") return;
      setStatus("Saving region…");
      try {
        await invoke("save_region", {
          document: state.currentDoc,
          page: ds.pageIdx,
          rect,
          path,
        });
        setStatus(`Region saved → ${path}`);
      } catch (e) {
        setStatus(`Region error: ${e}`);
      }
      return;
    }

    if (tool === "freetext") {
      let rectPx = {
        x: Math.min(sx.x, cx.x),
        y: Math.min(sx.y, cx.y),
        w: Math.abs(cx.x - sx.x),
        h: Math.abs(cx.y - sx.y),
      };
      if (rectPx.w < 60 || rectPx.h < 24) {
        rectPx = { x: sx.x, y: sx.y, w: 220, h: 40 };
      }
      spawnFreeTextEditor(ds.slot, ds.pageIdx, rectPx);
      return;
    }

    if (tool === "stamp") {
      if (!state.stampImagePath) {
        setStatus("Pick a stamp image first");
        return;
      }
      const rect: RectPt = {
        x0: Math.min(start.x, end.x),
        y0: Math.min(start.y, end.y),
        x1: Math.max(start.x, end.x),
        y1: Math.max(start.y, end.y),
      };
      setStatus("Stamping…");
      try {
        const snap = await invoke<DocSnapshot>("add_stamp", {
          document: state.currentDoc,
          args: { page: ds.pageIdx, rect, image_path: state.stampImagePath, opacity: 1.0, print: state.printAnnotations },
        });
        applyDocSnapshot(snap);
        setStatus("Stamp added");
      } catch (e) {
        setStatus(`stamp error: ${e}`);
      }
      return;
    }

    // rect / circle / line / arrow
    let rect: RectPt;
    if (tool === "line" || tool === "arrow") {
      rect = { x0: start.x, y0: start.y, x1: end.x, y1: end.y };
    } else {
      rect = {
        x0: Math.min(start.x, end.x),
        y0: Math.min(start.y, end.y),
        x1: Math.max(start.x, end.x),
        y1: Math.max(start.y, end.y),
      };
    }
    const stroke = hexToRgb01(dom.annotColorInput.value);
    setStatus(`Adding ${tool}…`);
    try {
      const snap = await invoke<DocSnapshot>("add_shape", {
        document: state.currentDoc,
        args: {
          page: ds.pageIdx,
          kind: tool,
          rect,
          stroke,
          border_width: 1.5,
          opacity: 1.0,
          print: state.printAnnotations,
        },
      });
      applyDocSnapshot(snap);
      setStatus(`${tool} added`);
    } catch (e) {
      setStatus(`shape error: ${e}`);
    }
  });

  // Comments panel buttons
  document.getElementById("commentToggleBtn")!.addEventListener("click", toggleCommentPanel);
  document.getElementById("commentCloseBtn")!.addEventListener("click", () => dom.commentPanelEl.classList.add("hidden"));
  document.getElementById("commentReloadBtn")!.addEventListener("click", () => void refreshComments());
  dom.commentKindFilter.addEventListener("change", renderComments);
  dom.commentAuthorFilter.addEventListener("input", renderComments);
  dom.commentHasContent.addEventListener("change", renderComments);

  document.getElementById("commentExportBtn")!.addEventListener("click", async () => {
    if (!state.currentDoc) return;
    const base = state.currentDocPath
      ? state.currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
      : "annotations";
    const path = await save({
      title: "Export annotations as XFDF",
      defaultPath: `${base}.xfdf`,
      filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
    });
    if (!path || typeof path !== "string") return;
    setStatus("Exporting XFDF…");
    try {
      const len = await invoke<number>("export_annotations_xfdf", { document: state.currentDoc, path });
      setStatus(`Exported ${len} bytes → ${path}`);
    } catch (e) {
      setStatus(`xfdf export error: ${e}`);
    }
  });

  document.getElementById("commentImportBtn")!.addEventListener("click", async () => {
    if (!state.currentDoc) return;
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
      title: "Import XFDF",
    });
    if (!picked || typeof picked !== "string") return;
    setStatus("Importing XFDF…");
    try {
      const snap = await invoke<DocSnapshot>("import_annotations_xfdf", {
        document: state.currentDoc,
        path: picked,
      });
      applyDocSnapshot(snap);
      setStatus("XFDF imported");
      void refreshComments();
    } catch (e) {
      setStatus(`xfdf import error: ${e}`);
    }
  });

  // F7 toggles comments
  document.addEventListener("keydown", (e) => {
    if (e.key === "F7") {
      e.preventDefault();
      toggleCommentPanel();
    }
  });

  // Form action buttons
  document.getElementById("formValidateBtn")!.addEventListener("click", validateForm);
  document.getElementById("formResetBtn")!.addEventListener("click", () => void runResetForm());

  // Register callbacks so other modules can trigger annot/form updates
  // without import cycles.
  callbacks.renderFormFor = renderFormFor;
  callbacks.refreshFormFields = refreshFormFields;
  callbacks.refreshComments = refreshComments;
  callbacks.notifyAnnotationsChanged = async () => {
    if (!dom.commentPanelEl.classList.contains("hidden")) await refreshComments();
    await refreshFormFields();
  };
}
