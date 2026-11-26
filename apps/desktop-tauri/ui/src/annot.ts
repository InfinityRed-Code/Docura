import { invoke } from "@tauri-apps/api/core";
import { open, save, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import type {
  AnnotationInfo, DocSnapshot, DrawState, FormFieldInfo, MarkupKind,
  RectPt, ShapeKindUi, Slot,
} from "./types";
import { state, callbacks } from "./state";
import { t } from "./i18n";
import { dom, setStatus } from "./dom";
import { refreshIcons } from "./icons";
import { applyDocSnapshot, markDirty, refreshUndoUi } from "./doc";
import { effectiveRotation, goToPage, navigateTo, rotateRectToSlot, setWheelZoomMode } from "./viewer";
import { noteBoxInSlot, noteRectInPage } from "./note-geom";

// ---------- Annotation tools (markup + shapes) ----------

// Clear any active Fill & Sign tool selection (sign.ts owns its own buttons,
// but markup/shape tools are mutually exclusive with it).
export function clearSignToolUi() {
  state.signTool = null;
  document.querySelectorAll<HTMLButtonElement>(".signMark").forEach((b) => b.classList.remove("active"));
  document.getElementById("signSignatureBtn")?.classList.remove("active");
  document.getElementById("signInitialsBtn")?.classList.remove("active");
  document.body.classList.remove("signMode");
}

/** Disarm every page tool (markup, shapes, eraser, fill & sign marks). */
export function disarmAllTools() {
  setAnnotTool(null);
  setShapeTool(null);
  clearEraseMode();
  clearSignToolUi();
}

/** Clear the markup tool and eraser (used when a sign mark gets armed). */
export function clearMarkupAndEraser() {
  setAnnotTool(null);
  clearEraseMode();
}

/** The Text tool (shape tool "freetext") turns lines of page text into click
 * targets; keep the body class and those targets in step with the tool. */
function syncTextEditMode() {
  const on = state.shapeTool === "freetext";
  if (document.body.classList.contains("textEditMode") === on) return;
  document.body.classList.toggle("textEditMode", on);
  callbacks.renderTextEditAll();
}

function setAnnotTool(k: MarkupKind | null) {
  state.annotTool = k;
  if (k !== null) {
    state.shapeTool = null; clearSignToolUi(); clearEraseMode();
    if (state.wheelZoomMode) setWheelZoomMode(false);
    cancelPoly();
    syncShapeMenu(null);
  }
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.classList.toggle("active", b.dataset.annot === k);
  });
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.classList.toggle("active", false);
  });
  document.body.classList.toggle("annotMode", k !== null);
  document.body.classList.toggle("shapeMode", false);
  syncTextEditMode();
}

/** The "Shapes" menu button shows (and highlights) the armed shape. */
function syncShapeMenu(k: ShapeKindUi | null) {
  const menuBtn = document.getElementById("shapeMenuBtn");
  if (!menuBtn) return;
  const item = k ? document.querySelector<HTMLElement>(`#shapeToolGroup [data-shape="${k}"]`) : null;
  menuBtn.classList.toggle("active", !!item);
  const icon = item?.dataset.icon ?? "shapes";
  const cur = menuBtn.querySelector("svg, i");
  // Lucide keeps the icon name on the generated <svg> as a class.
  const curName = cur?.getAttribute("data-lucide")
    ?? Array.from(cur?.classList ?? []).find((c) => c.startsWith("lucide-"))?.slice(7);
  if (cur && curName !== icon) {
    const i = document.createElement("i");
    i.setAttribute("data-lucide", icon);
    cur.replaceWith(i);
    refreshIcons();
  }
}

export function setShapeTool(k: ShapeKindUi | null) {
  if (poly && poly.kind !== k) cancelPoly();
  state.shapeTool = k;
  if (k !== null) {
    state.annotTool = null; clearSignToolUi(); clearEraseMode();
    if (state.wheelZoomMode) setWheelZoomMode(false);
  }
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.classList.toggle("active", b.dataset.shape === k);
  });
  syncShapeMenu(k);
  if (k === "polygon" || k === "polyline") setStatus(t("hint.polyDraw"));
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.classList.toggle("active", false);
  });
  document.body.classList.toggle("shapeMode", k !== null);
  document.body.classList.toggle("annotMode", false);
  syncTextEditMode();
}

// Eraser tool: click an existing annotation to delete it.
export function clearEraseMode() {
  state.eraseMode = false;
  document.getElementById("eraseAnnotBtn")?.classList.remove("active");
  document.body.classList.remove("eraseMode");
}

function setEraseMode(on: boolean) {
  state.eraseMode = on;
  if (on) {
    if (state.wheelZoomMode) setWheelZoomMode(false);
    state.annotTool = null;
    state.shapeTool = null;
    cancelPoly();
    syncShapeMenu(null);
    clearSignToolUi();
    document.querySelectorAll<HTMLButtonElement>(".annotTool,.shapeTool").forEach((b) => {
      b.classList.remove("active");
    });
    document.body.classList.remove("annotMode", "shapeMode");
    syncTextEditMode();
  }
  document.getElementById("eraseAnnotBtn")?.classList.toggle("active", on);
  document.body.classList.toggle("eraseMode", on);
}

// Topmost annotation (last in document order) whose rectangle contains the
// page-space point, or null. Used by the eraser hit-test.
function annotationAtPagePoint(pageIdx: number, px: number, py: number): AnnotationInfo | null {
  let best: AnnotationInfo | null = null;
  for (const a of state.cachedAnnotations) {
    if (a.page !== pageIdx) continue;
    // A note icon stays upright in a rotated view: hit what is shown.
    const r = a.subtype === "Text" ? noteRectInPage(a.rect_pt, effectiveRotation(pageIdx)) : a.rect_pt;
    const x0 = Math.min(r.x0, r.x1), x1 = Math.max(r.x0, r.x1);
    const y0 = Math.min(r.y0, r.y1), y1 = Math.max(r.y0, r.y1);
    if (px >= x0 && px <= x1 && py >= y0 && py <= y1) best = a;
  }
  return best;
}

