//! Filesystem locations, per platform and per mode (system / user).
//!
//! System mode (the normal installation):
//!
//! | purpose          | Linux                         | macOS                                              |
//! |------------------|-------------------------------|----------------------------------------------------|
//! | config           | `/etc/adblock/config.toml`    | `/Library/Application Support/AdBlock/config.toml` |
//! | root-only state  | `/var/lib/adblock`            | `/Library/Application Support/AdBlock`             |
//! | daemon data      | `/var/lib/adblock/data`       | `/Library/Application Support/AdBlock/data`        |
//! | control socket   | `/run/adblock/control.sock`   | `/Library/Application Support/AdBlock/data/control.sock` |
//! | logs             | journald                      | `/Library/Logs/AdBlock/adblock.log`                |
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

/// `<prefix>/share/adblock/filters`, derived from the executable location
/// (`<prefix>/bin/adblock`), so `/usr`, `/usr/local` and Homebrew prefixes
/// all work.
fn default_bundled() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(Path::parent)
    {
        let candidate = prefix.join("share/adblock/filters");
        if candidate.is_dir() {
            return candidate;
        }
    }
    for candidate in [
        "/usr/share/adblock/filters",
        "/usr/local/share/adblock/filters",
        "/opt/homebrew/share/adblock/filters",
    ] {
        if Path::new(candidate).is_dir() {
            return candidate.into();
        }
    }
    PathBuf::from("/usr/local/share/adblock/filters")
}

impl Paths {
    /// System-wide locations for the current platform.
    pub fn system() -> Paths {
        if cfg!(target_os = "macos") {
            let base = PathBuf::from("/Library/Application Support/AdBlock");
            Paths {
                config_file: base.join("config.toml"),
                system_dir: base.clone(),
                data_dir: base.join("data"),
                socket: base.join("data/control.sock"),
                bundled_filters: default_bundled(),
                log_file: Some(PathBuf::from("/Library/Logs/AdBlock/adblock.log")),
                user_mode: false,
            }
        } else {
            Paths {
                config_file: PathBuf::from("/etc/adblock/config.toml"),
                system_dir: PathBuf::from("/var/lib/adblock"),
                data_dir: PathBuf::from("/var/lib/adblock/data"),
                socket: PathBuf::from("/run/adblock/control.sock"),
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
            let base = home.join("Library/Application Support/AdBlock");
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
            (config.join("adblock"), data.join("adblock"))
        };
        Ok(Paths {
            config_file: config_dir.join("config.toml"),
            system_dir: data_dir.join("system"),
            socket: data_dir.join("control.sock"),
            data_dir: data_dir.clone(),
            bundled_filters: default_bundled(),
            log_file: Some(data_dir.join("adblock.log")),
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
