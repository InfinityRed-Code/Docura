# Stage veraPDF (the reference PDF/A validator) plus a portable Java runtime
# into vendor/verapdf. Used by the PDF/A conformance tests
# (`cargo test -p pdf_pdfa --test verapdf`) and scripts/pdfa-bench.ps1.
# Nothing here ships with the app; vendor/verapdf is gitignored.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-verapdf.ps1
#
# Afterwards `vendor/verapdf/verapdf/verapdf.bat` exists. Tests find it there
# or through the VERAPDF environment variable.

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root "vendor/verapdf"
$jreUrl = "https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jre/hotspot/normal/eclipse"
$veraUrl = "https://software.verapdf.org/rel/verapdf-installer.zip"

# Not Expand-Archive: started from PowerShell 7 (GitHub Actions, `just`),
# Windows PowerShell 5.1 picks up the newer Archive module and fails in it.
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Expand-Zip([string]$zip, [string]$target) {
    [System.IO.Compression.ZipFile]::ExtractToDirectory($zip, $target)
}

New-Item -ItemType Directory -Force $dest | Out-Null
$dl = Join-Path $dest "_download"
New-Item -ItemType Directory -Force $dl | Out-Null

# --- Java runtime (Temurin 21 JRE, portable zip) ---
$jreDir = Join-Path $dest "jre"
if (-not (Test-Path (Join-Path $jreDir "bin/java.exe"))) {
    Write-Host "Downloading Temurin 21 JRE..."
    $zip = Join-Path $dl "jre.zip"
    Invoke-WebRequest -Uri $jreUrl -OutFile $zip
    $tmp = Join-Path $dl "jre-x"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Zip $zip $tmp
    $inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
    if (Test-Path $jreDir) { Remove-Item -Recurse -Force $jreDir }
    Move-Item $inner.FullName $jreDir
}
$java = Join-Path $jreDir "bin/java.exe"
& $java -version

# --- veraPDF (IzPack installer, unattended) ---
$veraDir = Join-Path $dest "verapdf"
if (-not (Test-Path (Join-Path $veraDir "verapdf.bat"))) {
    Write-Host "Downloading veraPDF..."
    $zip = Join-Path $dl "verapdf-installer.zip"
    Invoke-WebRequest -Uri $veraUrl -OutFile $zip
    $tmp = Join-Path $dl "vera-x"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Zip $zip $tmp
    $jar = Get-ChildItem $tmp -Recurse -Filter "verapdf-izpack-installer-*.jar" | Select-Object -First 1
    if (-not $jar) { throw "installer jar not found in $veraUrl" }
    $auto = Join-Path $dl "auto-install.xml"
    $installPath = [System.Security.SecurityElement]::Escape($veraDir)
    @"
<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<AutomatedInstallation langpack="eng">
  <com.izforge.izpack.panels.htmlhello.HTMLHelloPanel id="welcome"/>
  <com.izforge.izpack.panels.target.TargetPanel id="install_dir">
    <installpath>$installPath</installpath>
  </com.izforge.izpack.panels.target.TargetPanel>
  <com.izforge.izpack.panels.packs.PacksPanel id="sdk_pack_select">
    <pack index="0" name="veraPDF GUI" selected="false"/>
    <pack index="1" name="veraPDF CLI" selected="true"/>
    <pack index="2" name="veraPDF Documentation" selected="false"/>
    <pack index="3" name="veraPDF Sample Plugins" selected="false"/>
  </com.izforge.izpack.panels.packs.PacksPanel>
  <com.izforge.izpack.panels.install.InstallPanel id="install"/>
  <com.izforge.izpack.panels.finish.FinishPanel id="finish"/>
</AutomatedInstallation>
"@ | Set-Content -Encoding UTF8 $auto
    & $java -jar $jar.FullName $auto
    if (-not (Test-Path (Join-Path $veraDir "verapdf.bat"))) { throw "veraPDF install failed" }
}

# verapdf.bat looks for java on PATH or JAVA_HOME; write a wrapper that pins
# the staged runtime so callers need neither.
$wrapper = Join-Path $dest "verapdf.cmd"
@"
@echo off
setlocal
set "JAVA_HOME=$jreDir"
set "PATH=$jreDir\bin;%PATH%"
call "$veraDir\verapdf.bat" %*
"@ | Set-Content -Encoding ASCII $wrapper

Remove-Item -Recurse -Force $dl
& $wrapper --version
Write-Host "veraPDF staged: $wrapper"
