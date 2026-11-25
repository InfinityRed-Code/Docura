// The viewer's picture against Adobe Acrobat Reader's, pixel by pixel.
//
// tests/fixtures/acrobat/*.png are the 3D annotation area (401 x 250 px) of
// window captures of Acrobat Reader 26.003.22110 showing files written by
// `cargo run -p pdf_mutate --example make_3d_samples` (one copy per default
// view, activated on page open). The viewer renders the same model in the
// same view at the same size; shading is compared on interior pixels.
// Run with `pnpm test:browser`; skipped when no Chromium is installed.

import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { compareImages, decodePng, findBrowser, packScene, packU3d, startViewerPage } from "./harness.mjs";

const skip = findBrowser() ? false : "no Chromium (Edge or Chrome) found; set DOCURA_TEST_BROWSER";
let page;
before(async () => { if (!skip) page = await startViewerPage(); });
after(async () => { await page?.close(); });

const W = 401, H = 250;
const reference = (name) => decodePng(fs.readFileSync(new URL(`../fixtures/acrobat/${name}.png`, import.meta.url)));
// Acrobat draws a small axis gizmo in the lower left corner.
const gizmo = (x, y) => x < 45 && y > H - 45;

async function render(scene, standardView) {
  const opts = standardView ? `{ w: ${W}, h: ${H}, standardView: ${JSON.stringify(standardView)} }` : `{ w: ${W}, h: ${H} }`;
  const data = await page.evaluate(`return await viewerTest.poster(${JSON.stringify(scene)}, ${opts})`);
  return { width: W, height: H, data: Uint8Array.from(data) };
}

/** The sphere of make_3d_samples (shading_*.pdf): radius 100, smooth
 * normals, specular 0.25 as the U3D writer stores it. */
function sphere(diffuse) {
  const rings = 48, segments = 96, radius = 100;
  const positions = [], normals = [], indices = [];
  for (let j = 0; j <= rings; j++) {
    const theta = (Math.PI * j) / rings;
    for (let i = 0; i <= segments; i++) {
      const phi = (2 * Math.PI * i) / segments;
      const n = [Math.sin(theta) * Math.cos(phi), Math.sin(theta) * Math.sin(phi), Math.cos(theta)];
      normals.push(...n);
      positions.push(n[0] * radius, n[1] * radius, n[2] * radius);
    }
  }
  const row = segments + 1;
  for (let j = 0; j < rings; j++) {
    for (let i = 0; i < segments; i++) {
      const a = j * row + i, b = a + 1, c = a + row, d = a + row + 1;
      if (j > 0) indices.push(a, c, b);
      if (j + 1 < rings) indices.push(b, c, d);
    }
  }
  return packScene({
    geometries: [{ positions, normals, indices }],
    instances: [{ node: "Kugel", path: ["Kugel"], matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], geometry: 0, material: 0 }],
    materials: [{ diffuse: [diffuse, diffuse, diffuse, 1], ambient: [0, 0, 0], specular: [0.25, 0.25, 0.25], emissive: [0, 0, 0], texture: null, vertex_colors: false, lighting: true }],
    tree: [{ name: "Kugel", kind: "model", parents: [""] }],
    bounds: [[-radius, -radius, -radius], [radius, radius, radius]],
  });
}

const orientation = () => packU3d("apps/desktop-tauri/ui/tests/fixtures/models/orientation.u3d");

// Shading under the "CAD" scheme. Limits: mean and 95th percentile of the
// color difference, as fractions of the display range.
const shading = [
  ["orientation.pdf, isometric (oblique flat faces)", orientation, "iso", "orientation-iso"],
  ["orientation.pdf, front", orientation, "front", "orientation-front"],
  ["orientation.pdf, top", orientation, "top", "orientation-top"],
  ["orientation.pdf, bottom", orientation, "bottom", "orientation-bottom"],
  ["grey sphere, isometric (diffuse term in every direction)", () => sphere(0.5), "iso", "sphere-gray-iso"],
  ["black sphere, isometric (highlight alone)", () => sphere(0), "iso", "sphere-black-iso"],
];
for (const [name, scene, view, ref] of shading) {
  test(`shading matches Acrobat Reader: ${name}`, { skip }, async () => {
    const r = compareImages(reference(ref), await render(scene(), view), gizmo);
    assert.ok(r.pixels > 500, `only ${r.pixels} pixels compared`);
    assert.ok(r.overlap > 0.85, `silhouettes overlap ${r.overlap.toFixed(3)}`);
    assert.ok(r.mean < 0.02, `mean color difference ${r.mean.toFixed(3)}`);
    assert.ok(r.p95 < 0.04, `95th percentile of the color difference ${r.p95.toFixed(3)}`);
  });
}

// Without any view in the PDF: Acrobat's default camera and background.
const noViews = [
  ["model without a view node (orientation_noviews.pdf)", orientation, "orientation-noviews"],
  ["Intel box scene (has a view node, which Acrobat ignores)", () => packU3d("crates/pdf_3d/tests/fixtures/intel/box.u3d"), "intel-box-noviews"],
  ["Intel surface scene (rotated instance)", () => packU3d("crates/pdf_3d/tests/fixtures/intel/surface.u3d"), "intel-surface-noviews"],
];
for (const [name, scene, ref] of noViews) {
  test(`default camera and background match Acrobat Reader: ${name}`, { skip }, async () => {
    const r = compareImages(reference(ref), await render(scene(), null), gizmo);
    assert.deepEqual(r.backgroundB, r.backgroundA, "background color");
    assert.deepEqual(r.backgroundA, [51, 51, 51]);
    for (let k = 0; k < 4; k++) assert.ok(Math.abs(r.bboxA[k] - r.bboxB[k]) <= 3, `silhouette box ${r.bboxB} against Acrobat ${r.bboxA}`);
    assert.ok(r.overlap > 0.94, `silhouettes overlap ${r.overlap.toFixed(3)}`);
  });
}
