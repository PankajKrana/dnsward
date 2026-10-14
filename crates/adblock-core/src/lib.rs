//! AdBlock core: the daemon that ties together the filter engine, the DNS
//! resolver, filter-list updates, statistics and the local control API.
//!
//! Nothing in this crate is platform-specific beyond POSIX (Unix sockets and
//! signals). OS integration (service managers, system DNS settings) lives in
//! `adblock-platform`.

pub mod control;
pub mod daemon;
pub mod protocol;
pub mod rules;
pub mod update;

pub use daemon::{Daemon, RunningDaemon, StartOptions, drop_privileges, run, run_with};
