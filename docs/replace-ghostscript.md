# Ghostscript ablösen — Analyse und Plan (Issue #2)

Stand: 27.11.2026, Basis `main` = v1.4.0 (48e94d1). Windows 11 Enterprise
26H2 (Build 26300), Ghostscript 10.07.0 lokal installiert (nur für den
Vergleich).

> **Status: zurückgestellt (in Arbeit).** Phase 1 (Analyse) ist bis auf die
> Punkte unter „Offen“ (§8) fertig. Phase 2 (Umsetzung) ist nicht begonnen,
> die Entscheidungen in §6 stehen aus.

**Kurzfassung.** Nach #21 hängen noch zwei Funktionen an Ghostscript: der
virtuelle Drucker „Docura PDF“ (PostScript → PDF, Pflicht) und der alte
PDF/A-Konverter (wählbare Alternative, nicht Standard). Empfehlung: Der Drucker
bekommt den in Windows eingebauten Treiber **„Microsoft Print To PDF“** auf
unserem eigenen Datei-Port; die App verarbeitet das fertige PDF in Rust nach
(Schriften verkleinern, Metadaten, optional PDF/A wie heute). Der alte
PDF/A-Weg über Ghostscript entfällt. Im Spike war das Ergebnis in jeder
Hinsicht besser als heute (durchsuchbarer Text, echte Seitenformate bis A0,
keine Ränder, Jobname als Titel), nur **langsamer** (1,2- bis 3-mal so lange,
bis das PDF fertig ist) und ohne Nachbearbeitung **größer** (Schriften).
Aufwand Phase 2: etwa 6 bis 8 Personentage.

## 1. Wo Ghostscript heute steckt

