//! Benchmark: built-in PDF/A converter against Ghostscript.
//!
//! ```text
//! cargo run --release -p pdf_pdfa --example pdfa_bench -- [options] <dir|file.pdf>...
//!
//!   --corpus          also benchmark the synthetic test corpus
//!   --levels 1b,2b    levels to convert to (default: 1b,2b,3b, the ones
//!                     Ghostscript can write)
//!   --runs N          conversions per file and engine, the median counts (default 3)
//!   --gs <exe>        Ghostscript executable (default: GHOSTSCRIPT, PATH,
//!                     C:\Program Files\gs\*\bin)
//!   --no-gs           built-in converter only
//!   --no-verapdf      skip validation
//!   --out <dir>       output directory (default: target/pdfa-bench)
//! ```
//!
//! For every input and level it measures conversion time and output size of
//! both engines and validates both outputs with veraPDF. Ghostscript is
//! called exactly as the app calls it. Results: a summary on stdout and
//! `summary.md`, `files.md`, `gs-matrix.md` in the output directory.
//! Nothing is written next to the inputs.

#[path = "../tests/common/mod.rs"]
mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pdf_pdfa::{convert, Level, Options};

struct Args {
    inputs: Vec<PathBuf>,
    corpus: bool,
    levels: Vec<String>,
    runs: usize,
    gs: Option<PathBuf>,
    use_gs: bool,
    use_vera: bool,
    out: PathBuf,
}

fn parse_args() -> Args {
    let mut a = Args {
        inputs: Vec::new(),
        corpus: false,
        levels: vec!["1b".into(), "2b".into(), "3b".into()],
        runs: 3,
        gs: None,
        use_gs: true,
        use_vera: true,
        out: common::workspace_root().join("target").join("pdfa-bench"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--corpus" => a.corpus = true,
            "--levels" => a.levels = it.next().unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "--runs" => a.runs = it.next().and_then(|s| s.parse().ok()).unwrap_or(3).max(1),
            "--gs" => a.gs = it.next().map(PathBuf::from),
            "--no-gs" => a.use_gs = false,
            "--no-verapdf" => a.use_vera = false,
            "--out" => a.out = it.next().map(PathBuf::from).unwrap_or(a.out),
            "-h" | "--help" => {
                println!("usage: pdfa_bench [--corpus] [--levels 1b,2b,3b] [--runs N] [--gs <exe>] [--no-gs] [--no-verapdf] [--out <dir>] <dir|file.pdf>...");
                std::process::exit(0);
            }
            _ => a.inputs.push(PathBuf::from(x)),
        }
    }
    if a.inputs.is_empty() && !a.corpus {
        eprintln!("no inputs: pass a directory, PDF files or --corpus");
        std::process::exit(2);
    }
    a
}

fn find_gs() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("GHOSTSCRIPT") {
        cands.push(PathBuf::from(p));
    }
    if cfg!(windows) {
        cands.push("gswin64c.exe".into());
        for base in [r"C:\Program Files\gs", r"C:\Program Files (x86)\gs"] {
            if let Ok(dir) = std::fs::read_dir(base) {
                for e in dir.flatten() {
                    cands.push(e.path().join("bin").join("gswin64c.exe"));
                }
            }
        }
    } else {
        cands.push("gs".into());
    }
    cands.into_iter().find(|c| std::process::Command::new(c).arg("--version").output().map(|o| o.status.success()).unwrap_or(false))
}

fn collect_pdfs(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(p).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        entries.sort();
        for e in entries {
            collect_pdfs(&e, out);
        }
    } else if p.extension().map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false) {
        out.push(p.to_path_buf());
    }
}

