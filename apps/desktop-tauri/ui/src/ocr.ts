// v1.0 OCR & scans: Tesseract-backed text recognition on rendered pages,
// injected as an invisible, searchable text layer. Includes language-pack
// download, batch OCR over files, and the "scanned document?" banner.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { DocSnapshot } from "./types";
import { state } from "./state";
import { t } from "./i18n";
import { setStatus } from "./dom";
import { applyDocSnapshot } from "./doc";

type TesseractInfo = {
  available: boolean;
  path: string | null;
  version: string | null;
  languages: string[];
};

type OcrRunResult = {
  snapshot: DocSnapshot;
  pages_done: number;
  pages_skipped: number;
  words: number;
};

type OcrBatchResult = { files_done: number; files_skipped: number; words: number };

type OcrProgress = { current: number; total: number; page: number; file: string | null };

/** Curated language choices; anything Tesseract reports gets appended. */
const KNOWN_LANGS: [string, string][] = [
  ["deu", "Deutsch"],
  ["eng", "English"],
  ["deu+eng", "Deutsch + English"],
  ["fra", "Français"],
  ["ita", "Italiano"],
  ["spa", "Español"],
  ["por", "Português"],
  ["nld", "Nederlands"],
  ["pol", "Polski"],
  ["ces", "Čeština"],
  ["dan", "Dansk"],
  ["swe", "Svenska"],
  ["nor", "Norsk"],
  ["fin", "Suomi"],
  ["hun", "Magyar"],
  ["ron", "Română"],
  ["tur", "Türkçe"],
  ["ell", "Ελληνικά"],
  ["rus", "Русский"],
  ["ukr", "Українська"],
];

let tessInfo: TesseractInfo | null = null;
let bannerDismissed = false;

function el<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

function ocrError(e: unknown): string {
  const s = String(e);
  if (s.includes("TESSERACT_MISSING")) return t("ocr.tessMissing");
  if (s.includes("OCR_LANG_MISSING")) {
    const lang = s.split("OCR_LANG_MISSING:")[1] ?? "?";
    return t("ocr.langMissing", { lang });
  }
  if (s.includes("OCR_DOWNLOAD_FAILED")) return t("ocr.downloadFailed");
  if (s.includes("OCR_NO_TEXT_FOUND")) return t("ocr.noText");
  return `${t("ocr.error")}: ${s}`;
}

// ---------- Language handling ----------

async function refreshTesseract(): Promise<TesseractInfo> {
  tessInfo = await invoke<TesseractInfo>("check_tesseract");
  return tessInfo;
}

function langInstalled(info: TesseractInfo, lang: string): boolean {
  return lang.split("+").every((l) => info.languages.includes(l));
}

async function populateLangSelect(info: TesseractInfo) {
  const sel = el<HTMLSelectElement>("ocrLang");
  const saved = await invoke<string>("get_ocr_language").catch(() => "deu");
  sel.innerHTML = "";
  const seen = new Set<string>();
  for (const [code, label] of KNOWN_LANGS) {
    seen.add(code);
    const opt = document.createElement("option");
    opt.value = code;
    opt.textContent = langInstalled(info, code) ? label : `${label} ⬇`;
    sel.appendChild(opt);
  }
  for (const code of info.languages) {
    if (seen.has(code)) continue;
    const opt = document.createElement("option");
    opt.value = code;
    opt.textContent = code;
    sel.appendChild(opt);
  }
  sel.value = saved;
  if (sel.selectedIndex < 0) sel.value = "deu";
}

/** Make sure every component of `lang` is installed, downloading if needed. */
async function ensureLanguage(lang: string): Promise<void> {
  const info = tessInfo ?? (await refreshTesseract());
  for (const part of lang.split("+")) {
    if (info.languages.includes(part)) continue;
    setStatus(t("ocr.downloading", { lang: part }));
    await invoke("download_ocr_language", { lang: part });
  }
  await refreshTesseract();
}

// ---------- OCR dialog ----------

async function openOcrDialog() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const info = await refreshTesseract();
  if (!info.available) {
    setStatus(t("ocr.tessMissing"));
    return;
  }
  await populateLangSelect(info);
  el<HTMLInputElement>("ocrFrom").value = "1";
  el<HTMLInputElement>("ocrTo").value = String(state.pageCount);
  syncOcrRange();
  el("ocrOverlay").classList.remove("hidden");
}

function closeOcrDialog() {
  el("ocrOverlay").classList.add("hidden");
}

function syncOcrRange() {
  const scope = el<HTMLSelectElement>("ocrScope").value;
  el("ocrRange").style.display = scope === "range" ? "flex" : "none";
}

