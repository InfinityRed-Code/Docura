# rustpdf

A native Windows PDF reader, editor, and print-to-PDF replacement, built in Rust
+ Tauri 2. Targets parity with PDF24, Foxit Reader, and Adobe Acrobat for the
everyday workflows (open, read, organise, annotate, fill forms, print, export to
PDF/A). Linux-ready: the core is OS-agnostic and ships behind traits so the same
binary plan applies once the Linux PDFium binary is in place.

Current status: **v0.6.2** — Reader, page organisation, watermarking, virtual
printer, annotations, AcroForm fill, and PDF/A export are all shipped. Roadmap
through v1.1 lives in [`docs/roadmap.md`](docs/roadmap.md).

## What's in the box today

- **Reader.** Open via dialog, drop, recent files, or the virtual printer.
  Continuous scroll, single page, two-page spread, fit-width/fit-page, rotate
  (per page / whole document), dark/invert mode, fullscreen, presentation mode
  (F5).
- **Navigation.** Thumbnail strip, outline / bookmarks, attachments panel,
  history back/forward via internal links, external link opener, Ctrl+G goto,
  Ctrl+F find (current page + whole document with hit markers), text selection
  and copy.
- **Organise.** Delete, extract, rotate, reorder (drag), insert from another
  PDF, merge multiple PDFs, split by page ranges, undo/redo across all
  organise operations. Watermark (text + image), header/footer + page numbers.
- **Annotate.** Highlight, underline, strikeout, sticky notes, free-hand ink,
  shapes (rect / circle / line / arrow), stamps (image), free text. Comment
  panel with thread + filters. XFDF import/export. Print/no-print flag per
  annotation.
- **Forms.** AcroForm field detection and inline rendering. Text, checkbox,
  radio, combobox, listbox fill. Required-field validation. Form reset.
  Signature-field placeholders (cryptography not yet wired).
- **Export.** Save as plain PDF, save as PDF/A-1b/2b/3b/4 via Ghostscript,
  batch convert PDFs to PDF/A from a tray.
- **Print to PDF.** Windows virtual printer "rustpdf" (auto-installed PS class
  driver + Ghostscript pipeline). Jobs dropped into a watched spool folder land
  back in the reader automatically.
- **Performance.** Async render pool (no UI freeze on large pages), LRU page
  cache keyed by zoom, lazy thumbnails, eviction of far-from-viewport slots.

See `docs/roadmap.md` for the full feature inventory and what's still open
(form-field JS calculations, digital signatures, redaction, format conversion,
OCR, polish).

## Layout

```
apps/desktop-tauri/
  src-tauri/         # Rust backend, Tauri commands, virtual printer service
  ui/                # TypeScript frontend (modular under src/)

crates/
  pdf_types          # Shared leaf types (DocumentId, PageSize, ...)
  pdf_core           # Domain traits, viewer orchestration (UI/OS-free)
  pdf_pdfium         # PdfEngine impl via pdfium-render
  pdf_services       # Platform service traits (dialog, fs, opener, settings)
  pdf_cache          # LRU render cache
  pdf_mutate         # Document mutation via lopdf (organise, watermark,
                     # annotations, forms, XFDF)

vendor/pdfium/       # Bundled PDFium binaries (per-target)
docs/                # Architecture and roadmap
```

UI is split into ~10 cohesive modules under `apps/desktop-tauri/ui/src/`
(`types`, `state`, `dom`, `template`, `viewer`, `doc`, `edit`, `annot`,
`chrome`, `main`). State is a single shared object; cross-module hooks go
through a small callback registry to keep the import graph acyclic.

## Architecture rules

1. `pdf_core` has no UI or OS dependencies — it talks to engines and services
   via traits in `pdf_core` / `pdf_services`.
2. Filesystem, dialogs, printer, cert-store, and settings live behind traits
   in `pdf_services`. Implementations are in the Tauri crate.
3. Document mutation (delete pages, watermark, annotations, form fill, ...)
   goes through `pdf_mutate` on top of `lopdf`. PDFium is render + text +
   forms only.
4. Anything > 16 ms runs in `spawn_blocking` or a dedicated worker. The UI
   thread never blocks on PDFium.
5. PDFium ships beside the executable. The app loads `vendor/pdfium/<target>/`
   in dev and the bundled resource in release.

Full detail in [`docs/architecture.md`](docs/architecture.md).

## Prerequisites

- Rust (rustup; the workspace pins via `rust-toolchain.toml`)
- Node.js LTS + pnpm
- Git
- Windows: MSVC C++ Build Tools + Windows SDK + WebView2 runtime
- Ghostscript (optional, only required for PDF/A export and the virtual
  printer). The settings dialog detects it and links to the installer.

## Build

Workspace typecheck:

```
cargo check --workspace
```

UI typecheck:

```
cd apps/desktop-tauri/ui
pnpm install
pnpm exec tsc --noEmit
```

Run the dev app (Tauri shell + Vite dev server with HMR):

```
cd apps/desktop-tauri
./ui/node_modules/.bin/tauri dev
```

The Tauri CLI must be invoked from `apps/desktop-tauri/` (not `ui/`); it only
looks in subfolders of cwd for `tauri.conf.json`. Incremental rebuilds are
≈10 s; cold builds ≈1–2 min.

Release bundle (NSIS installer):

```
cd apps/desktop-tauri
./ui/node_modules/.bin/tauri build
```

The installer ships `pdfium.dll` as a bundle resource and offers to register
the rustpdf virtual printer.

## Versioning

See [`CHANGELOG.md`](CHANGELOG.md). The workspace version, the Tauri bundle
version, and the latest git tag track each other.

## Licensing

This project is **source-available**, not OSI open source: the code is public,
but use is restricted by field. It is dual-licensed:

- **Noncommercial use — free.** Released under the
  [PolyForm Noncommercial License 1.0.0](LICENSE). Private individuals,
  hobbyists, students, researchers, charities, schools, and other
  noncommercial organisations may use, modify, and redistribute it at no cost.
- **Commercial use — paid license required.** Any use by or on behalf of a
  for-profit company (as defined under "Your company" in the PolyForm terms)
  requires a separate commercial license.

  > **Commercial licensing contact:** _TODO — add contact / purchase URL._

Note on terminology: because the PolyForm Noncommercial License restricts
commercial use, it does **not** meet the Open Source Definition (OSD §6, "No
Discrimination Against Fields of Endeavor"). Describe the project as
*source-available*, not *open source*.

### Third-party licenses

Bundled and dependency components keep their own permissive licenses, which are
compatible with the above: PDFium is Apache-2.0 / BSD-3-Clause; lopdf,
pdfium-render, image, lru and the other workspace crates are MIT / Apache-2.0.
These permissive upstream licenses impose no restriction on the project's own
noncommercial/commercial dual-licensing.