async function eraseAnnotation(a: AnnotationInfo) {
  if (!state.currentDoc) return;
  setStatus(t("annot.removing"));
  try {
    const snap = await invoke<DocSnapshot>("delete_annotation", {
      document: state.currentDoc,
      page: a.page,
      id: a.id,
    });
    applyDocSnapshot(snap);
    await refreshComments();
    setStatus(t("annot.removed"));
  } catch (e) {
    setStatus(t("annot.removeError", { e: String(e) }));
  }
}

export function hexToRgb01(hex: string): [number, number, number] {
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

// Union selection rectangles that lie on the same line so the resulting
// markup is one continuous bar per line instead of one quad per word. Without
// this, the per-word rects from `range.getClientRects()` can overlap (e.g. the
// OCR text layer), and overlapping semi-transparent highlight quads stack into
// darker "double-marked" seams.
function mergeLineRects(rects: RectPt[]): RectPt[] {
  const norm = rects.map((r) => ({
    x0: Math.min(r.x0, r.x1),
    x1: Math.max(r.x0, r.x1),
    y0: Math.min(r.y0, r.y1),
    y1: Math.max(r.y0, r.y1),
  }));
  let changed = true;
  while (changed) {
    changed = false;
    for (let i = 0; i < norm.length && !changed; i++) {
      for (let j = i + 1; j < norm.length; j++) {
        const a = norm[i];
        const b = norm[j];
        const minH = Math.min(a.y1 - a.y0, b.y1 - b.y0);
        const vOverlap = Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0);
        const sameLine = minH > 0 && vOverlap > minH * 0.5;
        // Merge when the x-spans overlap or sit within ~one space of each other.
        const hClose = Math.max(a.x0, b.x0) <= Math.min(a.x1, b.x1) + minH * 0.6;
        if (sameLine && hClose) {
          a.x0 = Math.min(a.x0, b.x0);
          a.x1 = Math.max(a.x1, b.x1);
          a.y0 = Math.min(a.y0, b.y0);
          a.y1 = Math.max(a.y1, b.y1);
          norm.splice(j, 1);
          changed = true;
          break;
        }
      }
    }
  }
  return norm.map((r) => ({ x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1 }));
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
  const label = toolNoun(kind);
  setStatus(t("st.adding", { label }));
  try {
    let snap: DocSnapshot | null = null;
    for (const page of pages) {
      const rects = mergeLineRects(byPage.get(page)!);
      snap = await invoke<DocSnapshot>("add_annotation", {
        document: state.currentDoc,
        args: { page, rects, kind, color, opacity, print: state.printAnnotations },
      });
    }
    sel.removeAllRanges();
    if (snap) applyDocSnapshot(snap);
    setStatus(t("st.added", { label }));
  } catch (e) {
    setStatus(t("st.addError", { label, e: String(e) }));
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

type Pt = { x: number; y: number };

/** Shapes drawn by dragging a box, stored as a polygon / polyline through
 * these corner points. */
const BOX_VERTEX_SHAPES = new Set<ShapeKindUi>(["triangle", "diamond", "hexagon", "star", "callout", "check", "cloud"]);
/** Shapes with an interior (take the fill color). */
const CLOSED_SHAPES = new Set<ShapeKindUi>(["rect", "circle", "triangle", "diamond", "hexagon", "star", "callout", "cloud", "polygon"]);

/** Fill color of closed shapes (#rrggbb), or null for none. */
function shapeFill(): string | null {
  const v = (document.getElementById("shapeFill") as HTMLInputElement | null)?.value ?? "";
  return /^#[0-9a-f]{6}$/i.test(v) ? v : null;
}

/** Corner points of a box-drawn shape spanning the drag from `s` to `c`
 * (slot pixels). */
function shapeVertices(kind: ShapeKindUi, s: Pt, c: Pt): Pt[] {
  const x0 = Math.min(s.x, c.x), y0 = Math.min(s.y, c.y);
  const w = Math.abs(c.x - s.x), h = Math.abs(c.y - s.y);
  const at = (fx: number, fy: number): Pt => ({ x: x0 + fx * w, y: y0 + fy * h });
  switch (kind) {
    case "triangle": return [at(0.5, 0), at(1, 1), at(0, 1)];
    case "diamond": return [at(0.5, 0), at(1, 0.5), at(0.5, 1), at(0, 0.5)];
    case "hexagon": return [at(0.25, 0), at(0.75, 0), at(1, 0.5), at(0.75, 1), at(0.25, 1), at(0, 0.5)];
    case "star": {
      // Five-pointed star, stretched to fill the box.
      const raw: Pt[] = [];
      for (let i = 0; i < 10; i++) {
        const r = i % 2 ? 0.382 : 1;
        const a = -Math.PI / 2 + (i * Math.PI) / 5;
        raw.push({ x: r * Math.cos(a), y: r * Math.sin(a) });
      }
      const minX = Math.min(...raw.map((p) => p.x)), maxX = Math.max(...raw.map((p) => p.x));
      const minY = Math.min(...raw.map((p) => p.y)), maxY = Math.max(...raw.map((p) => p.y));
      return raw.map((p) => at((p.x - minX) / (maxX - minX), (p.y - minY) / (maxY - minY)));
    }
    case "callout":
      return [at(0, 0), at(1, 0), at(1, 0.72), at(0.42, 0.72), at(0.16, 1), at(0.24, 0.72), at(0, 0.72)];
    case "check": return [at(0, 0.55), at(0.36, 1), at(1, 0)];
    case "cloud": return [at(0, 0), at(1, 0), at(1, 1), at(0, 1)];
    default: return [];
  }
}

/** Slot pixels per PDF point on a page at the current zoom. */
function pxPerPt(slot: Slot, pageIdx: number): number {
  const sz = state.pageSizes[pageIdx];
  if (!sz) return 1;
  const rot = effectiveRotation(pageIdx);
  const w = slot.wrap.getBoundingClientRect().width;
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  return dw > 0 && w > 0 ? w / dw : 1;
}

/** SVG path of a cloud around `pts` (screen coordinates, y down): every
 * edge split into half circles bulging outwards, as the backend draws it. */
function cloudPathD(pts: Pt[], r: number): string {
  let area = 0;
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i], b = pts[(i + 1) % pts.length];
    area += a.x * b.y - b.x * a.y;
  }
  // Positive area = clockwise on screen; sweeping clockwise from the
  // segment start then bulges outward.
  const sweep = area >= 0 ? 1 : 0;
  let d = "";
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i], b = pts[(i + 1) % pts.length];
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    if (len < 0.5) continue;
    const n = Math.max(1, Math.round(len / (2 * r)));
    if (!d) d = `M${a.x},${a.y}`;
    const rr = len / n / 2;
    for (let k = 1; k <= n; k++) {
      const x = a.x + ((b.x - a.x) * k) / n, y = a.y + ((b.y - a.y) * k) / n;
      d += ` A${rr},${rr} 0 0 ${sweep} ${x},${y}`;
    }
  }
  return d ? `${d} Z` : "";
}

