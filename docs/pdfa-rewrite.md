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
├── images.rs       Fotos als JPEG mit gemessener Qualität, übrige Bilder verlustfrei
│                   enger packen (siehe „Größe“ in Abschnitt 5)
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
- **Zahlengrenzen (nur Teil 1):** PDF/A-1 erlaubt reelle Zahlen nur bis
  ±32767. Große Zeichnungen in feiner Einheit liegen darüber: ein A0-Plan, den
  Ghostscript in 1/720 Zoll schreibt, hat Koordinaten bis 33 700. Solche
  Operanden in Seiteninhalten, Form-XObjects und Mustern werden zur nächsten
  ganzen Zahl (ganze Zahlen dürfen bis 2³¹ − 1 gehen). Die Verschiebung ist
  höchstens eine halbe Einheit des jeweiligen Koordinatensystems; weil dieses
  System verkleinert werden muss, damit es auf eine Seite passt, sind das auf
  dem Papier unter 0,22 pt, beim A0-Plan 0,05 pt (0,02 mm). Nur die
  betroffenen Operationen werden neu geschrieben, alles andere bleibt Byte
  für Byte. Ab Teil 2 gibt es die Grenze nicht, dort ändert sich nichts.

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
  optionaler Inhalt, LZW, Signatur, Plan mit Koordinaten über 32767.
- **Matrix-Test:** jede Korpusdatei × jede Stufe → veraPDF muss PASS melden
  (oder die Konvertierung liefert den erwarteten klaren Fehler).
- **Abdeckung:** `cargo llvm-cov -p pdf_pdfa` (Ziel: jede öffentliche Funktion
  und jede Stufe getestet; Zahlen im PR).
- **Benchmark:** `cargo run --release -p pdf_pdfa --example pdfa_bench --
  <dir>` vergleicht neuen Pfad und Ghostscript (Zeit, Größe, veraPDF-Ergebnis)
  auf dem Korpus und lokal auf `bsp-plaene` (nur lokal, nichts committen).

Stand (Messung auf dem Entwicklungsrechner, Windows 11, Release-Build):

- **veraPDF-Matrix** (`just pdfa-check`): 31 synthetische Dateien × 11 Stufen,
  302 × PASS, 0 × FAIL, 39 erwartete Ablehnungen (Datei mit Benutzerkennwort,
  4f ohne Anhang).
- **Echte Dokumente:** die 17 PDFs, die Ghostscript 10.07.1 mitbringt
  (`doc/`, `examples/`; Handbuch mit 8000 Objekten, Illustrator-Grafiken,
  DeviceN, Anmerkungen), bestehen in allen zehn Stufen außer 4f (dort fehlt
  ihnen der Anhang) die Prüfung: 170 von 170.
- **Abdeckung** (`just pdfa-coverage`, Unit-Tests, Matrix und
  Wiedergabetest): 100 % der Funktionen (1215 von 1215, Closures und
  Testcode mitgezählt), 94,5 % der Zeilen (10 560, davon 576 nicht
  ausgeführt). Von der Messung ausgenommen sind nur die Beispielprogramme
  und der Test-Korpus, wie bisher.
- **Wiedergabetest** (`cargo test -p pdf_pdfa --test fidelity`): Jede
  Korpusdatei und die 17 Ghostscript-Dokumente werden in 1b und 2b dreimal
  konvertiert: ohne Größenreduktion, mit den verlustfreien Schritten und mit
  den Standardeinstellungen (Fotos als JPEG). PDFium muss aus allen drei
  Fassungen denselben Text mit denselben Positionen (Toleranz 0,01 pt)
  lesen. Die verlustfreie Fassung muss jede Seite bei 108 dpi pixelgleich
  rendern; die Standardfassung ebenso, außer in Dokumenten mit neu
  komprimierten Bildern. Dort wird jede geänderte Seite gegen die
  Fassung ohne Größenreduktion gemessen (SSIM der Helligkeit, PSNR über
  RGB) und muss über den Schranken des Konverters bleiben. Ergebnis:
  94 Konvertierungen, 1492 Seiten, 770 729 Textläufe, kein Textunterschied,
  kein Pixelunterschied in der verlustfreien Fassung. Dokumente mit neu
  komprimierten Bildern:

  | Dokument | Stufen | Bilder neu komprimiert | Ersparnis | schlechtestes Bild (SSIM / PSNR) | geänderte Seiten | Seite: SSIM min. | Seite: PSNR min. |
  |---|---|---|---|---|---|---|---|
  | `images` (Korpus) | 1b, 2b | 3 | 112 KiB | 0,989 / 42,5 dB | 1 von 2 | 0,9984 | 53,8 dB |
  | `GS9_Color_Management.pdf` | 1b, 2b | 2 | 2113 KiB | 0,983 / 34,9 dB | 1 von 42 | 1,0000 | 67,0 dB |
  | `Object_Color.pdf` | 1b, 2b | 2 | 2113 KiB | 0,983 / 34,9 dB | 1 von 1 | 0,9999 | 56,8 dB |

  Die Seitenwerte liegen höher als die Bildwerte, weil die Bilder nur einen
  Teil der Seite füllen und verkleinert dargestellt werden. Die Tabelle
  schreibt der Test nach `target/pdfa-fidelity/images.md`.
