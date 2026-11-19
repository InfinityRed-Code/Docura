import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  PageLink, PageSize, RectPt, Rendered, FitMode, Slot, TextRun, ViewMode,
} from "./types";
import { state, callbacks } from "./state";
import { dom, setStatus } from "./dom";
import { t } from "./i18n";

// Entries of the status-bar zoom dropdown.
const ZOOM_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 4];
// Stops used by the zoom-in / zoom-out buttons and Ctrl +/-.
const ZOOM_STEPS = [0.1, 0.25, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, 5, 6, 8, 10];
const EVICT_DIST = 10;
const MAX_RENDER_DIST = 6;
// Largest whole-page bitmap requested. Beyond it (deep zoom on large
// formats) the page bitmap stays at this size, upscaled, and the visible
// window is rendered sharply on top ("detail").
const MAX_PAGE_PIXELS = 12_000_000;
// Detail windows extend this far past the viewport and snap to this grid,
// so small scrolls reuse the current one.
const DETAIL_MARGIN = 320;
const DETAIL_SNAP = 256;
// Quiet time after the last zoom step before re-rendering.
const ZOOM_SETTLE_MS = 160;
// Layout pixels per PDF point of the text layer (independent of the zoom).
const TEXT_LAYER_SCALE = 2;

export function syncZoomUi() {
  const pct = Math.round(state.zoom * 100);
  dom.zoomCustomOpt.textContent = `${pct}%`;
  // A fit mode always shows the effective percentage; a preset is only
  // selected when the user picked an exact zoom.
  const hit = state.fitMode === "custom"
    ? ZOOM_PRESETS.find((v) => Math.abs(v - state.zoom) < 0.005)
    : undefined;
  dom.zoomSelect.value = hit !== undefined ? String(hit) : "custom";
  document.getElementById("fitWidthBtn")?.classList.toggle("active", state.fitMode === "width");
  document.getElementById("fitPageBtn")?.classList.toggle("active", state.fitMode === "page");
}

/** Switch to a fit mode (or "custom") and re-layout. */
export function setFitMode(mode: FitMode) {
  state.fitMode = mode;
  if (state.currentDoc) reflow();
  else syncZoomUi();
}

/** Step the zoom to the next/previous stop (buttons, Ctrl +/-). */
export function stepZoom(dir: 1 | -1) {
  const z = state.zoom;
  const next = dir > 0
    ? ZOOM_STEPS.find((v) => v > z + 0.001) ?? ZOOM_STEPS[ZOOM_STEPS.length - 1]
    : [...ZOOM_STEPS].reverse().find((v) => v < z - 0.001) ?? ZOOM_STEPS[0];
  state.fitMode = "custom";
  state.zoom = clampZoom(next);
  holdRendersForZoom();
  reflow();
}

let titleLabel: string | null = null;

/** Show the document name in the custom title bar and keep the native
 * window title (taskbar / alt-tab) in sync. A leading "•" marks unsaved
 * changes. */
export function setWindowTitle(label: string | null) {
  titleLabel = label;
  refreshWindowTitle();
}

export function refreshWindowTitle() {
  const span = document.getElementById("titlebarTitle");
  const name = titleLabel ? `${state.dirty ? "• " : ""}${titleLabel}` : null;
  const text = name ? `${name} — Docura PDF` : "Docura PDF";
  if (span) span.textContent = text;
  getCurrentWindow().setTitle(text).catch(() => {});
}

export function updateChrome() {
  document.body.classList.toggle("hasDoc", !!state.currentDoc);
  if (!state.currentDoc) {
    dom.pageInput.value = "1";
    dom.pageTotal.textContent = "/ 0";
    syncZoomUi();
    return;
  }
  dom.pageInput.value = String(state.currentPage + 1);
  dom.pageInput.max = String(state.pageCount);
  dom.pageTotal.textContent = `/ ${state.pageCount}`;
  syncZoomUi();
  state.thumbWraps.forEach((w, i) => w.classList.toggle("active", i === state.currentPage));
}

export function computeZoom(): number {
  if (!state.currentDoc || !state.pageSizes[state.currentPage]) return state.zoom;
  const rs = rotatedSize(state.currentPage);
  if (rs.w === 0) return state.zoom;
  const vw = dom.viewer.clientWidth - 32;
  const vh = dom.viewer.clientHeight - 32;
  if (vw <= 0 || vh <= 0) return state.zoom;
  const cols = state.viewMode === "double" ? 2 : 1;
  const colW = (vw - (cols - 1) * 10) / cols;
  switch (state.fitMode) {
    case "width":
      return clampZoom(colW / rs.w);
    case "page":
      return clampZoom(Math.min(colW / rs.w, vh / rs.h));
    case "actual":
      return 1.0;
    case "custom":
    default:
      return state.zoom;
  }
}

export function clampZoom(z: number) {
  return Math.max(0.1, Math.min(10, z));
}

export function effectiveRotation(idx: number): number {
  return ((state.docRotation + (state.pageRotation.get(idx) || 0)) % 4 + 4) % 4;
}

export function rotatedSize(idx: number): { w: number; h: number } {
  const sz = state.pageSizes[idx];
  if (!sz) return { w: 0, h: 0 };
  const rot = effectiveRotation(idx);
  if (rot === 1 || rot === 3) return { w: sz.height_pt, h: sz.width_pt };
  return { w: sz.width_pt, h: sz.height_pt };
}

