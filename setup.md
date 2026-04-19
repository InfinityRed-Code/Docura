# Setup für einen KI-Code-Agenten: Rust PDF Desktop App (Windows-first, Linux-ready)

Dieses Dokument bündelt alle für den Code-Agenten relevanten Vorgaben, die empfohlene Projektstruktur und die Installationen, die **vor** dem eigentlichen Coding erledigt werden sollten. Die Empfehlung basiert auf einer **Windows-first MVP-Strategie mit Tauri 2**, weil Tauri 2 Windows und Linux unterstützt und für Datei-Dialogs, Dateisystemzugriffe und Datei-/Ordner-Öffnen offizielle Plugins bereitstellt.[cite:15][cite:9]

## Zielbild

Die Anwendung soll zuerst unter Windows stabil laufen, aber so aufgebaut werden, dass eine Linux-Version später ohne große Architekturbrüche möglich ist. Dafür muss die PDF-Logik von der UI getrennt werden, und plattformnahe Funktionen dürfen nicht direkt im Core landen.[cite:15][cite:38]

## Architekturentscheidung

**Empfohlener Hauptstack:**

- **Desktop-Shell / UI:** Tauri 2.[cite:15]
- **Anwendungslogik:** Rust Workspace mit getrennten Crates.[cite:15]
- **PDF-Rendering:** `pdfium-render`, das zur Laufzeit gegen eine vorhandene oder gebündelte PDFium-Bibliothek bindet.[cite:29][cite:23]
- **Optionale PDF-Manipulation:** `lopdf` für Struktur- und Dokumentoperationen, nicht als primäre Rendering-Engine.[cite:26]
- **Desktop-Integrationen:** Tauri Dialog Plugin, File System Plugin und Opener Plugin.[cite:31][cite:33][cite:40]

## Was vor dem Coding installiert werden muss

### Auf Windows

Für Tauri-Entwicklung unter Windows werden laut offizieller Doku mindestens **Rust**, die **Microsoft C++ Build Tools** und **Microsoft Edge WebView2** benötigt.[cite:9] Die Tauri-Dokumentation weist außerdem darauf hin, dass WebView2 für das Rendern der Weboberfläche unter Windows benötigt wird; auf vielen Windows-Installationen ist es bereits vorhanden, sollte aber trotzdem geprüft werden.[cite:9][cite:49]

**Installieren:**

1. **Rust via rustup**
   - Offizielle Installation: https://www.rust-lang.org/tools/install [cite:48]
2. **Microsoft Visual Studio C++ Build Tools**
   - Benötigt für native Rust-/Tauri-Builds unter Windows.[cite:9][cite:48]
   - Achte auf diese Komponenten:
     - C++ Build Tools
     - Windows SDK
3. **Microsoft Edge WebView2 Runtime**
   - Für Tauri-Apps unter Windows nötig.[cite:9][cite:49]
4. **Git**
   - Für Repo-Checkout, Submodule und Agent-Workflows.
5. **Node.js LTS**
   - Für das Frontend im Tauri-Fenster und Tauri-CLI-Workflows.
6. **pnpm oder npm**
   - Paketmanager für das Frontend.
7. **Visual Studio Code** oder eine andere IDE
   - Mit Rust Analyzer empfohlen.

### Optional, aber sehr sinnvoll auf Windows

- **LLVM/Clang** nur dann, wenn spätere native Abhängigkeiten das verlangen.
- **ImageMagick** oder ähnliche Tools nur bei zusätzlicher Asset-/Exportverarbeitung.
- **7-Zip** für Binary-Handling und Paketprüfung.

### Für spätere Linux-Builds

Tauri 2 läuft auch auf Linux, benötigt dort aber die passende Linux-WebView-/GTK-Umgebung; in der Praxis ist insbesondere **WebKitGTK 4.1** relevant.[cite:12] Slint dokumentiert Linux-Support für gängige Desktop-Distributionen mit Wayland oder X11, glibc und d-bus, was die generelle Linux-Zielrichtung zusätzlich stützt.[cite:38]

Wenn später auf Linux gebaut werden soll, sollten mindestens diese Pakete eingeplant werden:

- `build-essential`
- `pkg-config`
- `libwebkit2gtk-4.1-dev` oder distributionsäquivalente Pakete.[cite:12]
- `libgtk-3-dev` oder die von Tauri/Distribution geforderten GTK-Entwicklungspakete.
- `libssl-dev`
- `curl`
- `git`
- `nodejs`
- `npm` oder `pnpm`

