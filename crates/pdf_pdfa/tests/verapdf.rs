//! Conformance matrix: every corpus file × every PDF/A level is converted
//! and the output validated with veraPDF (the reference validator).
//!
//! Needs veraPDF staged by `scripts/fetch-verapdf.ps1` (or `VERAPDF`);
//! without it the test prints a notice and passes, so plain
//! `cargo test --workspace` keeps working on machines without Java.
//! Set `PDFA_REQUIRE_VERAPDF=1` (CI) to turn the notice into a failure.
//! Outputs land in `target/pdfa-matrix/<level>/` for inspection.

mod common;

use std::collections::BTreeMap;

use common::{corpus, Expect};
use pdf_pdfa::{convert, Options, PdfaError, ALL_LEVELS};

fn out_root() -> std::path::PathBuf {
    common::workspace_root().join("target").join("pdfa-matrix")
}

#[test]
fn verapdf_matrix() {
    let filter = std::env::var("PDFA_LEVELS").ok();
    let only = std::env::var("PDFA_CASES").ok();
    let raster = common::PdfiumRasterizer::new();
    let cases = corpus();
    let root = out_root();
    let _ = std::fs::remove_dir_all(&root);
    // The inputs too, for visual comparison and the benchmark.
    let src_dir = root.join("source");
    std::fs::create_dir_all(&src_dir).unwrap();
    for c in &cases {
        std::fs::write(src_dir.join(format!("{}.pdf", c.name)), &c.bytes).unwrap();
    }

    // (level, case) -> outcome text
    let mut matrix: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();

    for level in ALL_LEVELS {
        let id = level.id();
        if let Some(f) = &filter {
            if !f.split(',').any(|x| x == id) {
                continue;
            }
        }
        let dir = root.join(&id);
        std::fs::create_dir_all(&dir).unwrap();
        for c in &cases {
            if let Some(o) = &only {
                if !o.split(',').any(|x| x == c.name) {
                    continue;
                }
            }
            let mut opts = Options::new(level).with_rasterizer(raster.clone());
            opts.lang = Some("de-DE".into());
            let res = convert(&c.bytes, &opts);
            let expect_4f_error = c.expect == Expect::AttachmentRequiredFor4f && id == "4f";
            match (res, c.expect) {
                (Ok(out), _) if !expect_4f_error && c.expect != Expect::PasswordRequired => {
                    std::fs::write(dir.join(format!("{}.pdf", c.name)), &out.bytes).unwrap();
                    matrix.insert((id.clone(), c.name.to_string()), "converted".into());
                }
                (Err(PdfaError::PasswordRequired), Expect::PasswordRequired) => {
                    matrix.insert((id.clone(), c.name.to_string()), "refused (password)".into());
                }
                (Err(PdfaError::AttachmentRequired), _) if expect_4f_error => {
                    matrix.insert((id.clone(), c.name.to_string()), "refused (no attachment)".into());
                }
                (other, _) => {
                    let msg = match other {
                        Ok(_) => "converted, but an error was expected".to_string(),
                        Err(e) => e.to_string(),
                    };
                    problems.push(format!("{id} {}: {msg}", c.name));
                    matrix.insert((id.clone(), c.name.to_string()), format!("ERROR {msg}"));
                }
            }
        }
    }

    let Some(vera) = common::verapdf() else {
        let msg = "veraPDF not found (run scripts/fetch-verapdf.ps1 or set VERAPDF); conformance not checked";
        if std::env::var("PDFA_REQUIRE_VERAPDF").is_ok() {
            panic!("{msg}");
        }
        eprintln!("NOTE: {msg}");
        assert!(problems.is_empty(), "conversion problems:\n{}", problems.join("\n"));
        return;
    };

    for level in ALL_LEVELS {
        let id = level.id();
        let dir = root.join(&id);
        if !dir.exists() || std::fs::read_dir(&dir).map(|mut d| d.next().is_none()).unwrap_or(true) {
            continue;
        }
        let report = common::validate_dir(&vera, &dir, &id);
        for (file, failed) in report {
            let case = file.trim_end_matches(".pdf").to_string();
            if failed.is_empty() {
                matrix.insert((id.clone(), case), "PASS".into());
            } else {
                problems.push(format!("{id} {case}: veraPDF failed {}", failed.join(", ")));
                matrix.insert((id.clone(), case), format!("FAIL {}", failed.join(" ")));
            }
        }
    }

    // Matrix table (markdown) for the PR / docs.
    let levels: Vec<String> = ALL_LEVELS.iter().map(|l| l.id()).filter(|l| filter.as_ref().map(|f| f.split(',').any(|x| x == l)).unwrap_or(true)).collect();
    let mut table = String::from("| Datei |");
    for l in &levels {
        table.push_str(&format!(" {l} |"));
    }
    table.push_str("\n|---|");
    for _ in &levels {
        table.push_str("---|");
    }
    table.push('\n');
    for c in &cases {
        if only.as_ref().map(|o| !o.split(',').any(|x| x == c.name)).unwrap_or(false) {
            continue;
        }
        table.push_str(&format!("| {} |", c.name));
        for l in &levels {
            let v = matrix.get(&(l.clone(), c.name.to_string())).map(|s| s.as_str()).unwrap_or("-");
            let cell = match v {
                "PASS" => "✅".to_string(),
                v if v.starts_with("refused") => format!("⛔ {}", v.trim_start_matches("refused ")),
                "converted" => "❔".to_string(),
                v => format!("❌ {v}"),
            };
            table.push_str(&format!(" {cell} |"));
        }
        table.push('\n');
    }
    std::fs::write(root.join("matrix.md"), &table).unwrap();
    println!("{table}");
    assert!(problems.is_empty(), "PDF/A problems:\n{}", problems.join("\n"));
}
