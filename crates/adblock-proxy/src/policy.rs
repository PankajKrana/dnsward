//! Where the proxy is willing to connect.
//!
//! The proxy is reachable only from loopback, but anything on the machine can
//! ask it to connect somewhere. Without a policy it would be a way into
//! services that only listen locally (including AdBlock's own control
//! plane) or on the private network. Targets that resolve to such addresses
//! are refused; the generated PAC file sends local destinations DIRECT, so
//! well-behaved clients never ask.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 (CGNAT)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15
        || o[0] >= 240 // reserved
}

fn forbidden_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    if (s[0] & 0xfe00) == 0xfc00 // unique local
        || (s[0] & 0xffc0) == 0xfe80 // link local
        || (s[0] & 0xffc0) == 0xfec0 // site local
        || (s[0] == 0x2001 && s[1] == 0x0db8)
    // documentation
    {
        return true;
    }
    // Embedded IPv4 (mapped, compatible, NAT64): judge the IPv4 part.
    let mapped = s[..5] == [0, 0, 0, 0, 0] && (s[5] == 0xffff || s[5] == 0);
    let nat64 = s[0] == 0x0064 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0];
    let embedded = (mapped || nat64)
        .then(|| Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8));
    embedded.is_some_and(forbidden_v4)
}

/// Whether the proxy must not connect to `ip`.
pub fn forbidden_target(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => forbidden_v6(v6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> bool {
        forbidden_target(s.parse().unwrap())
    }

    #[test]
    fn local_and_special_ranges_are_forbidden() {
        for ip in [
            "127.0.0.1",
            "127.255.0.9",
            "0.0.0.0",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
            "192.0.2.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1",
            "2001:db8::1",
        ] {
            assert!(f(ip), "{ip} should be forbidden");
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "172.32.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "198.20.0.1",
            "2606:4700:4700::1111",
            "2a00:1450:4001::200e",
            "::ffff:8.8.8.8",
        ] {
            assert!(!f(ip), "{ip} should be allowed");
        }
    }
}
