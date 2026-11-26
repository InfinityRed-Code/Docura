// Camera convention of PDF 3D views (/C2W). Run with `pnpm test`
// (Node >= 22.18 imports the TypeScript module directly).
//
// Reference: media9 (CTAN) builds /C2W as [left up view position] with
// left = up x view; its default camera sits at -Y looking along +Y with Z up:
// [-1 0 0  0 0 1  0 1 0  0 -d 0]. The same reference values are checked on
// the Rust side in crates/pdf_mutate/tests/threed.rs.

import fs from "node:fs";
import test from "node:test";
import assert from "node:assert/strict";
import { c2wLookAt, cameraForView, cross, dot, standardViews, verticalFov } from "../src/threed-camera.ts";

const near = (a, b, eps = 1e-6) => assert.ok(a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) < eps), `${a} != ${b}`);

const view = (c2w, extra = {}) => ({
  name: "v", internal_name: null, c2w, u3d_path: [], center_of_orbit: null,
  orthographic: false, fov: 30, fov_span: "Min", ortho_scale: null, ortho_binding: null,
  background: null, render_mode: null, render_aux_color: null, lighting: null,
  cross_sections: [], nodes: [], ...extra,
});

test("front view of a Z-up model matches the media9 default matrix", () => {
  near(c2wLookAt([0, -10, 0], [0, 0, 0], [0, 0, 1]), [-1, 0, 0, 0, 0, 1, 0, 1, 0, 0, -10, 0]);
});

test("top view: looking down with +Y up gives left = -X", () => {
  near(c2wLookAt([0, 0, 5], [0, 0, 0], [0, 1, 0]), [-1, 0, 0, 0, 1, 0, 0, 0, -1, 0, 0, 5]);
});

test("c2wLookAt is a proper rotation with left = up x view", () => {
  const m = c2wLookAt([7, -4, 3], [1, 2, 0.5], [0, 0, 1]);
  const [x, y, z] = [m.slice(0, 3), m.slice(3, 6), m.slice(6, 9)];
  for (const v of [x, y, z]) assert.ok(Math.abs(Math.hypot(...v) - 1) < 1e-9);
  assert.ok(Math.abs(dot(x, y)) < 1e-9 && Math.abs(dot(y, z)) < 1e-9 && Math.abs(dot(x, z)) < 1e-9);
  assert.ok(Math.abs(dot(x, cross(y, z)) - 1) < 1e-9, "determinant +1");
  near(x, cross(y, z));
  assert.ok(y[2] > 0, "up column points up");
});

test("reading /C2W: y column is up, z column the view direction", () => {
  const cam = cameraForView(view([-1, 0, 0, 0, 0, 1, 0, 1, 0, 0, -10, 0], { center_of_orbit: 10 }), null, 400, 300, []);
  near(cam.position, [0, -10, 0]);
  near(cam.target, [0, 0, 0]);
  near(cam.up, [0, 0, 1]);
});

test("a written view reads back as the same camera", () => {
  const position = [30, -40, 25], target = [2, 3, 4];
  const d = Math.hypot(28, -43, 21);
  const cam = cameraForView(view(c2wLookAt(position, target, [0, 0, 1]), { center_of_orbit: d }), null, 400, 300, []);
  near(cam.position, position);
  near(cam.target, target, 1e-4);
  assert.ok(cam.up[2] > 0.5);
});

test("without /CO the orbit target is the model center's projection on the line of sight", () => {
  const bounds = { min: [-1, -1, -1], max: [1, 1, 1] };
  const cam = cameraForView(view([-1, 0, 0, 0, 0, 1, 0, 1, 0, 0, -10, 0]), bounds, 400, 300, []);
  near(cam.target, [0, 0, 0]);
});

test("/U3DPath: U3D view nodes look along -z with +y up", () => {
  // Column-major node matrix: identity rotation, positioned at (0, 0, 8).
  const cams = [{ name: "Cam", matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 8, 1], orthographic: false, projection: 30 }];
  const cam = cameraForView(view(null, { u3d_path: ["Cam"], center_of_orbit: 8 }), null, 400, 300, cams);
  near(cam.position, [0, 0, 8]);
  near(cam.target, [0, 0, 0]);
  near(cam.up, [0, 1, 0]);
});

test("standard views keep Z up and frame the model", () => {
  const bounds = { min: [0, 0, 0], max: [4, 2, 1] };
  const views = standardViews(bounds, {}, 400, 300);
  assert.deepEqual(views.map((v) => v.internal_name), ["iso", "front", "back", "left", "right", "top", "bottom"]);
  const by = Object.fromEntries(views.map((v) => [v.internal_name, v]));
  for (const key of ["iso", "front", "back", "left", "right"]) {
    const m = by[key].c2w;
    assert.ok(m[5] > 0.5, `${key}: up column has +Z`);
    assert.ok(Math.abs(dot(m.slice(0, 3), cross(m.slice(3, 6), m.slice(6, 9))) - 1) < 1e-6, `${key}: proper rotation`);
  }
  near(by.front.c2w.slice(0, 9), [-1, 0, 0, 0, 0, 1, 0, 1, 0]);
  near(by.top.c2w.slice(0, 9), [-1, 0, 0, 0, 1, 0, 0, 0, -1]);
  near(by.bottom.c2w.slice(0, 9), [-1, 0, 0, 0, -1, 0, 0, 0, 1]);
  // Every camera looks at the center from /CO away.
  for (const v of views) {
    const m = v.c2w;
    near([0, 1, 2].map((i) => m[9 + i] + m[6 + i] * v.center_of_orbit), [2, 1, 0.5], 1e-4);
  }
});