| # | Stelle | Dateien | Funktion für den Nutzer |
|---|---|---|---|
| 1 | Virtueller Drucker: Druckauftrag (PostScript) → PDF | `src-tauri/src/vprinter.rs` (`process_one`: `gswin64c -sDEVICE=pdfwrite`), `nsis/printer-setup.ps1` und `vprinter.rs::install_virtual_printer` (Treiber „Microsoft PS Class Driver“, Port `…\DocuraPDF\spool\docura-spool.ps`) | Aus jeder Windows-Anwendung auf „Docura PDF“ drucken; das PDF öffnet sich in Docura und liegt in `Dokumente\Docura PDF`. Ohne Ghostscript scheitert jeder Auftrag (`failed-nogs-*.ps`, Meldung „Ghostscript not installed“). |
| 2 | Virtueller Drucker mit PDF/A-Standardstufe und Konverter „Ghostscript“ | `vprinter.rs` (`-dPDFA=…` direkt im Ghostscript-Aufruf) | Druckausgabe gleich als PDF/A-1b/2b/3b. Mit dem integrierten Konverter läuft PDF/A schon heute danach in Rust. |
| 3 | PDF/A-Export „Speichern als PDF/A“, Konverter „Ghostscript“ | `src-tauri/src/lib.rs` (`save_as_pdfa`, `pdfa_flags`, `gs_output_file_arg`), `src-tauri/src/pdfa.rs` (`PdfaEngine::Ghostscript`) | Alternative zum integrierten Konverter (Standard seit 1.4.0), nur 1b/2b/3b, Formulare werden flach, veraPDF: 0 von 47 Dateien gültig (siehe `docs/pdfa-rewrite.md`). |
| 4 | Stapelkonvertierung nach PDF/A, Konverter „Ghostscript“ | `lib.rs` (`convert_files_to_pdfa`) | wie 3, für mehrere Dateien. |
| 5 | Erkennung von Ghostscript | `lib.rs` (`ghostscript_candidates`, `probe_ghostscript`, `find_ghostscript`, Befehl `check_ghostscript`), `vprinter.rs::virtual_printer_status` (`ghostscript_available/_version`) | Einstellungen zeigen „Ghostscript x.y — Pfad“ bzw. „nicht gefunden“. Erkennung über `GHOSTSCRIPT`, PATH, `C:\Program Files\gs\*`. |
| 6 | Oberfläche | `ui/src/chrome.ts` (`refreshGhostscript`, `runSaveAsPdfa`, `refreshVirtualPrinter`), `ui/src/gsview.ts`, `ui/src/pdfa.ts` (`GS_LEVELS`, `isBuiltin`), `ui/src/template.ts` (Link ghostscript.com, Auswahl „Konverter“), `ui/src/state.ts`, `ui/src/types.ts`, `ui/src/dom.ts`, `ui/src/doc.ts`, `ui/src/i18n.ts` (`gs.*`, `set.gsChecking`, `btn.recheckGs`, `set.vpWarn`, `st.gsRequired`, `pdfa.engine.ghostscript`, `pdfa.err.gs_level`, Hinweistext `set.pdfaHint`), `ui/tests/hidden.test.mjs` | Einstellungen › PDF/A-Export: Status, „Ghostscript erneut prüfen“, Download-Link, Konverter-Auswahl; Einstellungen › Virtueller Drucker: Warnung „Ghostscript wird benötigt“; Statusmeldung beim Export. |
| 7 | Einstellung `pdfa_engine` | `lib.rs` (`PersistedSettings.pdfa_engine`, `get/set_pdfa_engine`) | Gespeicherte Konverter-Wahl (`builtin`/`ghostscript`). |
| 8 | Installer | `nsis/hooks.nsi` (`DocuraOfferGhostscript`: prüft `HKLM\SOFTWARE\GPL Ghostscript`, bietet bei der Erstinstallation die Download-Seite an) | Frage „Ghostscript wurde nicht gefunden. Download-Seite öffnen?“ |
| 9 | Entwicklerwerkzeuge | `crates/pdf_pdfa/examples/pdfa_bench.rs` (Vergleich mit Ghostscript), `crates/pdf_pdfa/tests/fidelity.rs` (Beispiel-PDFs aus der Ghostscript-Installation als Testkorpus), `justfile` (`pdfa-bench`), `scripts/pdfa-check.ps1` | Nicht ausgeliefert. Ghostscript bleibt dort optionaler Vergleich. |
| 10 | Dokumentation | `README.md` (Voraussetzungen, Installer), `docs/roadmap.md`, `docs/pdfa-rewrite.md` §8, `CHANGELOG.md` | — |

Nicht betroffen (geprüft): Drucken aus Docura (`print_win.rs`, PDFium →
GDI), OCR (Tesseract), Office → PDF (LibreOffice), Komprimieren/Optimieren
(eigener Code in `pdf_pdfa`/`pdf_mutate`), Vorschaubilder (PDFium). Weitere
Aufrufe externer Programme gibt es nicht (`Command::new` durchsucht).

## 2. Was der heutige Drucker wirklich liefert (Spike)

Der „Microsoft PS Class Driver“ ist ein v4-Treiber: Windows wandelt den
GDI-Druck der Anwendung erst in XPS um, der Filter `MSxpsPS.dll` macht daraus
PostScript, Ghostscript daraus PDF. Für den Vergleich habe ich drei
Testdrucker auf Dateiports im Scratch-Verzeichnis angelegt (ohne Adminrechte
möglich, danach wieder entfernt):

- **PS**: „Microsoft PS Class Driver“ + Ghostscript (`-sDEVICE=pdfwrite`, wie die App) — heutiger Weg
- **MS-PDF**: „Microsoft Print To PDF“ (Windows-Bordmittel) auf eigenem Port — Option (a)
- **OXPS**: „Microsoft OpenXPS Class Driver 2“ (liefert das XPS ohne Filter) — Eingang für Option (b)

