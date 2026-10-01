//! Registration of the native-messaging host used by the browser extensions.
//!
//! The browser starts `adblock-native-host` (a symlink to `adblock`) and
//! talks to it over stdin/stdout; the host relays a fixed set of read-only
//! requests to the daemon's control socket. Browsers only allow the
//! extension IDs listed in the manifest to start the host.

use anyhow::{Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

pub const NATIVE_HOST_NAME: &str = "com.adblock.native";
/// ID derived from the public key embedded in `browser/chromium/manifest.json`.
pub const CHROMIUM_EXTENSION_ID: &str = "chlpacjmjgeglnhaedpfiffkdfakeana";
/// `browser_specific_settings.gecko.id` in `browser/firefox/manifest.json`.
pub const FIREFOX_EXTENSION_ID: &str = "adblock@adblock.local";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Chromium,
    Firefox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLocation {
    pub browser: &'static str,
    pub family: Family,
    /// Directory that holds `com.adblock.native.json`.
    pub dir: PathBuf,
}

impl HostLocation {
    pub fn manifest_path(&self) -> PathBuf {
        self.dir.join(format!("{NATIVE_HOST_NAME}.json"))
    }
}

fn loc(browser: &'static str, family: Family, dir: PathBuf) -> HostLocation {
    HostLocation {
        browser,
        family,
        dir,
    }
}

/// Per-user manifest locations for supported browsers.
pub fn user_locations(home: &Path) -> Vec<HostLocation> {
    use Family::*;
    if cfg!(target_os = "macos") {
        let s = home.join("Library/Application Support");
        vec![
            loc(
                "Google Chrome",
                Chromium,
                s.join("Google/Chrome/NativeMessagingHosts"),
            ),
            loc(
                "Chromium",
                Chromium,
                s.join("Chromium/NativeMessagingHosts"),
            ),
            loc(
                "Brave",
                Chromium,
                s.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            ),
            loc(
                "Microsoft Edge",
                Chromium,
                s.join("Microsoft Edge/NativeMessagingHosts"),
            ),
            loc("Vivaldi", Chromium, s.join("Vivaldi/NativeMessagingHosts")),
            loc("Firefox", Firefox, s.join("Mozilla/NativeMessagingHosts")),
        ]
    } else {
        let c = home.join(".config");
        vec![
            loc(
                "Google Chrome",
                Chromium,
                c.join("google-chrome/NativeMessagingHosts"),
            ),
            loc(
                "Chromium",
                Chromium,
                c.join("chromium/NativeMessagingHosts"),
            ),
            loc(
                "Brave",
                Chromium,
                c.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            ),
            loc(
                "Microsoft Edge",
                Chromium,
                c.join("microsoft-edge/NativeMessagingHosts"),
            ),
            loc("Vivaldi", Chromium, c.join("vivaldi/NativeMessagingHosts")),
            loc(
                "Firefox",
                Firefox,
                home.join(".mozilla/native-messaging-hosts"),
            ),
        ]
    }
}

/// System-wide manifest locations (written by the installer as root).
pub fn system_locations() -> Vec<HostLocation> {
    use Family::*;
    if cfg!(target_os = "macos") {
        vec![
            loc(
                "Google Chrome",
                Chromium,
                "/Library/Google/Chrome/NativeMessagingHosts".into(),
            ),
            loc(
                "Chromium",
                Chromium,
                "/Library/Application Support/Chromium/NativeMessagingHosts".into(),
            ),
            loc(
                "Microsoft Edge",
                Chromium,
                "/Library/Microsoft/Edge/NativeMessagingHosts".into(),
            ),
            loc(
                "Firefox",
                Firefox,
                "/Library/Application Support/Mozilla/NativeMessagingHosts".into(),
            ),
        ]
    } else {
        vec![
            loc(
                "Google Chrome",
                Chromium,
                "/etc/opt/chrome/native-messaging-hosts".into(),
            ),
            loc(
                "Chromium",
                Chromium,
                "/etc/chromium/native-messaging-hosts".into(),
            ),
            loc(
                "Microsoft Edge",
                Chromium,
                "/etc/opt/edge/native-messaging-hosts".into(),
            ),
            loc(
                "Firefox",
                Firefox,
                "/usr/lib/mozilla/native-messaging-hosts".into(),
            ),
            loc(
                "Firefox",
                Firefox,
                "/usr/lib64/mozilla/native-messaging-hosts".into(),
            ),
        ]
    }
}

/// Builds the manifest JSON. `extra_ids` allows store-assigned extension
/// IDs in addition to the built-in ones.
pub fn manifest(family: Family, host_path: &Path, extra_ids: &[String]) -> serde_json::Value {
    let path = host_path.display().to_string();
    let description = "AdBlock native messaging host";
    match family {
        Family::Chromium => {
            let mut origins = vec![format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")];
            origins.extend(
                extra_ids
                    .iter()
                    .filter(|id| id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)))
                    .map(|id| format!("chrome-extension://{id}/")),
            );
            json!({ "name": NATIVE_HOST_NAME, "description": description, "path": path,
                    "type": "stdio", "allowed_origins": origins })
        }
        Family::Firefox => {
            let mut ids = vec![FIREFOX_EXTENSION_ID.to_string()];
            ids.extend(
                extra_ids
                    .iter()
                    .filter(|id| id.contains('@') && !id.contains(['"', '\\']))
                    .cloned(),
            );
            json!({ "name": NATIVE_HOST_NAME, "description": description, "path": path,
                    "type": "stdio", "allowed_extensions": ids })
        }
    }
}

