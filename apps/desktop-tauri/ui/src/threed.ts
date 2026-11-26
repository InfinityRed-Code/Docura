// 3D annotations on the page: a per-page layer with a "3D" badge over each
// 3D annotation, a rendered preview where the file has no poster, and the
// live 3D view once the user activates one (or the file asks for activation
// when the page is shown). three.js (threed-view.ts) loads on first use.

import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { state, callbacks } from "./state";
import { setStatus } from "./dom";
import { t } from "./i18n";
import { refreshIcons } from "./icons";
import { effectiveRotation, rotateRectToSlot } from "./viewer";
import type { ThreeDAnnot } from "./types";
import type { Model3D, Viewer3D } from "./threed-view";

type ViewModule = typeof import("./threed-view");
let viewModule: Promise<ViewModule> | null = null;
function loadViewModule(): Promise<ViewModule> {
  viewModule ??= import("./threed-view");
  return viewModule;
}

// ---------- Annotation list (per document content generation) ----------

let listKey = "";
let listPromise: Promise<ThreeDAnnot[]> = Promise.resolve([]);

function currentKey() {
  return `${state.currentDoc}|${state.contentGen}`;
}

function annotations(): Promise<ThreeDAnnot[]> {
  const key = currentKey();
  if (key !== listKey) {
    listKey = key;
    const doc = state.currentDoc;
    listPromise = doc
      ? invoke<ThreeDAnnot[]>("list_3d", { document: doc }).catch(() => [] as ThreeDAnnot[])
      : Promise.resolve([]);
  }
  return listPromise;
}

const supported = (a: ThreeDAnnot) => a.format === "U3D";

// ---------- Models and previews ----------

/** The last decoded scene: the preview and a following activation of the
 * same annotation share one backend decode. */
let lastScene: { key: string; buf: Promise<ArrayBuffer> } | null = null;

async function loadModel(a: ThreeDAnnot): Promise<Model3D> {
  const key = `${currentKey()}|${a.id}`;
  if (lastScene?.key !== key) {
    const buf = invoke<ArrayBuffer>("load_3d_scene", { document: state.currentDoc, page: a.page, id: a.id });
    lastScene = { key, buf };
    buf.catch(() => {
      if (lastScene?.key === key) lastScene = null;
    });
  }
  const [mod, buf] = await Promise.all([loadViewModule(), lastScene.buf]);
  return mod.unpackScene(buf);
}

/** Rendered previews for annotations without a poster, by key|id. */
const previews = new Map<string, string>();
const previewPending = new Set<string>();
let previewChain: Promise<void> = Promise.resolve();

function boxSizePt(a: ThreeDAnnot) {
  return { w: Math.max(1, a.rect_pt.x1 - a.rect_pt.x0), h: Math.max(1, a.rect_pt.y1 - a.rect_pt.y0) };
}

function requestPreview(a: ThreeDAnnot) {
  const key = `${currentKey()}|${a.id}`;
  if (previews.has(key) || previewPending.has(key)) return;
  previewPending.add(key);
  // One at a time: models can be large.
  previewChain = previewChain.then(async () => {
    if (!key.startsWith(currentKey())) return;
    try {
      const model = await loadModel(a);
      const mod = await loadViewModule();
      await model.ready; // textures, or the preview shows bare materials
      const { w, h } = boxSizePt(a);
      const s = Math.min(2, 1600 / Math.max(w, h));
      const view = a.default_view !== null ? a.views[a.default_view] ?? null : null;
      previews.set(key, mod.renderPoster(model, view, Math.round(w * s), Math.round(h * s), w, h));
      model.dispose();
    } catch {
      previews.set(key, "");
    } finally {
      previewPending.delete(key);
    }
    if (key.startsWith(currentKey())) callbacks.render3dFor(a.page);
  });
}

// ---------- The active 3D view ----------

type Active = { annot: ThreeDAnnot; key: string; viewer: Viewer3D; model: Model3D; box: HTMLDivElement };
let active: Active | null = null;
let activating: string | null = null;

function deactivate() {
  if (!active) return;
  const a = active;
  active = null;
  a.viewer.dispose();
  a.model.dispose();
  a.box.remove();
  callbacks.render3dFor(a.annot.page);
}

async function activate(a: ThreeDAnnot) {
  if (!supported(a)) return;
  const key = `${currentKey()}|${a.id}`;
  if (active?.key === key || activating === key) return;
  deactivate();
  activating = key;
  setStatus(t("3d.loading"));
  try {
    const model = await loadModel(a);
    const mod = await loadViewModule();
    if (activating !== key || !key.startsWith(currentKey())) {
      model.dispose();
      return;
    }
    const { w, h } = boxSizePt(a);
    const viewer = new mod.Viewer3D({
      annot: a,
      model,
      wPt: w,
      hPt: h,
      onClose: () => deactivate(),
      onExport: () => void exportModel(a),
    });
    const box = document.createElement("div");
    box.className = "threeDBox active";
    box.appendChild(viewer.el);
    active = { annot: a, key, viewer, model, box };
    callbacks.render3dFor(a.page);
    viewer.applyView(true);
    refreshIcons();
    const n = model.header.triangles;
    setStatus(t("3d.loaded", { n: n.toLocaleString() }));
  } catch (e) {
    setStatus(t("3d.loadError", { e: String(e) }));
  } finally {
    if (activating === key) activating = null;
  }
}

