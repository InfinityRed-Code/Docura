# Changelog

All notable changes to rustpdf. Format roughly follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows
SemVer pre-1.0 (breaking changes allowed in any 0.x release).

Versions are reconstructed from the milestone breakdown in
`docs/roadmap.md` and the per-tag commits in `git log`; commit messages prior
to 0.6.2 only carried the version number, so the entries below summarise the
scope each tag landed.

## [0.7.0] — 2026-06-09

Milestone v0.7 — "Fill & Sign". Visual filling/signing plus cryptographic
PAdES digital signatures, a Foxit-style UI redesign, theming, German/English
localization, and native printing.

### Added
- **Forms tab.** Dedicated ribbon tab for AcroForm filling: field navigation
  (previous/next), required-field validation, reset, a field-highlight toggle,
  and a live field counter. (The backend form-fill from v0.6 is unchanged.)
- **Sign tab — visual Fill & Sign.** Draw / type / import a reusable
  signature and initials (stored as PNG under the app config dir); click-to-
  place check / cross / dot / date / text marks. Reuses the existing
  annotation writers (ink / shape / freetext / stamp).
- **Transparent stamps.** `add_stamp` now preserves an image's alpha channel
  via an `/SMask` soft mask, so drawn signatures keep their transparency
  instead of painting an opaque white box. Opaque images still use JPEG.
- **Digital signatures (PAdES).** New `pdf_sign` crate (pure RustCrypto):
  - PAdES-B-B detached CMS `SignedData` with the ESS signing-certificate-v2
    signed attribute; signing key from a **PKCS#12** `.pfx` (PBES2) or the
    **Windows certificate store** (CNG / `NCryptSignHash`).
  - Signing is an **incremental PDF update**, so any pre-existing signature
    stays valid.
  - **RFC 3161 timestamp** support upgrades signatures to PAdES-B-T.
  - **Verification**: recomputes the `/ByteRange` digest, verifies the CMS
    signature and signer certificate, and reports validity, coverage, signer
    CN and timestamp presence.
  - Tauri commands `sign_pdf_pkcs12`, `sign_pdf_windows_cert`,
    `list_windows_certificates`, `verify_signatures`, plus a Sign-tab signing
    modal and verification panel.
- **Theme switch.** Light / dark interface toggle in Settings (persisted),
  with light as the default.
- **Localization (i18n).** German (default) and English, switchable in
  Settings; the entire UI chrome and the new feature strings are translated.

### Changed
- **Foxit-style UI redesign.** The interface moved to a light Office-ribbon
  look (CSS variables, light ribbon, blue accent, dark page canvas); the dark
  theme is now an opt-in `body.theme-dark` variable override.
- **Printing opens the native Windows print dialog.** `print_document` now
  shows `PrintDlgW` and rasterises each page onto the chosen printer's device
  context (`StretchDIBits`) instead of silently spooling the PDF to the
  default printer. New engine method `PdfEngine::print_with_dialog`.
- Workspace version bumped to 0.7.0.

### Notes
- PKCS#12 import supports PBES2 (AES/3DES) only; legacy RC2-encrypted `.pfx`
  files must be re-exported with `-keypbe AES-256-CBC -certpbe AES-256-CBC`.
- The Windows-cert-store signer and native printing compile cleanly but were
  not runtime-verified in CI; digital signatures were cross-checked with
  OpenSSL but should also be validated in Adobe Reader / poppler `pdfsig`.

## [0.6.2] — 2026-05-17

### Changed
- **UI refactor.** Split `apps/desktop-tauri/ui/src/main.ts` (≈4700 LOC
  monolith) into 10 cohesive modules: `types`, `template`, `state`, `dom`,
  `viewer`, `doc`, `edit`, `annot`, `chrome`, `main`. Shared state is a single
  object; cross-module hooks (annotations / form refresh / form-field render)
  go through a callback registry to keep imports acyclic. Behaviour
  preserved end-to-end.
- **FreeText annotation.** Backend `FreeTextAnnotation` gains `bg_color`
  (RGBA — alpha 0 = transparent), `font_family` (currently "Helv", typed
  open for future fonts), and `rotation_quadrant` (0/1/2/3). Default
  background is white opaque, default text colour black; the previous debug
  yellow fill is gone.
- **Text tool moved out of the Draw group.** Edit ribbon now has its own
  *Text* section with foreground / background colour pickers, a transparent
  toggle, font dropdown, and font size input.

### Fixed
- FreeText annotations now render correctly: the appearance rectangle is
  read from the in-DOM editor *before* removing it. Previously
  `getBoundingClientRect` on a just-detached textarea collapsed to 0×0 and
  the annotation landed off-page with zero size.
- FreeText reads upright in rotated viewer modes. The appearance stream now
  selects a text matrix and line-advance vector per `rotation_quadrant`, so
  text and multi-line layout follow the user's view orientation.

## [0.6.1] — 2026-05-17

### Changed
- Synchronised the workspace `Cargo.toml` and `tauri.conf.json` to `0.6.1`.
  Both manifests had been left at `0.4.0` while the release tags advanced
  through `0.5.x` and `0.6.x`.

## [0.6.0] — Milestone: Forms