export function makeSlot(pageIdx: number): Slot {
  const wrap = document.createElement("div");
  wrap.className = "pageSlot";
  wrap.dataset.page = String(pageIdx);
  const img = document.createElement("img");
  img.className = "page";
  img.alt = t("page.one", { n: pageIdx + 1 });
  const selectLayer = document.createElement("div");
  selectLayer.className = "selectLayer";
  const highlightLayer = document.createElement("div");
  highlightLayer.className = "highlightLayer";
  const linkLayer = document.createElement("div");
  linkLayer.className = "linkLayer";
  const drawLayer = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  drawLayer.setAttribute("class", "drawLayer");
  drawLayer.dataset.page = String(pageIdx);
  const noteLayer = document.createElement("div");
  noteLayer.className = "noteLayer";
  const redactLayer = document.createElement("div");
  redactLayer.className = "redactLayer";
  const formLayer = document.createElement("div");
  formLayer.className = "formLayer";
  formLayer.dataset.page = String(pageIdx);
  const placeholder = document.createElement("div");
  placeholder.className = "placeholder";
  placeholder.textContent = `${pageIdx + 1}`;
  wrap.appendChild(placeholder);
  wrap.appendChild(img);
  wrap.appendChild(selectLayer);
  wrap.appendChild(highlightLayer);
  wrap.appendChild(linkLayer);
  wrap.appendChild(noteLayer);
  wrap.appendChild(redactLayer);
  wrap.appendChild(drawLayer);
  wrap.appendChild(formLayer);
  return {
    wrap, img, selectLayer, highlightLayer, linkLayer, noteLayer, redactLayer, drawLayer, formLayer, placeholder,
    loaded: false, renderingZoom: null, lastZoom: null,
    baseZoom: null, baseRot: null, baseGen: -1, detail: null, detailKey: null,
    textKey: null, textW: 0, textH: 0,
  };
}

export function rebuildSlots() {
  dom.pageList.innerHTML = "";
  state.slots = [];
  for (let i = 0; i < state.pageCount; i++) {
    const s = makeSlot(i);
    dom.pageList.appendChild(s.wrap);
    state.slots.push(s);
  }
  applyViewMode();
  sizeSlots();
  recenterScrollX();
  setupObserver();
}

export function applyViewMode() {
  dom.pageList.classList.remove("single", "continuous", "double");
  dom.pageList.classList.add(state.viewMode);
  if (state.viewMode === "single") {
    state.slots.forEach((s, i) => s.wrap.classList.toggle("current", i === state.currentPage));
  }
}

export function sizeSlots() {
  if (state.slots.length === 0) return;
  state.zoom = computeZoom();
  for (let i = 0; i < state.slots.length; i++) {
    const rs = rotatedSize(i);
    if (rs.w === 0) continue;
    const w = Math.round(rs.w * state.zoom);
    const h = Math.round(rs.h * state.zoom);
    const slot = state.slots[i];
    slot.wrap.style.width = `${w}px`;
    slot.wrap.style.height = `${h}px`;
    slot.img.width = w;
    slot.img.height = h;
    scaleTextLayer(slot);
    if (slot.detail) placeDetail(slot.detail, state.zoom);
  }
  if (state.wheelZoomMode) {
    const bx = Math.max(16, dom.viewer.clientWidth);
    const by = Math.max(16, dom.viewer.clientHeight);
    dom.pageList.style.padding = `${by}px ${bx}px`;
  } else {
    dom.pageList.style.padding = "";
  }
}

export function recenterScrollX() {
  const extra = dom.viewer.scrollWidth - dom.viewer.clientWidth;
  if (extra > 0) dom.viewer.scrollLeft = extra / 2;
}

export function evictSlot(idx: number) {
  const slot = state.slots[idx];
  if (!slot || !slot.loaded) return;
  dropDetail(slot);
  slot.baseZoom = null;
  slot.img.removeAttribute("src");
  slot.img.width = 0;
  slot.img.height = 0;
  slot.selectLayer.innerHTML = "";
  slot.textKey = null;
  slot.highlightLayer.innerHTML = "";
  slot.linkLayer.innerHTML = "";
  slot.loaded = false;
  slot.lastZoom = null;
  state.renderQueue.delete(idx);
}

export function evictFarSlots() {
  for (let i = 0; i < state.slots.length; i++) {
    if (Math.abs(i - state.currentPage) > EVICT_DIST) evictSlot(i);
  }
}

export function setupObserver() {
  if (state.observer) state.observer.disconnect();
  state.observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        const idx = parseInt((e.target as HTMLElement).dataset.page || "-1", 10);
        if (idx < 0) continue;
        if (e.isIntersecting) {
          scheduleRender(idx);
          if (state.viewMode === "continuous" || state.viewMode === "double") {
            updateCurrentFromScroll();
          }
        } else if (Math.abs(idx - state.currentPage) > EVICT_DIST) {
          evictSlot(idx);
        }
      }
    },
    { root: dom.viewer, rootMargin: "200px 0px", threshold: 0.01 },
  );
  for (const s of state.slots) state.observer.observe(s.wrap);
}