async function exportModel(a: ThreeDAnnot) {
  const base = state.currentDocPath
    ? state.currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "model";
  const ext = a.format === "PRC" ? "prc" : a.format === "U3D" ? "u3d" : "bin";
  const path = await save({
    title: t("3d.export"),
    defaultPath: `${base}_3D.${ext}`,
    filters: [{ name: a.format, extensions: [ext] }],
  });
  if (!path || typeof path !== "string") return;
  try {
    await invoke("export_3d_model", { document: state.currentDoc, page: a.page, id: a.id, path });
    setStatus(t("st.saved", { path }));
  } catch (e) {
    setStatus(t("st.saveError", { e: String(e) }));
  }
}

// ---------- Page layer ----------

function hint(a: ThreeDAnnot, box: HTMLDivElement) {
  box.querySelector(".threeDHint")?.remove();
  const pop = document.createElement("div");
  pop.className = "threeDHint";
  const msg = document.createElement("p");
  msg.textContent = t("3d.unsupported", { format: a.format });
  const btn = document.createElement("button");
  btn.type = "button";
  btn.textContent = t("3d.export");
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    void exportModel(a);
  });
  pop.append(msg, btn);
  box.appendChild(pop);
}

async function render3dFor(idx: number) {
  const slot = state.slots[idx];
  if (!slot) return;
  const list = (await annotations()).filter((a) => a.page === idx);
  const layer = slot.threeDLayer;
  if (active && !active.key.startsWith(currentKey())) deactivate();
  // Keep the live view element; rebuild the rest.
  for (const el of Array.from(layer.children)) {
    if (el !== active?.box) el.remove();
  }
  const sz = state.pageSizes[idx];
  if (!sz || list.length === 0) return;
  const rot = effectiveRotation(idx);
  for (const a of list) {
    const r = rotateRectToSlot(a.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (r.width < 4 || r.height < 4) continue;
    const place = (el: HTMLElement) => {
      el.style.left = `${r.left}px`;
      el.style.top = `${r.top}px`;
      el.style.width = `${r.width}px`;
      el.style.height = `${r.height}px`;
    };
    if (active && active.annot.id === a.id && active.annot.page === idx) {
      place(active.box);
      if (active.box.parentElement !== layer) layer.appendChild(active.box);
      if (!active.viewer.isFullscreen) active.viewer.resize(r.width, r.height);
      continue;
    }
    const box = document.createElement("div");
    box.className = "threeDBox";
    place(box);
    if (!a.has_poster && supported(a)) {
      const key = `${currentKey()}|${a.id}`;
      const url = previews.get(key);
      if (url) {
        const img = document.createElement("img");
        img.className = "threeDPreview";
        img.src = url;
        img.alt = "";
        box.appendChild(img);
      } else if (url === undefined) {
        requestPreview(a);
      }
    }
    const badge = document.createElement("button");
    badge.type = "button";
    badge.className = "threeDBadge";
    badge.innerHTML = `<i data-lucide="rotate-3d"></i><span>${supported(a) ? t("3d.badge") : `3D · ${a.format}`}</span>`;
    badge.title = supported(a) ? t("3d.activate") : t("3d.unsupported", { format: a.format });
    box.appendChild(badge);
    box.title = a.contents ?? "";
    const go = (e: Event) => {
      e.stopPropagation();
      if (supported(a)) void activate(a);
      else hint(a, box);
    };
    badge.addEventListener("click", go);
    // A click on the model activates it, unless a page tool is in use
    // (then the click belongs to the tool).
    box.addEventListener("click", (e) => {
      if (state.shapeTool || state.annotTool || state.eraseMode || state.signTool) return;
      go(e);
    });
    layer.appendChild(box);
    // Activation on page open / page visible (ISO 32000 /3DA /A).
    if ((a.activation === "PO" || a.activation === "PV") && supported(a) && !active && !activating && idx === state.currentPage) {
      void activate(a);
    }
  }
  refreshIcons();
}

/** Called by the inserter after a model was added. */
export function forget3dCaches() {
  listKey = "";
}

export function initThreeD() {
  callbacks.render3dFor = (idx) => void render3dFor(idx);
  callbacks.evict3dFor = (idx) => {
    if (active && active.annot.page === idx && !active.viewer.isFullscreen) deactivate();
  };
  callbacks.pick3dModel = async (path?: string) => (await import("./threed-import")).pickModel(path);
  callbacks.insert3dAt = async (page, rect) => (await import("./threed-import")).insertAt(page, rect);
}