- **Benchmark** (`just pdfa-bench`, Median aus 5 Läufen, Ghostscript 10.07.1
  mit dem Aufruf der App):

  | Dateien | Stufe | intern | Ghostscript | Faktor | Größe intern vorher | Größe intern, Bilder verlustfrei | Größe intern jetzt | Größe Ghostscript | veraPDF intern | veraPDF Ghostscript |
  |---|---|---|---|---|---|---|---|---|---|---|
  | Korpus (30) | 1b | 1,61 s | 16,05 s | 10,0× | 4316 KiB | 3931 KiB | 3866 KiB | 1681 KiB | 30/30 | 0/30 |
  | Korpus (30) | 2b | 0,38 s | 5,47 s | 14,3× | 1943 KiB | 1446 KiB | 1381 KiB | 280 KiB | 30/30 | 0/30 |
  | Korpus (30) | 3b | 0,38 s | 5,48 s | 14,3× | 1931 KiB | 1434 KiB | 1370 KiB | 280 KiB | 30/30 | 0/30 |
  | Ghostscript-Dokumente (17) | 1b | 1,17 s | 13,40 s | 11,4× | 19,22 MiB | 11,78 MiB | 9,21 MiB | 6,16 MiB | 17/17 | 0/17 |
  | Ghostscript-Dokumente (17) | 2b | 0,75 s | 7,84 s | 10,4× | 17,83 MiB | 10,40 MiB | 7,83 MiB | 4,69 MiB | 17/17 | 0/17 |
  | Ghostscript-Dokumente (17) | 3b | 0,75 s | 7,84 s | 10,5× | 17,83 MiB | 10,40 MiB | 7,83 MiB | 4,69 MiB | 17/17 | 0/17 |

  „Vorher“ ist die Ausgabe ohne jede Größenreduktion, „Bilder verlustfrei“
  die mit der Einstellung „Bilder verlustfrei lassen“, „jetzt“ der Standard.
  Die 31. Korpusdatei (Benutzerkennwort) lehnt der interne Konverter ab;
  Ghostscript liefert für sie ohne Fehlermeldung ein leeres Dokument.
- **Pläne (lokal):** fünf Baupläne des Owners, 28,7 MiB, nicht im Repository
  (`just pdfa-bench <Ordner>`, Median aus 3 Läufen):

  | Dateien | Stufe | intern | intern, Bilder verlustfrei | Ghostscript | Größe intern vorher | Größe intern, Bilder verlustfrei | Größe intern jetzt | Größe Ghostscript | veraPDF intern | veraPDF Ghostscript |
  |---|---|---|---|---|---|---|---|---|---|---|
  | Pläne (5) | 1b | 65,5 s | 53,6 s | 60,1 s | 28,62 MiB | 15,54 MiB | 15,54 MiB | 27,56 MiB | 5/5 | 0/5 |
  | Pläne (5) | 2b | 65,6 s | 53,5 s | 60,1 s | 28,59 MiB | 15,51 MiB | 15,51 MiB | 27,54 MiB | 5/5 | 0/5 |
  | Pläne (5) | 3b | 65,5 s | wie 2b | 60,0 s | 28,59 MiB | 15,51 MiB | 15,51 MiB | 27,54 MiB | 5/5 | 0/5 |

  - Drei der fünf Pläne bestehen aus einem einzigen seitengroßen Bild
    (33 704 × 23 839 und 42 520 × 23 839 Pixel, 2,4 bis 3,0 GB Rohdaten). Das
    verlustfreie Neupacken halbiert sie fast (27,36 → 15,02 MiB) und kostet
    je Plan 16 bis 21 s; mit Bildkompression (Standard) kommen weitere 4 bis
    5 s dazu, weil jedes Bild erst untersucht wird. Ghostscript braucht 18
    bis 23 s und spart nichts. Auf diesen Plänen ist der interne Konverter
    deshalb nicht schneller als Ghostscript; die
    beiden Vektorpläne brauchen 0,02 bis 0,05 s gegen 0,24 bis 0,37 s.
    („Bilder verlustfrei“: ein Lauf statt Median.)
  - Arbeitsspeicher: eine solche Konvertierung braucht in der Spitze 4,6 bis
    5,7 GiB.
  - Kein Bild wurde verlustbehaftet neu kodiert: die großen Bilder gelten als
    Zeichnung, alle übrigen sind schon JPEG und bleiben Byte für Byte. Die
    dekodierten Bilddaten von Quelle und Ausgabe sind gleich (SHA-256 über
    alle Samples), und jede geprüfte Seite rendert in PDFium pixelgleich zur
    Quelle (SSIM 1, lange Seite 6000 Pixel; sieben Seiten, 1b und 2b, mit und
    ohne Bildkompression). Ausnahme ist der Plan im nächsten Punkt in 1b.
  - Einzige messbare Änderung: Ein Vektorplan hat Koordinaten über 32767,
    die in PDF/A-1 gerundet werden (siehe „Zahlengrenzen“). Gegen die Quelle
    gemessen bei 12 000 Pixeln Breite: SSIM 0,99999, PSNR 60,0 dB, 0,018 % der
    Pixel verschieden. Ab Teil 2 ist auch dieser Plan pixelgleich.

