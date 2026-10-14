//! A local filtering forward proxy.
//!
//! * **`CONNECT host:port`** (HTTPS and anything else tunnelled): the proxy
//!   decides by hostname and then relays the encrypted bytes untouched. It
//!   never decrypts, never installs a certificate and never sees URLs.
//! * **Plain `http://` requests:** the full URL is visible, so URL filters
//!   apply (with the `Referer` as the page, when present).
//! * **`GET /proxy.pac`:** the PAC file for clients.
//!
//! Safety properties: loopback clients only; targets resolving to loopback,
//! private or link-local addresses are refused ([`policy`]); strict request
//! parsing with size limits ([`request`]); bounded concurrency and idle
//! timeouts; no URLs or allowed hostnames are ever logged or stored.
//!
//! The crate knows nothing about how rules are stored: the daemon supplies a
//! [`ProxyFilter`].

pub mod pac;
pub mod policy;
pub mod request;
mod server;

pub use adblock_filter::Category;
pub use server::{Listeners, bind, serve};

use std::sync::Arc;

/// The policy decision for a host or URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block { domain: String, category: Category },
}

/// Supplied by the daemon on top of the live filter and user rules.
pub trait ProxyFilter: Send + Sync + 'static {
    /// Decision for a hostname (CONNECT targets and the host of HTTP URLs).
    fn check_host(&self, host: &str) -> Verdict;
    /// Decision for a full `http://` URL. `referer` is the page that made
    /// the request, if the client sent one.
    fn check_url(&self, url: &str, referer: Option<&str>) -> Verdict;
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Destination ports the proxy will connect to.
    pub ports: Vec<u16>,
    pub max_connections: usize,
    /// Log each blocked hostname at debug level (never allowed ones).
    pub log_blocked: bool,
    /// Permit loopback/private targets. **Tests only**: the daemon always
    /// sets this to `false`.
    pub allow_local_targets: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        ProxyConfig {
            ports: vec![80, 443, 8080, 8443],
            max_connections: 512,
            log_blocked: false,
            allow_local_targets: false,
        }
    }
}

pub(crate) type SharedFilter = Arc<dyn ProxyFilter>;
