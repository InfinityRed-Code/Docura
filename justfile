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

# Release build of desktop app
build: pdfium
    cd {{tauri_dir}}; & ./ui/node_modules/.bin/tauri.CMD build

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