## PDFium vorbereiten

`pdfium-render` bringt **PDFium nicht selbst mit**, sondern bindet zur Laufzeit an eine vorhandene oder gebündelte PDFium-Bibliothek.[cite:29][cite:23] Das ist für die Architektur gut, weil PDFium pro Zielplattform separat verwaltet werden kann, verlangt aber von Anfang an eine bewusste Bundling-Strategie.[cite:29]

### Empfehlung

Für den MVP sollte PDFium **neben der Anwendung gebündelt** werden. `pdfium-render` dokumentiert, dass eine native Anwendung eine passende PDFium-Bibliothek zur Laufzeit laden kann, wenn sie im selben Verzeichnis wie das gebaute Programm liegt.[cite:29]

### Praktische Konsequenz

- Windows: `pdfium.dll` im App-/Bundle-Kontext einplanen.
- Linux: `libpdfium.so` für das Zielsystem bereitstellen.[cite:16]
- PDFium nie tief in die UI integrieren, sondern über ein `PdfEngine`-Interface kapseln.

## Empfohlene Installationsreihenfolge

1. Rust via `rustup` installieren.[cite:48]
2. Visual Studio C++ Build Tools + Windows SDK installieren.[cite:9][cite:48]
3. WebView2 Runtime prüfen oder installieren.[cite:9][cite:49]
4. Git installieren.
5. Node.js LTS installieren.
6. `pnpm` oder `npm` bereitstellen.
7. Editor mit Rust Analyzer einrichten.
8. Optional: Linux-Container/VM oder WSL für spätere Portabilitätstests vorbereiten.
9. PDFium-Binaries für Windows und später Linux beschaffen und versioniert ablegen.[cite:29][cite:16]

## Projektstruktur für den Agenten

Der Agent soll **nicht** monolithisch arbeiten, sondern mit einem Rust-Workspace und klar getrennten Schichten.

```text
pdf-app/
├─ Cargo.toml
├─ rust-toolchain.toml
├─ apps/
│  └─ desktop-tauri/
│     ├─ src-tauri/
│     └─ ui/
├─ crates/
│  ├─ pdf_core/
│  ├─ pdf_pdfium/
│  ├─ pdf_services/
│  ├─ pdf_types/
│  └─ pdf_cache/
├─ vendor/
│  └─ pdfium/
├─ assets/
├─ scripts/
└─ docs/
```

## Verantwortlichkeiten der Crates

### `pdf_types`

Gemeinsame Datentypen:

- `DocumentId`
- `PageIndex`
- `ZoomLevel`
- `SearchQuery`
- `SearchHit`
- `RecentFile`

### `pdf_core`

Domänenlogik ohne UI- oder OS-Abhängigkeiten:

- Dokument laden
- Seiten zählen
- Render-Aufträge planen
- Suche orchestrieren
- Navigations- und Viewer-Zustand modellieren
- Caching-Schnittstellen nutzen

### `pdf_pdfium`

Konkrete PDF-Engine auf Basis von `pdfium-render`.[cite:29][cite:23]

- PDFium laden
- Dokument öffnen
- Seiten rendern
- Text extrahieren
- Fehler sauber in Rust-Fehlertypen übersetzen

### `pdf_services`

Plattformnahe Services hinter Interfaces:

- Dateidialoge
- Dateisystemzugriff
- Settings
- „Im Explorer anzeigen“ / „Datei öffnen“

### `pdf_cache`

Thumbnail- und Render-Cache:

- Thumbnail-Keying
- Zoom-/Page-basierter Cache
- Eviction-Strategie

## Regeln für den Code-Agenten

Der Agent soll strikt nach diesen Regeln arbeiten:

1. **Kein UI-Framework im Core.** `pdf_core` darf keine Tauri-, Slint-, Iced- oder Frontend-Abhängigkeiten kennen.
2. **Keine Windows-spezifischen Dateipfade oder APIs im Core.**
3. **Alle OS-Funktionen hinter Traits kapseln.**
4. **PDF-Rendering zuerst stabilisieren, UI danach.**
5. **PDFium-Bundling früh einplanen, nicht erst zum Release.**[cite:29]
6. **Jede neue Funktion einer Schicht zuordnen:** Core, Service oder UI.
7. **Asynchrone Jobs sauber trennen:** Rendern, Thumbnail-Erstellung und Suche dürfen die UI nicht blockieren.

