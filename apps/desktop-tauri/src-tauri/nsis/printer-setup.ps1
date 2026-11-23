<#
  Docura PDF - virtual printer setup.

  Run by the NSIS installer (elevated, see hooks.nsi) to register or remove
  the "Docura PDF" printer. Mirrors install_virtual_printer /
  uninstall_virtual_printer in src/vprinter.rs: printer name, driver, spool
  directory and port path must stay in sync with that file, otherwise the app
  reports the printer as missing and never sees the print jobs.

  Can also be run by hand from an elevated PowerShell to repair the printer:
    powershell -ExecutionPolicy Bypass -File printer-setup.ps1 -Action Install

  Every step is appended to %ProgramData%\DocuraPDF\printer-setup.log and
  echoed to stdout (the installer shows it in its details list).
  Exit code: 0 = success, 1 = failure.

  Keep this file ASCII-only: Windows PowerShell 5.1 reads BOM-less scripts in
  the ANSI code page.
#>
param(
    [ValidateSet('Install', 'Uninstall')]
    [string] $Action = 'Install'
)

$ErrorActionPreference = 'Stop'

$PrinterName = 'Docura PDF'
$DriverName  = 'Microsoft PS Class Driver'
$BaseDir     = Join-Path $env:ProgramData 'DocuraPDF'
$SpoolDir    = Join-Path $BaseDir 'spool'
$PortPath    = Join-Path $SpoolDir 'docura-spool.ps'
$LogFile     = Join-Path $BaseDir 'printer-setup.log'

# Written by installers before the rename to Docura PDF (never worked, but
# remove it should an elevated run have created it).
$LegacyPrinter = 'rustpdf'
$LegacyPort    = Join-Path $env:ProgramData 'rustpdf\spool\rustpdf-spool.ps'

function Write-Log([string] $Message) {
    $line = '{0:yyyy-MM-dd HH:mm:ss} [{1}] {2}' -f (Get-Date), $Action, $Message
    Write-Output $line
    try { Add-Content -LiteralPath $LogFile -Value $line -Encoding UTF8 } catch { }
}

function Invoke-Native([string] $Exe, [string[]] $Arguments) {
    # Windows PowerShell 5.1 turns stderr of a redirected native command into
    # a terminating error under 'Stop'; judge by the exit code instead.
    $ErrorActionPreference = 'Continue'
    $out = & $Exe @Arguments 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed (exit $LASTEXITCODE): $($out.Trim())" }
}

function Remove-PortWithRetry([string] $Name) {
    if (-not (Get-PrinterPort -Name $Name -ErrorAction SilentlyContinue)) { return }
    # The spooler releases a port a moment after its printer is deleted.
    for ($i = 1; $i -le 5; $i++) {
        try {
            Remove-PrinterPort -Name $Name
            Write-Log "Removed port '$Name'"
            return
        } catch {
            if ($i -eq 5) { throw }
            Start-Sleep -Seconds 1
        }
    }
}

function Install-DocuraPrinter {
    New-Item -ItemType Directory -Force -Path $SpoolDir | Out-Null
    # The spooler (SYSTEM) writes the job file, the user's app moves and
    # deletes it: Authenticated Users modify, SYSTEM full control.
    Invoke-Native 'icacls.exe' @($SpoolDir, '/grant', '*S-1-5-11:(OI)(CI)M', '/grant', '*S-1-5-18:(OI)(CI)F', '/T')
    Write-Log "Spool directory ready: $SpoolDir"

    if (-not (Get-PrinterDriver -Name $DriverName -ErrorAction SilentlyContinue)) {
        Write-Log "Registering driver '$DriverName'"
        try {
            Add-PrinterDriver -Name $DriverName
        } catch {
            Write-Log "Add-PrinterDriver failed ($($_.Exception.Message)); trying the driver store"
            $inf = Get-ChildItem (Join-Path $env:SystemRoot 'System32\DriverStore\FileRepository') -Filter 'prnms*.inf_*' -Directory |
                ForEach-Object { Get-ChildItem $_.FullName -Filter '*.inf' -ErrorAction SilentlyContinue } |
                Where-Object { (Get-Content $_.FullName -Raw -ErrorAction SilentlyContinue) -match [regex]::Escape($DriverName) } |
                Select-Object -First 1 -ExpandProperty FullName
            if (-not $inf) { throw "'$DriverName' not found in the Windows driver store" }
            # pnputil also exits non-zero for "already in the store" / "reboot
            # required"; Add-PrinterDriver below is the real check.
            try { Invoke-Native 'pnputil.exe' @('/add-driver', $inf, '/install') }
            catch { Write-Log $_.Exception.Message }
            Add-PrinterDriver -Name $DriverName
        }
    }

    $legacy = Get-Printer -Name $LegacyPrinter -ErrorAction SilentlyContinue
    if ($legacy -and $legacy.PortName -eq $LegacyPort) {
        Remove-Printer -Name $LegacyPrinter
        Write-Log "Removed legacy printer '$LegacyPrinter'"
        Remove-PortWithRetry $LegacyPort
    }

    if (-not (Get-PrinterPort -Name $PortPath -ErrorAction SilentlyContinue)) {
        Add-PrinterPort -Name $PortPath
        Write-Log "Added port '$PortPath'"
    }

    $printer = Get-Printer -Name $PrinterName -ErrorAction SilentlyContinue
    if (-not $printer) {
        Add-Printer -Name $PrinterName -DriverName $DriverName -PortName $PortPath
        Write-Log "Added printer '$PrinterName'"
    } elseif ($printer.PortName -ne $PortPath) {
        Set-Printer -Name $PrinterName -PortName $PortPath
        Write-Log "Printer '$PrinterName' existed on port '$($printer.PortName)'; moved to '$PortPath'"
    } else {
        Write-Log "Printer '$PrinterName' already installed"
    }

    if (-not (Get-Printer -Name $PrinterName -ErrorAction SilentlyContinue)) {
        throw "printer '$PrinterName' is missing after setup"
    }
}

function Uninstall-DocuraPrinter {
    if (Get-Printer -Name $PrinterName -ErrorAction SilentlyContinue) {
        Remove-Printer -Name $PrinterName
        Write-Log "Removed printer '$PrinterName'"
    }
    Remove-PortWithRetry $PortPath
}

try {
    New-Item -ItemType Directory -Force -Path $BaseDir | Out-Null
    Write-Log "Start (PowerShell $($PSVersionTable.PSVersion), 64-bit process: $([Environment]::Is64BitProcess))"
    if ($Action -eq 'Install') { Install-DocuraPrinter } else { Uninstall-DocuraPrinter }
    Write-Log 'Done'
    exit 0
} catch {
    Write-Log "ERROR: $($_.Exception.Message)"
    exit 1
}
