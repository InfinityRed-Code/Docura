<#
  Docura PDF - unattended update test for the NSIS setup (#4).

  Installs setup A, then runs setup B (newer) over it with /S while the app
  and a bundled tesseract.exe are running, and checks that B updated A in
  place: one Apps entry with B's version, files replaced (a stale file in the
  Tesseract folder is gone), app data, shortcuts, autostart, the saved
  previous .pdf handler and the virtual printer kept (printer-setup.log shows
  no removal). Then the same version again (repair), a silent downgrade to A
  and back to B, and finally a silent uninstall (skip with -KeepInstalled).

  Run it from an elevated PowerShell for the per-machine release setups (the
  printer needs administrator rights). Without elevation it can test a setup
  built per user, e.g.
    tauri build --config '{"bundle":{"windows":{"nsis":{"installMode":"currentUser"}}}}'
  and skips the printer checks.

    powershell -ExecutionPolicy Bypass -File scripts\test-installer-update.ps1 `
      -SetupA 'Docura PDF_1.3.98_x64-setup.exe' -SetupB 'Docura PDF_1.3.99_x64-setup.exe'

  SetupA may also be a release from before the in-place update (1.3.1) to
  test the real update path; add -SkipDowngrade then, as its setup still
  recreates shortcuts and the .pdf backup when run over a newer version.

  The interactive dialogs are not covered; see the manual checklist in the
  pull request for #4. Docura PDF must not be installed in the way the setups
  install it (per machine or per user); an installation of the other kind is
  left alone, but a running Docura PDF may be closed by the setups.
  Exit code: 0 = all checks passed, 1 = a check failed.

  Keep this file ASCII-only: Windows PowerShell 5.1 reads BOM-less scripts in
  the ANSI code page.
#>
param(
    [Parameter(Mandatory)] [string] $SetupA,
    [Parameter(Mandatory)] [string] $SetupB,
    [switch] $KeepInstalled,
    [switch] $SkipDowngrade
)

$ErrorActionPreference = 'Stop'
$Product = 'Docura PDF'
$UninstKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\$Product"
$AppData = Join-Path $env:APPDATA 'com.docura.pdf'
$PrinterLog = Join-Path $env:ProgramData 'DocuraPDF\printer-setup.log'
$IsAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
$script:Failures = 0

function Write-Check([bool] $Ok, [string] $What) {
    if ($Ok) { Write-Host "  ok    $What" -ForegroundColor Green }
    else { Write-Host "  FAIL  $What" -ForegroundColor Red; $script:Failures++ }
}

function Get-SetupVersion([string] $Path) {
    if ((Split-Path $Path -Leaf) -match '_(\d+\.\d+\.\d+[^_]*)_') { return $Matches[1] }
    throw "cannot read the version from the file name '$Path' (expected <name>_<version>_<arch>-setup.exe)"
}

# Uninstall entries of Docura PDF, per machine (64-bit view) and per user.
function Get-Installs([string[]] $Hives = @('HKLM', 'HKCU')) {
    $Hives | ForEach-Object {
        $p = "$($_):\$UninstKey"
        if (Test-Path $p) { Get-ItemProperty $p | Select-Object @{ n = 'Hive'; e = { $_.PSDrive.Name } }, DisplayVersion, InstallLocation }
    }
}

# The .pdf association of the hive under test: current handler and the
# backup the uninstaller restores.
function Get-PdfAssociation {
    $p = "$($script:Hive):\Software\Classes\.pdf"
    $k = Get-ItemProperty $p -ErrorAction SilentlyContinue
    [pscustomobject] @{ Handler = $k.'(default)'; Backup = $k.'DocuraPDF.pdf_backup' }
}

function Invoke-Setup([string] $Path, [string[]] $Arguments) {
    Write-Host "> $(Split-Path $Path -Leaf) $($Arguments -join ' ')"
    $p = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru -Wait
    return $p.ExitCode
}

function Get-PrinterLogLength {
    if (Test-Path $PrinterLog) { return (Get-Content $PrinterLog).Count } else { return 0 }
}

function Get-Shortcuts {
    $dirs = @(
        [Environment]::GetFolderPath('CommonPrograms'), [Environment]::GetFolderPath('Programs'),
        [Environment]::GetFolderPath('CommonDesktopDirectory'), [Environment]::GetFolderPath('DesktopDirectory'))
    $dirs | ForEach-Object { Join-Path $_ "$Product.lnk" } | Where-Object { Test-Path $_ } | Sort-Object
}

function Get-Autostart {
    (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).$Product
}

function Test-Installed([string] $Version, [string] $Label) {
    $installs = @(Get-Installs $script:Hive)
    Write-Check ($installs.Count -eq 1) "$Label - exactly one Apps entry (found $($installs.Count))"
    if ($installs.Count -ge 1) {
        Write-Check ($installs[0].DisplayVersion -eq $Version) "$Label - Apps entry shows $Version (found $($installs[0].DisplayVersion))"
    }
    foreach ($f in 'desktop-tauri.exe', 'uninstall.exe', 'resources\pdfium.dll', 'tesseract\tesseract.exe', 'printer-setup.ps1') {
        Write-Check (Test-Path (Join-Path $script:InstDir $f)) "$Label - $f present"
    }
}

$versionA = Get-SetupVersion $SetupA
$versionB = Get-SetupVersion $SetupB
$SetupA = (Resolve-Path $SetupA).Path
$SetupB = (Resolve-Path $SetupB).Path
Write-Host "Setup A: $versionA, setup B: $versionB, elevated: $IsAdmin"
# An installation of the other kind (per machine / per user) may stay; the
# setups under test must start from an empty hive.
$existing = @(Get-Installs)
$existing | ForEach-Object { Write-Host "  existing installation left alone: $($_.Hive) $($_.DisplayVersion) $($_.InstallLocation)" }
$shortcutsBefore = @(Get-Shortcuts)

# --- 1. fresh install of A ---------------------------------------------------
Write-Host "`n1. Fresh install of $versionA"
Write-Check ((Invoke-Setup $SetupA '/S') -eq 0) 'setup A exit code 0'
$install = @(Get-Installs | Where-Object { $_.Hive -notin $existing.Hive })[0]
if (-not $install) {
    throw "setup A registered no installation in a hive that was empty before (existing: $($existing.Hive -join ', ')); uninstall $Product of that kind first"
}
$script:Hive = $install.Hive
$script:InstDir = $install.InstallLocation.Trim('"')
$perMachine = $script:Hive -eq 'HKLM'
Write-Host "  installed in $script:InstDir ($(if ($perMachine) { 'per machine' } else { 'per user' }))"
Test-Installed $versionA 'A'
$assoc = Get-PdfAssociation
Write-Check ($assoc.Handler -eq 'DocuraPDF.pdf') ".pdf opens with $Product ($($script:Hive))"
Write-Check ($assoc.Backup -ne 'DocuraPDF.pdf') "previous .pdf handler saved for the uninstaller ('$($assoc.Backup)')"
$checkPrinter = $perMachine -and $IsAdmin
if ($checkPrinter) {
    Write-Check ($null -ne (Get-Printer -Name $Product -ErrorAction SilentlyContinue)) "printer '$Product' installed"
} else {
    Write-Host '  (printer checks skipped: needs a per-machine setup run elevated)'
}

