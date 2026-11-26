// Where the icon of a sticky note (Text annotation) sits.
//
// A note icon does not turn with the page or the view: the renderer draws
// it upright, right of and below its pinned corner (pdf_pdfium::notes). The
// `rect_pt` of a Text annotation is that square in the unrotated view, its
// top-left corner being the pin. A view rotation moves the pin with the
// page, but the icon keeps growing right and down from it on screen.
//
// Pure geometry, no DOM: tested with `pnpm test`.

import type { PageSize, RectPt } from "./types";

export type SlotBox = { left: number; top: number; width: number; height: number };

/** The icon square of a note in slot pixels. `rot`: quarter turns of the
 * view, `sz`: the page as shown without view rotation, `imgW` x `imgH`: the
 * slot showing the page with it. */
export function noteBoxInSlot(r: RectPt, rot: number, sz: PageSize, imgW: number, imgH: number): SlotBox {
  const w0 = sz.width_pt, h0 = sz.height_pt;
  let x = r.x0, y = r.y0, dw = w0, dh = h0;
  switch (rot & 3) {
    case 1: x = h0 - r.y0; y = r.x0; dw = h0; dh = w0; break;
    case 2: x = w0 - r.x0; y = h0 - r.y0; break;
    case 3: x = r.y0; y = w0 - r.x0; dw = h0; dh = w0; break;
  }
  const sx = imgW / dw, sy = imgH / dh;
  return { left: x * sx, top: y * sy, width: (r.x1 - r.x0) * sx, height: (r.y1 - r.y0) * sy };
}

/** The part of the page the icon covers at view rotation `rot`, in page
 * points of the unrotated view (what a click maps to): for hit tests. */
export function noteRectInPage(r: RectPt, rot: number): RectPt {
  const w = r.x1 - r.x0, h = r.y1 - r.y0;
  switch (rot & 3) {
    case 1: return { x0: r.x0, y0: r.y0 - w, x1: r.x0 + h, y1: r.y0 };
    case 2: return { x0: r.x0 - w, y0: r.y0 - h, x1: r.x0, y1: r.y0 };
    case 3: return { x0: r.x0 - h, y0: r.y0, x1: r.x0, y1: r.y0 + w };
    default: return r;
  }
}