function arrowHeadPoints(tip: Pt, from: Pt, size = 10): string {
  const dx = tip.x - from.x, dy = tip.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len, uy = dy / len;
  const bx = tip.x - ux * size, by = tip.y - uy * size;
  const px = -uy * size * 0.5, py = ux * size * 0.5;
  return `${tip.x},${tip.y} ${bx + px},${by + py} ${bx - px},${by - py}`;
}

function makePreviewElement(kind: ShapeKindUi, color: string): SVGElement {
  const ns = "http://www.w3.org/2000/svg";
  const fill = CLOSED_SHAPES.has(kind) ? shapeFill() : null;
  const styled = <T extends SVGElement>(el: T): T => {
    el.setAttribute("stroke", color);
    el.setAttribute("stroke-width", "2");
    el.setAttribute("stroke-linejoin", "round");
    el.setAttribute("stroke-linecap", "round");
    el.setAttribute("fill", fill ?? "none");
    if (fill) el.setAttribute("fill-opacity", "0.6");
    return el;
  };
  if (kind === "freetext" || kind === "stamp" || kind === "region" || kind === "redact") {
    const el = document.createElementNS(ns, "rect");
    el.setAttribute("fill", kind === "region" ? "rgba(74,140,255,0.12)" : kind === "redact" ? "rgba(0,0,0,0.55)" : "none");
    el.setAttribute("stroke", kind === "region" ? "#4a8cff" : kind === "redact" ? "#d23b3b" : color);
    el.setAttribute("stroke-width", "2");
    el.setAttribute("stroke-dasharray", "5 4");
    return el;
  }
  if (kind === "rect") return styled(document.createElementNS(ns, "rect"));
  if (kind === "circle") return styled(document.createElementNS(ns, "ellipse"));
  if (kind === "cloud") return styled(document.createElementNS(ns, "path"));
  if (kind === "check" || kind === "polyline") return styled(document.createElementNS(ns, "polyline"));
  if (BOX_VERTEX_SHAPES.has(kind) || kind === "polygon") return styled(document.createElementNS(ns, "polygon"));
  if (kind === "line") {
    const el = document.createElementNS(ns, "line");
    el.setAttribute("stroke", color);
    el.setAttribute("stroke-width", "2");
    return el;
  }
  if (kind === "arrow" || kind === "arrow2") {
    const g = document.createElementNS(ns, "g");
    const ln = document.createElementNS(ns, "line");
    ln.setAttribute("stroke", color);
    ln.setAttribute("stroke-width", "2");
    g.appendChild(ln);
    for (let i = 0; i < (kind === "arrow2" ? 2 : 1); i++) {
      const head = document.createElementNS(ns, "polygon");
      head.setAttribute("fill", color);
      head.setAttribute("stroke", color);
      g.appendChild(head);
    }
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
  if (kind === "rect" || kind === "freetext" || kind === "stamp" || kind === "region" || kind === "redact") {
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
  } else if (kind === "arrow" || kind === "arrow2") {
    const g = el as SVGGElement;
    const ln = g.firstElementChild as SVGLineElement;
    ln.setAttribute("x1", `${s.x}`); ln.setAttribute("y1", `${s.y}`);
    ln.setAttribute("x2", `${c.x}`); ln.setAttribute("y2", `${c.y}`);
    const heads = g.querySelectorAll("polygon");
    heads[0]?.setAttribute("points", arrowHeadPoints(c, s));
    heads[1]?.setAttribute("points", arrowHeadPoints(s, c));
  } else if (kind === "cloud") {
    const slot = state.drawState?.slot;
    const ppp = slot && state.drawState ? pxPerPt(slot, state.drawState.pageIdx) : 1;
    // Same bump size as the backend: a tenth of the shorter side, 3–12 pt.
    const r = Math.max(3 * ppp, Math.min(12 * ppp, Math.min(Math.abs(c.x - s.x), Math.abs(c.y - s.y)) / 10));
    el.setAttribute("d", cloudPathD(shapeVertices(kind, s, c), r));
  } else if (BOX_VERTEX_SHAPES.has(kind)) {
    el.setAttribute("points", shapeVertices(kind, s, c).map((p) => `${p.x},${p.y}`).join(" "));
  }
}

function updateInkPreview(el: SVGElement, points: { x: number; y: number }[]) {
  const poly = el as SVGPolylineElement;
  poly.setAttribute("points", points.map((p) => `${p.x},${p.y}`).join(" "));
}

export function imagePxToPagePt(slot: Slot, pageIdx: number, x: number, y: number): { x: number; y: number } | null {
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

const TOOL_KIND: Record<string, string> = {
  highlight: "Highlight", underline: "Underline", strikeout: "StrikeOut", squiggly: "Squiggly",
  freetext: "FreeText", note: "Text", ink: "Ink", stamp: "Stamp",
  rect: "Square", circle: "Circle", line: "Line",
};

/** Localized noun for an annotation tool ("Hervorhebung", "Rechteck", …). */
function toolNoun(tool: string | null): string {
  if (tool && (tool === "arrow" || tool === "arrow2" || BOX_VERTEX_SHAPES.has(tool as ShapeKindUi)
    || tool === "polygon" || tool === "polyline")) return t(`tip.${tool}`);
  const kind = tool ? TOOL_KIND[tool] : undefined;
  return kind ? t(`kind.${kind}`) : (tool ?? "");
}

/** True for controls layered over a page (editors, buttons, form fields)
 * that must receive their own clicks instead of the active page tool. */
export function isOverlayControl(target: EventTarget | null): boolean {
  return !!(target as Element | null)?.closest?.(
    "textarea, input, select, button, .stickyNote, .formField, .redactMark, .noteHotspot, .textEd, .textEdBar",
  );
}

export function findSlotFromTarget(target: EventTarget | null): { slot: Slot; idx: number } | null {
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

// ---------- Shape annotations ----------

type ShapeRequest = {
  page: number;
  kind: string;
  rect?: RectPt;
  vertices?: [number, number][];
  fill?: [number, number, number] | null;
  border_width?: number;
};

async function addShape(label: string, req: ShapeRequest) {
  if (!state.currentDoc) return;
  setStatus(t("st.adding", { label }));
  try {
    const snap = await invoke<DocSnapshot>("add_shape", {
      document: state.currentDoc,
      args: {
        border_width: 1.5,
        subject: label,
        ...req,
        stroke: hexToRgb01(dom.annotColorInput.value),
        opacity: 1.0,
        print: state.printAnnotations,
      },
    });
    applyDocSnapshot(snap);
    setStatus(t("st.added", { label }));
  } catch (e) {
    setStatus(t("st.addError", { label, e: String(e) }));
  }
}

/** Slot pixels → page points for a list of corner points. */
function toPageVertices(slot: Slot, pageIdx: number, pts: Pt[]): [number, number][] {
  const out: [number, number][] = [];
  for (const p of pts) {
    const q = imagePxToPagePt(slot, pageIdx, p.x, p.y);
    if (q) out.push([q.x, q.y]);
  }
  return out;
}

// ---------- Free polygon / polyline (click by click) ----------

type PolyDraw = {
  kind: "polygon" | "polyline";
  pageIdx: number;
  slot: Slot;
  pts: Pt[];
  cursor: Pt;
  preview: SVGElement;
};

let poly: PolyDraw | null = null;

function renderPoly() {
  if (!poly) return;
  const pts = [...poly.pts, poly.cursor];
  poly.preview.setAttribute("points", pts.map((p) => `${p.x},${p.y}`).join(" "));
}

function cancelPoly() {
  poly?.preview.remove();
  poly = null;
}

async function finishPoly() {
  const p = poly;
  if (!p) return;
  cancelPoly();
  const min = p.kind === "polygon" ? 3 : 2;
  if (p.pts.length < min) return;
  await addShape(toolNoun(p.kind), {
    page: p.pageIdx,
    kind: p.kind,
    vertices: toPageVertices(p.slot, p.pageIdx, p.pts),
    fill: p.kind === "polygon" && shapeFill() ? hexToRgb01(shapeFill()!) : null,
  });
}

/** A press with the polygon / polyline tool: start, add a corner, or (on
 * the first corner) close the polygon. */
function polyPress(slot: Slot, pageIdx: number, at: Pt) {
  const kind = state.shapeTool as "polygon" | "polyline";
  if (!poly) {
    ensureDrawLayerViewBox(slot);
    // The rubber band is an open polyline either way; a polygon shows
    // closed once it is committed.
    const preview = makePreviewElement("polyline", dom.annotColorInput.value);
    if (kind === "polygon") {
      const fill = shapeFill();
      if (fill) { preview.setAttribute("fill", fill); preview.setAttribute("fill-opacity", "0.35"); }
    }
    slot.drawLayer.appendChild(preview);
    poly = { kind, pageIdx, slot, pts: [at], cursor: at, preview };
    renderPoly();
    return;
  }
  if (pageIdx !== poly.pageIdx) return;
  const first = poly.pts[0], last = poly.pts[poly.pts.length - 1];
  if (poly.kind === "polygon" && poly.pts.length >= 3 && Math.hypot(at.x - first.x, at.y - first.y) <= 8) {
    void finishPoly();
    return;
  }
  // The second press of a double click lands on the same spot.
  if (Math.hypot(at.x - last.x, at.y - last.y) < 3) return;
  poly.pts.push(at);
  renderPoly();
}

function spawnTextNoteEditor(
  slot: Slot,
  pageIdx: number,
  anchorPx: { x: number; y: number },
  anchorPt: { x: number; y: number },
) {
  const wrap = document.createElement("div");
  wrap.className = "stickyNote";
  wrap.style.left = `${anchorPx.x}px`;
  wrap.style.top = `${anchorPx.y}px`;

  const head = document.createElement("div");
  head.className = "stickyNoteHead";
  head.textContent = t("note.title");
  wrap.appendChild(head);

  const ed = document.createElement("textarea");
  ed.placeholder = t("note.placeholder");
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
    setStatus(t("st.adding", { label: toolNoun("note") }));
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
      setStatus(t("st.added", { label: toolNoun("note") }));
    } catch (e) {
      setStatus(t("st.addError", { label: toolNoun("note"), e: String(e) }));
    }
  };
  ed.addEventListener("blur", () => { void commit(); });
  ed.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { committed = true; cleanup(); }
    else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ed.blur(); }
  });
}

