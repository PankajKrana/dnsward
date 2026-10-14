//! Line-level parsing for adblock-style lists, hosts files and plain domain
//! lists.

use crate::ParseError;
use crate::cosmetic::{CosmeticFilter, find_cosmetic_separator};
use crate::domain::normalize_hostname;
use crate::network::NetworkFilter;
use serde::{Deserialize, Serialize};

/// Lines longer than this are rejected outright.
pub const MAX_LINE_LEN: usize = 8 * 1024;

/// Source format of a filter list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListFormat {
    /// Detect per line: hosts entries, plain hostnames and adblock syntax are
    /// all accepted.
    #[default]
    Auto,
    /// Adblock Plus / uBlock Origin / AdGuard syntax.
    Adblock,
    /// `/etc/hosts` format (`0.0.0.0 ads.example.com`).
    Hosts,
    /// One hostname per line.
    Domains,
}

/// A successfully parsed line.
#[derive(Debug, Clone)]
pub enum Rule {
    Network(NetworkFilter),
    Cosmetic(CosmeticFilter),
    /// Hostnames to block entirely (hosts-file entries or domain lists).
    Hosts(Vec<String>),
}

/// Hostnames that appear in hosts files for the local machine and must never
/// be treated as blocking rules.
const HOSTS_IGNORED: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "local",
    "broadcasthost",
    "ip6-localhost",
    "ip6-loopback",
    "ip6-localnet",
    "ip6-mcastprefix",
    "ip6-allnodes",
    "ip6-allrouters",
    "ip6-allhosts",
    "0.0.0.0",
];

fn is_comment(line: &str) -> bool {
    line.starts_with('!')
        || (line.starts_with('[') && line.ends_with(']'))
        || (line.starts_with('#')
            && !line.starts_with("##")
            && !line.starts_with("#@#")
            && !line.starts_with("#?#")
            && !line.starts_with("#@?#"))
}

/// Parses a single line. Returns `Ok(None)` for blank lines and comments.
pub fn parse_line(line: &str, format: ListFormat) -> Result<Option<Rule>, ParseError> {
    let line = line.trim();
    if line.is_empty() || is_comment(line) {
        return Ok(None);
    }
    if line.len() > MAX_LINE_LEN {
        return Err(ParseError::Invalid("line too long"));
    }
    match format {
        ListFormat::Hosts => parse_hosts_line(line),
        ListFormat::Domains => parse_domain_line(line),
        ListFormat::Adblock => parse_adblock_line(line),
        ListFormat::Auto => {
            if starts_with_ip(line) {
                parse_hosts_line(line)
            } else if is_plain_hostname(line) {
                parse_domain_line(line)
            } else {
                parse_adblock_line(line)
            }
        }
    }
}

fn starts_with_ip(line: &str) -> bool {
    line.split_whitespace()
        .next()
        .is_some_and(|tok| tok.parse::<std::net::IpAddr>().is_ok())
        && line.split_whitespace().nth(1).is_some()
}

fn is_plain_hostname(line: &str) -> bool {
    line.contains('.')
        && !line.contains(['/', '^', '$', '|', '*', '#', '@'])
        && normalize_hostname(line).is_some()
        && !line.ends_with('.')
}

fn strip_inline_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => line[..i].trim_end(),
        None => line,
    }
}

fn parse_hosts_line(line: &str) -> Result<Option<Rule>, ParseError> {
    let line = strip_inline_comment(line);
    let mut tokens = line.split_whitespace();
    let Some(ip) = tokens.next() else {
        return Ok(None);
    };
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(ParseError::Invalid("hosts entry without address"));
    }
    let hosts: Vec<String> = tokens
        .filter(|h| !HOSTS_IGNORED.contains(&h.to_ascii_lowercase().as_str()))
        .filter_map(normalize_hostname)
        .filter(|h| h.contains('.') && h.parse::<std::net::IpAddr>().is_err())
        .collect();
    if hosts.is_empty() {
        return Ok(None);
    }
    Ok(Some(Rule::Hosts(hosts)))
}

fn parse_domain_line(line: &str) -> Result<Option<Rule>, ParseError> {
    let line = strip_inline_comment(line);
    let line = line.strip_prefix("*.").unwrap_or(line);
    if line.is_empty() {
        return Ok(None);
    }
    if HOSTS_IGNORED.contains(&line) {
        return Ok(None);
    }
    match normalize_hostname(line) {
        Some(h) if h.contains('.') => Ok(Some(Rule::Hosts(vec![h]))),
        _ => Err(ParseError::Invalid("invalid hostname")),
    }
}

fn parse_adblock_line(line: &str) -> Result<Option<Rule>, ParseError> {
    if find_cosmetic_separator(line).is_some() {
        return CosmeticFilter::parse(line).map(|f| Some(Rule::Cosmetic(f)));
    }
    NetworkFilter::parse(line).map(|f| Some(Rule::Network(f)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_lines() {
        let Some(Rule::Hosts(h)) = parse_line(
            "0.0.0.0 ads.example.com tracker.example.com # x",
            ListFormat::Auto,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(h, vec!["ads.example.com", "tracker.example.com"]);
        assert!(
            parse_line("127.0.0.1 localhost", ListFormat::Auto)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_line("::1 ip6-localhost", ListFormat::Auto)
                .unwrap()
                .is_none()
        );
        assert!(parse_line("# comment", ListFormat::Auto).unwrap().is_none());
    }

    #[test]
    fn plain_domains() {
        assert!(matches!(
            parse_line("ads.example.com", ListFormat::Auto).unwrap(),
            Some(Rule::Hosts(_))
        ));
        assert!(matches!(
            parse_line("*.ads.example.com", ListFormat::Domains).unwrap(),
            Some(Rule::Hosts(_))
        ));
    }

    #[test]
    fn fuck_ads_lines() {
        assert!(matches!(
            parse_line("||ads.example.com^", ListFormat::Auto).unwrap(),
            Some(Rule::Network(_))
        ));
        assert!(matches!(
            parse_line("example.com##.ad", ListFormat::Auto).unwrap(),
            Some(Rule::Cosmetic(_))
        ));
        assert!(matches!(
            parse_line("##.ad", ListFormat::Auto).unwrap(),
            Some(Rule::Cosmetic(_))
        ));
        assert!(matches!(
            parse_line("###ad", ListFormat::Auto).unwrap(),
            Some(Rule::Cosmetic(_))
        ));
        assert!(
            parse_line("! Title: x", ListFormat::Auto)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_line("[Adblock Plus 2.0]", ListFormat::Auto)
                .unwrap()
                .is_none()
        );
    }
}
