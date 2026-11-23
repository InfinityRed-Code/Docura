//! Checks on the NSIS installer configuration (no installer is built here).

const TAURI_CONF: &str = include_str!("../tauri.conf.json");
const TEMPLATE: &str = include_str!("../nsis/installer.nsi");
const UI_LOCKFILE: &str = include_str!("../../ui/pnpm-lock.yaml");

fn tauri_conf() -> serde_json::Value {
    serde_json::from_str(TAURI_CONF).expect("tauri.conf.json parses")
}

/// The installer registers this version (Apps list, `DisplayVersion`) and
/// compares the next setup against it to decide between update, repair and
/// downgrade, so it must be the version the app itself reports.
#[test]
fn bundle_version_matches_workspace_version() {
    assert_eq!(
        tauri_conf()["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "tauri.conf.json \"version\" and the workspace version in Cargo.toml must be bumped together"
    );
}

/// nsis/installer.nsi is a copy of the Tauri CLI's NSIS template with the
/// in-place update changes (#4). A CLI bump must bring the new upstream
/// template along, otherwise the installer silently keeps the old one.
#[test]
fn nsis_template_matches_tauri_cli_version() {
    let template_version = TEMPLATE
        .lines()
        .find_map(|l| l.strip_prefix("; Copy of the Tauri NSIS template of tauri-cli v"))
        .map(str::trim)
        .expect("installer.nsi names the tauri-cli version it is based on");

    let mut lines = UI_LOCKFILE.lines().map(str::trim);
    lines
        .by_ref()
        .find(|l| *l == "'@tauri-apps/cli':")
        .expect("pnpm-lock.yaml lists @tauri-apps/cli");
    let cli_version = lines
        .find_map(|l| l.strip_prefix("version: "))
        .expect("pnpm-lock.yaml pins the @tauri-apps/cli version");

    assert_eq!(
        template_version, cli_version,
        "@tauri-apps/cli changed: re-apply the \"Docura:\" blocks of nsis/installer.nsi to the new upstream template \
         (crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi) and update its version line"
    );
}

#[test]
fn installer_uses_the_update_template() {
    let nsis = &tauri_conf()["bundle"]["windows"]["nsis"];
    assert_eq!(nsis["template"].as_str(), Some("nsis/installer.nsi"));
    assert_eq!(nsis["installerHooks"].as_str(), Some("nsis/hooks.nsi"));
    // The in-place update: detected in .onInit, no "uninstall first" page.
    assert!(TEMPLATE.contains("StrCpy $UpdateMode 1"));
    assert!(TEMPLATE.contains("${IfThen} $UpdateMode = 1 ${|} Abort ${|}"));
}
