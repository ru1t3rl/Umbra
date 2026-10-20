//! `umbra-server`: the REST API and WebSocket stream over the engine handle.
//!
//! Responsibility: expose `umbra-engine` over HTTP under [`API_BASE`] and stream frames to clients,
//! without changing what is simulated (REQ-X-4..9, REQ-X-7, D-10, D-11, ADR 0025). M5 WI-1 only
//! scaffolds the crate and WI-6 declares its surface (settings, shutdown signal, wire mapping and
//! problem types are real; the router, handlers and stream loop are `todo!` signatures that
//! WI-11 onward implement).
//!
//! Dependency rule (REQ-A-1..4, REQ-X-7): depends on `umbra-engine` and `umbra-api-types` only
//! among workspace crates, never on `umbra-kernel` or `umbra-core` directly. It contains no
//! `unsafe` code.

#![forbid(unsafe_code)]

pub mod config;
pub mod map;
pub mod problem;
pub mod routes;
pub mod shutdown;
pub mod stream;

pub use config::ServerSettings;
pub use problem::ApiError;
pub use routes::Server;
pub use shutdown::Shutdown;
pub use stream::{ConnectionParams, ConnectionStats, Encoding, run_connection};

/// Path prefix of every REST route and of the stream (ADR 0026).
pub const API_BASE: &str = "/api/v1";

#[cfg(test)]
mod tests {
    use super::API_BASE;

    #[test]
    fn api_base_matches_the_contract_version() {
        let major = umbra_api_types::API_VERSION.split('.').next();
        assert_eq!(API_BASE, format!("/api/v{}", major.unwrap_or_default()));
    }
}
