<#
  Stage Tesseract OCR for the Windows installer into vendor\tesseract.

  The installer hook (apps/desktop-tauri/src-tauri/nsis/hooks.nsi) copies this
  directory to <install dir>\tesseract and refuses to build without it; the
  app finds the bundled tesseract.exe there (src/ocr.rs).

  Sources, pinned by SHA-256:
    - UB Mannheim Windows build of Tesseract (the build the Tesseract project
      points Windows users to). Its NSIS installer is unpacked with 7-Zip,
      not run. Only tesseract.exe and the DLLs it loads are kept; training
      tools, ICU, cairo, pango etc. are not needed at runtime.
    - tessdata_fast language models deu + eng (the same model family the app
      downloads for further languages).
  Plus, from this repo: THIRD-PARTY-NOTICES.txt and licenses\ (full GPL,
  LGPL, Apache and per-library license texts), see
  tesseract-THIRD-PARTY-NOTICES.txt and tesseract-licenses\.

  The matching corresponding-source archive is built separately by
  fetch-tesseract-sources.ps1 and attached to the GitHub release; bump both
  scripts and the notices together.

  Requires 7-Zip (7z.exe on PATH or in Program Files).

  Usage:  powershell -ExecutionPolicy Bypass -File scripts\fetch-tesseract.ps1 [-Force]
          (or: just tesseract)
#>
param([switch] $Force)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Version     = '5.4.0.20240606'
$SetupUrl    = "https://github.com/UB-Mannheim/tesseract/releases/download/v$Version/tesseract-ocr-w64-setup-$Version.exe"
$SetupSha256 = 'C885FFF6998E0608BA4BB8AB51436E1C6775C2BAFC2559A19B423E18678B60C9'

$TessdataTag = '4.1.0'
$Models = [ordered]@{
    deu = '19D219BBB6672C869D20A9636C6816A81EB9A71796CB93EBE0CB1530E2CDB22D'
    eng = '7D4322BD2A7749724879683FC3912CB542F19906C83BCC1A52132556427170B2'
}

# tesseract.exe plus the transitive closure of its DLL imports in this build
# (objdump -p). Re-derive when bumping $Version; the smoke test at the end
# fails on a missing DLL.
$Runtime = @(
    'tesseract.exe', 'libtesseract-5.dll', 'libleptonica-6.dll',
    'libarchive-13.dll', 'libb2-1.dll', 'libbz2-1.dll', 'libcrypto-3-x64.dll',
    'libdeflate.dll', 'libexpat-1.dll', 'libgcc_s_seh-1.dll', 'libgif-7.dll',
    'libiconv-2.dll', 'libjbig-0.dll', 'libjpeg-8.dll', 'libLerc.dll',
    'liblz4.dll', 'liblzma-5.dll', 'libopenjp2-7.dll', 'libpng16-16.dll',
    'libsharpyuv-0.dll', 'libstdc++-6.dll', 'libtiff-6.dll', 'libwebp-7.dll',
    'libwebpmux-3.dll', 'libwinpthread-1.dll', 'libzstd.dll', 'zlib1.dll'
)

$Root   = Split-Path -Parent $PSScriptRoot
$Dest   = Join-Path $Root 'vendor\tesseract'
$Stamp  = Join-Path $Dest 'VERSION.txt'
$StampText = "Tesseract $Version (UB Mannheim build), tessdata_fast $TessdataTag ($($Models.Keys -join ', '))"

# Notices and full license texts (GPL/LGPL included) live in the repo next
# to this script; refreshed on every run, also when the binaries are current.
function Copy-Notices {
    # The notice's source offer must name the archive that
    # fetch-tesseract-sources.ps1 builds for exactly this bundle.
    $notices = Join-Path $PSScriptRoot 'tesseract-THIRD-PARTY-NOTICES.txt'
    if ((Get-Content $notices -Raw) -notmatch [regex]::Escape("docura-pdf-tesseract-sources-$Version.zip")) {
        throw "tesseract-THIRD-PARTY-NOTICES.txt does not name docura-pdf-tesseract-sources-$Version.zip"
    }
    if ((Get-Content (Join-Path $PSScriptRoot 'fetch-tesseract-sources.ps1') -Raw) -notmatch [regex]::Escape("`$Bundle = '$Version'")) {
        throw "fetch-tesseract-sources.ps1 is not pinned to Tesseract $Version"
    }
    Copy-Item -Force $notices (Join-Path $Dest 'THIRD-PARTY-NOTICES.txt')
    $licenses = Join-Path $Dest 'licenses'
    if (Test-Path $licenses) { Remove-Item -Recurse -Force $licenses }
    Copy-Item -Recurse (Join-Path $PSScriptRoot 'tesseract-licenses') $licenses
    foreach ($required in 'GPL-2.0.txt', 'GPL-3.0.txt', 'LGPL-2.1.txt', 'Apache-2.0.txt', 'GCC-exception-3.1.txt') {
        if (-not (Test-Path (Join-Path $licenses $required))) { throw "license text missing: $required" }
    }
}

