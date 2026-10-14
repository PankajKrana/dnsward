//! Configuration parsing and validation.

use fuck_ads_config::{BlockingMode, Config, DEFAULT_CONFIG_TOML, Upstream};
use std::path::Path;

fn parse(text: &str) -> Result<Config, String> {
    Config::from_toml(text, Path::new("test.toml")).map_err(|e| e.to_string())
}

#[test]
fn defaults_file_matches_built_in_defaults() {
    assert_eq!(parse(DEFAULT_CONFIG_TOML).unwrap(), Config::default());
    assert_eq!(
        parse("").unwrap(),
        Config::default(),
        "an empty file means defaults"
    );
}

#[test]
fn spec_example_parses() {
    let c = parse(
        r#"
[general]
enabled = true
[dns]
enabled = true
listen = ["127.0.0.1:5353"]
upstreams = ["https://cloudflare-dns.com/dns-query", "https://dns.google/dns-query"]
[filters]
enabled = true
auto_update = true
update_interval_hours = 12
[browser]
enabled = true
[privacy]
telemetry = false
"#,
    )
    .unwrap();
    assert_eq!(c.listen_addrs(), vec!["127.0.0.1:5353".parse().unwrap()]);
    assert!(
        matches!(&c.upstreams()[0], Upstream::Https { host, .. } if host == "cloudflare-dns.com")
    );
    assert_eq!(c.dns.blocking_mode, BlockingMode::Null);
}

#[test]
fn rejects_errors_with_useful_messages() {
    let cases = [
        ("[dns]\nlisten = [\"0.0.0.0:53\"]", "loopback"),
        (
            "[dns]\nupstreams = [\"http://dns.example/dns-query\"]",
            "https",
        ),
        ("[dns]\nupstreams = []", "at least one"),
        ("[dns]\nlisten = [\"not-an-ip\"]", "invalid address"),
        ("[dns]\ncache_size = 99999999", "cache_size"),
        ("[dns]\ntimeout_ms = 1", "timeout_ms"),
        ("[dns]\nbootstrap = []", "bootstrap"),
        (
            "[filters]\nupdate_interval_hours = 0",
            "update_interval_hours",
        ),
        (
            "[[filters.lists]]\nname = \"../etc\"\nurl = \"https://x/\"",
            "name",
        ),
        (
            "[[filters.lists]]\nname = \"a\"\nurl = \"http://x/list\"",
            "https",
        ),
        (
            "[[filters.lists]]\nname = \"a\"\npath = \"relative.txt\"",
            "absolute",
        ),
        (
            "[[filters.lists]]\nname = \"a\"\nbundled = \"../x\"",
            "plain file name",
        ),
        (
            "[[filters.lists]]\nname = \"a\"\nurl = \"https://x/l\"\nsha256 = \"zz\"",
            "sha256",
        ),
        (
            "[[filters.lists]]\nname = \"a\"\nurl = \"https://x/l\"\n[[filters.lists]]\nname = \"a\"\nurl = \"https://x/m\"",
            "duplicate",
        ),
        ("[[filters.lists]]\nname = \"a\"", "url, path or bundled"),
        ("[privacy]\ntelemetry = true", "telemetry"),
        ("[general]\nlog_level = \"loud\"", "log_level"),
        ("[custom]\nallow = [\"bad domain\"]", "invalid domain"),
        ("[dns.forward_zones]\nlan = [\"router\"]", "IP addresses"),
        ("[dns]\nlisten_typo = []", "unknown field"),
        ("[dns\nbroken", "syntax"),
    ];
    for (text, expected) in cases {
        let err = parse(text).expect_err(text);
        assert!(
            err.contains(expected),
            "{text:?}: expected {expected:?} in {err:?}"
        );
    }
}

#[test]
fn non_loopback_requires_explicit_opt_in() {
    assert!(parse("[dns]\nlisten = [\"192.168.1.2:53\"]\nallow_non_loopback = true").is_ok());
}

#[test]
fn plain_upstreams_and_forward_zones() {
    let c = parse("[dns]\nupstreams = [\"9.9.9.9\", \"udp://[2620:fe::fe]:53\"]\n[dns.forward_zones]\n\"Lan.\" = [\"192.168.1.1\"]").unwrap();
    assert_eq!(c.upstreams().len(), 2);
    assert_eq!(
        c.forward_zones(),
        vec![("lan".to_string(), vec!["192.168.1.1:53".parse().unwrap()])]
    );
}

#[test]
fn load_missing_file_gives_defaults() {
    assert_eq!(
        Config::load(Path::new("/nonexistent/fuck-ads.toml")).unwrap(),
        Config::default()
    );
}