// ---------- Comments ----------

// Lucide icon names per annotation subtype (rendered via refreshIcons()).
const SUBTYPE_ICON: Record<string, string> = {
  Highlight: "highlighter",
  Underline: "underline",
  StrikeOut: "strikethrough",
  Squiggly: "waves",
  Square: "square",
  Circle: "circle",
  Line: "slash",
  Polygon: "pentagon",
  PolyLine: "spline",
  Ink: "pen-line",
  FreeText: "type",
  Stamp: "stamp",
  Text: "sticky-note",
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
    const empty = document.createElement("div");
    empty.className = "attachEmpty";
    empty.textContent = state.cachedAnnotations.length > 0 ? t("comments.noMatch") : t("comments.none");
    dom.commentListEl.replaceChildren(empty);
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
    out.push(`<div class="commentGroup"><div class="commentGroupHead">${escapeHtml(t("page.one", { n: p + 1 }))}</div>`);
    for (const a of byPage.get(p)!) {
      const icon = `<i data-lucide="${Object.hasOwn(SUBTYPE_ICON, a.subtype) ? SUBTYPE_ICON[a.subtype] : "dot"}"></i>`;
      const kindKey = `kind.${a.subtype}`;
      const kindLabel = a.subject || (t(kindKey) === kindKey ? a.subtype : t(kindKey));
      const author = a.author ? `<span class="author">${escapeHtml(a.author)}</span>` : "";
      const date = a.modified ? `<span class="date">${escapeHtml(formatPdfDate(a.modified))}</span>` : "";
      const noprint = a.print ? "" : `<span class="nopaint">${escapeHtml(t("comments.noPrint"))}</span>`;
      const body = a.contents ? `<div class="body">${escapeHtml(a.contents)}</div>` : "";
      out.push(
        `<div class="commentItem" data-page="${p}" data-id="${escapeHtml(a.id)}">` +
          `<div class="row1"><span class="kind">${icon} ${escapeHtml(kindLabel)}</span>${author}${noprint}${date}</div>` +
          body +
        `</div>`,
      );
    }
    out.push(`</div>`);
  }
  dom.commentListEl.innerHTML = out.join("");
  refreshIcons();
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
    renderNotesAllSlots();
    return;
  }
  const doc = state.currentDoc;
  let list: AnnotationInfo[];
  try {
    list = await invoke<AnnotationInfo[]>("list_annotations", { document: doc });
  } catch (e) {
    setStatus(t("st.error", { e: String(e) }));
    list = [];
  }
  // A slow lookup for a document that has since been replaced must not
  // overwrite the cache (the eraser hit-tests against it).
  if (doc !== state.currentDoc) return;
  state.cachedAnnotations = list;
  renderComments();
  renderNotesAllSlots();
  callbacks.renderTextEditAll();
}

