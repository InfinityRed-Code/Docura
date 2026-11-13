import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  DocSnapshot, RectPt, SignatureEntry, SignatureReport, SignTool, Slot, WinCertEntry,
} from "./types";
import { openNewTextBox } from "./textedit";
import { state } from "./state";
import { t } from "./i18n";
import { setStatus } from "./dom";
import { applyDocSnapshot } from "./doc";
import { effectiveRotation } from "./viewer";
import {
  clearMarkupAndEraser, clearSignToolUi, findSlotFromTarget, hexToRgb01, imagePxToPagePt,
  isOverlayControl, setShapeTool,
} from "./annot";

/** Localized name of a signature kind / mark tool for status messages. */
function toolLabel(tool: string): string {
  switch (tool) {
    case "signature": return t("btn.signature");
    case "initials": return t("btn.initials");
    case "check": return t("mark.check");
    case "cross": return t("mark.cross");
    case "dot": return t("mark.dot");
    case "date": return t("mark.date");
    default: return tool;
  }
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[c]!);
}

// ---------- Saved signatures ----------

export async function refreshSignatures() {
  try {
    state.signatures = await invoke<SignatureEntry[]>("list_signatures");
  } catch (e) {
    setStatus(t("st.signaturesError", { e: String(e) }));
    state.signatures = [];
  }
}

function signatureFor(kind: string): SignatureEntry | undefined {
  return state.signatures.find((s) => s.kind === kind);
}

function setSignHint(msg: string) {
  const el = document.getElementById("signHint");
  if (el) el.textContent = msg;
}

// Arm signature/initials placement by reusing the stamp drag-to-place pipeline.
async function armSignature(kind: "signature" | "initials") {
  const entry = signatureFor(kind);
  if (!entry) {
    openSignModal(kind);
    return;
  }
  state.stampImagePath = entry.path;
  setShapeTool("stamp"); // clears sign tool UI…
  // …so re-mark this button as the armed tool afterwards.
  const btnId = kind === "signature" ? "signSignatureBtn" : "signInitialsBtn";
  document.getElementById(btnId)?.classList.add("active");
  setSignHint(t("st.dragToPlace", { kind: toolLabel(kind) }));
}

// ---------- Click-to-place marks ----------

function setSignTool(tool: SignTool | null) {
  clearSignToolUi();
  setShapeTool(null);
  clearMarkupAndEraser();
  state.signTool = tool;
  state.stampImagePath = null;
  document.querySelectorAll<HTMLButtonElement>(".signMark").forEach((b) => {
    b.classList.toggle("active", b.dataset.sign === tool);
  });
  document.body.classList.toggle("signMode", tool !== null);
  if (tool) {
    setSignHint(tool === "text" ? t("st.clickToText") : t("st.clickToPlace", { tool: toolLabel(tool) }));
  }
}

function signColor(): [number, number, number] {
  const el = document.getElementById("signColor") as HTMLInputElement | null;
  return hexToRgb01(el?.value ?? "#1a3a8f");
}

// Points-per-pixel of the displayed page (uniform; pixels are square).
function pxPerPt(slot: Slot, pageIdx: number): number {
  const sz = state.pageSizes[pageIdx];
  if (!sz) return 1;
  const bb = slot.wrap.getBoundingClientRect();
  const rot = effectiveRotation(pageIdx);
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  return dw > 0 ? bb.width / dw : 1;
}

function rectFromCornersPx(
  slot: Slot, pageIdx: number,
  x0: number, y0: number, x1: number, y1: number,
): RectPt | null {
  const a = imagePxToPagePt(slot, pageIdx, x0, y0);
  const b = imagePxToPagePt(slot, pageIdx, x1, y1);
  if (!a || !b) return null;
  return {
    x0: Math.min(a.x, b.x), y0: Math.min(a.y, b.y),
    x1: Math.max(a.x, b.x), y1: Math.max(a.y, b.y),
  };
}

