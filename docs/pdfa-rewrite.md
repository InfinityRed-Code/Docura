# PDF/A ohne Ghostscript — Analyse und Plan (Issue #3)

Branch: `rewrite-pdfa`. Ziel: alle PDF/A-Stufen (1a/1b, 2a/2b/2u, 3a/3b/3u,
4, 4e, 4f) in Rust erzeugen, ohne Ghostscript, geprüft mit veraPDF.
Der neue Code ist so geschnitten, dass Issue #2 („Ghostscript ablösen“)
darauf aufbauen kann.

## 1. Ist-Zustand

| Stelle | Was passiert |
|---|---|
| `src-tauri/src/lib.rs` `save_as_pdfa` | Holt die Dokument-Bytes aus PDFium, schreibt sie in eine Temp-Datei und ruft `gswin64c -sDEVICE=pdfwrite -dPDFA=<n> -dPDFACompatibilityPolicy=1 -sColorConversionStrategy=RGB -sProcessColorModel=DeviceRGB` auf. |
| `lib.rs` `convert_files_to_pdfa` | Dasselbe als Stapel für mehrere Dateien (Konvertieren-Ansicht). |
| `lib.rs` `pdfa_flags` | Erlaubte Stufen: `1b`, `2b`, `3b`, `4`. |
| `src-tauri/src/vprinter.rs` | Virtueller Drucker: PostScript → PDF per Ghostscript; mit gesetzter Standardstufe gleich PDF/A. |
| Einstellungen | `default_pdfa_level` (`off`/`1b`/`2b`/`3b`/`4`) in `settings.json`. Gilt für „Speichern unter“, den Drucker und die Vorauswahl im Konvertieren-Dialog. |
| UI | `template.ts` (Auswahl in Einstellungen › PDF/A-Export und im Konvertieren-Bereich), `chrome.ts` (`runSaveAsPdfa`), `doc.ts` (`saveAs` mit PDF/A, Hinweis „Formular wird abgeflacht“), `edit.ts` (Stapelkonvertierung), `i18n.ts`. |

Befunde (Ghostscript 10.07.0, veraPDF 1.30.4):

- **PDF/A-4 geht nicht.** Ghostscript kennt nur `-dPDFA=1|2|3`; `-dPDFA=4`
  bricht mit `Unrecoverable error: rangecheck in .putdeviceprops` ab. Die UI
  bietet die Stufe trotzdem an.
- **Nur b-Stufen.** Ghostscript erzeugt keine Strukturbäume (a) und prüft keine
  Unicode-Abbildung (u). Daher „Maximum ist 3b“.
- **Auch b ist nicht immer konform.** Eine einfache Testdatei mit
  `DeviceCMYK`-Füllung und `ca 0.5` fällt bei veraPDF durch (1b: 6.2.3.3-1,
  2b: 6.2.4.3-2/-4), obwohl Ghostscript Exit-Code 0 liefert.
- Formulare werden immer abgeflacht, Anhänge gehen verloren, ohne
  installiertes Ghostscript gibt es gar kein PDF/A.

## 2. Anforderungen je Stufe

Grundlage sind die veraPDF-Profile (`PDFA-1B.xml` … `PDFA-4F.xml`, 109–155
Regeln je Profil). Kurzfassung der Regeln, die eine Konvertierung aktiv
herstellen muss:

