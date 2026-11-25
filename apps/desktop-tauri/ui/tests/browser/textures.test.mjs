// Texture layers of U3D lit texture shaders (ECMA-363 9.8.3) in the real
// viewer: every layer is drawn, combined with the blend function the
// shader declares. Run with `pnpm test:browser`.

import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { compareImages, decodePng, encodePng, findBrowser, packScene, packU3d, startViewerPage } from "./harness.mjs";

const skip = findBrowser() ? false : "no Chromium (Edge or Chrome) found; set DOCURA_TEST_BROWSER";
let page;
before(async () => { if (!skip) page = await startViewerPage(); });
after(async () => { await page?.close(); });

const W = 401, H = 250;
const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
/** A view that names no camera (so the default one applies) and no
 * lights: the picture is what the materials and textures give. */
const unlit = {
  name: "", internal_name: null, c2w: null, u3d_path: [], center_of_orbit: null, orthographic: false, fov: null, fov_span: null,
  ortho_scale: null, ortho_binding: null, background: [0.2, 0.2, 0.2], render_mode: null, render_aux_color: null, lighting: "None",
  cross_sections: [], nodes: [],
};

async function render(scene) {
  const data = await page.evaluate(`return await viewerTest.poster(${JSON.stringify(scene)}, { w: ${W}, h: ${H}, view: ${JSON.stringify(unlit)} })`);
  return { width: W, height: H, data: Uint8Array.from(data) };
}

/** A one-colored 2 x 2 texture. */
const solid = (name, r, g, b, a = 255) => ({ name, mime: "image/png", data: encodePng(2, 2, Uint8Array.from([r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a])) });

/** A big square facing the default camera, emitting mid grey (0.5), with
 * the given texture layers; two texture coordinate sets. */
function square(layers, textures) {
  const s = 1000;
  // In the plane perpendicular to the default view direction.
  const az = (202.5 * Math.PI) / 180, el = (11.25 * Math.PI) / 180;
  const n = [Math.cos(el) * Math.cos(az), Math.cos(el) * Math.sin(az), Math.sin(el)];
  const right = [-Math.sin(az), Math.cos(az), 0];
  const up = [n[1] * right[2] - n[2] * right[1], n[2] * right[0] - n[0] * right[2], n[0] * right[1] - n[1] * right[0]];
  const corner = (a, b) => [0, 1, 2].map((k) => (right[k] * a + up[k] * b) * s);
  const positions = [...corner(-1, -1), ...corner(1, -1), ...corner(1, 1), ...corner(-1, 1)];
  const uv = [0, 0, 1, 0, 1, 1, 0, 1];
  return packScene({
    geometries: [{ positions, normals: [...n, ...n, ...n, ...n], uvs: [uv, uv], indices: [0, 1, 2, 0, 2, 3] }],
    instances: [{ node: "Quadrat", path: ["Quadrat"], matrix: IDENTITY, geometry: 0, material: 0 }],
    materials: [{ diffuse: [0, 0, 0, 1], ambient: [0, 0, 0], specular: [0, 0, 0], emissive: [0.5, 0.5, 0.5], layers, vertex_colors: false, lighting: true }],
    textures,
    tree: [{ name: "Quadrat", kind: "model", parents: [""] }],
    bounds: [positions.filter((_, i) => i % 3 === 0), positions.filter((_, i) => i % 3 === 1), positions.filter((_, i) => i % 3 === 2)].reduce((b, axis) => [[...b[0], Math.min(...axis)], [...b[1], Math.max(...axis)]], [[], []]),
  });
}

const layer = (texture, blend, more = {}) => ({ texture, uv: 0, intensity: 1, blend, blend_alpha: false, blend_constant: 0.5, transform: IDENTITY, repeat_u: true, repeat_v: true, ...more });

// Base color 0.5 (128). Texture A = (204, 102, 51), texture B = (51, 204, 255).
const A = [204, 102, 51], B = [51, 204, 255];
const blends = [
  ["multiply", [layer(0, "multiply")], A.map((v) => (128 * v) / 255)],
  ["add", [layer(0, "add")], A.map((v) => Math.min(255, 128 + v))],
  ["replace", [layer(0, "replace")], A],
  ["blend by constant 0.25", [layer(0, "blend", { blend_constant: 0.25 })], A.map((v) => 128 * 0.75 + v * 0.25)],
  ["blend by the texture's alpha", [layer(2, "blend", { blend_alpha: true })], A.map((v) => 128 * (1 - 102 / 255) + v * (102 / 255))],
  ["two layers: replace, then multiply", [layer(0, "replace"), layer(1, "multiply", { uv: 1 })], A.map((v, k) => (v * B[k]) / 255)],
  ["two layers: multiply, then add", [layer(0, "multiply"), layer(1, "add", { uv: 1 })], A.map((v, k) => Math.min(255, (128 * v) / 255 + B[k]))],
  ["three layers: replace, blend 0.5, multiply", [layer(0, "replace"), layer(1, "blend"), layer(0, "multiply")], A.map((v, k) => ((v * 0.5 + B[k] * 0.5) * v) / 255)],
];
for (const [name, layers, expected] of blends) {
  test(`texture layers: ${name}`, { skip }, async () => {
    const scene = square(layers, [solid("A", ...A), solid("B", ...B), solid("A halb", ...A, 102)]);
    const img = await render(scene);
    const got = [0, 1, 2].map((k) => img.data[((H >> 1) * W + (W >> 1)) * 4 + k]);
    got.forEach((v, k) => assert.ok(Math.abs(v - expected[k]) <= 3, `center pixel ${got}, expected ${expected.map((x) => Math.round(x))}`));
  });
}

// Intel's test scenes without lights, as Acrobat Reader 26.003 shows them
// when the PDF has no view (crops of window captures, see acrobat.test.mjs).
// TextureLayers: one material with two layers (channels 0 and 2; the mesh
// has no coordinates for channel 2), one with a single layer; all multiply.
for (const name of ["TextureLayers", "shading2_base"]) {
  test(`textured Intel scene matches Acrobat Reader: ${name}`, { skip }, async () => {
    const reference = decodePng(fs.readFileSync(new URL(`../fixtures/acrobat/intel-${name}-noviews.png`, import.meta.url)));
    const r = compareImages(reference, await render(packU3d(`crates/pdf_3d/tests/fixtures/intel/${name}.u3d`)), (x, y) => x < 45 && y > H - 45);
    assert.ok(r.pixels > 5000, `only ${r.pixels} pixels compared`);
    assert.ok(r.overlap > 0.94, `silhouettes overlap ${r.overlap.toFixed(3)}`);
    assert.ok(r.mean < 0.01, `mean color difference ${r.mean.toFixed(3)}`);
    assert.ok(r.p95 < 0.03, `95th percentile of the color difference ${r.p95.toFixed(3)}`);
  });
}