function todayString(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()}`;
}

async function placeMarkAt(slot: Slot, pageIdx: number, cx: number, cy: number) {
  const tool = state.signTool;
  if (!tool || !state.currentDoc) return;
  const ppx = pxPerPt(slot, pageIdx);
  const col = signColor();

  if (tool === "text") {
    openNewTextBox(slot, pageIdx, cx, cy);
    return;
  }

  if (tool === "check" || tool === "cross") {
    const S = 16 * ppx; // mark box size in screen px
    const polys: number[][][] = tool === "check"
      ? [[[0, 0.55], [0.4, 1.0], [1.0, 0.0]]]
      : [[[0, 0], [1, 1]], [[1, 0], [0, 1]]];
    const strokes: number[][] = [];
    for (const poly of polys) {
      const s: number[] = [];
      for (const [ux, uy] of poly) {
        const pt = imagePxToPagePt(slot, pageIdx, cx + (ux - 0.5) * S, cy + (uy - 0.5) * S);
        if (pt) s.push(pt.x, pt.y);
      }
      if (s.length >= 4) strokes.push(s);
    }
    if (strokes.length === 0) return;
    await commit("add_ink", {
      page: pageIdx, strokes, color: col, width: 1.8,
      opacity: 1.0, print: state.printAnnotations,
    }, toolLabel(tool));
    return;
  }

  if (tool === "dot") {
    const r = 4 * ppx;
    const rect = rectFromCornersPx(slot, pageIdx, cx - r, cy - r, cx + r, cy + r);
    if (!rect) return;
    await commit("add_shape", {
      page: pageIdx, kind: "circle", rect, stroke: col, fill: col,
      border_width: 1.0, opacity: 1.0, print: state.printAnnotations,
    }, toolLabel("dot"));
    return;
  }

  if (tool === "date") {
    const w = 78 * ppx, h = 16 * ppx;
    const rect = rectFromCornersPx(slot, pageIdx, cx, cy - h / 2, cx + w, cy + h / 2);
    if (!rect) return;
    await commit("add_freetext", {
      page: pageIdx, rect, text: todayString(), color: col,
      bg_color: [1, 1, 1, 0], font_size: 12, font_family: "Helv",
      rotation_quadrant: effectiveRotation(pageIdx), opacity: 1.0,
      print: state.printAnnotations,
    }, toolLabel("date"));
  }
}

async function commit(cmd: string, args: Record<string, unknown>, label: string) {
  setStatus(t("st.adding", { label }));
  try {
    const snap = await invoke<DocSnapshot>(cmd, { document: state.currentDoc, args });
    applyDocSnapshot(snap);
    setStatus(t("st.added", { label }));
  } catch (e) {
    setStatus(t("st.addError", { label, e: String(e) }));
  }
}

// ---------- Create-signature modal ----------

let drawing = false;
let lastPt: { x: number; y: number } | null = null;
let sigDirty = false;

function sigCanvas(): HTMLCanvasElement {
  return document.getElementById("sigCanvas") as HTMLCanvasElement;
}
function sigCtx(): CanvasRenderingContext2D {
  return sigCanvas().getContext("2d")!;
}

function clearSigCanvas() {
  const c = sigCanvas();
  sigCtx().clearRect(0, 0, c.width, c.height);
  sigDirty = false;
}

function setSigTab(tab: "draw" | "type" | "import") {
  document.querySelectorAll<HTMLButtonElement>("[data-sig-tab]").forEach((b) => {
    b.classList.toggle("active", b.dataset.sigTab === tab);
  });
  document.getElementById("sigDrawSection")!.classList.toggle("hidden", tab !== "draw");
  document.getElementById("sigTypeSection")!.classList.toggle("hidden", tab !== "type");
  document.getElementById("sigImportSection")!.classList.toggle("hidden", tab !== "import");
}

function openSignModal(presetTarget?: "signature" | "initials") {
  const overlay = document.getElementById("sigOverlay")!;
  if (presetTarget) {
    (document.getElementById("sigTarget") as HTMLSelectElement).value = presetTarget;
  }
  setSigTab("draw");
  clearSigCanvas();
  overlay.classList.remove("hidden");
}

function closeSignModal() {
  document.getElementById("sigOverlay")!.classList.add("hidden");
}

function canvasPos(ev: PointerEvent): { x: number; y: number } {
  const c = sigCanvas();
  const bb = c.getBoundingClientRect();
  return {
    x: ((ev.clientX - bb.left) / bb.width) * c.width,
    y: ((ev.clientY - bb.top) / bb.height) * c.height,
  };
}

function renderTypedSignature(text: string) {
  const c = sigCanvas();
  const ctx = sigCtx();
  ctx.clearRect(0, 0, c.width, c.height);
  sigDirty = false;
  if (!text.trim()) return;
  ctx.fillStyle = "#101a40";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  let size = 64;
  ctx.font = `italic ${size}px "Segoe Script", "Brush Script MT", cursive`;
  while (size > 18 && ctx.measureText(text).width > c.width - 24) {
    size -= 2;
    ctx.font = `italic ${size}px "Segoe Script", "Brush Script MT", cursive`;
  }
  ctx.fillText(text, c.width / 2, c.height / 2);
  sigDirty = true;
}

function dataUrlToBytes(dataUrl: string): number[] {
  const b64 = dataUrl.split(",")[1] ?? "";
  const bin = atob(b64);
  const out = new Array<number>(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

async function saveDrawnSignature() {
  const kind = (document.getElementById("sigTarget") as HTMLSelectElement).value;
  if (!sigDirty) {
    setStatus(t("st.drawFirst"));
    return;
  }
  const bytes = dataUrlToBytes(sigCanvas().toDataURL("image/png"));
  try {
    await invoke<string>("save_signature", { kind, data: bytes });
    await refreshSignatures();
    closeSignModal();
    setStatus(t("st.sigSaved", { kind: toolLabel(kind) }));
  } catch (e) {
    setStatus(t("st.saveSigError", { e: String(e) }));
  }
}

async function importSignatureFile() {
  const kind = (document.getElementById("sigTarget") as HTMLSelectElement).value;
  const picked = await open({
    multiple: false, directory: false,
    filters: [{ name: t("lbl.images"), extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
    title: t("btn.pickImage"),
  });
  if (!picked || typeof picked !== "string") return;
  try {
    await invoke<string>("import_signature", { kind, srcPath: picked });
    await refreshSignatures();
    closeSignModal();
    setStatus(t("st.sigImported", { kind: toolLabel(kind) }));
  } catch (e) {
    setStatus(t("st.importSigError", { e: String(e) }));
  }
}

// ---------- Digital signatures (PAdES) ----------

let dsignPfxPath: string | null = null;
let winCertsLoaded = false;

function setDsignTab(tab: "pfx" | "win") {
  document.querySelectorAll<HTMLButtonElement>("[data-dsign-tab]").forEach((b) => {
    b.classList.toggle("active", b.dataset.dsignTab === tab);
  });
  document.getElementById("dsignPfxSection")!.classList.toggle("hidden", tab !== "pfx");
  document.getElementById("dsignWinSection")!.classList.toggle("hidden", tab !== "win");
  if (tab === "win" && !winCertsLoaded) void loadWinCerts();
}

async function loadWinCerts() {
  const sel = document.getElementById("dsignWinCert") as HTMLSelectElement;
  const placeholder = (text: string) => {
    const o = document.createElement("option");
    o.value = "";
    o.textContent = text;
    sel.replaceChildren(o);
  };
  placeholder(t("dsign.loading"));
  try {
    const certs = await invoke<WinCertEntry[]>("list_windows_certificates");
    winCertsLoaded = true;
    const usable = certs.filter((c) => c.has_private_key);
    if (usable.length === 0) {
      placeholder(t("st.noSigCerts"));
      return;
    }
    sel.innerHTML = "";
    for (const c of usable) {
      const o = document.createElement("option");
      o.value = c.thumbprint;
      o.textContent = `${c.subject || "(no name)"} — ${c.thumbprint.slice(0, 12)}…`;
      sel.appendChild(o);
    }
  } catch (e) {
    placeholder(`— ${e} —`);
  }
}

function openDsignModal() {
  if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
  setDsignTab("pfx");
  document.getElementById("dsignOverlay")!.classList.remove("hidden");
}

function closeDsignModal() {
  document.getElementById("dsignOverlay")!.classList.add("hidden");
}

function dsignMeta() {
  const val = (id: string) => (document.getElementById(id) as HTMLInputElement).value.trim() || undefined;
  return { reason: val("dsignReason"), name: val("dsignName"), location: val("dsignLocation"), tsa_url: val("dsignTsa") };
}

let signing = false;

async function runDigitalSign() {
  if (!state.currentDoc || signing) return;
  signing = true;
  const commitBtn = document.getElementById("dsignCommit") as HTMLButtonElement;
  commitBtn.disabled = true;
  try {
    await runDigitalSignInner();
  } finally {
    signing = false;
    commitBtn.disabled = false;
  }
}

async function runDigitalSignInner() {
  if (!state.currentDoc) return;
  const doc = state.currentDoc;
  const winTab = document.querySelector("[data-dsign-tab='win']")?.classList.contains("active");
  const meta = dsignMeta();
  setStatus(t("st.signing"));
  try {
    let snap: DocSnapshot;
    if (winTab) {
      const thumbprint = (document.getElementById("dsignWinCert") as HTMLSelectElement).value;
      if (!thumbprint) { setStatus(t("st.pickCert")); return; }
      snap = await invoke<DocSnapshot>("sign_pdf_windows_cert", { document: state.currentDoc, thumbprint, meta });
    } else {
      if (!dsignPfxPath) { setStatus(t("st.pickPfx")); return; }
      const password = (document.getElementById("dsignPfxPass") as HTMLInputElement).value;
      snap = await invoke<DocSnapshot>("sign_pdf_pkcs12", {
        document: state.currentDoc, pfxPath: dsignPfxPath, password, meta,
      });
    }
    if (doc !== state.currentDoc) return;
    applyDocSnapshot(snap);
    closeDsignModal();
    setStatus(t("st.signed"));
    await runVerify();
  } catch (e) {
    if (String(e).includes("PROTECTED_DOC")) {
      setStatus(t("st.signProtected"));
    } else {
      setStatus(t("st.signError", { e: String(e) }));
    }
  }
}

function renderReports(reports: SignatureReport[]) {
  const body = document.getElementById("dverifyBody")!;
  if (reports.length === 0) {
    body.innerHTML = `<div class='sigEmpty'>${t("verify.empty")}</div>`;
    return;
  }
  body.innerHTML = "";
  for (const r of reports) {
    const card = document.createElement("div");
    card.className = `sigReport ${r.status}`;
    const badge = r.status === "valid" ? t("verify.badgeValid")
      : r.status === "invalid" ? t("verify.badgeInvalid") : t("verify.badgeUnsupported");
    const row = (label: string, v: string) =>
      `<div class="sigRow"><b>${escapeHtml(label)}:</b> ${escapeHtml(v)}</div>`;
    card.innerHTML =
      `<span class="sigBadge">${escapeHtml(badge)}</span>` +
      row(t("verify.signer"), r.signer_cn ?? "—") +
      row(t("verify.signed"), r.signing_time ?? "—") +
      row(t("verify.coversWhole"), r.covers_whole_document ? t("verify.yes") : t("verify.coversNo")) +
      row(t("verify.timestamp"), r.has_timestamp ? t("verify.tsYes") : t("verify.no")) +
      row(t("verify.detail"), r.detail);
    body.appendChild(card);
  }
}

async function runVerify() {
  if (!state.currentDoc) { setStatus(t("st.openFirst")); return; }
  setStatus(t("st.verifying"));
  try {
    const reports = await invoke<SignatureReport[]>("verify_signatures", { document: state.currentDoc });
    renderReports(reports);
    document.getElementById("dverifyOverlay")!.classList.remove("hidden");
    const valid = reports.filter((r) => r.status === "valid").length;
    setStatus(reports.length === 0 ? t("st.noSignatures") : t("st.sigsValid", { valid, total: reports.length }));
  } catch (e) {
    setStatus(t("st.verifyError", { e: String(e) }));
  }
}

function initDigitalSign() {
  document.getElementById("dsignBtn")!.addEventListener("click", openDsignModal);
  document.getElementById("dverifyBtn")!.addEventListener("click", () => void runVerify());
  document.getElementById("dsignClose")!.addEventListener("click", closeDsignModal);
  document.getElementById("dsignCancel")!.addEventListener("click", closeDsignModal);
  document.getElementById("dverifyClose")!.addEventListener("click", () =>
    document.getElementById("dverifyOverlay")!.classList.add("hidden"));
  document.querySelectorAll<HTMLButtonElement>("[data-dsign-tab]").forEach((b) => {
    b.addEventListener("click", () => setDsignTab(b.dataset.dsignTab as "pfx" | "win"));
  });
  document.getElementById("dsignPfxPick")!.addEventListener("click", async () => {
    const picked = await open({
      multiple: false, directory: false,
      filters: [{ name: "PKCS#12", extensions: ["pfx", "p12"] }],
      title: t("ph.pickPfx"),
    });
    if (picked && typeof picked === "string") {
      dsignPfxPath = picked;
      (document.getElementById("dsignPfxPath") as HTMLInputElement).value = picked;
    }
  });
  document.getElementById("dsignCommit")!.addEventListener("click", () => void runDigitalSign());
}

// ---------- Init ----------

export function initSign() {
  initDigitalSign();
  void refreshSignatures();

  document.getElementById("signSignatureBtn")!.addEventListener("click", () => void armSignature("signature"));
  document.getElementById("signInitialsBtn")!.addEventListener("click", () => void armSignature("initials"));
  document.getElementById("signManageBtn")!.addEventListener("click", () => openSignModal());

  document.querySelectorAll<HTMLButtonElement>(".signMark").forEach((b) => {
    b.addEventListener("click", () => {
      const t = b.dataset.sign as SignTool;
      setSignTool(state.signTool === t ? null : t);
    });
  });

  // Click-to-place: a plain click (no drag) on a page while a mark tool is armed.
  document.addEventListener("click", (ev) => {
    if (!state.signTool || !state.currentDoc) return;
    if (state.lastWasDrag) return;
    if (isOverlayControl(ev.target)) return;
    const hit = findSlotFromTarget(ev.target);
    if (!hit) return;
    const bb = hit.slot.wrap.getBoundingClientRect();
    void placeMarkAt(hit.slot, hit.idx, ev.clientX - bb.left, ev.clientY - bb.top);
  });

  // Signature modal
  document.querySelectorAll<HTMLButtonElement>("[data-sig-tab]").forEach((b) => {
    b.addEventListener("click", () => {
      const tab = b.dataset.sigTab as "draw" | "type" | "import";
      setSigTab(tab);
      if (tab !== "type") clearSigCanvas();
    });
  });
  document.getElementById("sigClose")!.addEventListener("click", closeSignModal);
  document.getElementById("sigCancel")!.addEventListener("click", closeSignModal);
  document.getElementById("sigClear")!.addEventListener("click", () => {
    const tt = document.getElementById("sigTypeText") as HTMLInputElement;
    tt.value = "";
    clearSigCanvas();
  });
  document.getElementById("sigSave")!.addEventListener("click", () => void saveDrawnSignature());
  document.getElementById("sigImportBtn")!.addEventListener("click", () => void importSignatureFile());
  document.getElementById("sigTypeText")!.addEventListener("input", (e) => {
    renderTypedSignature((e.target as HTMLInputElement).value);
  });

  const c = sigCanvas();
  c.addEventListener("pointerdown", (ev) => {
    drawing = true;
    lastPt = canvasPos(ev);
    c.setPointerCapture(ev.pointerId);
  });
  c.addEventListener("pointermove", (ev) => {
    if (!drawing || !lastPt) return;
    const p = canvasPos(ev);
    const ctx = sigCtx();
    ctx.strokeStyle = "#101a40";
    ctx.lineWidth = 2.5;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(lastPt.x, lastPt.y);
    ctx.lineTo(p.x, p.y);
    ctx.stroke();
    lastPt = p;
    sigDirty = true;
  });
  const endDraw = () => { drawing = false; lastPt = null; };
  c.addEventListener("pointerup", endDraw);
  c.addEventListener("pointerleave", endDraw);
}
