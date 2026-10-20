//! The state every handler shares, and the one way a handler talks to the engine (REQ-X-7).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::oneshot;
use umbra_engine::Confinement;
use umbra_engine::handle::{Command, EngineHandle, HandleError, Reply};

use crate::config::ServerSettings;
use crate::layers::host::{HostPolicy, OriginPolicy};
use crate::problem::ApiError;
use crate::shutdown::Shutdown;
use crate::stream::{ConnectionStats, LiveStats};

/// Shared by the clones of a [`Server`](crate::Server) and by every handler.
pub(crate) struct AppState {
    pub(crate) handle: EngineHandle,
    pub(crate) settings: ServerSettings,
    pub(crate) shutdown: Shutdown,
    pub(crate) hosts: HostPolicy,
    pub(crate) origins: OriginPolicy,
    /// Open stream connections (WI-15), for `[server.stream] max_connections`.
    pub(crate) connections: AtomicUsize,
    /// The counters of the open stream connections, in connection order (WI-16).
    streams: Mutex<Vec<Arc<LiveStats>>>,
    /// For scrubbing the text of library errors (ASM-194). `None` only if the configured
    /// directories cannot be resolved; the engine then has none either (see `ServerSettings`).
    confinement: Option<Confinement>,
}

impl AppState {
    pub(crate) fn new(handle: EngineHandle, settings: ServerSettings, shutdown: Shutdown) -> Self {
        let confinement = settings.confinement().ok();
        let hosts = HostPolicy::new(&settings.server);
        let origins = OriginPolicy::new(&settings.server);
        Self {
            handle,
            settings,
            shutdown,
            hosts,
            origins,
            connections: AtomicUsize::new(0),
            streams: Mutex::new(Vec::new()),
            confinement,
        }
    }

    /// Takes one of the `max` stream connections, or `None` if all are taken. The connection is
    /// given back when the returned guard drops.
    pub(crate) fn take_connection(self: &Arc<Self>, max: usize) -> Option<ConnectionSlot> {
        self.connections
            .try_update(Ordering::AcqRel, Ordering::Acquire, |open| {
                (open < max).then_some(open + 1)
            })
            .ok()
            .map(|_| {
                let live = Arc::new(LiveStats::default());
                self.streams().push(Arc::clone(&live));
                ConnectionSlot {
                    state: Arc::clone(self),
                    live,
                }
            })
    }

    fn streams(&self) -> std::sync::MutexGuard<'_, Vec<Arc<LiveStats>>> {
        self.streams.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The counters of every open stream connection, in connection order.
    pub(crate) fn stream_stats(&self) -> Vec<ConnectionStats> {
        self.streams().iter().map(|live| live.snapshot()).collect()
    }

    /// The problem for a failed command, with library messages written in display form.
    pub(crate) fn problem(&self, error: &HandleError) -> ApiError {
        match &self.confinement {
            Some(confinement) => ApiError::from_handle_error_in(error, confinement),
            None => ApiError::from_handle_error(error),
        }
    }

    /// Sends the command built by `make` and waits for its answer without blocking a thread.
    pub(crate) async fn call<T: Send + 'static>(
        &self,
        make: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, ApiError> {
        exchange(|reply| self.handle.submit(make(reply)))
            .await
            .map_err(|error| self.problem(&error))
    }
}

/// One of the stream connections `[server.stream] max_connections` allows; released on drop.
pub(crate) struct ConnectionSlot {
    state: Arc<AppState>,
    live: Arc<LiveStats>,
}

impl ConnectionSlot {
    /// The counters of this connection, listed by [`AppState::stream_stats`].
    pub(crate) fn live(&self) -> Arc<LiveStats> {
        Arc::clone(&self.live)
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.state
            .streams()
            .retain(|live| !Arc::ptr_eq(live, &self.live));
        self.state.connections.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Runs one request-reply exchange. `submit` queues the command that completes the reply it is
/// given.
///
/// If `submit` refuses, the reply was dropped inside it and has already told the channel `Closed`;
/// that callback is ignored and the refusal is the answer, so a refusal answers once with its own
/// error (ASM-187 h). If the engine thread dies while the command is queued, the reply drops
/// uncompleted and the answer is `Closed` (`503 engine_unavailable`).
pub(crate) async fn exchange<T: Send + 'static>(
    submit: impl FnOnce(Reply<T>) -> Result<(), HandleError>,
) -> Result<T, HandleError> {
    let (sender, receiver) = oneshot::channel();
    let reply = Reply::new(move |result| {
        // The receiver is gone if the client went away or the command was refused: nobody waits.
        drop(sender.send(result));
    });
    submit(reply)?;
    receiver.await.unwrap_or(Err(HandleError::Closed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_refused_submit_answers_with_its_own_error_not_closed() {
        // `submit` drops the reply, as `EngineHandle::submit` drops a refused command.
        let busy = exchange::<u8>(|reply| {
            drop(reply);
            Err(HandleError::Busy)
        })
        .await;
        assert!(matches!(busy, Err(HandleError::Busy)), "{busy:?}");
        let closed = exchange::<u8>(|reply| {
            drop(reply);
            Err(HandleError::Closed)
        })
        .await;
        assert!(matches!(closed, Err(HandleError::Closed)), "{closed:?}");
    }

    #[tokio::test]
    async fn a_completed_reply_is_the_answer_and_a_dropped_one_is_closed() {
        let done = exchange(|reply| {
            reply.complete(Ok(7_u8));
            Ok(())
        })
        .await;
        assert!(matches!(done, Ok(7)), "{done:?}");
        let dropped = exchange::<u8>(|reply| {
            drop(reply);
            Ok(())
        })
        .await;
        assert!(matches!(dropped, Err(HandleError::Closed)), "{dropped:?}");
    }
}
