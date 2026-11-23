import { t } from "./i18n";
import { refreshIcons } from "./icons";

// ---------- Color chooser ----------
//
// Every color in the app is picked through this one popover: a palette of
// 16 preset colors, the custom colors used last, and a custom picker (a
// saturation / brightness field with a hue slider, plus hex and R/G/B
// fields). `bindColorButton` attaches it to any button;
// `initColorPickers` turns every `<input type="color">` (and
// `<input data-color>`) into such a button, keeping the input as the value
// holder so existing `value` reads and input / change listeners keep working.

/** The preset palette, in display order (two rows of eight). */
export const PALETTE: { hex: string; key: string }[] = [
  { hex: "#000000", key: "black" }, { hex: "#595959", key: "darkGray" },
  { hex: "#a6a6a6", key: "gray" }, { hex: "#ffffff", key: "white" },
  { hex: "#c00000", key: "darkRed" }, { hex: "#ff0000", key: "red" },
  { hex: "#ff8c00", key: "orange" }, { hex: "#ffe600", key: "yellow" },
  { hex: "#92d050", key: "lightGreen" }, { hex: "#00a651", key: "green" },
  { hex: "#00b0f0", key: "lightBlue" }, { hex: "#0070c0", key: "blue" },
  { hex: "#1a3a8f", key: "navy" }, { hex: "#7030a0", key: "purple" },
  { hex: "#ff66cc", key: "pink" }, { hex: "#8b4513", key: "brown" },
];

const RECENT_KEY = "rustpdf-recent-colors";
const CUSTOM_OPEN_KEY = "rustpdf-color-custom-open";
const RECENT_MAX = 8;

export type ColorOptions = {
  /** Current color (#rrggbb), or null for "none". */
  value: string | null;
  /** Offer a "none" entry (transparent background, no fill). */
  allowNone?: boolean;
  /** Label of the "none" entry. */
  noneLabel?: string;
  /** Live updates while the custom picker is dragged. */
  onInput: (hex: string | null) => void;
  /** The final choice (a swatch, "none", or the custom color on close). */
  onChange?: (hex: string | null) => void;
};

// ---------- Color math ----------

export function normalizeHex(s: string): string | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s.trim());
  if (!m) return null;
  let h = m[1].toLowerCase();
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  return `#${h}`;
}

function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.slice(1), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

function rgbToHex(r: number, g: number, b: number): string {
  const h = (v: number) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

/** RGB 0..255 → hue 0..360, saturation and value 0..1. */
function rgbToHsv(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return [h, max === 0 ? 0 : d / max, max];
}

function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [f(5) * 255, f(3) * 255, f(1) * 255];
}

// ---------- Recent custom colors ----------

function loadRecent(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(v) ? v.map((x) => normalizeHex(String(x))).filter((x): x is string => !!x) : [];
  } catch {
    return [];
  }
}

function pushRecent(hex: string) {
  if (PALETTE.some((p) => p.hex === hex)) return;
  const list = [hex, ...loadRecent().filter((c) => c !== hex)].slice(0, RECENT_MAX);
  try { localStorage.setItem(RECENT_KEY, JSON.stringify(list)); } catch { /* storage unavailable */ }
}

// ---------- Popover ----------

type Session = {
  anchor: HTMLElement;
  opts: ColorOptions;
  original: string | null;
  /** Custom color shown in the picker (h, s, v). */
  hsv: [number, number, number];
  /** The custom picker changed the color since the popover opened. */
  customDirty: boolean;
};

let pop: HTMLDivElement | null = null;
let session: Session | null = null;

function swatchButton(hex: string, title: string, cls = "cpSwatch"): string {
  return `<button type="button" class="${cls}" data-hex="${hex}" title="${title}" aria-label="${title}" style="--c:${hex}"></button>`;
}

