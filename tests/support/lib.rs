//! Shared helpers for integration tests: a mock upstream DNS server, a mock
//! filter-list fetcher and an in-process daemon bound to random ports.

use adblock_config::{Config, FilterList, Paths};
use adblock_core::control;
use adblock_core::protocol::{Request, Response};
use adblock_core::update::{BoxFuture, FetchRequest, FetchResponse, Fetcher};
use adblock_core::{Daemon, RunningDaemon};
use adblock_filter::{Category, ListFormat};
use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::rdata::{A, AAAA, CNAME};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UdpSocket;

pub const UPSTREAM_V4: Ipv4Addr = Ipv4Addr::new(93, 184, 216, 34);
pub const UPSTREAM_V6: Ipv6Addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);

/// A fake recursive resolver on 127.0.0.1 (UDP only).
///
/// * `cloaked.example` → CNAME `tracker.example` → A
/// * `nx.example` → NXDOMAIN
/// * everything else → A [`UPSTREAM_V4`] / AAAA [`UPSTREAM_V6`], TTL 300
pub struct MockUpstream {
    pub addr: SocketAddr,
    pub queries: Arc<AtomicUsize>,
    pub silent: Arc<AtomicBool>,
    pub last_query: Arc<Mutex<Option<Message>>>,
}

impl MockUpstream {
    pub async fn start() -> MockUpstream {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let queries = Arc::new(AtomicUsize::new(0));
        let silent = Arc::new(AtomicBool::new(false));
        let last_query = Arc::new(Mutex::new(None));
        let (q, s, l) = (queries.clone(), silent.clone(), last_query.clone());
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                let Ok((n, peer)) = socket.recv_from(&mut buf).await else {
                    continue;
                };
                q.fetch_add(1, Ordering::SeqCst);
                if s.load(Ordering::SeqCst) {
                    continue;
                }
                let Ok(req) = Message::from_vec(&buf[..n]) else {
                    continue;
                };
                *l.lock().unwrap() = Some(req.clone());
                let resp = answer(&req);
                let _ = socket.send_to(&resp.to_vec().unwrap(), peer).await;
            }
        });
        MockUpstream {
            addr,
            queries,
            silent,
            last_query,
        }
    }

    pub fn count(&self) -> usize {
        self.queries.load(Ordering::SeqCst)
    }
}

fn answer(req: &Message) -> Message {
    let mut resp = Message::new(req.metadata.id, MessageType::Response, OpCode::Query);
    resp.metadata.recursion_desired = true;
    resp.metadata.recursion_available = true;
    resp.queries = req.queries.clone();
    resp.edns = req.edns.clone();
    let q = &req.queries[0];
    let name = q.name().clone();
    let host = name.to_ascii().to_ascii_lowercase();
    if host == "nx.example." {
        resp.metadata.response_code = ResponseCode::NXDomain;
        return resp;
    }
    if host == "cloaked.example." {
        let target = Name::from_ascii("tracker.example.").unwrap();
        resp.add_answer(Record::from_rdata(
            name,
            300,
            RData::CNAME(CNAME(target.clone())),
        ));
        resp.add_answer(Record::from_rdata(target, 300, RData::A(A(UPSTREAM_V4))));
        return resp;
    }
    match q.query_type() {
        RecordType::A => {
            resp.add_answer(Record::from_rdata(name, 300, RData::A(A(UPSTREAM_V4))));
        }
        RecordType::AAAA => {
            resp.add_answer(Record::from_rdata(
                name,
                300,
                RData::AAAA(AAAA(UPSTREAM_V6)),
            ));
        }
        _ => {}
    }
    resp
}

/// Serves list content from memory; responses can be changed between calls.
#[derive(Default)]
pub struct MockFetcher {
    pub responses: Mutex<HashMap<String, Result<FetchResponse, String>>>,
    pub requests: Mutex<Vec<(String, Option<String>)>>,
}

impl MockFetcher {
    pub fn set_body(&self, url: &str, body: &str, etag: Option<&str>) {
        self.responses.lock().unwrap().insert(
            url.to_string(),
            Ok(FetchResponse::Body {
                bytes: body.as_bytes().to_vec(),
                etag: etag.map(String::from),
                last_modified: None,
            }),
        );
    }
    pub fn set_bytes(&self, url: &str, bytes: Vec<u8>) {
        self.responses.lock().unwrap().insert(
            url.to_string(),
            Ok(FetchResponse::Body {
                bytes,
                etag: None,
                last_modified: None,
            }),
        );
    }
    pub fn set(&self, url: &str, resp: Result<FetchResponse, String>) {
        self.responses.lock().unwrap().insert(url.to_string(), resp);
    }
}