/** Zoom at which page `idx` is rendered as a whole: the view zoom, capped
 * by the bitmap budget and — for proxied scans — the proxy's sharpness. */
export function baseZoomFor(idx: number): number {
  const rs = rotatedSize(idx);
  const area = Math.max(1, rs.w * rs.h);
  let z = Math.min(state.zoom, Math.sqrt(MAX_PAGE_PIXELS / area));
  if (state.detailScale) z = Math.min(z, state.detailScale);
  return z;
}

function needsDetail(idx: number): boolean {
  return baseZoomFor(idx) < state.zoom - 1e-3;
}

export function scheduleRender(idx: number) {
  if (!state.currentDoc) return;
  const slot = state.slots[idx];
  if (!slot) return;
  if (slot.loaded && slot.lastZoom === state.zoom) return;
  // Deep zoom on a large page: the capped bitmap is still valid — only the
  // overlays (text, links, …) and the sharp detail window need updating.
  if (
    slot.img.getAttribute("src") &&
    slot.baseGen === state.contentGen &&
    slot.baseRot === effectiveRotation(idx) &&
    slot.baseZoom !== null &&
    Math.abs(slot.baseZoom - baseZoomFor(idx)) < 1e-4
  ) {
    slot.loaded = true;
    slot.lastZoom = state.zoom;
    slot.placeholder.style.display = "none";
    void refreshLayers(idx);
    scheduleDetailUpdate();
    return;
  }
  state.renderQueue.add(idx);
  pumpRenderQueue();
}

/** Hold renders back while the user keeps zooming. */
export function holdRendersForZoom() {
  state.renderHoldUntil = performance.now() + ZOOM_SETTLE_MS;
}

let holdTimer: number | null = null;

function pruneRenderQueue() {
  for (const idx of state.renderQueue) {
    if (Math.abs(idx - state.currentPage) > MAX_RENDER_DIST) {
      state.renderQueue.delete(idx);
    }
  }
}

// Bounded render pool. PDFium rasterisation is serialised in the backend (the
// engine is not thread-safe), but JPEG-encoding runs on its own blocking task,
// so keeping a few render_page calls in flight lets one page encode while the
// next rasterises. Dispatch is always nearest-to-viewport first, so the visible
// page never waits behind far ones.
const RENDER_CONCURRENCY = 3;

function pumpRenderQueue() {
  const wait = state.renderHoldUntil - performance.now();
  if (wait > 0) {
    if (holdTimer === null) {
      holdTimer = window.setTimeout(() => {
        holdTimer = null;
        // The sharp window over the visible area first; the whole-page
        // bitmap (only seen when panning past that window) queues behind.
        void updateDetails();
        pumpRenderQueue();
      }, wait + 5);
    }
    return;
  }
  pruneRenderQueue();
  while (state.renderInFlight.size < RENDER_CONCURRENCY) {
    const nearest = pickNearest();
    if (nearest === null) break;
    state.renderQueue.delete(nearest);
    state.renderInFlight.add(nearest);
    void renderSlot(nearest).finally(() => {
      state.renderInFlight.delete(nearest);
      pumpRenderQueue();
    });
  }
}

function pickNearest(): number | null {
  let best: number | null = null;
  let bestDist = Infinity;
  for (const idx of state.renderQueue) {
    if (state.renderInFlight.has(idx)) continue;
    const d = Math.abs(idx - state.currentPage);
    if (d < bestDist) {
      bestDist = d;
      best = idx;
    }
  }
  return best;
}

async function renderSlot(idx: number) {
  if (!state.currentDoc) return;
  const slot = state.slots[idx];
  if (!slot) return;
  const docAtStart = state.currentDoc;
  const zoomAtStart = state.zoom;
  const epochAtStart = state.renderEpoch;
  const genAtStart = state.contentGen;
  const rot = effectiveRotation(idx);
  const renderZoom = baseZoomFor(idx);
  slot.renderingZoom = zoomAtStart;
  try {
    const res = await invoke<Rendered>("render_page", {
      document: docAtStart,
      page: idx,
      zoom: renderZoom,
      rotation: rot,
    });
    // Drop the result if the view was invalidated (zoom/rotate/doc change) or
    // the document switched while this render was in flight.
    if (docAtStart !== state.currentDoc || epochAtStart !== state.renderEpoch) return;
    slot.img.src = res.data_url;
    // The slot keeps its full CSS size; a capped bitmap is upscaled.
    const rs = rotatedSize(idx);
    const cw = Math.round(rs.w * zoomAtStart);
    const ch = Math.round(rs.h * zoomAtStart);
    slot.img.width = cw;
    slot.img.height = ch;
    slot.wrap.style.width = `${cw}px`;
    slot.wrap.style.height = `${ch}px`;
    slot.placeholder.style.display = "none";
    slot.loaded = true;
    slot.lastZoom = zoomAtStart;
    slot.baseZoom = renderZoom;
    slot.baseRot = rot;
    slot.baseGen = genAtStart;
    await refreshLayers(idx);
    if (needsDetail(idx)) scheduleDetailUpdate();
  } catch (e) {
    // A render for a document that has since been closed is not news.
    if (docAtStart === state.currentDoc) setStatus(t("st.error", { e: `${t("page.one", { n: idx + 1 })}: ${e}` }));
  } finally {
    slot.renderingZoom = null;
  }
}

