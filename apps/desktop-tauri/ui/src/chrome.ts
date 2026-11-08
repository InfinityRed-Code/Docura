import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { enable as enableAutostart, disable as disableAutostart, isEnabled } from "@tauri-apps/plugin-autostart";
import type {
  FitMode, GhostscriptInfo, SearchHit, VirtualPrinterInfo,
} from "./types";
import { state } from "./state";
import { getLang, setLang, t, type Lang } from "./i18n";
import { refreshIcons } from "./icons";
import { dom, setStatus } from "./dom";
import {
  applyViewMode, goToPage, reflow, renderAllHighlights, setFitMode, stepZoom, toggleSidebar,
} from "./viewer";
import {
  confirmDiscardChanges, confirmFormsLostInPdfa, docBaseName, doRedo, doUndo,
  openByPath, openWithDialog, saveDocument, saveDocumentAs,
} from "./doc";
import { deleteSelectedPages, triggerPrint } from "./edit";
import { disarmAllTools } from "./annot";

// ---------- Window controls (custom title bar) ----------
//
// Native decorations are off (tauri.conf.json); the ribbon's tab row doubles
// as the title bar. Dragging / double-click maximize is handled natively via
// data-tauri-drag-region; the three buttons here cover minimize / maximize /
// close, with the middle icon reflecting the current maximize state.

async function syncMaximizeButton() {
  const btn = document.getElementById("winMax");
  if (!btn) return;
  try {
    const max = await getCurrentWindow().isMaximized();
    btn.innerHTML = `<i data-lucide="${max ? "copy" : "square"}"></i>`;
    btn.title = max ? t("tip.winRestore") : t("tip.winMax");
    refreshIcons();
  } catch {}
}

function initWindowControls() {
  const win = getCurrentWindow();
  document.getElementById("winMin")!.addEventListener("click", () => void win.minimize());
  document.getElementById("winMax")!.addEventListener("click", async () => {
    await win.toggleMaximize().catch(() => {});
    void syncMaximizeButton();
  });
  document.getElementById("winClose")!.addEventListener("click", () => void win.close());
  // Covers maximize via drag-region double-click, snap and Win+arrow keys.
  void win.onResized(() => void syncMaximizeButton());
  void syncMaximizeButton();

  // Closing (button, Alt+F4, taskbar): never lose unsaved edits silently.
  // With "run in background" the Rust side only hides the window, so there
  // is nothing to ask.
  void win.onCloseRequested(async (event) => {
    const runInBackground = await invoke<boolean>("get_run_in_background").catch(() => false);
    if (runInBackground) {
      event.preventDefault();
      return;
    }
    if (!(await confirmDiscardChanges())) event.preventDefault();
  });
}

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
  const doc = state.currentDoc;
  try {
    const hits = await invoke<SearchHit[]>("search_document", {
      document: doc,
      term,
      caseSensitive: dom.searchCase.checked,
    });
    // Stale answer (document switched or the term changed meanwhile).
    if (doc !== state.currentDoc || term !== dom.searchInput.value.trim()) return;
    state.hits = hits;
    state.currentHit = state.hits.length > 0 ? 0 : -1;
    dom.searchCount.textContent = state.hits.length === 0
      ? t("st.noMatches")
      : t("st.matches", { n: state.hits.length });
    if (state.currentHit >= 0) jumpToHit(0);
    else renderAllHighlights();
  } catch (e) {
    dom.searchCount.textContent = t("st.error", { e: String(e) });
  }
}

function jumpToHit(idx: number) {
  if (state.hits.length === 0) return;
  state.currentHit = (idx + state.hits.length) % state.hits.length;
  dom.searchCount.textContent = `${state.currentHit + 1} / ${state.hits.length}`;
  const target = state.hits[state.currentHit].page;
  if (target !== state.currentPage) {
    goToPage(target);
  }
  renderAllHighlights();
}

// ---------- Fullscreen / Presentation ----------

async function toggleFullscreen() {
  try {
    const win = getCurrentWindow();
    const full = await win.isFullscreen();
    await win.setFullscreen(!full);
  } catch (e) {
    setStatus(t("st.fullscreenError", { e: String(e) }));
  }
}

async function togglePresentation() {
  if (!state.currentDoc && !state.presentationActive) return;
  state.presentationActive = !state.presentationActive;
  document.body.classList.toggle("presentation", state.presentationActive);
  try {
    await getCurrentWindow().setFullscreen(state.presentationActive);
  } catch {}
  if (state.presentationActive) {
    state.viewMode = "single";
    dom.viewModeSelect.value = "single";
    state.fitMode = "page";
    applyViewMode();
  }
  reflow();
}

