use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const PRINTER_NAME: &str = "rustpdf";

static PDFA_LEVEL: Mutex<Option<String>> = Mutex::new(None);

/// Set the default PDF/A level applied to virtual-printer output.
/// `level` is one of `"1b" | "2b" | "3b" | "4"`. `None` disables PDF/A.
pub fn set_pdfa_default(level: Option<String>) {
    *PDFA_LEVEL.lock().unwrap() = level;
}

pub fn pdfa_default() -> Option<String> {
    PDFA_LEVEL.lock().unwrap().clone()
}

pub fn spool_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(pd) = std::env::var("ProgramData") {
            return PathBuf::from(pd).join("rustpdf").join("spool");
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rustpdf")
        .join("spool")
}

pub fn output_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rustpdf")
}

pub fn port_path() -> PathBuf {
    spool_dir().join("rustpdf-spool.ps")
}

#[derive(Serialize)]
pub struct VirtualPrinterInfo {
    pub installed: bool,
    pub printer_name: String,
    pub port_path: String,
    pub output_dir: String,
    pub ghostscript_available: bool,
    pub ghostscript_version: Option<String>,
    pub supported: bool,
}

#[derive(Clone, Serialize)]
struct PrintJobReady {
    path: String,
}

#[derive(Clone, Serialize)]
struct PrintJobFailed {
    message: String,
}

pub fn start_spool_watcher(app: AppHandle) {
    let _ = std::fs::create_dir_all(spool_dir());
    let _ = std::fs::create_dir_all(output_dir());

    let app_scan = app.clone();
    std::thread::spawn(move || scan_and_process(&app_scan));

    std::thread::spawn(move || watcher_loop(app));
}

fn watcher_loop(app: AppHandle) {
    use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(tx, notify::Config::default()) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("vprinter: watcher init failed: {e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(&spool_dir(), RecursiveMode::NonRecursive) {
        tracing::warn!("vprinter: watch {} failed: {e}", spool_dir().display());
        return;
    }

    let mut pending = false;
    loop {
        let ev = match rx.recv_timeout(Duration::from_millis(750)) {
            Ok(Ok(ev)) => Some(ev),
            Ok(Err(_)) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if pending {
                    pending = false;
                    scan_and_process(&app);
                }
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if let Some(ev) = ev {
            if matches!(
                ev.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
            ) {
                pending = true;
            }
        }
    }
}

fn scan_and_process(app: &AppHandle) {
    let spool = spool_dir();
    let Ok(entries) = std::fs::read_dir(&spool) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "ps" {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.starts_with("rustpdf-spool") && !name.starts_with("job-") {
            continue;
        }
        if !is_stable(&p) {
            continue;
        }
        process_one(app, &p);
    }
}

fn process_one(app: &AppHandle, ps: &Path) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = spool_dir().join(format!("job-{stamp}.ps.work"));
    if std::fs::rename(ps, &work).is_err() {
        return;
    }
    let out_dir = output_dir();
    let _ = std::fs::create_dir_all(&out_dir);
    let out_pdf = out_dir.join(format!("printed-{stamp}.pdf"));

    let gs = match crate::find_ghostscript() {
        Some(g) => g,
        None => {
            let _ = std::fs::rename(&work, spool_dir().join(format!("failed-nogs-{stamp}.ps")));
            let _ = app.emit(
                "print-job-failed",
                PrintJobFailed {
                    message: "Ghostscript not installed — install it to process print jobs."
                        .to_string(),
                },
            );
            return;
        }
    };

    let mut cmd = std::process::Command::new(&gs);
    cmd.args(["-dBATCH", "-dNOPAUSE", "-dQUIET", "-sDEVICE=pdfwrite"]);
    if let Some(level) = pdfa_default() {
        if let Ok((val, _)) = crate::pdfa_flags_for(&level) {
            cmd.arg(format!("-dPDFA={val}"));
            cmd.arg("-dPDFACompatibilityPolicy=1");
            cmd.arg("-sColorConversionStrategy=RGB");
            cmd.arg("-sProcessColorModel=DeviceRGB");
        }
    }
    cmd.arg(format!("-sOutputFile={}", out_pdf.display()));
    cmd.arg(&work);
    let status = cmd.status();
    match status {
        Ok(s) if s.success() => {
            let _ = std::fs::remove_file(&work);
            tracing::info!("vprinter: job → {}", out_pdf.display());
            let _ = app.emit(
                "print-job-ready",
                PrintJobReady {
                    path: out_pdf.to_string_lossy().to_string(),
                },
            );
        }
        other => {
            let dead = spool_dir().join(format!("failed-{stamp}.ps"));
            let _ = std::fs::rename(&work, &dead);
            let msg = format!("Ghostscript conversion failed: {other:?}");
            tracing::warn!("{msg}");
            let _ = app.emit("print-job-failed", PrintJobFailed { message: msg });
        }
    }
}

fn is_stable(p: &Path) -> bool {
    let s1 = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);
    std::thread::sleep(Duration::from_millis(400));
    let s2 = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);
    s1 > 0 && s1 == s2
}

