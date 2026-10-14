//! End-to-end tests with real sockets on loopback.

use fuck_ads_proxy::{Category, ProxyConfig, ProxyFilter, Verdict, bind, serve};
use fuck_ads_stats::StatsStore;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::timeout;

/// Blocks hosts ending in `.ads.test` and URLs containing `/banner`, and
/// remembers the referers it saw.
#[derive(Default)]
struct TestFilter {
    referers: Mutex<Vec<Option<String>>>,
}

impl ProxyFilter for TestFilter {
    fn check_host(&self, host: &str) -> Verdict {
        if host.ends_with(".ads.test") {
            Verdict::Block {
                domain: "ads.test".into(),
                category: Category::Ads,
            }
        } else {
            Verdict::Allow
        }
    }
    fn check_url(&self, url: &str, referer: Option<&str>) -> Verdict {
        self.referers
            .lock()
            .unwrap()
            .push(referer.map(String::from));
        if url.contains("/banner") {
            Verdict::Block {
                domain: "example".into(),
                category: Category::Trackers,
            }
        } else {
            Verdict::Allow
        }
    }
}

struct Proxy {
    addr: SocketAddr,
    filter: Arc<TestFilter>,
    stats: Arc<StatsStore>,
    _shutdown: watch::Sender<bool>,
}

async fn start(origin_port: u16, tweak: impl FnOnce(&mut ProxyConfig)) -> Proxy {
    let listeners = bind(&["127.0.0.1:0".parse().unwrap()]).await.unwrap();
    let addr = listeners.bound[0];
    let filter = Arc::new(TestFilter::default());
    let stats = Arc::new(StatsStore::open(None, true, true));
    let mut cfg = ProxyConfig {
        ports: vec![origin_port],
        allow_local_targets: true, // the test origins live on loopback
        ..ProxyConfig::default()
    };
    tweak(&mut cfg);
    let (tx, rx) = watch::channel(false);
    tokio::spawn(serve(listeners, filter.clone(), stats.clone(), cfg, rx));
    Proxy {
        addr,
        filter,
        stats,
        _shutdown: tx,
    }
}

/// An origin that records what it receives and answers per connection.
async fn origin(reply: &'static [u8]) -> (u16, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            let log = log.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                log.lock().unwrap().push(buf[..n].to_vec());
                let _ = s.write_all(reply).await;
                let _ = s.shutdown().await;
            });
        }
    });
    (port, seen)
}

