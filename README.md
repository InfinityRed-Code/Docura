# rustpdf

Windows-first PDF desktop app, Linux-ready. Rust workspace + Tauri 2 shell.

See `setup.md` for the full brief and `docs/architecture.md` for layer rules.

## Layout

- `apps/desktop-tauri/` — Tauri 2 shell (Rust backend in `src-tauri`, web UI in `ui`)
- `crates/pdf_types` — shared data types
- `crates/pdf_core` — UI/OS-free domain logic
- `crates/pdf_pdfium` — PDFium engine via `pdfium-render`
- `crates/pdf_services` — platform services behind traits
- `crates/pdf_cache` — thumbnail/render cache
- `vendor/pdfium/` — bundled PDFium binaries (per-target)

## Prerequisites

Rust (rustup), Node.js LTS, pnpm, Git, MSVC C++ Build Tools + Windows SDK, WebView2 runtime.

## Build

```bash
cargo check --workspace
```
