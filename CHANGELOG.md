# Changelog

All notable changes to rustpdf. Format roughly follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows
SemVer pre-1.0 (breaking changes allowed in any 0.x release).

Versions are reconstructed from the milestone breakdown in
`docs/roadmap.md` and the per-tag commits in `git log`; commit messages prior
to 0.6.2 only carried the version number, so the entries below summarise the
scope each tag landed.

## [Unreleased]

Edit cleanup (#8): more shapes, inline text format, one color picker, more fonts.
Installer: virtual printer set up during installation (#5), Tesseract bundled
for OCR (#6), per-machine install.

### Added
- **More shapes** in Comment → Shapes, grouped as lines / shapes / symbols:
  double arrow, polyline, triangle, diamond, hexagon, star, free polygon,
  cloud, speech bubble and check mark, besides rectangle, ellipse, line and
  arrow. They are saved as real PDF annotations (/Polygon, /PolyLine, /Line;
  the cloud is a /Polygon with the cloudy border effect Acrobat uses) with
  their own appearance streams, and named in the comment list via /Subj.
  Polygon and polyline are placed click by click (double click or Enter
  finishes, Backspace removes the last point, Esc cancels).
- **Fill color** for closed shapes (none by default).
- **XFDF export / import of shapes** now carries polygons (incl. the cloud)
  and polylines with their `<vertices>`, plus for every shape the fill
  (`interior-color`), border `width`, cloudy border (`style`, `intensity`),
  `intent`, line endpoints (`start` / `end`, so arrows keep their direction),
  line endings (`head` / `tail`, polylines can now carry arrow heads too) and
  `subject`. Acrobat-style `x,y;x,y` vertex lists are read as well.
- **Color picker** for every color button (markup / shape color, fill, sign
  marks, text and background color): 16 preset colors, recently used custom
  colors and a custom picker (saturation / brightness field, hue slider, hex
  and R/G/B fields). Background and fill offer "none".
- **Bundled open fonts** for text boxes and page text: Roboto, Open Sans,
  Lato, Montserrat, Lora, Source Serif 4, Roboto Mono and Caveat (SIL Open
  Font License 1.1, license files shipped under `licenses/fonts/`). A text box
  embeds the font once per document (Type0 / Identity-H with /ToUnicode, so
  copy and search work); edited page text embeds it via PDFium and is
  recognized again on the next edit. Adds about 4 MB to the executable
  (about 1.3 MB to the compressed installer).

### Changed
- The text format controls (font, size, bold, italic, text and background
  color) sit directly next to "Text" in the Edit tab instead of only in the
  floating bar; both stay in sync and the ribbon sets the style of the next
  text box when none is open. The last text style is remembered.
- **The installer installs per machine** (`C:\Program Files\Docura PDF`,
  administrator prompt) instead of per user. An existing per-user install is
  removed first (app data and an enabled autostart are kept), so `.pdf` files
  do not keep opening the old copy.
- The installer no longer asks whether to install the virtual printer; it
  always does.

### Fixed
- **The installer sets up the virtual printer (#5).** It had to be installed
  again from the app after every installation. The installer ran per user
  without administrator rights, and adding a printer port / printer needs
  them; the step also still used the pre-rename names (printer "rustpdf",
  `rustpdf-spool.ps` under a literal `$PROGRAMDATA` path, which is not an
  NSIS constant) and passed the driver name unquoted to PowerShell — and
  every failure was swallowed. The installer now runs elevated, registers
  "Docura PDF" on `%ProgramData%\DocuraPDF\spool\docura-spool.ps` exactly
  like the in-app button (`nsis/printer-setup.ps1`, run by the 64-bit
  PowerShell), logs to `%ProgramData%\DocuraPDF\printer-setup.log`, and
  shows a message if it fails. Uninstalling removes the printer again.
- **OCR works right after installation (#6).** The installer ships
  Tesseract 5.4 (UB-Mannheim build: `tesseract.exe` and the DLLs it loads)
  with the `deu` and `eng` models in `<install dir>\tesseract`, licenses
  included. The app uses it first and points it at its own `tessdata`, so no
  PATH or environment setup is needed. `scripts/fetch-tesseract.ps1`
  (`just tesseract`) stages it, pinned by SHA-256; the installer build stops
  if it is missing. The bundle carries full license texts in
  `tesseract\licenses\` (GPL-2.0, GPL-3.0, LGPL-2.1, Apache-2.0, GCC runtime
  exception and every library's own license) and a notice that it is under
  its own licenses, not Docura's. Its complete corresponding source
  (Tesseract at the UB Mannheim tag plus MSYS2 recipe and upstream source of
  every bundled library, JBIG-KIT and GNU libiconv included) is built by
  `scripts/fetch-tesseract-sources.ps1` (`just tesseract-sources`) and
  attached to each GitHub release, not put into the installer.

## [1.3.0] — 2026-11-19

Text editing reworked (#7); zooming large plans is fast.

### Added
- **Edit existing text.** With the Text tool armed, every line of text on the
  page — native text as well as an OCR layer — is outlined and can be clicked
  and retyped. The old text is removed from the page content and the new text
  drawn at the same baseline in the closest standard font (sans / serif /
  monospace, bold / italic kept); on scans the printed line is painted over
  with the sampled page color first. Characters beyond Latin-1 use an embedded
  system font. Editing works line by line; text does not reflow.
  Text from fonts without a Unicode mapping (PDFs printed through a
  PostScript / XPS driver, the virtual printer included — copy and search
  yield gibberish there) is read back by matching the embedded glyphs against
  the installed fonts. Where the original font is not installed the editor
  opens empty on the line, which stays visible until typing starts.
  Where the line's font is installed on this machine (identified by those
  glyphs or by its PostScript name) the new text is written with that very
  font, embedded once per page, so an edited line looks like its neighbours;
  changing family / bold / italic in the format bar switches to the standard
  fonts. Arial, Times New Roman and Courier New are never embedded — the
  standard fonts are their metric twins.
- **Floating format bar** on the text editor: font family, size, bold, italic,
  text color, background color / transparent, delete, done. It replaces the
  "Text format" popover in the ribbon. Text boxes can be dragged by the grip.
- Text boxes can use Times and Courier besides Helvetica, each bold / italic.

### Changed
- **Text boxes stay editable.** Clicking a text box reopens it in the editor
  (text, style, position); the change is one undo step. Emptying it deletes it.
- The Text tool places a box with a single click; the box grows with the text
  instead of clipping at a pre-drawn rectangle. Enter starts a new line,
  Ctrl+Enter or a click outside finishes, Esc cancels.
- The mouse wheel zooms by a factor per notch (×1.2 with Ctrl, ×1.25 in
  hand mode) instead of a fixed +10 %, which crawled at high zoom; touchpad
  pinch zooms smoothly.

### Fixed — zooming large plans
- **Zooming lagged seconds behind.** Every wheel notch started a sharp
  region render (up to 2 s each on scanned plans) that PDFium can't abort,
  so they queued up — 5–8 s of backlog after a quick zoom. Region renders
  now run one at a time, wait until the wheel settles, and only the newest
  view is rendered; the previous sharp window stays visible (scaled) while
  zooming instead of dropping back to the blurry preview.
- **Sharp regions were slow just above the preview resolution.** Scanned
  plans now get a resolution pyramid (½ and ¼ besides full resolution);
  each region uses the coarsest level that is still sharp, so a
  viewer-sized region renders in ~50–100 ms instead of 0.6–1.9 s.
- After a big zoom step the visible area is rendered sharp first, the
  whole-page bitmap (only seen when panning further) afterwards.
- **Opening plans took 9–12 s when the window wasn't focused.** Windows
  moved the decoder to efficiency cores at low clocks on hybrid CPUs
  (Intel 12th gen and later); decoding and rendering threads now opt out
  of power throttling (2.4–3.4 s on the test laptop).
- **Zooming rebuilt the whole text layer on every step.** The selectable
  text is now laid out once per page and only rescaled while zooming
  (about 20 layout passes per zoom gesture instead of several hundred to
  over a thousand).

## [1.2.0] — 2026-11-09

More room for the document: the ribbon can auto-hide.

### Added
- **Ribbon auto-hide.** A small arrow at the right end of the tab row toggles
  the ribbon: arrow down = always shown, arrow up = auto-hide — the command
  row then only slides in (as an overlay, the document doesn't move) while
  the pointer is on the tabs or the ribbon, giving the document ~80 px more
  height. Remembered across starts; Ctrl+F1 toggles it as well.

## [1.1.1] — 2026-11-09

Faster, complete rendering of large CAD / scanned plans.

### Fixed — large CAD / scanned plans
- **Plans with huge scanned images rendered half black.** PDFium refuses to
  inflate more than 1 GiB from a single Flate stream, so the lower part of
  multi-gigapixel scans (often mistaken for DRM on signed plans) stayed black.
  Such images are now decoded once on open (streamed inflate + PNG unfilter,
  multi-threaded) into a downsampled preview proxy and a tiled full-resolution
  proxy; PDFium renders from those. Annotation appearances (stamps, signature
  widgets) are flattened into the full-resolution proxy so they still show.
  Saving still writes the original bytes.
- **Opening and zooming plans was very slow.** Whole-page renders of these
  plans took 2–3 s each (PDFium re-decodes the image every time); with the
  preview proxy they take ~60 ms. Deep zoom keeps a capped whole-page bitmap
  and renders only the visible window sharply (new `render_region` command),
  and zooming holds re-renders until the wheel settles.
- **Hand tool could not reach the left part of wide pages at high zoom.**
  The page list centred with `align-items: center`, which pushes overflow off
  the left edge where it can't be scrolled to; it now uses `safe center`.

## [1.1.0] — 2026-11-08

The v1.1 "Performance & Polish" milestone: a simpler, more intuitive UI, save
with unsaved-changes protection, and a broad bug-fix pass over the UI, the
Tauri backend and the PDF crates.

### Changed — simpler UI
- **Seven task-oriented tabs instead of nine.** Start (view tools), Kommentieren,
  Seiten (page grid: rotate / delete / extract / insert / merge / split),
  Bearbeiten (text, image, watermark, header/footer), Ausfüllen & Signieren
  (forms + signatures + digital ID), Konvertieren (Word/Excel/images/text/HTML,
  create PDF, PDF/A, OCR) and Schützen. Duplicated commands (OCR, Office
  export, extract, rotate, text box) now live in exactly one place, and every
  ribbon group carries a caption.
- **Status bar.** Status messages, page navigation and zoom (zoom steps,
  presets, fit width / fit page) moved into a bottom status bar; the go-to
  popup is replaced by the page box (Ctrl+G focuses it).
- **Welcome screen.** Without a document the window shows a large "PDF öffnen"
  button, the drag-and-drop hint, shortcuts to merge / images → PDF /
  Office → PDF and the recently opened files.
- Commands that need a document are greyed out until one is open; the
  Konvertieren tab no longer hides the document (batch PDF/A opens on demand).
- Delete mode removed: select pages in the Seiten grid and delete (with
  confirmation, undoable).
- Split got a real dialog ("every N pages" or page ranges, with a live
  preview) — the old `window.prompt` never worked inside the WebView.
- Page drag & drop in the Seiten grid shows a clear insertion bar, a ghost of
  the grabbed page and dashed outlines on the pages being moved; dropping a
  page onto itself no longer moves it to the end.
- **Restricted PDFs stay restricted.** Protecting with permissions but no owner
  password now uses a random owner password instead of the user password
  (which made every user an owner and voided the restrictions).

### Added
- **Save (Ctrl+S) and unsaved-changes protection.** "Speichern" writes back to
  the opened file (temp documents such as merge results ask for a location);
  a "•" in the title marks unsaved changes, and opening another file, changing
  the language or closing the window asks to save, discard or cancel.
- Escape closes the topmost dialog (else the search bar / presentation, else
  disarms the active tool); every dialog closes on a backdrop click.

### Fixed
- **Script injection from PDF content (security).** Annotation subtypes,
  signer names and verification details from a PDF were inserted as HTML with
  CSP disabled; they are now escaped / set as text.
- Pending redaction marks are discarded when pages change (delete, reorder,
  insert, undo) instead of blacking out the wrong page; the ✕ on a mark no
  longer removes a different mark.
- Page tools (markup, shapes, eraser, sign marks, hand) are mutually exclusive
  and disarmed when switching tabs; clicks inside a text box / note editor no
  longer start a new one.
- Radio-button groups: every option renders and can be selected.
- Form "next/previous field" works from the ribbon and waits for the target
  page to render; filling a field enables Undo.
- Late results (OCR, digital signing, redaction, text/link layers, thumbnails,
  comment lists) are dropped when the document changed meanwhile.
- Double clicks no longer sign / merge / convert / redact twice.
- Missing translations (e.g. the "Schutz anwenden" button showed its raw key)
  and ~150 hard-coded English status messages are now localized.
- Saving writes to a temp file and renames it over the target, so a failure
  mid-write can't destroy the original.

### Fixed — PDF processing (`pdf_mutate`, `pdf_sign`, `pdf_pdfium`, …)
- **Redaction removed content for real**: original images, XObjects, fonts and
  the page thumbnail of a redacted page were still in the file; shared content
  streams of other pages were blanked. Deleted / unextracted pages and
  sanitized metadata no longer linger in the saved PDF either.
- A second digital signature corrupted the first (ByteRange placeholder search
  hit the old signature); signing no longer drops inline AcroForm fields, and
  non-ASCII reason / name / location are written as proper PDF text.
- Watermark, header/footer and OCR no longer wipe fonts and images of pages
  that inherit `/Resources`; adding a text box no longer wipes form fields.
- Reorder / insert keep inherited MediaBox, Resources, CropBox and Rotate.
- Rotated pages: header/footer, watermarks, annotation positions (eraser,
  notes, XFDF) and region export now use the displayed orientation.
- Malformed PDFs: `/Parent` / `/Kids` cycles, bad ByteRanges and truncated DER
  no longer hang or crash; XFDF import keeps umlauts and imports squiggly.
- Case-insensitive search matches umlauts; permissions report the print and
  fill-forms bits correctly; a second image watermark keeps the first one;
  transparent PNG watermarks keep their alpha.
- Ghostscript (PDF/A, virtual printer) works with `%` in file names; temp
  files are cleaned up on errors; a document that fails to open is released.

### Added (earlier in this cycle)
- **Foxit-style ribbon redesign.** Three-tier chrome: a title bar with the app
  icon + a compact icon-only quick-access toolbar (open/save/print/undo/redo/
  page-nav), a flat text tab strip with an accent underline on the active tab,
  and a ribbon body of uniform icon-over-label command buttons. Closely-related
  actions are grouped into clean dropdowns (Markup shapes, Text options, Export,
  Create, Sign marks, page Rotate/Insert). Split the former combined `edit` tab
  into dedicated **Comment** and **View** tabs, mirroring Foxit's layout. The
  per-tab show/hide matrix was collapsed to one compact `:not()` rule per tab.
- **Squiggly (wavy) text markup.** A new text-markup annotation alongside
  highlight/underline/strikeout, drawn as a wavy underline appearance stream
  (`MarkupKind::Squiggly`, subtype `Squiggly`); the three markup tools are now
  individual ribbon buttons instead of a dropdown.
- **Fuller Edit tab.** "Text einfügen" (free text), "Text erkennen" (OCR) and a
  "Nach Office" (Word/Excel) dropdown are now surfaced on the Edit tab too,
  reusing the existing OCR/export handlers.
- **High-contrast theme + accessibility pass.** A new High-contrast theme
  (black/white with a yellow accent and solid borders) in Settings → Appearance,
  a visible keyboard focus outline everywhere (`:focus-visible`), `aria-label`s
  mirrored from tooltips onto icon-only buttons, and a `lang` attribute for
  assistive tech.
- **Default-zoom setting.** A new "Default zoom" dropdown (fit page / width /
  actual) in Settings → Appearance, persisted and applied to documents on open.
- **Render-cache budget setting.** Settings → Performance now exposes the
  render cache as a memory budget in MB (default 256, floor 16), persisted in
  `settings.json` and applied on startup.

### Changed
- **Text-markup annotations now carry an appearance stream.** Highlight,
  underline and strikeout each write an explicit `/AP /N` form instead of
  relying on the viewer to synthesise one, so they render identically across
  viewers and when printed or flattened. Highlights use the Multiply blend mode
  so the text underneath stays readable (a real-highlighter look) instead of a
  flat wash; underline/strikeout draw a clean per-line stroke at the baseline /
  mid-line.
- **Render cache is now byte-budgeted.** `pdf_cache` evicts by an approximate
  MB budget instead of a fixed entry count, a better proxy for actual memory
  given how much encoded page sizes vary.
- **Bounded render pool with viewport priority.** Page rendering now keeps up to
  three `render_page` calls in flight (nearest-to-viewport first) instead of
  rendering strictly one at a time, so a page encodes while the next
  rasterises. PDFium rasterisation stays serialised (it is not thread-safe); a
  render epoch drops superseded results so stale bitmaps never paint after a
  zoom/rotate.
- **Parallel OCR.** OCR now recognises pages across a bounded pool of Tesseract
  processes (up to 6, one per core) instead of one page at a time — a large
  speed-up on multi-page scans. Core budget is split between the pool and each
  process's OpenMP threads, so a single-page job still uses all cores. (Tesseract
  has no usable GPU path, so multithreading is the available lever.)
- **Page loading skeleton.** Never-rendered pages show a blank sheet with a soft
  shimmer instead of the old diagonal hatch.
- **Persistent thumbnail cache.** Thumbnails are cached on disk
  (`<app_cache>/thumbnails`) keyed by a document content hash, so the strip
  fills instantly when reopening a file in a later session. The cache is
  invalidated automatically when a document is edited and pruned to 200 MB on
  startup.
- **System tray + autostart.** A tray icon (Open / Quit) plus two Settings →
  System options: "keep running in the tray on close" (the window hides instead
  of quitting so the print-to-PDF spool watcher stays active) and "start at
  login".
- **Settings window redesigned.** A vertical navigation rail replaces the long
  stacked layout: each entry scrolls its section into view (with a scrollspy
  highlight) and horizontal scrolling is gone.
- **Relicensed to PolyForm Noncommercial 1.0.0** (source-available): free for
  noncommercial use, commercial license required for for-profit companies.
  Replaces the previous MIT/Apache-2.0 dual license.

## [1.0.1] — 2026-06-13

Bug fixes and small tools for the v1.0 OCR and annotation features.

### Fixed
- **OCR found no text with downloaded language packs.** TSV word boxes are
  now requested via `-c tessedit_create_tsv=1` instead of the `tsv`
  configfile. The configfile lives in `<tessdata>/configs/`, which a
  downloaded language-pack directory (passed as `--tessdata-dir`) does not
  have, so Tesseract silently fell back to plain-text output and the parser
  found zero words. Only affected downloaded languages (e.g. `deu`), not the
  bundled-system `eng`.
- **OCR text landed off-page on some PDFs (e.g. Canva exports).** Appended
  content (the OCR text layer and every overlay) is now isolated from the
  existing page content with `q`/`Q`. Content streams that leave a
  non-identity CTM active no longer drag the injected text far off-page —
  which had made it unsearchable (`FPDFText_GetBoundedText` over the page
  rect found nothing) and unselectable.
- **Selection/highlight boxes of tightly spaced words overlapped.** The text
  layer now scales each run's glyphs horizontally to its box width, so the
  invisible selection text no longer overflows into the neighbouring word.
- **Highlighting words produced doubled, darker overlapping marks.** Markup
  rectangles are merged per line before the annotation is created, yielding
  one continuous bar instead of overlapping per-word quads.

### Added
- **Eraser tool.** Click an annotation to delete it (Edit tab, next to the
  markup tools). Undo restores it.
- **Extract Selected** is now also available in the Organize ribbon, before
  the split tool.

## [1.0.0] — 2026-06-12

Milestone v1.0 "OCR & Scans": scanned documents become searchable.

### Added
- **OCR (Texterkennung).** New group in the Convert tab. Tesseract is
  driven as an external CLI (detected like Ghostscript: `TESSERACT` env
  var, PATH, Program Files, `%LOCALAPPDATA%\Programs` — UB-Mannheim
  build recommended, never bundled). Pages are rendered at 300 DPI,
  recognised words are injected as an **invisible text layer** (render
  mode 3, Helvetica/WinAnsi, `Tz`-scaled to the word boxes, /Rotate
  0/90/180/270 handled) — the page becomes searchable/extractable while
  the scan image stays untouched. Undo works like any other mutation.
- **Language packs.** OCR dialog lists 20 curated languages (plus
  everything the local Tesseract reports); missing packs are downloaded
  on first use from `tessdata_fast` into the app config dir and passed
  via `--tessdata-dir`. Combos like `deu+eng` work across system and
  downloaded packs. Selected language persists in the settings.
- **Image preprocessing.** Optional (on by default): contrast stretch
  (1–99 percentile), 3×3 median despeckle, and deskew (±5°, projection
  profile scoring on the Otsu-binarised image).
- **Batch OCR.** Pick multiple PDFs and an output folder; each file is
  OCRed (pages that already carry text are skipped) and written as
  `<name>_ocr.pdf`. Encrypted inputs are skipped.
- **Scan detection.** When an opened document's first pages contain no
  extractable text, a dismissible banner offers to run OCR directly.
- Progress events (`ocr-progress`) drive per-page/per-file status; new
  error markers `TESSERACT_MISSING` / `OCR_LANG_MISSING` map to
  localized messages (de/en fully covered).

### Tests
- `pdf_pdfium/tests/ocr_layer.rs`: injected layer is searchable,
  reports as hidden text, hit rectangles match the word boxes (±8 pt),
  rendering stays visually unchanged, rotated pages work.
- `desktop-tauri` `ocr::tests::tesseract_roundtrip`: end-to-end against
  a real Tesseract install (skips when absent).

## [0.9.5] — 2026-06-11

Rebrand: the app is now **Docura PDF**.

### Changed
- **Name.** Product, window/taskbar title, installer, uninstall entry,
  Start-menu shortcut, file-association ProgID (`DocuraPDF.pdf`) and the
  virtual printer (now "Docura PDF", output folder `Dokumente\Docura
  PDF`) all use the new name. App identifier changed to `com.docura.pdf`
  — settings, recents and saved signatures are migrated automatically
  from the old `com.rustpdf.app` config dir on first start.
- **Logo.** New violet Docura logo everywhere: exe/installer/file icons
  (rasterized from `CI/Logo.svg`, background stripped, multi-size .ico),
  title-bar glyph and the logo-with-wordmark as the viewer's empty state
  — both shipped as cropped vector SVGs. Generator:
  `cargo run -p pdf_mutate --example brand_icons`.

### Added
- **Docura theme.** Third theme in the logo's violet palette (alongside
  light and dark), selectable in the settings.

## [0.9.4] — 2026-06-11

### Fixed
- **Hatched placeholder flashing on zoom.** Changing zoom (or rotation)
  dropped every page bitmap and showed the diagonal-striped placeholder
  until the re-render landed. The stale bitmap now stays visible as a
  scaled preview (briefly soft, then sharp); the placeholder only appears
  for pages that were never rendered.

## [0.9.3] — 2026-06-11

### Fixed
- **Console window flashing on startup.** Every spawned console helper
  (the Ghostscript version probe runs on each app start; also soffice
  and the PowerShell calls of the virtual-printer setup) briefly opened
  a terminal window — very visible when opening a PDF via the file
  association. All child processes now start with CREATE_NO_WINDOW.

## [0.9.2] — 2026-06-11

### Added
- **PDF file association.** The installer now registers rustpdf as a PDF
  handler (ProgID + "open with" entry), so it can be picked under
  Windows' "Öffnen mit" and as the default app for .pdf. Double-clicking
  a PDF (or dragging one onto the exe) opens it directly: the CLI path
  argument is consumed by the frontend on startup.

### Changed
- Installer/window icon now ships the multi-resolution `icon.ico`.

## [0.9.1] — 2026-06-11

Milestone v0.9 completed (PDF → PowerPoint deliberately deferred).

### Added
- **Export as Word (.docx).** New `pdf_export` crate reconstructs text
  flow from the page layout — lines from vertical overlap, paragraphs
  from leading gaps, headings from size jumps — and writes a flowed
  document (page breaks between PDF pages). Meant for further editing,
  not layout fidelity.
- **Export as Excel (.xlsx).** One worksheet per page; lines become rows
  and columns are clustered from the x-start positions of the text runs
  (~10 pt tolerance). Numeric cells are written as numbers. Works for
  visually tabular PDFs; prose lands in column A.
- **Office → PDF.** Converts Word/Excel/PowerPoint/OpenDocument files via
  LibreOffice headless (`soffice --convert-to pdf`; found in Program
  Files or PATH) and opens the result directly in the app. Shows a
  localized hint when LibreOffice is not installed.

### Fixed
- **Word/Excel exports were rejected by Office for many real-world
  PDFs.** PDFium emits C0 control characters (e.g. U+0002) for glyphs
  without a Unicode mapping; they ended up unescaped in the OOXML and
  Word/Excel refused the file. The engine now strips them from text
  extraction, layout, char boxes and hidden-text scans (also cleaning up
  text selection), and `pdf_export` additionally sanitizes all run text
  (defense in depth). Verified via Word/Excel COM automation on a
  previously failing sample.

## [0.9.0] — 2026-06-11

Milestone v0.9 "Export & Konvertierung", first block. (Supersedes the
0.8.2 tag — same content, renumbered to match the roadmap milestone.)

### Added
- **Export pages as PNG/JPG.** Convert-tab dialog with format, resolution
  (96–600 DPI) and page scope (all / current / range); pages render via
  PDFium and land as `name_pNNN.png|jpg` in a chosen folder.
- **Export as plain text.** All pages via PDFium text extraction, blank
  line between pages, saved as UTF-8 `.txt`.
- **Export as simple HTML.** One paragraph per text line, pages as
  `<section>` separated by `<hr>` — meant for copy-paste, not layout
  fidelity.
- **Images → PDF.** Pick multiple PNG/JPG/BMP/WebP files and get a new
  PDF with one page per image (page size = pixel size at 72 dpi). JPEGs
  are embedded as-is (no recompression), other formats as lossless
  Flate-compressed RGB. The result opens directly in the app, like a
  merge result. E2E-tested: opens in PDFium, page sizes and rendered
  pixel colors match the inputs.

## [0.8.1] — 2026-06-11

### Changed
- **Integrated title bar.** Native window decorations are off; the ribbon's
  tab row now doubles as the title bar — app icon and tabs on the left, a
  draggable area with the current document name in the middle, and themed
  minimize/maximize/close buttons on the right (close hovers red, the
  middle icon switches between maximize and restore). Drag to move,
  double-click to maximize, taskbar/alt-tab title stays in sync. Saves a
  full bar of vertical space and follows the light/dark theme.

## [0.8.0] — 2026-06-11

Completes milestone v0.8 "Sicherheit & Redaktion": redaction, metadata
sanitising, hidden-content discovery and pattern redaction (the password
protection block shipped earlier with 0.7.1).

### Added
- **Redaction (Schwärzen).** New tool in the Protect tab: drag boxes over
  content, review/remove the pending marks (✕ on each box), then apply with
  a confirmation. Affected pages are rasterised at ~200 DPI with the regions
  blacked out in pixel data and the page content replaced by that bitmap —
  original text, vectors, images and the page's annotations are removed from
  the file, not merely covered (verified end-to-end: the secret string is
  absent from the raw bytes and from text extraction). Trade-off: redacted
  pages become images (no selectable text); byte-level undo still works
  until the file is saved.
- **Clean metadata.** One click in the Protect tab strips the /Info
  dictionary, XMP /Metadata streams (catalog and pages) and private
  /PieceInfo data.
- **Find hidden content.** New scan in the Protect tab reports content a
  reader doesn't (obviously) see: optional-content layers with their
  default visibility (hidden layers ship content that isn't displayed),
  all comments/annotations (links, popups and form widgets excluded), and
  invisible text — characters drawn with text rendering mode 3, detected
  via PDFium's per-character render mode and grouped into snippets.
  Read-only report dialog; nothing is modified.
- **Pattern redaction.** Regex search over the page text marks every match
  as a pending redaction box (one per matched line, mapped via per-character
  bounding boxes). Presets for e-mail addresses and phone numbers plus a
  custom-regex field; matches feed into the normal redaction flow —
  review the marks, then apply.
- Dev utilities: `cargo run -p pdf_mutate --example make_big` (generate a
  many-page text PDF) and `cargo run -p pdf_pdfium --example bench_text`
  (time open/render/text-layout per page).

## [0.7.3] — 2026-06-10

### Added
- **Interactive sticky-note popups.** Note icons on the page are now
  clickable: hovering previews the note in its yellow sticky look, a click
  pins it open, a second click closes it. Popups flip to the left near the
  page edge and track zoom/rotation (new per-page note layer).

### Changed
- **Lucide icon set.** All ~75 toolbar/panel buttons switched from
  emoji/unicode glyphs to Lucide SVG icons (ISC license, tree-shaken,
  `stroke: currentColor` so they follow the light/dark theme). Covers the
  ribbon (all tabs), comment panel, merge/convert bars, sign marks and the
  settings gear; duplicate emoji inside i18n button labels removed; the
  comment list's annotation-type icons use Lucide too.
- **Sidebar tabs are icons.** "Miniaturen / Gliederung / Dateien" no longer
  overflow the 200 px sidebar — the tabs show grid / tree / paperclip icons
  with the localized name as tooltip.
- Sticky-note editor restyled onto shared CSS classes and localized
  (German/English) instead of hardcoded English strings.

## [0.7.2] — 2026-06-10

### Added
- **Themed Windows title bar.** The native title bar follows the app theme
  (DWM caption/text color + dark-mode hint, Windows 11+); applied on startup
  and on every theme switch.
- Dev utility `cargo run -p pdf_mutate --example make_encrypted` — generates
  AES-256-protected sample PDFs (password, optional owner password, page
  count) for manual testing.

### Fixed
- **Permissions of AES-256-protected PDFs read as "all denied".**
  pdfium-render's permission helpers fail for security handler revisions 5/6,
  so every flag read as `false` — and a save would have re-encrypted the file
  with everything denied. Permissions are now read from the raw `/P` bits via
  `FPDF_GetDocPermissions`; the document properties dialog also shows the
  correct handler revision ("Standard (rev 6, AES-256)").

## [0.7.1] — 2026-06-10

Start of milestone v0.8 — "Sicherheit & Redaktion": password protection
(open, protect, permissions). Redaction and sanitization follow in a later
release.

### Added
- **Open password-protected PDFs.** PDFium opens the file with the user or
  owner password; a proper password modal replaces the previous
  `window.prompt()` call, which WebView2 does not support — password entry
  was effectively broken before. Up to 3 attempts, localized messages.
- **Protect tab.** New ribbon tab with a protect dialog: open password
  (user), permissions password (owner), and permission switches for
  printing, modification, copying and annotating. AES-256 (V5/R6) via a new
  `pdf_mutate::encryption` module; protection is applied when the file is
  saved (the dialog chains straight into Save As). "Remove protection"
  un-sets it the same way.
- **Decrypt-at-open / re-encrypt-at-save architecture.** Protected documents
  are flattened to a plaintext working copy on open (raw `FPDF_SaveAsCopy`
  with `FPDF_REMOVE_SECURITY`; pdfium-render's safe wrapper hardcodes
  `flags = 0`), so rendering, all lopdf mutations, undo/redo and forms work
  unchanged; `save_pdf_as` re-applies the tracked protection settings
  (`AppState.doc_crypto`). Permissions of the original file are preserved.
- **Encrypted-input guard.** Merge and insert-pages reject password-protected
  input files with a clear localized message instead of silently producing a
  broken document (lopdf cannot parse encrypted files).
- E2E test `pdf_pdfium/tests/encryption_roundtrip.rs`: lopdf-encrypt →
  pdfium-open (wrong/user/owner password) → flatten → text extraction +
  lopdf mutation on the plaintext copy.

### Changed
- Document properties of a protected document now report the tracked
  protection settings ("AES-256 (applied on save)") instead of pdfium's view
  of the decrypted in-memory copy, which would claim "Unprotected".
- Opening a protected document shows a 🔒 status note; PDF/A export notes
  that PDF/A forbids encryption (protection is not applied there).
- Digital signing is blocked while password protection is active
  (re-encrypting on save would invalidate the signature's `/ByteRange`);
  the UI points to the Protect tab.

### Known limitations
- Signature verification on documents that were signed *and* encrypted
  reports invalid — the flattened working copy no longer matches the signed
  bytes.
- Owner-only-restricted files (no open password) are re-encrypted on save
  with an empty user password and their original permission bits.

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
