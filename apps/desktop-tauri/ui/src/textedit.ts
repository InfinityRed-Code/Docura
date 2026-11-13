import { invoke } from "@tauri-apps/api/core";
import type { AnnotationInfo, DocSnapshot, RectPt, Slot, TextBlock } from "./types";
import { state, callbacks } from "./state";
import { t } from "./i18n";
import { dom, setStatus } from "./dom";
import { refreshIcons } from "./icons";
import { applyDocSnapshot } from "./doc";
import { effectiveRotation, rotateRectToSlot } from "./viewer";
import { findSlotFromTarget, hexToRgb01, imagePxToPagePt, isOverlayControl } from "./annot";

// ---------- Text editing ----------
//
// One floating editor serves three cases:
//  - a new text box (FreeText annotation) placed with the Text tool,
//  - an existing text box, reopened by clicking it,
//  - a line of text that is part of the page itself (native or OCR text),
//    which is replaced in the page content on commit.
//
// The editor is a plaintext contenteditable laid out with the metrics of the
// font the backend will write (Arial = Helvetica, Times New Roman = Times,
// Courier New = Courier), so what is typed is what lands in the PDF.

type Family = "Helv" | "Times" | "Cour";

type Style = {
  family: Family;
  /** Font size in PDF points. */
  size: number;
  bold: boolean;
  italic: boolean;
  /** Text color, #rrggbb. */
  color: string;
  /** Box background, #rrggbb, or null for transparent (text boxes only). */
  bg: string | null;
  /** Page text only: family name of the line's own installed font, kept as
   * long as family / bold / italic are left alone; null = standard font. */
  face?: string | null;
};

type Target =
  | { kind: "new" }
  | { kind: "annot"; annot: AnnotationInfo }
  | { kind: "block"; block: TextBlock; cover: string };

type Editor = {
  slot: Slot;
  pageIdx: number;
  target: Target;
  style: Style;
  initialStyle: Style;
  initialText: string;
  /** Anchor in slot pixels: top-left of the box, or for page text the start
   * of the baseline. */
  x: number;
  y: number;
  moved: boolean;
  /** Screen rotation of the text in quarter turns clockwise. */
  quadrant: number;
  root: HTMLDivElement;
  box: HTMLDivElement;
  bar: HTMLDivElement;
  patch: HTMLDivElement | null;
};

const CSS_FAMILY: Record<Family, string> = {
  Helv: "Arial, Helvetica, sans-serif",
  Times: '"Times New Roman", Times, serif',
  Cour: '"Courier New", Courier, monospace',
};

function cssFamily(style: Style): string {
  const std = CSS_FAMILY[style.family];
  return style.face ? `"${style.face.replace(/["\\]/g, "")}", ${std}` : std;
}

/** Padding inside a text box in PDF points — the margin the appearance
 * stream leaves around the text. */
const BOX_PAD_PT = 2;
const LINE_HEIGHT = 1.2;

let active: Editor | null = null;
/** Style of the last text box, reused for the next new one. */
let lastStyle: Style = { family: "Helv", size: 14, bold: false, italic: false, color: "#000000", bg: null };

const blocksCache = new Map<number, TextBlock[]>();
let blocksGen = -1;

function asFamily(s: string | undefined): Family {
  return s === "Times" || s === "Cour" ? s : "Helv";
}

function rgbToHex(c: [number, number, number]): string {
  const h = (v: number) => Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).padStart(2, "0");
  return `#${h(c[0])}${h(c[1])}${h(c[2])}`;
}

function sameStyle(a: Style, b: Style): boolean {
  return a.family === b.family && a.size === b.size && a.bold === b.bold
    && a.italic === b.italic && a.color === b.color && a.bg === b.bg
    && (a.face ?? null) === (b.face ?? null);
}

/** Screen pixels per PDF point on this page at the current zoom. */
function pxPerPt(slot: Slot, pageIdx: number): number {
  const sz = state.pageSizes[pageIdx];
  if (!sz) return 1;
  const rot = effectiveRotation(pageIdx);
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  const w = slot.wrap.getBoundingClientRect().width;
  return dw > 0 && w > 0 ? w / dw : 1;
}

