// Form field overlays stay on the page at every zoom, rotation and display
// scaling (issue #28). Run with `pnpm test` (Node >= 22.18 imports the
// TypeScript module directly).
//
// The renderer side (widget rectangles against PDFium's bitmap, font size of
// the appearance) is checked in crates/pdf_pdfium/tests/form_scaling.rs.

import test from "node:test";
import assert from "node:assert/strict";
import {
  cssAlign, cssColor, editorSize, fallbackFontSize, fieldBoxPercent, fontStack, pxPerPoint, shownRect, textTurns,
} from "../src/form-geom.ts";

// A 400 x 600 pt page; a field 200 x 20 pt at (100, 50).
const sz = { width_pt: 400, height_pt: 600 };
const field = { x0: 100, y0: 50, x1: 300, y1: 70 };
const near = (a, b, msg) => assert.ok(Math.abs(a - b) < 1e-9, `${msg}: ${a} vs ${b}`);

test("the field box turns with the view", () => {
  assert.deepEqual(shownRect(field, 0, sz), { x: 100, y: 50, w: 200, h: 20, pageW: 400, pageH: 600 });
  // 90° clockwise: the top edge of the page becomes the right edge.
  assert.deepEqual(shownRect(field, 1, sz), { x: 530, y: 100, w: 20, h: 200, pageW: 600, pageH: 400 });
  assert.deepEqual(shownRect(field, 2, sz), { x: 100, y: 530, w: 200, h: 20, pageW: 400, pageH: 600 });
  assert.deepEqual(shownRect(field, 3, sz), { x: 50, y: 100, w: 20, h: 200, pageW: 600, pageH: 400 });
  // Four quarter turns are none.
  assert.deepEqual(shownRect(field, 4, sz), shownRect(field, 0, sz));
});

test("the box is a fixed share of the slot, so it follows every zoom without being moved", () => {
  for (let rot = 0; rot < 4; rot++) {
    const s = shownRect(field, rot, sz);
    const p = fieldBoxPercent(s);
    for (const zoom of [0.1, 0.5, 1, 1.25, 2, 4, 16]) {
      // Slot size in CSS pixels as the viewer sets it (rounded).
      const slotW = Math.round(s.pageW * zoom), slotH = Math.round(s.pageH * zoom);
      const pt = pxPerPoint(slotW, rot, sz);
      near((p.left / 100) * slotW, s.x * pt, `left @${zoom} rot ${rot}`);
      near((p.width / 100) * slotW, s.w * pt, `width @${zoom} rot ${rot}`);
      // Vertically the rounding of the slot may differ by a fraction of a pixel.
      assert.ok(Math.abs((p.top / 100) * slotH - s.y * pt) < 1, `top @${zoom} rot ${rot}`);
      assert.ok(Math.abs((p.height / 100) * slotH - s.h * pt) < 0.5, `height @${zoom} rot ${rot}`);
    }
  }
});

test("text scales with the page: font pixels = points x zoom", () => {
  const fontPt = 12;
  for (const zoom of [0.5, 1, 2, 4]) {
    const pt = pxPerPoint(sz.width_pt * zoom, 0, sz);
    assert.equal(fontPt * pt, 12 * zoom);
    // In a turned view the slot is as wide as the page is high.
    assert.equal(pxPerPoint(sz.height_pt * zoom, 1, sz), zoom);
    assert.equal(pxPerPoint(sz.height_pt * zoom, 3, sz), zoom);
  }
  // Display scaling (125 % / 150 %) changes device pixels per CSS pixel, not
  // CSS pixels per point: page and overlay share the same CSS box.
  assert.equal(pxPerPoint(800, 0, sz), 2);
  assert.equal(pxPerPoint(0, 0, sz), 0);
  assert.equal(pxPerPoint(100, 0, { width_pt: 0, height_pt: 0 }), 0);
});

test("the editor has the text's direction: page /Rotate, the field's own rotation and the view", () => {
  const style = (turns) => ({ turns });
  assert.equal(textTurns(style(0), 0), 0);
  assert.equal(textTurns(style(1), 0), 1); // page /Rotate 90
  assert.equal(textTurns(style(1), 3), 0); // … viewed turned back
  assert.equal(textTurns(style(3), 2), 1);
  assert.equal(textTurns(null, 1), 1);
  assert.equal(textTurns(undefined, -1), 3);

  // On a page with /Rotate 90 the field is shown upright as 20 x 200; its
  // editor is the 200 x 20 line box, turned by 90°.
  const s = shownRect({ x0: 530, y0: 100, x1: 550, y1: 300 }, 0, { width_pt: 600, height_pt: 400 });
  assert.deepEqual(editorSize(s, 1), { w: 200, h: 20 });
  assert.deepEqual(editorSize(s, 3), { w: 200, h: 20 });
  assert.deepEqual(editorSize(s, 0), { w: 20, h: 200 });
  assert.deepEqual(editorSize(s, 2), { w: 20, h: 200 });
});

test("fields without style information get the automatic size of their height", () => {
  assert.equal(fallbackFontSize(22, false), 17.39);
  assert.equal(fallbackFontSize(30, false), 24.34);
  assert.equal(fallbackFontSize(3, false), 4);
  assert.equal(fallbackFontSize(80, true), 12);
});

test("style values map to CSS", () => {
  assert.equal(cssColor([1, 0, 0.5]), "rgb(255, 0, 128)");
  assert.equal(cssColor(undefined), "rgb(0, 0, 0)");
  assert.equal(cssColor([2, -1, 0]), "rgb(255, 0, 0)");
  assert.deepEqual([0, 1, 2, undefined].map(cssAlign), ["left", "center", "right", "left"]);
  assert.match(fontStack("Times"), /Times New Roman/);
  assert.match(fontStack("Cour"), /Courier New/);
  assert.match(fontStack("Helv"), /Arial/);
  assert.match(fontStack(undefined), /Arial/);
});
