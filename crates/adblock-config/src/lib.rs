//! Configuration for AdBlock.
//!
//! The configuration file is TOML. Every field has a privacy-preserving
//! default, unknown keys are rejected (so typos are reported instead of being
//! silently ignored) and all values are validated before use.

pub mod fs;
mod paths;

pub use paths::{Paths, PathsError};

use adblock_filter::{Category, ListFormat};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid configuration syntax in {path}: {message}")]
    Syntax { path: PathBuf, message: String },
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

fn invalid(msg: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(msg.into())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct Config {
    pub general: General,
    pub dns: Dns,
    pub filters: Filters,
    pub browser: Browser,
    pub stats: Stats,
    pub privacy: Privacy,
    pub custom: Custom,
    pub paths: PathOverrides,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// Master switch. When false the daemon answers DNS without filtering.
    pub enabled: bool,
    /// `error`, `warn`, `info`, `debug` or `trace`.
    pub log_level: String,
}

impl Default for General {
    fn default() -> Self {
        General {
            enabled: true,
            log_level: "info".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BlockingMode {
    /// Answer A with 0.0.0.0 and AAAA with `::` (fastest failure in browsers).
    #[default]
    Null,
    /// Answer NXDOMAIN.
    Nxdomain,
    /// Answer REFUSED.
    Refused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Dns {
    pub enabled: bool,
    /// Addresses to listen on (UDP and TCP).
    pub listen: Vec<String>,
    /// Upstream resolvers: `https://host/path` (DNS-over-HTTPS), or plain
    /// `ip[:port]` / `udp://ip[:port]` (UDP with TCP fallback).
    pub upstreams: Vec<String>,
    /// Plain-DNS servers used only to resolve DoH upstream hostnames. Never
    /// used for ordinary queries.
    pub bootstrap: Vec<String>,
    pub blocking_mode: BlockingMode,
    /// TTL of synthesized blocking answers, in seconds.
    pub blocked_ttl: u32,
    /// Maximum number of cached responses (0 disables caching).
    pub cache_size: usize,
    pub cache_min_ttl: u32,
    pub cache_max_ttl: u32,
    /// Per-upstream timeout.
    pub timeout_ms: u64,
    pub max_concurrent: usize,
    /// Answer reverse lookups for private address ranges locally (NXDOMAIN)
    /// instead of leaking them upstream, unless a forward zone covers them.
    pub block_private_ptr: bool,
    /// Conditional forwarding: zone → plain DNS servers, e.g.
    /// `"lan" = ["192.168.1.1"]` for local network names.
    pub forward_zones: BTreeMap<String, Vec<String>>,
    /// Permit non-loopback listen addresses. Off by default: an open resolver
    /// on a LAN or the Internet is a security problem.
    pub allow_non_loopback: bool,
}

impl Default for Dns {
    fn default() -> Self {
        Dns {
            enabled: true,
            listen: vec!["127.0.0.1:53".into(), "[::1]:53".into()],
            upstreams: vec![
                "https://cloudflare-dns.com/dns-query".into(),
                "https://dns.google/dns-query".into(),
            ],
            bootstrap: vec![
                "1.1.1.1".into(),
                "8.8.8.8".into(),
                "2606:4700:4700::1111".into(),
            ],
            blocking_mode: BlockingMode::Null,
            blocked_ttl: 60,
            cache_size: 10_000,
            cache_min_ttl: 0,
            cache_max_ttl: 86_400,
            timeout_ms: 3_000,
            max_concurrent: 1_024,
            block_private_ptr: true,
            forward_zones: BTreeMap::new(),
            allow_non_loopback: false,
        }
    }
}

/// A parsed upstream resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upstream {
    /// DNS-over-HTTPS (RFC 8484) endpoint.
    Https { url: String, host: String },
    /// Plain DNS over UDP with TCP fallback.
    Plain(SocketAddr),
}

impl Upstream {
    pub fn parse(input: &str) -> Result<Upstream, ConfigError> {
        let s = input.trim();
        if let Some(rest) = s.strip_prefix("https://") {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            if authority.is_empty() || authority.contains('@') {
                return Err(invalid(format!("invalid DoH upstream {s:?}")));
            }
            let host = if let Some(stripped) = authority.strip_prefix('[') {
                stripped.split(']').next().unwrap_or("").to_string()
            } else {
                authority.split(':').next().unwrap_or("").to_string()
            };
            if host.is_empty() || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(invalid(format!("invalid DoH upstream {s:?}")));
            }
            return Ok(Upstream::Https {
                url: s.to_string(),
                host,
            });
        }
        if s.starts_with("http://") {
            return Err(invalid(format!(
                "upstream {s:?}: plain-HTTP DoH would expose queries; use https://"
            )));
        }
        if s.starts_with("tls://") || s.starts_with("quic://") {
            return Err(invalid(format!(
                "upstream {s:?}: DNS-over-TLS/QUIC is not supported; use https://"
            )));
        }
        let addr = s.strip_prefix("udp://").unwrap_or(s);
        parse_socket_addr(addr, 53)
            .map(Upstream::Plain)
            .ok_or_else(|| invalid(format!("invalid upstream {s:?}")))
    }
}

/// Parses `ip`, `ip:port`, or `[ipv6]:port`.
pub fn parse_socket_addr(s: &str, default_port: u16) -> Option<SocketAddr> {
    if let Ok(addr) = s.parse::<SocketAddr>() {
        return Some(addr);
    }
    s.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .ok()
        .map(|ip| SocketAddr::new(ip, default_port))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Filters {
    pub enabled: bool,
    pub auto_update: bool,
    pub update_interval_hours: u32,
    /// Maximum size of a single downloaded list.
    pub max_list_size_mb: u32,
    pub lists: Vec<FilterList>,
}

impl Default for Filters {
    fn default() -> Self {
        Filters {
            enabled: true,
            auto_update: true,
            update_interval_hours: 12,
            max_list_size_mb: 32,
            lists: default_lists(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterList {
    /// Identifier, also used as the on-disk file name.
    pub name: String,
    /// HTTPS URL to download updates from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// File name inside the bundled filters directory, used until the first
    /// successful download (or permanently if `url` is not set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled: Option<String>,
    /// Absolute path of a local list file (alternative to `url`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub category: Category,
    #[serde(default)]
    pub format: ListFormat,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Optional pinned SHA-256 of the list content (hex). Useful for lists
    /// you host yourself; public lists change too often to pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Updates producing fewer valid rules than this are rejected.
    #[serde(default = "default_min_rules")]
    pub min_rules: usize,
}

fn yes() -> bool {
    true
}
fn default_min_rules() -> usize {
    10
}

fn list(
    name: &str,
    url: Option<&str>,
    bundled: Option<&str>,
    category: Category,
    format: ListFormat,
) -> FilterList {
    FilterList {
        name: name.into(),
        url: url.map(Into::into),
        bundled: bundled.map(Into::into),
        path: None,
        category,
        format,
        enabled: true,
        sha256: None,
        min_rules: default_min_rules(),
    }
}

/// Default lists: widely used, openly licensed, actively maintained.
pub fn default_lists() -> Vec<FilterList> {
    vec![
        list(
            "easylist",
            Some("https://easylist.to/easylist/easylist.txt"),
            None,
            Category::Ads,
            ListFormat::Adblock,
        ),
        list(
            "easyprivacy",
            Some("https://easylist.to/easylist/easyprivacy.txt"),
            None,
            Category::Trackers,
            ListFormat::Adblock,
        ),
        list(
            "ublock-filters",
            Some("https://ublockorigin.github.io/uAssets/filters/filters.txt"),
            None,
            Category::Ads,
            ListFormat::Adblock,
        ),
        list(
            "peter-lowe",
            Some(
                "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=hosts&showintro=0&mimetype=plaintext",
            ),
            None,
            Category::Ads,
            ListFormat::Hosts,
        ),
        // Bundled baseline so protection works before the first download.
        FilterList {
            min_rules: 1,
            ..list(
                "baseline",
                None,
                Some("baseline.txt"),
                Category::Ads,
                ListFormat::Auto,
            )
        },
        // YouTube compatibility rules, kept separate from the generic lists
        // so they can be updated independently of the binary. Point `url` at
        // your own copy to receive updates.
        FilterList {
            min_rules: 1,
            ..list(
                "youtube",
                None,
                Some("youtube.txt"),
                Category::Ads,
                ListFormat::Adblock,
            )
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Browser {
    pub enabled: bool,
    /// Cap for exported `declarativeNetRequest` rules (Chromium limit: 30,000).
    pub max_dnr_rules: usize,
}

impl Default for Browser {
    fn default() -> Self {
        Browser {
            enabled: true,
            max_dnr_rules: 30_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Stats {
    pub enabled: bool,
    /// Keep per-domain counts of *blocked* domains for "top blocked" output.
    /// Allowed (visited) domains are never recorded.
    pub record_blocked_domains: bool,
    pub retention_days: u32,
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            enabled: true,
            record_blocked_domains: true,
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Privacy {
    /// AdBlock contains no telemetry. This key exists so that the setting is
    /// explicit; enabling it is rejected.
    pub telemetry: bool,
    /// Log each blocked hostname at debug level (never allowed hostnames).
    pub log_blocked: bool,
}

/// Administrator-defined rules (in addition to `adblock allow/block`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Custom {
    pub allow: Vec<String>,
    pub block: Vec<String>,
    /// Raw filter rules in adblock syntax.
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PathOverrides {
    pub data_dir: Option<PathBuf>,
    pub socket: Option<PathBuf>,
    pub bundled_filters: Option<PathBuf>,
}

/// Valid list names are short and filesystem-safe.
pub fn valid_list_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl Config {
    /// Parses and validates TOML text.
    pub fn from_toml(text: &str, origin: &Path) -> Result<Config, ConfigError> {
        let config: Config = toml::from_str(text).map_err(|e| ConfigError::Syntax {
            path: origin.to_path_buf(),
            message: e.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Loads a configuration file. A missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::from_toml(&text, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(source) => Err(ConfigError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("config always serializes")
    }

    pub fn listen_addrs(&self) -> Vec<SocketAddr> {
        self.dns
            .listen
            .iter()
            .filter_map(|s| parse_socket_addr(s, 53))
            .collect()
    }

    pub fn upstreams(&self) -> Vec<Upstream> {
        self.dns
            .upstreams
            .iter()
            .filter_map(|u| Upstream::parse(u).ok())
            .collect()
    }

    pub fn bootstrap_addrs(&self) -> Vec<SocketAddr> {
        self.dns
            .bootstrap
            .iter()
            .filter_map(|s| parse_socket_addr(s, 53))
            .collect()
    }

    pub fn forward_zones(&self) -> Vec<(String, Vec<SocketAddr>)> {
        self.dns
            .forward_zones
            .iter()
            .map(|(zone, servers)| {
                let zone = zone.trim_end_matches('.').to_ascii_lowercase();
                (
                    zone,
                    servers
                        .iter()
                        .filter_map(|s| parse_socket_addr(s, 53))
                        .collect(),
                )
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        const LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
        if !LEVELS.contains(&self.general.log_level.as_str()) {
            return Err(invalid(format!(
                "general.log_level must be one of {LEVELS:?}"
            )));
        }
        if self.privacy.telemetry {
            return Err(invalid(
                "privacy.telemetry: AdBlock has no telemetry; this must remain false",
            ));
        }

        // DNS
        if self.dns.enabled && self.dns.listen.is_empty() {
            return Err(invalid("dns.listen must contain at least one address"));
        }
        for l in &self.dns.listen {
            let addr = parse_socket_addr(l, 53)
                .ok_or_else(|| invalid(format!("dns.listen: invalid address {l:?}")))?;
            if !addr.ip().is_loopback() && !self.dns.allow_non_loopback {
                return Err(invalid(format!(
                    "dns.listen: {addr} is not a loopback address; set dns.allow_non_loopback = true \
                     only if you intend to serve other machines"
                )));
            }
        }
        if self.dns.enabled && self.dns.upstreams.is_empty() {
            return Err(invalid("dns.upstreams must contain at least one resolver"));
        }
        let mut needs_bootstrap = false;
        for u in &self.dns.upstreams {
            if let Upstream::Https { host, .. } = Upstream::parse(u)?
                && host.parse::<IpAddr>().is_err()
            {
                needs_bootstrap = true;
            }
        }
        for b in &self.dns.bootstrap {
            if parse_socket_addr(b, 53).is_none() {
                return Err(invalid(format!(
                    "dns.bootstrap: {b:?} must be an IP address"
                )));
            }
        }
        if needs_bootstrap && self.dns.bootstrap.is_empty() {
            return Err(invalid(
                "dns.bootstrap is required when DoH upstreams use hostnames (otherwise resolving \
                 the upstream would loop back through this resolver)",
            ));
        }
        for (zone, servers) in &self.dns.forward_zones {
            if adblock_filter::domain::normalize_hostname(zone).is_none() {
                return Err(invalid(format!("dns.forward_zones: invalid zone {zone:?}")));
            }
            if servers.is_empty() || servers.iter().any(|s| parse_socket_addr(s, 53).is_none()) {
                return Err(invalid(format!(
                    "dns.forward_zones.{zone}: servers must be IP addresses"
                )));
            }
        }
        if self.dns.cache_size > 1_000_000 {
            return Err(invalid("dns.cache_size must be at most 1000000"));
        }
        if self.dns.cache_min_ttl > self.dns.cache_max_ttl {
            return Err(invalid(
                "dns.cache_min_ttl must not exceed dns.cache_max_ttl",
            ));
        }
        if !(100..=30_000).contains(&self.dns.timeout_ms) {
            return Err(invalid("dns.timeout_ms must be between 100 and 30000"));
        }
        if !(1..=65_536).contains(&self.dns.max_concurrent) {
            return Err(invalid("dns.max_concurrent must be between 1 and 65536"));
        }
        if self.dns.blocked_ttl > 86_400 {
            return Err(invalid("dns.blocked_ttl must be at most 86400"));
        }

        // Filters
        if !(1..=168).contains(&self.filters.update_interval_hours) {
            return Err(invalid(
                "filters.update_interval_hours must be between 1 and 168",
            ));
        }
        if !(1..=256).contains(&self.filters.max_list_size_mb) {
            return Err(invalid(
                "filters.max_list_size_mb must be between 1 and 256",
            ));
        }
        let mut names = std::collections::HashSet::new();
        for l in &self.filters.lists {
            if !valid_list_name(&l.name) {
                return Err(invalid(format!(
                    "filter list name {:?} must be 1-64 characters of a-z, 0-9, '-' or '_'",
                    l.name
                )));
            }
            if !names.insert(&l.name) {
                return Err(invalid(format!("duplicate filter list name {:?}", l.name)));
            }
            if l.url.is_none() && l.path.is_none() && l.bundled.is_none() {
                return Err(invalid(format!(
                    "filter list {:?} needs a url, path or bundled file",
                    l.name
                )));
            }
            if let Some(url) = &l.url
                && (!url.starts_with("https://")
                    || url.len() <= 8
                    || url.chars().any(char::is_whitespace))
            {
                return Err(invalid(format!(
                    "filter list {:?}: url must be an https:// URL",
                    l.name
                )));
            }
            if let Some(p) = &l.path
                && (!p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir)))
            {
                return Err(invalid(format!(
                    "filter list {:?}: path must be absolute without '..'",
                    l.name
                )));
            }
            if let Some(b) = &l.bundled
                && (b.is_empty() || b.contains(['/', '\\']) || b.starts_with('.'))
            {
                return Err(invalid(format!(
                    "filter list {:?}: bundled must be a plain file name",
                    l.name
                )));
            }
            if let Some(h) = &l.sha256
                && (h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(invalid(format!(
                    "filter list {:?}: sha256 must be 64 hex characters",
                    l.name
                )));
            }
        }

        // Custom
        for d in self.custom.allow.iter().chain(&self.custom.block) {
            if adblock_filter::domain::normalize_hostname(d).is_none() {
                return Err(invalid(format!("custom: invalid domain {d:?}")));
            }
        }
        if !(1..=3650).contains(&self.stats.retention_days) {
            return Err(invalid("stats.retention_days must be between 1 and 3650"));
        }
        if !(100..=300_000).contains(&self.browser.max_dnr_rules) {
            return Err(invalid(
                "browser.max_dnr_rules must be between 100 and 300000",
            ));
        }
        for p in [
            &self.paths.data_dir,
            &self.paths.socket,
            &self.paths.bundled_filters,
        ]
        .into_iter()
        .flatten()
        {
            if !p.is_absolute() {
                return Err(invalid(format!("paths: {} must be absolute", p.display())));
            }
        }
        Ok(())
    }
}

/// The commented configuration file written by installers.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default-config.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        Config::default().validate().unwrap();
        let parsed = Config::from_toml(DEFAULT_CONFIG_TOML, Path::new("default")).unwrap();
        assert_eq!(
            parsed,
            Config::default(),
            "default-config.toml must match Config::default()"
        );
    }

    #[test]
    fn round_trip() {
        let c = Config::default();
        let again = Config::from_toml(&c.to_toml(), Path::new("x")).unwrap();
        assert_eq!(c, again);
    }

    #[test]
    fn upstream_parsing() {
        assert!(
            matches!(Upstream::parse("https://dns.google/dns-query"), Ok(Upstream::Https { host, .. }) if host == "dns.google")
        );
        assert!(
            matches!(Upstream::parse("https://[2606:4700::1111]/dns-query"), Ok(Upstream::Https { host, .. }) if host == "2606:4700::1111")
        );
        assert_eq!(
            Upstream::parse("9.9.9.9").unwrap(),
            Upstream::Plain("9.9.9.9:53".parse().unwrap())
        );
        assert_eq!(
            Upstream::parse("udp://[::1]:5300").unwrap(),
            Upstream::Plain("[::1]:5300".parse().unwrap())
        );
        assert!(Upstream::parse("http://x/dns-query").is_err());
        assert!(Upstream::parse("tls://1.1.1.1").is_err());
        assert!(Upstream::parse("https://user@x/").is_err());
        assert!(Upstream::parse("nonsense").is_err());
    }
}
