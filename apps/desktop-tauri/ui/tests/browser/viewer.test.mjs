// The live 3D viewer in a real browser (headless Chromium, WebGL).
// Run with `pnpm test:browser`; skipped when no Chromium is installed.

import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import { findBrowser, packU3d, startViewerPage } from "./harness.mjs";

const skip = findBrowser() ? false : "no Chromium (Edge or Chrome) found; set DOCURA_TEST_BROWSER";
let page;
before(async () => { if (!skip) page = await startViewerPage(); });
after(async () => { await page?.close(); });

/** Pixels with a clearly colored (not grey) value. */
function coloredPixels(shot) {
  let n = 0;
  for (let y = 0; y < shot.height; y++) {
    for (let x = 0; x < shot.width; x++) {
      const c = shot.at(x, y);
      if (Math.max(...c) - Math.min(...c) > 40) n++;
    }
  }
  return n;
}

test("textures are on screen without any input", { skip }, async () => {
  // Texture images decode asynchronously. The view used to be drawn once,
  // before they arrived, and showed the bare material until the user moved
  // the model. Nothing below sends input or asks for a redraw.
  const scene = packU3d("crates/pdf_3d/tests/fixtures/intel/shading2_base.u3d");
  const info = await page.evaluate(`return viewerTest.show(${JSON.stringify(scene)}, { w: 480, h: 360 })`);
  assert.equal(info.textures, 2);
  await page.evaluate("await viewerTest.ready()");
  const shot = await page.screenshot({ x: 0, y: 0, width: 480, height: 360 });
  // One face of the cube carries the "lines" texture: red, green and blue
  // strokes. Without its texture that face is plain white to grey, like
  // the rest of the picture.
  const colored = coloredPixels(shot);
  assert.ok(colored > 300, `expected the colored texture on screen, found ${colored} colored pixels`);
  await page.evaluate("viewerTest.close()");
});