Druckaufträge: (1) eine GDI+-Testseite (System.Drawing `PrintDocument`, wie
typische Windows-Programme): drei Seiten A4 hoch, A3 quer, A0 hoch; Text in
Arial, Times, Segoe UI, Consolas, Calibri, Cambria Math, Wingdings mit
Umlauten und Sonderzeichen, 4-pt-Text, gedrehter Text, Linien, Kurven,
Verlauf, Transparenz, Schraffur, ein Foto; ein CAD-artiges Blatt mit 2 000
Linien. (2) Die fünf Beispielpläne des Owners, gedruckt wie ein Browser oder
PDF-Betrachter druckt (PDFium `FPDF_RenderPage` auf den Drucker-DC, je Seite
das Format der PDF-Seite angefordert). Zeit = vom Druckbefehl bis die Datei
am Port vollständig ist (+ Ghostscript beim PS-Weg).

### Angebotene Seitenformate (PrintCapabilities)

| Treiber | Formate |
|---|---|
| PS Class Driver (heute) | **nur Letter und A4** (PPD `MSxpsPS.ppd`) |
| XPS/OpenXPS Class Driver | nur Letter und A4 (GPD/PDC) |
| Microsoft Print To PDF | 35: A0, A1, A2, A3, A4, A5, A6, JIS B4/B5, Letter, Legal, Tabloid, ANSI C/D/E, Umschläge, Fotoformate. **Keine freien Formate.** |

Freie Formate (z. B. 841 × 1500 mm) bietet keiner der eingebauten Treiber.
Für „Microsoft Print To PDF“ geht das nur durch Ändern der Treiberdatei im
Windows-Treiberspeicher, was wir nicht tun.

### Testseite

| | PS + Ghostscript (heute) | MS-PDF (a) | OXPS (Eingang für b) |
|---|---|---|---|
| Seite 1 (A4 angefordert) | 210 × 297 | 210 × 297 | 210 × 297 |
| Seite 2 (A3 quer) | **A4** (gedreht) | 420 × 297 | Letter quer |
| Seite 3 (A0) | **Letter** | 841 × 1189 | Letter |
| Text durchsuchbar/kopierbar | **nein**: Schriften ohne ToUnicode, Textextraktion liefert Zeichensalat („'RFXUD3')±…“) | **ja**, alle Zeichen korrekt (€ ‰ „“ → ✓ ß) | ja, `UnicodeString` steht im XPS (ein Konverter müsste ToUnicode schreiben) |
| Schriften | eingebettet, Teilmenge | eingebettet (CIDFontType2, ToUnicode), Umrisse schon reduziert, aber GPOS/GSUB/hdmx vollständig | vollständige Schriften (obfuskiert, `.odttf`) |
| Ränder | 0,5″ unbedruckbar: Inhalt verschoben, rechte Kante abgeschnitten | randlos | — |
| Titel im PDF | „MSxpsPS“ | **Jobname** („Spike Testdokument“) | — |
| Vektorgrafik, Verlauf, Transparenz, Foto | ja (Verlauf/Transparenz als Bild) | ja (gleich) | ja |
| Zeit bis PDF fertig (3 Läufe) | 1,6–1,7 s + 0,5 s Ghostscript | **6,8–8,4 s** | 1,6–3,9 s + eigener Konverter |
| Größe | 261 KiB | **1 048 KiB** (davon 904 KiB Schriften; nur die PDF-relevanten Tabellen: 257 KiB) | 1 187 KiB XPS |

Optisch sind PS- und MS-PDF-Ausgabe gleich (gerendert und verglichen), bis
auf die Ränder des PS-Wegs.

### Beispielpläne (PDFium → GDI-Vektordruck)

