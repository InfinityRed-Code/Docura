# PDFium binaries

`pdfium-render` does not bundle the native PDFium library. Drop per-target
binaries here so the engine can bind to them at runtime.

Suggested source: https://github.com/bblanchon/pdfium-binaries/releases

Expected layout after download:

```
vendor/pdfium/
├─ windows/x64/pdfium.dll
├─ linux/x64/libpdfium.so
└─ macos/arm64/libpdfium.dylib   # optional, future
```

For development the engine also probes the current working directory and
`./vendor/pdfium` for `pdfium.dll` / `libpdfium.so`. For release builds,
copy the matching binary next to the packaged executable (Tauri resource
or manual copy step).
