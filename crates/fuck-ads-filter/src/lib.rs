//! Filter-list parsing and matching for FuckAds.
//!
//! The crate is platform-independent and has no I/O: callers feed it list
//! text and query the compiled [`Engine`]. Supported syntax:
//!
//! * hosts files and plain domain lists
//! * Adblock Plus / uBlock Origin network filters (`||host^`, `|`, `*`, `^`,
//!   regex, `@@` exceptions, `$third-party`, `$domain=`, resource types,
//!   `$important`, `$match-case`, `$badfilter`, `$elemhide`/`$generichide`,
//!   `$removeparam=name`, `$redirect=` to packaged neutral resources)
//! * cosmetic filters (`##`, `#@#`, entity hostnames, standard CSS in `#?#`)
//! * scriptlet filters (`##+js(...)`) referencing built-in scriptlets by name
//! * procedural cosmetic filters (`:has-text()`, `:matches-css()`, `:xpath()`,
//!   `:upward()`, `:remove()`) with validated, typed arguments
//!
//! Unsupported syntax is skipped and counted, never approximated in a way
//! that could break sites.

pub mod cosmetic;
pub mod domain;
pub mod engine;
pub mod export;
pub mod network;
pub mod parser;
pub mod procedural;
pub mod request;
pub mod resources;

pub use engine::{
    Category, CosmeticResult, Engine, EngineBuilder, HostVerdict, ListIndex, ListInfo, ListStats,
    ScriptletRule, UrlVerdict,
};
pub use parser::{ListFormat, Rule, parse_line};
pub use procedural::Procedural;
pub use request::{Request, ResourceType, ResourceTypes};

/// Why a line could not be turned into a rule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Syntactically wrong or unsafe.
    #[error("invalid rule: {0}")]
    Invalid(&'static str),
    /// Valid syntax that this engine intentionally does not implement.
    #[error("unsupported rule: {0}")]
    Unsupported(&'static str),
}
