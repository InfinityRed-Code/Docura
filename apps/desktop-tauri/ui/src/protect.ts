import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";
import { state } from "./state";
import { setStatus } from "./dom";
import { triggerSaveAs } from "./edit";

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
  void refreshProtectUi();
  // Protection only takes effect on disk — offer the save dialog right away.
  await triggerSaveAs();
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
  void refreshProtectUi();
  await triggerSaveAs();
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

  // Keep the protect ribbon state in sync with the open document.
  document.addEventListener("rustpdf:doc-changed", () => void refreshProtectUi());
  void refreshProtectUi();
}
