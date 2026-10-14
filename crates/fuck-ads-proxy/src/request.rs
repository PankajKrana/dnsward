//! Parsing of the first request on a proxy connection.
//!
//! Deliberately small and strict: one request head, hard size limits, no
//! obsolete line folding, no control characters. Anything unusual is
//! rejected instead of interpreted.

use fuck_ads_filter::domain::normalize_hostname;
use std::net::IpAddr;

/// Largest request head accepted (request line plus headers).
pub const MAX_HEAD: usize = 16 * 1024;
const MAX_HEADERS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("malformed request")]
    Malformed,
    #[error("request head too large")]
    TooLarge,
    #[error("unsupported request")]
    Unsupported,
}

/// What the client asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `CONNECT host:port`: an opaque tunnel (TLS passes through untouched).
    Connect { host: String, port: u16 },
    /// `GET http://host[:port]/path ...`: a plain HTTP request.
    Http {
        host: String,
        port: u16,
        /// Absolute URL, used for filter matching.
        url: String,
        /// Path and query, for the request forwarded to the origin.
        path: String,
    },
    /// A request for the proxy's own PAC file.
    Pac,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub version: String,
    pub target: Target,
    pub headers: Vec<(String, String)>,
    /// Bytes consumed from the buffer (the head including the blank line).
    pub len: usize,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

/// Parses `host[:port]` (IPv6 in brackets). `default_port` applies when the
/// port is omitted.
pub fn parse_authority(s: &str, default_port: Option<u16>) -> Option<(String, u16)> {
    if s.is_empty() || s.len() > 262 || s.contains(['@', '/', '?', '#', ' ']) {
        return None;
    }
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        let port = match tail {
            "" => None,
            t => Some(t.strip_prefix(':')?),
        };
        (h, port)
    } else {
        match s.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => (h, Some(p)),
            Some(_) => return None, // bare IPv6 without brackets
            None => (s, None),
        }
    };
    let port = match port {
        Some(p) => {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            p.parse::<u16>().ok().filter(|p| *p != 0)?
        }
        None => default_port?,
    };
    let host = if let Ok(ip) = host.parse::<IpAddr>() {
        ip.to_string()
    } else {
        normalize_hostname(host)?
    };
    Some((host, port))
}

fn parse_target(method: &str, target: &str) -> Result<Target, ParseError> {
    if method == "CONNECT" {
        let (host, port) = parse_authority(target, None).ok_or(ParseError::Malformed)?;
        return Ok(Target::Connect { host, port });
    }
    if let Some(rest) = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("HTTP://"))
    {
        let end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        let (host, port) = parse_authority(authority, Some(80)).ok_or(ParseError::Malformed)?;
        let tail = tail.split('#').next().unwrap_or("");
        let path = if tail.is_empty() {
            "/".to_string()
        } else if tail.starts_with('?') {
            format!("/{tail}")
        } else {
            tail.to_string()
        };
        let shown_host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.clone()
        };
        let url = if port == 80 {
            format!("http://{shown_host}{path}")
        } else {
            format!("http://{shown_host}:{port}{path}")
        };
        return Ok(Target::Http {
            host,
            port,
            url,
            path,
        });
    }
    if target.starts_with("https://") || target.starts_with("HTTPS://") {
        // Proxies tunnel HTTPS with CONNECT; an absolute https URL here would
        // mean sending TLS traffic in the clear.
        return Err(ParseError::Unsupported);
    }
    if (method == "GET" || method == "HEAD") && target.split('?').next() == Some("/proxy.pac") {
        return Ok(Target::Pac);
    }
    Err(ParseError::Unsupported)
}