// ---------- Settings (Ghostscript + Virtual Printer) ----------

const OK_COLOR = "var(--ok, #2c7a2c)";
const BAD_COLOR = "var(--danger)";

async function loadDefaultPdfa() {
  try {
    const v = await invoke<string>("get_default_pdfa");
    if (["off", "1b", "2b", "3b", "4"].includes(v)) dom.pdfaLevelEl.value = v;
  } catch {}
}

async function refreshGhostscript() {
  dom.gsStatusEl.textContent = t("set.gsChecking");
  dom.gsStatusEl.style.color = "";
  dom.gsInstallEl.classList.add("hidden");
  try {
    state.gsInfo = await invoke<GhostscriptInfo>("check_ghostscript");
    if (state.gsInfo.available) {
      dom.gsStatusEl.textContent = t("gs.found", {
        version: state.gsInfo.version ?? "", path: state.gsInfo.path ?? "",
      });
      dom.gsStatusEl.style.color = OK_COLOR;
    } else {
      dom.gsStatusEl.textContent = t("gs.notFound");
      dom.gsStatusEl.style.color = BAD_COLOR;
      dom.gsInstallEl.classList.remove("hidden");
    }
  } catch (e) {
    dom.gsStatusEl.textContent = t("gs.error", { e: String(e) });
    dom.gsStatusEl.style.color = BAD_COLOR;
  }
}

/** Save a PDF/A copy of the open document (Convert tab + Settings). */
async function runSaveAsPdfa() {
  if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
  if (!state.gsInfo.available) await refreshGhostscript();
  if (!state.gsInfo.available) {
    setStatus(t("st.gsRequired"));
    return;
  }
  const level = dom.pdfaLevelEl.value === "off" ? "2b" : dom.pdfaLevelEl.value;
  const dest = await save({
    defaultPath: `${docBaseName()}_pdfa-${level}.pdf`,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
    title: t("lbl.saveAsPdfa"),
  });
  if (!dest || typeof dest !== "string") return;
  if (!(await confirmFormsLostInPdfa(level))) {
    setStatus(t("st.cancelled"));
    return;
  }
  dom.settingsOverlay.classList.add("hidden");
  setStatus(t("st.savingPdfa", { level }));
  try {
    const label = await invoke<string>("save_as_pdfa", {
      document: state.currentDoc,
      args: { path: dest, level },
    });
    setStatus(t("st.savedPdfa", { label, path: dest }));
  } catch (e) {
    setStatus(t("st.pdfaError", { e: String(e) }));
  }
}

async function refreshVirtualPrinter() {
  dom.vpStatusEl.textContent = t("set.vpChecking");
  dom.vpStatusEl.style.color = "";
  dom.vpPathRowEl.classList.add("hidden");
  dom.vpWarnEl.classList.add("hidden");
  try {
    const info = await invoke<VirtualPrinterInfo>("virtual_printer_status");
    if (!info.supported) {
      dom.vpStatusEl.textContent = t("vp.onlyWindows");
      dom.vpStatusEl.style.color = BAD_COLOR;
      dom.vpInstallBtn.disabled = true;
      dom.vpUninstallBtn.disabled = true;
      return;
    }
    if (info.installed) {
      dom.vpStatusEl.textContent = t("vp.installed", { name: info.printer_name });
      dom.vpStatusEl.style.color = OK_COLOR;
      dom.vpInstallBtn.disabled = true;
      dom.vpUninstallBtn.disabled = false;
    } else {
      dom.vpStatusEl.textContent = t("vp.notInstalled");
      dom.vpStatusEl.style.color = BAD_COLOR;
      dom.vpInstallBtn.disabled = false;
      dom.vpUninstallBtn.disabled = true;
    }
    dom.vpPathRowEl.textContent = t("vp.port", { path: info.port_path });
    dom.vpPathRowEl.classList.remove("hidden");
    dom.vpOutRowEl.textContent = t("vp.out", { path: info.output_dir });
    if (!info.ghostscript_available) {
      dom.vpWarnEl.classList.remove("hidden");
    }
  } catch (e) {
    dom.vpStatusEl.textContent = t("gs.error", { e: String(e) });
    dom.vpStatusEl.style.color = BAD_COLOR;
  }
}

