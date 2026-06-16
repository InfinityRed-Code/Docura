# rustpdf — Roadmap

Ziel: Native Windows-Desktop-App (Tauri 2 + Rust), die Foxit PDF, PDF24 und Adobe Acrobat in den alltäglichen Anwendungsfällen ersetzt. Priorisiert nach MVP-Prinzip: Essenzielles zuerst, Nischen zuletzt. Jede Meilensteinversion ist in sich nutzbar.

Legende: **[C]** = Core-Rust, **[E]** = PDFium-Engine, **[S]** = Services, **[U]** = UI (TS), **[X]** = externe Bibliothek / neuer Crate.

---

## Meilenstein v0.1 — "Reader MVP" ✅

Minimum, um eine PDF tatsächlich zu lesen. Ersetzt bereits den Browser-Reader für viele Nutzer.

- [x] PDF öffnen (Dialog + FS-Plugin) [S][U]
- [x] Seite als Bild rendern [E][U]
- [x] Seitenwechsel per Mausrad [U]
- [x] Zoom (Ctrl+Rad, Buttons, Preset-Dropdown) [U]
- [x] Pfeiltasten / PageUp / PageDown [U]
- [x] Fit-to-Width / Fit-to-Page Modus [U]
- [x] Textsuche (aktuelle Seite) [E][U]
- [x] Textsuche (gesamtes Dokument) mit Treffermarkierung [C][E][U]
- [x] Text selektieren und kopieren (Selection-Layer über Bild) [C][E][U]
- [x] Thumbnail-Leiste links mit Klick-Navigation [U][X: pdf_cache]
- [x] Outline / Bookmarks (TOC-Sidebar) [E][U]
- [x] Asynchroner Render-Pool (kein UI-Freeze bei großen Seiten) [C][E]
- [x] Seiten-Cache mit LRU (Zoom-Level-keyed) [C][pdf_cache]
- [x] Zuletzt geöffnete Dateien [S][U]
- [x] Drag & Drop einer PDF ins Fenster [U]
- [x] Tastenkürzel: Strg+O, Strg+F, Strg+Plus/Minus/0, Strg+G (Go-to-page)

---

## Meilenstein v0.2 — "Navigation & Komfort" ✅

Nutzererwartung an einen ernsthaften Reader.

- [x] Kontinuierlicher Scroll-Modus (alle Seiten, virtualisiert) [U]
- [x] Doppelseiten-Modus [U]
- [x] Drehen pro Seite / dokumentweit (nur Anzeige) [C][U]
- [x] Goto-Page Popup (Strg+G) [U]
- [x] Verlaufs-Navigation: Zurück/Vor nach internen Links [C][U]
- [x] Links im PDF klickbar (interne Ziele + externe URLs via Opener-Plugin) [E][S][U]
- [x] Anhänge (Attachments) anzeigen + speichern [E][S][U]
- [x] Vollbildmodus (F11) + Präsentationsmodus [U]
- [x] Druck-Dialog (Windows-Native) [S]
- [x] Dark-/Invert-Mode für Seitenrendering [U]
- [x] Dokumenten-Eigenschaften (Metadaten, Schriftarten, Seitengröße) [E][U]

---

## Meilenstein v0.3 — "Organisation" ✅

Seiten manipulieren — klassischer PDF24-Einstieg.

- [x] Seiten löschen [E][X: lopdf]
- [x] Seiten einfügen (aus anderer PDF) [X: lopdf]
- [x] Seiten neu anordnen (Drag&Drop im Thumbnail-Panel) [X: lopdf][U]
- [x] Seiten rotieren (persistent im Dokument) [X: lopdf]
- [x] Seiten extrahieren → neue PDF [X: lopdf]
- [x] PDFs zusammenführen (Merge mehrerer Dateien, Organize-Subview + Temp-Datei) [X: lopdf]
- [x] PDF splitten (nach Seitenbereich) [X: lopdf]
- [x] Speichern / Speichern unter [S]
- [x] Undo/Redo Stack für Organisations-Operationen [C]
- [x] Wasserzeichen (Text) [X: lopdf]
- [x] Wasserzeichen Bild [X: lopdf + image]
- [x] Kopf-/Fußzeile, Seitenzahlen einfügen [X: lopdf]

