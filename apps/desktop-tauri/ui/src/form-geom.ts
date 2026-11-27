// Where a form field sits on its page slot and how its editor is set.
//
// An unfocused field shows what the page render draws: the field's own
// appearance. The overlay on top only marks it and, while it is edited,
// shows an editor that must match that appearance: same place, same font
// size in page points, same direction of the text. Everything here is
// expressed relative to the slot (fractions) or in page points, so the
// overlay follows the slot at every zoom through CSS alone: positions are
// percentages, lengths are `points * var(--pt)` with `--pt` the slot's CSS
// pixels per page point (see `pxPerPoint`, set by the viewer).
//
// Pure geometry, no DOM: tested with `pnpm test`.

import type { FieldTextStyle, PageSize, RectPt } from "./types";

/** A rectangle in page points of the page as shown (view rotation applied),
 * top-left origin, together with that page's size. */
export type ShownRect = { x: number; y: number; w: number; h: number; pageW: number; pageH: number };

/** `r` (points of the page without view rotation, `sz`) turned by the view
 * rotation `rot` (quarter turns clockwise). */
export function shownRect(r: RectPt, rot: number, sz: PageSize): ShownRect {
  const w0 = sz.width_pt, h0 = sz.height_pt;
  const w = r.x1 - r.x0, h = r.y1 - r.y0;
  switch (rot & 3) {
    case 1: return { x: h0 - r.y1, y: r.x0, w: h, h: w, pageW: h0, pageH: w0 };
    case 2: return { x: w0 - r.x1, y: h0 - r.y1, w, h, pageW: w0, pageH: h0 };
    case 3: return { x: r.y0, y: w0 - r.x1, w: h, h: w, pageW: h0, pageH: w0 };
    default: return { x: r.x0, y: r.y0, w, h, pageW: w0, pageH: h0 };
  }
}

/** The field box as CSS percentages of the slot. */
export function fieldBoxPercent(s: ShownRect): { left: number; top: number; width: number; height: number } {
  return {
    left: (s.x / s.pageW) * 100,
    top: (s.y / s.pageH) * 100,
    width: (s.w / s.pageW) * 100,
    height: (s.h / s.pageH) * 100,
  };
}

/** CSS pixels per page point of a slot `slotWidthPx` wide that shows page
 * `sz` at view rotation `rot`. Display scaling (devicePixelRatio) is not
 * part of it: CSS pixels already scale with it, like the slot itself. */
export function pxPerPoint(slotWidthPx: number, rot: number, sz: PageSize): number {
  const shownW = rot & 1 ? sz.height_pt : sz.width_pt;
  return shownW > 0 ? slotWidthPx / shownW : 0;
}

/** Quarter turns (clockwise) the field's text is shown at: its direction on
 * the page plus the view rotation. */
export function textTurns(style: FieldTextStyle | null | undefined, rot: number): number {
  return (((style?.turns ?? 0) + rot) % 4 + 4) % 4;
}

/** Size of the editor in page points: the field box in the orientation of
 * its text. The editor is centred on the box and turned by `turns`. */
export function editorSize(s: ShownRect, turns: number): { w: number; h: number } {
  return turns & 1 ? { w: s.h, h: s.w } : { w: s.w, h: s.h };
}

/** Font size in page points for a field without style information: what
 * automatic sizing gives an empty single-line field of height `h`
 * (pdf_mutate::auto_size_single), multi-line fields start at 12. */
export function fallbackFontSize(h: number, multiline: boolean): number {
  if (multiline) return 12;
  return Math.max(4, Math.floor(((h - 2) / 1.15) * 100) / 100);
}

/** CSS font stack of a standard family id. */
export function fontStack(family: string | undefined): string {
  switch (family) {
    case "Times": return '"Times New Roman", Times, serif';
    case "Cour": return '"Courier New", Courier, monospace';
    default: return "Helvetica, Arial, sans-serif";
  }
}

export function cssColor(c: [number, number, number] | undefined): string {
  const v = (x: number) => Math.round(Math.min(1, Math.max(0, x)) * 255);
  return c ? `rgb(${v(c[0])}, ${v(c[1])}, ${v(c[2])})` : "rgb(0, 0, 0)";
}

export function cssAlign(align: number | undefined): string {
  return align === 1 ? "center" : align === 2 ? "right" : "left";
}