### Größe: was getan ist und was bleibt

Verlustfrei spart der Konverter bei den 17 Ghostscript-Dokumenten (2b)
7,43 MiB: private Anwendungsdaten (`PieceInfo`), in eingebetteten Fonts
nicht benutzte Umrisse und Tabellen, die kein Viewer liest, und Bilder, die
mit PNG-Prädiktor enger gepackt werden. Fotos als JPEG sparen weitere
2,57 MiB.

**Bilder (`images.rs`).** Angefasst werden nur Bilder mit 8 Bit je Kanal,
die verlustfrei gespeichert sind (Flate, LZW, RunLength, unkomprimiert).

- *Fotos werden JPEG.* Als Foto gilt ein Bild, in dem weniger als die
  Hälfte der Pixel dem rechten und dem unteren Nachbarn gleicht und das mehr
  als 256 Farben (bei einem Kanal: 64 Graustufen) hat. Nicht angefasst
  werden: Strichgrafik, Screenshots und textartige Bilder (flächig), Bilder
  unter 32 Pixel Kantenlänge oder 8 KiB, Bilder mit Maske (`SMask`, `Mask`)
  und Masken selbst, indizierte Bilder, Lab, Separation und DeviceN,
  1-Bit-Bilder, Bilder mit eigenem `Decode`, und alles, was schon JPEG,
  JPEG 2000, CCITT oder JBIG2 ist (eine zweite verlustbehaftete Runde kostet
  Qualität).
- *Qualität.* Baseline-JPEG, ein Scan, Standard-Huffman-Tabellen, **keine
  Farbunterabtastung** (4:4:4). Probiert werden die Qualitäten 80, 85, 90,
  95 (IJG-Skala); jedes Ergebnis wird dekodiert und mit dem Original
  verglichen. Genommen wird die erste Stufe mit **SSIM ≥ 0,98** (Helligkeit,
  Mittel über 8×8-Fenster) **und PSNR ≥ 34 dB** (alle Kanäle), deren Stream
  höchstens 80 % des bisherigen groß ist; sonst bleibt das Original. Die
  Pixelmaße ändern sich nie.
- *Vergleich mit Ghostscript* (`QFactor 0.9` entspricht etwa Qualität 55,
  dazu 4:2:0). Die beiden CMYK-Fotos aus `Object_Color.pdf`, von Ghostscript
  mit `-sColorConversionStrategy=LeaveColorUnchanged` kodiert und mit
  demselben Verfahren gemessen: SSIM 0,946 / PSNR 30,7 dB (121 KiB) und
  SSIM 0,894 / PSNR 27,6 dB (92 KiB). Der interne Konverter: 0,983 /
  34,9 dB (197 KiB, Qualität 80) und 0,983 / 36,1 dB (220 KiB,
  Qualität 90).
- *Farbraum.* Bleibt, wie er ist. CMYK wird als CMYK-JPEG geschrieben
  (YCCK mit Adobe-Marker, Werte in der Polarität, die PDF erwartet), nicht
  nach RGB umgerechnet; für die Konformität ist keine Umrechnung nötig.
  Ghostscript rechnet mit dem Aufruf der App alles nach RGB um.
- *Alles andere wird verlustfrei enger gepackt* (Teil von `reduce_size`):
  Flate mit PNG-Prädiktor, Filter je Zeile gewählt; übernommen, wenn der
  Stream mindestens 5 % kleiner wird (bei LZW und unkomprimierten Streams,
  sobald er kleiner ist).