---

## Meilenstein v0.4 — "Export & Virtueller Drucker" ✅

Archivierung (PDF/A) und systemweiter Print-to-PDF — das PDF24/Adobe-Kernstück für Workflows.

- [x] Ghostscript-Erkennung (Registry + Path-Suche) + Download-Prompt [S][U]
- [x] Speichern als PDF/A-1b/2b/3b/4 via Ghostscript `pdfwrite` [X: Ghostscript]
- [x] Einstellungen-Modal (Zahnrad in der Ribbon) [U]
- [x] Windows virtueller Drucker "rustpdf" (Microsoft PS Class Driver + lokaler Dateiport) [S]
- [x] Auto-Registrierung des PS Class Drivers aus DriverStore (pnputil Fallback) [S]
- [x] Spool-Watcher (notify-crate) mit Stabilitäts-Check → PS→PDF-Konvertierung via Ghostscript [C][S][X: notify]
- [x] Automatisches Öffnen der gedruckten PDF in rustpdf (Tauri-Event) [U]
- [x] NSIS-Installer mit Ghostscript-Hinweis und optionalem Druckertreiber-Setup [S]
- [x] `pdfium.dll` als Bundle-Resource (Offline-Install) [S]
- [x] ACL-Einrichtung auf `%ProgramData%\rustpdf\spool` (SYSTEM Full + Authenticated Users Modify) [S]

---

## Meilenstein v0.5 — "Annotation" ✅

Herzstück der Reviewer-Workflows.

- [x] Textmarker (Highlight) mit Farbwahl [E + X: lopdf]
- [x] Unterstrich / Durchstrich [E + X: lopdf]
- [x] Notizzettel (Sticky Notes) mit Kommentar-Popup [X: lopdf]
- [x] Freihand-Zeichnen (Ink Annotation) [X: lopdf]
- [x] Formen: Rechteck, Kreis, Linie, Pfeil [X: lopdf]
- [x] Textfeld-Annotation (Freitext auf Seite) [X: lopdf]
- [x] Stempel (Bild-Stempel via Form-XObject) [X: lopdf]
- [x] Kommentar-Panel rechts mit Thread + Filter [U]
- [x] Annotationen exportieren / importieren (XFDF) [X: hand-rolled]
- [x] Annotationen drucken/nicht drucken Flag [C][U]

---

## Meilenstein v0.6 — "Formulare ausfüllen"

Abdeckung aller AcroForm-basierten Behörden-/Unternehmensformulare.

- [x] AcroForm-Felder erkennen und rendern [E]
- [x] Textfeld-, Checkbox-, Radio-, Dropdown-Eingabe [E][U]
- [ ] Feld-Berechnungen (JS-Formeln für Summen/Format) [X: boa / quickjs-Bindung]
- [x] Formular speichern (ausgefüllt) [X: lopdf]
- [x] Formular zurücksetzen
- [x] Validierung + Required-Feld-Hinweise [U]
- [x] Signaturfelder vorbereiten (Platzhalter, noch ohne Krypto)

---

## Meilenstein v0.7 — "Fill & Sign"

Unterschreiben — Adobe-Kernfeature.

