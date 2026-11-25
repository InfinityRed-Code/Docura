# PDF/A conformance gate (`just pdfa-check`).
#
# Stages veraPDF if it is missing, runs the unit tests of `pdf_pdfa` and the
# conformance matrix (every corpus file x every PDF/A level, validated with
# veraPDF) and fails on the first non-conformant output. A missing veraPDF
# is an error here (PDFA_REQUIRE_VERAPDF=1), unlike in plain `cargo test`.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/pdfa-check.ps1
#
# Needs PDFium (vendor/pdfium, see scripts/fetch-pdfium.md); the matrix
# renders the pages that have no lossless fix with PDFium, as the app does.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# The engine looks for the library in vendor/pdfium; the repository keeps it
# in vendor/pdfium/bin (a fresh checkout or CI has only that copy).
if (-not (Test-Path "vendor/pdfium/pdfium.dll") -and (Test-Path "vendor/pdfium/bin/pdfium.dll")) {
    Copy-Item "vendor/pdfium/bin/pdfium.dll" "vendor/pdfium/pdfium.dll"
}
if (-not (Test-Path "vendor/pdfium/pdfium.dll")) {
    Write-Host "MISSING: vendor/pdfium/pdfium.dll (see scripts/fetch-pdfium.md)"
    exit 1
}
if (-not (Test-Path "vendor/verapdf/verapdf.cmd")) {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "fetch-verapdf.ps1")
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$env:PDFA_REQUIRE_VERAPDF = "1"

cargo test -p pdf_pdfa --lib
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo test -p pdf_pdfa --test verapdf
$code = $LASTEXITCODE

$matrix = "target/pdfa-matrix/matrix.md"
if (Test-Path $matrix) {
    $text = Get-Content $matrix -Raw -Encoding UTF8
    $pass = ([regex]::Matches($text, [string][char]0x2705)).Count
    $refused = ([regex]::Matches($text, [string][char]0x26D4)).Count
    $fail = ([regex]::Matches($text, [string][char]0x274C)).Count
    $unchecked = ([regex]::Matches($text, [string][char]0x2754)).Count
    Write-Host "PDF/A matrix: $pass PASS, $fail FAIL, $refused refused as expected, $unchecked not validated ($matrix)"
    if ($fail -gt 0 -or $unchecked -gt 0) { $code = 1 }
}
exit $code
