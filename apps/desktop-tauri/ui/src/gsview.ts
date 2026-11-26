import type { GhostscriptInfo } from "./types";

/** What Settings › PDF/A export shows for a Ghostscript detection result. */
export type GhostscriptView = {
  /** i18n key of the status line. */
  key: "gs.found" | "gs.notFound";
  params: Record<string, string>;
  ok: boolean;
  /** The "Install from ghostscript.com" hint, only when Ghostscript is missing. */
  showInstall: boolean;
};

export function ghostscriptView(info: GhostscriptInfo): GhostscriptView {
  if (info.available) {
    return {
      key: "gs.found",
      params: { version: info.version ?? "", path: info.path ?? "" },
      ok: true,
      showInstall: false,
    };
  }
  return { key: "gs.notFound", params: {}, ok: false, showInstall: true };
}