/// Writes manifests. In user mode only browsers whose profile directory
/// exists are registered; in system mode all standard locations are used.
pub fn install(
    locations: &[HostLocation],
    host_path: &Path,
    extra_ids: &[String],
    only_existing: bool,
) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for l in locations {
        if only_existing && !l.dir.parent().is_some_and(Path::exists) {
            continue;
        }
        std::fs::create_dir_all(&l.dir)
            .with_context(|| format!("cannot create {}", l.dir.display()))?;
        let body = serde_json::to_vec_pretty(&manifest(l.family, host_path, extra_ids))?;
        let path = l.manifest_path();
        adblock_config::fs::write_atomic(&path, &body, 0o644)
            .with_context(|| format!("cannot write {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// Removes manifests; returns the files removed.
pub fn uninstall(locations: &[HostLocation]) -> Vec<PathBuf> {
    locations
        .iter()
        .map(HostLocation::manifest_path)
        .filter(|p| p.exists() && std::fs::remove_file(p).is_ok())
        .collect()
}

/// Manifests currently present for the given user (and system-wide).
pub fn installed(home: Option<&Path>) -> Vec<(String, PathBuf)> {
    let mut all = system_locations();
    if let Some(h) = home {
        all.extend(user_locations(h));
    }
    all.into_iter()
        .filter(|l| l.manifest_path().exists())
        .map(|l| (l.browser.to_string(), l.manifest_path()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests() {
        let m = manifest(
            Family::Chromium,
            Path::new("/usr/local/bin/adblock-native-host"),
            &["bad".into()],
        );
        assert_eq!(m["allowed_origins"].as_array().unwrap().len(), 1);
        assert_eq!(m["path"], "/usr/local/bin/adblock-native-host");
        let m = manifest(
            Family::Firefox,
            Path::new("/x"),
            &["other@example.org".into()],
        );
        assert_eq!(m["allowed_extensions"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn install_user_only_existing() {
        let home = tempfile::tempdir().unwrap();
        let locs = user_locations(home.path());
        assert!(
            install(&locs, Path::new("/x"), &[], true)
                .unwrap()
                .is_empty()
        );
        std::fs::create_dir_all(locs[0].dir.parent().unwrap()).unwrap();
        let written = install(&locs, Path::new("/x"), &[], true).unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(uninstall(&locs).len(), 1);
    }
}
