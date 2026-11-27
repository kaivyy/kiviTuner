//! Cross-platform application update checker, verified downloader, and installer trigger.
//!
//! Uses GitHub Releases metadata as the canonical source when no signed Tauri updater
//! public key is configured, and coordinates smoothly with `tauri-plugin-updater` when
//! release builds supply a signing keypair.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/kaivyy/kiviTuner/releases/latest";
pub const UPDATE_PROGRESS_EVENT: &str = "kivituner:update-progress";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubReleasePayload {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_name: Option<String>,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub release_url: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub asset_size: Option<u64>,
    pub sha256_url: Option<String>,
    pub signed_updater_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgressEvent {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: u8,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInstallResult {
    pub mode: String,
    pub should_exit: bool,
    pub downloaded_path: String,
    pub message: String,
}

/// Parses a clean `major.minor.patch` version (with optional leading `v` and suffix).
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64, bool)> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    if trimmed.is_empty() {
        return None;
    }
    let (core, has_pre) = match trimmed.split_once('-') {
        Some((c, _)) => (c, true),
        None => (trimmed, false),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch_raw = parts.next()?.split('+').next()?;
    let patch = patch_raw.parse::<u64>().ok()?;
    Some((major, minor, patch, has_pre))
}

/// Returns `true` when `latest` represents a newer semantic release than `current`.
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    let (Some(cur), Some(lat)) = (parse_semver(current), parse_semver(latest)) else {
        return false;
    };
    let cur_core = (cur.0, cur.1, cur.2);
    let lat_core = (lat.0, lat.1, lat.2);
    if lat_core != cur_core {
        return lat_core > cur_core;
    }
    // Stable release (no pre-release suffix) supersedes same numeric pre-release.
    cur.3 && !lat.3
}

