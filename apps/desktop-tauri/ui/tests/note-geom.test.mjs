// Sticky-note icons stay upright at their pinned corner (issue #15). Run
// with `pnpm test` (Node >= 22.18 imports the TypeScript module directly).
//
// The renderer side of the same rule is checked against PDFium in
// crates/pdf_pdfium/tests/note_icons.rs, the listed rectangle in
// crates/pdf_mutate (drawn_geometry_lands_where_it_was_drawn_on_rotated_page).

import test from "node:test";
import assert from "node:assert/strict";
import { noteBoxInSlot, noteRectInPage } from "../src/note-geom.ts";

// A 400 x 600 pt page shown at 2 px per point; the note's icon square is
// 22 pt, pinned at (120, 150) of the unrotated view.
const sz = { width_pt: 400, height_pt: 600 };
const zoom = 2;
const note = { x0: 120, y0: 150, x1: 142, y1: 172 };
const slot = (rot) => (rot % 2 ? [sz.height_pt * zoom, sz.width_pt * zoom] : [sz.width_pt * zoom, sz.height_pt * zoom]);

// The pin on screen: the page point (120, 150) turned with the view.
const pinOnScreen = [[240, 300], [900, 240], [560, 900], [300, 560]];

test("the hotspot starts at the pinned corner and grows right and down in every view rotation", () => {
  for (let rot = 0; rot < 4; rot++) {
    const [w, h] = slot(rot);
    assert.deepEqual(
      noteBoxInSlot(note, rot, sz, w, h),
      { left: pinOnScreen[rot][0], top: pinOnScreen[rot][1], width: 44, height: 44 },
      `view ${rot * 90}°`,
    );
  }
});

test("rotations beyond a full turn wrap", () => {
  const [w, h] = slot(1);
  assert.deepEqual(noteBoxInSlot(note, 5, sz, w, h), noteBoxInSlot(note, 1, sz, w, h));
  assert.deepEqual(noteRectInPage(note, 6), noteRectInPage(note, 2));
});

// Screen pixel -> page point of the unrotated view (imagePxToPagePt in
// annot.ts): what a click on the page is hit-tested with.
function toPage(rot, x, y) {
  const [nx, ny] = [x / zoom, y / zoom];
  switch (rot) {
    case 1: return [ny, sz.height_pt - nx];
    case 2: return [sz.width_pt - nx, sz.height_pt - ny];
    case 3: return [sz.width_pt - ny, nx];
    default: return [nx, ny];
  }
}

test("the hit rectangle is the hotspot mapped back to the page", () => {
  for (let rot = 0; rot < 4; rot++) {
    const [w, h] = slot(rot);
    const box = noteBoxInSlot(note, rot, sz, w, h);
    const a = toPage(rot, box.left, box.top);
    const b = toPage(rot, box.left + box.width, box.top + box.height);
    const want = {
      x0: Math.min(a[0], b[0]), y0: Math.min(a[1], b[1]),
      x1: Math.max(a[0], b[0]), y1: Math.max(a[1], b[1]),
    };
    assert.deepEqual(noteRectInPage(note, rot), want, `view ${rot * 90}°`);
  }
});

test("without view rotation the listed rectangle is the icon itself", () => {
  assert.equal(noteRectInPage(note, 0), note);
});
