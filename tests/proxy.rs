//! The local filtering proxy running inside the daemon, using the live
//! filter lists and user rules.

use adblock_core::protocol::{Request, Response};
use adblock_tests::*;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

async fn send(proxy: SocketAddr, raw: &str) -> String {
    let mut s = TcpStream::connect(proxy).await.unwrap();
    s.write_all(raw.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    let _ = timeout(Duration::from_secs(5), s.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}

fn status_line(resp: &str) -> &str {
    resp.lines().next().unwrap_or("")
}

async fn env_with_proxy() -> TestEnv {
    let mut env = TestEnv::new().await;
    env.config.proxy.enabled = true;
    env.config.proxy.listen = vec!["127.0.0.1:0".into()];
    env.config.custom.rules = vec!["||cdn.example/banners/".into()];
    env
}

#[tokio::test]
async fn proxy_is_off_by_default() {
    let env = TestEnv::new().await;
    let d = env.start().await;
    assert!(d.proxy_addrs.is_empty());
    d.shutdown().await;
}

#[tokio::test]
async fn blocks_hosts_from_the_live_filter_lists() {
    let env = env_with_proxy().await;
    let d = env.start().await;
    let proxy = d.proxy_addrs[0];

    let r = send(proxy, "CONNECT ads.example:443 HTTP/1.1\r\n\r\n").await;
    assert_eq!(status_line(&r), "HTTP/1.1 403 Forbidden", "{r:?}");
    // Subdomains and hosts-file entries too.
    let r = send(proxy, "CONNECT sub.tracker.example:443 HTTP/1.1\r\n\r\n").await;
    assert_eq!(status_line(&r), "HTTP/1.1 403 Forbidden", "{r:?}");
    let r = send(proxy, "CONNECT hosts.example:443 HTTP/1.1\r\n\r\n").await;
    assert_eq!(status_line(&r), "HTTP/1.1 403 Forbidden", "{r:?}");
    d.shutdown().await;
}

#[tokio::test]
async fn url_rules_apply_to_plain_http_only() {
    let env = env_with_proxy().await;
    let d = env.start().await;
    let proxy = d.proxy_addrs[0];

    // `||cdn.example/banners/` is a URL rule, not a hostname rule: it can
    // only be enforced where the URL is visible.
    let r = send(
        proxy,
        "GET http://cdn.example/banners/a.gif HTTP/1.1\r\nHost: cdn.example\r\n\r\n",
    )
    .await;
    assert_eq!(status_line(&r), "HTTP/1.1 403 Forbidden", "{r:?}");
    d.shutdown().await;
}

#[tokio::test]
async fn user_block_rules_and_stats_apply() {
    let env = env_with_proxy().await;
    let d = env.start().await;
    let proxy = d.proxy_addrs[0];

    let r = send(proxy, "CONNECT annoying.example:443 HTTP/1.1\r\n\r\n").await;
    assert_ne!(status_line(&r), "HTTP/1.1 403 Forbidden");
    let resp = env
        .control(Request::Block {
            domain: "annoying.example".into(),
        })
        .await;
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");
    let r = send(proxy, "CONNECT www.annoying.example:443 HTTP/1.1\r\n\r\n").await;
    assert_eq!(status_line(&r), "HTTP/1.1 403 Forbidden", "{r:?}");

    // Blocked proxy requests are counted; allowed ones are not recorded.
    let Response::Status(status) = env.control(Request::Status).await else {
        panic!("expected status");
    };
    assert_eq!(status.proxy_listen, vec![proxy.to_string()]);
    assert!(status.blocked_today >= 1);
    d.shutdown().await;
}

#[tokio::test]
async fn pausing_filtering_lets_blocked_hosts_through_the_proxy() {
    let env = env_with_proxy().await;
    let d = env.start().await;
    let proxy = d.proxy_addrs[0];
    d.daemon.filter.set_enabled(false);
    let r = send(proxy, "CONNECT ads.example:443 HTTP/1.1\r\n\r\n").await;
    assert_ne!(status_line(&r), "HTTP/1.1 403 Forbidden", "{r:?}");
    d.shutdown().await;
}