| Thema | Teil 1 (1a/1b) | Teil 2 (2a/2b/2u) | Teil 3 (3a/3b/3u) | Teil 4 (4/4e/4f) |
|---|---|---|---|---|
| Header / Version | `%PDF-1.4`, keine Xref-/Objekt-Streams | `%PDF-1.7` | `%PDF-1.7` | `%PDF-2.0` |
| Trailer | `/ID`, kein `/Encrypt` | dito | dito | dito, **kein `/Info`** (außer bei `PieceInfo`, dann nur `ModDate`) |
| Dateisyntax | Binärkommentar, EOL-Regeln, keine Daten nach `%%EOF`, `stream`/`endstream`-EOLs, Längen korrekt | dito | dito | dito |
| Grenzen | Real ≤ 32767, Name ≤ 127 B, String ≤ 65535 B, Array ≤ 8191, Dict ≤ 4095, q-Tiefe ≤ 28, DeviceN ≤ 8 | Real ±3.4e38, String ≤ 32767 B, DeviceN ≤ 32 | wie 2 | wie 2 |
| Filter | kein LZW, keine externen Streams (`F`/`FFilter`/`FDecodeParms`) | dito, kein Crypt | dito | dito |
| Verschlüsselung | verboten | verboten | verboten | verboten |
| Farbe | OutputIntent `GTS_PDFA1` mit ICC-Profil; Device-Farben nur passend zum OutputIntent | dito, zusätzlich `DefaultRGB/CMYK/Gray` erlaubt; OPM-Regel für ICCBased CMYK | wie 2 | wie 2 |
| Transparenz | **verboten** (SMask, `ca/CA≠1`, BM≠Normal, Transparenzgruppen) | erlaubt (Standard-Blendmodi) | erlaubt | erlaubt |
| Bilder | kein `Alternates`/`OPI`, `Interpolate false`, BPC 1/2/4/8, **kein JPX** | JPX mit Einschränkungen, BPC bis 16 | wie 2 | wie 2 |
| XObjects | keine PostScript-/Referenz-XObjects | dito | dito | dito |
| ExtGState | kein `TR`, `TR2` nur `Default` | dito, kein `HTP`, Halftone 1/5 | wie 2 | wie 2, kein `HTO` |
| Fonts | alle eingebettet (außer Modus 3), alle benutzten Glyphen vorhanden, Breiten Dict = Fontprogramm (±1), TrueType-Encoding-Regeln, Subsets mit `CharSet`/`CIDSet` | dito, `CharSet`/`CIDSet` optional aber korrekt, kein `.notdef`-Bezug, OpenType erlaubt | wie 2 | wie 2; `ToUnicode` falls vorhanden gültig, `ActualText` ohne PUA |
| Unicode (u) | – | 2u: jede benutzte Glyphe auf Unicode abbildbar, Werte ≠ 0/FEFF/FFFE | 3u: dito | – |
| Struktur (a) | 1a: `MarkInfo/Marked true`, `StructTreeRoot`, RoleMap auf Standardtypen, `ToUnicode` | 2a: dito + Unicode wie 2u + PUA nur mit `ActualText`, `Lang` gültig | 3a: wie 2a | – |
| Annotationen | erlaubte Typen (kein FileAttachment/Sound/Movie), `F` mit Print=1 und Hidden/Invisible/NoView=0, `CA`=1, `AP` nur `/N` | dito, **jede Annotation braucht `AP`** (außer Popup/Link/Nullgröße), kein 3D/Screen | wie 2 | wie 2, kein FileAttachment (außer 4f), kein RichMedia/3D (außer 4e) |
| Formulare | `NeedAppearances` nicht true, jedes Feld mit Erscheinungsbild, keine Aktionen an Feldern | dito, kein `XFA`, keine `A`/`AA` an Widgets/Feldern | wie 2 | wie 2 |
| Aktionen / JS | kein JavaScript, Launch, Sound, Movie, ResetForm, ImportData; Named nur Next/Prev/First/LastPage; kein `AA` im Katalog | dito + kein Hide, SetOCGState, Rendition, Trans, GoTo3DView; kein `AA` an Seiten | wie 2 | wie 2; `AA` nur E/X/D/U/Fo/Bl |
| Metadaten | XMP im Katalog, **unkomprimiert**, `pdfaid:part/conformance`, Info ↔ XMP identisch, Erweiterungsschemata für Fremd-Properties, kein `bytes`/`encoding` im xpacket | dito | dito | `pdfaid:part=4`, `pdfaid:rev=2020`, `conformance` nur `E`/`F` |
| Anhänge | **verboten** | nur PDF/A-1/-2-konforme PDFs (veraPDF prüft rekursiv) | beliebig, mit `F`+`UF`, `AFRelationship`, MIME-`Subtype`, über `/AF` verknüpft | 4: nur PDF/A-1/2/4; 4f: beliebig, mindestens einer Pflicht; 4e: wie 4 |
| Optional Content | **verboten** (`OCProperties`) | erlaubt: jede Konfiguration mit eindeutigem `Name`, `Order` vollständig, kein `AS` | wie 2 | wie 2 |
| Sonstiges | – | kein `NeedsRendering`, keine `AlternatePresentations`, `PresSteps`, `Requirements` | wie 2 | wie 2; 4e: 3D nur U3D/PRC |