- *Abschalten:* Einstellungen › PDF/A-Export › „Bilder verlustfrei lassen“
  (`Options::recompress_images = false`).
- *Nebenbei behoben:* Der TIFF-Prädiktor und PNG-Prädiktoren mit weniger
  als 8 Bit je Komponente wurden nicht zurückgerechnet (lopdf kann beides
  nicht). Ein LZW-Bild mit TIFF-Prädiktor kam dadurch verfälscht in die
  Ausgabe. `util::stream_data` rechnet Prädiktoren jetzt selbst zurück.

**Was vom Abstand zu Ghostscript bleibt** (2b: 8016 KiB gegen 4802 KiB,
Differenz 3214 KiB), nach Stream-Art:

| Anteil | intern | Ghostscript | Differenz |
|---|---|---|---|
| Bilder, verlustfrei | 2715 KiB | 575 KiB | +2140 KiB |
| Bilder, JPEG | 1891 KiB | 1231 KiB | +660 KiB |
| Font-Programme | 988 KiB | 444 KiB | +544 KiB |
| ICC-Profile | 186 KiB | 0 | +186 KiB |
| Objekte, Xref, XMP | 422 KiB | 314 KiB | +108 KiB |
| Seiteninhalte und Übriges | 1814 KiB | 2238 KiB | −424 KiB |

Nach Ursache (gerundet):

- **rund 1,75 MiB: Screenshots im Handbuch** (`Ghostscript.pdf`: neun
  Bilder mit Text und Diagrammen, 69–95 % der Pixel flächig; zwei enthalten
  eingebettete Fotos). Verlustfrei rund 2,47 MiB, bei Ghostscript als JPEG
  0,72 MiB (dort gemessen: SSIM 0,986–0,988, PSNR 34–35 dB). Sie bleiben
  verlustfrei, weil JPEG an Schriftkanten sichtbar wird. Mit den
  Foto-Einstellungen oben würden die vier größten geschätzt 1,4 MiB sparen;
  dafür müsste die Schwelle `FLAT_SHARE` fallen.
- **rund 0,62 MiB: höhere Qualität der vier CMYK-Fotos** (834 KiB gegen
  195 KiB bei Ghostscript, das nach RGB umrechnet und mit Qualität 55 und
  4:2:0 kodiert).
- **rund 0,59 MiB: vorhandene JPEGs bleiben unangetastet.** Drei
  CMYK-JPEGs zu je 232 KiB; Ghostscript rechnet sie nach RGB um und kodiert
  sie ein zweites Mal (31 KiB).
- **rund 0,41 MiB: Type-1-Programme** (34 Programme, 556 KiB; Ghostscript
  schreibt sie als CFF neu, 132 KiB). Sie sind schon Subsets. Rund 230 KiB
  sind Glyphen- und Unterprogramme, eexec-verschlüsselt und damit nicht
  komprimierbar; geschätzt 250 KiB sind die Einträge leerer Unterprogramme,
  die der TeX-Subsetter als Platzhalter stehen lässt (etwa 800 je
  Latin-Modern- und TeX-Gyre-Font). Sie loszuwerden heißt, die
  Unterprogramme neu zu nummerieren, auch dort, wo die Nummer über die
  Hint-Ersetzung weitergereicht wird, also Glyphen-Programme umzuschreiben;
  der Weg von Ghostscript (Umwandlung nach CFF) schreibt zusätzlich alle
  Hints neu. Für beides fehlt ein unabhängiger Nachweis, dass jede Glyphe
  gleich bleibt (`ttf-parser` liest kein Type 1), deshalb ist es nicht
  umgesetzt.
- **0,18 MiB: ICC-Profile**, die Ghostscript nicht schreibt (deshalb besteht
  keine seiner Ausgaben veraPDF).

Dagegen stehen 0,41 MiB, die Ghostscript bei den Seiteninhalten mehr
schreibt.

## 6. App-Integration

- Einstellung `pdfa_engine`: `builtin` (neu, Standard sobald Parität erreicht)
  oder `ghostscript` (bisheriger Weg, solange er gebraucht wird).
- Stufenliste in Einstellungen und Konvertieren-Dialog: 1a, 1b, 2a, 2b, 2u,
  3a, 3b, 3u, 4, 4e, 4f (DE/EN). Ghostscript-Pfad bietet nur 1b/2b/3b.
- Einstellung `pdfa_images_lossless` („Bilder verlustfrei lassen“, DE/EN,
  Standard: aus): schaltet die JPEG-Kompression von Fotos im integrierten
  Konverter ab. Gilt für „Speichern als PDF/A“, „Speichern unter“, die
  Stapelkonvertierung und den Drucker.
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
