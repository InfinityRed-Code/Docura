# Docura PDF

A native Windows PDF reader, editor, and print-to-PDF replacement, built in Rust
+ Tauri 2. Targets parity with PDF24, Foxit Reader, and Adobe Acrobat for the
everyday workflows: open, read, organise, annotate, fill forms, sign, protect,
redact, OCR, and convert. Linux-ready: the core is OS-agnostic and ships behind
traits so the same binary plan applies once the Linux PDFium binary is in place.

> The application is branded **Docura PDF** (`com.docura.pdf`). The repository,
> crates, and internal keys stay named `rustpdf` for historical reasons.

Current status: **v1.3.2** — milestones v0.1 through v1.0 (Reader, Navigation,
Organise, Export & Virtual printer, Annotation, Forms, Fill & Sign, Security &
Redaction, Export & Conversion, OCR) are shipped; v1.1 "Performance & Polish"
is largely done (simplified UI, render pool, caches, tray, accessibility —
open: progressive rendering, MSI installer, auto-update, crash reporting). The
full inventory lives in [`docs/roadmap.md`](docs/roadmap.md).

## What's in the box today

- **Simple, task-oriented UI.** Seven ribbon tabs — Start, Kommentieren,
  Seiten, Bearbeiten, Ausfüllen & Signieren, Konvertieren, Schützen — with
  captioned groups; a status bar for messages, page navigation and zoom; a
  welcome screen with "Open PDF", recent files and quick conversions. Save
  (Ctrl+S) / Save as with an unsaved-changes prompt before closing or opening
  another file.
- **Reader.** Open via dialog, drop, recent files, file association, or the
  virtual printer. Continuous scroll, single page, two-page spread,
  fit-width/fit-page, rotate (per page / whole document), dark/invert mode,
  fullscreen, presentation mode. Configurable default zoom and render-cache
  budget in Settings.
- **Navigation.** Thumbnail strip, outline / bookmarks, attachments panel,
  history back/forward via internal links, external link opener, page box in
  the status bar (Ctrl+G), Ctrl+F find (current page + whole document with hit
  markers), text selection and copy.
- **Organise.** Page grid with multi-select: delete, extract, rotate
  (persistent), reorder (drag), insert from another PDF, merge multiple PDFs,
  split every N pages or by page ranges, undo/redo across all organise
  operations. Watermark (text + image), header/footer + page
  numbers.
- **Annotate.** Highlight, underline, strikeout, squiggly, sticky notes, free-hand ink,
  shapes (rect / circle / line / arrow), stamps (image), free text. Comment
  panel with thread + filters. XFDF import/export. Print/no-print flag per
  annotation.
- **Forms.** AcroForm field detection and inline rendering. Text, checkbox,
  radio, combobox, listbox fill. Required-field validation. Form reset. Save
  filled. (Field JS calculations are still open.)
- **Fill & Sign.** Draw / type / import a signature, place date / check / cross
  stamps and free text by click. Cryptographic digital signatures (PKCS#7/CMS,
  PAdES-B-B) from a PKCS#12 file or the Windows keystore (CNG), RFC 3161
  timestamping to PAdES-B-T, and signature verification (ByteRange digest, CMS,
  signer CN, coverage) — all via the pure-RustCrypto `pdf_sign` crate.
- **Security & redaction.** Open password-protected PDFs (user + owner),
  protect with AES-256, set permissions. True redaction (content removed, not
  just covered), pattern redaction (email / phone / custom regex), metadata
  sanitize, and a hidden-content scanner (layers, comments, invisible text).
- **Export & convert.** PDF → PNG/JPG, plain text, HTML, Word (.docx,
  flow-reconstructed), Excel (.xlsx, table heuristic); images → PDF; Office
  files → PDF via headless LibreOffice. Save as PDF/A-1b/2b/3b/4 via
  Ghostscript, batch convert from a tray.
- **OCR.** Tesseract-CLI OCR (parallelised across pages) with an invisible text
  layer over the image, UI-driven language packs (`tessdata_fast`), image
  preprocessing (deskew / despeckle / contrast), batch OCR, and an auto-detect
  banner for text-less scans.
- **Print to PDF.** Windows virtual printer (auto-installed PS class driver +
  Ghostscript pipeline). Jobs dropped into a watched spool folder land back in
  the reader automatically. Optional system-tray / autostart mode keeps the
  spool watcher running in the background without an open window.
