//! CLI parsing, rendering and end-to-end commands against a live daemon.

use clap::Parser;
use fuck_ads_cli::cli::{Cli, Command};
use fuck_ads_cli::main_with_args;
use fuck_ads_cli::output::{DnsRouting, Style, render_rules, render_stats, render_status};
use fuck_ads_core::protocol::{ListStatus, Request, Response, StatsInfo, StatusInfo};
use fuck_ads_tests::TestEnv;
use std::ffi::OsString;
use std::process::ExitCode;

/// Runs the real entry point on a separate thread (it owns its runtime).
fn run(args: &[&str]) -> ExitCode {
    let args: Vec<OsString> = std::iter::once("fuck-ads")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    std::thread::spawn(move || main_with_args(args))
        .join()
        .unwrap()
}

#[test]
fn every_documented_command_parses() {
    let commands = [
        "status",
        "enable",
        "disable",
        "start",
        "stop",
        "restart",
        "update",
        "rules",
        "stats",
        "logs",
        "config",
        "doctor",
        "uninstall",
        "allowlist",
        "blocklist",
        "pause",
        "resume",
    ];
    for c in commands {
        Cli::try_parse_from(["fuck-ads", c]).unwrap_or_else(|e| panic!("{c}: {e}"));
    }
    for args in [
        vec!["fuck-ads", "allow", "example.com"],
        vec!["fuck-ads", "block", "example.com"],
        vec!["fuck-ads", "remove-allow", "example.com"],
        vec!["fuck-ads", "remove-block", "example.com"],
        vec!["fuck-ads", "stats", "reset"],
        vec!["fuck-ads", "logs", "--follow"],
        vec!["fuck-ads", "update", "--force"],
        vec!["fuck-ads", "proxy", "on"],
        vec!["fuck-ads", "proxy", "off"],
        vec!["fuck-ads", "proxy", "status"],
        vec!["fuck-ads", "proxy", "show"],
        vec!["fuck-ads", "enforce-dns", "on"],
        vec!["fuck-ads", "enforce-dns", "off"],
        vec!["fuck-ads", "enforce-dns", "status"],
        vec!["fuck-ads", "enforce-dns", "show"],
    ] {
        Cli::try_parse_from(&args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
    }
    assert!(matches!(
        Cli::try_parse_from(["fuck-ads", "allow", "x.com"])
            .unwrap()
            .command,
        Command::Allow(_)
    ));
}

#[test]
fn status_output_matches_spec_layout() {
    let s = StatusInfo {
        platform: "Linux".into(),
        filtering: true,
        dns_enabled: true,
        browser_enabled: true,
        rule_count: 183_421,
        blocked_today: 12_842,
        trackers_today: 4_291,
        last_update: Some(10_000 - 240),
        browser_last_seen: Some(9_990),
        now: 10_000,
        ..Default::default()
    };
    let out = render_status(&s, DnsRouting::Active, Style { color: false });
    let expected = [
        "FuckAds",
        "Status:          ● Active",
        "Platform:        Linux",
        "DNS filtering:   ● Active",
        "Browser filter:  ● Active",
        "Filter rules:    183,421",
        "Blocked today:   12,842",
        "Trackers:        4,291",
        "Last update:     4 minutes ago",
    ];
    for line in expected {
        assert!(out.contains(line), "missing {line:?} in\n{out}");
    }
}

#[test]
fn stats_and_rules_rendering() {
    let s = StatsInfo {
        days: 1,
        queries: 40_000,
        blocked: 18_421,
        ads: 9_812,
        trackers: 8_609,
        top_blocked: vec![
            ("ads.example.com".into(), 900),
            ("tracker.example.com".into(), 500),
        ],
        ..Default::default()
    };
    let out = render_stats(&s, Style { color: false });
    assert!(out.starts_with("Today"));
    assert!(out.contains("Requests blocked:  18,421"));
    assert!(out.contains("Top blocked domains:\n  ads.example.com"));
    let lists = vec![ListStatus {
        name: "easylist".into(),
        enabled: true,
        category: "ads".into(),
        rules: 1234,
        ..Default::default()
    }];
    assert!(render_rules(&lists, Style { color: false }, 0).contains("Total: 1,234 rules"));
}

/// Read-only proxy commands against a daemon whose proxy is running. `proxy
/// on` is deliberately not run here: as root it would change the real system
/// proxy settings.
#[tokio::test(flavor = "multi_thread")]
async fn proxy_commands_report_the_running_proxy() {
    let mut env = TestEnv::new().await;
    env.config.proxy.enabled = true;
    env.config.proxy.listen = vec!["127.0.0.1:0".into()];
    env.config.paths.data_dir = Some(env.paths.data_dir.clone());
    env.config.paths.socket = Some(env.paths.socket.clone());
    let config_file = env.dir.path().join("cli-config.toml");
    std::fs::write(&config_file, env.config.to_toml()).unwrap();
    let cfg = config_file.to_str().unwrap().to_string();
    let call = |rest: &[&str]| {
        let mut v = vec!["--user", "--config", cfg.as_str(), "--no-color"];
        v.extend_from_slice(rest);
        run(&v)
    };

    // Daemon down: informational commands still work.
    assert_eq!(call(&["proxy", "status"]), ExitCode::SUCCESS);
    assert_eq!(call(&["proxy", "show"]), ExitCode::SUCCESS);

    let d = env.start().await;
    assert_eq!(d.proxy_addrs.len(), 1);
    assert_eq!(call(&["proxy", "status"]), ExitCode::SUCCESS);
    assert_eq!(call(&["--json", "proxy", "status"]), ExitCode::SUCCESS);
    assert_eq!(call(&["proxy", "show"]), ExitCode::SUCCESS);
    // Changing system settings needs root; refused otherwise.
    if !fuck_ads_platform::is_root() {
        assert_eq!(call(&["proxy", "on"]), ExitCode::FAILURE);
        assert_eq!(call(&["proxy", "off"]), ExitCode::FAILURE);
    }
    d.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn end_to_end_commands() {
    let mut env = TestEnv::new().await;
    env.config.paths.data_dir = Some(env.paths.data_dir.clone());
    env.config.paths.socket = Some(env.paths.socket.clone());
    let config_file = env.dir.path().join("cli-config.toml");
    std::fs::write(&config_file, env.config.to_toml()).unwrap();
    let cfg = config_file.to_str().unwrap().to_string();
    let base = |rest: &[&str]| {
        let mut v = vec!["--user", "--config", cfg.as_str(), "--no-color"];
        v.extend_from_slice(rest);
        v.into_iter().map(String::from).collect::<Vec<_>>()
    };
    let call = |rest: &[&str]| {
        let args = base(rest);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run(&refs)
    };

    // Not running yet: status exits with 3, mutating commands fail.
    assert_eq!(call(&["status"]), ExitCode::from(3));
    assert_eq!(call(&["allow", "example.com"]), ExitCode::FAILURE);
    assert_eq!(call(&["config", "validate"]), ExitCode::SUCCESS);

    let d = env.start().await;
    assert_eq!(call(&["status"]), ExitCode::SUCCESS);
    assert_eq!(call(&["--json", "status"]), ExitCode::SUCCESS);
    assert_eq!(call(&["rules"]), ExitCode::SUCCESS);
    assert_eq!(call(&["check", "ads.example"]), ExitCode::SUCCESS);
    assert_eq!(
        call(&["check", "https://ads.example/x.js", "--type", "script"]),
        ExitCode::SUCCESS
    );
    assert_eq!(call(&["allow", "ads.example"]), ExitCode::SUCCESS);
    assert_eq!(call(&["allowlist"]), ExitCode::SUCCESS);
    assert_eq!(call(&["allow", "not a domain"]), ExitCode::FAILURE);
    assert_eq!(call(&["remove-allow", "ads.example"]), ExitCode::SUCCESS);
    assert_eq!(call(&["block", "bad.example"]), ExitCode::SUCCESS);
    assert_eq!(call(&["blocklist"]), ExitCode::SUCCESS);
    assert_eq!(call(&["stats"]), ExitCode::SUCCESS);
    assert_eq!(call(&["stats", "reset"]), ExitCode::SUCCESS);
    assert_eq!(call(&["pause"]), ExitCode::SUCCESS);
    assert_eq!(call(&["resume"]), ExitCode::SUCCESS);
    assert_eq!(call(&["frobnicate"]), ExitCode::from(2));

    assert_eq!(
        env.control(Request::ListBlock).await,
        Response::List(vec!["bad.example".into()])
    );
    d.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_config_is_reported() {
    let env = TestEnv::new().await;
    let file = env.dir.path().join("bad.toml");
    std::fs::write(&file, "[dns]\nlisten = [\"0.0.0.0:53\"]\n").unwrap();
    let f = file.to_str().unwrap();
    assert_eq!(
        run(&["--config", f, "config", "validate"]),
        ExitCode::FAILURE
    );
    // The daemon refuses to start with EX_CONFIG so service managers do not loop.
    assert_eq!(
        run(&["--user", "--config", f, "daemon"]),
        ExitCode::from(78)
    );
}