function buildPopover(): HTMLDivElement {
  const el = document.createElement("div");
  el.className = "colorPop hidden";
  el.setAttribute("role", "dialog");
  el.innerHTML = `
    <div class="cpGrid">${PALETTE.map((p) => swatchButton(p.hex, `${t(`color.${p.key}`)} (${p.hex})`)).join("")}</div>
    <button type="button" class="cpNone"><span class="cpSwatch none"></span><span class="cpNoneLabel"></span></button>
    <div class="cpRecent"><div class="cpLabel">${t("cp.recent")}</div><div class="cpRecentRow"></div></div>
    <button type="button" class="cpMoreToggle" aria-expanded="false"><i data-lucide="chevron-right"></i><span>${t("cp.custom")}</span></button>
    <div class="cpCustom hidden">
      <div class="cpSV" tabindex="0" aria-label="${t("cp.sv")}"><div class="cpSVThumb"></div></div>
      <input class="cpHue" type="range" min="0" max="360" step="1" aria-label="${t("cp.hue")}" />
      <div class="cpFields">
        <span class="cpPreview"></span>
        <label class="cpHexLabel">#<input class="cpHex" type="text" maxlength="7" spellcheck="false" aria-label="Hex" /></label>
        <label>R<input class="cpR" type="number" min="0" max="255" step="1" /></label>
        <label>G<input class="cpG" type="number" min="0" max="255" step="1" /></label>
        <label>B<input class="cpB" type="number" min="0" max="255" step="1" /></label>
      </div>
      <div class="cpActions"><button type="button" class="cpOk primaryBtn">${t("cp.ok")}</button></div>
    </div>`;
  document.body.appendChild(el);
  refreshIcons();

  // Buttons keep the focus where it was (e.g. the caret in a text box).
  el.addEventListener("mousedown", (e) => {
    if ((e.target as Element).closest("button")) e.preventDefault();
  });
  // Keys typed into the fields stay here (no app shortcuts).
  el.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); closeColorPopover(true); }
    else if (e.key === "Enter" && (e.target as Element).matches("input")) {
      e.preventDefault();
      closeColorPopover();
    }
  });
  el.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const sw = target.closest<HTMLElement>(".cpSwatch[data-hex]");
    if (sw && session) { pick(sw.dataset.hex!); return; }
    if (target.closest(".cpNone") && session) { pick(null); return; }
    if (target.closest(".cpMoreToggle")) { setCustomOpen(el.querySelector(".cpCustom")!.classList.contains("hidden")); return; }
    if (target.closest(".cpOk")) closeColorPopover();
  });

  // Saturation / brightness field.
  const sv = el.querySelector<HTMLDivElement>(".cpSV")!;
  const svFromEvent = (e: PointerEvent) => {
    if (!session) return;
    const r = sv.getBoundingClientRect();
    const s = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
    const v = 1 - Math.max(0, Math.min(1, (e.clientY - r.top) / r.height));
    setCustom([session.hsv[0], s, v]);
  };
  sv.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    sv.setPointerCapture(e.pointerId);
    svFromEvent(e);
    const move = (ev: PointerEvent) => svFromEvent(ev);
    const up = () => { sv.removeEventListener("pointermove", move); sv.removeEventListener("pointerup", up); };
    sv.addEventListener("pointermove", move);
    sv.addEventListener("pointerup", up);
  });
  sv.addEventListener("keydown", (e) => {
    if (!session) return;
    const [h, s, v] = session.hsv;
    const step = e.shiftKey ? 0.1 : 0.02;
    const moves: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step],
    };
    const m = moves[e.key];
    if (!m) return;
    e.preventDefault();
    setCustom([h, Math.max(0, Math.min(1, s + m[0])), Math.max(0, Math.min(1, v + m[1]))]);
  });
  el.querySelector<HTMLInputElement>(".cpHue")!.addEventListener("input", (e) => {
    if (!session) return;
    setCustom([parseFloat((e.target as HTMLInputElement).value), session.hsv[1], session.hsv[2]]);
  });
  el.querySelector<HTMLInputElement>(".cpHex")!.addEventListener("input", (e) => {
    const hex = normalizeHex((e.target as HTMLInputElement).value);
    if (hex && hex.length === 7) setCustom(rgbToHsv(...hexToRgb(hex)), "hex");
  });
  for (const cls of ["cpR", "cpG", "cpB"]) {
    el.querySelector<HTMLInputElement>(`.${cls}`)!.addEventListener("input", () => {
      const ch = (c: string) => {
        const v = parseInt(el.querySelector<HTMLInputElement>(`.${c}`)!.value, 10);
        return Number.isFinite(v) ? Math.max(0, Math.min(255, v)) : 0;
      };
      setCustom(rgbToHsv(ch("cpR"), ch("cpG"), ch("cpB")), "rgb");
    });
  }
  return el;
}

function setCustomOpen(open: boolean) {
  if (!pop) return;
  pop.querySelector(".cpCustom")!.classList.toggle("hidden", !open);
  const toggle = pop.querySelector<HTMLElement>(".cpMoreToggle")!;
  toggle.setAttribute("aria-expanded", String(open));
  toggle.classList.toggle("open", open);
  try { localStorage.setItem(CUSTOM_OPEN_KEY, open ? "1" : "0"); } catch { /* storage unavailable */ }
  if (session) place(session.anchor);
}