let measureCanvas: HTMLCanvasElement | null = null;
function measureCtx(style: Style, px: number): CanvasRenderingContext2D | null {
  measureCanvas ??= document.createElement("canvas");
  const ctx = measureCanvas.getContext("2d");
  if (!ctx) return null;
  ctx.font = `${style.italic ? "italic " : ""}${style.bold ? "bold " : ""}${px}px ${cssFamily(style)}`;
  return ctx;
}

/** Distance from the top of a line box to its baseline, in pixels. */
function baselineOffset(style: Style, px: number): number {
  const m = measureCtx(style, px)?.measureText("Hg");
  const asc = m?.fontBoundingBoxAscent ?? px * 0.9;
  const desc = m?.fontBoundingBoxDescent ?? px * 0.21;
  return (LINE_HEIGHT * px - (asc + desc)) / 2 + asc;
}

/** Font size (pt) at which `text` is `widthPt` wide — for OCR text, whose
 * own size is only the height of a recognised word box. */
function fitFontSize(text: string, style: Style, widthPt: number, fallback: number): number {
  const w = measureCtx(style, 100)?.measureText(text).width ?? 0;
  if (w <= 0 || widthPt <= 0) return fallback;
  return Math.max(4, Math.min(400, Math.round((widthPt / (w / 100)) * 10) / 10));
}

/** Page background (and, for scans, ink color) around a box of the rendered
 * page bitmap. Falls back to white / black when the bitmap is unavailable. */
function samplePage(
  slot: Slot,
  box: { left: number; top: number; width: number; height: number },
): { bg: string; ink: string } {
  const fallback = { bg: "#ffffff", ink: "#000000" };
  const img = slot.img;
  if (!img.naturalWidth || !img.width) return fallback;
  const k = img.naturalWidth / img.width;
  const pad = 3;
  const sx = Math.max(0, Math.floor((box.left - pad) * k));
  const sy = Math.max(0, Math.floor((box.top - pad) * k));
  const sw = Math.min(img.naturalWidth - sx, Math.ceil((box.width + 2 * pad) * k));
  const sh = Math.min(img.naturalHeight - sy, Math.ceil((box.height + 2 * pad) * k));
  if (sw < 2 || sh < 2) return fallback;
  try {
    const cv = document.createElement("canvas");
    cv.width = sw;
    cv.height = sh;
    const ctx = cv.getContext("2d", { willReadFrequently: true });
    if (!ctx) return fallback;
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, sw, sh);
    const d = ctx.getImageData(0, 0, sw, sh).data;
    const lum = (i: number) => 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
    // Background: median-luminance pixel of the frame around the box.
    const frame: number[] = [];
    const edge = Math.max(1, Math.round(pad * k));
    for (let y = 0; y < sh; y++) {
      for (let x = 0; x < sw; x++) {
        if (x < edge || y < edge || x >= sw - edge || y >= sh - edge) frame.push((y * sw + x) * 4);
      }
    }
    frame.sort((a, b) => lum(a) - lum(b));
    const b = frame[frame.length >> 1];
    const bgLum = lum(b);
    const hex = (r: number, g: number, bl: number) => rgbToHex([r / 255, g / 255, bl / 255]);
    // Ink: average of the pixels that contrast clearly with the background.
    let r = 0, g = 0, bl = 0, n = 0;
    for (let i = 0; i < d.length; i += 4) {
      if (Math.abs(lum(i) - bgLum) > 90) { r += d[i]; g += d[i + 1]; bl += d[i + 2]; n++; }
    }
    return {
      bg: hex(d[b], d[b + 1], d[b + 2]),
      ink: n > 0 ? hex(r / n, g / n, bl / n) : bgLum < 110 ? "#ffffff" : "#000000",
    };
  } catch {
    return fallback;
  }
}

// ---------- Editor ----------

function editorText(ed: Editor): string {
  return ed.box.innerText.replace(/\r/g, "").replace(/ /g, " ").replace(/\n+$/, "");
}