function selectedPages(): number[] | null {
  const scope = el<HTMLSelectElement>("ocrScope").value;
  if (scope === "current") return [state.currentPage];
  if (scope === "all") return [];
  const from = parseInt(el<HTMLInputElement>("ocrFrom").value, 10);
  const to = parseInt(el<HTMLInputElement>("ocrTo").value, 10);
  if (isNaN(from) || isNaN(to) || from < 1 || to > state.pageCount || from > to) return null;
  return Array.from({ length: to - from + 1 }, (_, i) => from - 1 + i);
}

async function commitOcr() {
  if (!state.currentDoc) return;
  const pages = selectedPages();
  if (!pages) {
    setStatus(t("export.badRange"));
    return;
  }
  const lang = el<HTMLSelectElement>("ocrLang").value;
  const preprocess = el<HTMLInputElement>("ocrPreprocess").checked;
  const skipText = el<HTMLInputElement>("ocrSkipText").checked;
  closeOcrDialog();
  hideBanner();
  void invoke("set_ocr_language", { lang }).catch(() => {});
  setStatus(t("ocr.working"));
  try {
    await ensureLanguage(lang);
    const res = await invoke<OcrRunResult>("ocr_document", {
      document: state.currentDoc,
      args: { pages, lang, preprocess, skip_text_pages: skipText },
    });
    applyDocSnapshot(res.snapshot);
    const skipped =
      res.pages_skipped > 0 ? t("ocr.doneSkipped", { n: res.pages_skipped }) : "";
    setStatus(t("ocr.done", { pages: res.pages_done, words: res.words, skipped }));
  } catch (e) {
    setStatus(ocrError(e));
  }
}

// ---------- Batch OCR ----------

async function batchOcr() {
  const info = await refreshTesseract();
  if (!info.available) {
    setStatus(t("ocr.tessMissing"));
    return;
  }
  const sel = await open({ multiple: true, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!sel) return;
  const paths = Array.isArray(sel) ? sel : [sel];
  if (paths.length === 0) return;
  const outDir = await open({ directory: true });
  if (!outDir || typeof outDir !== "string") return;
  const lang = await invoke<string>("get_ocr_language").catch(() => "deu");
  setStatus(t("ocr.working"));
  try {
    await ensureLanguage(lang);
    const res = await invoke<OcrBatchResult>("ocr_files_batch", {
      args: { paths, out_dir: outDir, lang, preprocess: true },
    });
    setStatus(
      t("ocr.batchDone", {
        done: res.files_done,
        skipped: res.files_skipped,
        words: res.words,
      }),
    );
  } catch (e) {
    setStatus(ocrError(e));
  }
}

// ---------- Scanned-document banner ----------

function hideBanner() {
  el("ocrBanner").classList.add("hidden");
}

async function maybeShowBanner() {
  bannerDismissed = false;
  hideBanner();
  const doc = state.currentDoc;
  if (!doc) return;
  try {
    const needs = await invoke<boolean>("detect_needs_ocr", { document: doc });
    // The document may have changed while we were checking.
    if (needs && !bannerDismissed && state.currentDoc === doc) {
      el("ocrBanner").classList.remove("hidden");
    }
  } catch {
    /* detection is best-effort */
  }
}

// ---------- Init ----------

export function initOcr() {
  el("ocrBtn").addEventListener("click", () => void openOcrDialog());
  el("ocrBatchBtn").addEventListener("click", () => void batchOcr());
  // OCR ("Schnellerkennung") also offered on the Edit tab.
  document.getElementById("editOcrBtn")?.addEventListener("click", () => void openOcrDialog());
  el("ocrDlgClose").addEventListener("click", closeOcrDialog);
  el("ocrCancel").addEventListener("click", closeOcrDialog);
  el("ocrCommit").addEventListener("click", () => void commitOcr());
  el("ocrScope").addEventListener("change", syncOcrRange);
  el("ocrOverlay").addEventListener("click", (e) => {
    if (e.target === el("ocrOverlay")) closeOcrDialog();
  });
  el("ocrBannerStart").addEventListener("click", () => {
    hideBanner();
    void openOcrDialog();
  });
  el("ocrBannerClose").addEventListener("click", () => {
    bannerDismissed = true;
    hideBanner();
  });
  document.addEventListener("docura:doc-opened", () => void maybeShowBanner());
  void listen<OcrProgress>("ocr-progress", (ev) => {
    const p = ev.payload;
    setStatus(
      p.file
        ? t("ocr.filePage", { file: p.file, page: p.page, current: p.current, total: p.total })
        : t("ocr.page", { page: p.page, current: p.current, total: p.total }),
    );
  });
}
