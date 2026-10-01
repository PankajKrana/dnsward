//! Filtering DNS resolver.
//!
//! * Listens on loopback UDP and TCP.
//! * Consults a [`QueryFilter`] for each query (and for CNAME targets in
//!   responses, to defeat CNAME cloaking).
//! * Forwards allowed queries to DNS-over-HTTPS or plain upstreams, with
//!   failover, response validation and a TTL-respecting cache.
//! * Never forwards special-use local names or private reverse lookups.
//! * Records only aggregate statistics (see `adblock-stats`).

pub mod cache;
pub mod resolver;
pub mod server;
pub mod upstream;

pub use resolver::{
    Decision, Metrics, PROBE_ADDR, PROBE_NAME, QueryFilter, Resolver, ResolverConfig, Transport,
};
pub use server::{Listeners, ServerError, bind, serve};

/// Sends an A query for [`PROBE_NAME`] directly to `server` and reports
/// whether the AdBlock resolver answered. Used by health checks.
pub async fn probe(server: std::net::SocketAddr, timeout: std::time::Duration) -> bool {
    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RData, RecordType};
    let Ok(name) = Name::from_ascii(format!("{PROBE_NAME}.")) else {
        return false;
    };
    let mut msg = Message::new(upstream::rand_id(), MessageType::Query, OpCode::Query);
    msg.metadata.recursion_desired = true;
    msg.add_query(Query::query(name, RecordType::A));
    let Ok(bytes) = msg.to_vec() else {
        return false;
    };
    match upstream::plain_exchange(server, &bytes, timeout).await {
        Ok(resp) => Message::from_vec(&resp)
            .map(|m| {
                m.answers
                    .iter()
                    .any(|r| matches!(&r.data, RData::A(a) if a.0 == PROBE_ADDR))
            })
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Resolves `name` (A record) through `server` and reports whether a
/// successful answer came back. Used by `adblock doctor` to test upstreams
/// end to end.
pub async fn resolves(
    server: std::net::SocketAddr,
    name: &str,
    timeout: std::time::Duration,
) -> bool {
    use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
    use hickory_proto::rr::{Name, RecordType};
    let Ok(qname) = Name::from_ascii(format!("{}.", name.trim_end_matches('.'))) else {
        return false;
    };
    let mut msg = Message::new(upstream::rand_id(), MessageType::Query, OpCode::Query);
    msg.metadata.recursion_desired = true;
    msg.add_query(Query::query(qname, RecordType::A));
    let Ok(bytes) = msg.to_vec() else {
        return false;
    };
    match upstream::plain_exchange(server, &bytes, timeout).await {
        Ok(resp) => Message::from_vec(&resp)
            .map(|m| m.metadata.response_code == ResponseCode::NoError && !m.answers.is_empty())
            .unwrap_or(false),
        Err(_) => false,
    }
}
