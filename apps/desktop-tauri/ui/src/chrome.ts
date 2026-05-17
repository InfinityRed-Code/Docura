import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  FitMode, GhostscriptInfo, SearchHit, VirtualPrinterInfo,
} from "./types";
import { state } from "./state";
import { dom, setStatus } from "./dom";
import {
  applyViewMode, goToPage, reflow, renderAllHighlights,
} from "./viewer";
import { openByPath, closeProperties, doRedo, doUndo } from "./doc";
import {
  confirmFormsLostInPdfa, deleteSelectedPages, triggerPrint, triggerSaveAs,
} from "./edit";

// ---------- Search ----------

function openSearch() {
  if (!state.currentDoc) return;
  dom.searchPanel.classList.remove("hidden");
  dom.searchInput.focus();
  dom.searchInput.select();
}

function closeSearch() {
  dom.searchPanel.classList.add("hidden");
  state.hits = [];
  state.currentHit = -1;
  dom.searchCount.textContent = "";
  renderAllHighlights();
}

async function runSearch() {
  if (!state.currentDoc) return;
  const term = dom.searchInput.value.trim();
  if (!term) {
    state.hits = [];
    state.currentHit = -1;
    dom.searchCount.textContent = "";
    renderAllHighlights();
    return;
  }
  try {
    state.hits = await invoke<SearchHit[]>("search_document", {
      document: state.currentDoc,
      term,
      caseSensitive: dom.searchCase.checked,
    });
    state.currentHit = state.hits.length > 0 ? 0 : -1;
    dom.searchCount.textContent = state.hits.length === 0 ? "No matches" : `${state.hits.length} matches`;
    if (state.currentHit >= 0) jumpToHit(0);
  } catch (e) {
    dom.searchCount.textContent = `Error: ${e}`;
  }
}

function jumpToHit(idx: number) {
  if (state.hits.length === 0) return;
  state.currentHit = (idx + state.hits.length) % state.hits.length;
  dom.searchCount.textContent = `${state.currentHit + 1} / ${state.hits.length}`;
  const target = state.hits[state.currentHit].page;
  if (target !== state.currentPage) {
    goToPage(target);
  } else {
    renderAllHighlights();
  }
}

// ---------- Go-to ----------

function openGoto() {
  if (!state.currentDoc) return;
  dom.gotoPanel.classList.remove("hidden");
  dom.gotoInput.value = String(state.currentPage + 1);
  dom.gotoInput.max = String(state.pageCount);
  dom.gotoInput.min = "1";
  dom.gotoInput.focus();
  dom.gotoInput.select();
}

function closeGoto() {
  dom.gotoPanel.classList.add("hidden");
}

function submitGoto() {
  const v = parseInt(dom.gotoInput.value, 10);
  if (!isNaN(v)) goToPage(v - 1);
  closeGoto();
}

// ---------- Fullscreen / Presentation ----------

async function toggleFullscreen() {
  try {
    const win = getCurrentWindow();
    const full = await win.isFullscreen();
    await win.setFullscreen(!full);
  } catch (e) {
    setStatus(`Fullscreen error: ${e}`);
  }
}

async function togglePresentation() {
  state.presentationActive = !state.presentationActive;
  document.body.classList.toggle("presentation", state.presentationActive);
  try {
    await getCurrentWindow().setFullscreen(state.presentationActive);
  } catch {}
  if (state.presentationActive) {
    state.viewMode = "single";
    dom.viewModeSelect.value = "single";
    state.fitMode = "page";
    dom.fitSelect.value = "page";
    applyViewMode();
    reflow();
  }
}

// ---------- Settings (Ghostscript + Virtual Printer) ----------

async function loadDefaultPdfa() {
  try {
    const v = await invoke<string>("get_default_pdfa");
    if (["off", "1b", "2b", "3b", "4"].includes(v)) dom.pdfaLevelEl.value = v;
  } catch {}
}