| Plan | PDF-Format | PS + GS: Format / Zeit / Größe | MS-PDF: Format / Zeit / Größe | OXPS: Format / Zeit / Größe |
|---|---|---|---|---|
| plan1 (181 KiB, Vektor) | 1189 × 841 | Letter / 1,7 s / **leere Seite** | 1189 × 841 / 2,4 s / 570 KiB | Letter / 1,3 s / 266 KiB |
| plan2 (9,5 MiB, Bild) | 1189 × 841 | Letter / 3,3 s / **leere Seite** | 1189 × 841 / 3,9 s / 452 KiB | Letter / 2,9 s / 368 KiB |
| plan3 (6,4 MiB, Bild) | 1189 × 841 | Letter / 3,1 s / **leere Seite** | 1189 × 841 / 4,3 s / 383 KiB | Letter / 3,2 s / 283 KiB |
| plan4 (11,5 MiB, Bild) | 1500 × 841 | Letter / 3,8 s / 313 KiB | A4 quer (kein passendes Format) / 5,0 s / 391 KiB | Letter / 3,4 s / 303 KiB |
| plan5 (1,2 MiB, 4 × A4) | 210 × 297 | A4 / 4,6 s / 2 095 KiB | A4 / 9,5 s / 1 779 KiB | A4 / 2,5 s / 956 KiB |

Leere Seite: Der PS-Filter schrieb für drei Pläne nur ein leeres
`showpage`, wenn das angeforderte Format (A0 quer) nicht angeboten wird. Mit
dem Standardformat A4 kam der Inhalt an. Das ist ein Fehler im heutigen Weg,
nicht in Docura; ich habe ihn nur mit meinem Testprogramm gesehen, nicht mit
einem zweiten Programm geprüft. Text auf den Plänen wird von PDFium beim
Drucken als Kurven ausgegeben, in allen drei Wegen.

## 3. Optionen für den virtuellen Drucker

### (a) „Microsoft Print To PDF“ auf eigenem Port — empfohlen

Der Drucker „Docura PDF“ bekommt statt des PS-Treibers den eingebauten
Treiber „Microsoft Print To PDF“ und den Port `…\DocuraPDF\spool\docura-spool.pdf`.
Windows schreibt das fertige PDF in die Datei, Docura holt es dort ab wie
heute die `.ps`-Datei, verarbeitet es in Rust nach und öffnet es.

- **Treue:** Schriften eingebettet mit ToUnicode → Text durchsuchbar
  (heute nicht). Vektorgrafik bleibt Vektor. Farbe RGB (wie heute; GDI kennt
  nur RGB). Formate A0–A6, ANSI, US-Formate; keine freien Formate (heute nur
  Letter/A4). Randlos (heute 0,5″ Rand).
- **Metadaten:** `/Title` = Jobname (z. B. Dokumentname aus Word), heute
  „MSxpsPS“. Autor fehlt in beiden Wegen. Docura setzt `/Producer`/`/Creator`
  nach und kann den Dateinamen aus dem Titel bilden.
- **Windows 10/11:** Der Treiber gehört seit Windows 10 zum System
  (optionales Feature `Printing-PrintToPDFServices-Features`, standardmäßig
  an). Er kann von Admins abgeschaltet sein; der Installer prüft das und
  schaltet ihn bei Bedarf ein (DISM, läuft ohnehin als Admin). Die Formatliste
  habe ich nur unter Windows 11 26H2 gesehen; unter Windows 10 kann sie
  kürzer sein (ältere Berichte nennen A3 als größtes Format) — in Phase 2 auf
  einem Windows-10-Rechner prüfen, falls vorhanden.
- **Nachteile:** langsamer (Microsofts XPS→PDF-Filter im Spooler, siehe
  Messungen); größere Dateien ohne Nachbearbeitung (vollständige
  Layout-Tabellen in den Schriften). Das zweite gleicht die Nachbearbeitung
  aus: Der vorhandene Schriften-Verkleinerer in `pdf_pdfa` (`fonts/shrink.rs`)
  greift bei dieser Ausgabe noch nicht und muss das lernen (Ursache in Phase 2;
  Schätzung: Schriften 904 → ≤ 257 KiB).
