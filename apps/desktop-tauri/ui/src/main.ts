import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { initI18n } from "./i18n";
import { applyTemplate } from "./template";
import { initDom } from "./dom";
import { refreshIcons } from "./icons";
import { state } from "./state";
import { initViewer, updateChrome, goToPage } from "./viewer";
import { initDoc, openByPath } from "./doc";
import { initEdit, addMergePaths } from "./edit";
import { initAnnot } from "./annot";
import { initTextEdit } from "./textedit";
import { initSign } from "./sign";
import { initProtect } from "./protect";
import { initExport } from "./export";
import { initOcr } from "./ocr";
import { initChrome } from "./chrome";
import { initColorPopover } from "./colorpicker";
import { initFonts } from "./fonts";
import { initUpdates } from "./update";
import { dom } from "./dom";

// 0. Resolve language (German default) before the template renders.
initI18n();

// 1. Build DOM + capture refs before any module touches the page.
applyTemplate();
initDom();
refreshIcons();

// 2. Wire each subsystem. Order matters: annot registers callbacks consumed
// by doc.applyDocSnapshot, so it must run before any snapshot fires.
initColorPopover();
void initFonts();
initViewer();
initAnnot();
initTextEdit();
initSign();
initDoc();
initEdit();
initProtect();
initExport();
initOcr();
initChrome();
initUpdates();

// 3. Thumb-click goto bridge — edit.ts dispatches this when a thumb is clicked
// in the Reader tab (avoids a viewer<->edit import cycle).
document.addEventListener("rustpdf:goto-page", (ev) => {
  const detail = (ev as CustomEvent<{ page: number }>).detail;
  goToPage(detail.page);
});

// 4. Native window drag-and-drop → open dropped PDF (or add to merge tray).
getCurrentWebview()
  .onDragDropEvent(async (event) => {
    const p = event.payload;
    if (p.type === "over" || p.type === "enter") {
      if (state.mergeMode) document.body.classList.add("mergeDrop");
      else dom.dropOverlay.classList.remove("hidden");
    } else if (p.type === "leave") {
      document.body.classList.remove("mergeDrop");
      dom.dropOverlay.classList.add("hidden");
    } else if (p.type === "drop") {
      document.body.classList.remove("mergeDrop");
      dom.dropOverlay.classList.add("hidden");
      const paths = (p as { paths: string[] }).paths;
      if (state.mergeMode) {
        addMergePaths(paths);
        return;
      }
      const pdf = paths.find((f) => /\.pdf$/i.test(f));
      if (pdf) await openByPath(pdf);
    }
  })
  .catch(() => {});

// 5. Initial chrome paint.
updateChrome();

// 6. Opened via file association / "open with" / drag onto the exe?
// The backend holds the CLI path argument; consume it once.
void invoke<string | null>("take_startup_file")
  .then((p) => {
    if (p) return openByPath(p);
  })
  .catch(() => {});