/** Apply the editor's style and anchor to the DOM and re-place the toolbar. */
function layout(ed: Editor) {
  const ppx = pxPerPt(ed.slot, ed.pageIdx);
  const px = ed.style.size * ppx;
  const s = ed.box.style;
  s.fontFamily = cssFamily(ed.style);
  s.fontSize = `${px}px`;
  s.fontWeight = ed.style.bold ? "bold" : "normal";
  s.fontStyle = ed.style.italic ? "italic" : "normal";
  s.color = ed.style.color;
  ed.root.style.left = `${ed.x}px`;
  ed.root.style.top = `${ed.y}px`;
  if (ed.target.kind === "block") {
    // Anchor = baseline start: lift the line box so its baseline sits on it.
    s.padding = "0";
    s.background = "transparent";
    ed.root.style.transform =
      `rotate(${ed.quadrant * 90}deg) translate(0, ${-baselineOffset(ed.style, px)}px)`;
  } else {
    s.padding = `${BOX_PAD_PT * ppx}px`;
    s.background = ed.style.bg ?? "transparent";
    ed.root.style.transform = "";
  }
  placeBar(ed);
  syncBar(ed);
}

/** Put the toolbar just above the box (below when there is no room). */
function placeBar(ed: Editor) {
  const wrap = ed.slot.wrap.getBoundingClientRect();
  const box = ed.box.getBoundingClientRect();
  const bar = ed.bar.getBoundingClientRect();
  const gap = 8;
  let top = box.top - wrap.top - bar.height - gap;
  // Keep it inside the viewer: flip below the box near the top edge.
  const viewerTop = dom.viewer.getBoundingClientRect().top;
  if (box.top - bar.height - gap < viewerTop) top = box.bottom - wrap.top + gap;
  const left = Math.max(0, Math.min(box.left - wrap.left, wrap.width - bar.width));
  ed.bar.style.left = `${left}px`;
  ed.bar.style.top = `${top}px`;
}

function syncBar(ed: Editor) {
  const q = <T extends HTMLElement>(sel: string) => ed.bar.querySelector(sel) as T;
  q<HTMLSelectElement>(".teFamily").value = ed.style.family;
  q<HTMLInputElement>(".teSize").value = String(ed.style.size);
  q<HTMLButtonElement>(".teBold").classList.toggle("active", ed.style.bold);
  q<HTMLButtonElement>(".teItalic").classList.toggle("active", ed.style.italic);
  q<HTMLInputElement>(".teColor").value = ed.style.color;
  q<HTMLElement>(".teColorSwatch").style.background = ed.style.color;
  const bg = ed.bar.querySelector(".teBg") as HTMLInputElement | null;
  if (bg) {
    bg.value = ed.style.bg ?? "#ffffff";
    q<HTMLElement>(".teBgSwatch").style.background = ed.style.bg ?? "transparent";
    q<HTMLElement>(".teBgSwatch").classList.toggle("none", ed.style.bg === null);
    q<HTMLButtonElement>(".teNoBg").classList.toggle("active", ed.style.bg === null);
  }
}

