// v0.9 export & conversion: PDF → PNG/JPG (per page), PDF → plain text,
// PDF → simple flowed HTML, and images → PDF (one page per image, opened
// like a merge result). Lives in the Convert tab.

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { OpenedDocument } from "./types";
import { state } from "./state";
import { t } from "./i18n";
import { setStatus } from "./dom";
import { applyOpenedDoc, confirmDiscardChanges } from "./doc";

/** Filename stem of the open document, e.g. "report" for ".../report.pdf". */
function docBase(): string {
  const p = state.currentDocPath;
  const name = p ? (p.split(/[\\/]/).pop() ?? "document") : "document";
  return name.replace(/\.pdf$/i, "");
}

// ---------- PDF → PNG/JPG ----------

function openExportImagesDialog() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  (document.getElementById("expImgFrom") as HTMLInputElement).value = "1";
  (document.getElementById("expImgTo") as HTMLInputElement).value = String(state.pageCount);
  syncRangeVisibility();
  document.getElementById("exportImgOverlay")!.classList.remove("hidden");
}

function closeExportImagesDialog() {
  document.getElementById("exportImgOverlay")!.classList.add("hidden");
}

function syncRangeVisibility() {
  const scope = (document.getElementById("expImgScope") as HTMLSelectElement).value;
  (document.getElementById("expImgRange") as HTMLElement).style.display =
    scope === "range" ? "flex" : "none";
}

function selectedPages(): number[] | null {
  const scope = (document.getElementById("expImgScope") as HTMLSelectElement).value;
  if (scope === "current") return [state.currentPage];
  if (scope === "all") return Array.from({ length: state.pageCount }, (_, i) => i);
  const from = parseInt((document.getElementById("expImgFrom") as HTMLInputElement).value, 10);
  const to = parseInt((document.getElementById("expImgTo") as HTMLInputElement).value, 10);
  if (isNaN(from) || isNaN(to) || from < 1 || to > state.pageCount || from > to) return null;
  return Array.from({ length: to - from + 1 }, (_, i) => from - 1 + i);
}

async function commitExportImages() {
  if (!state.currentDoc) return;
  const pages = selectedPages();
  if (!pages) {
    setStatus(t("export.badRange"));
    return;
  }
  const format = (document.getElementById("expImgFormat") as HTMLSelectElement).value;
  const dpi = parseInt((document.getElementById("expImgDpi") as HTMLSelectElement).value, 10);
  const dir = await open({ directory: true });
  if (!dir || typeof dir !== "string") return;
  closeExportImagesDialog();
  setStatus(t("export.working"));
  try {
    const n = await invoke<number>("export_pages_as_images", {
      document: state.currentDoc,
      args: { format, dpi, pages, dir, base: docBase() },
    });
    setStatus(t("export.imagesDone", { n }));
  } catch (e) {
    setStatus(`${t("export.error")}: ${e}`);
  }
}

// ---------- PDF → Text / HTML ----------

async function exportText() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const dest = await save({
    defaultPath: `${docBase()}.txt`,
    filters: [{ name: t("btn.expText"), extensions: ["txt"] }],
  });
  if (!dest || typeof dest !== "string") return;
  setStatus(t("export.working"));
  try {
    const n = await invoke<number>("export_text_file", {
      document: state.currentDoc,
      path: dest,
    });
    setStatus(t("export.textDone", { n }));
  } catch (e) {
    setStatus(`${t("export.error")}: ${e}`);
  }
}

async function exportHtml() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const dest = await save({
    defaultPath: `${docBase()}.html`,
    filters: [{ name: "HTML", extensions: ["html"] }],
  });
  if (!dest || typeof dest !== "string") return;
  setStatus(t("export.working"));
  try {
    const n = await invoke<number>("export_html_file", {
      document: state.currentDoc,
      path: dest,
      title: docBase(),
    });
    setStatus(t("export.htmlDone", { n }));
  } catch (e) {
    setStatus(`${t("export.error")}: ${e}`);
  }
}