async function refreshGhostscript() {
  dom.gsStatusEl.textContent = "Checking Ghostscript…";
  dom.gsInstallEl.classList.add("hidden");
  try {
    state.gsInfo = await invoke<GhostscriptInfo>("check_ghostscript");
    if (state.gsInfo.available) {
      dom.gsStatusEl.textContent = `Ghostscript ${state.gsInfo.version ?? ""} — ${state.gsInfo.path ?? ""}`;
      dom.gsStatusEl.style.color = "#8fcf8f";
    } else {
      dom.gsStatusEl.textContent = "Ghostscript not detected.";
      dom.gsStatusEl.style.color = "#d98a8a";
      dom.gsInstallEl.classList.remove("hidden");
    }
  } catch (e) {
    dom.gsStatusEl.textContent = `Detection error: ${e}`;
    dom.gsStatusEl.style.color = "#d98a8a";
  }
}

async function runSaveAsPdfa() {
  if (!state.currentDoc) { setStatus("Open a PDF first"); return; }
  if (!state.gsInfo.available) {
    setStatus("Ghostscript required — install it first");
    return;
  }
  const level = dom.pdfaLevelEl.value === "off" ? "2b" : dom.pdfaLevelEl.value;
  const base = state.currentDocPath
    ? state.currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "document";
  const dest = await save({
    defaultPath: `${base}_pdfa-${level}.pdf`,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
    title: `Save as PDF/A-${level}`,
  });
  if (!dest || typeof dest !== "string") return;
  if (!(await confirmFormsLostInPdfa(level))) {
    setStatus("Save cancelled");
    return;
  }
  dom.settingsOverlay.classList.add("hidden");
  setStatus(`Converting to PDF/A-${level}…`);
  try {
    const label = await invoke<string>("save_as_pdfa", {
      document: state.currentDoc,
      args: { path: dest, level },
    });
    setStatus(`Saved as ${label}`);
  } catch (e) {
    setStatus(`PDF/A export error: ${e}`);
  }
}

async function refreshVirtualPrinter() {
  dom.vpStatusEl.textContent = "Checking virtual printer…";
  dom.vpStatusEl.style.color = "#bbb";
  dom.vpPathRowEl.classList.add("hidden");
  dom.vpWarnEl.classList.add("hidden");
  try {
    const info = await invoke<VirtualPrinterInfo>("virtual_printer_status");
    if (!info.supported) {
      dom.vpStatusEl.textContent = "Virtual printer is only available on Windows.";
      dom.vpStatusEl.style.color = "#d98a8a";
      dom.vpInstallBtn.disabled = true;
      dom.vpUninstallBtn.disabled = true;
      return;
    }
    if (info.installed) {
      dom.vpStatusEl.textContent = `Installed as "${info.printer_name}".`;
      dom.vpStatusEl.style.color = "#8fcf8f";
      dom.vpInstallBtn.disabled = true;
      dom.vpUninstallBtn.disabled = false;
    } else {
      dom.vpStatusEl.textContent = "Not installed.";
      dom.vpStatusEl.style.color = "#d98a8a";
      dom.vpInstallBtn.disabled = false;
      dom.vpUninstallBtn.disabled = true;
    }
    dom.vpPathRowEl.textContent = `Port file: ${info.port_path}`;
    dom.vpPathRowEl.classList.remove("hidden");
    dom.vpOutRowEl.textContent = `Output folder: ${info.output_dir}`;
    if (!info.ghostscript_available) {
      dom.vpWarnEl.classList.remove("hidden");
    }
  } catch (e) {
    dom.vpStatusEl.textContent = `Detection error: ${e}`;
    dom.vpStatusEl.style.color = "#d98a8a";
  }
}

// ---------- Init ----------

