//! The proxy auto-config (PAC) file clients use to find the proxy.
//!
//! Local destinations (plain host names, `.local`/`.lan`, private and IPv6
//! literals) go DIRECT, so the proxy's refusal to connect to local addresses
//! never affects normal use. Everything else goes to the proxy with a DIRECT
//! fallback, so a stopped proxy cannot take the machine offline.

use std::net::SocketAddr;

/// The PAC script for a proxy listening on `addr`.
pub fn pac_script(addr: SocketAddr) -> String {
    format!(
        r#"function FindProxyForURL(url, host) {{
  if (isPlainHostName(host) || host === "localhost" ||
      shExpMatch(host, "*.local") || shExpMatch(host, "*.lan") ||
      shExpMatch(host, "*.home.arpa") || host.indexOf(":") !== -1) {{
    return "DIRECT";
  }}
  if (/^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$/.test(host) &&
      (isInNet(host, "10.0.0.0", "255.0.0.0") ||
       isInNet(host, "172.16.0.0", "255.240.0.0") ||
       isInNet(host, "192.168.0.0", "255.255.0.0") ||
       isInNet(host, "169.254.0.0", "255.255.0.0") ||
       isInNet(host, "100.64.0.0", "255.192.0.0") ||
       isInNet(host, "127.0.0.0", "255.0.0.0"))) {{
    return "DIRECT";
  }}
  return "PROXY {addr}; DIRECT";
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_names_the_proxy_and_falls_back_to_direct() {
        let s = pac_script("127.0.0.1:8118".parse().unwrap());
        assert!(s.contains("function FindProxyForURL(url, host)"));
        assert!(s.contains(r#"return "PROXY 127.0.0.1:8118; DIRECT";"#));
        assert!(s.contains("isPlainHostName(host)"));
        assert!(s.contains("192.168.0.0"));
    }

    #[test]
    fn ipv6_listener_is_bracketed() {
        let s = pac_script("[::1]:8118".parse().unwrap());
        assert!(s.contains("PROXY [::1]:8118; DIRECT"));
    }
}