function buildBar(ed: Editor): HTMLDivElement {
  const bar = document.createElement("div");
  bar.className = "textEdBar";
  const isBox = ed.target.kind !== "block";
  bar.innerHTML = `
    ${isBox ? `<span class="teGrip" title="${t("text.move")}"><i data-lucide="grip-vertical"></i></span>` : ""}
    <select class="teFamily" title="${t("tip.fontFamily")}">
      <option value="Helv">${t("text.fontSans")}</option>
      <option value="Times">${t("text.fontSerif")}</option>
      <option value="Cour">${t("text.fontMono")}</option>
    </select>
    <input class="teSize" type="number" min="4" max="400" step="1" title="${t("tip.fontSize")}" />
    <span class="teSep"></span>
    <button type="button" class="teBtn teBold" title="${t("text.bold")}"><i data-lucide="bold"></i></button>
    <button type="button" class="teBtn teItalic" title="${t("text.italic")}"><i data-lucide="italic"></i></button>
    <label class="teBtn teSwatchBtn" title="${t("tip.textColor")}">
      <i data-lucide="type"></i><span class="teSwatch teColorSwatch"></span>
      <input class="teColor" type="color" />
    </label>
    ${isBox ? `
    <label class="teBtn teSwatchBtn" title="${t("text.bg")}">
      <i data-lucide="paint-bucket"></i><span class="teSwatch teBgSwatch"></span>
      <input class="teBg" type="color" />
    </label>
    <button type="button" class="teBtn teNoBg" title="${t("tip.transparentBg")}"><i data-lucide="droplet"></i></button>` : ""}
    <span class="teSep"></span>
    <button type="button" class="teBtn teDelete" title="${t("btn.delete")}"><i data-lucide="trash-2"></i></button>
    <button type="button" class="teBtn teDone" title="${t("text.done")}"><i data-lucide="check"></i></button>`;

  const q = <T extends HTMLElement>(sel: string) => bar.querySelector(sel) as T | null;
  const change = (fn: (s: Style) => void, refocus = true) => {
    fn(ed.style);
    layout(ed);
    if (refocus) ed.box.focus();
  };
  // Buttons must not take the focus (and with it the caret) from the text.
  bar.querySelectorAll("button").forEach((b) => b.addEventListener("mousedown", (e) => e.preventDefault()));

  q<HTMLSelectElement>(".teFamily")!.addEventListener("change", (e) =>
    change((s) => { s.family = asFamily((e.target as HTMLSelectElement).value); s.face = null; }));
  q<HTMLInputElement>(".teSize")!.addEventListener("input", (e) => {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (Number.isFinite(v) && v >= 4 && v <= 400) change((s) => { s.size = v; }, false);
  });
  q(".teBold")!.addEventListener("click", () => change((s) => { s.bold = !s.bold; s.face = null; }));
  q(".teItalic")!.addEventListener("click", () => change((s) => { s.italic = !s.italic; s.face = null; }));
  q<HTMLInputElement>(".teColor")!.addEventListener("input", (e) =>
    change((s) => { s.color = (e.target as HTMLInputElement).value; }, false));
  q<HTMLInputElement>(".teBg")?.addEventListener("input", (e) =>
    change((s) => { s.bg = (e.target as HTMLInputElement).value; }, false));
  q(".teNoBg")?.addEventListener("click", () => change((s) => { s.bg = s.bg === null ? "#ffffff" : null; }));
  q(".teDelete")!.addEventListener("click", () => void finish(true));
  q(".teDone")!.addEventListener("click", () => void finish(false));

  // Drag the grip to move a text box.
  const grip = q(".teGrip");
  grip?.addEventListener("pointerdown", (ev) => {
    ev.preventDefault();
    const sx = ev.clientX, sy = ev.clientY, ox = ed.x, oy = ed.y;
    grip.setPointerCapture(ev.pointerId);
    const move = (e: PointerEvent) => {
      const wrap = ed.slot.wrap.getBoundingClientRect();
      ed.x = Math.max(0, Math.min(wrap.width - 8, ox + e.clientX - sx));
      ed.y = Math.max(0, Math.min(wrap.height - 8, oy + e.clientY - sy));
      ed.moved = true;
      layout(ed);
    };
    const up = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", up);
      ed.box.focus();
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", up);
  });
  return bar;
}

function openEditor(init: {
  slot: Slot; pageIdx: number; target: Target; style: Style; text: string;
  x: number; y: number; quadrant?: number;
  /** Box covering the original on the page bitmap while it is edited. */
  patch?: { left: number; top: number; width: number; height: number; color: string };
  /** Show the patch only once something has been typed. */
  patchWhenTyped?: boolean;
}) {
  if (active) closeEditor();
  const root = document.createElement("div");
  root.className = "textEd";
  const box = document.createElement("div");
  box.className = "textEdBox";
  box.setAttribute("contenteditable", "plaintext-only");
  if (box.contentEditable !== "plaintext-only") box.contentEditable = "true";
  box.spellcheck = false;
  box.textContent = init.text;
  root.appendChild(box);

  let patch: HTMLDivElement | null = null;
  if (init.patch) {
    patch = document.createElement("div");
    patch.className = "textEdPatch";
    patch.style.left = `${init.patch.left - 1}px`;
    patch.style.top = `${init.patch.top - 1}px`;
    patch.style.width = `${init.patch.width + 2}px`;
    patch.style.height = `${init.patch.height + 2}px`;
    patch.style.background = init.patch.color;
    init.slot.wrap.appendChild(patch);
  }

  const ed: Editor = {
    slot: init.slot, pageIdx: init.pageIdx, target: init.target,
    style: { ...init.style }, initialStyle: { ...init.style }, initialText: init.text,
    x: init.x, y: init.y, moved: false, quadrant: init.quadrant ?? 0,
    root, box, bar: document.createElement("div"), patch,
  };
  ed.bar = buildBar(ed);
  init.slot.wrap.appendChild(root);
  init.slot.wrap.appendChild(ed.bar);
  refreshIcons();
  active = ed;
  layout(ed);
  // Presses inside the editor belong to it alone (no pan, selection, tools).
  for (const el of [root, ed.bar]) {
    for (const type of ["pointerdown", "mousedown", "mouseup", "click", "dblclick"]) {
      el.addEventListener(type, (e) => e.stopPropagation());
    }
  }

  const syncPatch = () => {
    if (patch && init.patchWhenTyped) patch.style.display = box.textContent ? "" : "none";
  };
  syncPatch();
  box.addEventListener("input", () => { syncPatch(); placeBar(ed); });
  // Pasting must not bring formatting along where plaintext-only is missing.
  box.addEventListener("paste", (e) => {
    if (box.contentEditable === "plaintext-only") return;
    e.preventDefault();
    document.execCommand("insertText", false, e.clipboardData?.getData("text/plain") ?? "");
  });
  const onKey = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); closeEditor(); return; }
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey || ed.target.kind === "block")) {
      e.preventDefault();
      void finish(false);
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "b" || e.key === "i")) {
      e.preventDefault();
      if (e.key === "b") ed.style.bold = !ed.style.bold;
      else ed.style.italic = !ed.style.italic;
      ed.style.face = null;
      layout(ed);
    }
  };
  box.addEventListener("keydown", onKey);
  ed.bar.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); closeEditor(); }
    else if (e.key === "Enter") { e.preventDefault(); ed.box.focus(); }
  });

  box.focus();
  // Caret at the end of the existing text.
  const sel = window.getSelection();
  if (sel) {
    sel.selectAllChildren(box);
    sel.collapseToEnd();
  }
}

