# Architecture

Windows-first PDF desktop app, Linux-ready. Rust workspace with a Tauri 2
shell. The design intentionally keeps the core UI- and OS-agnostic so the
same logic can run under a future Linux build without structural churn.

## Layers

```
┌────────────────────────────────────────────────────────────┐
│ apps/desktop-tauri                                         │
│   src-tauri (Rust)   ui (TS + Vite, WebView2 on Windows)   │
│   - wires commands, plugins, window lifecycle              │
│   - implements platform services (dialog, fs, opener)      │
└──────────────▲─────────────────────────────────────────────┘
               │ traits from pdf_services
               │
┌──────────────┴─────────────────────────────────────────────┐
│ pdf_services                                               │
│   FileDialog, FileSystem, Opener, Settings (traits only)   │
└──────────────▲─────────────────────────────────────────────┘
               │
┌──────────────┴─────────────────────────────────────────────┐
│ pdf_core                                                   │
│   PdfEngine trait, ViewerState, search orchestration       │
│   no Tauri / no OS calls / no frontend types               │
└──────────────▲─────────────────────────────────────────────┘
               │ implemented by
               │
┌──────────────┴─────────────────────────────────────────────┐
│ pdf_pdfium                                                 │
│   Concrete engine via pdfium-render + runtime PDFium bind  │
└────────────────────────────────────────────────────────────┘

pdf_types   ← shared leaf types (DocumentId, PageIndex, ZoomLevel, ...)
pdf_cache   ← thumbnail / render LRU used by core or UI
```

## Layer rules

1. **No UI framework in the core.** `pdf_core` must not depend on Tauri,
   Slint, Iced, or any frontend crate.
2. **No OS paths or APIs in the core.** Filesystem work goes through
   `pdf_services` traits.
3. **All OS functionality sits behind traits.** Implementations live in
   the app crate; the core consumes the trait.
4. **PDF rendering stabilises before UI polish.** Render correctness first,
   visual refinement later.
5. **Bundle PDFium from day one.** `pdfium-render` binds at runtime — the
   app must ship `pdfium.dll` / `libpdfium.so` beside the executable, or
   fall back to the system library.
6. **Every new feature belongs to one layer:** core, service, or UI.
   Cross-layer leakage is a red flag in review.
7. **Async work stays off the UI thread.** Rendering, thumbnail generation
   and search run in blocking tasks via `tokio::task::spawn_blocking` (or
   a dedicated render pool); the UI only awaits results.

## Portability notes

- Windows uses WebView2 for the Tauri web view; Linux uses WebKitGTK 4.1.
  Neither is referenced from `pdf_core`.
- Path handling inside the core uses `DocumentId`, not filesystem paths.
  Paths enter at the app boundary and are converted immediately.
- PDFium binaries are platform-specific. See `vendor/pdfium/README.md`.

## Phase map

| Phase | Scope                                                                |
|-------|----------------------------------------------------------------------|
| 1     | Workspace + `pdf_types` / `pdf_core` / `pdf_pdfium` + open/render/extract |
| 2     | Tauri shell, UI skeleton, dialog/fs/opener plugins                   |
| 3     | MVP viewer: open, page count, render, zoom, paging, thumbnails, text search |