### Added
- AcroForm field detection and inline HTML rendering (text, checkbox,
  radio, combobox, listbox, push button, signature placeholder).
- Form field editing round-trips through the backend (`list_form_fields`,
  `set_form_field`).
- Form reset (`reset_form`).
- Required-field validation with on-page highlighting of missing fields.
- Signature field placeholders (visual only — cryptography not yet wired).

### Notes
- Field-level JS calculations are still open (see roadmap; planned via
  `boa` for v0.7-era work).

## [0.5.2] / [0.5.1] / [0.5.0] — Milestone: Annotation

### Added
- Text markup: highlight, underline, strikeout, with colour picker.
- Shape annotations: rectangle, circle, line, arrow, free-hand ink.
- Free-text annotation (drag a rectangle, type, commit).
- Sticky notes (click on the page, type a comment).
- Stamp annotation (pick an image, drag a rectangle to place it).
- Region extract (drag a rectangle, save as a new single-page PDF).
- Comment panel with per-kind / per-author / has-content filters.
- XFDF export and import (hand-rolled writer + parser).
- Print / no-print flag per annotation.
- `pdf_mutate::annotations` module backed by `lopdf` for all writes.

## [0.4] — Milestone: Export & Virtual Printer

### Added
- Save as PDF/A-1b/2b/3b/4 via Ghostscript `pdfwrite` pipeline.
- Ghostscript auto-detection (registry + PATH search) with install link.
- Settings overlay (PDF/A default level, Ghostscript status, virtual
  printer status).
- Windows virtual printer named **rustpdf**:
  - Uses the Microsoft PS Class Driver from DriverStore (pnputil fallback).
  - Watches a spool folder under `%ProgramData%\rustpdf\spool` with notify.
  - Converts incoming PS jobs to PDF via Ghostscript.
  - Auto-opens the produced PDF in the running rustpdf window (Tauri
    event).
  - ACLs set on the spool folder (SYSTEM Full + Authenticated Users
    Modify).
- NSIS installer with optional printer-driver setup and a Ghostscript
  install hint. Bundles `pdfium.dll` as a resource for offline install.
- Batch PDF → PDF/A converter pane (multi-file in, output folder out).

## [0.3.x] — Milestone: Organisation

### Added
- Delete pages, extract pages, rotate (persistent), reorder by drag.
- Insert pages from another PDF (before / after / end).
- Merge multiple PDFs into a new document (Organize subview).
- Split a PDF by page-range spec.
- Save / Save As.
- Undo / Redo stack scoped per document.
- Text watermark (font, rotation, opacity, gray).
- Image watermark (scale, opacity, anchor, margin).
- Header / footer with `{page}` and `{total}` tokens, configurable
  font/margin/first-page/gray.
- `pdf_mutate` crate introduced as the structural mutation backend on top
  of `lopdf`. Reserved for organise operations and overlay drawing;
  PDFium stays read-only for content.

## [0.2.x] — Milestone: Navigation & Comfort

### Added
- Continuous scroll mode (virtualised), double-page spread, single-page.
- Goto-page popup (Ctrl+G).
- Per-page and document-wide rotate (display only at this stage).
- History back/forward via internal links.
- Clickable links: internal page targets + external URLs via opener
  plugin.
- Attachments panel + save to disk.
- Fullscreen (F11) and presentation (F5) modes.
- Native Windows print dialog (`print_document`).
- Dark / invert mode for the page bitmap.
- Document properties dialog (metadata + permissions).

## [0.1] — Milestone: Reader MVP

### Added
- Open PDF via dialog and the FS plugin.
- Page rendering as raster image with zoom (Ctrl+Wheel, buttons, preset
  dropdown).
- Page navigation: wheel, arrow / PgUp / PgDn, page-number input.
- Fit-to-width / fit-to-page modes.
- Text search on the current page and across the whole document with
  per-hit highlighting and navigation.
- Text selection and copy (selection layer on top of the bitmap).
- Thumbnail sidebar with click-to-navigate.
- Outline / bookmarks sidebar (TOC).
- Asynchronous render pool — no UI freeze on heavy pages.
- LRU page cache keyed by `(doc, page, zoom, rotation)`.
- Recent files list.
- Drag & drop a PDF onto the window.
- Keyboard shortcuts: Ctrl+O, Ctrl+F, Ctrl+±/0, Ctrl+G.

[0.6.2]: https://github.com/InfinityRed-Code/rustpdf/commit/42c4689
[0.6.1]: https://github.com/InfinityRed-Code/rustpdf/commit/869a5b6
[0.6.0]: https://github.com/InfinityRed-Code/rustpdf/commit/12fc51d
[0.5.2]: https://github.com/InfinityRed-Code/rustpdf/commit/96f3244
[0.5.1]: https://github.com/InfinityRed-Code/rustpdf/commit/aabdb00
[0.5.0]: https://github.com/InfinityRed-Code/rustpdf/commit/3f8693d
[0.4]: https://github.com/InfinityRed-Code/rustpdf/commit/dc1bf9a
[0.3.x]: https://github.com/InfinityRed-Code/rustpdf/commit/262e660
[0.2.x]: https://github.com/InfinityRed-Code/rustpdf/commit/a66d872
[0.1]: https://github.com/InfinityRed-Code/rustpdf/commit/c9809ae