function closeEditor() {
  const ed = active;
  if (!ed) return;
  active = null;
  ed.root.remove();
  ed.bar.remove();
  ed.patch?.remove();
}

function styleArgs(s: Style) {
  const bg = s.bg ? hexToRgb01(s.bg) : [1, 1, 1];
  return {
    color: hexToRgb01(s.color),
    bg_color: [bg[0], bg[1], bg[2], s.bg ? 1 : 0],
    font_size: s.size,
    font_family: s.family,
    bold: s.bold,
    italic: s.italic,
  };
}

/** Close the editor and write its content to the document. `remove` deletes
 * the edited text / box instead. */
async function finish(remove: boolean) {
  const ed = active;
  if (!ed || !state.currentDoc) { closeEditor(); return; }
  const text = remove ? "" : editorText(ed);
  const changed = text !== ed.initialText || !sameStyle(ed.style, ed.initialStyle) || ed.moved;
  // Geometry has to be read before the editor leaves the DOM.
  const r = ed.box.getBoundingClientRect();
  const wrap = ed.slot.wrap.getBoundingClientRect();
  const { slot, pageIdx, target, style } = ed;
  closeEditor();

  const document_ = state.currentDoc;
  try {
    if (target.kind === "block") {
      if (!changed) return;
      setStatus(t("text.updating"));
      const snap = await invoke<DocSnapshot>("replace_page_text", {
        document: document_,
        page: pageIdx,
        edit: {
          objects: target.block.objects,
          text,
          font_size: style.size,
          font_family: style.family,
          bold: style.bold,
          italic: style.italic,
          color: hexToRgb01(style.color),
          cover: target.block.invisible ? hexToRgb01(target.cover) : null,
          font_file: style.face ? target.block.font_file ?? null : null,
        },
      });
      applyDocSnapshot(snap);
      setStatus(t(text ? "text.updated" : "text.removed"));
      return;
    }

    if (target.kind === "new" && !text.trim()) return;
    if (target.kind === "annot" && !changed) return;
    const tl = imagePxToPagePt(slot, pageIdx, r.left - wrap.left, r.top - wrap.top);
    const br = imagePxToPagePt(slot, pageIdx, r.right - wrap.left, r.bottom - wrap.top);
    if (!tl || !br) return;
    const rect: RectPt = {
      x0: Math.min(tl.x, br.x), y0: Math.min(tl.y, br.y),
      x1: Math.max(tl.x, br.x), y1: Math.max(tl.y, br.y),
    };
    const args = {
      page: pageIdx,
      rect,
      text,
      ...styleArgs(style),
      rotation_quadrant: effectiveRotation(pageIdx),
      opacity: 1.0,
      print: target.kind === "annot" ? target.annot.print : state.printAnnotations,
      author: target.kind === "annot" ? target.annot.author : null,
    };
    lastStyle = { ...style };
    const label = t("kind.FreeText");
    if (target.kind === "new") {
      setStatus(t("st.adding", { label }));
      applyDocSnapshot(await invoke<DocSnapshot>("add_freetext", { document: document_, args }));
      setStatus(t("st.added", { label }));
    } else {
      setStatus(t("text.updating"));
      applyDocSnapshot(await invoke<DocSnapshot>("update_freetext", {
        document: document_, id: target.annot.id, args,
      }));
      setStatus(t(text.trim() ? "text.updated" : "text.removed"));
    }
  } catch (e) {
    setStatus(t("text.error", { e: String(e) }));
  }
}