/** Rebuild the per-page overlays (text, search hits, links, notes,
 * redactions, form fields) for the slot's current size. */
async function refreshLayers(idx: number) {
  await ensureTextLayer(idx);
  renderHighlightsFor(idx);
  await ensureLinkLayer(idx);
  callbacks.renderNotesFor(idx);
  callbacks.renderRedactsFor(idx);
  callbacks.renderFormFor(idx);
}

// ---------- Sharp detail window (deep zoom / huge scans) ----------

function dropDetail(slot: Slot) {
  slot.detail?.remove();
  slot.detail = null;
  slot.detailKey = null;
}

/** Position a detail window (rendered at its own zoom) for view `zoom`:
 * while a zoom gesture runs, the previous sharp window stays visible,
 * scaled, until its replacement arrives. */
function placeDetail(img: HTMLImageElement, zoom: number) {
  const k = zoom / Number(img.dataset.zoom);
  const x0 = Number(img.dataset.x0), y0 = Number(img.dataset.y0);
  img.style.left = `${x0 * k}px`;
  img.style.top = `${y0 * k}px`;
  img.style.width = `${(Number(img.dataset.x1) - x0) * k}px`;
  img.style.height = `${(Number(img.dataset.y1) - y0) * k}px`;
}

let detailTimer: number | null = null;
// One region render at a time: PDFium can't abort one, so firing a request
// per wheel notch only queues work whose result is stale on arrival.
let detailBusy = false;
let detailAgain = false;

/** Refresh the detail windows shortly after scrolling stops - and not
 * before a zoom gesture has settled. */
export function scheduleDetailUpdate() {
  if (detailTimer !== null) clearTimeout(detailTimer);
  const hold = state.renderHoldUntil - performance.now();
  detailTimer = window.setTimeout(() => {
    detailTimer = null;
    void updateDetails();
  }, Math.max(120, hold + 20));
}

async function updateDetails() {
  if (detailBusy) {
    detailAgain = true;
    return;
  }
  detailBusy = true;
  try {
    do {
      detailAgain = false;
      if (!state.currentDoc) return;
      if (state.renderHoldUntil > performance.now()) {
        scheduleDetailUpdate();
        return;
      }
      for (let i = 0; i < state.slots.length; i++) {
        const slot = state.slots[i];
        if (!slot.img.getAttribute("src") || !needsDetail(i)) {
          if (slot.detail) dropDetail(slot);
          continue;
        }
        const vr = dom.viewer.getBoundingClientRect();
        const wr = slot.wrap.getBoundingClientRect();
        if (wr.bottom < vr.top || wr.top > vr.bottom || wr.right < vr.left || wr.left > vr.right) {
          if (slot.detail) dropDetail(slot);
          continue;
        }
        await updateDetail(i, vr, wr);
        // The view moved on meanwhile: start over with fresh geometry.
        if (detailAgain) break;
      }
    } while (detailAgain);
  } finally {
    detailBusy = false;
  }
}

async function updateDetail(idx: number, vr: DOMRect, wr: DOMRect) {
  const slot = state.slots[idx];
  const cw = Math.round(wr.width);
  const ch = Math.round(wr.height);
  // Visible part of the page, in page-slot pixels.
  const vx0 = Math.max(0, vr.left - wr.left);
  const vy0 = Math.max(0, vr.top - wr.top);
  const vx1 = Math.min(cw, vr.right - wr.left);
  const vy1 = Math.min(ch, vr.bottom - wr.top);
  if (vx1 <= vx0 || vy1 <= vy0) return;
  const cur = slot.detail;
  const zoom = state.zoom;
  const rot = effectiveRotation(idx);
  if (
    cur && cur.dataset.zoom === String(zoom) &&
    cur.dataset.rot === String(rot) && cur.dataset.gen === String(state.contentGen) &&
    Number(cur.dataset.x0) <= vx0 && Number(cur.dataset.y0) <= vy0 &&
    Number(cur.dataset.x1) >= vx1 && Number(cur.dataset.y1) >= vy1
  ) return; // current window still covers the view
  const x0 = Math.max(0, Math.floor((vx0 - DETAIL_MARGIN) / DETAIL_SNAP) * DETAIL_SNAP);
  const y0 = Math.max(0, Math.floor((vy0 - DETAIL_MARGIN) / DETAIL_SNAP) * DETAIL_SNAP);
  const x1 = Math.min(cw, Math.ceil((vx1 + DETAIL_MARGIN) / DETAIL_SNAP) * DETAIL_SNAP);
  const y1 = Math.min(ch, Math.ceil((vy1 + DETAIL_MARGIN) / DETAIL_SNAP) * DETAIL_SNAP);
  const key = `${zoom}|${rot}|${state.contentGen}|${x0},${y0},${x1},${y1}`;
  if (slot.detailKey === key) return; // already showing / requested
  slot.detailKey = key;
  const doc = state.currentDoc;
  const epoch = state.renderEpoch;
  try {
    const res = await invoke<Rendered>("render_region", {
      document: doc, page: idx, zoom, rotation: rot,
      x: x0, y: y0, width: x1 - x0, height: y1 - y0,
    });
    if (doc !== state.currentDoc || epoch !== state.renderEpoch || slot.detailKey !== key) return;
    let img = slot.detail;
    if (!img) {
      img = document.createElement("img");
      img.className = "detail";
      img.alt = "";
      img.draggable = false;
      slot.img.after(img);
      slot.detail = img;
    }
    img.src = res.data_url;
    img.dataset.zoom = String(zoom);
    img.dataset.rot = String(rot);
    img.dataset.gen = String(state.contentGen);
    img.dataset.x0 = String(x0);
    img.dataset.y0 = String(y0);
    img.dataset.x1 = String(x1);
    img.dataset.y1 = String(y1);
    placeDetail(img, zoom);
  } catch {
    if (slot.detailKey === key) slot.detailKey = null;
  }
}