- **3D PDFs.** 3D annotations with a U3D model (as written by Tekla, Allplan,
  MeshLab, LaTeX media9 and others) open in the page as an interactive view:
  orbit / pan / zoom, the file's saved views, render modes, lighting schemes,
  model tree with visibility, a section plane, fullscreen. Files without a
  poster get a rendered preview. Insert a 3D model from glTF/GLB, OBJ (+MTL),
  STL, PLY or 3MF: it is written as U3D with seven preset views and a poster.
  PRC models show their poster with a hint and can be exported as raw data.
- **Performance.** Bounded, viewport-prioritised render pool (pages encode while
  the next rasterises; no UI freeze on large pages), byte-budgeted LRU page
  cache keyed by zoom, lazy thumbnails, eviction of far-from-viewport slots.

See `docs/roadmap.md` for the full feature inventory and what's still open
(form-field JS calculations, PDF → PowerPoint, and the v1.1 performance/polish
items).

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
  pdf_cache          # Byte-budgeted LRU render cache
  pdf_mutate         # Document mutation via lopdf (organise, watermark,
                     # annotations, forms, XFDF, encryption, redaction, OCR layer)
  pdf_sign           # PAdES digital signatures (pure RustCrypto)
  pdf_export         # PDF → Word/Excel/text/HTML export
  pdf_3d             # U3D (ECMA-363) decoder and writer for 3D annotations

vendor/pdfium/       # Bundled PDFium binaries (per-target)
docs/                # Architecture and roadmap
```

UI is split into cohesive modules under `apps/desktop-tauri/ui/src/`
(`types`, `state`, `dom`, `template`, `viewer`, `doc`, `edit`, `annot`,
`chrome`, `sign`, `protect`, `export`, `ocr`, `threed*`, `i18n`, `main`). State is a single
shared object; cross-module hooks go through a small callback registry to keep
the import graph acyclic. The interface is localised (German default + English).

## Architecture rules

1. `pdf_core` has no UI or OS dependencies — it talks to engines and services
   via traits in `pdf_core` / `pdf_services`.
2. Filesystem, dialogs, printer, cert-store, and settings live behind traits
   in `pdf_services`. Implementations are in the Tauri crate.
3. Document mutation (delete pages, watermark, annotations, form fill,
   encryption, redaction, OCR layer, ...) goes through `pdf_mutate` on top of
   `lopdf`. PDFium is render + text + forms only.
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
- Ghostscript (optional — PDF/A export and the virtual printer). The settings
  dialog detects it and links to the installer.
- Tesseract (OCR). The Windows installer bundles it (UB-Mannheim build with
  `deu` + `eng`); `just tesseract` stages it into `vendor/tesseract` and needs
  7-Zip. At runtime the app prefers the bundled copy (debug builds: the staged
  one), then the `TESSERACT` env var, PATH, or a standard install.
- LibreOffice (optional — Office → PDF conversion via `soffice --headless`).

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
just build
```

or by hand:

```
powershell -ExecutionPolicy Bypass -File scripts/fetch-tesseract.ps1
cd apps/desktop-tauri
./ui/node_modules/.bin/tauri build
```

The installer installs per machine (`C:\Program Files\Docura PDF`) and asks
for administrator rights. It ships `pdfium.dll` as a bundle resource and
Tesseract in `<install dir>\tesseract` (the build stops if
`vendor/tesseract` is not staged), and registers the "Docura PDF" virtual
printer via `nsis/printer-setup.ps1` (log:
`%ProgramData%\DocuraPDF\printer-setup.log`; a failure is shown, not
swallowed). An older per-user installation is removed first, app data is
kept. Hooks: `apps/desktop-tauri/src-tauri/nsis/hooks.nsi`.

### Updates

There is one setup per release for new installations and updates alike.
Run over an installed copy it updates it in place, no uninstall needed: the
welcome page says which version is updated to which, the program files and
the Tesseract folder are replaced, and the install directory, settings and
saved signatures (`%APPDATA%\com.docura.pdf`), shortcuts, autostart and the
virtual printer stay (the printer step only checks and repairs it). A running
Docura PDF is closed after asking. The same version installs again in place
(repair); an older version (downgrade) asks before it replaces a newer one.
Ghostscript is only offered at the first installation.

Unattended: `"Docura PDF_<version>_x64-setup.exe" /S` updates or installs
without any dialog and closes a running app without asking; add `/R` to start
the app afterwards. Exit code 0 means success.