// ---------- Modal helpers ----------

/** Topmost open dialog (overlays are appended in DOM order). */
function openOverlay(): HTMLElement | null {
  const open = document.querySelectorAll<HTMLElement>(".propsOverlay:not(.hidden)");
  return open.length > 0 ? open[open.length - 1] : null;
}

/** Close a dialog through its own ✕ so module-specific cleanup runs. */
function closeOverlay(o: HTMLElement) {
  const btn = o.querySelector<HTMLElement>(".propsHeader .propsClose, .propsClose");
  if (btn) btn.click();
  else o.classList.add("hidden");
}

// ---------- Init ----------

export function initChrome() {
  // Window controls (custom title bar)
  initWindowControls();

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
      if (state.hits.length === 0) void runSearch();
      else jumpToHit(state.currentHit + (e.shiftKey ? -1 : 1));
    } else if (e.key === "Escape") {
      e.stopPropagation();
      closeSearch();
    }
  });
  dom.searchCase.addEventListener("change", () => void runSearch());
  document.getElementById("searchPrev")!.addEventListener("click", () => jumpToHit(state.currentHit - 1));
  document.getElementById("searchNext")!.addEventListener("click", () => jumpToHit(state.currentHit + 1));

  // Fullscreen / Presentation
  document.getElementById("fullscreenBtn")!.addEventListener("click", toggleFullscreen);
  document.getElementById("presentBtn")!.addEventListener("click", togglePresentation);
  document.getElementById("sidebarBtn")!.addEventListener("click", () => {
    toggleSidebar();
    document.getElementById("sidebarBtn")!.classList.toggle("active", !dom.sidebarEl.classList.contains("hidden"));
  });

  // Theme — persisted in localStorage.
  const themeSelect = document.getElementById("themeSelect") as HTMLSelectElement;
  const applyTheme = (theme: string) => {
    document.body.classList.toggle("theme-dark", theme === "dark");
    document.body.classList.toggle("theme-docura", theme === "docura");
    document.body.classList.toggle("theme-contrast", theme === "contrast");
    // Color the native Windows title bar along with the app (Windows 11+).
    invoke("set_titlebar_theme", { dark: theme === "dark" || theme === "contrast" }).catch(() => {});
  };
  const savedTheme = localStorage.getItem("rustpdf-theme") ?? "light";
  themeSelect.value = savedTheme;
  applyTheme(savedTheme);
  themeSelect.addEventListener("change", () => {
    localStorage.setItem("rustpdf-theme", themeSelect.value);
    applyTheme(themeSelect.value);
  });

  // Accessibility: expose the UI language to assistive tech, and give every
  // icon-only button an aria-label mirrored from its tooltip (screen readers
  // announce nothing for a button that holds only an SVG icon).
  document.documentElement.lang = getLang();
  document.querySelectorAll<HTMLElement>("button[title]").forEach((btn) => {
    if (!btn.getAttribute("aria-label") && !btn.textContent?.trim()) {
      btn.setAttribute("aria-label", btn.getAttribute("title") ?? "");
    }
  });

  // Language — changing it re-renders the whole UI, so reload after persisting.
  // Reloading drops the open document, so unsaved work is offered first.
  const langSelect = document.getElementById("langSelect") as HTMLSelectElement;
  langSelect.value = getLang();
  langSelect.addEventListener("change", async () => {
    if (!(await confirmDiscardChanges())) {
      langSelect.value = getLang();
      return;
    }
    setLang(langSelect.value as Lang);
    location.reload();
  });

  // Default zoom — the fit mode applied to documents on open. Persisted in
  // localStorage and seeded into state now so the first opened doc uses it.
  const defaultZoomSelect = document.getElementById("defaultZoomSelect") as HTMLSelectElement;
  const applyDefaultZoom = (v: string) => {
    const mode = (["page", "width", "actual"].includes(v) ? v : "page") as FitMode;
    setFitMode(mode);
  };
  const savedZoom = localStorage.getItem("docura-default-zoom") ?? "page";
  defaultZoomSelect.value = savedZoom;
  applyDefaultZoom(savedZoom);
  defaultZoomSelect.addEventListener("change", () => {
    localStorage.setItem("docura-default-zoom", defaultZoomSelect.value);
    applyDefaultZoom(defaultZoomSelect.value);
  });

  // Render-cache budget (MB) — read the effective value from the backend and
  // push changes back (clamped + persisted server-side).
  const cacheMbInput = document.getElementById("cacheMbInput") as HTMLInputElement;
  void invoke<number>("get_cache_mb")
    .then((mb) => { cacheMbInput.value = String(mb); })
    .catch(() => {});
  cacheMbInput.addEventListener("change", () => {
    let mb = parseInt(cacheMbInput.value, 10);
    if (isNaN(mb) || mb < 16) mb = 16;
    void invoke<number>("set_cache_mb", { mb })
      .then((applied) => { cacheMbInput.value = String(applied); })
      .catch(() => {});
  });

  // System: run-in-background (close-to-tray) + autostart at login.
  const runBgCheck = document.getElementById("runBackgroundCheck") as HTMLInputElement;
  void invoke<boolean>("get_run_in_background")
    .then((v) => { runBgCheck.checked = v; })
    .catch(() => {});
  runBgCheck.addEventListener("change", () => {
    void invoke("set_run_in_background", { enabled: runBgCheck.checked }).catch(() => {});
  });

  const autostartCheck = document.getElementById("autostartCheck") as HTMLInputElement;
  void isEnabled()
    .then((v) => { autostartCheck.checked = v; })
    .catch(() => {});
  autostartCheck.addEventListener("change", () => {
    const op = autostartCheck.checked ? enableAutostart() : disableAutostart();
    void op.catch(() => { autostartCheck.checked = !autostartCheck.checked; });
  });

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

  // Settings nav rail: clicking a tab scrolls its section into the body;
  // a scrollspy keeps the active tab in sync while scrolling.
  const settingsBody = document.getElementById("settingsBody")!;
  const navItems = Array.from(
    document.querySelectorAll<HTMLButtonElement>("#settingsNav .settingsNavItem"),
  );
  navItems.forEach((btn) => {
    btn.addEventListener("click", () => {
      document.getElementById(btn.dataset.sec!)?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  });
  // Each callback only reports sections whose intersection *changed*, so we
  // keep a running map of all sections' visibility and pick the topmost one in
  // DOM order — otherwise the last-changed section wins and highlighting sticks.
  const secVisible = new Map<string, boolean>();
  const spy = new IntersectionObserver(
    (entries) => {
      entries.forEach((e) => secVisible.set(e.target.id, e.isIntersecting));
      const active = navItems.find((n) => secVisible.get(n.dataset.sec!));
      if (active) navItems.forEach((n) => n.classList.toggle("active", n === active));
    },
    { root: settingsBody, rootMargin: "0px 0px -55% 0px", threshold: 0 },
  );
  navItems.forEach((btn) => {
    const sec = document.getElementById(btn.dataset.sec!);
    if (sec) spy.observe(sec);
  });
  document.getElementById("gsRecheckBtn")!.addEventListener("click", () => void refreshGhostscript());
  document.getElementById("gsLink")!.addEventListener("click", (e) => {
    e.preventDefault();
    void invoke("open_external", { url: "https://ghostscript.com/releases/gsdnld.html" });
  });
  document.getElementById("pdfaSaveBtn")!.addEventListener("click", () => void runSaveAsPdfa());
  document.getElementById("pdfaSaveTabBtn")!.addEventListener("click", () => void runSaveAsPdfa());

  // Virtual printer buttons
  dom.vpInstallBtn.addEventListener("click", async () => {
    dom.vpInstallBtn.disabled = true;
    setStatus(t("vp.installing"));
    try {
      await invoke("install_virtual_printer");
      setStatus(t("vp.installedOk"));
    } catch (e) {
      setStatus(t("vp.installFail", { e: String(e) }));
    }
    await refreshVirtualPrinter();
  });
  dom.vpUninstallBtn.addEventListener("click", async () => {
    dom.vpUninstallBtn.disabled = true;
    setStatus(t("vp.removing"));
    try {
      await invoke("uninstall_virtual_printer");
      setStatus(t("vp.removed"));
    } catch (e) {
      setStatus(t("vp.removeFail", { e: String(e) }));
    }
    await refreshVirtualPrinter();
  });
  dom.vpOpenFolderBtn.addEventListener("click", async () => {
    try {
      await invoke("open_print_output_folder");
    } catch (e) {
      setStatus(t("vp.openFail", { e: String(e) }));
    }
  });

  // Auto-open newly printed PDFs from virtual printer
  void listen<{ path: string }>("print-job-ready", async (ev) => {
    const path = ev.payload.path;
    setStatus(t("vp.jobReady", { path }));
    await openByPath(path);
  });
  void listen<{ message: string }>("print-job-failed", (ev) => {
    setStatus(t("vp.jobFailed", { e: ev.payload.message }));
  });

  // Every dialog closes on a click on its dimmed backdrop.
  document.querySelectorAll<HTMLElement>(".propsOverlay").forEach((o) => {
    o.addEventListener("click", (e) => {
      if (e.target === o) closeOverlay(o);
    });
  });

  // Escape: close the topmost dialog, else the search bar, else leave the
  // presentation, else disarm the active page tool. Capture phase so it
  // wins over inputs inside the dialog.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    const o = openOverlay();
    if (o) {
      e.preventDefault();
      e.stopPropagation();
      closeOverlay(o);
      return;
    }
    if (!dom.searchPanel.classList.contains("hidden")) { closeSearch(); return; }
    if (state.presentationActive) { void togglePresentation(); return; }
    disarmAllTools();
  }, true);

  // Global keyboard shortcuts
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") return; // handled above
    const ae = document.activeElement as HTMLElement | null;
    const inField = !!ae && (/^(INPUT|TEXTAREA|SELECT)$/.test(ae.tagName) || ae.isContentEditable);
    // While a dialog is open only the dialog gets the keyboard.
    if (openOverlay()) return;
    const key = e.key.toLowerCase();
    if (e.key === "F11") { e.preventDefault(); void toggleFullscreen(); return; }
    if (e.key === "F5") { e.preventDefault(); void togglePresentation(); return; }
    if (e.key === "F4") {
      e.preventDefault();
      document.getElementById("sidebarBtn")!.click();
      return;
    }
    if (e.ctrlKey && key === "o") { e.preventDefault(); void openWithDialog(); return; }
    if (e.ctrlKey && key === "s") {
      e.preventDefault();
      void (e.shiftKey ? saveDocumentAs() : saveDocument());
      return;
    }
    if (e.ctrlKey && key === "f") { e.preventDefault(); openSearch(); return; }
    if (e.ctrlKey && key === "g") {
      e.preventDefault();
      if (state.currentDoc) { dom.pageInput.focus(); dom.pageInput.select(); }
      return;
    }
    if (e.ctrlKey && key === "p") { e.preventDefault(); void triggerPrint(); return; }
    if (!inField && e.ctrlKey && !e.shiftKey && key === "z") { e.preventDefault(); void doUndo(); return; }
    if (!inField && e.ctrlKey && (key === "y" || (e.shiftKey && key === "z"))) {
      e.preventDefault();
      void doRedo();
      return;
    }
    if (!inField && state.ribbonTab === "organize" && e.key === "Delete") {
      e.preventDefault();
      if (state.selectedPages.size > 0) void deleteSelectedPages();
      return;
    }
    if (e.key === "F3") {
      e.preventDefault();
      if (state.hits.length > 0) jumpToHit(state.currentHit + (e.shiftKey ? -1 : 1));
      return;
    }
    if (e.ctrlKey && (e.key === "+" || e.key === "=")) { e.preventDefault(); if (state.currentDoc) stepZoom(1); return; }
    if (e.ctrlKey && e.key === "-") { e.preventDefault(); if (state.currentDoc) stepZoom(-1); return; }
    if (e.ctrlKey && e.key === "0") { e.preventDefault(); setFitMode("actual"); return; }
    if (e.ctrlKey && e.key === "1") { e.preventDefault(); setFitMode("page"); return; }
    if (e.ctrlKey && e.key === "2") { e.preventDefault(); setFitMode("width"); return; }
    if (inField) return;
    if (!state.currentDoc) return;
    if (e.key === "ArrowDown" || e.key === "PageDown") {
      e.preventDefault();
      goToPage(state.currentPage + 1);
    } else if (e.key === "ArrowUp" || e.key === "PageUp") {
      e.preventDefault();
      goToPage(state.currentPage - 1);
    } else if (e.key === "ArrowRight" && state.viewMode === "single") {
      e.preventDefault();
      goToPage(state.currentPage + 1);
    } else if (e.key === "ArrowLeft" && state.viewMode === "single") {
      e.preventDefault();
      goToPage(state.currentPage - 1);
    } else if (e.key === "Home") {
      e.preventDefault();
      goToPage(0);
    } else if (e.key === "End") {
      e.preventDefault();
      goToPage(state.pageCount - 1);
    }
  });
}