async function ensureTextLayer(idx: number) {
  if (!state.currentDoc) return;
  const slot = state.slots[idx];
  if (!slot) return;
  let runs = state.textRunsCache.get(idx);
  if (!runs) {
    const doc = state.currentDoc;
    const gen = state.contentGen;
    try {
      runs = await invoke<TextRun[]>("page_text_layout_cmd", { document: doc, page: idx });
    } catch {
      runs = [];
    }
    if (doc !== state.currentDoc || gen !== state.contentGen) return;
    state.textRunsCache.set(idx, runs);
  }
  const sz = state.pageSizes[idx];
  if (!sz) return;
  const rot = effectiveRotation(idx);
  // The layer is laid out once per content/rotation at a fixed size and only
  // rescaled (one CSS transform) on zoom — rebuilding thousands of spans per
  // zoom step is what made text-heavy plans crawl.
  const key = `${state.contentGen}|${rot}`;
  if (slot.textKey === key) {
    scaleTextLayer(slot);
    return;
  }
  const rs = rotatedSize(idx);
  const layerW = rs.w * TEXT_LAYER_SCALE;
  const layerH = rs.h * TEXT_LAYER_SCALE;
  const layer = slot.selectLayer;
  layer.innerHTML = "";
  layer.style.transform = "";
  layer.style.width = `${layerW}px`;
  layer.style.height = `${layerH}px`;
  // First pass: place each run's text at its box, font-size = box height,
  // but DON'T constrain the width — let the glyphs lay out at their natural
  // size so we can measure it.
  const pending: { span: HTMLSpanElement; targetW: number }[] = [];
  for (const run of runs) {
    const box = rotateRectToSlot(run.rect_pt, rot, sz, layerW, layerH);
    if (box.width <= 0 || box.height <= 0) continue;
    const span = document.createElement("span");
    span.textContent = run.text;
    span.style.left = `${box.left}px`;
    span.style.top = `${box.top}px`;
    span.style.fontSize = `${box.height}px`;
    span.style.height = `${box.height}px`;
    layer.appendChild(span);
    pending.push({ span, targetW: box.width });
  }
  // Second pass (single reflow): scale each run's glyphs horizontally to fill
  // exactly its box width. Without this the natural text width overflows the
  // box and the selection/highlight rectangles of tightly spaced words
  // overlap. transform-origin is 0 0 (CSS) so the left edge stays put.
  // Read every width first, then write every transform: interleaving the two
  // dirties style after each write and forces a fresh layout per span.
  // (The layer is untransformed here, so the rects are in layout pixels.)
  const naturals = pending.map(({ span }) => span.getBoundingClientRect().width);
  for (let i = 0; i < pending.length; i++) {
    const natural = naturals[i];
    if (natural > 0) pending[i].span.style.transform = `scaleX(${pending[i].targetW / natural})`;
  }
  slot.textKey = key;
  slot.textW = layerW;
  slot.textH = layerH;
  scaleTextLayer(slot);
}

/** Fit the fixed-size text layer to the slot's current (zoomed) size. */
function scaleTextLayer(slot: Slot) {
  if (!slot.textKey || slot.textW <= 0 || slot.textH <= 0) return;
  const sx = slot.img.width / slot.textW;
  const sy = slot.img.height / slot.textH;
  slot.selectLayer.style.transform = `scale(${sx}, ${sy})`;
}

export function rotateRectToSlot(
  r: RectPt,
  rot: number,
  sz: PageSize,
  imgW: number,
  imgH: number,
): { left: number; top: number; width: number; height: number } {
  const w0 = sz.width_pt;
  const h0 = sz.height_pt;
  let nx0 = r.x0, ny0 = r.y0, nx1 = r.x1, ny1 = r.y1;
  let dw = w0, dh = h0;
  switch (rot & 3) {
    case 0:
      break;
    case 1:
      nx0 = h0 - r.y1; ny0 = r.x0; nx1 = h0 - r.y0; ny1 = r.x1;
      dw = h0; dh = w0;
      break;
    case 2:
      nx0 = w0 - r.x1; ny0 = h0 - r.y1; nx1 = w0 - r.x0; ny1 = h0 - r.y0;
      break;
    case 3:
      nx0 = r.y0; ny0 = w0 - r.x1; nx1 = r.y1; ny1 = w0 - r.x0;
      dw = h0; dh = w0;
      break;
  }
  const sx = imgW / dw;
  const sy = imgH / dh;
  return {
    left: nx0 * sx,
    top: ny0 * sy,
    width: (nx1 - nx0) * sx,
    height: (ny1 - ny0) * sy,
  };
}