/** Show `hsv` in the custom picker; `from` names the field being typed in,
 * which is left alone. */
function renderCustom(hsv: [number, number, number], from?: "hex" | "rgb") {
  if (!pop) return;
  const [h, s, v] = hsv;
  const [r, g, b] = hsvToRgb(h, s, v);
  const hex = rgbToHex(r, g, b);
  const sv = pop.querySelector<HTMLDivElement>(".cpSV")!;
  sv.style.setProperty("--hue", `hsl(${h}, 100%, 50%)`);
  const thumb = pop.querySelector<HTMLDivElement>(".cpSVThumb")!;
  thumb.style.left = `${s * 100}%`;
  thumb.style.top = `${(1 - v) * 100}%`;
  thumb.style.background = hex;
  pop.querySelector<HTMLInputElement>(".cpHue")!.value = String(Math.round(h));
  pop.querySelector<HTMLElement>(".cpPreview")!.style.background = hex;
  if (from !== "hex") pop.querySelector<HTMLInputElement>(".cpHex")!.value = hex.slice(1);
  if (from !== "rgb") {
    pop.querySelector<HTMLInputElement>(".cpR")!.value = String(Math.round(r));
    pop.querySelector<HTMLInputElement>(".cpG")!.value = String(Math.round(g));
    pop.querySelector<HTMLInputElement>(".cpB")!.value = String(Math.round(b));
  }
}

function setCustom(hsv: [number, number, number], from?: "hex" | "rgb") {
  if (!session) return;
  session.hsv = hsv;
  session.customDirty = true;
  renderCustom(hsv, from);
  const hex = rgbToHex(...hsvToRgb(...hsv));
  markCurrent(hex);
  session.opts.onInput(hex);
}

function markCurrent(value: string | null) {
  if (!pop) return;
  pop.querySelectorAll<HTMLElement>(".cpSwatch[data-hex]").forEach((b) => {
    b.classList.toggle("current", b.dataset.hex === value);
  });
  pop.querySelector(".cpNone")!.classList.toggle("current", value === null);
}

function pick(value: string | null) {
  const s = session;
  if (!s) return;
  session = null;
  s.opts.onInput(value);
  s.opts.onChange?.(value);
  hide();
}

function hide() {
  if (!pop) return;
  pop.classList.add("hidden");
  document.dispatchEvent(new CustomEvent("rustpdf:colorpop-closed"));
}

/** Close the popover. A color dialed in with the custom picker counts as
 * chosen unless `revert` (Escape) restores the color it opened with. */
export function closeColorPopover(revert = false) {
  const s = session;
  session = null;
  if (s?.customDirty) {
    if (revert) {
      s.opts.onInput(s.original);
    } else {
      const hex = rgbToHex(...hsvToRgb(...s.hsv));
      pushRecent(hex);
      s.opts.onChange?.(hex);
    }
  }
  hide();
}

/** Whether the popover is open for a control inside `root`. */
export function isColorPopoverOpenIn(root: Element): boolean {
  return !!session && root.contains(session.anchor);
}

function place(anchor: HTMLElement) {
  if (!pop) return;
  const a = anchor.getBoundingClientRect();
  const w = pop.offsetWidth, h = pop.offsetHeight;
  const margin = 6;
  let left = Math.min(a.left, window.innerWidth - w - margin);
  left = Math.max(margin, left);
  let top = a.bottom + 4;
  if (top + h > window.innerHeight - margin && a.top - h - 4 >= margin) top = a.top - h - 4;
  top = Math.max(margin, Math.min(top, window.innerHeight - h - margin));
  pop.style.left = `${left}px`;
  pop.style.top = `${top}px`;
}

/** Open the popover under `anchor`; `focus` moves the keyboard focus into
 * it (when it was opened from the keyboard). */