// ---------- Opening the editor ----------

/** New text box with its top-left corner near the given slot position. */
export function openNewTextBox(slot: Slot, pageIdx: number, x: number, y: number) {
  const ppx = pxPerPt(slot, pageIdx);
  // The click lands on the middle of the first line.
  const lift = (BOX_PAD_PT + lastStyle.size * LINE_HEIGHT / 2) * ppx;
  openEditor({
    slot, pageIdx, target: { kind: "new" }, style: lastStyle, text: "",
    x: Math.max(0, x - BOX_PAD_PT * ppx), y: Math.max(0, y - lift),
  });
}

function slotBox(slot: Slot, pageIdx: number, r: RectPt) {
  const sz = state.pageSizes[pageIdx];
  if (!sz) return null;
  return rotateRectToSlot(r, effectiveRotation(pageIdx), sz, slot.img.width, slot.img.height);
}

function openAnnotEditor(slot: Slot, pageIdx: number, a: AnnotationInfo) {
  const box = slotBox(slot, pageIdx, a.rect_pt);
  if (!box) return;
  const ft = a.freetext;
  const style: Style = {
    family: asFamily(ft?.font_family),
    size: ft?.font_size ?? 14,
    bold: ft?.bold ?? false,
    italic: ft?.italic ?? false,
    color: rgbToHex(ft?.text_color ?? [0, 0, 0]),
    bg: ft?.bg_color ? rgbToHex(ft.bg_color) : null,
  };
  openEditor({
    slot, pageIdx, target: { kind: "annot", annot: a }, style, text: a.contents ?? "",
    x: box.left, y: box.top,
    patch: { ...box, color: samplePage(slot, box).bg },
  });
}

function openBlockEditor(slot: Slot, pageIdx: number, b: TextBlock) {
  const box = slotBox(slot, pageIdx, b.rect_pt);
  const origin = slotBox(slot, pageIdx, { x0: b.origin_x, y0: b.origin_y, x1: b.origin_x, y1: b.origin_y });
  if (!box || !origin) return;
  const sample = samplePage(slot, box);
  const style: Style = {
    family: asFamily(b.font_family),
    size: Math.round(b.font_size * 10) / 10,
    bold: b.bold,
    italic: b.italic,
    color: b.invisible ? sample.ink : rgbToHex(b.color),
    bg: null,
    face: b.font_file ? b.font_name ?? null : null,
  };
  if (b.invisible) {
    // OCR text: size the visible replacement to the printed line's length.
    const along = b.dir % 2 === 0 ? b.rect_pt.x1 - b.rect_pt.x0 : b.rect_pt.y1 - b.rect_pt.y0;
    style.size = fitFontSize(b.text, style, along, style.size);
  }
  // Unreadable text (no Unicode mapping, original font not installed): the
  // editor starts empty and the printed line stays visible until typing
  // begins, so it can be read off while retyping.
  openEditor({
    slot, pageIdx, target: { kind: "block", block: b, cover: sample.bg }, style, text: b.text,
    x: origin.left, y: origin.top,
    quadrant: (b.dir + effectiveRotation(pageIdx)) % 4,
    patch: { ...box, color: sample.bg },
    patchWhenTyped: !!b.unreadable,
  });
  if (b.unreadable) setStatus(t("text.unreadable"));
}

// ---------- Click targets on the page ----------

function textToolActive(): boolean {
  return state.shapeTool === "freetext";
}

function editLayer(slot: Slot): HTMLDivElement {
  let layer = slot.wrap.querySelector(":scope > .textEditLayer") as HTMLDivElement | null;
  if (!layer) {
    layer = document.createElement("div");
    layer.className = "textEditLayer";
    slot.wrap.appendChild(layer);
  }
  return layer;
}