- **Aufwand:** klein bis mittel (Druckereinrichtung, Watcher, Nachbearbeitung,
  Migration, Tests), Risiko gering: nur Windows-Bordmittel, kein eigener
  Treiber, keine Signatur nötig.

### (b) XPS-Warteschlange + eigener XPS→PDF-Konverter in Rust

„Microsoft XPS Class Driver“ bzw. „OpenXPS Class Driver“ (im selben
Treiberpaket `prnms005` wie der heutige PS-Treiber, also überall dort
vorhanden, wo es heute klappt) schreibt das XPS ohne Filter in den Port.
Docura wandelt es selbst um.

- **Treue:** potenziell am besten: `Glyphs` tragen Glyph-IDs und Unicode,
  Schriften liegen bei (vollständig, obfuskiert), Pfade und Bilder sind
  direkt übertragbar. Schnell (kein Spooler-Filter; Spool 1,3–3,9 s).
- **Aber:** derselbe Formatdeckel wie heute: **nur Letter und A4**. Größere
  Formate gingen nur mit einem eigenen Treiberpaket (Daten-only v4-Treiber mit
  eigener Formatliste), das signiert sein muss (Code-Signing-Zertifikat, ggf.
  Microsoft-Attestierung), oder mit (d).
- **Aufwand:** groß: OPC-Paket mit verschachtelten Teilen (`[n].piece`),
  `.odttf`-Deobfuskierung, Untermengen der Schriften (vorhanden in
  `pdf_pdfa/fonts/sfnt.rs`), Pfad-Minisprache, Pinsel (Farbe, Bild, Verläufe,
  VisualBrush), Transformationen, Clip, Deckkraft und Deckkraftmasken,
  Bildformate PNG/JPEG/TIFF und ggf. JPEG XR. Geschätzt 3–5 Wochen bis zu
  verlässlicher Treue. Risiko mittel.

### (c) PostScript-Interpreter in Rust oder unter freier Lizenz

- Es gibt keinen produktionsreifen PostScript-Interpreter in Rust und keinen
  unter einer freizügigen Lizenz, der PDF schreibt (Ghostscript und GhostPDL
  sind AGPL/kommerziell; die freien Alternativen sind Lehr- oder
  Spielprojekte).
- Der PostScript-Strom entsteht ohnehin aus XPS (`MSxpsPS.dll`). Ein eigener
  Interpreter für genau diese Ausgabe würde dieselbe Information wie (b)
  verarbeiten, nur nach einem verlustbehafteten Umweg (der Text verliert
  dabei heute schon seine Unicode-Zuordnung). (c) ist damit (b) in jeder
  Hinsicht unterlegen. Aufwand: Monate. Nicht empfohlen.

### Weitere Wege (der Vollständigkeit halber)

- **(d) Print-Support-App mit virtuellem Drucker** (`printSupportVirtualPrinter`,
  Treiber „Microsoft Virtual Print Class Driver“, wie „OneNote (Desktop) -
  Protected“): eigene Formatliste möglich, Windows liefert PDF oder XPS, und
  es ist der von Microsoft vorgesehene Weg unter dem „Windows Protected Print
  Mode“. Aber: nur Windows 11 (einige Funktionen ab 22H2), braucht eine
  MSIX-Paketidentität (für eine NSIS-App ein signiertes „Sparse Package“,
  also ein Code-Signing-Zertifikat). Für später, wenn Windows 10 keine Rolle
  mehr spielt.
- **(e) Eigener Druckertreiber/Portmonitor:** Signatur nötig, Microsoft baut
  Drittanbieter-Druckertreiber gerade ab. Nein.
- **(f) Kommerzielle Ghostscript-Lizenz (Artifex):** würde das Mitliefern
  erlauben, löst aber weder die Treueprobleme (kein durchsuchbarer Text, nur
  Letter/A4) noch die Abhängigkeit. Nur erwähnt.

### Bewertung