async function ensureLinkLayer(idx: number) {
  if (!state.currentDoc) return;
  const slot = state.slots[idx];
  if (!slot) return;
  slot.linkLayer.innerHTML = "";
  let links = state.linksCache.get(idx);
  if (!links) {
    const doc = state.currentDoc;
    const gen = state.contentGen;
    try {
      links = await invoke<PageLink[]>("page_links_cmd", { document: doc, page: idx });
    } catch {
      links = [];
    }
    if (doc !== state.currentDoc || gen !== state.contentGen) return;
    state.linksCache.set(idx, links);
  }
  const sz = state.pageSizes[idx];
  if (!sz || links.length === 0) return;
  const rot = effectiveRotation(idx);
  for (const link of links) {
    const box = rotateRectToSlot(link.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) continue;
    const a = document.createElement("a");
    a.style.left = `${box.left}px`;
    a.style.top = `${box.top}px`;
    a.style.width = `${box.width}px`;
    a.style.height = `${box.height}px`;
    if (link.target.kind === "Page") {
      const targetPage = link.target.value;
      a.title = t("page.one", { n: targetPage + 1 });
      a.href = `#page-${targetPage + 1}`;
      a.addEventListener("click", (ev) => {
        ev.preventDefault();
        navigateTo(targetPage, { recordHistory: true });
      });
    } else {
      const url = link.target.value;
      a.title = url;
      a.href = url;
      a.addEventListener("click", async (ev) => {
        ev.preventDefault();
        try {
          await invoke("open_external", { url });
        } catch (err) {
          setStatus(t("st.error", { e: String(err) }));
        }
      });
    }
    slot.linkLayer.appendChild(a);
  }
}

export function renderHighlightsFor(idx: number) {
  const slot = state.slots[idx];
  if (!slot) return;
  const sz = state.pageSizes[idx];
  if (!sz) return;
  slot.highlightLayer.innerHTML = "";
  const rot = effectiveRotation(idx);
  state.hits.forEach((hit, hIdx) => {
    if (hit.page !== idx) return;
    for (const r of hit.rects_pt) {
      const box = rotateRectToSlot(r, rot, sz, slot.img.width, slot.img.height);
      if (box.width <= 0 || box.height <= 0) continue;
      const el = document.createElement("div");
      el.className = "hit" + (hIdx === state.currentHit ? " current" : "");
      el.style.left = `${box.left}px`;
      el.style.top = `${box.top}px`;
      el.style.width = `${box.width}px`;
      el.style.height = `${box.height}px`;
      slot.highlightLayer.appendChild(el);
    }
  });
}

export function renderAllHighlights() {
  for (let i = 0; i < state.slots.length; i++) renderHighlightsFor(i);
}

export function invalidateRenders() {
  for (const [i, s] of state.slots.entries()) {
    s.loaded = false;
    s.lastZoom = null;
    // Keep the stale bitmap as a scaled preview while the re-render is
    // pending — dropping it would flash the hatched placeholder on every
    // zoom/rotate. The placeholder only shows for never-rendered slots.
    if (!s.img.getAttribute("src")) s.placeholder.style.display = "";
    s.linkLayer.innerHTML = "";
    // Same for the sharp detail window on a pure zoom change (sizeSlots
    // rescales it); new content or a rotation makes it wrong, not stale.
    const d = s.detail;
    if (d && (d.dataset.gen !== String(state.contentGen) || d.dataset.rot !== String(effectiveRotation(i)))) {
      dropDetail(s);
    }
    s.detailKey = null;
  }
  state.renderQueue.clear();
  // Supersede any in-flight renders: their results paint stale bitmaps.
  state.renderEpoch++;
}

export function reflow() {
  if (!state.currentDoc) return;
  invalidateRenders();
  sizeSlots();
  recenterScrollX();
  if (state.viewMode === "single") {
    applyViewMode();
    scheduleRender(state.currentPage);
    const slot = state.slots[state.currentPage];
    if (slot) slot.wrap.scrollIntoView({ block: "start", inline: "center" });
  } else {
    for (const [i, slot] of state.slots.entries()) {
      const rect = slot.wrap.getBoundingClientRect();
      const vRect = dom.viewer.getBoundingClientRect();
      const visible = rect.bottom > vRect.top - 400 && rect.top < vRect.bottom + 400;
      if (visible) scheduleRender(i);
    }
  }
  updateChrome();
}

export function updateCurrentFromScroll() {
  if (state.slots.length === 0) return;
  const vRect = dom.viewer.getBoundingClientRect();
  const midY = vRect.top + vRect.height / 2;
  let best = state.currentPage;
  let bestDist = Infinity;
  for (let i = 0; i < state.slots.length; i++) {
    const r = state.slots[i].wrap.getBoundingClientRect();
    const c = (r.top + r.bottom) / 2;
    const d = Math.abs(c - midY);
    if (d < bestDist) {
      bestDist = d;
      best = i;
    }
  }
  if (best !== state.currentPage) {
    state.currentPage = best;
    updateChrome();
  }
}

