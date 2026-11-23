; Docura PDF installer hooks, included by the Tauri NSIS template.
;
; The installer runs per machine (tauri.conf.json: bundle.windows.nsis.
; installMode = "perMachine"), so Windows elevates it up front and every step
; below runs as administrator. The virtual printer needs that: adding a
; printer port / printer is admin-only, and the old per-user installer ran
; without elevation, so the printer step always failed silently (#5).
;
; Install:   remove an old per-user install, ship Tesseract OCR (#6),
;            register the "Docura PDF" virtual printer (#5), offer Ghostscript.
; Update:    the same setup run over an installed copy (installer.nsi, our
;            template, sets $UpdateMode, #4): files are overwritten in place,
;            Tesseract is replaced, the printer setup only checks/repairs the
;            printer (never removes it), Ghostscript is not offered again.
; Uninstall: remove the printer, the Tesseract files and the setup script.

!define DOCURA_PRINTER_NAME "Docura PDF"
; Evaluated here, at include time: inside the macros __FILEDIR__ would be the
; directory of the generated installer.nsi instead of this file's.
!define DOCURA_PRINTER_PS1 "${__FILEDIR__}\printer-setup.ps1"
!define DOCURA_TESSERACT_SRC "${__FILEDIR__}\..\..\..\..\vendor\tesseract"

; An installer without Tesseract would ship OCR broken again (#6): refuse to
; build one.
!if ! /FileExists "${DOCURA_TESSERACT_SRC}\tesseract.exe"
  !error "Tesseract is not staged in vendor\tesseract. Run 'just tesseract' (scripts\fetch-tesseract.ps1) before building the installer."
!endif

; ------------------------------------------------------------------ helpers

; Run printer-setup.ps1 (installed to $INSTDIR) with -Action ACTION and leave
; its exit code in RESULT ("error" if PowerShell could not be started). The
; script logs to %ProgramData%\DocuraPDF\printer-setup.log; its stdout lands
; in the installer's details list.
!macro DocuraRunPrinterSetup ACTION RESULT
  !insertmacro DocuraPowerShellPath ${RESULT}
  nsExec::ExecToLog '"${RESULT}" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\printer-setup.ps1" -Action ${ACTION}'
  Pop ${RESULT}
!macroend

!macro DocuraPowerShellPath RESULT
  ${If} ${RunningX64}
    ; The installer is a 32-bit process. Sysnative reaches the 64-bit
    ; PowerShell, which sees the real System32 (pnputil, driver store).
    StrCpy ${RESULT} "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${Else}
    StrCpy ${RESULT} "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
!macroend

; The template closes desktop-tauri.exe before it touches any file, but an OCR
; run the app started can outlive it and keep tesseract.exe and its DLLs in
; $INSTDIR\tesseract open: they could then be neither deleted nor overwritten,
; and the update would mix two Tesseract builds. Stop those processes; a
; Tesseract installed elsewhere on the machine is left alone.
!macro DocuraStopBundledTesseract
  Push $R0
  nsis_tauri_utils::FindProcess "tesseract.exe"
  Pop $R0
  ${If} $R0 = 0
    DetailPrint "Stopping Tesseract processes started from $INSTDIR\tesseract"
    !insertmacro DocuraPowerShellPath $R0
    nsExec::ExecToLog `"$R0" -NoProfile -NonInteractive -Command "Get-Process tesseract -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\tesseract\*' } | ForEach-Object { $$_.Kill(); [void]$$_.WaitForExit(5000) }"`
    Pop $R0
  ${EndIf}
  Pop $R0
!macroend

; Installers up to 1.3.0 installed per user (%LOCALAPPDATA%\Docura PDF,
; HKCU). A per-machine install would leave that copy behind, and its HKCU
; .pdf association would keep opening the old exe. Remove it with its own
; uninstaller (silent, app data is kept) and move an enabled autostart entry,
; which that uninstaller deletes, over to the new location.
!macro DocuraRemovePerUserInstall
  Push $R0
  Push $R1
  Push $R2
  Push $R3
  ReadRegStr $R0 HKCU "${UNINSTKEY}" "UninstallString"
  ${If} $R0 != ""
    ; Stored quoted: "C:\...\uninstall.exe"
    StrCpy $R1 $R0 1
    ${If} $R1 == '"'
      StrCpy $R0 $R0 -1 1
    ${EndIf}
    ${GetParent} $R0 $R1
    ${If} ${FileExists} "$R0"
      ReadRegStr $R3 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
      DetailPrint "Removing the previous per-user installation in $R1"
      ClearErrors
      ExecWait '"$R0" /S _?=$R1' $R2
      ${If} ${Errors}
      ${OrIf} $R2 <> 0
        DetailPrint "Warning: the previous per-user installation could not be removed (exit code $R2). Remove it under Settings > Apps."
      ${Else}
        ; With _?= the uninstaller cannot delete itself.
        Delete "$R0"
        RMDir "$R1"
        ${If} $R3 != ""
          ${WordReplace} "$R3" "$R1" "$INSTDIR" "+" $R3
          WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}" "$R3"
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
!macroend

; Tesseract OCR (binary, runtime DLLs, deu + eng traineddata, licenses) into
; $INSTDIR\tesseract, where the app looks first (ocr.rs). Wiped first so an
; update never mixes DLLs of two Tesseract builds.
!macro DocuraInstallTesseract
  DetailPrint "Installing Tesseract OCR"
  !insertmacro DocuraStopBundledTesseract
  RMDir /r "$INSTDIR\tesseract"
  SetOutPath "$INSTDIR\tesseract"
  File /r "${DOCURA_TESSERACT_SRC}\*.*"
  SetOutPath "$INSTDIR"
!macroend

!macro DocuraInstallPrinter
  Push $R0
  Push $R1
  DetailPrint "Installing the virtual printer '${DOCURA_PRINTER_NAME}'"
  File "/oname=$INSTDIR\printer-setup.ps1" "${DOCURA_PRINTER_PS1}"
  !insertmacro DocuraRunPrinterSetup Install $R0
  ${If} $R0 != 0
    ReadEnvStr $R1 "ProgramData"
    DetailPrint "Virtual printer setup failed (exit code $R0). Log: $R1\DocuraPDF\printer-setup.log"
    MessageBox MB_OK|MB_ICONEXCLAMATION "The virtual printer '${DOCURA_PRINTER_NAME}' could not be installed (exit code $R0).$\r$\n$\r$\nDetails: $R1\DocuraPDF\printer-setup.log$\r$\n$\r$\nDocura PDF itself is installed. You can retry later in the app under Settings > Virtual printer > Install printer." /SD IDOK
  ${Else}
    DetailPrint "Virtual printer '${DOCURA_PRINTER_NAME}' installed"
  ${EndIf}
  Pop $R1
  Pop $R0
!macroend

; Ghostscript converts the virtual printer's PostScript output (and PDF/A
; exports). It is not bundled; offer the download page if it is missing.
!macro DocuraOfferGhostscript
  Push $R0
  EnumRegKey $R0 HKLM "SOFTWARE\GPL Ghostscript" 0
  ${If} $R0 == ""
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "Docura PDF uses Ghostscript for PDF/A export and for the virtual printer.$\r$\n$\r$\nGhostscript was not detected. Open the official download page now?" \
      /SD IDNO IDNO docura_skip_gs
    ExecShell "open" "https://ghostscript.com/releases/gsdnld.html"
    docura_skip_gs:
  ${EndIf}
  Pop $R0
!macroend

; -------------------------------------------------------------- Tauri hooks

!macro NSIS_HOOK_PREINSTALL
  ${If} $UpdateMode = 1
    DetailPrint "Updating ${PRODUCTNAME} $InstalledVersion in $INSTDIR to ${VERSION} in place"
  ${EndIf}
  ; Only a per-machine install replaces a per-user one; built per user (e.g.
  ; to test without elevation) this would uninstall the copy being updated.
  !if "${INSTALLMODE}" == "perMachine"
    !insertmacro DocuraRemovePerUserInstall
  !endif
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro DocuraInstallTesseract
  ; Install or, on an update, check and repair (printer-setup.ps1 is
  ; idempotent and never removes a working printer).
  !insertmacro DocuraInstallPrinter
  ; Asked once, at the first installation.
  ${If} $UpdateMode <> 1
    !insertmacro DocuraOfferGhostscript
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Push $R0
  ; Keeps the printer when run with /UPDATE. The installer itself never runs
  ; the uninstaller for an update (it updates in place, see installer.nsi), so
  ; this only guards a manual "uninstall.exe /UPDATE".
  ${If} $UpdateMode <> 1
  ${AndIf} ${FileExists} "$INSTDIR\printer-setup.ps1"
    DetailPrint "Removing the virtual printer '${DOCURA_PRINTER_NAME}'"
    !insertmacro DocuraRunPrinterSetup Uninstall $R0
    ${If} $R0 != 0
      DetailPrint "Removing the virtual printer failed (exit code $R0), see printer-setup.log in %ProgramData%\DocuraPDF"
      MessageBox MB_OK|MB_ICONEXCLAMATION "The virtual printer '${DOCURA_PRINTER_NAME}' could not be removed (exit code $R0).$\r$\n$\r$\nDetails: %ProgramData%\DocuraPDF\printer-setup.log$\r$\n$\r$\nRemove it under Settings > Bluetooth & devices > Printers & scanners." /SD IDOK
    ${EndIf}
  ${EndIf}
  Pop $R0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Not tracked by the Tauri bundler, so its RMDir "$INSTDIR" left the
  ; directory in place.
  !insertmacro DocuraStopBundledTesseract
  RMDir /r "$INSTDIR\tesseract"
  Delete "$INSTDIR\printer-setup.ps1"
  RMDir "$INSTDIR"
!macroend
