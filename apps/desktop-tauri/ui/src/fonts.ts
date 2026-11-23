import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

// ---------- Fonts for text boxes and page text ----------
//
// Three PDF standard fonts need no file; the bundled open-licensed families
// (crates/pdf_fonts) come from the backend, which also hands out their
// TrueType bytes so the editor previews with exactly the font that gets
// embedded into the PDF.

export type FontCategory = "sans" | "serif" | "mono" | "handwriting";

export type FontInfo = {
  /** Id stored in documents: "Helv" | "Times" | "Cour" | a bundled id. */
  id: string;
  name: string;
  category: FontCategory;
  /** Real bold / italic faces available. */
  bold: boolean;
  italic: boolean;
  standard?: boolean;
};

const STANDARD: FontInfo[] = [
  { id: "Helv", name: "Helvetica", category: "sans", bold: true, italic: true, standard: true },
  { id: "Times", name: "Times", category: "serif", bold: true, italic: true, standard: true },
  { id: "Cour", name: "Courier", category: "mono", bold: true, italic: true, standard: true },
];

const STANDARD_CSS: Record<string, string> = {
  Helv: "Arial, Helvetica, sans-serif",
  Times: '"Times New Roman", Times, serif',
  Cour: '"Courier New", Courier, monospace',
};

const FALLBACK_CSS: Record<FontCategory, string> = {
  sans: "Arial, sans-serif",
  serif: '"Times New Roman", serif',
  mono: '"Courier New", monospace',
  handwriting: "cursive",
};

let bundled: FontInfo[] = [];
const loading = new Map<string, Promise<boolean>>();
const ready = new Set<string>();

/** The face key a style resolves to (missing bold / italic faces fall back,
 * as in the backend). */
function faceKey(info: FontInfo, bold: boolean, italic: boolean): [string, boolean, boolean] {
  const b = bold && info.bold, i = italic && info.italic;
  return [`${info.id}/${b}/${i}`, b, i];
}

/** Whether text in this style can be laid out with the real font now. */
export function faceReady(id: string, bold: boolean, italic: boolean): boolean {
  const info = fontInfo(id);
  return !info || !!info.standard || ready.has(faceKey(info, bold, italic)[0]);
}

/** Fetch the bundled font list once at start-up. */
export async function initFonts(): Promise<void> {
  try {
    bundled = await invoke<FontInfo[]>("list_fonts");
  } catch {
    bundled = [];
  }
  document.dispatchEvent(new CustomEvent("rustpdf:fonts-ready"));
}

export function allFonts(): FontInfo[] {
  return [...STANDARD, ...bundled];
}

export function fontInfo(id: string): FontInfo | undefined {
  return allFonts().find((f) => f.id === id);
}

export function isKnownFamily(id: string | undefined | null): id is string {
  return !!id && !!fontInfo(id);
}

function faceFamily(id: string): string {
  return `rp-${id}`;
}

/** CSS font-family list for a family id. */
export function cssFamily(id: string): string {
  const std = STANDARD_CSS[id];
  if (std) return std;
  const info = fontInfo(id);
  return `"${faceFamily(id)}", ${FALLBACK_CSS[info?.category ?? "sans"]}`;
}

/** Load the face of a bundled family into the document (no-op for the
 * standard fonts). Resolves true once the face can be used. */
export function ensureFace(id: string, bold: boolean, italic: boolean): Promise<boolean> {
  const info = fontInfo(id);
  if (!info || info.standard) return Promise.resolve(true);
  const [key, b, i] = faceKey(info, bold, italic);
  let p = loading.get(key);
  if (!p) {
    p = invoke<ArrayBuffer>("font_data", { family: id, bold: b, italic: i })
      .then(async (data) => {
        const face = new FontFace(faceFamily(id), data, {
          weight: b ? "700" : "400",
          style: i ? "italic" : "normal",
        });
        await face.load();
        document.fonts.add(face);
        ready.add(key);
        return true;
      })
      .catch(() => {
        loading.delete(key);
        return false;
      });
    loading.set(key, p);
  }
  return p;
}

const CATEGORY_LABEL: Record<FontCategory, string> = {
  sans: "font.sans",
  serif: "font.serif",
  mono: "font.mono",
  handwriting: "font.handwriting",
};

/** `<option>`s for a family picker: the standard fonts, then the bundled
 * families grouped by look. */
export function familyOptionsHtml(): string {
  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
  const opt = (f: FontInfo) => `<option value="${esc(f.id)}">${esc(f.name)}</option>`;
  let html = `<optgroup label="${esc(t("font.standard"))}">${STANDARD.map(opt).join("")}</optgroup>`;
  for (const cat of ["sans", "serif", "mono", "handwriting"] as FontCategory[]) {
    const list = bundled.filter((f) => f.category === cat);
    if (list.length) html += `<optgroup label="${esc(t(CATEGORY_LABEL[cat]))}">${list.map(opt).join("")}</optgroup>`;
  }
  return html;
}
