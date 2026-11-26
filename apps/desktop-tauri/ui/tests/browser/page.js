// Page side of the browser tests: puts the real viewer (src/threed-view.ts)
// on a blank page. Loaded by harness.html through the Vite dev server.

import * as view from "/src/threed-view.ts";
import * as camera from "/src/threed-camera.ts";

const stage = document.getElementById("stage");

function bytes(base64) {
  const bin = atob(base64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out.buffer;
}

/** A 3D annotation as `list_3d` reports it. */
function annot(views, defaultView, w, h) {
  return {
    id: "test", page: 0, rect_pt: { x0: 0, y0: 0, x1: w, y1: h }, subtype: "3D", format: "U3D",
    model_size: 0, has_poster: false, activation: "XA", toolbar: true, model_tree: false,
    views, default_view: views.length ? defaultView : null, contents: null,
  };
}

let current = null;

window.viewerTest = {
  view,
  camera,
  /** Shows a packed scene in the live viewer, the way threed.ts activates
   * a 3D annotation. Nothing here waits for textures or sends input. */
  show(base64, { w = 480, h = 360, views = [], defaultView = 0, hideToolbar = true } = {}) {
    this.close();
    const model = view.unpackScene(bytes(base64));
    const viewer = new view.Viewer3D({ annot: annot(views, defaultView, w, h), model, wPt: w, hPt: h, onClose() {}, onExport() {} });
    const box = document.createElement("div");
    box.style.cssText = `position:absolute;left:0;top:0;width:${w}px;height:${h}px`;
    box.appendChild(viewer.el);
    stage.appendChild(box);
    viewer.resize(w, h);
    viewer.applyView(true);
    if (hideToolbar) for (const el of viewer.el.querySelectorAll(".threeDBar")) el.style.display = "none";
    current = { viewer, model, box };
    return { warnings: model.header.warnings, materials: model.header.materials.length, textures: model.header.textures.length };
  },
  /** Resolves once the model's textures are decoded and two frames have
   * passed (the test's clock; the viewer has to redraw by itself). */
  async ready() {
    await current.model.ready;
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  },
  close() {
    if (!current) return;
    current.viewer.dispose();
    current.model.dispose();
    current.box.remove();
    current = null;
  },
  /** Renders a packed scene with renderPoster (`w` x `h` pixels, also the
   * box size in points) and returns RGBA pixels: the whole image, or a
   * `grid` x `grid` raster (row-major, y down). */
  async poster(base64, { w = 480, h = 360, view: v = null, standardView = null, grid = 0 } = {}) {
    const model = view.unpackScene(bytes(base64));
    await model.ready;
    // One of the inserter's preset views ("iso", "front", ...), as written
    // by the app and by make_3d_samples.
    if (standardView) v = camera.standardViews(model.bounds, {}, w, h).find((x) => x.internal_name === standardView);
    const url = view.renderPoster(model, v, w, h, w, h);
    model.dispose();
    const img = new Image();
    img.src = url;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const g = c.getContext("2d", { willReadFrequently: true });
    g.drawImage(img, 0, 0);
    if (!grid) return Array.from(g.getImageData(0, 0, w, h).data);
    const out = [];
    for (let j = 0; j < grid; j++) {
      for (let i = 0; i < grid; i++) {
        const d = g.getImageData(Math.floor(((i + 0.5) * w) / grid), Math.floor(((j + 0.5) * h) / grid), 1, 1).data;
        out.push(d[0], d[1], d[2], d[3]);
      }
    }
    return out;
  },
};
window.viewerTestReady = true;