/// The Ghostscript call of the app (`save_as_pdfa` in the Tauri backend).
fn run_gs(gs: &Path, pdfa: &str, src: &Path, dst: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(dst);
    let out = std::process::Command::new(gs)
        .args(["-dBATCH", "-dNOPAUSE", "-dQUIET", "-sDEVICE=pdfwrite"])
        .arg(format!("-dPDFA={pdfa}"))
        .arg("-dPDFACompatibilityPolicy=1")
        .arg("-sColorConversionStrategy=RGB")
        .arg("-sProcessColorModel=DeviceRGB")
        .arg(format!("-sOutputFile={}", dst.to_string_lossy().replace('%', "%%")))
        .arg(src)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() || !dst.exists() {
        let _ = std::fs::remove_file(dst);
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_string();
        return Err(line.chars().take(80).collect());
    }
    Ok(())
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[derive(Default, Clone)]
struct Cell {
    time: Option<Duration>,
    size: Option<u64>,
    error: Option<String>,
    /// `Some(empty)` = veraPDF pass.
    failed: Option<Vec<String>>,
}

impl Cell {
    fn pass(&self) -> bool {
        self.failed.as_ref().map(|f| f.is_empty()).unwrap_or(false)
    }
}

fn ms(d: Duration) -> String {
    format!("{:.0}", d.as_secs_f64() * 1000.0)
}

fn kb(b: u64) -> String {
    format!("{:.1}", b as f64 / 1024.0)
}

fn main() {
    let args = parse_args();
    let gs = if args.use_gs { args.gs.clone().or_else(find_gs) } else { None };
    if args.use_gs && gs.is_none() {
        eprintln!("NOTE: Ghostscript not found; only the built-in converter is measured");
    }
    let vera = if args.use_vera { common::verapdf() } else { None };
    if args.use_vera && vera.is_none() {
        eprintln!("NOTE: veraPDF not found (scripts/fetch-verapdf.ps1); outputs are not validated");
    }
    let _ = std::fs::remove_dir_all(&args.out);
    std::fs::create_dir_all(&args.out).expect("output directory");

    // (display name, path)
    let mut inputs: Vec<(String, PathBuf)> = Vec::new();
    if args.corpus {
        let dir = args.out.join("source");
        std::fs::create_dir_all(&dir).unwrap();
        for c in common::corpus() {
            let p = dir.join(format!("{}.pdf", c.name));
            std::fs::write(&p, &c.bytes).unwrap();
            inputs.push((c.name.to_string(), p));
        }
    }
    let mut files = Vec::new();
    for i in &args.inputs {
        collect_pdfs(i, &mut files);
    }
    for (n, f) in files.iter().enumerate() {
        // Stable, unique and free of customer file names in reports.
        let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        inputs.push((format!("{:03}_{}", n + 1, stem), f.clone()));
    }
    let raster = common::PdfiumRasterizer::new();

    // (level, engine, file) -> cell; engine: "builtin" | "gs"
    let mut cells: BTreeMap<(String, &'static str, String), Cell> = BTreeMap::new();
    for level in &args.levels {
        let lvl: Level = match level.parse() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("skipping level {level}: {e}");
                continue;
            }
        };
        let own_dir = args.out.join(level).join("builtin");
        let gs_dir = args.out.join(level).join("gs");
        std::fs::create_dir_all(&own_dir).unwrap();
        std::fs::create_dir_all(&gs_dir).unwrap();
        let gs_flag = match level.as_str() {
            "1b" => Some("1"),
            "2b" => Some("2"),
            "3b" => Some("3"),
            _ => None,
        };
        for (name, path) in &inputs {
            let Ok(bytes) = std::fs::read(path) else { continue };
            // Built-in.
            let mut cell = Cell::default();
            let mut times = Vec::new();
            for _ in 0..args.runs {
                let opts = Options::new(lvl).with_rasterizer(raster.clone());
                let t = Instant::now();
                let res = convert(&bytes, &opts);
                let dt = t.elapsed();
                match res {
                    Ok(out) => {
                        times.push(dt);
                        cell.size = Some(out.bytes.len() as u64);
                        std::fs::write(own_dir.join(format!("{name}.pdf")), &out.bytes).unwrap();
                    }
                    Err(e) => {
                        cell.error = Some(e.to_string().chars().take(80).collect());
                        break;
                    }
                }
            }
            if cell.error.is_none() {
                cell.time = Some(median(times));
            }
            cells.insert((level.clone(), "builtin", name.clone()), cell);
            // Ghostscript.
            if let (Some(gs), Some(flag)) = (&gs, gs_flag) {
                let mut cell = Cell::default();
                let dst = gs_dir.join(format!("{name}.pdf"));
                let mut times = Vec::new();
                for _ in 0..args.runs {
                    let t = Instant::now();
                    let res = run_gs(gs, flag, path, &dst);
                    let dt = t.elapsed();
                    match res {
                        Ok(()) => times.push(dt),
                        Err(e) => {
                            cell.error = Some(e);
                            break;
                        }
                    }
                }
                if cell.error.is_none() {
                    cell.time = Some(median(times));
                    cell.size = std::fs::metadata(&dst).ok().map(|m| m.len());
                }
                cells.insert((level.clone(), "gs", name.clone()), cell);
            }
            eprint!(".");
        }
        eprintln!(" {level}");
        if let Some(vera) = &vera {
            for (engine, dir) in [("builtin", &own_dir), ("gs", &gs_dir)] {
                if std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(true) {
                    continue;
                }
                for (file, failed) in common::validate_dir(vera, dir, level) {
                    let name = file.trim_end_matches(".pdf").to_string();
                    if let Some(c) = cells.get_mut(&(level.clone(), engine, name)) {
                        c.failed = Some(failed);
                    }
                }
            }
        }
    }

    // ---- summary per level
    let mut summary = String::from("| Stufe | Dateien | Konverter | konvertiert | veraPDF PASS | Zeit gesamt (ms) | Größe gesamt (KiB) |\n|---|---|---|---|---|---|---|\n");
    // Fair totals: only files both engines converted.
    let mut fair = String::from("| Stufe | Dateien (beide konvertiert) | Zeit intern (ms) | Zeit Ghostscript (ms) | Faktor | Größe intern (KiB) | Größe Ghostscript (KiB) |\n|---|---|---|---|---|---|---|\n");
    for level in &args.levels {
        for engine in ["builtin", "gs"] {
            let mine: Vec<&Cell> = inputs.iter().filter_map(|(n, _)| cells.get(&(level.clone(), engine, n.clone()))).collect();
            if mine.is_empty() {
                continue;
            }
            let ok = mine.iter().filter(|c| c.error.is_none()).count();
            let pass = mine.iter().filter(|c| c.pass()).count();
            let time: Duration = mine.iter().filter_map(|c| c.time).sum();
            let size: u64 = mine.iter().filter_map(|c| c.size).sum();
            let vp = if vera.is_some() { pass.to_string() } else { "–".into() };
            summary.push_str(&format!(
                "| {level} | {} | {} | {ok} | {vp} | {} | {} |\n",
                mine.len(),
                if engine == "builtin" { "intern" } else { "Ghostscript" },
                ms(time),
                kb(size)
            ));
        }
        let both: Vec<(&Cell, &Cell)> = inputs
            .iter()
            .filter_map(|(n, _)| Some((cells.get(&(level.clone(), "builtin", n.clone()))?, cells.get(&(level.clone(), "gs", n.clone()))?)))
            .filter(|(a, b)| a.error.is_none() && b.error.is_none())
            .collect();
        if !both.is_empty() {
            let ta: Duration = both.iter().filter_map(|(a, _)| a.time).sum();
            let tb: Duration = both.iter().filter_map(|(_, b)| b.time).sum();
            let sa: u64 = both.iter().filter_map(|(a, _)| a.size).sum();
            let sb: u64 = both.iter().filter_map(|(_, b)| b.size).sum();
            let factor = if ta.as_secs_f64() > 0.0 { format!("{:.1}×", tb.as_secs_f64() / ta.as_secs_f64()) } else { "–".into() };
            fair.push_str(&format!("| {level} | {} | {} | {} | {factor} | {} | {} |\n", both.len(), ms(ta), ms(tb), kb(sa), kb(sb)));
        }
    }

    // ---- per file
    let mut detail = String::from("| Datei | Stufe | Eingabe (KiB) | intern ms | intern KiB | intern veraPDF | GS ms | GS KiB | GS veraPDF |\n|---|---|---|---|---|---|---|---|---|\n");
    let verdict = |c: Option<&Cell>| -> String {
        match c {
            None => "–".into(),
            Some(c) => match (&c.error, &c.failed) {
                (Some(e), _) => format!("Fehler: {e}"),
                (None, Some(f)) if f.is_empty() => "✅".into(),
                (None, Some(f)) => format!("❌ {}", f.join(" ")),
                (None, None) => "❔".into(),
            },
        }
    };
    for (name, path) in &inputs {
        let insize = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        for level in &args.levels {
            let a = cells.get(&(level.clone(), "builtin", name.clone()));
            let b = cells.get(&(level.clone(), "gs", name.clone()));
            let t = |c: Option<&Cell>| c.and_then(|c| c.time).map(ms).unwrap_or_else(|| "–".into());
            let s = |c: Option<&Cell>| c.and_then(|c| c.size).map(kb).unwrap_or_else(|| "–".into());
            detail.push_str(&format!("| {name} | {level} | {} | {} | {} | {} | {} | {} | {} |\n", kb(insize), t(a), s(a), verdict(a), t(b), s(b), verdict(b)));
        }
    }

    // ---- Ghostscript conformance matrix (file × level)
    let gs_levels: Vec<&String> = args.levels.iter().filter(|l| cells.keys().any(|k| &k.0 == *l && k.1 == "gs")).collect();
    let mut matrix = String::from("| Datei (Ghostscript-Ausgabe) |");
    for l in &gs_levels {
        matrix.push_str(&format!(" {l} |"));
    }
    matrix.push_str("\n|---|");
    for _ in &gs_levels {
        matrix.push_str("---|");
    }
    matrix.push('\n');
    for (name, _) in &inputs {
        matrix.push_str(&format!("| {name} |"));
        for l in &gs_levels {
            matrix.push_str(&format!(" {} |", verdict(cells.get(&((*l).clone(), "gs", name.clone())))));
        }
        matrix.push('\n');
    }

    std::fs::write(args.out.join("summary.md"), format!("{summary}\n{fair}")).unwrap();
    std::fs::write(args.out.join("files.md"), &detail).unwrap();
    std::fs::write(args.out.join("gs-matrix.md"), &matrix).unwrap();
    println!("{summary}\n{fair}");
    println!("details: {}", args.out.display());
}