export function initChrome() {
  // Search panel
  document.getElementById("searchBtn")!.addEventListener("click", openSearch);
  document.getElementById("searchClose")!.addEventListener("click", closeSearch);
  dom.searchInput.addEventListener("input", () => {
    if (state.searchDebounce) clearTimeout(state.searchDebounce);
    state.searchDebounce = window.setTimeout(runSearch, 250);
  });
  dom.searchInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (state.hits.length === 0) runSearch();
      else jumpToHit(state.currentHit + (e.shiftKey ? -1 : 1));
    } else if (e.key === "Escape") {
      closeSearch();
    }
  });
  dom.searchCase.addEventListener("change", runSearch);
  document.getElementById("searchPrev")!.addEventListener("click", () => jumpToHit(state.currentHit - 1));
  document.getElementById("searchNext")!.addEventListener("click", () => jumpToHit(state.currentHit + 1));

  // Goto panel
  document.getElementById("gotoBtn")!.addEventListener("click", openGoto);
  document.getElementById("gotoGo")!.addEventListener("click", submitGoto);
  document.getElementById("gotoClose")!.addEventListener("click", closeGoto);
  dom.gotoInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      submitGoto();
    } else if (e.key === "Escape") {
      closeGoto();
    }
  });

  // Fullscreen / Presentation
  document.getElementById("fullscreenBtn")!.addEventListener("click", toggleFullscreen);
  document.getElementById("presentBtn")!.addEventListener("click", togglePresentation);

  // Settings overlay
  void loadDefaultPdfa();
  dom.pdfaLevelEl.addEventListener("change", () => {
    void invoke("set_default_pdfa", { level: dom.pdfaLevelEl.value }).catch(() => {});
  });
  void refreshGhostscript();
  document.getElementById("settingsBtnTop")!.addEventListener("click", () => {
    dom.settingsOverlay.classList.remove("hidden");
    void refreshGhostscript();
    void refreshVirtualPrinter();
  });
  document.getElementById("settingsClose")!.addEventListener("click", () => dom.settingsOverlay.classList.add("hidden"));
  dom.settingsOverlay.addEventListener("click", (e) => {
    if (e.target === dom.settingsOverlay) dom.settingsOverlay.classList.add("hidden");
  });
  document.getElementById("gsRecheckBtn")!.addEventListener("click", () => void refreshGhostscript());
  document.getElementById("gsLink")!.addEventListener("click", (e) => {
    e.preventDefault();
    void invoke("open_external", { url: "https://ghostscript.com/releases/gsdnld.html" });
  });
  document.getElementById("pdfaSaveBtn")!.addEventListener("click", () => void runSaveAsPdfa());

  // Virtual printer buttons
  dom.vpInstallBtn.addEventListener("click", async () => {
    dom.vpInstallBtn.disabled = true;
    setStatus("Installing virtual printer (admin prompt will appear)…");
    try {
      await invoke("install_virtual_printer");
      setStatus("Virtual printer installed");
    } catch (e) {
      setStatus(`Install failed: ${e}`);
    }
    await refreshVirtualPrinter();
  });
  dom.vpUninstallBtn.addEventListener("click", async () => {
    dom.vpUninstallBtn.disabled = true;
    setStatus("Removing virtual printer (admin prompt will appear)…");
    try {
      await invoke("uninstall_virtual_printer");
      setStatus("Virtual printer removed");
    } catch (e) {
      setStatus(`Uninstall failed: ${e}`);
    }
    await refreshVirtualPrinter();
  });
  dom.vpOpenFolderBtn.addEventListener("click", async () => {
    try {
      await invoke("open_print_output_folder");
    } catch (e) {
      setStatus(`Open folder failed: ${e}`);
    }
  });

  // Auto-open newly printed PDFs from virtual printer
  void listen<{ path: string }>("print-job-ready", async (ev) => {
    const path = ev.payload.path;
    setStatus(`Print job ready — opening ${path}`);
    await openByPath(path);
  });
  void listen<{ message: string }>("print-job-failed", (ev) => {
    setStatus(`Print job failed: ${ev.payload.message}`);
  });

  // Global keyboard shortcuts
  window.addEventListener("keydown", (e) => {
    const inField = document.activeElement && /INPUT|TEXTAREA/.test(document.activeElement.tagName);
    if (e.key === "F11") { e.preventDefault(); toggleFullscreen(); return; }
    if (e.key === "F5") { e.preventDefault(); togglePresentation(); return; }
    if (e.key === "F4") {
      e.preventDefault();
      document.getElementById("sidebarBtn")!.click();
      return;
    }
    if (e.ctrlKey && (e.key === "o" || e.key === "O")) {
      e.preventDefault();
      (document.getElementById("open") as HTMLButtonElement).click();
      return;
    }
    if (e.ctrlKey && (e.key === "f" || e.key === "F")) {
      e.preventDefault();
      openSearch();
      return;
    }
    if (e.ctrlKey && (e.key === "g" || e.key === "G")) {
      e.preventDefault();
      openGoto();
      return;
    }
    if (e.ctrlKey && (e.key === "p" || e.key === "P")) {
      e.preventDefault();
      triggerPrint();
      return;
    }
    if (e.ctrlKey && e.shiftKey && (e.key === "s" || e.key === "S")) {
      e.preventDefault();
      triggerSaveAs();
      return;
    }
    if (!inField && e.ctrlKey && !e.shiftKey && (e.key === "z" || e.key === "Z")) {
      e.preventDefault();
      void doUndo();
      return;
    }
    if (!inField && e.ctrlKey && ((e.key === "y" || e.key === "Y") || (e.shiftKey && (e.key === "z" || e.key === "Z")))) {
      e.preventDefault();
      void doRedo();
      return;
    }
    if (!inField && state.ribbonTab === "edit" && (e.key === "Delete" || e.key === "Del")) {
      e.preventDefault();
      if (state.selectedPages.size > 0) deleteSelectedPages();
      return;
    }
    if (e.key === "F3") {
      e.preventDefault();
      if (state.hits.length > 0) jumpToHit(state.currentHit + (e.shiftKey ? -1 : 1));
      return;
    }
    if (e.ctrlKey && (e.key === "+" || e.key === "=")) {
      e.preventDefault();
      (document.getElementById("zoomIn") as HTMLButtonElement).click();
      return;
    }
    if (e.ctrlKey && e.key === "-") {
      e.preventDefault();
      (document.getElementById("zoomOut") as HTMLButtonElement).click();
      return;
    }
    if (e.ctrlKey && e.key === "0") {
      e.preventDefault();
      state.fitMode = "actual";
      dom.fitSelect.value = "actual";
      reflow();
      return;
    }
    if (e.ctrlKey && e.key === "1") {
      e.preventDefault();
      state.fitMode = "page";
      dom.fitSelect.value = "page";
      reflow();
      return;
    }
    if (e.ctrlKey && e.key === "2") {
      e.preventDefault();
      state.fitMode = "width" as FitMode;
      dom.fitSelect.value = "width";
      reflow();
      return;
    }
    if (inField) return;
    if (!state.currentDoc) return;
    if (e.key === "ArrowDown" || e.key === "PageDown") {
      e.preventDefault();
      goToPage(state.currentPage + 1);
    } else if (e.key === "ArrowUp" || e.key === "PageUp") {
      e.preventDefault();
      goToPage(state.currentPage - 1);
    } else if (e.key === "Home") {
      e.preventDefault();
      goToPage(0);
    } else if (e.key === "End") {
      e.preventDefault();
      goToPage(state.pageCount - 1);
    } else if (e.key === "Escape") {
      if (!dom.propsOverlay.classList.contains("hidden")) closeProperties();
      else if (!dom.searchPanel.classList.contains("hidden")) closeSearch();
      else if (!dom.gotoPanel.classList.contains("hidden")) closeGoto();
      else if (state.presentationActive) togglePresentation();
    }
  });
}
