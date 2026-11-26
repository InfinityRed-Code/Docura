# 3D-PDFs öffnen und erstellen — Analyse und Umfangsvorschlag (Issue #1)

Stand: 2026-11-23, Basis `main` v1.3.2. Phase 1 (Analyse + Spike). Der Umfang
für Version 1 wird vom Owner entschieden, bevor das Feature gebaut wird.

## 1. Kurzfassung

- **Heute** stürzt Docura bei 3D-PDFs nicht ab, zeigt aber nur das statische
  Vorschaubild (Poster) der 3D-Annotation — und wenn die Datei keins hat
  (häufig bei LaTeX-/Tool-Exporten), eine **leere weiße Fläche**. Keine
  Interaktion, kein Hinweis auf 3D-Inhalt. Erstellen ist nicht möglich.
- **U3D** (ECMA-363, Spezifikation frei) ist machbar: Im Spike dekodiert ein
  neuer Rust-Crate `pdf_3d` U3D-Daten aus PDFs (inkl. arithmetischer
  Kompression), und three.js zeigt sie an (Screenshots s. Abschnitt 7).
  Der aufwendigste Teil, die *progressiven* Meshes, ist noch offen, aber
  durch Intels Referenzimplementierung (Apache-2.0) gut abgesichert.
- **PRC** (ISO 14739-1, Spezifikation kostenpflichtig) ist deutlich teurer
  und riskanter, vor allem Dateien mit exakter B-Rep-Geometrie ohne
  Tessellierung. Tekla Structures und Allplan — für die Bau-Zielgruppe
  relevant — exportieren 3D-PDF als **U3D**.
- **Erstellen**: Meshes (glTF/GLB, OBJ, STL, PLY, 3MF) → U3D → 3D-Annotation
  mit Standardansichten und Poster ist gut machbar; der U3D-Writer steht im
  Spike. Offenes Risiko: Prüfung der erzeugten Dateien in Adobe Reader (auf
  dem Entwicklungsrechner nicht installiert).
- **Empfehlung V1** (Variante A, ca. 16–20 PT): U3D öffnen und interaktiv
  anzeigen (Ansichten, Render-Modi, Beleuchtung, Modellbaum) + 3D-Modell aus
  Mesh-Formaten einfügen. PRC: Poster + klarer Hinweis + Export der Rohdaten.

## 2. Ist-Zustand (verifiziert)

Geprüft mit Intels U3D-Testszenen (`box.pdf`, `TextureLayers.pdf`,
`SimpleShapesAnimation.pdf`, `surface_base.pdf`, `shading2_base.pdf`) und
Varianten mit nachträglich eingesetztem Poster (`/AP /N`). Probe:
`cargo run -p pdf_pdfium --example probe3d -- <out_dir> <file.pdf>...`

| Bereich | Verhalten heute |
|---|---|
| Öffnen | Ohne Fehler. PDFium meldet die Annotation als `ThreeD` (unsupported feature), rendert aber nur deren Appearance-Stream. |
| Darstellung | Poster vorhanden → statisches Bild (90 000/90 000 Pixel gezeichnet). Kein Poster (bzw. leeres `/AP << /N << >> >>` wie in allen Testdateien) → **leere weiße Fläche** (0 Pixel gezeichnet). |
| Interaktion | Keine. Kein Hinweis, dass ein 3D-Modell enthalten ist. |
| Kommentarliste | Listet die Annotation als Typ „3D“ mit Punkt-Icon und z. B. Inhalt „3D surface“ — verwirrend, aber löschbar. |
| Speichern / Seiten-Ops | lopdf übernimmt den `/3D`-Stream unverändert (verifiziert für Speichern). |
| Schwärzen | Ersetzt die Seite durch ein Bild und löscht alle Annotationen; der 3D-Stream wird mit `prune_unreachable` entfernt — korrekt. |
| Drucken / Thumbnails | Zeigen das Poster bzw. nichts. |
| Erstellen | Nicht möglich. |

## 3. Hintergrund

### 3.1 3D in PDF