/// Picks the best platform-native installer asset for `(target_os, target_arch)`.
pub fn select_platform_asset<'a>(
    assets: &'a [ReleaseAsset],
    target_os: &str,
    target_arch: &str,
) -> Option<&'a ReleaseAsset> {
    let candidates: Vec<(&'a ReleaseAsset, i32)> = assets
        .iter()
        .filter_map(|asset| {
            let lower = asset.name.to_lowercase();
            if lower.ends_with(".sig") || lower.ends_with(".json") || lower == "sha256sums.txt" {
                return None;
            }
            let score = match target_os {
                "windows" => {
                    let is_x86_target = target_arch == "x86";
                    let is_x86_asset = (lower.contains("x86") && !lower.contains("x86_64"))
                        || lower.contains("i686")
                        || lower.contains("win32");
                    let is_x64_asset = lower.contains("x64")
                        || lower.contains("x86_64")
                        || lower.contains("amd64");
                    if is_x86_target && !is_x86_asset {
                        return None;
                    }
                    if !is_x86_target && is_x86_asset {
                        return None;
                    }
                    let arch_bonus = if (!is_x86_target && is_x64_asset)
                        || (is_x86_target && is_x86_asset)
                    {
                        20
                    } else {
                        0
                    };
                    if lower.ends_with("-setup.exe") || lower.ends_with(".exe") {
                        100 + arch_bonus
                    } else if lower.ends_with(".msi") {
                        80 + arch_bonus
                    } else {
                        return None;
                    }
                }
                "linux" => {
                    if lower.ends_with(".appimage") {
                        100
                    } else if lower.ends_with(".deb") {
                        80
                    } else if lower.ends_with(".rpm") {
                        60
                    } else {
                        return None;
                    }
                }
                "macos" => {
                    let wants_arm = target_arch == "aarch64" || target_arch == "arm64";
                    let is_arm_asset = lower.contains("aarch64") || lower.contains("arm64");
                    let is_x64_asset = lower.contains("x64") || lower.contains("x86_64");
                    let arch_bonus = if (wants_arm && is_arm_asset) || (!wants_arm && is_x64_asset)
                    {
                        20
                    } else if !is_arm_asset && !is_x64_asset {
                        10
                    } else {
                        0
                    };
                    if lower.ends_with(".dmg") {
                        100 + arch_bonus
                    } else if lower.ends_with(".app.tar.gz") {
                        70 + arch_bonus
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            Some((asset, score))
        })
        .collect();

    candidates
        .into_iter()
        .max_by_key(|(_, score)| *score)
        .map(|(asset, _)| asset)
}

/// Extracts an expected lowercase SHA-256 hex digest for `asset_name` from `SHA256SUMS.txt`.
pub fn parse_sha256_for_asset(sums_text: &str, asset_name: &str) -> Option<String> {
    let target = asset_name.trim();
    if target.is_empty() {
        return None;
    }
    for line in sums_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(name_raw) = parts.next() else {
            continue;
        };
        let clean_name = name_raw.trim_start_matches('*');
        if clean_name == target
            && hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

pub fn has_signed_updater_config(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("pubkey"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty())
}

fn is_allowed_release_url(url: &str) -> bool {
    url.starts_with("https://github.com/kaivyy/kiviTuner/")
        || url.starts_with("https://objects.githubusercontent.com/")
        || url.starts_with("https://release-assets.githubusercontent.com/")
        || url.starts_with("https://api.github.com/repos/kaivyy/kiviTuner/")
}

fn sanitize_asset_filename(raw: &str) -> Option<String> {
    let name = PathBuf::from(raw)
        .file_name()?
        .to_string_lossy()
        .to_string();
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        return None;
    }
    Some(name)
}

fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("kiviTuner/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("failed to build update HTTP client: {e}"))
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<UpdateCheckResult, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let signed_updater_ready = has_signed_updater_config(&app);

    let client = build_http_client(12)?;
    let resp = client
        .get(GITHUB_LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("failed to query release feed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("release server returned status {}", resp.status()));
    }

    let payload: GithubReleasePayload = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse release metadata: {e}"))?;

    if payload.draft || payload.prerelease {
        return Ok(UpdateCheckResult {
            update_available: false,
            current_version: current.clone(),
            latest_version: current,
            release_name: payload.name,
            release_notes: String::new(),
            published_at: payload.published_at,
            release_url: payload.html_url,
            asset_name: None,
            asset_url: None,
            asset_size: None,
            sha256_url: None,
            signed_updater_ready,
        });
    }

    let latest_clean = payload
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    let update_available = is_newer_version(&current, &latest_clean);
    let selected = select_platform_asset(
        &payload.assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    let sha_asset = payload
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("SHA256SUMS.txt"));

    Ok(UpdateCheckResult {
        update_available,
        current_version: current,
        latest_version: latest_clean,
        release_name: payload.name,
        release_notes: payload.body.unwrap_or_default(),
        published_at: payload.published_at,
        release_url: payload.html_url,
        asset_name: selected.map(|a| a.name.clone()),
        asset_url: selected.map(|a| a.browser_download_url.clone()),
        asset_size: selected.map(|a| a.size),
        sha256_url: sha_asset.map(|a| a.browser_download_url.clone()),
        signed_updater_ready,
    })
}

fn emit_progress(app: &AppHandle, downloaded: u64, total: Option<u64>, phase: &str) {
    let percent = match total {
        Some(t) if t > 0 => ((downloaded.saturating_mul(100)) / t).min(100) as u8,
        _ => 0,
    };
    let _ = app.emit(
        UPDATE_PROGRESS_EVENT,
        UpdateProgressEvent {
            downloaded,
            total,
            percent,
            phase: phase.to_string(),
        },
    );
}

#[tauri::command]
pub async fn download_and_install_update(
    asset_url: String,
    asset_name: String,
    sha256_url: Option<String>,
    app: AppHandle,
) -> Result<UpdateInstallResult, String> {
    if !is_allowed_release_url(&asset_url) {
        return Err("Refusing update download from unverified host".to_string());
    }
    if let Some(ref sum_url) = sha256_url {
        if !is_allowed_release_url(sum_url) {
            return Err("Refusing checksum manifest from unverified host".to_string());
        }
    }

    let clean_name =
        sanitize_asset_filename(&asset_name).ok_or_else(|| "Invalid asset filename".to_string())?;
    let update_dir = std::env::temp_dir().join("kivituner-updates");
    std::fs::create_dir_all(&update_dir)
        .map_err(|e| format!("failed to prepare update directory: {e}"))?;
    let dest_path = update_dir.join(&clean_name);

    let client = build_http_client(300)?;
    emit_progress(&app, 0, None, "Connecting to release server...");

    let expected_sha = if let Some(ref sum_url) = sha256_url {
        if let Ok(resp) = client.get(sum_url).send().await {
            if resp.status().is_success() {
                resp.text()
                    .await
                    .ok()
                    .and_then(|txt| parse_sha256_for_asset(&txt, &clean_name))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let resp = client
        .get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("asset download returned status {}", resp.status()));
    }

    let total = resp.content_length();
    emit_progress(&app, 0, total, "Downloading update package...");

    let part_path = update_dir.join(format!("{clean_name}.part"));
    let mut file = std::fs::File::create(&part_path)
        .map_err(|e| format!("failed to create update file: {e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = resp.bytes_stream();
    let mut last_emit = Instant::now();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("stream error while downloading: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("disk write error: {e}"))?;
        hasher.update(&chunk);
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if last_emit.elapsed() >= Duration::from_millis(120) {
            emit_progress(&app, downloaded, total, "Downloading update package...");
            last_emit = Instant::now();
        }
    }
    file.flush()
        .map_err(|e| format!("failed to flush update file: {e}"))?;
    drop(file);

    let digest_hex = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha {
        if digest_hex != expected {
            let _ = std::fs::remove_file(&part_path);
            return Err(format!(
                "SHA-256 verification failed for {clean_name} (expected {expected}, got {digest_hex})"
            ));
        }
    }

    if dest_path.exists() {
        let _ = std::fs::remove_file(&dest_path);
    }
    std::fs::rename(&part_path, &dest_path)
        .map_err(|e| format!("failed to finalize update package: {e}"))?;

    emit_progress(&app, downloaded, total.or(Some(downloaded)), "Applying update...");

    apply_downloaded_package(&dest_path, &clean_name)
}

fn apply_downloaded_package(dest_path: &PathBuf, clean_name: &str) -> Result<UpdateInstallResult, String> {
    let path_str = dest_path.to_string_lossy().to_string();
    let lower = clean_name.to_lowercase();

    #[cfg(target_os = "linux")]
    {
        if lower.ends_with(".appimage") {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(dest_path)
                .map_err(|e| format!("failed to inspect AppImage permissions: {e}"))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(dest_path, perms)
                .map_err(|e| format!("failed to mark AppImage executable: {e}"))?;

            if let Ok(appimage_env) = std::env::var("APPIMAGE") {
                let target = PathBuf::from(&appimage_env);
                if target.exists() {
                    let backup = target.with_extension("AppImage.bak");
                    let _ = std::fs::remove_file(&backup);
                    std::fs::rename(&target, &backup)
                        .map_err(|e| format!("failed to stage AppImage backup: {e}"))?;
                    if let Err(err) = std::fs::copy(dest_path, &target) {
                        let _ = std::fs::rename(&backup, &target);
                        return Err(format!("failed to replace AppImage binary: {err}"));
                    }
                    let mut t_perms = std::fs::metadata(&target)
                        .map_err(|e| format!("failed to inspect replaced AppImage: {e}"))?
                        .permissions();
                    t_perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&target, t_perms);
                    let _ = std::fs::remove_file(&backup);
                    return Ok(UpdateInstallResult {
                        mode: "restart_ready".to_string(),
                        should_exit: false,
                        downloaded_path: appimage_env,
                        message: "Update installed. Click Restart Now to launch the new version."
                            .to_string(),
                    });
                }
            }
        }

        let _ = Command::new("xdg-open").arg(&path_str).spawn();
        return Ok(UpdateInstallResult {
            mode: "package_opened".to_string(),
            should_exit: false,
            downloaded_path: path_str,
            message: format!(
                "Update downloaded ({clean_name}). Complete installation in your system package handler and reopen kiviTuner."
            ),
        });
    }

    #[cfg(target_os = "windows")]
    {
        if lower.ends_with(".exe") || lower.ends_with(".msi") {
            Command::new("cmd")
                .args(["/C", "start", "", &path_str])
                .spawn()
                .map_err(|e| format!("failed to launch installer: {e}"))?;
            return Ok(UpdateInstallResult {
                mode: "installer_launched".to_string(),
                should_exit: true,
                downloaded_path: path_str,
                message: "Installer launched. Exit or restart kiviTuner to finish updating."
                    .to_string(),
            });
        }
        return Ok(UpdateInstallResult {
            mode: "downloaded".to_string(),
            should_exit: false,
            downloaded_path: path_str,
            message: format!("Downloaded {clean_name}."),
        });
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(&path_str).spawn();
        return Ok(UpdateInstallResult {
            mode: "package_opened".to_string(),
            should_exit: false,
            downloaded_path: path_str,
            message: format!(
                "Opened {clean_name}. Drag the new kiviTuner bundle to Applications and relaunch."
            ),
        });
    }

    #[allow(unreachable_code)]
    Ok(UpdateInstallResult {
        mode: "downloaded".to_string(),
        should_exit: false,
        downloaded_path: path_str,
        message: format!("Downloaded {clean_name}."),
    })
}

#[tauri::command]
pub fn restart_app(app: AppHandle) -> Result<(), String> {
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!(
                "https://github.com/kaivyy/kiviTuner/releases/download/v0.2.0/{name}"
            ),
            size: 12_345_678,
        }
    }

    #[test]
    fn semver_comparison_handles_v_prefix_and_prerelease() {
        assert!(is_newer_version("0.1.0", "0.1.1"));
        assert!(is_newer_version("0.1.0", "v0.2.0"));
        assert!(is_newer_version("0.1.9", "1.0.0"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.2.1", "v0.2.0"));
        assert!(is_newer_version("0.2.0-rc.1", "0.2.0"));
        assert!(!is_newer_version("0.2.0", "0.2.0-rc.1"));
    }

    #[test]
    fn platform_asset_selection_picks_preferred_native_installers() {
        let assets = vec![
            mk_asset("SHA256SUMS.txt"),
            mk_asset("kiviTuner_0.2.0_x64-setup.exe"),
            mk_asset("kiviTuner_0.2.0_x64_en-US.msi"),
            mk_asset("kiviTuner_0.2.0_x86-setup.exe"),
            mk_asset("kiviTuner_0.2.0_amd64.deb"),
            mk_asset("kiviTuner_0.2.0_amd64.AppImage"),
            mk_asset("kiviTuner_0.2.0_amd64.AppImage.sig"),
            mk_asset("kiviTuner_0.2.0_aarch64.dmg"),
            mk_asset("kiviTuner_0.2.0_x64.dmg"),
        ];

        assert_eq!(
            select_platform_asset(&assets, "windows", "x86_64").map(|a| a.name.as_str()),
            Some("kiviTuner_0.2.0_x64-setup.exe")
        );
        assert_eq!(
            select_platform_asset(&assets, "windows", "x86").map(|a| a.name.as_str()),
            Some("kiviTuner_0.2.0_x86-setup.exe")
        );
        assert_eq!(
            select_platform_asset(&assets, "linux", "x86_64").map(|a| a.name.as_str()),
            Some("kiviTuner_0.2.0_amd64.AppImage")
        );
        assert_eq!(
            select_platform_asset(&assets, "macos", "aarch64").map(|a| a.name.as_str()),
            Some("kiviTuner_0.2.0_aarch64.dmg")
        );
        assert_eq!(
            select_platform_asset(&assets, "macos", "x86_64").map(|a| a.name.as_str()),
            Some("kiviTuner_0.2.0_x64.dmg")
        );
    }

    #[test]
    fn sha256_manifest_parser_matches_exact_asset_and_rejects_malformed() {
        let manifest = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  kiviTuner_0.2.0_amd64.AppImage
abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789 *kiviTuner_0.2.0_x64-setup.exe
short_hash  bad.exe
";
        assert_eq!(
            parse_sha256_for_asset(manifest, "kiviTuner_0.2.0_amd64.AppImage").as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert_eq!(
            parse_sha256_for_asset(manifest, "kiviTuner_0.2.0_x64-setup.exe").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert_eq!(parse_sha256_for_asset(manifest, "bad.exe"), None);
        assert_eq!(parse_sha256_for_asset(manifest, "missing.exe"), None);
    }
}