async function blocksFor(idx: number): Promise<TextBlock[]> {
  if (blocksGen !== state.contentGen) {
    blocksCache.clear();
    blocksGen = state.contentGen;
  }
  const cached = blocksCache.get(idx);
  if (cached) return cached;
  const doc = state.currentDoc;
  const gen = state.contentGen;
  if (!doc) return [];
  let blocks: TextBlock[] = [];
  try {
    blocks = await invoke<TextBlock[]>("page_text_blocks_cmd", { document: doc, page: idx });
  } catch {
    blocks = [];
  }
  if (doc === state.currentDoc && gen === state.contentGen) blocksCache.set(idx, blocks);
  return blocks;
}

function hotspot(cls: string, box: { left: number; top: number; width: number; height: number }) {
  const hs = document.createElement("div");
  hs.className = cls;
  hs.style.left = `${box.left}px`;
  hs.style.top = `${box.top}px`;
  hs.style.width = `${box.width}px`;
  hs.style.height = `${box.height}px`;
  return hs;
}

/** Rebuild the click targets of one page: every text box (always), and with
 * the Text tool armed every line of page text. */
async function renderTextEditFor(idx: number) {
  const slot = state.slots[idx];
  if (!slot || !slot.loaded) return;
  const layer = editLayer(slot);
  const blocks = textToolActive() ? await blocksFor(idx) : [];
  const gen = state.contentGen;
  // The slot may have been rebuilt or the tool dropped while waiting.
  if (state.slots[idx] !== slot || !slot.loaded) return;
  layer.innerHTML = "";
  if (textToolActive()) {
    for (const b of blocks) {
      const box = slotBox(slot, idx, b.rect_pt);
      if (!box || box.width <= 0 || box.height <= 0) continue;
      const hs = hotspot("textBlockHs", box);
      hs.addEventListener("pointerdown", (ev) => {
        if (ev.button !== 0) return;
        ev.preventDefault();
        ev.stopPropagation();
        // Object indices are only valid for the content they were read from.
        if (gen !== state.contentGen) return;
        openBlockEditor(slot, idx, b);
      });
      layer.appendChild(hs);
    }
  }
  for (const a of state.cachedAnnotations) {
    if (a.subtype !== "FreeText" || a.page !== idx) continue;
    const box = slotBox(slot, idx, a.rect_pt);
    if (!box || box.width <= 0 || box.height <= 0) continue;
    const hs = hotspot("textBoxHs", box);
    hs.title = t("text.clickToEdit");
    hs.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      ev.preventDefault();
      ev.stopPropagation();
      openAnnotEditor(slot, idx, a);
    });
    layer.appendChild(hs);
  }
}

function renderTextEditAll() {
  for (let i = 0; i < state.slots.length; i++) void renderTextEditFor(i);
}

export function initTextEdit() {
  callbacks.renderTextEditFor = (idx) => void renderTextEditFor(idx);
  callbacks.renderTextEditAll = renderTextEditAll;

  // A press outside the open editor finishes it. On the page the press is
  // consumed, so it does not also start the next box or a selection.
  document.addEventListener("pointerdown", (ev) => {
    const ed = active;
    if (!ed) return;
    const target = ev.target as Node | null;
    if (target && (ed.root.contains(target) || ed.bar.contains(target))) return;
    if (target && dom.pageList.contains(target)) {
      ev.preventDefault();
      ev.stopPropagation();
    }
    void finish(false);
  }, true);

  // Text tool: a press on free page area starts a new text box.
  document.addEventListener("pointerdown", (ev) => {
    if (!textToolActive() || !state.currentDoc || ev.button !== 0) return;
    if (isOverlayControl(ev.target)) return;
    const hit = findSlotFromTarget(ev.target);
    if (!hit) return;
    ev.preventDefault();
    const bb = hit.slot.wrap.getBoundingClientRect();
    openNewTextBox(hit.slot, hit.idx, ev.clientX - bb.left, ev.clientY - bb.top);
  });

  // Pixel positions go stale when the page is re-laid out or replaced.
  document.addEventListener("rustpdf:doc-changed", () => {
    closeEditor();
    blocksCache.clear();
  });
  window.addEventListener("resize", () => { if (active) void finish(false); });
  dom.viewer.addEventListener("wheel", (e) => {
    if (active && e.ctrlKey) void finish(false);
  }, { passive: true, capture: true });
}