if (-not $Force -and (Test-Path $Stamp) -and ((Get-Content $Stamp -Raw).Trim() -eq $StampText)) {
    Copy-Notices
    Write-Host "up to date: $Dest ($StampText), notices refreshed"
    exit 0
}

function Find-SevenZip {
    $cmd = Get-Command 7z.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($base in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if ($base -and (Test-Path (Join-Path $base '7-Zip\7z.exe'))) { return (Join-Path $base '7-Zip\7z.exe') }
    }
    throw '7-Zip not found (needed to unpack the Tesseract installer). Install it, e.g. winget install 7zip.7zip'
}

function Get-Verified([string] $Url, [string] $OutFile, [string] $Sha256) {
    Write-Host "download: $Url"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile
    $actual = (Get-FileHash -Algorithm SHA256 $OutFile).Hash
    if ($actual -ne $Sha256) { throw "SHA-256 mismatch for $Url`n  expected $Sha256`n  actual   $actual" }
}

$sevenZip = Find-SevenZip
$work = Join-Path ([IO.Path]::GetTempPath()) ("docura-tesseract-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
try {
    $setup = Join-Path $work 'setup.exe'
    Get-Verified $SetupUrl $setup $SetupSha256

    $unpacked = Join-Path $work 'unpacked'
    & $sevenZip x $setup "-o$unpacked" -y | Out-Null
    # 1 = warnings only (7-Zip cannot restore NSIS-internal entries).
    if ($LASTEXITCODE -gt 1) { throw "7-Zip failed to unpack the installer (exit $LASTEXITCODE)" }

    if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
    New-Item -ItemType Directory -Force -Path (Join-Path $Dest 'tessdata') | Out-Null

    foreach ($f in $Runtime) { Copy-Item (Join-Path $unpacked $f) $Dest }
    Copy-Item (Join-Path $unpacked 'doc\LICENSE') (Join-Path $Dest 'LICENSE.txt')
    Copy-Item (Join-Path $unpacked 'doc\AUTHORS') (Join-Path $Dest 'AUTHORS.txt')
    Copy-Notices
    Copy-Item -Recurse (Join-Path $unpacked 'tessdata\configs') (Join-Path $Dest 'tessdata\configs')
    Copy-Item (Join-Path $unpacked 'tessdata\pdf.ttf') (Join-Path $Dest 'tessdata')

    foreach ($lang in $Models.Keys) {
        Get-Verified "https://github.com/tesseract-ocr/tessdata_fast/raw/$TessdataTag/$lang.traineddata" `
            (Join-Path $Dest "tessdata\$lang.traineddata") $Models[$lang]
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

# Smoke test with a minimal PATH, so a DLL missing from $Runtime cannot be
# satisfied by some other install on this machine. SEM_FAILCRITICALERRORS
# (inherited by the child) turns the loader's "DLL not found" dialog into a
# plain exit code instead of blocking the script.
Add-Type -Namespace DocuraFetch -Name Kernel32 -MemberDefinition '[DllImport("kernel32.dll")] public static extern uint SetErrorMode(uint mode);'
$savedMode = [DocuraFetch.Kernel32]::SetErrorMode(0x0001)
$savedPath = $env:PATH
$savedPrefix = $env:TESSDATA_PREFIX
try {
    $ErrorActionPreference = 'Continue'
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:TESSDATA_PREFIX = Join-Path $Dest 'tessdata'
    $langs = & (Join-Path $Dest 'tesseract.exe') --list-langs 2>&1 | Out-String
    $code = $LASTEXITCODE
} finally {
    $ErrorActionPreference = 'Stop'
    $env:PATH = $savedPath
    $env:TESSDATA_PREFIX = $savedPrefix
    [void][DocuraFetch.Kernel32]::SetErrorMode($savedMode)
}
if ($code -ne 0) { throw "staged tesseract.exe does not start (exit $code), a DLL is probably missing:`n$langs" }
foreach ($lang in $Models.Keys) {
    if ($langs -notmatch "(?m)^$lang\s*$") { throw "staged tesseract.exe does not list '$lang':`n$langs" }
}

Set-Content -Path $Stamp -Value $StampText -Encoding ASCII
$sizeMb = [math]::Round(((Get-ChildItem $Dest -Recurse -File | Measure-Object Length -Sum).Sum / 1MB), 1)
Write-Host "staged: $Dest ($StampText, $sizeMb MB)"
