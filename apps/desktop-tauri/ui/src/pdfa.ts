// PDF/A export helpers: level lists, converter choice, result/error texts.

import { getLang, t } from "./i18n";
import { state } from "./state";

/** Every level the built-in converter writes, in menu order. */
export const PDFA_LEVELS = ["1a", "1b", "2a", "2b", "2u", "3a", "3b", "3u", "4", "4e", "4f"] as const;
/** Levels the previous Ghostscript path can produce. */
export const GS_LEVELS = ["1b", "2b", "3b"];

export type PdfaEngineId = "builtin" | "ghostscript";

export type PdfaNote = { code: string; detail: string };
export type PdfaSaved = { label: string; engine: PdfaEngineId; notes: PdfaNote[] };

export function isBuiltin(): boolean {
  return state.pdfaEngine !== "ghostscript";
}

/** <option> list for a level <select>; `off` adds the "plain PDF" entry. */
export function levelOptionsHtml(selected: string, off: boolean): string {
  const opts: string[] = [];
  if (off) opts.push(`<option value="off"${selected === "off" ? " selected" : ""}>${t("pdfa.off")}</option>`);
  for (const l of PDFA_LEVELS) {
    opts.push(`<option value="${l}"${selected === l ? " selected" : ""}>${t(`pdfa.lvl.${l}`)}</option>`);
  }
  return opts.join("");
}

/** Disable levels the selected converter cannot write. */
export function syncLevelSelect(sel: HTMLSelectElement | null | undefined) {
  if (!sel) return;
  const gs = !isBuiltin();
  for (const o of Array.from(sel.options)) {
    if (o.value === "off") continue;
    o.disabled = gs && !GS_LEVELS.includes(o.value);
  }
  if (sel.selectedOptions[0]?.disabled) sel.value = sel.options[0].value === "off" ? "off" : "2b";
}

/** Document language for level a, from the UI language. */
export function pdfaLang(): string {
  return getLang() === "de" ? "de-DE" : "en-US";
}

/** Translate `PDFA_ERR:<code>:<detail>` errors from the backend. */
export function pdfaErrorText(e: unknown): string {
  const s = String(e);
  const m = /^PDFA_ERR:([a-z_]+):([\s\S]*)$/.exec(s);
  if (!m) return s;
  const key = `pdfa.err.${m[1]}`;
  const text = t(key, { detail: m[2] });
  return text === key ? m[2] : text;
}

/** Notes worth telling the user (the rest is routine). */
const SHOWN_NOTES = [
  "signatures_removed", "javascript_removed", "private_data_removed", "attachments_removed", "attachments_converted",
  "pages_rendered", "pages_scaled", "annotations_removed", "layers_flattened", "xfa_removed", "postscript_removed",
];

export function pdfaNotesText(notes: PdfaNote[]): string {
  return notes
    .filter((n) => SHOWN_NOTES.includes(n.code))
    .map((n) => t(`pdfa.note.${n.code}`, { detail: n.detail }))
    .join(" · ");
}