async fn read_all(s: &mut TcpStream) -> String {
    let mut out = Vec::new();
    let _ = timeout(Duration::from_secs(5), s.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}

async fn request(proxy: &Proxy, raw: &str) -> String {
    let mut s = TcpStream::connect(proxy.addr).await.unwrap();
    s.write_all(raw.as_bytes()).await.unwrap();
    read_all(&mut s).await
}

#[tokio::test]
async fn connect_tunnels_bytes_untouched() {
    let (port, seen) = origin(b"pong").await;
    let proxy = start(port, |_| {}).await;
    let mut s = TcpStream::connect(proxy.addr).await.unwrap();
    s.write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
        .await
        .unwrap();
    // Read the proxy's status line, then talk through the tunnel.
    let mut status = [0u8; 39];
    s.read_exact(&mut status).await.unwrap();
    assert_eq!(&status, b"HTTP/1.1 200 Connection Established\r\n\r\n");
    s.write_all(b"ping").await.unwrap();
    assert_eq!(read_all(&mut s).await, "pong");
    assert_eq!(seen.lock().unwrap()[0], b"ping");
}

#[tokio::test]
async fn early_data_after_connect_head_is_forwarded() {
    let (port, seen) = origin(b"ok").await;
    let proxy = start(port, |_| {}).await;
    let mut s = TcpStream::connect(proxy.addr).await.unwrap();
    s.write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\nhello").as_bytes())
        .await
        .unwrap();
    let out = read_all(&mut s).await;
    assert!(out.ends_with("ok"), "{out:?}");
    assert_eq!(seen.lock().unwrap()[0], b"hello");
}

#[tokio::test]
async fn blocked_host_is_refused_without_contacting_anyone() {
    let (port, seen) = origin(b"never").await;
    let proxy = start(port, |_| {}).await;
    let out = request(
        &proxy,
        &format!("CONNECT tracker.ads.test:{port} HTTP/1.1\r\n\r\n"),
    )
    .await;
    assert!(out.starts_with("HTTP/1.1 403 Forbidden"), "{out:?}");
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(proxy.stats.today().blocked, 1);
}

#[tokio::test]
async fn ports_outside_the_allowlist_are_refused() {
    let (port, seen) = origin(b"never").await;
    let proxy = start(port, |_| {}).await;
    let out = request(&proxy, "CONNECT 127.0.0.1:22 HTTP/1.1\r\n\r\n").await;
    assert!(out.starts_with("HTTP/1.1 403 Forbidden"), "{out:?}");
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn local_targets_are_refused_by_default() {
    let (port, seen) = origin(b"secret").await;
    let proxy = start(port, |c| c.allow_local_targets = false).await;
    for target in [
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
        format!("10.1.2.3:{port}"),
        format!("169.254.169.254:{port}"),
    ] {
        let out = request(&proxy, &format!("CONNECT {target} HTTP/1.1\r\n\r\n")).await;
        assert!(
            out.starts_with("HTTP/1.1 403 Forbidden"),
            "{target}: {out:?}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn plain_http_is_rewritten_and_forwarded() {
    let (port, seen) = origin(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi").await;
    let proxy = start(port, |_| {}).await;
    let out = request(
        &proxy,
        &format!(
            "GET http://127.0.0.1:{port}/page?a=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nProxy-Connection: keep-alive\r\nReferer: http://news.example/\r\n\r\n"
        ),
    )
    .await;
    assert!(out.ends_with("hi"), "{out:?}");
    let got = String::from_utf8(seen.lock().unwrap()[0].clone()).unwrap();
    assert!(got.starts_with("GET /page?a=1 HTTP/1.1\r\n"), "{got:?}");
    assert!(!got.to_ascii_lowercase().contains("proxy-"));
    assert!(got.contains("Connection: close"));
    assert_eq!(
        proxy.filter.referers.lock().unwrap()[0].as_deref(),
        Some("http://news.example/")
    );
}

#[tokio::test]
async fn plain_http_url_filters_apply() {
    let (port, seen) = origin(b"never").await;
    let proxy = start(port, |_| {}).await;
    let out = request(
        &proxy,
        &format!("GET http://127.0.0.1:{port}/banner/1.gif HTTP/1.1\r\nHost: x\r\n\r\n"),
    )
    .await;
    assert!(out.starts_with("HTTP/1.1 403 Forbidden"), "{out:?}");
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(proxy.stats.today().blocked, 1);
}

#[tokio::test]
async fn pac_file_is_served() {
    let proxy = start(1, |_| {}).await;
    let out = request(&proxy, "GET /proxy.pac HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;
    assert!(out.starts_with("HTTP/1.1 200 OK"), "{out:?}");
    assert!(out.contains("application/x-ns-proxy-autoconfig"));
    assert!(out.contains(&format!("PROXY {}; DIRECT", proxy.addr)));
}

#[tokio::test]
async fn bad_requests_get_clear_errors() {
    let proxy = start(1, |_| {}).await;
    assert!(
        request(&proxy, "NOT A REQUEST\r\n\r\n")
            .await
            .starts_with("HTTP/1.1 400")
    );
    assert!(
        request(&proxy, "GET /elsewhere HTTP/1.1\r\n\r\n")
            .await
            .starts_with("HTTP/1.1 501")
    );
    assert!(
        request(&proxy, "GET https://example.com/ HTTP/1.1\r\n\r\n")
            .await
            .starts_with("HTTP/1.1 501")
    );
    let huge = format!(
        "GET http://a/ HTTP/1.1\r\nX: {}\r\n\r\n",
        "a".repeat(20_000)
    );
    assert!(request(&proxy, &huge).await.starts_with("HTTP/1.1 431"));
}

#[tokio::test]
async fn connection_limit_is_enforced() {
    let (port, _) = origin(b"x").await;
    let proxy = start(port, |c| c.max_connections = 1).await;
    // First client holds the only slot by never finishing its request.
    let mut hold = TcpStream::connect(proxy.addr).await.unwrap();
    hold.write_all(b"CONNECT ").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // The second is closed without a response.
    let mut second = TcpStream::connect(proxy.addr).await.unwrap();
    let _ = second.write_all(b"GET /proxy.pac HTTP/1.1\r\n\r\n").await;
    assert_eq!(read_all(&mut second).await, "");
}

#[tokio::test]
async fn only_loopback_addresses_can_be_bound() {
    assert!(bind(&["0.0.0.0:0".parse().unwrap()]).await.is_err());
    assert!(bind(&["192.168.1.5:0".parse().unwrap()]).await.is_err());
    assert!(bind(&["127.0.0.1:0".parse().unwrap()]).await.is_ok());
}