impl Fetcher for MockFetcher {
    fn fetch<'a>(&'a self, req: FetchRequest<'a>) -> BoxFuture<'a, Result<FetchResponse, String>> {
        self.requests
            .lock()
            .unwrap()
            .push((req.url.to_string(), req.etag.map(String::from)));
        let resp = self
            .responses
            .lock()
            .unwrap()
            .get(req.url)
            .cloned()
            .unwrap_or(Err("404".into()));
        Box::pin(async move { resp })
    }
}

/// A list served by [`MockFetcher`].
pub fn remote_list(name: &str, category: Category, min_rules: usize) -> FilterList {
    FilterList {
        name: name.into(),
        url: Some(format!("https://lists.test/{name}.txt")),
        bundled: None,
        path: None,
        category,
        format: ListFormat::Auto,
        enabled: true,
        sha256: None,
        min_rules,
    }
}

/// A list read from a local file.
pub fn local_list(name: &str, path: &Path, category: Category) -> FilterList {
    FilterList {
        name: name.into(),
        url: None,
        bundled: None,
        path: Some(path.to_path_buf()),
        category,
        format: ListFormat::Auto,
        enabled: true,
        sha256: None,
        min_rules: 1,
    }
}

pub const DEFAULT_RULES: &str = "\
! ads
||ads.example^
||tracker.example^
0.0.0.0 hosts.example
@@||good.ads.example^
example.org##.ad-banner
";

pub struct TestEnv {
    pub dir: tempfile::TempDir,
    pub paths: Paths,
    pub config: Config,
    pub upstream: MockUpstream,
    pub fetcher: Arc<MockFetcher>,
}

impl TestEnv {
    /// Creates an environment with one local list (`DEFAULT_RULES`,
    /// category ads) and a mock plain-DNS upstream.
    pub async fn new() -> TestEnv {
        let dir = tempfile::tempdir().unwrap();
        let upstream = MockUpstream::start().await;
        let list_path = dir.path().join("local.txt");
        std::fs::write(&list_path, DEFAULT_RULES).unwrap();

        let mut config = Config::default();
        config.dns.listen = vec!["127.0.0.1:0".into(), "[::1]:0".into()];
        config.dns.upstreams = vec![upstream.addr.to_string()];
        config.dns.bootstrap = vec![];
        config.dns.timeout_ms = 500;
        config.filters.auto_update = false;
        config.filters.lists = vec![local_list("local", &list_path, Category::Ads)];

        let data = dir.path().join("data");
        let paths = Paths {
            config_file: dir.path().join("config.toml"),
            system_dir: dir.path().join("system"),
            socket: data.join("c.sock"),
            data_dir: data,
            bundled_filters: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filters"),
            log_file: None,
            user_mode: true,
        };
        TestEnv {
            dir,
            paths,
            config,
            upstream,
            fetcher: Arc::new(MockFetcher::default()),
        }
    }

    pub async fn start(&self) -> RunningDaemon {
        self.config.validate().expect("test config must be valid");
        Daemon::start(
            self.config.clone(),
            self.paths.clone(),
            Some(self.fetcher.clone()),
        )
        .await
        .unwrap()
    }

    pub async fn control(&self, req: Request) -> Response {
        control::request(&self.paths.socket, &req, Duration::from_secs(30))
            .await
            .unwrap()
    }
}

/// Sends a DNS query over UDP and returns the parsed response.
pub async fn query(server: SocketAddr, name: &str, rtype: RecordType) -> Option<Message> {
    let mut msg = Message::new(rand_id(), MessageType::Query, OpCode::Query);
    msg.metadata.recursion_desired = true;
    msg.add_query(Query::query(Name::from_ascii(name).unwrap(), rtype));
    let bytes = msg.to_vec().unwrap();
    let resp = adblock_dns::upstream::plain_exchange(server, &bytes, Duration::from_secs(3))
        .await
        .ok()?;
    Message::from_vec(&resp).ok()
}

/// Sends a DNS query over TCP.
pub async fn query_tcp(server: SocketAddr, name: &str, rtype: RecordType) -> Option<Message> {
    let mut msg = Message::new(rand_id(), MessageType::Query, OpCode::Query);
    msg.add_query(Query::query(Name::from_ascii(name).unwrap(), rtype));
    let resp = adblock_dns::upstream::tcp_exchange(server, &msg.to_vec().unwrap())
        .await
        .ok()?;
    Message::from_vec(&resp).ok()
}

pub fn rand_id() -> u16 {
    adblock_dns::upstream::rand_id()
}

pub fn first_ip(msg: &Message) -> Option<std::net::IpAddr> {
    msg.answers.iter().find_map(|r| match &r.data {
        RData::A(a) => Some(a.0.into()),
        RData::AAAA(a) => Some(a.0.into()),
        _ => None,
    })
}