## Was der Agent zuerst implementieren soll

### Phase 1: Minimal lauffähiger PDF-Kern

- Rust-Workspace erzeugen.
- `pdf_types`, `pdf_core` und `pdf_pdfium` anlegen.
- `pdfium-render` integrieren.[cite:29]
- Test-PDF laden.
- Erste Seite rendern.
- Text von Seite 1 extrahieren.

### Phase 2: Desktop-Hülle

- Tauri-2-Projekt anlegen.[cite:15][cite:9]
- Frontend-UI initialisieren.
- Tauri Dialog Plugin einbauen.[cite:31]
- Tauri FS Plugin einbauen.[cite:33]
- Tauri Opener Plugin für Datei-/Ordneraktionen einbauen.[cite:40]

### Phase 3: MVP-Viewer

- Datei öffnen.
- Seitenanzahl anzeigen.
- Seite rendern.
- Zoom.
- Seitenwechsel.
- Thumbnail-Leiste.
- Textsuche.

## Installation der Rust- und Frontend-Werkzeuge

### Rust prüfen

```bash
rustc --version
cargo --version
rustup --version
```

### Node prüfen

```bash
node --version
npm --version
```

### pnpm optional installieren

```bash
npm install -g pnpm
```

### Tauri-CLI Nutzung

Der konkrete CLI-Weg kann je nach Projektsetup variieren, aber Tauri 2 dokumentiert die Voraussetzungen zentral in der Prerequisites-Dokumentation.[cite:9] Der Agent sollte das App-Scaffolding erst starten, wenn Rust, Node, Build Tools und WebView2 vollständig vorhanden sind.[cite:9]

## Empfohlene Rust-Abhängigkeiten

```toml
[workspace.dependencies]
anyhow = "1"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "env-filter"] }
uuid = { version = "1", features = ["v4", "serde"] }
lopdf = "0.38"
pdfium-render = "0.8"
```

Die Kernauswahl stützt sich darauf, dass `pdfium-render` die zentrale Rendering-Schicht bilden soll und `lopdf` eine optionale Ergänzung für Dokumentmanipulationen ist.[cite:29][cite:26]

## Tauri-spezifische Plugins

Im Desktop-Projekt sollten diese Bausteine vorbereitet werden:

- `tauri`
- `tauri-plugin-dialog`.[cite:31]
- `tauri-plugin-fs`.[cite:33]
- `tauri-plugin-opener`.[cite:40]

## Installations-Checkliste vor Start des Agenten

- [ ] Rust via `rustup` installiert.[cite:48]
- [ ] `cargo`, `rustc`, `rustup` funktionieren.
- [ ] Visual Studio C++ Build Tools installiert.[cite:9][cite:48]
- [ ] Windows SDK installiert.[cite:48]
- [ ] WebView2 Runtime vorhanden.[cite:9][cite:49]
- [ ] Git installiert.
- [ ] Node.js LTS installiert.
- [ ] `npm` oder `pnpm` eingerichtet.
- [ ] Editor/IDE mit Rust Analyzer eingerichtet.
- [ ] PDFium-Binaries für Windows besorgt und im Projekt abgelegt.[cite:29]
- [ ] Linux-Abhängigkeiten für später dokumentiert oder in VM/Container vorbereitet.[cite:12][cite:38]

## Setup-Schritte direkt vor Agentenstart

1. Leeres Git-Repository erstellen.
2. Workspace-Ordnerstruktur anlegen.
3. `vendor/pdfium/` vorbereiten.
4. `docs/architecture.md` mit den Schichtgrenzen anlegen.
5. Erst dann den Code-Agenten starten.

## Kurzer Master-Auftrag für den Agenten

Der Code-Agent soll eine **Windows-first PDF-Desktop-Anwendung in Rust mit Tauri 2** aufsetzen, die so strukturiert ist, dass sie später auf Linux portiert werden kann. Die Architektur muss einen **UI-unabhängigen Rust-Core**, eine **gekapselte PDFium-Integration** und **plattformnahe Services hinter Interfaces** enthalten; zuerst sind Workspace, PDF-Kern, Datei öffnen und Rendern der ersten Seite umzusetzen, danach die Viewer-UI und weitere MVP-Funktionen.[cite:15][cite:29][cite:31][cite:33]