export function navigateTo(idx: number, _opts: { recordHistory?: boolean } = {}) {
  const clamped = Math.max(0, Math.min(state.pageCount - 1, idx));
  if (clamped === state.currentPage) {
    updateChrome();
    return;
  }
  state.currentPage = clamped;
  if (state.viewMode === "single") {
    applyViewMode();
    scheduleRender(clamped);
  } else {
    const slot = state.slots[clamped];
    if (slot) slot.wrap.scrollIntoView({ block: "start", inline: "center", behavior: "auto" });
    scheduleRender(clamped);
  }
  evictFarSlots();
  updateChrome();
}

export function goToPage(idx: number) {
  navigateTo(idx);
}

export function rotate(delta: number, docWide: boolean) {
  if (!state.currentDoc) return;
  if (docWide) {
    state.docRotation = (((state.docRotation + delta) % 4) + 4) % 4;
  } else {
    const cur = state.pageRotation.get(state.currentPage) || 0;
    state.pageRotation.set(state.currentPage, (((cur + delta) % 4) + 4) % 4);
  }
  reflow();
}

export function setDarkMode(on: boolean) {
  state.darkMode = on;
  document.body.classList.toggle("dark", state.darkMode);
  document.getElementById("darkToggle")?.classList.toggle("active", on);
}

function findSlotAtClient(clientX: number, clientY: number): { slot: Slot; idx: number } | null {
  let best: { slot: Slot; idx: number; dist: number } | null = null;
  for (let i = 0; i < state.slots.length; i++) {
    const s = state.slots[i];
    if (!s) continue;
    const r = s.wrap.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    if (clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom) {
      return { slot: s, idx: i };
    }
    const cx = (r.left + r.right) / 2;
    const cy = (r.top + r.bottom) / 2;
    const d = Math.hypot(clientX - cx, clientY - cy);
    if (!best || d < best.dist) best = { slot: s, idx: i, dist: d };
  }
  return best ? { slot: best.slot, idx: best.idx } : null;
}

export function applyZoomAround(newZoom: number, clientX: number, clientY: number) {
  if (!state.currentDoc) return;
  const clamped = clampZoom(newZoom);
  if (clamped === state.zoom) return;
  const anchor = findSlotAtClient(clientX, clientY);
  let fx = 0.5, fy = 0.5;
  let anchorSlot: Slot | null = null;
  if (anchor) {
    const r = anchor.slot.wrap.getBoundingClientRect();
    fx = (clientX - r.left) / r.width;
    fy = (clientY - r.top) / r.height;
    anchorSlot = anchor.slot;
  }
  state.fitMode = "custom";
  state.zoom = clamped;
  holdRendersForZoom();
  invalidateRenders();
  sizeSlots();
  if (state.viewMode === "single") {
    applyViewMode();
    scheduleRender(state.currentPage);
  } else {
    for (const [i, slot] of state.slots.entries()) {
      const rect = slot.wrap.getBoundingClientRect();
      const vRect = dom.viewer.getBoundingClientRect();
      const visible = rect.bottom > vRect.top - 400 && rect.top < vRect.bottom + 400;
      if (visible) scheduleRender(i);
    }
  }
  if (anchorSlot) {
    const nr = anchorSlot.wrap.getBoundingClientRect();
    const wantedLeft = clientX - nr.width * fx;
    const wantedTop = clientY - nr.height * fy;
    dom.viewer.scrollLeft += nr.left - wantedLeft;
    dom.viewer.scrollTop += nr.top - wantedTop;
  }
  updateChrome();
}

export function setWheelZoomMode(on: boolean) {
  state.wheelZoomMode = on;
  document.body.classList.toggle("wheelZoomMode", on);
  if (!on) document.body.classList.remove("panning");
  const btn = document.getElementById("wheelZoomBtn");
  if (btn) btn.classList.toggle("active", on);
  if (state.currentDoc) {
    sizeSlots();
    const slot = state.slots[state.currentPage];
    if (slot) {
      slot.wrap.scrollIntoView({ block: "start", inline: "center" });
    } else if (on) {
      recenterScrollX();
    }
  }
}

export function toggleSidebar() {
  const becomingVisible = dom.sidebarEl.classList.contains("hidden");
  dom.sidebarEl.classList.toggle("hidden");
  if (becomingVisible) {
    const activeTab = document.querySelector(".tab.active") as HTMLElement | null;
    const tab = activeTab?.dataset.tab;
    if (tab === "thumbs") {
      // armThumbObserver is in doc.ts (thumbs section). Use callback if needed; here defer to import.
      // Inline: dispatch a custom event that doc.ts listens for.
      document.dispatchEvent(new CustomEvent("rustpdf:arm-thumb-observer"));
    }
  }
}