| Kriterium | heute (PS + GS) | (a) MS Print To PDF | (b) XPS + Rust | (c) PS-Interpreter |
|---|---|---|---|---|
| Ghostscript nötig | ja | nein | nein | nein |
| Text durchsuchbar | nein | ja | ja | nein (Info fehlt im PS) |
| Seitenformate | Letter, A4 | A0–A6, ANSI u. a. (35) | Letter, A4 | Letter, A4 |
| Vektor, Farbe | ja, RGB | ja, RGB | ja, RGB | ja, RGB |
| Ränder | 0,5″ | randlos | randlos | 0,5″ |
| Jobname als Titel | nein | ja | nein (nicht im XPS) | nein |
| Zeit (Spike) | 2,2 s / 1,7–4,6 s | 6,9 s / 2,4–9,5 s | ~2–4 s + Konverter | wie heute |
| Größe vor/nach Nachbearbeitung | klein | groß / ähnlich wie heute | offen | offen |
| Windows 10 | ja | ja (Formatliste prüfen) | ja | ja |
| Aufwand | — | ~6–8 PT gesamt | 3–5 Wochen + Formatproblem | Monate |
| Risiko | — | gering | mittel | hoch |

## 4. Ersatz je Funktion

| # | Funktion | Ersatz ohne Ghostscript | Aufwand | Risiko |
|---|---|---|---|---|
| 1 | Drucker → PDF | (a): `printer-setup.ps1`/`vprinter.rs` richten „Docura PDF“ mit „Microsoft Print To PDF“ und Port `docura-spool.pdf` ein; ein bestehender Drucker wird beim Update umgestellt (Treiber + Port), der alte `.ps`-Port entfernt. Watcher holt `.pdf` ab, wartet, bis der Spooler die Datei freigibt (exklusives Öffnen statt nur Größenvergleich), prüft das PDF mit lopdf, verkleinert die Schriften, setzt Producer/Creator, bildet den Dateinamen aus dem Titel. | 3–4 PT | gering |
| 2 | Drucker mit PDF/A | Läuft schon heute mit dem integrierten Konverter nach der Umwandlung; der Ghostscript-Zweig entfällt. | im Punkt 1 | gering |
| 3, 4 | PDF/A-Export, Stapel | Nur noch integrierter Konverter (`pdf_pdfa`, alle Stufen, veraPDF-geprüft). Ghostscript-Zweige, `pdfa_flags`, `gs_output_file_arg`, `PdfaEngine::Ghostscript` entfallen. Gespeichertes `pdfa_engine = "ghostscript"` wird beim Laden als `builtin` gelesen. | 0,5 PT | gering |
| 5–7 | Erkennung, Oberfläche, Einstellung | `check_ghostscript`, `gsview.ts`, Ghostscript-Status und -Link, Konverter-Auswahl, „Ghostscript wird benötigt“ entfallen; Drucker-Status zeigt stattdessen, ob der Windows-Treiber vorhanden ist. i18n DE/EN anpassen, `hidden.test.mjs` umstellen. | 1 PT | gering |
| 8 | Installer | `DocuraOfferGhostscript` entfällt; Druckereinrichtung prüft/aktiviert „Microsoft Print To PDF“. Test nach dem Verfahren aus #4 (Per-User-Varianten, ein UAC-Lauf mit Zustimmung). | 0,5–1 PT | gering |
| 9 | Entwicklerwerkzeuge | `pdfa_bench` bleibt (Ghostscript optional als Vergleich). Neu: Drucker-Benchmark alter gegen neuen Weg. | in Punkt 1/Tests | — |
| 10 | Doku | README (Voraussetzungen ohne Ghostscript), CHANGELOG, roadmap, `pdfa-rewrite.md` §8. | 0,5 PT | — |

## 5. Was schlechter oder inkompatibel wird