## 3. Architektur

Neuer Crate **`crates/pdf_pdfa`** (reines Rust, keine UI-/OS-Abhängigkeit):

```
pdf_pdfa
├── level.rs        Stufe (Part 1–4 × a/b/u/e/f), Parser für "2b", "4f" …, Fähigkeiten je Stufe
├── convert.rs      Einstieg: convert(bytes, Options) -> Result<Output{bytes, report}>
├── load.rs         Laden mit lopdf, Entschlüsseln (leeres Passwort / übergebenes Passwort)
├── cos.rs          Syntax/Grenzen: LZW→Flate, externe Streams, Hex/Namen/Zahlen, q-Tiefe
├── color.rs        sRGB-ICC (selbst erzeugt, v2), CMYK-ICC (kompakt, LUT), OutputIntent,
│                   DefaultCMYK in allen Ressourcen, ExtGState-Bereinigung (TR/HT/OPM/BM)
├── transparency.rs Erkennung je Seite; Teil 1: Seite über Rasterizer-Hook flachrechnen
├── fonts/          Analyse benutzter Codes je Font, Einbetten fehlender Fonts (System-
│                   fonts, Standard-14-Ersatz Arial/Times/Courier), TrueType-Subset,
│                   Breiten-Abgleich, Glyph-Prüfung, ToUnicode-Erzeugung, CIDSet/CharSet
├── annots.rs       Typen, Flags, AP nur /N, fehlende Appearances erzeugen, CA/C/IC
├── forms.rs        NeedAppearances, XFA, Feld-Aktionen, Widget-Appearances
├── actions.rs      JavaScript-/Launch-/…-Aktionen und AA entfernen, Named-Actions filtern
├── files.rs        Anhänge je Stufe (entfernen / rekursiv nach PDF/A konvertieren /
│                   AF + AFRelationship + MIME + Params)
├── oc.rs           Optional Content je Stufe
├── tagging.rs      a-Stufen: vorhandene Struktur reparieren (RoleMap, MarkInfo, Lang)
│                   oder automatisch taggen (Text→P, Bild→Figure mit Alt, Rest→Artifact)
├── xmp.rs          XMP neu schreiben (pdfaid, dc, xmp, pdf, xmpMM), Info abgleichen/entfernen
└── write.rs        Serialisierung mit PDF/A-konformer Syntax (Header, Binärkommentar,
                    klassische Xref für Teil 1, Objekt-Streams ab Teil 2, ID)
```

Schnittstelle zum Rest der App:

- `pdf_pdfa::convert(&[u8], &Options) -> Result<Converted, PdfaError>`;
  `Converted` enthält Bytes und einen Bericht (was geändert/entfernt wurde).
- Hook-Trait `Rasterizer` (Seite → RGB-Bitmap + Textwörter). Die App
  implementiert ihn mit PDFium (`pdf_pdfium`), Tests ebenfalls. Gebraucht nur
  für Fälle ohne verlustfreien Weg: Transparenz in PDF/A-1, nicht reparierbare
  Fonts als letzte Rückfallebene, JPX in PDF/A-1. Ohne Hook liefern diese
  Fälle einen klaren Fehler statt einer nicht konformen Datei.
