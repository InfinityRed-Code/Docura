import { invoke } from "@tauri-apps/api/core";
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import type { DocSnapshot, RectPt } from "./types";
import { t } from "./i18n";
import { state, callbacks } from "./state";
import { setStatus } from "./dom";
import { applyDocSnapshot, markDirty, refreshUndoUi, saveDocumentAs } from "./doc";
import { effectiveRotation, rotateRectToSlot } from "./viewer";

// ---------- Protect dialog ----------
// (The password prompt modal lives in doc.ts to avoid an import cycle:
// doc → protect → edit → doc.)

type SecurityInfo = {
  protected: boolean;
  has_user_password: boolean;
  permissions: { print: boolean; modify: boolean; copy: boolean; annotate: boolean };
};

async function refreshProtectUi() {
  const unprotectBtn = document.getElementById("unprotectBtn") as HTMLButtonElement;
  const hint = document.getElementById("protectHint")!;
  if (!state.currentDoc) {
    unprotectBtn.disabled = true;
    hint.textContent = t("hint.noDoc");
    return;
  }
  try {
    const info = await invoke<SecurityInfo>("get_pdf_security", { document: state.currentDoc });
    unprotectBtn.disabled = !info.protected;
    hint.textContent = info.protected ? t("protect.statusOn") : t("protect.statusOff");
  } catch {
    unprotectBtn.disabled = true;
    hint.textContent = "";
  }
}

async function openProtectDialog() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const overlay = document.getElementById("protectOverlay")!;
  try {
    const info = await invoke<SecurityInfo>("get_pdf_security", { document: state.currentDoc });
    (document.getElementById("permPrint") as HTMLInputElement).checked = info.permissions.print;
    (document.getElementById("permModify") as HTMLInputElement).checked = info.permissions.modify;
    (document.getElementById("permCopy") as HTMLInputElement).checked = info.permissions.copy;
    (document.getElementById("permAnnotate") as HTMLInputElement).checked = info.permissions.annotate;
  } catch {}
  (document.getElementById("protectUserPw") as HTMLInputElement).value = "";
  (document.getElementById("protectOwnerPw") as HTMLInputElement).value = "";
  overlay.classList.remove("hidden");
  (document.getElementById("protectUserPw") as HTMLInputElement).focus();
}

function closeProtectDialog() {
  document.getElementById("protectOverlay")!.classList.add("hidden");
}

async function commitProtection() {
  if (!state.currentDoc) return;
  const userPw = (document.getElementById("protectUserPw") as HTMLInputElement).value;
  const ownerPw = (document.getElementById("protectOwnerPw") as HTMLInputElement).value;
  if (!userPw && !ownerPw) {
    setStatus(t("protect.needPw"));
    return;
  }
  const permissions = {
    print: (document.getElementById("permPrint") as HTMLInputElement).checked,
    modify: (document.getElementById("permModify") as HTMLInputElement).checked,
    copy: (document.getElementById("permCopy") as HTMLInputElement).checked,
    annotate: (document.getElementById("permAnnotate") as HTMLInputElement).checked,
  };
  try {
    await invoke("set_pdf_protection", {
      document: state.currentDoc,
      userPassword: userPw,
      ownerPassword: ownerPw,
      permissions,
    });
  } catch (e) {
    setStatus(`${t("protect.error")}: ${e}`);
    return;
  }
  closeProtectDialog();
  setStatus(t("protect.applied"));
  markDirty();
  void refreshProtectUi();
  // Protection only takes effect on disk — offer the save dialog right away.
  await saveDocumentAs();
}

async function removeProtection() {
  if (!state.currentDoc) return;
  try {
    await invoke("remove_pdf_protection", { document: state.currentDoc });
  } catch (e) {
    setStatus(`${t("protect.error")}: ${e}`);
    return;
  }
  setStatus(t("protect.removed"));
  markDirty();
  void refreshProtectUi();
  await saveDocumentAs();
}

// ---------- Redaction ----------
//
// Pending marks live in state.pendingRedactions (page + rect in pt) and are
// drawn into each slot's redactLayer. "Apply" rasterises the affected pages
// with the regions blacked out — the original content is removed for good
// (undo via the byte-level undo stack still works until the file is saved).

export function renderRedactMarksFor(idx: number) {
  const slot = state.slots[idx];
  if (!slot || !slot.loaded) return;
  const sz = state.pageSizes[idx];
  if (!sz) return;
  slot.redactLayer.innerHTML = "";
  const rot = effectiveRotation(idx);
  state.pendingRedactions.forEach((m) => {
    if (m.page !== idx) return;
    const box = rotateRectToSlot(m.rect, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) return;
    const el = document.createElement("div");
    el.className = "redactMark";
    el.style.left = `${box.left}px`;
    el.style.top = `${box.top}px`;
    el.style.width = `${box.width}px`;
    el.style.height = `${box.height}px`;
    const rm = document.createElement("button");
    rm.textContent = "✕";
    rm.title = t("redact.removeMark");
    rm.addEventListener("click", (e) => {
      e.stopPropagation();
      const j = state.pendingRedactions.indexOf(m);
      if (j >= 0) state.pendingRedactions.splice(j, 1);
      renderRedactMarksFor(idx);
      updateRedactUi();
    });
    el.appendChild(rm);
    slot.redactLayer.appendChild(el);
  });
}