/// Parses a request head from `buf`. Returns `Ok(None)` if the head is not
/// complete yet.
pub fn parse_head(buf: &[u8]) -> Result<Option<Head>, ParseError> {
    let search = &buf[..buf.len().min(MAX_HEAD)];
    let Some(end) = search.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() >= MAX_HEAD {
            Err(ParseError::TooLarge)
        } else {
            Ok(None)
        };
    };
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| ParseError::Malformed)?;
    if text
        .bytes()
        .any(|b| (b < 0x20 && b != b'\r' && b != b'\n') || b == 0x7f)
    {
        return Err(ParseError::Malformed);
    }
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or(ParseError::Malformed)?;
    let mut parts = request_line.split(' ');
    let (method, target, version) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v), None) => (m, t, v),
        _ => return Err(ParseError::Malformed),
    };
    if !is_token(method) || !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(ParseError::Malformed);
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.starts_with([' ', '\t']) || line.contains('\n') || line.contains('\r') {
            return Err(ParseError::Malformed); // obsolete folding / bare CR or LF
        }
        let (name, value) = line.split_once(':').ok_or(ParseError::Malformed)?;
        if !is_token(name) {
            return Err(ParseError::Malformed);
        }
        if headers.len() >= MAX_HEADERS {
            return Err(ParseError::TooLarge);
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    Ok(Some(Head {
        method: method.to_string(),
        version: version.to_string(),
        target: parse_target(method, target)?,
        headers,
        len: end + 4,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Option<Head>, ParseError> {
        parse_head(s.as_bytes())
    }

    #[test]
    fn connect() {
        let h = parse("CONNECT Example.COM:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            h.target,
            Target::Connect {
                host: "example.com".into(),
                port: 443
            }
        );
        assert_eq!(h.header("host"), Some("example.com:443"));
        let h = parse("CONNECT [2001:db8::1]:8443 HTTP/1.1\r\n\r\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            h.target,
            Target::Connect {
                host: "2001:db8::1".into(),
                port: 8443
            }
        );
        for bad in [
            "CONNECT example.com HTTP/1.1\r\n\r\n",
            "CONNECT example.com:0 HTTP/1.1\r\n\r\n",
            "CONNECT example.com:99999 HTTP/1.1\r\n\r\n",
            "CONNECT user@example.com:443 HTTP/1.1\r\n\r\n",
            "CONNECT 2001:db8::1:443 HTTP/1.1\r\n\r\n",
            "CONNECT exa mple.com:443 HTTP/1.1\r\n\r\n",
            "CONNECT :443 HTTP/1.1\r\n\r\n",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn absolute_http_requests() {
        let h =
            parse("GET http://Ads.Example.com:8080/a/b.js?x=1#frag HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap()
                .unwrap();
        assert_eq!(
            h.target,
            Target::Http {
                host: "ads.example.com".into(),
                port: 8080,
                url: "http://ads.example.com:8080/a/b.js?x=1".into(),
                path: "/a/b.js?x=1".into(),
            }
        );
        let h = parse("GET http://example.com HTTP/1.1\r\n\r\n")
            .unwrap()
            .unwrap();
        assert!(matches!(&h.target, Target::Http { path, url, .. }
            if path == "/" && url == "http://example.com/"));
        let h = parse("GET http://example.com?q=1 HTTP/1.1\r\n\r\n")
            .unwrap()
            .unwrap();
        assert!(matches!(&h.target, Target::Http { path, .. } if path == "/?q=1"));
        // Credentials in the URL, and absolute https, are refused.
        assert!(parse("GET http://u:p@example.com/ HTTP/1.1\r\n\r\n").is_err());
        assert_eq!(
            parse("GET https://example.com/ HTTP/1.1\r\n\r\n"),
            Err(ParseError::Unsupported)
        );
    }

    #[test]
    fn pac_and_unsupported() {
        assert_eq!(
            parse("GET /proxy.pac HTTP/1.1\r\n\r\n")
                .unwrap()
                .unwrap()
                .target,
            Target::Pac
        );
        assert_eq!(
            parse("GET /other HTTP/1.1\r\n\r\n"),
            Err(ParseError::Unsupported)
        );
        assert_eq!(
            parse("POST /proxy.pac HTTP/1.1\r\n\r\n"),
            Err(ParseError::Unsupported)
        );
    }

    #[test]
    fn incomplete_and_oversized() {
        assert_eq!(parse("CONNECT example.com:443 HTTP/1.1\r\nHo"), Ok(None));
        let big = format!("GET http://a/ HTTP/1.1\r\nX: {}", "a".repeat(MAX_HEAD));
        assert_eq!(parse(&big), Err(ParseError::TooLarge));
        let many: String = (0..=MAX_HEADERS).map(|i| format!("H{i}: v\r\n")).collect();
        assert_eq!(
            parse(&format!("GET http://a/ HTTP/1.1\r\n{many}\r\n")),
            Err(ParseError::TooLarge)
        );
    }

    #[test]
    fn malformed_heads() {
        for bad in [
            "GET http://a/ HTTP/2\r\n\r\n",
            "GET http://a/ HTTP/1.1 extra\r\n\r\n",
            "GET  http://a/ HTTP/1.1\r\n\r\n",
            "G\u{0}T http://a/ HTTP/1.1\r\n\r\n",
            "GET http://a/ HTTP/1.1\r\nBad Header: x\r\n\r\n",
            "GET http://a/ HTTP/1.1\r\nA: b\r\n folded\r\n\r\n",
            "GET http://a/ HTTP/1.1\r\nNoColon\r\n\r\n",
            "GET http://a/ HTTP/1.1\r\nA: b\u{7f}\r\n\r\n",
            "GET http://a/ HTTP/1.1\nA: b\r\n\r\n",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    /// Garbage and mutated valid heads must never panic, and anything that
    /// parses must satisfy the invariants the server relies on.
    #[test]
    fn randomized_inputs_never_panic() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let seeds: [&[u8]; 4] = [
            b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com\r\n\r\n",
            b"GET http://example.com:8080/a?b=c HTTP/1.1\r\nHost: x\r\nReferer: http://y/\r\n\r\n",
            b"GET /proxy.pac HTTP/1.0\r\n\r\n",
            b"CONNECT [::1]:443 HTTP/1.1\r\n\r\n",
        ];
        for _ in 0..50_000 {
            let mut buf: Vec<u8> = if next() % 4 == 0 {
                (0..next() % 200).map(|_| next() as u8).collect()
            } else {
                seeds[(next() % 4) as usize].to_vec()
            };
            for _ in 0..(next() % 6) {
                if buf.is_empty() {
                    break;
                }
                let i = (next() as usize) % buf.len();
                match next() % 3 {
                    0 => buf[i] = next() as u8,
                    1 => buf.insert(i, next() as u8),
                    _ => {
                        buf.remove(i);
                    }
                }
            }
            if let Ok(Some(head)) = parse_head(&buf) {
                assert!(head.len <= buf.len());
                match head.target {
                    Target::Connect { host, port } | Target::Http { host, port, .. } => {
                        assert!(!host.is_empty() && port != 0);
                        assert!(!host.contains(['@', '/', ' ']));
                    }
                    Target::Pac => {}
                }
            }
        }
    }

    #[test]
    fn head_length_excludes_body() {
        let raw = "POST http://a/ HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc";
        let h = parse(raw).unwrap().unwrap();
        assert_eq!(&raw[h.len..], "abc");
    }
}