export function openColorPopover(anchor: HTMLElement, opts: ColorOptions, focus = false) {
  if (session?.anchor === anchor) { closeColorPopover(); return; }
  if (session) closeColorPopover();
  pop ??= buildPopover();
  const value = opts.value ? normalizeHex(opts.value) : null;
  const start = value ?? "#000000";
  session = { anchor, opts, original: value, hsv: rgbToHsv(...hexToRgb(start)), customDirty: false };

  const none = pop.querySelector<HTMLElement>(".cpNone")!;
  none.classList.toggle("hidden", !opts.allowNone);
  none.querySelector(".cpNoneLabel")!.textContent = opts.noneLabel ?? t("cp.none");
  const recent = loadRecent();
  pop.querySelector(".cpRecent")!.classList.toggle("hidden", recent.length === 0);
  pop.querySelector(".cpRecentRow")!.innerHTML = recent.map((c) => swatchButton(c, c)).join("");
  let customOpen = false;
  try { customOpen = localStorage.getItem(CUSTOM_OPEN_KEY) === "1"; } catch { /* storage unavailable */ }
  pop.classList.remove("hidden");
  setCustomOpen(customOpen);
  renderCustom(session.hsv);
  markCurrent(value);
  place(anchor);
  if (focus) pop.querySelector<HTMLElement>(".cpSwatch.current, .cpSwatch")?.focus({ preventScroll: true });
}

// ---------- Bindings ----------

export type ColorButtonOptions = {
  get: () => string | null;
  /** `final` is false for live updates from the custom picker. */
  set: (hex: string | null, final: boolean) => void;
  allowNone?: boolean;
  noneLabel?: string;
};

/** Open the popover from `btn`. */
export function bindColorButton(btn: HTMLElement, o: ColorButtonOptions) {
  btn.setAttribute("aria-haspopup", "dialog");
  btn.addEventListener("mousedown", (e) => e.preventDefault());
  btn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openColorPopover(btn, {
      value: o.get(),
      allowNone: o.allowNone,
      noneLabel: o.noneLabel,
      onInput: (hex) => o.set(hex, false),
      onChange: (hex) => o.set(hex, true),
    }, e.detail === 0);
  });
}

/** Paint a color (or "none") into a swatch element. */
export function paintSwatch(el: HTMLElement, value: string | null) {
  el.style.background = value ?? "";
  el.classList.toggle("none", value === null);
}

/** Replace `input` (type=color, or any input with `data-color`) by a swatch
 * button. The input stays, hidden, as the value holder: picks set its value
 * ("" = none) and fire `input` / `change` on it. */
export function enhanceColorInput(input: HTMLInputElement): HTMLButtonElement {
  const existing = input.previousElementSibling;
  if (input.dataset.cpBound && existing instanceof HTMLButtonElement) return existing;
  const allowNone = input.dataset.colorNone !== undefined;
  const initial = normalizeHex(input.value) ?? (allowNone ? "" : "#000000");
  input.type = "hidden";
  input.value = initial;
  input.dataset.cpBound = "1";
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "colorBtn";
  if (input.id) btn.id = `${input.id}Btn`;
  btn.title = input.closest("[title]")?.getAttribute("title") ?? "";
  btn.innerHTML = `<span class="colorBtnSwatch"></span><i data-lucide="chevron-down"></i>`;
  input.before(btn);
  const swatch = btn.querySelector<HTMLElement>(".colorBtnSwatch")!;
  const sync = () => paintSwatch(swatch, input.value || null);
  sync();
  input.addEventListener("input", sync);
  bindColorButton(btn, {
    get: () => input.value || null,
    set: (hex, final) => {
      input.value = hex ?? "";
      input.dispatchEvent(new Event("input", { bubbles: true }));
      if (final) input.dispatchEvent(new Event("change", { bubbles: true }));
    },
    allowNone,
    noneLabel: input.dataset.colorNone || undefined,
  });
  return btn;
}

/** Enhance every color input under `root` (idempotent). */
export function initColorPickers(root: ParentNode = document) {
  root.querySelectorAll<HTMLInputElement>('input[type="color"], input[data-color]').forEach((i) => {
    if (!i.dataset.cpBound) enhanceColorInput(i);
  });
  refreshIcons();
}

export function initColorPopover() {
  initColorPickers();
  // A press outside closes it; one on its own trigger toggles (click).
  document.addEventListener("pointerdown", (e) => {
    if (!session || !pop) return;
    const target = e.target as Node;
    if (pop.contains(target) || session.anchor.contains(target)) return;
    closeColorPopover();
  }, true);
  // Escape closes the popover first, wherever the focus is (e.g. still in
  // the text box being formatted), and goes no further.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !session) return;
    e.preventDefault();
    e.stopPropagation();
    closeColorPopover(true);
  }, true);
  window.addEventListener("resize", () => { if (session) closeColorPopover(); });
}
