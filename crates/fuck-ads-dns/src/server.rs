//! UDP and TCP listeners.

use crate::resolver::{Resolver, Transport};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;

/// Idle timeout for TCP connections (RFC 7766 suggests seconds, not minutes).
const TCP_IDLE: Duration = Duration::from_secs(10);
/// Maximum simultaneous TCP connections.
const MAX_TCP_CONNECTIONS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("could not listen on any address: {0}")]
    NoListener(String),
}

/// Bound sockets, created before privileges or configuration are applied so
/// binding errors surface early.
#[derive(Debug)]
pub struct Listeners {
    udp: Vec<Arc<UdpSocket>>,
    tcp: Vec<TcpListener>,
    pub bound: Vec<SocketAddr>,
}

/// Binds UDP and TCP sockets on every address. Fails only if no address at
/// all could be bound (e.g. `[::1]` is unavailable when IPv6 is disabled).
pub async fn bind(addrs: &[SocketAddr]) -> Result<Listeners, ServerError> {
    let mut listeners = Listeners {
        udp: Vec::new(),
        tcp: Vec::new(),
        bound: Vec::new(),
    };
    let mut errors = Vec::new();
    for addr in addrs {
        let udp = match UdpSocket::bind(addr).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(%addr, error = %e, "cannot bind UDP");
                errors.push(format!("{addr}: {e}"));
                continue;
            }
        };
        // Use the actually bound address (resolves port 0 in tests).
        let actual = udp.local_addr().unwrap_or(*addr);
        let tcp = match TcpListener::bind(actual).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(addr = %actual, error = %e, "cannot bind TCP");
                errors.push(format!("{actual}: {e}"));
                continue;
            }
        };
        listeners.udp.push(Arc::new(udp));
        listeners.tcp.push(tcp);
        listeners.bound.push(actual);
    }
    if listeners.bound.is_empty() {
        return Err(ServerError::NoListener(errors.join("; ")));
    }
    Ok(listeners)
}

/// Serves DNS until `shutdown` becomes true.
pub async fn serve(
    listeners: Listeners,
    resolver: Arc<Resolver>,
    max_concurrent: usize,
    shutdown: watch::Receiver<bool>,
) {
    let limit = Arc::new(Semaphore::new(max_concurrent));
    let tcp_limit = Arc::new(Semaphore::new(MAX_TCP_CONNECTIONS));
    let mut tasks = JoinSet::new();
    for socket in listeners.udp {
        tasks.spawn(udp_loop(
            socket,
            resolver.clone(),
            limit.clone(),
            shutdown.clone(),
        ));
    }
    for listener in listeners.tcp {
        tasks.spawn(tcp_loop(
            listener,
            resolver.clone(),
            tcp_limit.clone(),
            shutdown.clone(),
        ));
    }
    while tasks.join_next().await.is_some() {}
}

async fn udp_loop(
    socket: Arc<UdpSocket>,
    resolver: Arc<Resolver>,
    limit: Arc<Semaphore>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut buf = vec![0u8; 4096];
    loop {
        let (len, peer) = tokio::select! {
            r = socket.recv_from(&mut buf) => match r {
                Ok(v) => v,
                Err(e) => {
                    // ICMP errors from previous sends surface here on some
                    // platforms; they are not fatal.
                    tracing::debug!(error = %e, "UDP receive error");
                    continue;
                }
            },
            _ = shutdown.changed() => return,
        };
        let Ok(permit) = limit.clone().try_acquire_owned() else {
            // Overloaded: drop; the client will retry.
            continue;
        };
        let packet = buf[..len].to_vec();
        let socket = socket.clone();
        let resolver = resolver.clone();
        tokio::spawn(async move {
            if let Some(resp) = resolver.handle(&packet, Transport::Udp).await {
                let _ = socket.send_to(&resp, peer).await;
            }
            drop(permit);
        });
    }
}

async fn tcp_loop(
    listener: TcpListener,
    resolver: Arc<Resolver>,
    limit: Arc<Semaphore>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let (stream, _) = tokio::select! {
            r = listener.accept() => match r {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!(error = %e, "TCP accept error");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            _ = shutdown.changed() => return,
        };
        let Ok(permit) = limit.clone().try_acquire_owned() else {
            continue; // drops the connection
        };
        let resolver = resolver.clone();
        tokio::spawn(async move {
            let _ = tcp_connection(stream, resolver).await;
            drop(permit);
        });
    }
}

async fn tcp_connection(mut stream: TcpStream, resolver: Arc<Resolver>) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    loop {
        let len = match tokio::time::timeout(TCP_IDLE, stream.read_u16()).await {
            Ok(Ok(len)) => usize::from(len),
            _ => return Ok(()),
        };
        if len < 12 {
            return Ok(());
        }
        let mut packet = vec![0u8; len];
        tokio::time::timeout(TCP_IDLE, stream.read_exact(&mut packet))
            .await
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))??;
        if let Some(resp) = resolver.handle(&packet, Transport::Tcp).await {
            let len = u16::try_from(resp.len())
                .map_err(|_| std::io::Error::other("response too large"))?;
            let mut framed = Vec::with_capacity(resp.len() + 2);
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&resp);
            stream.write_all(&framed).await?;
        }
    }
}