1. **Zeit beim Drucken.** Microsofts Filter braucht länger als PS-Filter +
   Ghostscript: im Spike 1,2- bis 3-mal so lange, bis das PDF fertig ist
   (Testseite 2,2 → 6,9 s, 4 Seiten A4 mit Scan 4,6 → 9,5 s, A0-Pläne
   1,7–3,8 → 2,4–5,0 s). Die Anwendung, die druckt, ist gleich schnell fertig;
   nur das Öffnen in Docura kommt später. Beeinflussen können wir das nicht.
2. **Dateigröße ohne Nachbearbeitung** größer (Schriften mit allen
   Layout-Tabellen); mit Nachbearbeitung voraussichtlich ähnlich wie heute.
   Wird in Phase 2 gemessen und gehört in den Benchmark.
3. **Freie Seitenformate** gibt es weiterhin nicht (heute auch nicht). Ein
   Plan 841 × 1500 mm landet auf A4 quer, verkleinert (heute: Letter).
4. **Abhängigkeit vom Windows-Feature** „Microsoft Print to PDF“: Ist es per
   Richtlinie entfernt und lässt es sich nicht aktivieren, gibt es keinen
   Drucker (heute: kein Drucker ohne Ghostscript). Unter Windows 10 kann die
   Formatliste kürzer sein (zu prüfen).
5. **Konverter-Wahl „Ghostscript“ für PDF/A entfällt.** Wer ihn wegen
   kleinerer Dateien nutzt (Ghostscript rechnet Fotos aggressiver als JPEG
   um; Korpus 2b: 280 KiB gegen 1 381 KiB), bekommt größere, dafür gültige
   PDF/A-Dateien (Ghostscript-Ausgabe: 0/47 veraPDF-gültig; bei den Plänen ist
   der integrierte Konverter kleiner und 9-mal schneller).
6. **Bestehende Druckereinstellungen** (Papierformat-Vorgabe von „Docura
   PDF“) gehen beim Treiberwechsel verloren; der Name des Druckers bleibt.
   Liegen beim Update noch unverarbeitete `.ps`-Aufträge im Spool-Ordner,
   kann Docura sie ohne Ghostscript nicht mehr umwandeln (Meldung, Datei
   bleibt liegen).
7. **Windows Protected Print Mode** (optional, Unternehmen): Ob ein lokaler
   Dateiport dort erlaubt ist, ist offen — das gilt heute genauso. Der
   zukunftssichere Weg wäre (d).

## 6. Entscheidungen für den Owner

1. **Druckerweg:** (a) „Microsoft Print To PDF“ auf eigenem Port
   (empfohlen) — oder (b) eigener XPS-Konverter (schneller, aber nur
   Letter/A4 und mehrere Wochen Aufwand)?
2. **Ghostscript ganz entfernen oder als optionalen Rückfall behalten?**
   Empfehlung: ganz entfernen (Drucker und PDF/A). Ein Rückfall müsste den
   alten Treiber und den Ghostscript-Code weiter pflegen und testen und hätte
   die alten Schwächen.
3. **Langsamere Druckumwandlung** (bis etwa 3-mal so lange) für
   durchsuchbaren Text und echte Formate akzeptieren?
4. **Dateiname der Druckausgabe** aus dem Jobnamen bilden (z. B.
   `Rechnung 4711.pdf`, bei Kollision mit Zähler) statt `printed-<Zeit>.pdf`?
   Empfehlung: ja.
5. **Fehlt das Windows-Feature**, soll der Installer es selbst aktivieren
   (DISM, Admin) oder nur einen Hinweis zeigen? Empfehlung: aktivieren und
   protokollieren.

## 7. Plan Phase 2 (nach Entscheidung)

1. Druckereinrichtung (`printer-setup.ps1`, `vprinter.rs`), Migration
   bestehender Drucker, Watcher für `.pdf`, Nachbearbeitung in Rust
   (Schriften, Metadaten, Dateiname, PDF/A wie bisher).