export function initViewer() {
  // Page nav (status bar)
  document.getElementById("prevPage")!.addEventListener("click", () => goToPage(state.currentPage - 1));
  document.getElementById("nextPage")!.addEventListener("click", () => goToPage(state.currentPage + 1));
  const commitPageInput = () => {
    const v = parseInt(dom.pageInput.value, 10);
    if (!isNaN(v)) goToPage(v - 1);
    else updateChrome();
  };
  dom.pageInput.addEventListener("change", commitPageInput);
  dom.pageInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitPageInput();
      dom.pageInput.blur();
    } else if (e.key === "Escape") {
      updateChrome();
      dom.pageInput.blur();
    }
  });

  // Zoom (status bar)
  document.getElementById("zoomIn")!.addEventListener("click", () => stepZoom(1));
  document.getElementById("zoomOut")!.addEventListener("click", () => stepZoom(-1));
  document.getElementById("fitWidthBtn")!.addEventListener("click", () => setFitMode("width"));
  document.getElementById("fitPageBtn")!.addEventListener("click", () => setFitMode("page"));
  dom.zoomSelect.addEventListener("change", () => {
    const v = dom.zoomSelect.value;
    if (v === "custom") { syncZoomUi(); return; }
    if (v === "width" || v === "page") { setFitMode(v); return; }
    const z = parseFloat(v);
    if (!isFinite(z)) return;
    state.fitMode = "custom";
    state.zoom = clampZoom(z);
    reflow();
  });

  // View mode
  dom.viewModeSelect.addEventListener("change", () => {
    state.viewMode = dom.viewModeSelect.value as ViewMode;
    applyViewMode();
    reflow();
    if (state.viewMode === "single") navigateTo(state.currentPage, { recordHistory: false });
  });
  dom.viewModeSelect.value = state.viewMode;

  // Rotate / dark
  document.getElementById("rotLeft")!.addEventListener("click", (e) => rotate(-1, (e as MouseEvent).shiftKey));
  document.getElementById("rotRight")!.addEventListener("click", (e) => rotate(1, (e as MouseEvent).shiftKey));
  document.getElementById("darkToggle")!.addEventListener("click", () => setDarkMode(!state.darkMode));

  // Sidebar
  document.getElementById("sidebarBtn")!.addEventListener("click", toggleSidebar);

  // Wheel-zoom mode toggle
  document.getElementById("wheelZoomBtn")!.addEventListener("click", () => {
    setWheelZoomMode(!state.wheelZoomMode);
  });

  // Pan via left-drag in wheel-zoom mode
  dom.viewer.addEventListener("pointerdown", (e: PointerEvent) => {
    if (!state.wheelZoomMode) return;
    if (e.button !== 0) return;
    e.preventDefault();
    state.panState = {
      pointerId: e.pointerId,
      startClientX: e.clientX,
      startClientY: e.clientY,
      startScrollLeft: dom.viewer.scrollLeft,
      startScrollTop: dom.viewer.scrollTop,
    };
    dom.viewer.setPointerCapture(e.pointerId);
    document.body.classList.add("panning");
  });
  dom.viewer.addEventListener("pointermove", (e: PointerEvent) => {
    if (!state.panState || e.pointerId !== state.panState.pointerId) return;
    dom.viewer.scrollLeft = state.panState.startScrollLeft - (e.clientX - state.panState.startClientX);
    dom.viewer.scrollTop = state.panState.startScrollTop - (e.clientY - state.panState.startClientY);
  });
  const endPan = (e: PointerEvent) => {
    if (!state.panState || e.pointerId !== state.panState.pointerId) return;
    try { dom.viewer.releasePointerCapture(state.panState.pointerId); } catch { /* ignore */ }
    state.panState = null;
    document.body.classList.remove("panning");
  };
  dom.viewer.addEventListener("pointerup", endPan);
  dom.viewer.addEventListener("pointercancel", endPan);

  // Wheel zoom / single-page-mode wheel turn
  dom.viewer.addEventListener(
    "wheel",
    (e: WheelEvent) => {
      if (!state.currentDoc) return;
      if (e.ctrlKey || state.wheelZoomMode) {
        e.preventDefault();
        // A factor per notch feels the same at every zoom level (a fixed
        // +10 % crawled at 300 %). Scaling by the wheel distance keeps
        // precision touchpads and pinch (ctrl+wheel, small deltas) smooth.
        let dy = e.deltaY;
        if (e.deltaMode === 1) dy *= 40;
        else if (e.deltaMode === 2) dy *= 400;
        dy = Math.max(-300, Math.min(300, dy));
        const perNotch = e.ctrlKey ? 1.2 : 1.25;
        applyZoomAround(state.zoom * Math.pow(perNotch, -dy / 100), e.clientX, e.clientY);
        return;
      }
      if (state.viewMode !== "single") return;
      const cs = getComputedStyle(dom.pageList);
      const padTop = parseFloat(cs.paddingTop) || 0;
      const padBottom = parseFloat(cs.paddingBottom) || 0;
      const atTop = dom.viewer.scrollTop <= padTop + 1;
      const atBottom =
        dom.viewer.scrollTop + dom.viewer.clientHeight >= dom.viewer.scrollHeight - padBottom - 1;
      if (e.deltaY < 0 && atTop && state.currentPage > 0) {
        e.preventDefault();
        goToPage(state.currentPage - 1);
        dom.viewer.scrollTop = dom.viewer.scrollHeight;
      } else if (e.deltaY > 0 && atBottom && state.currentPage < state.pageCount - 1) {
        e.preventDefault();
        goToPage(state.currentPage + 1);
        dom.viewer.scrollTop = 0;
      }
    },
    { passive: false },
  );

  // Scroll updates current page
  dom.viewer.addEventListener("scroll", () => {
    if (state.viewMode !== "single") updateCurrentFromScroll();
    scheduleDetailUpdate();
  });

  // Resize re-flows fit modes
  window.addEventListener("resize", () => {
    if (state.fitMode === "width" || state.fitMode === "page") reflow();
  });
}
