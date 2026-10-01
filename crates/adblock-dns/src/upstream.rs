//! Upstream resolvers: DNS-over-HTTPS (RFC 8484) and plain DNS (UDP with
//! TCP fallback).

use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::{Name, RData, RecordType};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

/// Largest DNS message accepted from an upstream.
pub const MAX_MESSAGE: usize = 65_535;

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("timed out")]
    Timeout,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("malformed response: {0}")]
    Malformed(String),
}

/// Sends `query` over UDP and returns the raw response, retrying over TCP
/// when the response is truncated.
pub async fn plain_exchange(
    server: SocketAddr,
    query: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, UpstreamError> {
    let response = tokio::time::timeout(timeout, udp_exchange(server, query))
        .await
        .map_err(|_| UpstreamError::Timeout)??;
    // TC bit: byte 2, mask 0x02.
    if response.len() >= 3 && response[2] & 0x02 != 0 {
        return tokio::time::timeout(timeout, tcp_exchange(server, query))
            .await
            .map_err(|_| UpstreamError::Timeout)?;
    }
    Ok(response)
}

async fn udp_exchange(server: SocketAddr, query: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    // A fresh socket per query gives each query a random source port,
    // which (with the random ID) protects against off-path spoofing.
    let bind: SocketAddr = if server.is_ipv4() {
        "0.0.0.0:0".parse().unwrap()
    } else {
        "[::]:0".parse().unwrap()
    };
    let socket = UdpSocket::bind(bind).await?;
    socket.connect(server).await?;
    socket.send(query).await?;
    let mut buf = vec![0u8; 4096];
    loop {
        let n = socket.recv(&mut buf).await?;
        // Ignore datagrams whose ID does not match (stray or spoofed).
        if n >= 12 && buf[..2] == query[..2] {
            buf.truncate(n);
            return Ok(buf);
        }
    }
}

/// Sends a length-prefixed query over TCP.
pub async fn tcp_exchange(server: SocketAddr, query: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    let mut stream = TcpStream::connect(server).await?;
    stream.set_nodelay(true)?;
    let len = u16::try_from(query.len())
        .map_err(|_| UpstreamError::Malformed("query too large".into()))?;
    let mut framed = Vec::with_capacity(query.len() + 2);
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(query);
    stream.write_all(&framed).await?;
    let len = stream.read_u16().await? as usize;
    if len < 12 {
        return Err(UpstreamError::Malformed("short TCP response".into()));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    if buf[..2] != query[..2] {
        return Err(UpstreamError::Malformed("ID mismatch".into()));
    }
    Ok(buf)
}

/// Resolves the hostnames of DoH servers using dedicated bootstrap servers.
///
/// Without this, reqwest would use the system resolver, which — once AdBlock
/// is the system resolver — is this very process: a resolution loop.
#[derive(Debug, Clone)]
pub struct BootstrapResolver {
    inner: Arc<BootstrapInner>,
}

#[derive(Debug)]
struct BootstrapInner {
    servers: Vec<SocketAddr>,
    timeout: Duration,
    cache: Mutex<HashMap<String, (Vec<IpAddr>, Instant)>>,
}

impl BootstrapResolver {
    pub fn new(servers: Vec<SocketAddr>, timeout: Duration) -> Self {
        BootstrapResolver {
            inner: Arc::new(BootstrapInner {
                servers,
                timeout,
                cache: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn lookup(&self, host: &str) -> Result<Vec<IpAddr>, UpstreamError> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        if let Some((ips, expires)) = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(host)
            && *expires > Instant::now()
        {
            return Ok(ips.clone());
        }
        let name = Name::from_ascii(format!("{host}."))
            .map_err(|e| UpstreamError::Malformed(format!("bad bootstrap name: {e}")))?;
        let mut last_err = UpstreamError::Timeout;
        for server in &self.inner.servers {
            let mut ips = Vec::new();
            let mut min_ttl = u32::MAX;
            for rtype in [RecordType::A, RecordType::AAAA] {
                let mut msg = Message::new(rand_id(), MessageType::Query, OpCode::Query);
                msg.metadata.recursion_desired = true;
                msg.add_query(Query::query(name.clone(), rtype));
                let bytes = msg
                    .to_vec()
                    .map_err(|e| UpstreamError::Malformed(e.to_string()))?;
                match plain_exchange(*server, &bytes, self.inner.timeout).await {
                    Ok(resp) => {
                        let Ok(resp) = Message::from_vec(&resp) else {
                            continue;
                        };
                        for r in &resp.answers {
                            match &r.data {
                                RData::A(a) => ips.push(IpAddr::V4(a.0)),
                                RData::AAAA(a) => ips.push(IpAddr::V6(a.0)),
                                _ => continue,
                            }
                            min_ttl = min_ttl.min(r.ttl);
                        }
                    }
                    Err(e) => last_err = e,
                }
            }
            if !ips.is_empty() {
                let ttl = Duration::from_secs(u64::from(min_ttl.clamp(60, 3_600)));
                self.inner
                    .cache
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(host.to_string(), (ips.clone(), Instant::now() + ttl));
                return Ok(ips);
            }
        }
        Err(last_err)
    }
}

impl reqwest::dns::Resolve for BootstrapResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        let this = self.clone();
        Box::pin(async move {
            let ips = this
                .lookup(&host)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            let addrs: reqwest::dns::Addrs =
                Box::new(ips.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

/// Cryptographically random 16-bit query ID.
pub fn rand_id() -> u16 {
    let mut b = [0u8; 2];
    getrandom(&mut b);
    u16::from_ne_bytes(b)
}

/// Fills `buf` from the OS CSPRNG.
fn getrandom(buf: &mut [u8]) {
    use std::io::Read;
    // /dev/urandom exists on every supported platform (Linux, macOS).
    if let Ok(mut f) = std::fs::File::open("/dev/urandom")
        && f.read_exact(buf).is_ok()
    {
        return;
    }
    // Fallback: hash of time and address entropy (should never happen).
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    let v = h.finish().to_ne_bytes();
    for (i, b) in buf.iter_mut().enumerate() {
        *b = v[i % 8];
    }
}

/// A configured upstream.
#[derive(Debug, Clone)]
pub enum UpstreamClient {
    Https {
        url: String,
        client: reqwest::Client,
    },
    Plain {
        addr: SocketAddr,
    },
}

impl UpstreamClient {
    pub fn https(
        url: &str,
        bootstrap: BootstrapResolver,
        timeout: Duration,
    ) -> Result<Self, UpstreamError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .dns_resolver(Arc::new(bootstrap))
            .timeout(timeout)
            .connect_timeout(timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("adblock/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| UpstreamError::Http(e.to_string()))?;
        Ok(UpstreamClient::Https {
            url: url.to_string(),
            client,
        })
    }

    pub fn describe(&self) -> String {
        match self {
            UpstreamClient::Https { url, .. } => url.clone(),
            UpstreamClient::Plain { addr } => addr.to_string(),
        }
    }

    /// Sends a wire-format query and returns the wire-format response.
    pub async fn exchange(
        &self,
        query: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, UpstreamError> {
        match self {
            UpstreamClient::Plain { addr } => plain_exchange(*addr, query, timeout).await,
            UpstreamClient::Https { url, client } => {
                let fut = async {
                    let resp = client
                        .post(url)
                        .header("content-type", "application/dns-message")
                        .header("accept", "application/dns-message")
                        .body(query.to_vec())
                        .send()
                        .await
                        .map_err(|e| UpstreamError::Http(e.without_url().to_string()))?;
                    if !resp.status().is_success() {
                        return Err(UpstreamError::Http(format!("status {}", resp.status())));
                    }
                    if let Some(ct) = resp.headers().get("content-type")
                        && !ct.as_bytes().starts_with(b"application/dns-message")
                    {
                        return Err(UpstreamError::Malformed("unexpected content type".into()));
                    }
                    if resp
                        .content_length()
                        .is_some_and(|l| l as usize > MAX_MESSAGE)
                    {
                        return Err(UpstreamError::Malformed("response too large".into()));
                    }
                    let mut resp = resp;
                    let mut body = Vec::new();
                    while let Some(chunk) = resp
                        .chunk()
                        .await
                        .map_err(|e| UpstreamError::Http(e.to_string()))?
                    {
                        body.extend_from_slice(&chunk);
                        if body.len() > MAX_MESSAGE {
                            return Err(UpstreamError::Malformed("response too large".into()));
                        }
                    }
                    Ok(body)
                };
                tokio::time::timeout(timeout, fut)
                    .await
                    .map_err(|_| UpstreamError::Timeout)?
            }
        }
    }
}
