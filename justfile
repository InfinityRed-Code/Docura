# rustpdf dev tasks — run `just` to list, `just <recipe>` to invoke

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

tauri_dir := "apps/desktop-tauri"
ui_dir := "apps/desktop-tauri/ui"
res_dll := "apps/desktop-tauri/src-tauri/resources/pdfium.dll"

default:
    @just --list

# One-shot setup: UI deps + stage pdfium + verify toolchain
setup: install pdfium doctor

# Stage pdfium.dll into src-tauri/resources (tauri bundle resource; gitignored, needed by every build)
pdfium:
    @if (-not (Test-Path vendor/pdfium/pdfium.dll)) { Write-Host "MISSING: vendor/pdfium/pdfium.dll"; exit 1 }
    @New-Item -ItemType Directory -Force (Split-Path {{res_dll}}) | Out-Null
    @Copy-Item vendor/pdfium/pdfium.dll {{res_dll}} -Force
    @Write-Host "staged: {{res_dll}}"

# Launch Tauri app in dev mode (hot reload)
dev: pdfium
    cd {{tauri_dir}}; & ./ui/node_modules/.bin/tauri.CMD dev

# Alias for `dev`
open: dev

# Alias for `dev`
run: dev

# Kill running dev app + Vite on :1420
stop:
    -Get-Process desktop-tauri -ErrorAction SilentlyContinue | Stop-Process -Force
    -Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

# Stage Tesseract OCR into vendor/tesseract (the installer bundles it; needs 7-Zip)
tesseract:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-tesseract.ps1

# Release build of desktop app (NSIS installer)
build: pdfium tesseract
    cd {{tauri_dir}}; & ./ui/node_modules/.bin/tauri.CMD build

# Source archive for the bundled Tesseract into dist/ (GPL/LGPL source offer; attach to the GitHub release, not part of the installer)
tesseract-sources:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-tesseract-sources.ps1

# Everything a GitHub release needs: the installer and the Tesseract source archive
release: build tesseract-sources
    @Write-Host "Attach to the GitHub release: target/release/bundle/nsis/*-setup.exe and dist/docura-pdf-tesseract-sources-*.zip"

# Clean artifacts then relaunch dev
rebuild: clean dev

# cargo clean
clean:
    cargo clean

# cargo check workspace
check:
    cargo check --workspace

# Run all tests
test:
    cargo test --workspace

# Format Rust code
fmt:
    cargo fmt --all

# Clippy with warnings as errors
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# fmt check + clippy + tests (CI gate)
ci: fmt-check lint test

# Verify formatting without writing
fmt-check:
    cargo fmt --all -- --check

# PDF/A gate: stage veraPDF if missing, unit tests + conformance matrix (corpus x all levels), fails on any FAIL
pdfa-check:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/pdfa-check.ps1

# Function/line coverage of the PDF/A converter (needs `cargo install cargo-llvm-cov`)
pdfa-coverage:
    cargo llvm-cov -p pdf_pdfa --ignore-filename-regex "fonts.tables\.rs|examples|pdf_(pdfium|mutate|fonts|core|types|cache|render|text)" --summary-only

# PDF/A benchmark against Ghostscript on the test corpus; pass directories or files to add your own PDFs
pdfa-bench *ARGS:
    cargo run --release -p pdf_pdfa --example pdfa_bench -- --corpus {{ARGS}}

# Install UI deps
install:
    pnpm --dir {{ui_dir}} install

# Verify toolchain + pdfium are present
doctor:
    @Write-Host "== toolchain =="
    @cargo --version
    @rustc --version
    @node --version
    @pnpm --version
    @Write-Host "== pdfium =="
    @if (Test-Path vendor/pdfium/pdfium.dll) { Write-Host "ok: vendor/pdfium/pdfium.dll" } else { Write-Host "MISSING: vendor/pdfium/pdfium.dll"; exit 1 }
    @if (Test-Path {{res_dll}}) { Write-Host "ok: {{res_dll}}" } else { Write-Host "MISSING (run: just pdfium): {{res_dll}}" }
    @Write-Host "== tesseract (installer only) =="
    @if (Test-Path vendor/tesseract/tesseract.exe) { Write-Host "ok: vendor/tesseract" } else { Write-Host "MISSING (run: just tesseract): vendor/tesseract" }
