//! Filesystem locations, per platform and per mode (system / user).
//!
//! System mode (the normal installation):
//!
//! | purpose          | Linux                         | macOS                                              |
//! |------------------|-------------------------------|----------------------------------------------------|
//! | config           | `/etc/fuck-ads/config.toml`    | `/Library/Application Support/FuckAds/config.toml` |
//! | root-only state  | `/var/lib/fuck-ads`            | `/Library/Application Support/FuckAds`             |
//! | daemon data      | `/var/lib/fuck-ads/data`       | `/Library/Application Support/FuckAds/data`        |
//! | control socket   | `/run/fuck-ads/control.sock`   | `/Library/Application Support/FuckAds/data/control.sock` |
//! | logs             | journald                      | `/Library/Logs/FuckAds/fuck-ads.log`                |
//!
//! User mode keeps everything under the user's home directory and is meant
//! for development or for running only the browser integration.

use crate::PathOverrides;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("HOME is not set; cannot determine user directories")]
    NoHome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_file: PathBuf,
    /// Root-owned directory for privileged state (DNS configuration backups).
    pub system_dir: PathBuf,
    /// Directory owned by the daemon's service account.
    pub data_dir: PathBuf,
    pub socket: PathBuf,
    /// Filter lists shipped with the package.
    pub bundled_filters: PathBuf,
    /// Log file, on platforms without a system journal.
    pub log_file: Option<PathBuf>,
    pub user_mode: bool,
}

/// `<prefix>/share/fuck-ads/filters`, derived from the executable location
/// (`<prefix>/bin/fuck-ads`), so `/usr`, `/usr/local` and Homebrew prefixes
/// all work.
fn default_bundled() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(Path::parent)
    {
        let candidate = prefix.join("share/fuck-ads/filters");
        if candidate.is_dir() {
            return candidate;
        }
    }
    for candidate in [
        "/usr/share/fuck-ads/filters",
        "/usr/local/share/fuck-ads/filters",
        "/opt/homebrew/share/fuck-ads/filters",
    ] {
        if Path::new(candidate).is_dir() {
            return candidate.into();
        }
    }
    PathBuf::from("/usr/local/share/fuck-ads/filters")
}

impl Paths {
    /// System-wide locations for the current platform.
    pub fn system() -> Paths {
        if cfg!(target_os = "macos") {
            let base = PathBuf::from("/Library/Application Support/FuckAds");
            Paths {
                config_file: base.join("config.toml"),
                system_dir: base.clone(),
                data_dir: base.join("data"),
                socket: base.join("data/control.sock"),
                bundled_filters: default_bundled(),
                log_file: Some(PathBuf::from("/Library/Logs/FuckAds/fuck-ads.log")),
                user_mode: false,
            }
        } else {
            Paths {
                config_file: PathBuf::from("/etc/fuck-ads/config.toml"),
                system_dir: PathBuf::from("/var/lib/fuck-ads"),
                data_dir: PathBuf::from("/var/lib/fuck-ads/data"),
                socket: PathBuf::from("/run/fuck-ads/control.sock"),
                bundled_filters: default_bundled(),
                log_file: None,
                user_mode: false,
            }
        }
    }

    /// Per-user locations (no root required).
    pub fn user() -> Result<Paths, PathsError> {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or(PathsError::NoHome)?;
        let (config_dir, data_dir) = if cfg!(target_os = "macos") {
            let base = home.join("Library/Application Support/FuckAds");
            (base.clone(), base.join("data"))
        } else {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".config"));
            let data = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share"));
            (config.join("fuck-ads"), data.join("fuck-ads"))
        };
        Ok(Paths {
            config_file: config_dir.join("config.toml"),
            system_dir: data_dir.join("system"),
            socket: data_dir.join("control.sock"),
            data_dir: data_dir.clone(),
            bundled_filters: default_bundled(),
            log_file: Some(data_dir.join("fuck-ads.log")),
            user_mode: true,
        })
    }

    /// Applies `[paths]` overrides from the configuration file.
    pub fn apply_overrides(&mut self, overrides: &PathOverrides) {
        if let Some(d) = &overrides.data_dir {
            self.data_dir = d.clone();
            if overrides.socket.is_none() && self.user_mode {
                self.socket = d.join("control.sock");
            }
        }
        if let Some(s) = &overrides.socket {
            self.socket = s.clone();
        }
        if let Some(b) = &overrides.bundled_filters {
            self.bundled_filters = b.clone();
        }
    }

    /// Directory holding downloaded and installed filter lists.
    pub fn lists_dir(&self) -> PathBuf {
        self.data_dir.join("lists")
    }

    pub fn stats_file(&self) -> PathBuf {
        self.data_dir.join("stats.json")
    }

    pub fn user_rules_file(&self) -> PathBuf {
        self.data_dir.join("user-rules.json")
    }

    pub fn update_state_file(&self) -> PathBuf {
        self.data_dir.join("update-state.json")
    }

    /// Root-only record of how system DNS was configured before `enable`.
    pub fn system_state_file(&self) -> PathBuf {
        self.system_dir.join("system-state.json")
    }
}