- `PdfaError` mit unterscheidbaren Fällen (`PasswordRequired`, `Unsupported{reason}`,
  `FontUnavailable{name}` …), die die UI DE/EN anzeigt.

Wiederverwendung aus dem Workspace:

| Crate | Nutzen |
|---|---|
| `lopdf` 0.38 | Parsen, Entschlüsseln (`decrypt`), Content-Streams (`Content::decode/encode`), Schreiben inkl. Objekt-Streams |
| `ttf-parser` 0.25 | TrueType/OpenType lesen: cmap, hmtx, Glyph-Existenz, Namen |
| `flate2` | Flate neu kodieren (LZW, Metadaten, erzeugte Streams) |
| `image` | Pixel für CMYK-/JPX-Fallback und Flachrechnen (PNG/JPEG) |
| `pdf_pdfium` | Rasterizer-Hook (Rendern, Text mit Positionen), JPX-Dekodierung |
| `pdf_mutate::truetype` | Vorbild für Type0/Identity-H-Einbettung mit ToUnicode (unsichtbarer Textlayer beim Flachrechnen) |
| `pdf_fonts` | Gebündelte Fonts als letzter Ersatz (unsichtbarer Text) |

Keine neue Ghostscript-Abhängigkeit im neuen Pfad. Neue externe Crates nur,
wenn nötig (Kandidat: keiner; Font-Subsetting wird für glyf-TrueType selbst
gebaut, CFF wird vollständig eingebettet).

## 4. Umsetzung je Anforderung

- **Farbe:** Immer sRGB-OutputIntent (wie bisher mit Ghostscript
  `ColorConversionStrategy=RGB`). `DeviceCMYK` bleibt im Inhalt; jede
  Ressourcen-Dictionary bekommt `DefaultCMYK` → ICCBased mit einem kompakten,
  selbst erzeugten CMYK-Profil. Kein Umrechnen von Content-Streams oder Bildern.
  Ist `DefaultCMYK` in PDF/A-1 nicht zulässig (wird mit veraPDF geprüft), werden
  CMYK-Operatoren und -Bilder für Teil 1 nach RGB umgerechnet.
- **Fonts:** Content-Streams (Seiten, Form-XObjects, Muster, Type3-Glyphen,
  Annotation-Appearances) werden einmal durchlaufen; pro Font werden die
  benutzten Codes und der Render-Modus gesammelt. Nicht eingebettete Fonts
  werden aus Systemfonts eingebettet (Name → Datei; Standard-14 → Arial,
  Times New Roman, Courier New, Symbol), TrueType mit Subset. Breiten im
  Fontprogramm werden an `/Widths` angepasst (hmtx im Subset), damit sich das
  Layout nicht verschiebt. ToUnicode aus Encoding/Glyphnamen (AGL), cmap oder
  CFF-Charset. Nicht abbildbare Glyphen: PUA + `ActualText` (a) bzw. PUA (u).
- **Transparenz (nur Teil 1):** Seiten mit Transparenz werden über den
  Rasterizer flachgerechnet (300 dpi, Bild + unsichtbarer Textlayer, damit Text
  durchsuchbar bleibt). Annotationen mit Transparenz werden dabei mit
  eingerechnet und entfernt.
- **Annotationen:** Flags korrigieren, `AP` auf `/N` reduzieren, fehlende
  Appearances für die gängigen Typen selbst erzeugen (Text-Notiz, Square,
  Circle, Line, Polygon/PolyLine, Ink, Highlight/Underline/StrikeOut/Squiggly,
  FreeText), versteckte Annotationen entfernen, unzulässige Typen entfernen.
- **Formulare bleiben ausfüllbar** (PDF/A erlaubt AcroForm): `NeedAppearances`
  entfernen, fehlende Widget-Appearances erzeugen, `XFA` und Feld-Aktionen
  entfernen. Damit entfällt das bisherige Abflachen.
