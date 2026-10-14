//! `fuck-ads doctor`: checks every layer and explains how to fix failures.

use crate::context::{Ctx, system_dns_routing, system_resolution_works};
use crate::output::{BROWSER_FRESH_SECS, DnsRouting, State};
use anyhow::Result;
use fuck_ads_core::protocol::{Request, Response, StatusInfo};
use fuck_ads_platform::{DnsConfigState, ServiceStatus, browser};
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Debug, Serialize)]
struct Check {
    name: &'static str,
    #[serde(serialize_with = "ser_state")]
    state: State,
    detail: String,
    fix: Option<String>,
}

fn ser_state<S: serde::Serializer>(s: &State, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(match s {
        State::Good => "ok",
        State::Warn => "warning",
        State::Bad => "error",
    })
}

fn ok(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        state: State::Good,
        detail: detail.into(),
        fix: None,
    }
}
fn warn(name: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        name,
        state: State::Warn,
        detail: detail.into(),
        fix: Some(fix.into()),
    }
}
fn bad(name: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        name,
        state: State::Bad,
        detail: detail.into(),
        fix: Some(fix.into()),
    }
}

/// Connectivity test independent of FuckAds's resolver: TCP to well-known
/// anycast resolvers on 443.
fn internet_reachable() -> bool {
    ["1.1.1.1:443", "8.8.8.8:443", "[2606:4700:4700::1111]:443"]
        .iter()
        .any(|a| {
            a.parse::<SocketAddr>()
                .map(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
                .unwrap_or(false)
        })
}

pub async fn doctor(ctx: &Ctx) -> Result<ExitCode> {
    let mut checks = Vec::new();
    let platform = fuck_ads_platform::current();

    // OS / architecture
    checks.push(ok(
        "OS supported",
        format!("{} ({})", platform.name(), std::env::consts::OS),
    ));
    if fuck_ads_platform::arch_supported() {
        checks.push(ok("Architecture supported", std::env::consts::ARCH));
    } else {
        checks.push(bad(
            "Architecture supported",
            std::env::consts::ARCH,
            "Only x86_64 and arm64 are supported.",
        ));
    }

    // Configuration
    let config = match ctx.config() {
        Ok(c) => {
            checks.push(ok(
                "Configuration valid",
                ctx.paths.config_file.display().to_string(),
            ));
            Some(c)
        }
        Err(e) => {
            checks.push(bad(
                "Configuration valid",
                format!("{e:#}"),
                format!(
                    "Edit {} and run: fuck-ads config validate",
                    ctx.paths.config_file.display()
                ),
            ));
            None
        }
    };

    // Service
    let service = platform.service_status();
    match service {
        ServiceStatus::Running => checks.push(ok(
            "Service running",
            platform.service_file().display().to_string(),
        )),
        ServiceStatus::Stopped => checks.push(bad(
            "Service running",
            "installed but stopped",
            "Run: sudo fuck-ads enable",
        )),
        ServiceStatus::NotInstalled if ctx.paths.user_mode => {
            if ctx.paths.socket.exists() {
                checks.push(ok("Service running", "per-user daemon"));
            } else {
                checks.push(bad(
                    "Service running",
                    "per-user mode: daemon not running",
                    "Run `fuck-ads --user daemon` to start it.",
                ));
            }
        }
        ServiceStatus::NotInstalled => checks.push(bad(
            "Service running",
            "the service is not installed",
            "Run the installer (installers/install.sh or install-macos.sh).",
        )),
    }

    // Control API / status
    let status: Option<Box<StatusInfo>> = match ctx.request(Request::Status).await {
        Ok(Response::Status(s)) => {
            checks.push(ok(
                "Control API reachable",
                ctx.paths.socket.display().to_string(),
            ));
            Some(s)
        }
        Ok(other) => {
            checks.push(bad(
                "Control API reachable",
                format!("{other:?}"),
                "Restart: sudo fuck-ads restart",
            ));
            None
        }
        Err(e) => {
            checks.push(bad(
                "Control API reachable",
                e.to_string(),
                "Run: sudo fuck-ads start, then check `fuck-ads logs`.",
            ));
            None
        }
    };

    // DNS listener
    let listen: Vec<SocketAddr> = match (&status, &config) {
        (Some(s), _) if s.dns_enabled => {
            s.dns_listen.iter().filter_map(|a| a.parse().ok()).collect()
        }
        (None, Some(c)) if c.dns.enabled => c.listen_addrs(),
        _ => Vec::new(),
    };
    let dns_enabled = status
        .as_ref()
        .map(|s| s.dns_enabled)
        .or(config.as_ref().map(|c| c.dns.enabled))
        .unwrap_or(true);
    let mut listener_ok = false;
    if !dns_enabled {
        checks.push(warn(
            "DNS listener available",
            "DNS filtering disabled in configuration",
            "Set [dns] enabled = true to enable system-wide blocking.",
        ));
    } else {
        let mut answering = Vec::new();
        for addr in &listen {
            if fuck_ads_dns::probe(*addr, Duration::from_secs(2)).await {
                answering.push(addr.to_string());
            }
        }
        listener_ok = !answering.is_empty();
        if listener_ok {
            checks.push(ok("DNS listener available", answering.join(", ")));
        } else {
            let fix = if status.is_some() {
                "The daemon is running but its DNS listener is not answering. Another DNS server may hold port 53 \
                 (check with: sudo lsof -nP -i :53). See `fuck-ads logs`."
            } else {
                "Start the service: sudo fuck-ads enable"
            };
            let addrs: Vec<String> = listen.iter().map(ToString::to_string).collect();
            checks.push(bad(
                "DNS listener available",
                format!("no answer on {}", addrs.join(", ")),
                fix,
            ));
        }
    }

    // System DNS routing
    if dns_enabled {
        let routing = system_dns_routing().await;
        let static_state = platform.dns_config_state(&listen);
        match (routing, &static_state) {
            (DnsRouting::Active, _) => checks.push(ok("DNS configuration correct", "system lookups pass through FuckAds")),
            (_, DnsConfigState::Active) => checks.push(warn(
                "DNS configuration correct",
                "system settings point to FuckAds, but a test lookup did not pass through it",
                "A VPN, a local DNS cache or a browser's secure-DNS setting may bypass FuckAds. See docs/troubleshooting.md.",
            )),
            (_, DnsConfigState::Inactive(detail)) => {
                let first = listen.first().map(ToString::to_string).unwrap_or_else(|| "FuckAds".into());
                let detail = if listener_ok {
                    format!("{first} is running, but the system is not using it ({detail})")
                } else {
                    detail.clone()
                };
                checks.push(bad("DNS configuration correct", detail, "Run: sudo fuck-ads enable"));
            }
        }
    }

    // Filters
    if let Some(s) = &status {
        if s.rule_count > 0 {
            checks.push(ok(
                "Filter database valid",
                format!("{} rules from {} lists", s.rule_count, s.list_count),
            ));
        } else {
            checks.push(bad(
                "Filter database valid",
                "no rules loaded",
                "Run: sudo fuck-ads update",
            ));
        }
        let interval = config
            .as_ref()
            .map(|c| u64::from(c.filters.update_interval_hours) * 3600)
            .unwrap_or(43_200);
        match s.last_update {
            Some(t) if s.now.saturating_sub(t) <= interval * 2 + 3600 => checks.push(ok(
                "Filter database up to date",
                format!("updated {}", fuck_ads_stats::time::ago(Some(t), s.now)),
            )),
            Some(t) => checks.push(warn(
                "Filter database up to date",
                format!(
                    "last successful update {}{}",
                    fuck_ads_stats::time::ago(Some(t), s.now),
                    s.last_update_error
                        .as_ref()
                        .map(|e| format!(" ({e})"))
                        .unwrap_or_default()
                ),
                "Run: sudo fuck-ads update",
            )),
            None => checks.push(warn(
                "Filter database up to date",
                s.last_update_error
                    .clone()
                    .unwrap_or_else(|| "lists not downloaded yet (bundled rules in use)".into()),
                "Run: sudo fuck-ads update",
            )),
        }
    }

    // Upstream resolvers, tested end to end through the listener.
    if listener_ok && let Some(addr) = listen.first() {
        let upstreams = status
            .as_ref()
            .map(|s| s.dns_upstreams.join(", "))
            .unwrap_or_default();
        let mut ok_any = false;
        for name in ["example.com", "cloudflare.com"] {
            if fuck_ads_dns::resolves(*addr, name, Duration::from_secs(8)).await {
                ok_any = true;
                break;
            }
        }
        if ok_any {
            checks.push(ok("Upstream DNS reachable", upstreams));
        } else {
            checks.push(bad(
                "Upstream DNS reachable",
                format!("lookups through FuckAds fail ({upstreams})"),
                "Check network access to the [dns] upstreams (HTTPS, port 443) or configure other upstreams.",
            ));
        }
    }

    // Browser
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let hosts = browser::installed(home.as_deref());
    let seen = status
        .as_ref()
        .and_then(|s| s.browser_last_seen.map(|t| s.now.saturating_sub(t)));
    match (hosts.is_empty(), seen) {
        (_, Some(age)) if age <= BROWSER_FRESH_SECS => {
            checks.push(ok("Browser integration available", "extension connected"))
        }
        (false, _) => checks.push(warn(
            "Browser integration available",
            format!(
                "native host registered for {}, extension not connected",
                hosts
                    .iter()
                    .map(|h| h.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Install/enable the FuckAds extension in your browser (see docs/browser.md).",
        )),
        (true, _) => checks.push(warn(
            "Browser integration available",
            "native messaging host not registered",
            "Run: fuck-ads browser install   (then load the extension; see docs/browser.md)",
        )),
    }

    // Internet
    let reachable = tokio::task::spawn_blocking(internet_reachable)
        .await
        .unwrap_or(false);
    let resolves = system_resolution_works().await;
    match (reachable, resolves) {
        (true, true) => checks.push(ok("Internet connectivity", "network reachable and names resolve")),
        (true, false) => checks.push(bad(
            "Internet connectivity",
            "network reachable but name resolution fails",
            "Run `sudo fuck-ads disable` to restore your previous DNS settings, then check `fuck-ads logs`.",
        )),
        (false, _) => checks.push(warn("Internet connectivity", "cannot reach the Internet", "Check your network connection.")),
    }

    let failures = checks.iter().filter(|c| c.state == State::Bad).count();
    if ctx.json {
        ctx.print_json(&checks)?;
    } else {
        for c in &checks {
            println!(
                "{} {}  {}",
                ctx.style.check(c.state),
                c.name,
                ctx.style.dim(&c.detail)
            );
            if let Some(fix) = &c.fix
                && c.state != State::Good
            {
                for line in fix.lines() {
                    println!("    {line}");
                }
            }
        }
        println!();
        if failures == 0 {
            println!("{}", ctx.style.green("No problems found."));
        } else {
            println!(
                "{}",
                ctx.style.red(&format!("{failures} problem(s) found."))
            );
        }
    }
    Ok(if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