test("verticalFov converts the PDF field of view by its span", () => {
  assert.ok(Math.abs(verticalFov(30, "H", 400, 300) - 30) < 1e-9);
  assert.ok(Math.abs(verticalFov(30, "Min", 400, 300) - 30) < 1e-9);
  const w = verticalFov(30, "W", 400, 300); // 30 degrees across the width
  assert.ok(Math.abs(Math.tan((w * Math.PI) / 360) - Math.tan(Math.PI / 12) * 0.75) < 1e-9);
});

const unit = (v) => {
  const l = Math.hypot(...v);
  return v.map((x) => x / l);
};

test("CAD shading matches Adobe Acrobat Reader on a sphere, all seven views", async () => {
  const { shade } = await import("../src/threed-camera.ts");
  const ref = JSON.parse(fs.readFileSync(new URL("./fixtures/acrobat-cad-sphere.json", import.meta.url), "utf8"));
  for (const [name, sphere] of Object.entries(ref.spheres)) {
    for (const [view, samples] of Object.entries(sphere.views)) {
      const toCamera = unit(ref.views[view].toCamera);
      let sum = 0, worst = 0;
      for (const s of samples) {
        const got = shade("CAD", sphere.diffuse, sphere.specular, s.n, toCamera);
        for (let k = 0; k < 3; k++) {
          const d = Math.abs(got[k] - s.c[k]);
          sum += d;
          worst = Math.max(worst, d);
        }
      }
      const mean = sum / (samples.length * 3);
      assert.ok(mean < 0.015, `${name} sphere, view ${view}: mean deviation ${mean.toFixed(3)}`);
      assert.ok(worst < 0.05, `${name} sphere, view ${view}: largest deviation ${worst.toFixed(3)}`);
    }
  }
});

test("CAD shading of the faces measured on orientation.pdf", async () => {
  const { shade } = await import("../src/threed-camera.ts");
  // Flat faces of orientation.pdf in Acrobat Reader 26.003: diffuse color
  // (specular 0.25), face normal, direction to the camera, pixel.
  // Faces towards the camera only: on oblique faces the direction to the
  // camera changes across the face (tests/browser compares those pixels).
  const Y = [0, -1, 0], Z = [0, 0, 1];
  const red = [0.9, 0.1, 0.1], green = [0.1, 0.7, 0.15], blue = [0.1, 0.25, 0.9], yellow = [0.95, 0.8, 0.1];
  const cases = [
    ["front red", red, Y, Y, [240, 75, 84]],
    ["front blue", blue, Y, Y, [77, 108, 255]],
    ["front yellow", yellow, Y, Y, [251, 220, 84]],
    ["top red", red, Z, Z, [255, 86, 97]],
    ["top green", green, Z, Z, [87, 237, 111]],
  ];
  for (const [name, diffuse, normal, toCamera, px] of cases) {
    const got = shade("CAD", diffuse, [0.25, 0.25, 0.25], normal, toCamera);
    got.forEach((v, k) => assert.ok(Math.abs(v - px[k] / 255) < 0.03, `${name}: channel ${k} is ${v.toFixed(3)}, Acrobat ${(px[k] / 255).toFixed(3)}`));
  }
  near(shade("nope", [0.2, 0.3, 0.4], [0, 0, 0], Z, Z), [0.2, 0.3, 0.4]);
});

test("default camera without a view: Acrobat's direction, fitted to the model", async () => {
  const { defaultCamera, DEFAULT_BACKGROUND } = await import("../src/threed-camera.ts");
  const bounds = { min: [0, 0, 0], max: [300, 200, 120] };
  const cam = defaultCamera(bounds, 762 / 475);
  near(cam.target, [150, 100, 60]);
  const toCam = unit(cam.position.map((v, k) => v - cam.target[k]));
  // Measured in Acrobat Reader 26.003 (orientation_noviews.pdf, box.pdf).
  near(toCam, [-0.906, -0.375, 0.195], 0.002);
  near(cam.up, [0, 0, 1]);
  assert.equal(cam.orthographic, false);
  assert.ok(Math.abs(cam.fovV - 30) < 1e-9, "30 degrees over the smaller dimension");
  const radius = Math.hypot(300, 200, 120) / 2;
  const dist = Math.hypot(...cam.position.map((v, k) => v - cam.target[k]));
  assert.ok(Math.abs(dist / radius - 3.79) < 0.02, `distance ${(dist / radius).toFixed(3)} radii`);
  near(DEFAULT_BACKGROUND, [0.2, 0.2, 0.2]);
});
