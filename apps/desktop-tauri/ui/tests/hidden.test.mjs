// Issue #24: Settings › PDF/A export showed the Ghostscript install hint
// although Ghostscript was found. Two parts: the display decision
// (ghostscriptView) and the stylesheet, which had no rule for a bare
// class="hidden".
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { ghostscriptView } from "../src/gsview.ts";

const template = readFileSync(new URL("../src/template.ts", import.meta.url), "utf8");
const css = template.slice(template.indexOf("export const CSS = `"));
const html = template.slice(0, template.indexOf("export const CSS = `"));

test("Ghostscript found: version and path, no install hint", () => {
  const v = ghostscriptView({
    available: true,
    path: "C:\\Program Files\\gs\\gs10.07.1\\bin\\gswin64c.exe",
    version: "10.07.1",
  });
  assert.equal(v.key, "gs.found");
  assert.equal(v.ok, true);
  assert.equal(v.showInstall, false);
  assert.deepEqual(v.params, {
    version: "10.07.1",
    path: "C:\\Program Files\\gs\\gs10.07.1\\bin\\gswin64c.exe",
  });
});

test("Ghostscript missing: install hint", () => {
  const v = ghostscriptView({ available: false, path: null, version: null });
  assert.equal(v.key, "gs.notFound");
  assert.equal(v.ok, false);
  assert.equal(v.showInstall, true);
});

test("stylesheet hides a bare .hidden element", () => {
  // A selector that is exactly ".hidden", not ".popover.hidden" and the like.
  assert.match(css, /(^|[},])\s*\.hidden\s*\{\s*display\s*:\s*none\s*;?\s*\}/m);
});

test("status rows of the settings page start hidden", () => {
  for (const id of ["gsInstall", "vpPathRow", "vpWarn"]) {
    assert.match(html, new RegExp(`<div id="${id}" class="hidden"`), id);
  }
});