- [x] Freihand-Unterschrift zeichnen + speichern [U] — Sign-Tab, Canvas-Zeichnen, persistiert als PNG
- [x] Unterschrift als Bild einfügen [X: lopdf + image] — Import + Stamp mit Alpha (/SMask)
- [x] Initial-Stempel [U + X: lopdf]
- [x] Textfelder per Klick auf beliebige Stelle [U]
- [x] Datum, Häkchen, Kreuz-Stempel [U] — Klick-platzierte Marks (Ink/Shape/FreeText)
- [x] Digitale Signatur (PKCS#7/CMS, PAdES-B-B) — PKCS#12-Datei + Windows-Keystore (CNG) [crate `pdf_sign`, pure RustCrypto]
- [x] Signaturprüfung (ByteRange-Digest + CMS-Signatur + Signer-CN + Coverage) [X]
- [x] Zeitstempel-Dienst (RFC 3161) → PAdES-B-T [X: ureq]

> Hinweis: `pdf_sign` nutzt pure-RustCrypto (sauberer Windows-Build). PKCS#12
> nur mit PBES2 (AES/3DES) — alte RC2-`.pfx` neu exportieren mit
> `-keypbe AES-256-CBC -certpbe AES-256-CBC`. Signatur kryptographisch via
> OpenSSL-Oracle-Test verifiziert; mit echtem Adobe/`pdfsig` gegenprüfen.

**Zusätzlich mit v0.7 umgesetzt (UX/Infra, nicht im ursprünglichen Milestone):**

- [x] Eigener **Forms-Tab** — Feld-Navigation (vor/zurück), Validieren, Reset, Highlight-Toggle, Live-Feldzähler
- [x] Eigener **Sign-Tab** — Unterschrift zeichnen/tippen/importieren, Marks per Klick
- [x] **Foxit-Light-Theme** als Default + Hell/Dunkel-Umschalter in den Einstellungen
- [x] **i18n** — Deutsch (Standard) + Englisch, umschaltbar (`i18n.ts`, `t()`); komplette Oberfläche lokalisiert
- [x] **Nativer Windows-Druckdialog** (`PrintDlgW` + GDI/pdfium-Raster auf Drucker-DC) statt Still-Druck auf Standarddrucker

> Offen aus v0.6: Feld-Berechnungen (JS-Formeln). Status-Toasts in
> `edit.ts`/`doc.ts`/`chrome.ts`/`viewer.ts` noch nicht lokalisiert.

---

## Meilenstein v0.8 — "Sicherheit & Redaktion"

Anforderung aus Legal/Compliance. Foxit/Adobe-Parität.
Passwortschutz-Block (erste drei Items) ausgeliefert mit **v0.7.1**.

- [x] Passwort-geschützte PDFs öffnen (User + Owner Password) [E] — eigenes Passwort-Modal (window.prompt geht in WebView2 nicht), max. 3 Versuche *(v0.7.1)*
- [x] PDF mit Passwort schützen [X: lopdf, AES-256 V5/R6] — Schützen-Tab, Schutz wird beim Speichern angewendet *(v0.7.1)*
- [x] Berechtigungen setzen (Drucken, Kopieren, Ändern, Kommentieren) [X: lopdf /P-Bits] *(v0.7.1)*
- [x] Redaktion: Text dauerhaft schwärzen (Inhalt entfernt, nicht nur überdeckt) [X: lopdf + pdfium-Raster] — Schwärzen-Tool im Schützen-Tab; betroffene Seiten werden mit ~200 DPI gerastert und der Inhalt komplett ersetzt (inkl. Annotationen); E2E-Test prüft, dass der Text aus den Datei-Bytes verschwindet
- [x] Metadaten bereinigen (Sanitize) [X: lopdf] — entfernt /Info, XMP /Metadata (Catalog + Seiten) und /PieceInfo
- [x] Versteckte Inhalte finden (Layers, Kommentare, unsichtbarer Text) [X: lopdf-OCG + pdfium-RenderMode] — Scan im Schützen-Tab mit Report-Dialog: Ebenen inkl. Sichtbarkeit (/OCProperties /OFF), Kommentare/Anmerkungen (ohne Link/Popup/Widget), unsichtbarer Text per Zeichen-Rendermodus 3
- [x] Pattern-Redaktion (E-Mails, Telefonnummern per Regex) [X: pdfium-CharBoxes + regex] — Dialog im Schwärzen-Block (Presets E-Mail/Telefon + eigene Regex); Treffer werden zeilenweise als Schwärzungs-Markierungen vorgemerkt und über den normalen Anwenden-Flow entfernt

> Architektur Passwortschutz: lopdf 0.38 kann passwortgeschützte PDFs nicht
> laden (Reader parst nur das Encrypt-Dict). Deshalb: **PDFium entschlüsselt**
> beim Öffnen (`FPDF_SaveAsCopy` mit `FPDF_REMOVE_SECURITY`-Flag, raw binding —
> pdfium-render hardcodet flags=0) → Engine arbeitet immer auf Klartext-Bytes
> (Mutationen/Render unverändert) → **lopdf re-encryptet beim Speichern**
> (AES-256, `encrypt_pdf` in `pdf_mutate`; XRef-/ObjStm-Reste vorher strippen,
> sonst korrupte Ausgabe). Einstellungen pro Doc in `AppState.doc_crypto`.
> Einschränkungen: Digitales Signieren bei aktivem Schutz blockiert
> (Re-Encrypt würde ByteRange brechen); Signaturprüfung zeigt bei
> verschlüsselt signierten PDFs „ungültig“ (Bytes wurden geflattet);
> Owner-only-PDFs werden beim Speichern mit leerem User-Passwort + alten
> /P-Bits re-encryptet. Merge/Insert lehnen verschlüsselte Eingabedateien ab
> (`ENCRYPTED_INPUT`-Marker → lokalisierte Meldung); Dokument-Eigenschaften
> zeigen bei geschützten Docs die getrackten Einstellungen statt der
> Klartext-Engine-Sicht. E2E-Test: `pdf_pdfium/tests/encryption_roundtrip.rs`.

---

## Meilenstein v0.9 — "Export & Konvertierung"

Ohne Konvertierung kein PDF24-Ersatz.

- [x] PDF → PNG / JPG (pro Seite oder Bereich) [E] — Export-Dialog im Konvertieren-Tab (Format, 96–600 DPI, alle/aktuelle/Bereich), Dateien `name_pNNN.ext` in Zielordner
- [x] PDF → Plain Text [E] — alle Seiten, Leerzeile als Seitentrenner
- [x] PDF → HTML (einfach, für Copy-Paste) [E] — ein `<p>` pro Zeile, Seiten als `<section>` mit `<hr>`
- [x] PDF → Word (.docx) — Textfluss-rekonstruiert [X: docx-rs + heuristics] — neue Crate `pdf_export`: Zeilen/Absätze/Überschriften aus TextRun-Geometrie (flow.rs), Seitenumbrüche zwischen Seiten; E2E-Test `pdf_pdfium/tests/office_export.rs`
- [x] PDF → Excel (.xlsx) — Tabellen-Heuristik [X: rust_xlsxwriter + table-detection] — ein Arbeitsblatt pro Seite, Spalten per x-Start-Clustering (~10 pt Toleranz), Zahlen als Zahlen
- [ ] PDF → PowerPoint — optional, niedrige Prio (bewusst zurückgestellt)
- [x] Bilder → PDF (Multi-Page aus PNG/JPG) [X: lopdf + image] — Multi-Auswahl im Konvertieren-Tab, eine Seite pro Bild (JPEG verlustfrei eingebettet, Rest Flate-RGB), öffnet wie ein Merge-Ergebnis; E2E-Test `pdf_pdfium/tests/images_pdf.rs`
- [x] Office-Dateien → PDF (über System-Drucker-API oder LibreOffice-Headless) [S] — `soffice --headless --convert-to pdf` (Suche: Program Files + PATH), Ergebnis öffnet direkt in der App; ohne LibreOffice lokalisierter Hinweis

---

## Meilenstein v1.0 — "OCR & Scans" ✅

Gescannte Dokumente. Ohne OCR bleibt die App für ca. 20 % der echten PDFs blind.

- [x] OCR-Integration über Tesseract [X: Tesseract-CLI] — externes CLI statt
  leptess-Binding (kein vcpkg/nativer Build, gleiches Muster wie
  Ghostscript/LibreOffice): Erkennung über `TESSERACT`-Env, PATH,
  Program Files, `%LOCALAPPDATA%\Programs`; UB-Mannheim-Build empfohlen
- [x] Sprachpakete nachladen (UI-gesteuert) [S] — Download aus
  `tessdata_fast` in den App-Config-Ordner (`--tessdata-dir`), ⬇-Markierung
  im Sprach-Dropdown, `deu+eng`-Kombis über System- und User-Packs hinweg
- [x] OCR-Layer auf Seite (unsichtbarer Text über Bild) [X: lopdf] —
  `pdf_mutate::add_ocr_text_layer`: Rendermodus 3, Helvetica/WinAnsi,
  Tz-Breitenanpassung auf die Wortboxen, /Rotate 0/90/180/270
- [x] Batch-OCR über mehrere Dokumente [S] — Multi-Auswahl + Zielordner,
  Ausgabe `<name>_ocr.pdf`, Seiten mit Text und verschlüsselte Dateien
  werden übersprungen
- [x] Bildqualität-Vorverarbeitung (Deskew, Despeckle, Kontrast) [X: image] —
  Kontrast-Stretch (1–99-Perzentil), 3×3-Median, Deskew ±5° per
  Projektionsprofil auf Otsu-Binärbild; abschaltbar im Dialog
- [x] Auto-Erkennung: "Diese PDF enthält keinen Text, OCR starten?" [C][U] —
  Banner über dem Viewer nach dem Öffnen (erste 5 Seiten ohne Text)

> E2E-Tests: `pdf_pdfium/tests/ocr_layer.rs` (Layer durchsuchbar/unsichtbar/
> positionsgenau, Rotation) und `desktop-tauri` `ocr::tests::tesseract_roundtrip`
> (gegen echtes Tesseract, übersprungen wenn nicht installiert).

---

## Meilenstein v1.1 — "Performance & Polish"

Harter Schnitt: alles flüssig, stabil, mitnehmbar.

- [ ] Render-Worker-Pool (N Threads, Priorisierung nach Viewport) [C]
- [ ] Progressive Rendering (niedrige Auflösung zuerst, dann scharf) [C][E]
- [ ] GPU-beschleunigtes Blitting der Seitenbilder (wgpu-Backed Canvas optional) [X: wgpu]
- [ ] Virtuelles Scrollen im kontinuierlichen Modus (nur sichtbare Seiten) [U]
- [ ] Thumbnail-Cache auf Disk persistieren [S][pdf_cache]
- [ ] Memory-Footprint-Test mit 1000-Seiten-PDFs
- [ ] MSI-Installer inkl. WebView2 Bootstrapper [X: WiX]
- [ ] Auto-Update (Tauri Updater Plugin) [X]
- [ ] Crash-Reporting opt-in (Sentry oder eigenes) [X]
- [ ] Lokalisierung: de / en mindestens [U]
- [ ] Accessibility: Keyboard-Nav, Screenreader-Labels, High-Contrast [U]
- [x] Erweitertes Settings-Fenster (Standard-Zoom, Theme, Sprache, Cache-Größe)
- [ ] Tray-App / Autostart für Hintergrund-Spool-Watcher (Print-to-PDF ohne geöffnete Haupt-App)

---

## v1.2+ — Nischen & Pro-Features

Reihenfolge nach Nutzerbedarf sortieren, nicht streng umsetzen.

### Dokumentenvergleich
- [ ] PDF-Vergleich Seite-an-Seite mit Diff-Markierung (Text + Layout) [C]
- [ ] Report-Export des Vergleichs

### Formular-Erstellung
- [ ] Formular-Designer (Textfelder, Checkboxen platzieren) [U][X: lopdf]
- [ ] XFA-Support read-only (seltener, aber vorhanden) [X]

### Erweiterte PDF-Features
- [ ] Dateigrößenoptimierung (Bilder rekomprimieren, Metadaten strippen) [X: lopdf + image]
- [ ] Optional Content Groups (Layer ein/aus) [E][U]
- [ ] JavaScript-Actions in Formularen sicher ausführen [X: sandboxed JS]
- [ ] 3D-Annotationen (U3D/PRC) — nur Anzeige [X: optional]
- [ ] PDF/A-Validierung (veraPDF-CLI Bindung) [X]
- [ ] PDF/X für Print-Workflows
- [ ] Linearisierung (Fast Web View) beim Speichern [X: lopdf/qpdf-Bindung]
- [ ] Bates-Nummerierung (Legal) [X: lopdf]

### Enterprise / Workflow
- [ ] Batch-Prozessor UI (Dateien rein → Aktion → Export) [U][S]
- [ ] CLI-Modus (`rustpdf merge a.pdf b.pdf -o out.pdf`) [C]
- [ ] Watch-Folder (Auto-OCR, Auto-Sign)
- [ ] SharePoint / WebDAV / S3-Integration [S]
- [ ] Plugin-System (WASM-basierte Erweiterungen) [C][X: wasmtime]

### Kollaboration
- [ ] Kommentar-Export als JSON/CSV
- [ ] Multi-User-Annotations über Datei-Lock oder eigene Backend-API

---

## Linux-Portierung (parallel zur Roadmap)

Wenn v0.3 erreicht ist, Linux als zweites Ziel:

- [ ] `libpdfium.so` in `vendor/pdfium/linux/x64/` beschaffen
- [ ] CI-Matrix erweitern: `ubuntu-latest` mit `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`
- [ ] Dateidialoge/Opener auf XDG-Portal prüfen (Tauri-Plugin macht das automatisch)
- [ ] AppImage + .deb Packaging
- [ ] Signatur-Backend: Windows-Cert-Store → unter Linux auf PKCS#11 / Datei-Zertifikat umschalten (Trait `CertStore` in `pdf_services` erweitern)
- [ ] Virtueller Drucker: CUPS-Backend statt Windows Print Spooler (v0.4 Äquivalent)

---

## Architektur-Regeln für jedes Feature

Bevor ein Feature umgesetzt wird, zuordnen:

1. **Core-Logik** in `pdf_core` (UI-frei, OS-frei).
2. **PDFium-spezifisch** in `pdf_pdfium`; reine Dokument-Mutation über `lopdf` in eigenem Adapter-Crate (z. B. `pdf_mutate`).
3. **OS-Integration** (Drucker, Cert-Store, Dateidialog) als Trait in `pdf_services`, Impl im Tauri-Crate.
4. **UI-State** nie aus Core übergehen — nur Deltas als serialisierbare Typen aus `pdf_types`.
5. **Async**: alles > 16 ms läuft in `spawn_blocking` oder dediziertem Worker-Pool. UI nie blockieren.
6. **Neue Abhängigkeit**: Lizenz prüfen (kompatibel zu MIT/Apache-2.0). PDFium ist Apache-2.0/BSD.

---

## Entscheidungen, die früh getroffen werden müssen

Bevor v0.3+ anfängt, lohnt sich ein Proof-of-Concept:

| Thema | Optionen | Empfehlung |
|-------|----------|------------|
| Mutations-Backend | `lopdf` vs. `pdfium-render` Edit-APIs vs. eigenes | `lopdf` für Strukturop, PDFium nur für Render/Text |
| Digitale Signatur | `ring` + `pkcs7` vs. `openssl` vs. Windows CNG | Windows CNG für nativen Cert-Store, `pkcs7`-Crate für CMS-Encoding |
| OCR-Laufzeit | Tesseract nativ vs. WASM-Port | Nativ via `leptess`, System-Tesseract optional bundeln |
| JS-Engine für Formularskripte | `boa` (pure Rust) vs. `quickjs` vs. `v8` | `boa` — klein, sicher, reicht für AcroForm-Formeln |
| Installer | WiX MSI vs. NSIS vs. MSIX | NSIS für v0.4 (schneller, flexible Hooks für Druckertreiber-Setup); MSI via WiX später für Enterprise |
| Rendering-Fallback | PDFium only vs. zusätzlich `mupdf` | PDFium bleibt primär; mupdf nur wenn Lizenz erlaubt und Features fehlen |
| Print-to-PDF Backend | Eigener v4-Treiber vs. PS Class Driver + GS vs. XPS + GXPS | PS Class Driver + Ghostscript `pdfwrite` — keine Treiber-Signatur nötig, GS ohnehin für PDF/A |
