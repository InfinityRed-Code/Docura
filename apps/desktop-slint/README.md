# desktop-slint (experiment: `test-slint` branch)

A native [Slint](https://slint.dev) frontend for Docura PDF, replacing the
Tauri + Vite (webview) UI in `apps/desktop-tauri`. The point of this branch is
to evaluate dropping the webview/IPC layer and driving the existing Rust
engine crates directly.

## Why

The Tauri app ships a Chromium/WebView2 runtime and marshals every operation
over JSON IPC between TypeScript and Rust. The whole backend already lives in
pure-Rust crates (`pdf_core`, `pdf_pdfium`, `pdf_mutate`, `pdf_sign`,
`pdf_export`, …). A Slint UI calls those functions in-process — no IPC, no
webview, one language, smaller binary.

## Status — foundation only

Implemented:
- Window, toolbar, status bar (`ui/app.slint`).
- Open a PDF via a native file dialog (`rfd`).
- Render the current page with `pdf_pdfium` and display it (`slint::Image`
  from the engine's RGBA buffer).
- Page navigation (prev/next) and zoom (in/out/reset).

Not yet ported (all of these already exist as `pdf_*` crate APIs the Tauri
commands call — they need Slint UI + glue, not new engine work):
- Continuous scroll / thumbnails / outline / search.
- Text selection + highlight/annotation tools, eraser.
- OCR, signing, forms, encryption, redaction, watermarks, export.
- Undo/redo, page management (delete/insert/rotate/reorder), merge/split.

## Run

```
# from the workspace root (so ./vendor/pdfium/pdfium.dll is found)
cargo run -p desktop-slint
```

## Layout

- `ui/app.slint` — declarative UI.
- `build.rs` — compiles the `.slint` file.
- `src/main.rs` — engine wiring and callbacks.

The Tauri app under `apps/desktop-tauri` is untouched and remains the shipping
frontend on `main`.
