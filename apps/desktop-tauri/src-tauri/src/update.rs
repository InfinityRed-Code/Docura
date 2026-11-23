//! Update hint (#18): asks GitHub for the latest release, at most once a day
//! at startup and whenever the user clicks "Check for updates" in the
//! settings. A newer version is offered for download in the browser; the
//! setup updates the installed copy in place (#4).
//!
//! Nothing but the request itself leaves the machine: no identifiers, no
//! version or usage data (the User-Agent GitHub requires is a fixed string).
//! Without a network, or when GitHub answers with an error, the automatic
//! check stays silent.

use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{load_settings, save_settings, AppState};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/InfinityRed-Code/Docura/releases/latest";
/// Fallback when the response has no usable release or asset link.
const RELEASES_PAGE: &str = "https://github.com/InfinityRed-Code/Docura/releases/latest";
/// Only links on GitHub itself are opened from a release response.
const ALLOWED_LINK_PREFIX: &str = "https://github.com/";
const USER_AGENT: &str = "Docura-PDF-update-check";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const NOTES_MAX_CHARS: usize = 300;

/// The fields of GitHub's release object the hint needs.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// What the UI shows: in the hint when `available`, in the settings always.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    /// First paragraph of the release notes as plain text, shortened.
    pub notes: String,
    /// The release's setup, or the release page when it has none.
    pub download_url: String,
    pub release_url: String,
}

/// `v1.3.2`, `1.3.2` and `1.4.0-beta.1` parse; anything else is `None`.
pub(crate) fn parse_version(s: &str) -> Option<semver::Version> {
    let s = s.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    semver::Version::parse(s).ok()
}

/// Turn a `releases/latest` response into the hint for `current_version`.
pub(crate) fn evaluate_release(json: &str, current_version: &str) -> Result<UpdateInfo, String> {
    let release: GithubRelease =
        serde_json::from_str(json).map_err(|e| format!("unexpected release response: {e}"))?;
    let latest = parse_version(&release.tag_name)
        .ok_or_else(|| format!("release tag is not a version: {}", release.tag_name))?;
    let current = parse_version(current_version)
        .ok_or_else(|| format!("app version is not a version: {current_version}"))?;

    let release_url = github_link(&release.html_url)
        .unwrap_or(RELEASES_PAGE)
        .to_string();
    let download_url = setup_asset_url(&release.assets)
        .unwrap_or(&release_url)
        .to_string();
    Ok(UpdateInfo {
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        // `latest` never returns drafts or pre-releases; ignore them should it.
        available: !release.draft
            && !release.prerelease
            && latest.cmp_precedence(&current) == std::cmp::Ordering::Greater,
        notes: summarize_notes(release.body.as_deref().unwrap_or(""), NOTES_MAX_CHARS),
        download_url,
        release_url,
    })
}

fn github_link(url: &str) -> Option<&str> {
    let url = url.trim();
    url.starts_with(ALLOWED_LINK_PREFIX).then_some(url)
}

/// The Windows setup among the release assets (`Docura.PDF_<v>_x64-setup.exe`).
fn setup_asset_url(assets: &[GithubAsset]) -> Option<&str> {
    let is_setup = |suffix: &str| {
        assets.iter().find_map(|a| {
            let name = a.name.to_ascii_lowercase();
            if name.ends_with(suffix) {
                github_link(&a.browser_download_url)
            } else {
                None
            }
        })
    };
    is_setup("_x64-setup.exe").or_else(|| is_setup("-setup.exe"))
}

/// The first paragraph of the release notes (the changelog section) as
/// plain text, cut at a word boundary after at most `max_chars` characters.
pub(crate) fn summarize_notes(body: &str, max_chars: usize) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            if lines.is_empty() {
                continue;
            }
            break;
        }
        let line = ["- ", "* ", "+ "]
            .iter()
            .find_map(|b| line.strip_prefix(*b))
            .unwrap_or(line);
        lines.push(line);
    }
    let link = Regex::new(r"!?\[([^\]]*)\]\([^)]*\)").expect("valid regex");
    let joined = lines.join(" ");
    let text = link.replace_all(&joined, "$1");
    let text = text.replace("**", "").replace("__", "").replace('`', "");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max_chars {
        return text;
    }
    let cut: String = text.chars().take(max_chars).collect();
    let cut = match cut.rfind(' ') {
        Some(i) => &cut[..i],
        None => cut.as_str(),
    };
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ']))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The installed version. Debug builds accept `DOCURA_FAKE_VERSION` to try
/// the hint with an older version.
fn current_version(app: &tauri::AppHandle) -> String {
    #[cfg(debug_assertions)]
    if let Ok(v) = std::env::var("DOCURA_FAKE_VERSION") {
        if !v.trim().is_empty() {
            return v;
        }
    }
    app.package_info().version.to_string()
}