// ---------- Sticky-note popups on the page ----------
//
// Each /Text annotation gets an invisible hotspot over its icon (the icon
// itself is part of the pdfium-rendered page bitmap). Hover previews the
// note; a click pins it open, a second click closes it again.

const pinnedNotes = new Set<string>();

function buildNotePopup(a: AnnotationInfo): HTMLDivElement {
  const popup = document.createElement("div");
  popup.className = "stickyNote hidden";
  const head = document.createElement("div");
  head.className = "stickyNoteHead";
  const title = document.createElement("span");
  title.textContent = t("note.title");
  head.appendChild(title);
  const meta = document.createElement("span");
  meta.className = "meta";
  meta.textContent = [a.author, formatPdfDate(a.modified)].filter(Boolean).join(" · ");
  head.appendChild(meta);
  popup.appendChild(head);
  const body = document.createElement("div");
  body.className = "stickyNoteBody";
  body.textContent = a.contents ?? "";
  popup.appendChild(body);
  return popup;
}

export function renderNoteHotspotsFor(idx: number) {
  const slot = state.slots[idx];
  if (!slot || !slot.loaded) return;
  const sz = state.pageSizes[idx];
  if (!sz) return;
  slot.noteLayer.innerHTML = "";
  const rot = effectiveRotation(idx);
  for (const a of state.cachedAnnotations) {
    if (a.subtype !== "Text" || a.page !== idx) continue;
    // The icon is drawn upright at its pinned corner, whatever the rotation.
    const box = noteBoxInSlot(a.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) continue;

    const popup = buildNotePopup(a);
    // Open to the right of the icon; flip to the left near the page edge.
    const popupW = 224;
    const px = box.left + box.width + 6 + popupW > slot.img.width
      ? Math.max(0, box.left - popupW - 6)
      : box.left + box.width + 6;
    popup.style.left = `${px}px`;
    popup.style.top = `${Math.min(box.top, Math.max(0, slot.img.height - 120))}px`;

    const hs = document.createElement("div");
    hs.className = "noteHotspot";
    hs.style.left = `${box.left}px`;
    hs.style.top = `${box.top}px`;
    hs.style.width = `${box.width}px`;
    hs.style.height = `${box.height}px`;

    const show = () => popup.classList.remove("hidden");
    if (pinnedNotes.has(a.id)) show();
    hs.addEventListener("mouseenter", show);
    hs.addEventListener("mouseleave", () => {
      if (!pinnedNotes.has(a.id)) popup.classList.add("hidden");
    });
    hs.addEventListener("click", (e) => {
      e.stopPropagation();
      if (pinnedNotes.has(a.id)) {
        pinnedNotes.delete(a.id);
        popup.classList.add("hidden");
      } else {
        pinnedNotes.add(a.id);
        show();
      }
    });

    slot.noteLayer.appendChild(hs);
    slot.noteLayer.appendChild(popup);
  }
}

