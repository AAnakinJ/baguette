//! Screenshots of web results (desktop + phone) as visual proof, taken with the browser already
//! on the computer (Edge on Windows, Chrome/Chromium/Edge on Linux) in headless mode, with a
//! throw-away profile. Nothing is downloaded and the user's browser profile is never used.

use crate::claude::command::base_command;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    /// "desktop" or "mobile".
    pub device: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
}

pub const DEVICES: &[(&str, u32, u32)] = &[("desktop", 1280, 800), ("mobile", 390, 844)];

pub fn find_browser() -> Option<PathBuf> {
    if cfg!(windows) {
        let bases = [std::env::var_os("ProgramFiles(x86)"), std::env::var_os("ProgramFiles"), std::env::var_os("LOCALAPPDATA")];
        for base in bases.into_iter().flatten() {
            for rel in ["Microsoft\\Edge\\Application\\msedge.exe", "Google\\Chrome\\Application\\chrome.exe", "Chromium\\Application\\chrome.exe"] {
                let p = PathBuf::from(&base).join(rel);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        return None;
    }
    let path = std::env::var_os("PATH")?;
    for name in ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge", "microsoft-edge-stable"] {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// The page to photograph: a static entry point of the project, if any.
pub fn find_entry_page(project: &Path) -> Option<PathBuf> {
    ["index.html", "dist/index.html", "build/index.html", "public/index.html", "docs/index.html", "site/index.html"]
        .iter()
        .map(|rel| project.join(rel))
        .find(|p| p.is_file())
}

fn file_url(p: &Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches('/');
    // Percent-encode the few characters that break file URLs.
    let enc: String = s.chars().map(|c| match c {
        ' ' => "%20".to_string(),
        '#' => "%23".to_string(),
        '?' => "%3F".to_string(),
        c => c.to_string(),
    }).collect();
    format!("file:///{enc}")
}

/// Photographs `page` for each device into `out_dir`. Returns the shots that worked.
pub async fn capture(browser: &Path, page: &Path, out_dir: &Path) -> Result<Vec<Shot>> {
    std::fs::create_dir_all(out_dir)?;
    let profile = tempfile_dir(out_dir)?;
    let mut shots = Vec::new();
    for (device, w, h) in DEVICES {
        let out = out_dir.join(format!("{device}.png"));
        let _ = std::fs::remove_file(&out);
        let mut cmd = base_command(browser);
        cmd.args([
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-sync",
            "--mute-audio",
            "--virtual-time-budget=4000",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--window-size={w},{h}"))
        .arg(format!("--screenshot={}", out.display()))
        .arg(file_url(page))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        let ok = matches!(tokio::time::timeout(Duration::from_secs(40), cmd.status()).await, Ok(Ok(_)));
        if ok && out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            shots.push(Shot { device: device.to_string(), path: out.display().to_string(), width: *w, height: *h });
        }
    }
    let _ = std::fs::remove_dir_all(&profile);
    if shots.is_empty() {
        return Err(Error::Invalid("screenshot_failed".into()));
    }
    Ok(shots)
}

fn tempfile_dir(parent: &Path) -> Result<PathBuf> {
    let d = parent.join(format!(".profile-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// PNG file -> data URL, only for files inside `allowed_root` (the app's screenshot folder).
pub fn data_url(path: &Path, allowed_root: &Path) -> Result<String> {
    let (p, root) = (std::path::absolute(path)?, std::path::absolute(allowed_root)?);
    if !p.starts_with(&root) || p.extension().and_then(|e| e.to_str()) != Some("png") {
        return Err(Error::Invalid("screenshot path".into()));
    }
    let bytes = std::fs::read(&p)?;
    Ok(format!("data:image/png;base64,{}", base64(&bytes)))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn entry_page_lookup_order() {
        let d = tempfile::tempdir().unwrap();
        assert!(find_entry_page(d.path()).is_none());
        std::fs::create_dir_all(d.path().join("dist")).unwrap();
        std::fs::write(d.path().join("dist/index.html"), "x").unwrap();
        assert!(find_entry_page(d.path()).unwrap().ends_with("dist/index.html"));
        std::fs::write(d.path().join("index.html"), "x").unwrap();
        assert!(find_entry_page(d.path()).unwrap().ends_with("index.html"));
    }

    #[test]
    fn data_url_stays_in_its_folder() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.png"), [137u8, 80, 78, 71]).unwrap();
        assert!(data_url(&d.path().join("a.png"), d.path()).unwrap().starts_with("data:image/png;base64,iVBORw"));
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("b.png"), [1u8]).unwrap();
        assert!(data_url(&other.path().join("b.png"), d.path()).is_err());
    }

    #[tokio::test]
    async fn real_screenshots_when_a_browser_exists() {
        let Some(browser) = find_browser() else { return };
        let site = tempfile::tempdir().unwrap();
        std::fs::write(site.path().join("index.html"), "<!doctype html><html lang=fr><meta name=viewport content='width=device-width'><body style='background:#f5e6d3'><h1>Bonjour</h1></body></html>").unwrap();
        let out = tempfile::tempdir().unwrap();
        let shots = capture(&browser, &site.path().join("index.html"), out.path()).await.unwrap();
        assert_eq!(shots.len(), 2);
        for s in &shots {
            let bytes = std::fs::read(&s.path).unwrap();
            assert_eq!(&bytes[..4], &[137, 80, 78, 71], "PNG signature");
            assert!(bytes.len() > 1000);
        }
        assert!(std::fs::read_dir(out.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".profile")), "temp profile removed");
    }
}
