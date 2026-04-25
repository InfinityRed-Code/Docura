; rustpdf installer hooks
; 1) Prompts to download Ghostscript if missing (required for PDF/A + virtual printer).
; 2) Offers to install the "rustpdf" virtual printer (Print-to-PDF).
; Non-blocking: install succeeds either way.

!macro NSIS_HOOK_POSTINSTALL
  Push $0
  Push $1
  Push $2

  ; --- Ghostscript check ---
  StrCpy $0 0
  EnumRegKey $1 HKLM "SOFTWARE\GPL Ghostscript" $0
  ${If} $1 == ""
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "rustpdf uses Ghostscript for PDF/A export and Print-to-PDF.$\r$\n$\r$\nGhostscript was not detected. Open the official download page now?" \
      /SD IDNO IDNO skipGs
    ExecShell "open" "https://ghostscript.com/releases/gsdnld.html"
    skipGs:
  ${EndIf}

  ; --- Virtual printer install prompt ---
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Install the 'rustpdf' virtual printer now?$\r$\n$\r$\nAny Windows app will be able to print to rustpdf and get a PDF." \
    /SD IDNO IDNO skipPrinter

  StrCpy $2 "$PROGRAMDATA\rustpdf\spool\rustpdf-spool.ps"
  CreateDirectory "$PROGRAMDATA\rustpdf\spool"
  nsExec::ExecToLog 'icacls "$PROGRAMDATA\rustpdf\spool" /grant "*S-1-5-11:(OI)(CI)M" /grant "*S-1-5-18:(OI)(CI)F" /T'
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "if (-not (Get-PrinterPort -Name $\"$2$\" -ErrorAction SilentlyContinue)) { Add-PrinterPort -Name $\"$2$\" }; if (-not (Get-Printer -Name $\"rustpdf$\" -ErrorAction SilentlyContinue)) { Add-Printer -Name $\"rustpdf$\" -DriverName $\"Microsoft PS Class Driver$\" -PortName $\"$2$\" }"'
  skipPrinter:

  Pop $2
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Push $2
  StrCpy $2 "$PROGRAMDATA\rustpdf\spool\rustpdf-spool.ps"
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Remove-Printer -Name $\"rustpdf$\" -ErrorAction SilentlyContinue; Remove-PrinterPort -Name $\"$2$\" -ErrorAction SilentlyContinue"'
  Pop $2
!macroend