function updateRedactUi() {
  const apply = document.getElementById("redactApplyBtn") as HTMLButtonElement | null;
  const discard = document.getElementById("redactDiscardBtn") as HTMLButtonElement | null;
  const n = state.pendingRedactions.length;
  if (apply) {
    apply.disabled = n === 0;
    const label = apply.querySelector("span");
    if (label) label.textContent = n > 0 ? `${t("redact.apply")} (${n})` : t("redact.apply");
  }
  if (discard) discard.disabled = n === 0;
}

function discardRedactions() {
  const pages = new Set(state.pendingRedactions.map((m) => m.page));
  state.pendingRedactions.length = 0;
  for (const p of pages) renderRedactMarksFor(p);
  updateRedactUi();
}

let redacting = false;

async function applyRedactions() {
  if (!state.currentDoc || state.pendingRedactions.length === 0 || redacting) return;
  const ok = await confirmDialog(t("redact.confirmBody"), {
    title: t("redact.confirmTitle"),
    kind: "warning",
  });
  if (!ok) return;
  setStatus(t("redact.working"));
  redacting = true;
  try {
    const regions = state.pendingRedactions.map((m) => ({ page: m.page, rect: m.rect }));
    const doc = state.currentDoc;
    const snap = await invoke<DocSnapshot>("redact_regions", {
      document: doc,
      regions,
    });
    if (doc !== state.currentDoc) return;
    discardRedactions();
    applyDocSnapshot(snap);
    setStatus(t("redact.done", { n: regions.length }));
  } catch (e) {
    setStatus(`${t("redact.error")}: ${e}`);
  } finally {
    redacting = false;
  }
}

async function sanitizeMetadata() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  const ok = await confirmDialog(t("sanitize.confirmBody"), {
    title: t("sanitize.confirmTitle"),
    kind: "warning",
  });
  if (!ok) return;
  try {
    const removed = await invoke<number>("sanitize_metadata_cmd", { document: state.currentDoc });
    setStatus(t("sanitize.done", { n: removed }));
    markDirty();
    void refreshUndoUi();
  } catch (e) {
    setStatus(`${t("sanitize.error")}: ${e}`);
  }
}

// ---------- Pattern redaction ----------
//
// Regex search over the page text (per-character boxes from pdfium); every
// match is added as a pending redaction mark, so review/apply/discard run
// through the normal redaction flow above.

const PATTERN_EMAIL = "(?i)[A-Z0-9._%+-]+@[A-Z0-9.-]+\\.[A-Z]{2,}";
const PATTERN_PHONE = "(?:\\+[0-9]{1,3}[ \\-/]?|0)[0-9](?:[0-9 \\-/()]{5,18})[0-9]";

function openPatternDialog() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  document.getElementById("patternOverlay")!.classList.remove("hidden");
}

function closePatternDialog() {
  document.getElementById("patternOverlay")!.classList.add("hidden");
}

async function commitPatternSearch() {
  if (!state.currentDoc) return;
  const parts: string[] = [];
  if ((document.getElementById("patternEmail") as HTMLInputElement).checked) {
    parts.push(PATTERN_EMAIL);
  }
  if ((document.getElementById("patternPhone") as HTMLInputElement).checked) {
    parts.push(PATTERN_PHONE);
  }
  const custom = (document.getElementById("patternCustom") as HTMLInputElement).value.trim();
  if (custom) parts.push(custom);
  if (parts.length === 0) {
    setStatus(t("pattern.needOne"));
    return;
  }
  const pattern = parts.map((p) => `(?:${p})`).join("|");
  setStatus(t("pattern.working"));
  try {
    const regions = await invoke<{ page: number; rect: RectPt }[]>("find_pattern_regions", {
      document: state.currentDoc,
      pattern,
    });
    closePatternDialog();
    if (regions.length === 0) {
      setStatus(t("pattern.none"));
      return;
    }
    state.pendingRedactions.push(...regions);
    for (const p of new Set(regions.map((r) => r.page))) renderRedactMarksFor(p);
    updateRedactUi();
    setStatus(t("pattern.done", { n: regions.length }));
  } catch (e) {
    setStatus(`${t("pattern.error")}: ${e}`);
  }
}

// ---------- Hidden content scan ----------
//
// Read-only report: optional-content layers (with default visibility),
// comments/annotations and invisible text (render mode 3). Shown in the
// hiddenOverlay dialog; nothing is modified.