- **Anhänge:** Teil 1 entfernen; Teil 2 und 4: eingebettete PDFs rekursiv nach
  PDF/A-2b bzw. -4 konvertieren, andere entfernen; Teil 3 und 4f: behalten und
  ergänzen (`AFRelationship`, MIME, `UF`, `Params/ModDate`, `/AF`). 4f ohne
  Anhang → klarer Fehler.
- **Signaturen:** Jede Umwandlung ändert die Bytes, Signaturen werden ungültig.
  Signaturwerte werden entfernt (Bericht weist darauf hin).
- **Verschlüsselte Eingabe:** mit leerem Benutzerpasswort entschlüsseln; sonst
  Fehler `PasswordRequired` (keine nicht konforme Ausgabe).
- **a-Stufen:** vorhandene Struktur übernehmen und reparieren; ohne Struktur
  automatisch taggen (Marked Content um Textobjekte/Bilder, Rest als Artifact,
  `Lang` aus Einstellung bzw. `de-DE`/`en-US` je UI-Sprache).
- **Teil 4:** PDF 2.0-Header, `pdfaid:rev`, kein Info-Dictionary, 4e behält
  3D/RichMedia nur bei U3D/PRC, 4f verlangt Anhang.

## 5. Tests, Abdeckung, Leistung

- **veraPDF** als Orakel: `scripts/fetch-verapdf.ps1` legt veraPDF 1.30.4 und
  eine portable Java-Laufzeit in `vendor/verapdf/` ab (gitignored). Tests
  finden es dort oder über `VERAPDF`. Fehlt es, werden die veraPDF-Tests
  übersprungen und melden das deutlich.
- **Synthetischer Korpus** (in Rust erzeugt, keine Kundendateien):
  nicht eingebettete Fonts (Standard-14, TrueType), Type3-Fonts, Transparenz,
  Device-Farben (RGB/CMYK/Gray), JPX, Annotationen inkl. FreeText/Stempel/
  Formen aus `pdf_mutate`, Formulare, Anhänge, verschlüsselte Eingabe (mit und
  ohne Passwort), JavaScript/Aktionen, gedrehte Seiten, OCR-Textlayer,
  optionaler Inhalt, LZW, Signatur.
- **Matrix-Test:** jede Korpusdatei × jede Stufe → veraPDF muss PASS melden
  (oder die Konvertierung liefert den erwarteten klaren Fehler).
- **Abdeckung:** `cargo llvm-cov -p pdf_pdfa` (Ziel: jede öffentliche Funktion
  und jede Stufe getestet; Zahlen im PR).
- **Benchmark:** `cargo run --release -p pdf_pdfa --example pdfa_bench --
  <dir>` vergleicht neuen Pfad und Ghostscript (Zeit, Größe, veraPDF-Ergebnis)
  auf dem Korpus und lokal auf `bsp-plaene` (nur lokal, nichts committen).

Stand (Messung auf dem Entwicklungsrechner, Windows 11, Release-Build):

- **veraPDF-Matrix** (`just pdfa-check`): 29 synthetische Dateien × 11 Stufen,
  282 × PASS, 0 × FAIL, 37 erwartete Ablehnungen (Datei mit Benutzerkennwort,
  4f ohne Anhang).
- **Echte Dokumente:** die 17 PDFs, die Ghostscript 10.07.1 mitbringt
  (`doc/`, `examples/`; Handbuch mit 8000 Objekten, Illustrator-Grafiken,
  DeviceN, Anmerkungen), bestehen in allen zehn Stufen außer 4f (dort fehlt
  ihnen der Anhang) die Prüfung: 170 von 170.