- **3D-Annotation** (`/Subtype /3D`, seit PDF 1.6, ISO 32000-1/-2 §13.6):
  - `/3DD` — 3D-Stream (`/Type /3D`, `/Subtype /U3D` oder `/PRC`), mit
    `/VA` (Ansichten), `/DV` (Standardansicht), `/Resources`, `/AN`
    (Animation), `/OnInstantiate` (JavaScript).
  - `/3DV` — Standardansicht der Annotation.
  - `/3DA` — Aktivierung: `/A` (`/PO` Seite öffnen, `/PV` Seite sichtbar,
    `/XA` explizit per Klick), `/D` Deaktivierung, `/TB` Werkzeugleiste,
    `/NP` Modellbaum, ab PDF 2.0 `/Style /Embedded|/Windowed`.
  - `/AP /N` — Poster, das gezeigt wird, solange die Annotation nicht aktiv
    ist (und vom Drucken/Viewer ohne 3D benutzt wird).
  - `/3DB` (View-Box), `/3DI` (interaktiv), ab PDF 2.0 `/3DU` (Einheiten),
    3D-Messungen und 3D-Kommentare.
- **3D-Ansicht** (`/Type /3DView`): Kamera als `/C2W` (camera-to-world,
  3×4-Matrix) + `/CO` (Abstand zum Orbit-Zentrum) oder als `/U3DPath` auf
  einen U3D-View-Knoten; `/P` Projektion (perspektivisch `/FOV` oder
  orthographisch `/OS`), `/BG` Hintergrund, `/RM` Render-Modus (Solid,
  Wireframe, Transparent, Illustration, Vertices, … 15 Modi), `/LS`
  Beleuchtungsschema (Artwork, None, White, Day, Night, Hard, Primary, Blue,
  Red, Cube, CAD, Headlamp), `/SA` Schnittebenen (`/3DCrossSection`),
  `/NA` Knotenzustände (Sichtbarkeit, Deckkraft, Matrix).
- **RichMedia** (Adobe Extension Level 3, in PDF 2.0 übernommen): Container
  für Flash (tot), Video und 3D. In PDF 2.0 ist `RichMedia` mit 3D-Inhalt
  der Rahmen für alle 3D-Formate.
- **glTF in PDF**: ISO/TS 32007:2024 (veröffentlicht 1. 4. 2024) erlaubt
  glTF 2.0 als 3D-Format in RichMedia-Annotationen von PDF 2.0. Foxit PDF
  Editor zeigt glTF; für Adobe Acrobat fanden wir **keine** Bestätigung.
  Für maximale Kompatibilität ist glTF daher als Ausgabeformat ungeeignet.
- **PDF/A**: PDF/A-1/-2/-3 verbieten 3D. **PDF/A-4e** (ISO 19005-4,
  Anhang B, Nachfolger von PDF/E) erlaubt 3D- und RichMedia(3D)-Annotationen
  mit U3D oder PRC. Konsequenz: Der PDF/A-Export (Rewrite #3, anderer
  Worker) muss 3D-Annotationen außer bei 4e entfernen oder durch ihr Poster
  ersetzen — hier wird kein PDF/A-Code angefasst, nur abgestimmt.

### 3.2 Formate

| | U3D | PRC |
|---|---|---|
| Norm | ECMA-363 4. Ed. 2007 (frei, ~250 S.) | ISO 14739-1:2014 (kostenpflichtig); Adobe-Spezifikation 8137 (2008) war öffentlich |
| Inhalt | Szenengraph, Dreiecksnetze (CLOD, progressiv), Linien, Punkte, Materialien, Texturen, Lichter, Kameras, Animation | Produktstruktur, exakte B-Rep (NURBS), Tessellierung (auch hochkomprimiert), PMI, Ansichten |
| Kodierung | Blöcke mit adaptiver arithmetischer Kodierung; progressive Mesh-Updates | zlib-komprimierte Bitstreams, sehr umfangreiches Schema |
| Erzeuger | Tekla Structures, Allplan („U3D, ECMA-363 1st edition“), MeshLab, LaTeX (movie15/media9), Intel IDTFConverter, Macad3D | Acrobat/Tetra4D, HOOPS-basierte CAD-Exporte, SolidWorks/NX/CATIA-Workflows, Asymptote |
| Viewer | Acrobat/Reader, Foxit, PDF-XChange | Acrobat/Reader, Foxit, PDF-XChange (eingeschränkt) |