type HiddenReport = {
  layers: { name: string; hidden: boolean }[];
  annotations: { page: number; subtype: string }[];
  invisible_text: { page: number; text: string }[];
};

function hiddenSection(body: HTMLElement, title: string, lines: string[]) {
  if (lines.length === 0) return;
  const head = document.createElement("strong");
  head.style.fontSize = "0.85rem";
  head.textContent = `${title} (${lines.length})`;
  body.appendChild(head);
  const list = document.createElement("div");
  list.className = "hiddenList";
  const MAX = 50;
  for (const line of lines.slice(0, MAX)) {
    const row = document.createElement("div");
    row.textContent = line;
    row.title = line;
    list.appendChild(row);
  }
  if (lines.length > MAX) {
    const row = document.createElement("div");
    row.textContent = t("hidden.more", { n: lines.length - MAX });
    list.appendChild(row);
  }
  body.appendChild(list);
}

function renderHiddenReport(rep: HiddenReport) {
  const body = document.getElementById("hiddenBody")!;
  body.innerHTML = "";
  const layerLines = rep.layers.map(
    (l) => `${l.name} — ${l.hidden ? t("hidden.layerHidden") : t("hidden.layerVisible")}`,
  );
  const annotLines = rep.annotations.map(
    (a) => `${t("hidden.page", { n: a.page + 1 })} — ${a.subtype}`,
  );
  const textLines = rep.invisible_text.map(
    (x) =>
      `${t("hidden.page", { n: x.page + 1 })} — ${x.text.length > 80 ? `${x.text.slice(0, 80)}…` : x.text}`,
  );
  if (!layerLines.length && !annotLines.length && !textLines.length) {
    const none = document.createElement("div");
    none.textContent = t("hidden.none");
    body.appendChild(none);
  } else {
    hiddenSection(body, t("hidden.layers"), layerLines);
    hiddenSection(body, t("hidden.comments"), annotLines);
    hiddenSection(body, t("hidden.invisibleText"), textLines);
  }
  document.getElementById("hiddenOverlay")!.classList.remove("hidden");
}

async function findHiddenContent() {
  if (!state.currentDoc) {
    setStatus(t("st.openFirst"));
    return;
  }
  setStatus(t("hidden.working"));
  try {
    const rep = await invoke<HiddenReport>("find_hidden_content", {
      document: state.currentDoc,
    });
    setStatus("");
    renderHiddenReport(rep);
  } catch (e) {
    setStatus(`${t("hidden.error")}: ${e}`);
  }
}

function closeHiddenDialog() {
  document.getElementById("hiddenOverlay")!.classList.add("hidden");
}

// ---------- Init ----------

export function initProtect() {
  // Protect dialog
  document.getElementById("protectBtn")!.addEventListener("click", () => void openProtectDialog());
  document.getElementById("unprotectBtn")!.addEventListener("click", () => void removeProtection());
  document.getElementById("protectClose")!.addEventListener("click", closeProtectDialog);
  document.getElementById("protectCancel")!.addEventListener("click", closeProtectDialog);
  document.getElementById("protectCommit")!.addEventListener("click", () => void commitProtection());
  document.getElementById("protectOverlay")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("protectOverlay")) closeProtectDialog();
  });

  // Redaction + sanitize
  callbacks.renderRedactsFor = renderRedactMarksFor;
  callbacks.discardPendingRedactions = () => {
    if (state.pendingRedactions.length > 0) discardRedactions();
  };
  document.getElementById("redactApplyBtn")!.addEventListener("click", () => void applyRedactions());
  document.getElementById("redactDiscardBtn")!.addEventListener("click", discardRedactions);
  document.getElementById("sanitizeBtn")!.addEventListener("click", () => void sanitizeMetadata());
  document.addEventListener("rustpdf:redactions-changed", updateRedactUi);

  // Pattern redaction
  document.getElementById("redactPatternBtn")!.addEventListener("click", openPatternDialog);
  document.getElementById("patternClose")!.addEventListener("click", closePatternDialog);
  document.getElementById("patternCancel")!.addEventListener("click", closePatternDialog);
  document.getElementById("patternCommit")!.addEventListener("click", () => void commitPatternSearch());
  document.getElementById("patternOverlay")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("patternOverlay")) closePatternDialog();
  });

  // Hidden content scan
  document.getElementById("findHiddenBtn")!.addEventListener("click", () => void findHiddenContent());
  document.getElementById("hiddenClose")!.addEventListener("click", closeHiddenDialog);
  document.getElementById("hiddenOverlay")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("hiddenOverlay")) closeHiddenDialog();
  });

  // Keep the protect ribbon state in sync with the open document.
  document.addEventListener("rustpdf:doc-changed", () => {
    void refreshProtectUi();
    discardRedactions();
  });
  void refreshProtectUi();
  updateRedactUi();
}