The app looks for a newer release at startup, at most once a day
(`src-tauri/src/update.rs`, GitHub `releases/latest`), and on "Check for
updates" in Settings → Updates, where the startup check can also be turned
off. A newer version shows a hint whose "Download" opens the release's setup
asset (`*_x64-setup.exe`, else the release page) in the browser, so every
release needs that asset and a `v<version>` tag. Debug builds accept
`DOCURA_FAKE_VERSION=1.3.0` (pretend an older app) and `DOCURA_UPDATE_URL`
(another endpoint) to try it in `tauri dev`; release builds ignore both.

Coming from 1.3.0 or older (installed per user): the first per-machine setup
looks like a new installation (directory page), removes the per-user copy
and keeps its app data and autostart; later updates are in place.
Coming from 1.3.1: it has no update hint yet, so 1.3.2 is installed once by
hand; its setup updates 1.3.1 in place.

The in-place update lives in `nsis/installer.nsi`, a copy of the Tauri CLI's
NSIS template (`bundle.windows.nsis.template`): the stock template shows an
"Already Installed" page that preselects "Uninstall before installing" for
every newer version. Its changes are marked `Docura:`; after bumping
`@tauri-apps/cli`, re-apply them to the new upstream template (a test fails
until the version line matches `ui/pnpm-lock.yaml`).

### Publishing a release

Every GitHub release whose installer ships the Tesseract bundle must also
carry its source archive: the installed `tesseract\THIRD-PARTY-NOTICES.txt`
promises it there (GPL/LGPL source offer for JBIG-KIT and GNU libiconv,
plus Tesseract, Leptonica, LibTIFF and the other libraries). It is not part
of the installer.

1. Bump the version in `Cargo.toml` (workspace) and
   `apps/desktop-tauri/src-tauri/tauri.conf.json` together (a test checks
   it): the setup compares it with the installed version to tell an update
   from a repair or a downgrade.
2. Move the `[Unreleased]` notes in `CHANGELOG.md` under the new version.
3. Build and publish the one setup (new installations and updates) with the
   source archive:

   ```
   just release          # = just build + just tesseract-sources
   gh release create v<version> -R InfinityRed-Code/rustpdf \
     "target/release/bundle/nsis/Docura PDF_<version>_x64-setup.exe" \
     dist/docura-pdf-tesseract-sources-5.4.0.20240606.zip
   ```

   The release notes are that changelog section plus:

   > ### Install
   > Download `Docura.PDF_<version>_x64-setup.exe` below and run it
   > (Windows 10/11, x64). It installs for all users and asks for
   > administrator rights. An installed version is updated in place, there
   > is no need to uninstall it first; settings and signatures are kept.

For an existing release: `gh release upload v<version>
dist/docura-pdf-tesseract-sources-5.4.0.20240606.zip`. The archive only
changes with the Tesseract bundle, so the same file is attached again for
each release. `scripts/fetch-tesseract-sources.ps1` builds it
(~75 MB, every input pinned by SHA-256; `README.txt` and `SHA256SUMS.txt`
inside). When bumping Tesseract, update `fetch-tesseract.ps1`,
`fetch-tesseract-sources.ps1`, `tesseract-THIRD-PARTY-NOTICES.txt` and
`tesseract-licenses/` together; `fetch-tesseract.ps1` refuses to stage if
the versions disagree. The offer requires the archive to stay downloadable
for at least three years after the last release that ships the bundle.

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

The 3D support uses three.js (MIT, with the bundled fflate, MIT) for the view
and the model import, and its progressive mesh decoder
(`crates/pdf_3d/src/u3d/clod.rs`) is a Rust port of the decoder in Intel's
Universal 3D Sample Software (Apache-2.0). Notices and license texts are in
[`crates/pdf_3d/licenses/`](crates/pdf_3d/licenses/README.md) and ship with
the installer under `licenses/3d/`.

The Windows installer also ships Tesseract OCR in `<install dir>\tesseract`
as a separate program the app only starts; it is not covered by the
PolyForm / commercial license above. The folder holds the unmodified
UB-Mannheim build with the DLLs it loads, mostly permissively licensed;
`libjbig-0.dll` is GPL-2.0-or-later and `libiconv-2.dll` LGPL-2.1-or-later,
so Tesseract as a whole is distributed under GPL-3.0-or-later. Full license
texts ship in `tesseract\licenses\` (from `scripts/tesseract-licenses/`),
`tesseract\THIRD-PARTY-NOTICES.txt` (from
`scripts/tesseract-THIRD-PARTY-NOTICES.txt`) maps every file to its license
and points to the source archive on the GitHub release (see "Publishing a
release").
