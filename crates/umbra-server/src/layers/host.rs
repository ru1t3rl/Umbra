//! The `Host` allow-list (D-b, ADR 0026 amended): protects the loopback API from DNS rebinding.
//!
//! Accepted: a loopback name or address (`localhost`, `127.0.0.0/8`, `[::1]`) on any port; the
//! bound address; and the hosts of `[server] allowed_hosts`. Anything else, or a missing `Host`, is
//! `403 host_not_allowed`.

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::HOST;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use umbra_api_types::ProblemCode;
use umbra_engine::ServerConfig;

use crate::problem::ApiError;
use crate::state::AppState;

/// A host name or address, lower case, with the port if one was written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HostPort {
    host: String,
    port: Option<u16>,
}

impl HostPort {
    /// Parses `host`, `host:port`, `[v6]` or `[v6]:port`; `None` if it is none of these.
    fn parse(text: &str) -> Option<Self> {
        let (host, port) = if let Some(rest) = text.strip_prefix('[') {
            let (inside, after) = rest.split_once(']')?;
            let port = match after {
                "" => None,
                after => Some(after.strip_prefix(':')?),
            };
            (inside, port)
        } else if let Some((host, port)) = text.split_once(':') {
            (host, Some(port))
        } else {
            (text, None)
        };
        if host.is_empty() || host.contains(['/', '@', ' ']) {
            return None;
        }
        let port = port.map(str::parse::<u16>).transpose().ok()?;
        Some(Self {
            host: host.to_ascii_lowercase(),
            port,
        })
    }

    fn ip(&self) -> Option<IpAddr> {
        self.host.parse().ok()
    }

    /// `localhost` or a loopback address.
    fn is_loopback(&self) -> bool {
        self.ip()
            .map_or(self.host == "localhost", |ip| ip.is_loopback())
    }
}

/// What `Host` values the server answers to.
#[derive(Debug, Clone)]
pub(crate) struct HostPolicy {
    bound: IpAddr,
    allowed: Vec<HostPort>,
}

impl HostPolicy {
    pub(crate) fn new(config: &ServerConfig) -> Self {
        Self {
            bound: config.bind.ip(),
            allowed: config
                .allowed_hosts
                .iter()
                .filter_map(|entry| HostPort::parse(entry))
                .collect(),
        }
    }

    /// Whether `value`, a `Host` header, is allowed.
    pub(crate) fn allows(&self, value: &str) -> bool {
        let Some(requested) = HostPort::parse(value) else {
            return false;
        };
        if requested.is_loopback() || requested.ip() == Some(self.bound) {
            return true;
        }
        // A configured entry without a port allows any port; with one, exactly that port.
        self.allowed.iter().any(|entry| {
            let same_host = match (entry.ip(), requested.ip()) {
                (Some(a), Some(b)) => a == b,
                (None, None) => entry.host == requested.host,
                _ => false,
            };
            same_host && entry.port.is_none_or(|port| requested.port == Some(port))
        })
    }
}

/// What `Origin` values a WebSocket upgrade may carry (ADR 0026 amended): `http` and `https`
/// origins on a loopback host with any port, and the exact origins of `[server] allowed_origins`.
/// A request without `Origin` is not checked (a non-browser client).
#[derive(Debug, Clone)]
pub(crate) struct OriginPolicy {
    allowed: Vec<String>,
}

impl OriginPolicy {
    pub(crate) fn new(config: &ServerConfig) -> Self {
        Self {
            allowed: config
                .allowed_origins
                .iter()
                .map(|entry| entry.trim_end_matches('/').to_ascii_lowercase())
                .collect(),
        }
    }

    /// Whether `origin`, an `Origin` header value, is allowed.
    pub(crate) fn allows(&self, origin: &str) -> bool {
        let origin = origin.to_ascii_lowercase();
        if self.allowed.contains(&origin) {
            return true;
        }
        origin
            .split_once("://")
            .filter(|(scheme, _)| matches!(*scheme, "http" | "https"))
            .and_then(|(_, authority)| HostPort::parse(authority))
            .is_some_and(|host| host.is_loopback())
    }
}

/// The outermost layer: refuses a request whose `Host` is not allowed.
pub(crate) async fn check(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    // An absolute-form request target (`GET http://evil.example/ ...`) names a host too, and
    // servers must prefer it over `Host`: both have to be allowed.
    let authority_allowed = request
        .uri()
        .authority()
        .is_none_or(|authority| state.hosts.allows(authority.as_str()));
    let host_allowed = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| state.hosts.allows(host));
    let allowed = authority_allowed && host_allowed;
    if allowed {
        next.run(request).await
    } else {
        ApiError::new(
            ProblemCode::HostNotAllowed,
            "the Host header is not allowed (see [server] allowed_hosts)",
        )
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(bind: &str, allowed: &[&str]) -> HostPolicy {
        let mut config = ServerConfig::default();
        config.bind = bind.parse().expect("address");
        config.allowed_hosts = allowed.iter().map(|s| (*s).to_owned()).collect();
        HostPolicy::new(&config)
    }

    #[test]
    fn loopback_on_any_port_and_nothing_else_by_default() {
        let p = policy("127.0.0.1:7878", &[]);
        for ok in [
            "localhost",
            "LOCALHOST:1",
            "127.0.0.9:80",
            "[::1]",
            "[::1]:7878",
        ] {
            assert!(p.allows(ok), "{ok}");
        }
        for bad in [
            "",
            "evil.example",
            "localhost.evil.example",
            "10.0.0.5",
            "::1",
            "[::2]",
            "localhost:abc",
            "localhost:99999",
            "a@b",
            "[::1",
        ] {
            assert!(!p.allows(bad), "{bad}");
        }
    }

    #[test]
    fn origins_are_loopback_any_port_or_exactly_configured() {
        let mut config = ServerConfig::default();
        config.allowed_origins = vec!["https://Ops.example/".to_owned()];
        let p = OriginPolicy::new(&config);
        for ok in [
            "http://localhost:5173",
            "https://127.0.0.1",
            "http://[::1]:8080",
            "https://ops.example",
        ] {
            assert!(p.allows(ok), "{ok}");
        }
        for bad in [
            "",
            "null",
            "https://evil.example",
            "http://localhost.evil.example:3000",
            "https://ops.example:8443",
            "ftp://localhost",
            "http://localhost/path",
            "localhost",
        ] {
            assert!(!p.allows(bad), "{bad}");
        }
    }

    #[test]
    fn the_bound_address_is_allowed_on_its_ip() {
        let p = policy("192.0.2.7:7878", &[]);
        assert!(p.allows("192.0.2.7:7878"));
        assert!(p.allows("192.0.2.7"));
        assert!(!p.allows("192.0.2.8:7878"));
    }

    #[test]
    fn a_configured_host_without_a_port_allows_any_port_and_with_one_exactly_that() {
        let p = policy(
            "127.0.0.1:7878",
            &["umbra.example", "Ops.Example:8443", "198.51.100.4"],
        );
        assert!(p.allows("umbra.example") && p.allows("umbra.example:1"));
        assert!(p.allows("ops.example:8443"));
        assert!(!p.allows("ops.example:8444") && !p.allows("ops.example"));
        assert!(p.allows("198.51.100.4:80"));
        assert!(!p.allows("other.example"));
    }
}