/// Debug builds accept `DOCURA_UPDATE_URL` to point the check at a mock.
fn release_endpoint() -> String {
    #[cfg(debug_assertions)]
    if let Ok(u) = std::env::var("DOCURA_UPDATE_URL") {
        if !u.trim().is_empty() {
            return u;
        }
    }
    LATEST_RELEASE_API.to_string()
}

fn fetch_latest_release(url: &str) -> Result<String, String> {
    let resp = ureq::get(url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?;
    let mut json = String::new();
    resp.into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut json)
        .map_err(|e| e.to_string())?;
    Ok(json)
}

fn settings_path(state: &tauri::State<'_, AppState>) -> Option<PathBuf> {
    state.settings_path.lock().clone()
}

/// Check GitHub for a newer release.
///
/// `manual = false` (startup): only when enabled and the last check is a day
/// old; returns a hint only for a newer version the user has not closed, and
/// stays silent on any error. `manual = true` (settings button): always
/// checks, returns the result even without an update, and reports errors.
#[tauri::command]
pub async fn check_for_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    manual: bool,
) -> Result<Option<UpdateInfo>, String> {
    let path = settings_path(&state);
    let mut dismissed = None;
    if !manual {
        let Some(path) = path.as_ref() else {
            return Ok(None);
        };
        let mut s = load_settings(path);
        if !s.update_check.unwrap_or(true) {
            return Ok(None);
        }
        let now = now_secs();
        // A timestamp in the future (clock was wrong) counts as due.
        if let Some(last) = s.update_last_check {
            if last <= now && now - last < CHECK_INTERVAL.as_secs() {
                return Ok(None);
            }
        }
        // Counted as soon as it is attempted: at most one request a day,
        // also while offline.
        s.update_last_check = Some(now);
        let _ = save_settings(path, &s);
        dismissed = s.update_dismissed;
    }

    let current = current_version(&app);
    let url = release_endpoint();
    let result = tokio::task::spawn_blocking(move || {
        fetch_latest_release(&url).and_then(|json| evaluate_release(&json, &current))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    match result {
        Ok(info) if manual => Ok(Some(info)),
        Ok(info) => {
            let closed = dismissed.as_deref() == Some(info.latest_version.as_str());
            Ok((info.available && !closed).then_some(info))
        }
        Err(e) if manual => Err(format!("UPDATE_CHECK_FAILED:{e}")),
        Err(e) => {
            tracing::info!("update check failed (ignored): {e}");
            Ok(None)
        }
    }
}

/// Settings section: whether the startup check is on, and the version.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    enabled: bool,
    current_version: String,
}

#[tauri::command]
pub fn get_update_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> UpdateSettings {
    let enabled = settings_path(&state)
        .map(|p| load_settings(&p).update_check.unwrap_or(true))
        .unwrap_or(true);
    UpdateSettings {
        enabled,
        current_version: current_version(&app),
    }
}

#[tauri::command]
pub fn set_update_check(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let path = settings_path(&state).ok_or("settings path unavailable")?;
    let mut s = load_settings(&path);
    s.update_check = Some(enabled);
    save_settings(&path, &s).map_err(|e| e.to_string())
}

/// The user closed the hint for `version`: do not show it again at startup
/// (a newer version, or the settings button, still does).
#[tauri::command]
pub fn dismiss_update(state: tauri::State<'_, AppState>, version: String) -> Result<(), String> {
    let path = settings_path(&state).ok_or("settings path unavailable")?;
    let mut s = load_settings(&path);
    s.update_dismissed = Some(version);
    save_settings(&path, &s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real `releases/latest` response (v1.3.1), body and fields trimmed.
    const FIXTURE: &str = include_str!("../tests/fixtures/github-latest-release.json");

    #[test]
    fn parses_versions_with_and_without_v() {
        assert_eq!(parse_version("v1.3.1"), Some(semver::Version::new(1, 3, 1)));
        assert_eq!(
            parse_version(" 1.3.10 "),
            Some(semver::Version::new(1, 3, 10))
        );
        assert!(parse_version("1.4.0-beta.1").is_some());
        assert_eq!(parse_version("1.3"), None);
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn compares_versions_numerically() {
        let newer = |latest: &str, current: &str| {
            parse_version(latest)
                .unwrap()
                .cmp_precedence(&parse_version(current).unwrap())
                == std::cmp::Ordering::Greater
        };
        assert!(newer("1.3.10", "1.3.9"));
        assert!(newer("1.4.0", "1.3.99"));
        assert!(newer("2.0.0", "1.99.99"));
        assert!(newer("1.4.0", "1.4.0-beta.2"));
        assert!(!newer("1.3.1", "1.3.1"));
        assert!(!newer("1.3.0", "1.3.1"));
        assert!(!newer("1.4.0-beta.1", "1.4.0"));
        assert!(!newer("1.3.1+build.7", "1.3.1"));
    }

    #[test]
    fn real_response_with_older_app_offers_the_setup() {
        let info = evaluate_release(FIXTURE, "1.3.0").unwrap();
        assert_eq!(
            info,
            UpdateInfo {
                current_version: "1.3.0".into(),
                latest_version: "1.3.1".into(),
                available: true,
                notes: "Edit cleanup (#8): more shapes, inline text format, one color picker, \
                        more fonts. Installer: virtual printer set up during installation (#5), \
                        Tesseract bundled for OCR (#6), per-machine install. Text boxes (#11), \
                        stamps, signatures and text marks (#13) stay upright on pages with /Rotate."
                    .into(),
                download_url: "https://github.com/InfinityRed-Code/Docura/releases/download/v1.3.1/Docura.PDF_1.3.1_x64-setup.exe".into(),
                release_url: "https://github.com/InfinityRed-Code/Docura/releases/tag/v1.3.1".into(),
            }
        );
    }

    #[test]
    fn real_response_with_same_or_newer_app_offers_nothing() {
        assert!(!evaluate_release(FIXTURE, "1.3.1").unwrap().available);
        assert!(!evaluate_release(FIXTURE, "1.4.0").unwrap().available);
    }

    #[test]
    fn falls_back_to_the_release_page_without_a_setup() {
        let json = r#"{"tag_name":"v2.0.0","html_url":"https://github.com/InfinityRed-Code/Docura/releases/tag/v2.0.0",
            "assets":[{"name":"sources.zip","browser_download_url":"https://github.com/x/sources.zip"}]}"#;
        let info = evaluate_release(json, "1.3.1").unwrap();
        assert!(info.available);
        assert_eq!(info.download_url, info.release_url);
        assert_eq!(info.notes, "");
    }

    #[test]
    fn opens_only_github_links() {
        let json = r#"{"tag_name":"v2.0.0","html_url":"https://evil.example/releases",
            "assets":[{"name":"Docura.PDF_2.0.0_x64-setup.exe","browser_download_url":"https://github.com.evil.example/setup.exe"}]}"#;
        let info = evaluate_release(json, "1.3.1").unwrap();
        assert_eq!(info.release_url, RELEASES_PAGE);
        assert_eq!(info.download_url, RELEASES_PAGE);
    }

    #[test]
    fn ignores_drafts_and_prereleases() {
        let json = |extra: &str| format!(r#"{{"tag_name":"v9.0.0",{extra}}}"#);
        assert!(
            !evaluate_release(&json(r#""draft":true"#), "1.3.1")
                .unwrap()
                .available
        );
        assert!(
            !evaluate_release(&json(r#""prerelease":true"#), "1.3.1")
                .unwrap()
                .available
        );
    }

    #[test]
    fn rejects_unexpected_responses() {
        // GitHub's error body, a non-version tag, no JSON at all.
        assert!(evaluate_release(r#"{"message":"Not Found","status":"404"}"#, "1.3.1").is_err());
        assert!(evaluate_release(r#"{"tag_name":"nightly"}"#, "1.3.1").is_err());
        assert!(evaluate_release("<html>rate limited</html>", "1.3.1").is_err());
    }

    #[test]
    fn summarizes_markdown_notes() {
        let body = "## What's new\r\n\r\n- **Faster** zoom, see [#12](https://x/12)\r\n- `OCR` fix\r\n\r\n### Fixed\r\n- more";
        assert_eq!(summarize_notes(body, 300), "Faster zoom, see #12 OCR fix");
        let long = "word ".repeat(100);
        let s = summarize_notes(&long, 23);
        assert_eq!(s, "word word word word…");
        assert_eq!(summarize_notes("", 300), "");
    }
}
