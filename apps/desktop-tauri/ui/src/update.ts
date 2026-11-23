// Update hint (#18). The backend (update.rs) asks GitHub for the latest
// release, at most once a day at startup and on demand from the settings;
// a newer version shows a small closable card with a download button that
// opens the setup in the browser.
import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  available: boolean;
  notes: string;
  downloadUrl: string;
  releaseUrl: string;
}

interface UpdateSettings {
  enabled: boolean;
  currentVersion: string;
}

/** Startup check delay, so it never competes with opening a document. */
const STARTUP_DELAY_MS = 5000;

const el = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

let shown: UpdateInfo | null = null;
let currentVersion = "";

function showHint(info: UpdateInfo) {
  shown = info;
  el("updateHintTitle").textContent = t("upd.title", { version: info.latestVersion });
  el("updateHintInstalled").textContent = t("upd.installed", { version: info.currentVersion });
  el("updateHintNotes").textContent = info.notes;
  el("updateHint").classList.remove("hidden");
}

function hideHint() {
  el("updateHint").classList.add("hidden");
}

function setStatus(text: string) {
  el("updateStatus").textContent = text;
}

function download() {
  if (shown) void invoke("open_external", { url: shown.downloadUrl }).catch(() => {});
}

async function checkNow() {
  const btn = el<HTMLButtonElement>("updateCheckBtn");
  const dl = el("updateDownloadBtn");
  btn.disabled = true;
  dl.classList.add("hidden");
  setStatus(t("upd.checking"));
  try {
    const info = await invoke<UpdateInfo | null>("check_for_update", { manual: true });
    if (info?.available) {
      setStatus(t("upd.found", { version: info.latestVersion }));
      // The card waits behind the dialog; the dialog gets its own button.
      showHint(info);
      dl.classList.remove("hidden");
    } else {
      setStatus(t("upd.upToDate", { version: info?.currentVersion ?? currentVersion }));
    }
  } catch {
    setStatus(t("upd.failed"));
  } finally {
    btn.disabled = false;
  }
}

export function initUpdates() {
  el("updateHintDownload").addEventListener("click", download);
  el("updateDownloadBtn").addEventListener("click", download);
  el("updateHintRelease").addEventListener("click", () => {
    if (shown) void invoke("open_external", { url: shown.releaseUrl }).catch(() => {});
  });
  el("updateHintClose").addEventListener("click", () => {
    if (shown) void invoke("dismiss_update", { version: shown.latestVersion }).catch(() => {});
    hideHint();
  });

  const auto = el<HTMLInputElement>("updateAutoCheck");
  void invoke<UpdateSettings>("get_update_settings")
    .then((s) => {
      auto.checked = s.enabled;
      currentVersion = s.currentVersion;
      setStatus(t("upd.current", { version: s.currentVersion }));
    })
    .catch(() => {});
  auto.addEventListener("change", () => {
    void invoke("set_update_check", { enabled: auto.checked }).catch(() => {
      auto.checked = !auto.checked;
    });
  });
  el("updateCheckBtn").addEventListener("click", () => void checkNow());

  // At most once a day and only when enabled (decided by the backend);
  // silent when offline or when GitHub answers with an error.
  window.setTimeout(() => {
    void invoke<UpdateInfo | null>("check_for_update", { manual: false })
      .then((info) => {
        if (info?.available) showHint(info);
      })
      .catch(() => {});
  }, STARTUP_DELAY_MS);
}
