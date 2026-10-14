//! Human-readable output. Renderers are pure functions returning strings.

use adblock_core::protocol::{CheckInfo, ListStatus, StatsInfo, StatusInfo};
use adblock_stats::time::ago;

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub color: bool,
}

impl Style {
    pub fn detect(no_color: bool) -> Style {
        // SAFETY: isatty has no preconditions.
        let tty = unsafe { libc::isatty(1) } == 1;
        Style {
            color: tty && !no_color && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn green(&self, s: &str) -> String {
        self.paint("32", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.paint("31", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.paint("33", s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }

    pub fn state(&self, state: State, label: &str) -> String {
        match state {
            State::Good => self.green(&format!("● {label}")),
            State::Warn => self.yellow(&format!("● {label}")),
            State::Bad => self.red(&format!("● {label}")),
        }
    }

    pub fn check(&self, state: State) -> String {
        match state {
            State::Good => self.green("✓"),
            State::Warn => self.yellow("!"),
            State::Bad => self.red("✗"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Good,
    Warn,
    Bad,
}

pub const RULE: &str = "──────────────────────────────";

/// `1234567` → `1,234,567`.
pub fn thousands(n: impl Into<u64>) -> String {
    let s = n.into().to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn row(label: &str, value: &str) -> String {
    format!("{:<17}{}\n", format!("{label}:"), value)
}

/// What the CLI found out about system DNS routing, independent of the
/// daemon (the daemon cannot see how the OS is configured).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsRouting {
    Active,
    NotRouted,
    Unknown,
}

/// Browser considered connected if seen within this window.
pub const BROWSER_FRESH_SECS: u64 = 15 * 60;

pub fn browser_state(s: &StatusInfo) -> (State, String) {
    if !s.browser_enabled {
        return (State::Warn, "Disabled".into());
    }
    match s.browser_last_seen {
        Some(t) if s.now.saturating_sub(t) <= BROWSER_FRESH_SECS => (
            State::Good,
            match &s.browser_name {
                Some(n) if !n.is_empty() => format!("Active ({n})"),
                _ => "Active".into(),
            },
        ),
        Some(t) => (
            State::Warn,
            format!("Idle (last seen {})", ago(Some(t), s.now)),
        ),
        None => (State::Warn, "Not connected".into()),
    }
}

pub fn render_status(s: &StatusInfo, routing: DnsRouting, style: Style) -> String {
    let (dns_state, dns_label) = match (s.dns_enabled, routing) {
        (false, _) => (State::Warn, "Disabled".to_string()),
        (true, DnsRouting::Active) => (State::Good, "Active".to_string()),
        (true, DnsRouting::NotRouted) => (
            State::Bad,
            "Not in use by system (run: sudo adblock enable)".to_string(),
        ),
        (true, DnsRouting::Unknown) => (
            State::Warn,
            format!("Listening on {}", s.dns_listen.join(", ")),
        ),
    };
    let (browser_state, browser_label) = browser_state(s);
    let overall = if !s.filtering {
        (State::Warn, "Paused")
    } else if dns_state == State::Good || browser_state == State::Good {
        (State::Good, "Active")
    } else {
        (State::Bad, "Not protecting")
    };

    let mut out = String::new();
    out.push_str(&style.bold("AdBlock"));
    out.push('\n');
    out.push_str(&style.dim(RULE));
    out.push('\n');
    out.push_str(&row("Status", &style.state(overall.0, overall.1)));
    out.push_str(&row("Platform", &s.platform));
    out.push_str(&row("DNS filtering", &style.state(dns_state, &dns_label)));
    out.push_str(&row(
        "Browser filter",
        &style.state(browser_state, &browser_label),
    ));
    if let Some(addr) = s.proxy_listen.first() {
        out.push_str(&row(
            "Local proxy",
            &style.state(State::Good, &format!("Listening ({addr}/proxy.pac)")),
        ));
    }
    out.push_str(&row("Filter rules", &thousands(s.rule_count as u64)));
    out.push_str(&row("Blocked today", &thousands(s.blocked_today)));
    out.push_str(&row("Ads", &thousands(s.ads_today)));
    out.push_str(&row("Trackers", &thousands(s.trackers_today)));
    let update = if s.update_in_progress {
        "in progress…".to_string()
    } else {
        ago(s.last_update, s.now)
    };
    out.push_str(&row("Last update", &update));
    if let Some(err) = &s.last_update_error {
        out.push_str(&row("Update warning", &style.yellow(err)));
    }
    out
}

pub fn render_not_running(style: Style, installed: bool) -> String {
    let mut out = String::new();
    out.push_str(&style.bold("AdBlock"));
    out.push('\n');
    out.push_str(&style.dim(RULE));
    out.push('\n');
    out.push_str(&row("Status", &style.state(State::Bad, "Not running")));
    out.push_str(&row("Platform", adblock_core::daemon::platform_name()));
    out.push('\n');
    if installed {
        out.push_str("Start protection with:\n\n    sudo adblock enable\n");
    } else {
        out.push_str("The AdBlock service is not installed. Run the installer, then:\n\n    sudo adblock enable\n");
    }
    out
}

pub fn render_stats(s: &StatsInfo, style: Style) -> String {
    let title = if s.days <= 1 {
        "Today".to_string()
    } else {
        format!("Last {} days", s.days)
    };
    let mut out = String::new();
    out.push_str(&style.bold(&title));
    out.push('\n');
    out.push_str(&style.dim(RULE));
    out.push('\n');
    let w = [s.queries, s.blocked, s.ads, s.trackers]
        .iter()
        .map(|n| thousands(*n).len())
        .max()
        .unwrap_or(1);
    let line =
        |label: &str, n: u64| format!("{:<19}{:>w$}\n", format!("{label}:"), thousands(n), w = w);
    out.push_str(&line("DNS queries", s.queries));
    out.push_str(&line("Requests blocked", s.blocked));
    out.push_str(&line("Ads blocked", s.ads));
    out.push_str(&line("Trackers blocked", s.trackers));
    if s.malware > 0 {
        out.push_str(&line("Malware blocked", s.malware));
    }
    if s.other_blocked > 0 {
        out.push_str(&line("Other blocked", s.other_blocked));
    }
    if s.queries > 0 {
        let pct = s.blocked as f64 * 100.0 / s.queries as f64;
        out.push_str(&format!(
            "{:<19}{:>w$}\n",
            "Blocked share:",
            format!("{pct:.1}%"),
            w = w
        ));
    }
    if s.upstream_failures > 0 {
        out.push_str(&style.yellow(&line("Upstream failures", s.upstream_failures)));
    }
    if !s.top_blocked.is_empty() {
        out.push_str("\nTop blocked domains:\n");
        let width = s
            .top_blocked
            .iter()
            .map(|(d, _)| d.len())
            .max()
            .unwrap_or(0);
        for (domain, count) in &s.top_blocked {
            out.push_str(&format!("  {domain:<width$}  {}\n", thousands(*count)));
        }
    }
    out
}

pub fn render_rules(lists: &[ListStatus], style: Style, now: u64) -> String {
    let mut out = String::new();
    out.push_str(&style.bold("Filter lists"));
    out.push('\n');
    out.push_str(&style.dim(RULE));
    out.push('\n');
    let width = lists.iter().map(|l| l.name.len()).max().unwrap_or(4).max(4);
    let mut total = 0usize;
    for l in lists {
        total += l.rules;
        let state = if !l.enabled {
            style.dim("disabled")
        } else if l.last_error.is_some() {
            style.yellow("update failed")
        } else if l.rules == 0 {
            style.yellow("not loaded")
        } else {
            style.green("ok")
        };
        out.push_str(&format!(
            "{:<width$}  {:>9} rules  {:<9} {}  {}\n",
            l.name,
            thousands(l.rules as u64),
            l.category,
            state,
            style.dim(
                &if l.source.starts_with("bundled:") && l.last_success.is_none() {
                    "bundled".to_string()
                } else {
                    format!("updated {}", ago(l.last_success, now))
                }
            ),
        ));
        if let Some(e) = &l.last_error {
            out.push_str(&format!("{:<width$}  {}\n", "", style.yellow(e)));
        }
    }
    out.push_str(&format!("\nTotal: {} rules\n", thousands(total as u64)));
    out
}

pub fn render_check(c: &CheckInfo, style: Style) -> String {
    let verdict = if c.blocked {
        style.red("BLOCKED")
    } else {
        style.green("ALLOWED")
    };
    format!("{verdict}  {}\n  reason: {}\n", c.input, c.reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(thousands(0u64), "0");
        assert_eq!(thousands(999u64), "999");
        assert_eq!(thousands(1000u64), "1,000");
        assert_eq!(thousands(183421u64), "183,421");
        assert_eq!(thousands(1234567u64), "1,234,567");
    }

    #[test]
    fn status_rendering() {
        let s = StatusInfo {
            platform: "macOS".into(),
            filtering: true,
            dns_enabled: true,
            rule_count: 183_421,
            blocked_today: 12_842,
            trackers_today: 4_291,
            last_update: Some(1_000),
            now: 1_240,
            browser_enabled: true,
            browser_last_seen: Some(1_200),
            ..Default::default()
        };
        let text = render_status(&s, DnsRouting::Active, Style { color: false });
        assert!(text.contains("Status:          ● Active"));
        assert!(text.contains("Filter rules:    183,421"));
        assert!(text.contains("Last update:     4 minutes ago"));
        assert!(text.contains("Browser filter:  ● Active"));
        let text = render_status(&s, DnsRouting::NotRouted, Style { color: false });
        assert!(text.contains("sudo adblock enable"));
        assert!(
            !text.contains("Local proxy"),
            "hidden while the proxy is off"
        );
        let s = StatusInfo {
            proxy_listen: vec!["127.0.0.1:8118".into()],
            ..s
        };
        let text = render_status(&s, DnsRouting::Active, Style { color: false });
        assert!(text.contains("Local proxy:"));
        assert!(text.contains("127.0.0.1:8118/proxy.pac"));
    }
}