function renderNotesAllSlots() {
  for (let i = 0; i < state.slots.length; i++) renderNoteHotspotsFor(i);
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
    setStatus(total === 0 ? t("form.noRequired") : t("form.allFilled", { total }));
    return;
  }
  setStatus(t("form.missing", { count: missing.length, id: missing[0].id }));
  focusField(missing[0]);
}

async function runResetForm() {
  if (!state.currentDoc) return;
  const ok = await confirmDialog(
    t("form.resetConfirm"),
    { title: t("form.resetTitle"), kind: "warning" },
  );
  if (!ok) return;
  state.invalidFieldIds.clear();
  setStatus(t("form.resetting"));
  try {
    const snap = await invoke<DocSnapshot>("reset_form", { document: state.currentDoc });
    applyDocSnapshot(snap);
    setStatus(t("form.reset"));
  } catch (e) {
    setStatus(t("form.resetError", { e: String(e) }));
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
    setStatus(t("form.fieldsError", { e: String(e) }));
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
  updateFormHint();
  renderAllForms();
}

// Fields in fill order: by page, then top-to-bottom, then left-to-right.
function orderedFillableFields(): FormFieldInfo[] {
  return state.formFields
    .filter((f) => !f.readonly && f.kind !== "signature")
    .sort((a, b) =>
      a.page - b.page ||
      a.rect_pt.y0 - b.rect_pt.y0 ||
      a.rect_pt.x0 - b.rect_pt.x0);
}

function updateFormHint() {
  const total = state.formFields.length;
  const fillable = orderedFillableFields().length;
  const required = state.formFields.filter((f) => f.required && !f.readonly).length;
  const hasDoc = !!state.currentDoc;
  let msg: string;
  if (!hasDoc) msg = t("hint.noDoc");
  else if (total === 0) msg = t("form.noFields");
  else msg = required
    ? t("form.countReq", { total, fillable, required })
    : t("form.count", { total, fillable });
  dom.formHintEl.textContent = msg;
  const noFields = fillable === 0;
  dom.formPrevBtn.disabled = noFields;
  dom.formNextBtn.disabled = noFields;
}

let lastFieldKey: string | null = null;

function focusField(f: FormFieldInfo) {
  lastFieldKey = fieldKey(f);
  goToPage(f.page);
  let tries = 0;
  const attempt = () => {
    const wrap = document.querySelector(
      `.formField[data-field-key="${CSS.escape(fieldKey(f))}"]`,
    ) as HTMLElement | null;
    if (!wrap) {
      // Page not rendered yet — try again for ~1 s.
      if (++tries < 60) requestAnimationFrame(attempt);
      return;
    }
    wrap.scrollIntoView({ block: "center", behavior: "smooth" });
    const ed = wrap.querySelector("input, textarea, select") as HTMLElement | null;
    ed?.focus({ preventScroll: true });
  };
  requestAnimationFrame(attempt);
}

function navigateField(dir: 1 | -1) {
  const fields = orderedFillableFields();
  if (fields.length === 0) return;
  let idx = lastFieldKey ? fields.findIndex((f) => fieldKey(f) === lastFieldKey) : -1;
  idx = idx < 0
    ? (dir === 1 ? 0 : fields.length - 1)
    : (idx + dir + fields.length) % fields.length;
  focusField(fields[idx]);
}

function toggleFormHighlight() {
  const on = document.body.classList.toggle("formsNoHl");
  dom.formHighlightBtn.classList.toggle("active", !on);
}

function renderAllForms() {
  const active = document.activeElement as HTMLElement | null;
  const activeWrap = active?.closest(".formField") as HTMLElement | null;
  const activeFieldKey = activeWrap?.dataset.fieldKey ?? null;
  let caretStart: number | null = null;
  let caretEnd: number | null = null;
  if (active && (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement)) {
    caretStart = active.selectionStart;
    caretEnd = active.selectionEnd;
  }
  for (let i = 0; i < state.slots.length; i++) renderFormFor(i);
  if (activeFieldKey) {
    const wrap = document.querySelector(
      `.formField[data-field-key="${CSS.escape(activeFieldKey)}"]`,
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

/** DOM key of a field widget. Radio buttons of one group share the field
 * name, so their group index is part of the key. */
function fieldKey(f: FormFieldInfo): string {
  return f.kind === "radio" ? `${f.id}#${f.export_value ?? ""}` : f.id;
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
    const key = w.dataset.fieldKey;
    if (key) existing.set(key, w);
    else w.remove();
  }
  const seen = new Set<string>();
  const rot = effectiveRotation(pageIdx);
  for (const f of list) {
    const box = rotateRectToSlot(f.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) continue;
    const key = fieldKey(f);
    seen.add(key);
    let wrap = existing.get(key);
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
  for (const [key, w] of existing) {
    if (!seen.has(key)) w.remove();
  }
}

function buildFormFieldWrap(f: FormFieldInfo): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "formField";
  wrap.dataset.fieldId = f.id;
  wrap.dataset.fieldKey = fieldKey(f);
  wrap.dataset.kind = f.kind;
  if (f.readonly) wrap.classList.add("readonly");
  if (f.required) wrap.classList.add("required");
  if (state.invalidFieldIds.has(f.id)) wrap.classList.add("invalid");
  wrap.title = `${f.id}${f.required ? ` (${t("form.requiredMark")})` : ""}`;
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
      if (f.kind === "radio") {
        // Turn on exactly this option of the group ("radio:<index>").
        if (!cb.checked) { cb.checked = true; return; }
        void writeFormField(f.id, `radio:${f.export_value ?? "0"}`, true);
        return;
      }
      void writeFormField(f.id, cb.checked ? "Yes" : "Off");
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
    wrap.textContent = t("btn.signature");
  }
  return wrap;
}

function updateFormFieldWrap(wrap: HTMLElement, f: FormFieldInfo) {
  wrap.classList.toggle("readonly", f.readonly);
  wrap.classList.toggle("required", f.required);
  wrap.classList.toggle("invalid", state.invalidFieldIds.has(f.id));
  wrap.title = `${f.id}${f.required ? ` (${t("form.requiredMark")})` : ""}`;
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

async function writeFormField(name: string, value: string, reloadAll = false) {
  if (!state.currentDoc) return;
  setStatus(t("form.setting", { name }));
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
    // A radio choice flips its siblings too — reload every widget state.
    if (reloadAll) await refreshFormFields();
    markDirty();
    void refreshUndoUi();
    setStatus(t("form.fieldSaved", { name }));
  } catch (e) {
    setStatus(t("form.error", { e: String(e) }));
  }
}

// ---------- Init ----------

export function initAnnot() {
  // Print-annotation toggle
  dom.printToggleBtn.addEventListener("click", () => {
    state.printAnnotations = !state.printAnnotations;
    dom.printToggleBtn.classList.toggle("active", state.printAnnotations);
    setStatus(state.printAnnotations ? t("st.annotPrintOn") : t("st.annotPrintOff"));
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
          filters: [{ name: t("lbl.images"), extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
          title: t("btn.pickImage"),
        });
        if (!picked || typeof picked !== "string") return;
        state.stampImagePath = picked;
      }
      setShapeTool(k);
    });
  });

  // Eraser tool button
  document.getElementById("eraseAnnotBtn")?.addEventListener("click", () => {
    setEraseMode(!state.eraseMode);
  });

  // Eraser: click an annotation to delete it.
  document.addEventListener("pointerdown", (ev) => {
    if (!state.eraseMode || !state.currentDoc) return;
    if (ev.button !== 0) return;
    if (isOverlayControl(ev.target)) return;
    const hit = findSlotFromTarget(ev.target);
    if (!hit) return;
    const ipx = imagePxFromEvent(hit.slot, ev);
    const pt = imagePxToPagePt(hit.slot, hit.idx, ipx.x, ipx.y);
    if (!pt) return;
    const a = annotationAtPagePoint(hit.idx, pt.x, pt.y);
    if (!a) return;
    ev.preventDefault();
    void eraseAnnotation(a);
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
    // The Text tool places its editor on the press itself (textedit.ts).
    if (state.shapeTool === "freetext") return;
    if (ev.button !== 0) return;
    if (isOverlayControl(ev.target)) return;
    const hit = findSlotFromTarget(ev.target);
    if (!hit) return;
    ev.preventDefault();
    if (state.shapeTool === "polygon" || state.shapeTool === "polyline") {
      polyPress(hit.slot, hit.idx, imagePxFromEvent(hit.slot, ev));
      return;
    }
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

  // Free polygon / polyline: rubber band to the pointer, double click or
  // Enter finishes, Backspace drops the last corner, Escape cancels.
  document.addEventListener("pointermove", (ev) => {
    if (!poly) return;
    poly.cursor = imagePxFromEvent(poly.slot, ev);
    renderPoly();
  });
  document.addEventListener("dblclick", (ev) => {
    if (!poly) return;
    ev.preventDefault();
    void finishPoly();
  });
  document.addEventListener("keydown", (ev) => {
    if (!poly) return;
    const typing = (ev.target as Element | null)?.closest?.("input, textarea, select, [contenteditable]");
    if (typing) return;
    if (ev.key === "Enter") { ev.preventDefault(); ev.stopPropagation(); void finishPoly(); }
    else if (ev.key === "Escape") { ev.preventDefault(); ev.stopPropagation(); cancelPoly(); }
    else if (ev.key === "Backspace") {
      ev.preventDefault();
      ev.stopPropagation();
      poly.pts.pop();
      if (poly.pts.length === 0) cancelPoly();
      else renderPoly();
    }
  }, true);
  document.addEventListener("rustpdf:doc-changed", cancelPoly);

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
      setStatus(t("st.adding", { label: toolNoun("ink") }));
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
        setStatus(t("st.added", { label: toolNoun("ink") }));
      } catch (e) {
        setStatus(t("st.addError", { label: toolNoun("ink"), e: String(e) }));
      }
      return;
    }

    if (dxPx < 3 && dyPx < 3 && tool !== "note") return;

    const start = imagePxToPagePt(ds.slot, ds.pageIdx, sx.x, sx.y);
    const end = imagePxToPagePt(ds.slot, ds.pageIdx, cx.x, cx.y);
    if (!start || !end) return;

    if (tool === "note") {
      spawnTextNoteEditor(ds.slot, ds.pageIdx, sx, start);
      return;
    }

    if (tool === "redact") {
      const rect: RectPt = {
        x0: Math.min(start.x, end.x),
        y0: Math.min(start.y, end.y),
        x1: Math.max(start.x, end.x),
        y1: Math.max(start.y, end.y),
      };
      if (rect.x1 - rect.x0 < 2 || rect.y1 - rect.y0 < 2) return;
      state.pendingRedactions.push({ page: ds.pageIdx, rect });
      callbacks.renderRedactsFor(ds.pageIdx);
      document.dispatchEvent(new CustomEvent("rustpdf:redactions-changed"));
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
        title: t("lbl.region"),
        defaultPath: defaultName,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!path || typeof path !== "string") return;
      setStatus(t("st.saving"));
      try {
        await invoke("save_region", {
          document: state.currentDoc,
          page: ds.pageIdx,
          rect,
          path,
        });
        setStatus(t("st.saved", { path }));
      } catch (e) {
        setStatus(t("st.saveError", { e: String(e) }));
      }
      return;
    }

    if (tool === "stamp") {
      if (!state.stampImagePath) {
        setStatus(t("st.pickImageFirst"));
        return;
      }
      const rect: RectPt = {
        x0: Math.min(start.x, end.x),
        y0: Math.min(start.y, end.y),
        x1: Math.max(start.x, end.x),
        y1: Math.max(start.y, end.y),
      };
      setStatus(t("st.adding", { label: toolNoun("stamp") }));
      try {
        const snap = await invoke<DocSnapshot>("add_stamp", {
          document: state.currentDoc,
          args: {
            page: ds.pageIdx, rect, image_path: state.stampImagePath,
            // Upright in the rotated view, like a text box.
            rotation_quadrant: effectiveRotation(ds.pageIdx),
            opacity: 1.0, print: state.printAnnotations,
          },
        });
        applyDocSnapshot(snap);
        setStatus(t("st.added", { label: toolNoun("stamp") }));
      } catch (e) {
        setStatus(t("st.addError", { label: toolNoun("stamp"), e: String(e) }));
      }
      return;
    }

    const fillHex = CLOSED_SHAPES.has(tool) ? shapeFill() : null;
    const fill = fillHex ? hexToRgb01(fillHex) : null;

    // Triangle, star, cloud, …: a polygon / polyline through corner points
    // computed on screen, so they come out upright on rotated pages too.
    if (BOX_VERTEX_SHAPES.has(tool)) {
      await addShape(toolNoun(tool), {
        page: ds.pageIdx,
        kind: tool === "check" ? "polyline" : tool === "cloud" ? "cloud" : "polygon",
        vertices: toPageVertices(ds.slot, ds.pageIdx, shapeVertices(tool, sx, cx)),
        fill,
        border_width: tool === "check" ? 2.5 : 1.5,
      });
      return;
    }

    // rect / circle / line / arrow / double arrow
    let rect: RectPt;
    if (tool === "line" || tool === "arrow" || tool === "arrow2") {
      rect = { x0: start.x, y0: start.y, x1: end.x, y1: end.y };
    } else {
      rect = {
        x0: Math.min(start.x, end.x),
        y0: Math.min(start.y, end.y),
        x1: Math.max(start.x, end.x),
        y1: Math.max(start.y, end.y),
      };
    }
    await addShape(toolNoun(tool), { page: ds.pageIdx, kind: tool, rect, fill });
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
      title: t("tip.exportXfdf"),
      defaultPath: `${base}.xfdf`,
      filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
    });
    if (!path || typeof path !== "string") return;
    setStatus(t("st.saving"));
    try {
      const len = await invoke<number>("export_annotations_xfdf", { document: state.currentDoc, path });
      void len;
      setStatus(t("st.saved", { path }));
    } catch (e) {
      setStatus(t("st.error", { e: String(e) }));
    }
  });

  document.getElementById("commentImportBtn")!.addEventListener("click", async () => {
    if (!state.currentDoc) return;
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
      title: t("tip.importXfdf"),
    });
    if (!picked || typeof picked !== "string") return;
    setStatus(t("st.loading"));
    try {
      const snap = await invoke<DocSnapshot>("import_annotations_xfdf", {
        document: state.currentDoc,
        path: picked,
      });
      applyDocSnapshot(snap);
      setStatus(t("xfdf.imported"));
      void refreshComments();
    } catch (e) {
      setStatus(t("st.error", { e: String(e) }));
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
  dom.formPrevBtn.addEventListener("click", () => navigateField(-1));
  dom.formNextBtn.addEventListener("click", () => navigateField(1));
  dom.formHighlightBtn.addEventListener("click", toggleFormHighlight);
  updateFormHint();

  // Register callbacks so other modules can trigger annot/form updates
  // without import cycles.
  callbacks.renderFormFor = renderFormFor;
  callbacks.renderNotesFor = renderNoteHotspotsFor;
  callbacks.refreshFormFields = refreshFormFields;
  callbacks.refreshComments = refreshComments;
  callbacks.notifyAnnotationsChanged = async () => {
    // Always reload: the note hotspots on the page depend on the cache,
    // not just the comment panel.
    await refreshComments();
    await refreshFormFields();
  };
  // Next/previous field continues from the field the user last focused.
  document.addEventListener("focusin", (e) => {
    const w = (e.target as Element | null)?.closest?.(".formField") as HTMLElement | null;
    if (w?.dataset.fieldKey) lastFieldKey = w.dataset.fieldKey;
  });
  // Populate the annotation cache when a document is (re)opened.
  document.addEventListener("rustpdf:doc-changed", () => {
    pinnedNotes.clear();
    lastFieldKey = null;
    void refreshComments();
  });
}