#[tauri::command]
pub async fn virtual_printer_status() -> VirtualPrinterInfo {
    let supported = cfg!(target_os = "windows");
    let installed = if supported {
        is_printer_installed().await.unwrap_or(false)
    } else {
        false
    };
    let (gs_ok, gs_ver) = match crate::find_ghostscript() {
        Some(p) => {
            let ver = crate::probe_ghostscript_version(Path::new(&p));
            (true, ver)
        }
        None => (false, None),
    };
    VirtualPrinterInfo {
        installed,
        printer_name: PRINTER_NAME.to_string(),
        port_path: port_path().to_string_lossy().to_string(),
        output_dir: output_dir().to_string_lossy().to_string(),
        ghostscript_available: gs_ok,
        ghostscript_version: gs_ver,
        supported,
    }
}

#[tauri::command]
pub async fn install_virtual_printer() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let port = port_path();
        let spool = spool_dir();
        std::fs::create_dir_all(&spool).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(output_dir()).map_err(|e| e.to_string())?;
        let port_s = port.to_string_lossy().replace('\'', "''");
        let spool_s = spool.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference='Stop'; \
             New-Item -ItemType Directory -Force -Path '{spool}' | Out-Null; \
             icacls '{spool}' /grant '*S-1-5-11:(OI)(CI)M' /grant '*S-1-5-18:(OI)(CI)F' /T | Out-Null; \
             if (-not (Get-PrinterDriver -Name 'Microsoft PS Class Driver' -ErrorAction SilentlyContinue)) {{ \
                try {{ Add-PrinterDriver -Name 'Microsoft PS Class Driver' }} \
                catch {{ \
                    $inf = Get-ChildItem 'C:\\Windows\\System32\\DriverStore\\FileRepository' -Filter 'prnms*.inf_amd64_*' -Directory | \
                        ForEach-Object {{ Get-ChildItem $_.FullName -Filter '*.inf' -ErrorAction SilentlyContinue }} | \
                        Where-Object {{ (Get-Content $_.FullName -Raw -ErrorAction SilentlyContinue) -match 'Microsoft PS Class Driver' }} | \
                        Select-Object -First 1 -ExpandProperty FullName; \
                    if ($inf) {{ pnputil /add-driver $inf /install | Out-Null; Add-PrinterDriver -Name 'Microsoft PS Class Driver' }} \
                    else {{ throw 'Microsoft PS Class Driver not found in DriverStore' }} \
                }} \
             }}; \
             if (-not (Get-PrinterPort -Name '{port}' -ErrorAction SilentlyContinue)) {{ Add-PrinterPort -Name '{port}' }}; \
             if (-not (Get-Printer -Name '{name}' -ErrorAction SilentlyContinue)) {{ Add-Printer -Name '{name}' -DriverName 'Microsoft PS Class Driver' -PortName '{port}' }}",
            port = port_s,
            spool = spool_s,
            name = PRINTER_NAME,
        );
        run_powershell_elevated(&script).await?;
        if !is_printer_installed().await.unwrap_or(false) {
            return Err("Printer install ran but 'rustpdf' printer is not present. \
                        Likely cause: 'Microsoft PS Class Driver' could not be registered. \
                        Check Windows Event Log → System for PrintService errors.".to_string());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Virtual printer is only available on Windows".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_virtual_printer() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let port = port_path();
        let port_s = port.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Remove-Printer -Name '{name}' -ErrorAction SilentlyContinue; \
             Remove-PrinterPort -Name '{port}' -ErrorAction SilentlyContinue",
            port = port_s,
            name = PRINTER_NAME,
        );
        run_powershell_elevated(&script).await
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Virtual printer is only available on Windows".to_string())
    }
}

#[tauri::command]
pub async fn open_print_output_folder(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = output_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
async fn is_printer_installed() -> Result<bool, String> {
    let name = PRINTER_NAME;
    let script = format!(
        "if (Get-Printer -Name '{name}' -ErrorAction SilentlyContinue) {{ 'yes' }} else {{ 'no' }}"
    );
    let out = run_powershell_capture(&script).await?;
    Ok(out.trim() == "yes")
}

#[cfg(target_os = "windows")]
async fn run_powershell_capture(script: &str) -> Result<String, String> {
    let script = script.to_string();
    tokio::task::spawn_blocking(move || {
        let enc = encode_ps(&script);
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
                &enc,
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "windows")]
async fn run_powershell_elevated(script: &str) -> Result<(), String> {
    let inner_enc = encode_ps(script);
    // Launcher PS that elevates a hidden powershell to run the encoded script
    let launcher = format!(
        "$p = Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -PassThru -ArgumentList '-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand','{inner_enc}'; \
         if ($p.ExitCode -ne 0) {{ Write-Error (\"exit=\" + $p.ExitCode); exit 1 }}"
    );
    tokio::task::spawn_blocking(move || {
        let enc = encode_ps(&launcher);
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
                &enc,
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(if err.trim().is_empty() {
                "Elevation cancelled or failed".to_string()
            } else {
                err.to_string()
            });
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "windows")]
fn encode_ps(cmd: &str) -> String {
    use base64::Engine;
    let utf16: Vec<u16> = cmd.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for u in utf16 {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}