# State an update must keep. A silent first install creates a desktop
# shortcut; delete it like a user who does not want one. A silent run of the
# stock template would create it again, an update leaves it deleted.
$desktopLink = @(Get-Shortcuts | Where-Object { $_ -notin $shortcutsBefore -and $_ -like '*Desktop*' })
Write-Check ($desktopLink.Count -eq 1) "silent install created a desktop shortcut ($($desktopLink -join ', '))"
$desktopLink | ForEach-Object { Remove-Item -LiteralPath $_ }
New-Item -ItemType Directory -Force $AppData | Out-Null
$marker = Join-Path $AppData 'update-test-marker.txt'
Set-Content -LiteralPath $marker -Value "written before updating from $versionA"
$shortcuts = @(Get-Shortcuts)
$autostart = Get-Autostart
$printerLogStart = Get-PrinterLogLength
$stale = Join-Path $script:InstDir 'tesseract\stale-from-old-version.dll'
Set-Content -LiteralPath $stale -Value 'must be gone after the update'

# --- 2. update A -> B with the app and Tesseract running ---------------------
Write-Host "`n2. Update $versionA -> $versionB, app and tesseract.exe running"
$app = Start-Process -FilePath (Join-Path $script:InstDir 'desktop-tauri.exe') -PassThru
# Without input, tesseract waits on stdin and keeps its files open.
$tess = Start-Process -FilePath (Join-Path $script:InstDir 'tesseract\tesseract.exe') -ArgumentList 'stdin', 'stdout' -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 3
Write-Check (-not $app.HasExited) 'app running before the update'
Write-Check (-not $tess.HasExited) 'bundled tesseract.exe running before the update'
Write-Check ((Invoke-Setup $SetupB '/S') -eq 0) 'setup B exit code 0'
Write-Check $app.HasExited 'app was closed by the setup'
Write-Check $tess.HasExited 'bundled tesseract.exe was stopped by the setup'
Test-Installed $versionB 'B'
Write-Check (-not (Test-Path $stale)) 'Tesseract folder replaced (stale file gone)'
Write-Check (Test-Path $marker) "app data kept ($AppData)"
Write-Check ((@(Get-Shortcuts) -join '|') -eq ($shortcuts -join '|')) "shortcuts unchanged, deleted desktop shortcut not recreated ($($shortcuts.Count) left)"
Write-Check ((Get-Autostart) -eq $autostart) 'autostart entry unchanged'
Write-Check ((Get-PdfAssociation).Backup -eq $assoc.Backup) 'saved previous .pdf handler unchanged'
if ($checkPrinter) {
    Write-Check ($null -ne (Get-Printer -Name $Product -ErrorAction SilentlyContinue)) "printer '$Product' still installed"
    $log = @(Get-Content $PrinterLog | Select-Object -Skip $printerLogStart)
    Write-Check (-not ($log -match '\[Uninstall\]|Removed printer')) 'printer not removed during the update (printer-setup.log)'
    Write-Check ([bool]($log -match 'already installed')) 'printer setup only checked the printer (printer-setup.log)'
}