2. Ghostscript-Code, -Oberfläche, -Texte und Installer-Frage entfernen.
3. Tests („100 % der Funktionen“ = jede Funktion aus §1 hat einen Weg ohne
   Ghostscript und Tests dafür):
   - Unit: Watcher-Logik (Dateiauswahl, Freigabe, Fehlerfälle, Dateiname aus
     Titel), Nachbearbeitung (Schriften kleiner, Glyphen gleich, Text
     extrahierbar, Metadaten), Einstellungs-Migration, Installer-Skript-
     Konsistenz (wie `installer_printer_setup_matches_app`).
   - Ende-zu-Ende (Windows, ohne Admin): Test legt einen Testdrucker mit dem
     Treiber auf einem Temp-Port an, druckt eine GDI-Testseite (A4, A3 quer,
     A0), prüft Formate, durchsuchbaren Text, Titel und räumt auf.
   - PDF/A: bestehende Tests/veraPDF-Matrix, plus Druckausgabe → PDF/A.
   - UI: `tsc`, `node --test`.
4. Benchmark (Beispielprogramm, `just`-Rezept): alter Weg (PS-Treiber +
   Ghostscript, falls installiert) gegen neuen Weg, Zeit und Größe für die
   Testseite, eigene Testdokumente und lokal die Pläne (nicht im Repository).
5. Doku, CHANGELOG, PR auf Deutsch.

## 8. Offen

Noch nicht analysiert oder bestätigt:

- [ ] Entscheidungen des Owners (§6), vorher keine Umsetzung.
- [ ] Windows 10: Formatliste von „Microsoft Print To PDF“ (A0–A2 vorhanden?)
      und Verhalten auf eigenem Dateiport; kein Windows-10-Rechner getestet.
- [ ] Warum der Schriften-Verkleinerer (`pdf_pdfa/src/fonts/shrink.rs`) die
      Schriften der Microsoft-Ausgabe nicht anfasst (PDF/A-2b: 1 048 →
      1 034 KiB statt geschätzt ~400 KiB). Die Ausgabe selbst verarbeitet
      `pdf_pdfa` problemlos (PDF/A-2b in 28 ms).
- [ ] Leere Seiten im heutigen PS-Weg (§2, Pläne) mit einem zweiten Programm
      bestätigen (z. B. Acrobat/Edge mit „Papierquelle nach PDF-Seitengröße“).
- [ ] Zeiten belastbar messen (mehr Läufe, mehr Dokumenttypen: Word, Excel,
      Browser, CAD) und klären, wovon die längere Zeit des Microsoft-Filters
      abhängt (Bilder? Schriften?).
- [ ] Erkennen, wann der Spooler die PDF-Datei fertig geschrieben hat
      (exklusives Öffnen) — im Spike nur über die Warteschlange gelöst.
- [ ] Verhalten bei gleichzeitigen Aufträgen und mehreren angemeldeten
      Benutzern (ein gemeinsamer Port wie heute).
- [ ] Windows Protected Print Mode: sind lokale Dateiports und „Microsoft
      Print To PDF“ dort erlaubt? Keine verlässliche Microsoft-Quelle gefunden.
- [ ] Option (d) Print-Support-App nur grob bewertet (Windows-Mindestversion,
      Paketierung, Zertifikat).
- [ ] Hyperlinks, Lesezeichen und Formularfelder aus druckenden Programmen
      gehen in allen Druckwegen verloren (GDI-Druck) — nicht weiter untersucht.

## Anhang: Spike

Die Spike-Programme liegen nicht im Repository (Scratch): eine
GDI+-Testseite (C#/PowerShell), ein Druck-Probe-Programm (Rust, PDFium
`FPDF_RenderPage` auf den Drucker-DC mit Seitenformat je Seite) und
Auswertungen mit pypdf. Die Testdrucker „Docura Spike PS/PDF/OXPS“ und ihre
Ports wurden danach entfernt. Für Phase 2 wird die Testseite als Rust-Test
neu geschrieben.
