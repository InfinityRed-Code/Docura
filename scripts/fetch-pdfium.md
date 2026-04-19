# Fetching PDFium binaries

Short manual recipe — automate later in a real script.

## Windows x64

1. Download latest release from
   https://github.com/bblanchon/pdfium-binaries/releases
   (asset: `pdfium-win-x64.tgz`).
2. Extract and copy `bin/pdfium.dll` to `vendor/pdfium/windows/x64/`.
3. Also copy `pdfium.dll` next to the Tauri dev binary during development,
   or let the loader pick it up from `vendor/pdfium` (the engine probes
   both paths).

## Linux x64 (future)

1. Download `pdfium-linux-x64.tgz` from the same release.
2. Copy `lib/libpdfium.so` to `vendor/pdfium/linux/x64/`.