# --- 3. same version again ---------------------------------------------------
Write-Host "`n3. $versionB again (repair)"
Write-Check ((Invoke-Setup $SetupB '/S') -eq 0) 'setup B exit code 0'
Test-Installed $versionB 'B again'
Write-Check (Test-Path $marker) 'app data kept'

# --- 4. downgrade and back -----------------------------------------------------
if (-not $SkipDowngrade) {
    Write-Host "`n4. Downgrade $versionB -> $versionA (silent runs go ahead), then back to $versionB"
    Write-Check ((Invoke-Setup $SetupA '/S') -eq 0) 'setup A exit code 0'
    Test-Installed $versionA 'downgraded'
    Write-Check ((Invoke-Setup $SetupB '/S') -eq 0) 'setup B exit code 0'
    Test-Installed $versionB 'updated again'
    Write-Check (Test-Path $marker) 'app data kept'
    Write-Check ((@(Get-Shortcuts) -join '|') -eq ($shortcuts -join '|')) 'shortcuts unchanged'
    Write-Check ((Get-PdfAssociation).Backup -eq $assoc.Backup) 'saved previous .pdf handler unchanged'
}

# --- 5. uninstall ----------------------------------------------------------------
if (-not $KeepInstalled) {
    Write-Host "`n5. Uninstall"
    $uninstaller = Join-Path $script:InstDir 'uninstall.exe'
    $p = Start-Process -FilePath $uninstaller -ArgumentList '/S', "_?=$script:InstDir" -PassThru -Wait
    Write-Check ($p.ExitCode -eq 0) 'uninstaller exit code 0'
    # With _?= the uninstaller cannot delete itself.
    Remove-Item -LiteralPath $uninstaller -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $script:InstDir -ErrorAction SilentlyContinue
    Write-Check (@(Get-Installs $script:Hive).Count -eq 0) 'Apps entry removed'
    Write-Check (-not (Test-Path $script:InstDir)) 'install directory removed'
    Write-Check (Test-Path $marker) 'app data kept by a plain uninstall'
    Write-Check ([string](Get-PdfAssociation).Handler -eq [string]$assoc.Backup) ".pdf handler restored to '$($assoc.Backup)'"
    if ($checkPrinter) {
        Write-Check ($null -eq (Get-Printer -Name $Product -ErrorAction SilentlyContinue)) "printer '$Product' removed"
    }
}
Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue

Write-Host ''
if ($script:Failures -eq 0) { Write-Host 'All checks passed.' -ForegroundColor Green; exit 0 }
Write-Host "$script:Failures check(s) failed." -ForegroundColor Red
exit 1
