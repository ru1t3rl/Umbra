//! The server: the router over the engine handle (REQ-X-4, REQ-X-5, ADR 0026).

#![expect(
    clippy::todo,
    unused_variables,
    reason = "WI-6 signature stubs; the bodies arrive in WI-11 onward"
)]

use std::sync::Arc;

use tokio::net::TcpListener;
use umbra_engine::handle::EngineHandle;

use crate::config::ServerSettings;
use crate::shutdown::Shutdown;
use crate::stream::ConnectionStats;

/// The REST API and stream over one engine handle. It talks only to the handle (REQ-X-7).
///
/// `Server` is cheap to clone: clones share one state, so a test or the CLI keeps a clone for
/// [`Server::stream_stats`] while another is consumed by [`Server::serve`].
#[derive(Clone)]
pub struct Server {
    #[expect(dead_code, reason = "read by the WI-11 bodies")]
    inner: Arc<Inner>,
}

#[expect(dead_code, reason = "read by the WI-11 bodies")]
struct Inner {
    handle: EngineHandle,
    settings: ServerSettings,
    shutdown: Shutdown,
}

impl Server {
    /// A server for `handle`. `shutdown` ends the stream connections and the serve loop.
    #[must_use]
    pub fn new(handle: EngineHandle, settings: ServerSettings, shutdown: Shutdown) -> Self {
        Self {
            inner: Arc::new(Inner {
                handle,
                settings,
                shutdown,
            }),
        }
    }

    /// The router of every `/api/v1` route with its layers (WI-11 onward). In-process tests call
    /// it with `tower::ServiceExt::oneshot`.
    pub fn router(&self) -> axum::Router {
        todo!("WI-11")
    }

    /// Serves on `listener` until the shutdown signal fires, then waits for open requests up to the
    /// grace period (WI-11, WI-17).
    ///
    /// # Errors
    /// The listener's I/O error.
    pub async fn serve(self, listener: TcpListener) -> std::io::Result<()> {
        todo!("WI-11")
    }

    /// Per-connection queue maxima and drop counts, in connection order (WI-16).
    #[doc(hidden)]
    #[must_use]
    pub fn stream_stats(&self) -> Vec<ConnectionStats> {
        todo!("WI-16")
    }
}