Befund aus den Intel-Testszenen: Der Intel-IDTFConverter (Standardweg für
MeshLab, LaTeX usw.) schreibt Meshes fast immer **progressiv**: der
Base-Mesh-Block fehlt oder ist leer, die Dreiecke stecken in
Progressive-Mesh-Blöcken (45 von 49 Testdateien mit Meshes; nur 4 sind
reine Base-Meshes). Ein Viewer, der nur Base-Meshes kann, zeigt bei
solchen Dateien nichts. Der progressive Decoder ist deshalb Pflicht für V1.

### 3.3 Bibliotheken und Lizenzen

Lizenz von Docura: PolyForm Noncommercial 1.0.0 → nur permissive Lizenzen
(MIT/Apache/BSD/MPL als separate Dateien) direkt nutzbar; LGPL nur als
separat austauschbares Modul; GPL/AGPL nicht.

| Projekt | Zweck | Lizenz | Bewertung |
|---|---|---|---|
| ECMA-363 inkl. C#-Referenzcode (Annex) | U3D-Spezifikation | frei | Grundlage des Decoders |
| Universal 3D Sample Software (Intel, `ningfei/u3d`) | Referenz-Encoder/-Decoder (C++), Testszenen | Apache-2.0 | Vorlage für den progressiven Decoder; Testszenen als Fixtures |
| FEBioStudio `U3DFile.cpp` | U3D-Bitstream + Base-Mesh-Leser (C++) | MIT | Gegencheck Bitstream |
| Macad3D `Macad.Exchange/U3d` | U3D-Writer (C#), in Produktion für 3D-PDF | MIT | Vorlage Writer (Profil „no compression“, Blockreihenfolge „Shader vor Material“ für Acrobat) |
| `mradugin/u3d-to-stl` | U3D-Base-Mesh → STL (C) | Apache-2.0 | nur Base-Mesh |
| three.js (+ GLTF/OBJ/STL/PLY/3MF-Loader, OrbitControls) | WebGL-Rendering, Import | MIT | Viewer + Import; per dynamic import nachladen |
| occt-import-js (OpenCascade als WASM) | STEP/IGES/BREP → Mesh | LGPL-2.1, ~11 MB | optional, separates Modul |
| web-ifc (ThatOpen) | IFC (BIM) → Mesh | MPL-2.0, ~26 MB unpacked | optional, für Bau relevant |
| Asymptote `prc/` (oPRCFile) | PRC-Writer (C++) | LGPL-3 | nur als Referenz lesbar, nicht einbinden |
| nanoPRC (`t3dr0/prc_*_rw`) | PRC ↔ OBJ/GLB | AGPL-3.0 | ungeeignet |
| `jaydenhchen/convert` | U3D/PRC-PDF → STL (JS) | ohne Lizenz | ungeeignet; zeigt, dass Base-Mesh-only bei U3D häufig nicht reicht |
| pdf.js, PDFium | PDF | Apache/BSD | beide ohne 3D-Unterstützung |

Kein fertiger U3D- oder PRC-Parser in Rust oder JS mit passender Lizenz und
vollem Umfang gefunden. Eigenimplementierung in Rust ist der Weg.

## 4. Öffnen — Entwurf

```
PDF ──lopdf──▶ 3D-Annotationen finden, /3DD dekodieren, /VA /3DV /3DA lesen   (pdf_mutate)
      U3D-Bytes ──▶ pdf_3d::u3d::parse ──▶ Szene ──▶ render::flatten            (pdf_3d, neu)
      Tauri-Command (binär) ──▶ UI: three.js-Viewer über dem Annotations-Rechteck (lazy chunk)
```

- **Warum Rust für den Decoder**: Eingabe ist nicht vertrauenswürdige
  Binärdaten → speichersicher, Grenzen gegen Speicher-Bomben (im Spike: max.
  20 Mio. Elemente, Plausibilitätsprüfung gegen Blockgröße), testbar mit
  `cargo test`, UI bleibt dünn (Architekturregel: keine Fachlogik im UI).
- **Anzeige in der Seite** (wie Acrobat): neuer Layer pro Seiten-Slot nach
  dem Muster des Link-Layers (`rotateRectToSlot`). Über jeder 3D-Annotation
  liegt ein Badge „3D“; Klick aktiviert (bzw. `/PO`/`/PV` gemäß `/3DA`) und
  ersetzt das Poster durch ein WebGL-Canvas. Nur ein aktives 3D-Canvas
  gleichzeitig (WebGL-Kontextlimit). Toolbar: Ansichten aus `/VA`, Zurücksetzen,
  Render-Modus, Beleuchtung, Modellbaum (Sichtbarkeit), Vollbild,
  Deaktivieren. Folgt Zoom, Scrollen und Seitendrehung.
- **Ohne Poster**: Docura rendert nach dem Laden selbst ein Vorschaubild in
  die leere Fläche (nur Anzeige, die Datei bleibt unverändert).
- **Mapping**: `/C2W` + `/CO` → Kamera + Orbit-Ziel; `/P` → Perspektive/
  Ortho; `/BG` → Hintergrund; `/RM` → Solid/Wireframe/Transparent/
  Illustration (Kanten) — exotische Modi auf den nächsten abbilden; `/LS` →
  Lichtsets; `/NA` → Sichtbarkeit/Deckkraft; `/SA` → three.js-Clipping-Plane
  (V1 optional, ohne Deckel).
- **U3D-Umfang**: Knoten/Instanzen, CLOD-Meshes (Base ✔ Spike, progressiv),
  Materialien/Shader ✔, Vertex-Farben ✔, Texturen (PNG/JPEG aus
  Continuation-Blöcken), Linien-/Punktmengen, U3D-Kameras und -Lichter.
  Nicht: Animation, Bones, Subdivision, Glyphen, JavaScript (`/OnInstantiate`).
- **PRC** in V1: Poster/Vorschau + Hinweis „PRC-3D-Inhalt wird noch nicht
  interaktiv angezeigt“ + „3D-Daten speichern…“ (Roh-Export `.prc`).
- **RichMedia mit 3D-Asset** (U3D) wird wie eine 3D-Annotation behandelt.

## 5. Erstellen — Entwurf

- **Eingabeformate**

  | Format | Weg | Aufwand | Kommentar |
  |---|---|---|---|
  | glTF/GLB | three.js GLTFLoader (+ Draco optional) | gering | moderner Austausch, Farben/Materialien |
  | OBJ (+MTL), STL, PLY, 3MF | three.js-Loader | gering | STL/3MF typisch aus CAD/3D-Druck |
  | IFC | web-ifc (WASM, MPL-2.0) | mittel | **BIM-Austauschformat der Bau-Zielgruppe**; große WASM-Datei |
  | STEP/IGES | occt-import-js (WASM, LGPL-2.1) | mittel | mechanisches CAD; LGPL-Modul separat ausliefern |
  | DWG/RVT/SKP/NWD | — | nicht machbar | proprietär; Nutzer exportieren IFC/glTF |

- **Ausgabe**: U3D, Profil „no compression“, je Mesh ein vollständiges
  Base-Mesh (keine Progression) — so macht es auch Macad3D, Acrobat liest es.
  Der Stream wird mit FlateDecode komprimiert. Größe grob 25–40 Byte pro
  Dreieck vor Flate; 1 Mio. Dreiecke ≈ 15–25 MB im PDF. Später möglich:
  komprimierter U3D-Writer (arithmetischer Encoder, 1–2 PT) oder PRC-Writer.
- **Annotation**: `/3D` mit `/3DD` (U3D), `/VA` mit Iso + 6 Normalansichten
  (aus der Bounding-Box berechnete `/C2W`), `/DV` Iso, `/LS /CAD`,
  `/RM /Solid`, weißer Hintergrund, `/3DA << /A /XA /D /PI /TB true /NP true >>`
  (Aktivierung per Klick), Poster als Bild-XObject in `/AP /N`.
- **Poster**: three.js rendert die Standardansicht offscreen als PNG.
- **Bedienung**: Bearbeiten → „3D-Modell einfügen“ → Datei wählen → Vorschau-
  Dialog (Ausrichtung Z-oben/Y-oben, Einheit, Farbe) → Rechteck auf der Seite
  aufziehen (wie Stempel). Danach verschieben/skalieren/löschen wie andere
  Annotationen.

## 6. Aufwand und Risiko

PT = Entwicklertage inkl. Tests. Schätzungen nach dem Spike.

| Nr. | Teil | PT | Risiko | Anmerkung |
|---|---|---|---|---|
| O1 | Erkennung, 3D-Badge, Kommentarliste „3D-Modell“, Roh-Export `.u3d/.prc`, Vorschau für posterlose Annotationen | 1–2 | gering | |
| O2 | U3D-Decoder komplett: progressive Meshes (Port der Intel-Referenz), Texturen, Linien/Punkte, Härtung | 4–5 | mittel | Testorakel: Intel liefert jede Szene als progressive *und* Base-Variante (`box.u3d` ↔ `box_base.u3d`) |
| O3 | Viewer in der Seite: three.js-Chunk, Aktivierung, Orbit/Pan/Zoom, Ansichten (`/VA`, `/C2W`, `/P`, `/BG`, `/RM`, `/LS`), Modellbaum, Vollbild, Drehung/Zoom der Seite | 4–5 | mittel | Kamera-Konventionen der PDF-Ansichten genau umsetzen |
| O4 | Schnittebenen (`/SA`), Messen | 2–3 | mittel | optional |
| O5 | PRC lesen (nur tessellierte Geometrie, inkl. komprimierter Tessellierung) | 8–12 | **hoch** | Spezifikation kaufen/beschaffen; B-Rep-only-Dateien bleiben beim Poster |
| O6 | RichMedia/glTF (ISO/TS 32007) lesen | 1–2 | gering | selten |
| C1 | U3D-Writer (Spike ✔) + Annotations-Writer (Ansichten, Aktivierung, Poster) | 2–3 | mittel | **Gegenprüfung in Adobe Reader nötig** |
| C2 | Import glTF/GLB, OBJ, STL, PLY, 3MF | 1–2 | gering | |
| C3 | Einfüge-Dialog mit Vorschau, Platzierung, Poster-Rendering, Bearbeiten/Löschen | 2–3 | gering | |
| C4 | IFC-Import (web-ifc) | 2–3 | mittel | +WASM-Größe im Installer, MPL-2.0-Hinweise |
| C5 | STEP/IGES-Import (occt-import-js) | 2–3 | mittel | +11 MB, LGPL-Modul separat |
| C6 | Kompakter Writer (komprimiertes U3D) / PRC-Writer | 1–2 / 6–8 | gering / hoch | |
| Q | Tests: Fixtures, E2E via CDP-Screenshots, 2D-Regression | 2 | — | |

## 7. Spike (in diesem Branch)

- **Crate `crates/pdf_3d`** (neu, ohne PDF-Abhängigkeit):
  - `u3d::bits` — ECMA-363-Bitstream-Decoder (arithmetische Dekodierung mit
    statischen und dynamischen Kontexten), nach dem Referenzcode im Anhang
    der Norm.
  - `u3d::parse` — Blockstruktur, Modifier-Chains, Gruppen-/Modell-/Licht-/
    View-Knoten, Shading-Modifier, Mesh-Deklaration, **CLOD-Base-Mesh**
    (komprimierte Indizes), Shader, Materialien, Lichter. Progressive Meshes
    und Texturen werden noch als Warnung übersprungen.
  - `u3d::write` — U3D-Writer (no compression, Base-Mesh, Farben je
    Material).
  - `render::flatten` — Szene → Dreiecks-Batches in Weltkoordinaten
    (Instanzen über Mehrfach-Eltern, Normalen, Vertex-Farben).
- **Tests** (`cargo test -p pdf_3d`, grün):
  - Roundtrip Writer → Parser (Namen, Transformationen, Materialien, Opazität).
  - Intel-Testszenen: `box_base` (12 Dreiecke, 8 Ecken), `surface_base`
    (960 Dreiecke), `shading2_base` (2 Shader), `plane64x64_base`
    (7938 Dreiecke) vollständig dekodiert; progressive Dateien parsen ohne
    Fehler.
- **Ende-zu-Ende**: `probe3d` holt den `/3DD`-Stream aus den Intel-PDFs,
  `cargo run -p pdf_3d --example u3d_viewer -- in.u3d out.html` erzeugt eine
  three.js-Seite; per Headless-Edge geprüft: grüne Fläche (`surface_base`)
  und Würfel mit zwei Materialien (`shading2_base`) erscheinen korrekt
  schattiert. Dekodierzeit 4–40 ms (Debug-Build).
- Fixtures: sechs Intel-Testszenen (Apache-2.0) mit Herkunftsnachweis in
  `crates/pdf_3d/tests/fixtures/intel/README.md`. Keine Kundendateien.

## 8. Vorschlag

**A — empfohlen: „U3D anzeigen + aus Mesh erstellen“** — O1 + O2 + O3 + C1 +
C2 + C3 + Q ≈ **16–20 PT**.
Öffnen: U3D (Tekla, Allplan, MeshLab, LaTeX, Intel/IDTF) interaktiv; PRC mit
Poster/Vorschau, Hinweis und Roh-Export. Erstellen: glTF/GLB, OBJ, STL, PLY,
3MF. Issue-Status danach: „Teil von #1“ (PRC fehlt).

**B — schlanker: „nur Öffnen (U3D)“** — O1 + O2 + O3 + Q ≈ **11–14 PT**.
Wie A ohne Erstellen. Sinnvoll, wenn Nutzer 3D-PDFs vor allem empfangen.

**C — „Bau-Paket“: A + IFC-Import (+ optional STEP/IGES)** — ≈ **18–23 PT**
(+2–3 für STEP/IGES). Erstellen direkt aus dem BIM-Modell (IFC). Kostet
Installergröße (WASM ~10–25 MB, nachladbar) und Lizenzhinweise (MPL/LGPL).

**D — „Vollausbau“: C + PRC lesen (tessellierte Geometrie)** — ≈ **30–38 PT**,
hohes Risiko bei PRC; damit „Closes #1“.

Für alle Varianten gilt: Bestehende 2D-Funktionen bleiben unberührt (neuer
Crate, neuer Layer, three.js nur bei Bedarf nachgeladen); PDF/A-Code wird
nicht angefasst.

## 9. Offene Fragen an den Owner

1. Umfang: A, B, C oder D?
2. Aus welchen Programmen stammen die 3D-PDFs der Nutzer (Tekla, Allplan,
   Revit-Plugin, SolidWorks, …)? Gibt es eine Beispieldatei zum lokalen
   Testen (wird nicht eingecheckt)?
3. Darf zur Gegenprüfung der erzeugten Dateien Adobe Acrobat Reader auf dem
   Testrechner installiert werden, oder prüft der Owner selbst?

## Quellen

- ECMA-363 Universal 3D File Format, 4th ed. (2007): <https://ecma-international.org/publications-and-standards/standards/ecma-363/>
- ISO 14739-1:2014 (PRC): <https://www.iso.org/standard/54948.html>; <https://www.pdfa.org/prc-now-published-as-iso-14739-12014/>
- ISO 32000-2 Klausel 13 (Multimedia/3D): <https://pdf-issues.pdfa.org/32000-2-2020/clause13.html>
- glTF in PDF, ISO/TS 32007: <https://pdfa.org/pdf-2-0-adds-gltf-model-support/>, <https://pdfa.org/resource/iso-ts-32007/>
- PDF/A-4e: <https://loc.gov/preservation/digital/formats/fdd/fdd000651>, <https://pdfa.org/resource/3d-formats/>
- Universal 3D Sample Software (Apache-2.0): <https://github.com/ningfei/u3d>
- FEBioStudio U3DFile (MIT): <https://github.com/febiosoftware/FEBioStudio/blob/develop/PostLib/U3DFile.cpp>
- Macad3D U3D-Export (MIT): <https://github.com/Macad3D/Macad3D/tree/main/Source/Macad.Exchange/U3d>
- Tekla 3D-PDF-Export (U3D): <https://support.tekla.com/doc/tekla-structures/2026/int_export_to_3d_pdf>
- Allplan Datenaustauschformate 2026: <https://www.allplan.com/fileadmin/user_upload/countries/international/pdfs/system/Data_exchange_formats_ALLPLAN_2026_EN.pdf>
- Foxit 3D (U3D, PRC, glTF): <https://help.foxit.com/manuals/pdf-editor/mobile/en_us/2025.5.0/Work_on_3D_Content_in_PDF_Files.html>
- PDF-XChange 3D: <https://forum.pdf-xchange.com/viewtopic.php?p=120572>
- three.js (MIT): <https://github.com/mrdoob/three.js>; occt-import-js (LGPL-2.1): <https://github.com/kovacsv/occt-import-js>; web-ifc (MPL-2.0): <https://github.com/ThatOpen/engine_web-ifc>