// ---------- PDF → Word / Excel ----------

async function exportDocx() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const dest = await save({
    defaultPath: `${docBase()}.docx`,
    filters: [{ name: "Word", extensions: ["docx"] }],
  });
  if (!dest || typeof dest !== "string") return;
  setStatus(t("export.working"));
  try {
    const n = await invoke<number>("export_docx_file", {
      document: state.currentDoc,
      path: dest,
    });
    setStatus(t("export.docxDone", { n }));
  } catch (e) {
    setStatus(`${t("export.error")}: ${e}`);
  }
}

async function exportXlsx() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const dest = await save({
    defaultPath: `${docBase()}.xlsx`,
    filters: [{ name: "Excel", extensions: ["xlsx"] }],
  });
  if (!dest || typeof dest !== "string") return;
  setStatus(t("export.working"));
  try {
    const n = await invoke<number>("export_xlsx_file", {
      document: state.currentDoc,
      path: dest,
    });
    setStatus(t("export.xlsxDone", { n }));
  } catch (e) {
    setStatus(`${t("export.error")}: ${e}`);
  }
}

// ---------- Office → PDF ----------

async function officeToPdf() {
  const sel = await open({
    multiple: false,
    filters: [{
      name: "Office",
      extensions: ["doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt"],
    }],
  });
  if (!sel || typeof sel !== "string") return;
  if (!(await confirmDiscardChanges())) return;
  setStatus(t("export.officeWorking"));
  try {
    type Res = OpenedDocument & { temp_path: string };
    const res = await invoke<Res>("office_to_pdf_and_open", { path: sel });
    applyOpenedDoc(res, res.temp_path, { temp: true });
    setStatus(t("export.officeDone"));
  } catch (e) {
    if (String(e).includes("LIBREOFFICE_MISSING")) setStatus(t("export.loMissing"));
    else setStatus(`${t("export.error")}: ${e}`);
  }
}

// ---------- Images → PDF ----------

async function imagesToPdf() {
  const sel = await open({
    multiple: true,
    filters: [{ name: t("lbl.images"), extensions: ["png", "jpg", "jpeg", "bmp", "webp"] }],
  });
  if (!sel) return;
  const paths = Array.isArray(sel) ? sel : [sel];
  if (paths.length === 0) return;
  if (!(await confirmDiscardChanges())) return;
  setStatus(t("export.working"));
  try {
    type Res = OpenedDocument & { temp_path: string };
    const res = await invoke<Res>("images_to_pdf_and_open", { paths });
    applyOpenedDoc(res, res.temp_path, { temp: true });
    setStatus(t("export.imagesPdfDone", { n: paths.length }));
  } catch (e) {
    setStatus(`${t("export.error")}: ${e}`);
  }
}

// ---------- Init ----------

export function initExport() {
  document.getElementById("exportImagesBtn")!.addEventListener("click", openExportImagesDialog);
  document.getElementById("exportImgClose")!.addEventListener("click", closeExportImagesDialog);
  document.getElementById("exportImgCancel")!.addEventListener("click", closeExportImagesDialog);
  document.getElementById("exportImgCommit")!.addEventListener("click", () => void commitExportImages());
  document.getElementById("expImgScope")!.addEventListener("change", syncRangeVisibility);
  document.getElementById("exportImgOverlay")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("exportImgOverlay")) closeExportImagesDialog();
  });
  document.getElementById("exportTextBtn")!.addEventListener("click", () => void exportText());
  document.getElementById("exportHtmlBtn")!.addEventListener("click", () => void exportHtml());
  document.getElementById("exportDocxBtn")!.addEventListener("click", () => void exportDocx());
  document.getElementById("exportXlsxBtn")!.addEventListener("click", () => void exportXlsx());
  document.getElementById("imagesToPdfBtn")!.addEventListener("click", () => void imagesToPdf());
  document.getElementById("officeToPdfBtn")!.addEventListener("click", () => void officeToPdf());
}