- **Abdeckung** (`just pdfa-coverage`, Unit-Tests plus Matrix): 100 % der
  Funktionen (1092 von 1092, Closures und Testcode mitgezählt), 93,1 % der
  Zeilen. Nichts ist von der Messung ausgenommen außer den Beispielprogrammen
  und dem Test-Korpus. Für die zuletzt fehlenden Fehlerpfade gibt es eigene
  Tests: ein Schreibziel, das jeden Schreibversuch ablehnt; die
  Schriftverzeichnisse mit vorgegebener Umgebung (ohne `WINDIR`); eine
  Testschrift, deren Glyphen Namen außerhalb der Adobe Glyph List und von
  MacRoman tragen; ein CID-kodiertes CFF-Programm.
- **Benchmark** (`just pdfa-bench`, Median aus 5 Läufen, Ghostscript 10.07.1
  mit dem Aufruf der App):

  | Dateien | Stufe | intern | Ghostscript | Faktor | Größe intern | Größe Ghostscript | veraPDF intern | veraPDF Ghostscript |
  |---|---|---|---|---|---|---|---|---|
  | Korpus (28) | 1b | 1,44 s | 13,95 s | 9,7× | 3714 KiB | 1320 KiB | 28/28 | 0/28 |
  | Korpus (28) | 2b | 0,37 s | 5,07 s | 13,8× | 1570 KiB | 241 KiB | 28/28 | 0/28 |
  | Korpus (28) | 3b | 0,37 s | 5,07 s | 13,9× | 1558 KiB | 241 KiB | 28/28 | 0/28 |
  | Ghostscript-Dokumente (17) | 1b | 0,84 s | 13,38 s | 15,9× | 19,2 MiB | 6,2 MiB | 17/17 | 0/17 |
  | Ghostscript-Dokumente (17) | 2b | 0,43 s | 7,83 s | 18,1× | 17,8 MiB | 4,7 MiB | 17/17 | 0/17 |
  | Ghostscript-Dokumente (17) | 3b | 0,43 s | 7,82 s | 18,2× | 17,8 MiB | 4,7 MiB | 17/17 | 0/17 |

  Die Ausgaben des internen Konverters sind größer: Er lässt Bilder und
  Inhalte, wie sie sind (17,8 MiB bei 17,9 MiB Eingabe), Ghostscript
  komprimiert Bilder neu und verwirft private Daten. Bei den kleinen
  Korpusdateien zählen die eingebetteten Schriften (TrueType-Subsets mit
  Hinting) und die als Bild gespeicherten Seiten. Die 29. Korpusdatei
  (Benutzerkennwort) lehnt der interne Konverter ab; Ghostscript liefert für
  sie ohne Fehlermeldung ein leeres Dokument.

## 6. App-Integration

- Einstellung `pdfa_engine`: `builtin` (neu, Standard sobald Parität erreicht)
  oder `ghostscript` (bisheriger Weg, solange er gebraucht wird).
- Stufenliste in Einstellungen und Konvertieren-Dialog: 1a, 1b, 2a, 2b, 2u,
  3a, 3b, 3u, 4, 4e, 4f (DE/EN). Ghostscript-Pfad bietet nur 1b/2b/3b.
- Virtueller Drucker: PostScript → PDF bleibt Ghostscript (Issue #2), die
  PDF/A-Umwandlung danach läuft über den neuen Pfad.

## 7. Meilensteine

1. Analyse + Plan (dieses Dokument), veraPDF-Setup.
2. 1b/2b/3b ohne Ghostscript bestehen veraPDF (Struktur, Farbe, Metadaten,
   Annotationen, Formulare, Aktionen, Anhänge, Fonts einbetten).
3. Transparenz-Flachrechnen für Teil 1, JPX, CMYK.
4. u- und a-Stufen (ToUnicode, Tagging).
5. Teil 4, 4e, 4f.
6. App-Integration, UI DE/EN, Abdeckung, Benchmark, PR.

## 8. Bleibt Ghostscript-abhängig

- Virtueller Drucker (PostScript → PDF) bis Issue #2.
- Optionaler alter PDF/A-Pfad (Einstellung), bis er entfernt wird.
