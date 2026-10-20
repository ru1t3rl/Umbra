//! Running the server to completion: the runtime, OS signals and the shutdown order (ADR 0026,
//! ADR 0024, review M12/m10).

use tokio::net::TcpListener;
use umbra_engine::handle::{EngineHandle, EngineThread};

use crate::config::ServerSettings;
use crate::routes::Server;
use crate::shutdown::Shutdown;

/// How long dropping the runtime waits for blocking work still running after the grace period.
const RUNTIME_WIND_DOWN: std::time::Duration = std::time::Duration::from_millis(100);

/// What ends a [`run`].
#[derive(Debug, Clone)]
pub enum ShutdownMode {
    /// SIGINT (Ctrl-C) and, on Unix, SIGTERM, handled inside the server.
    Signals,
    /// A trigger the caller holds: for tests and embedding. No signal handler is installed.
    Manual(Shutdown),
}

/// Why [`run`] failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerError {
    /// The async runtime could not be built.
    #[error("cannot start the async runtime: {0}")]
    Runtime(#[source] std::io::Error),
    /// The address cannot be listened on.
    #[error("cannot listen on {addr}: {source}")]
    Bind {
        /// The configured address.
        addr: std::net::SocketAddr,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// A signal handler could not be installed.
    #[error("cannot install the signal handlers: {0}")]
    Signals(#[source] std::io::Error),
    /// Serving ended with an I/O error.
    #[error("the server failed: {0}")]
    Serve(#[source] std::io::Error),
}

/// Binds `[server] bind` and serves until the shutdown fires, then shuts down in order (ADR 0026):
/// 1. stop accepting connections;
/// 2. shut the engine thread down, so pending steps answer `503 engine_unavailable`;
/// 3. signal the stream tasks, which close with 1001;
/// 4. wait for open requests up to `[server] shutdown_grace_s`;
/// 5. drop the runtime.
///
/// Blocks the calling thread; it is the only place that owns a runtime. `on_listening` is called once
/// with the bound address (port 0 is resolved), after the socket is bound and, for
/// [`ShutdownMode::Signals`], the signal handlers are installed: a SIGTERM that follows what the
/// callback announced is therefore always handled. A callback error ends the run.
///
/// # Errors
/// [`ServerError`]. The engine thread is shut down on every path.
pub fn run(
    settings: ServerSettings,
    handle: EngineHandle,
    thread: EngineThread,
    mode: ShutdownMode,
    on_listening: impl FnOnce(std::net::SocketAddr) -> std::io::Result<()>,
) -> Result<(), ServerError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ServerError::Runtime)?;
    let result = runtime.block_on(serve(settings, handle, thread, mode, on_listening));
    // Whatever is left (a connection past the deadline, an engine join that timed out) is dropped
    // with the runtime; the grace period is already spent, so only a short wait remains.
    runtime.shutdown_timeout(RUNTIME_WIND_DOWN);
    result
}

async fn serve(
    settings: ServerSettings,
    handle: EngineHandle,
    thread: EngineThread,
    mode: ShutdownMode,
    on_listening: impl FnOnce(std::net::SocketAddr) -> std::io::Result<()>,
) -> Result<(), ServerError> {
    let shutdown = match &mode {
        ShutdownMode::Manual(signal) => signal.clone(),
        ShutdownMode::Signals => Shutdown::new(),
    };
    let addr = settings.server.bind;
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| ServerError::Bind { addr, source })?;
    let bound = listener
        .local_addr()
        .map_err(|source| ServerError::Bind { addr, source })?;
    if matches!(mode, ShutdownMode::Signals) {
        let signals = Signals::install().map_err(ServerError::Signals)?;
        let trigger = shutdown.clone();
        tokio::spawn(async move {
            signals.wait().await;
            trigger.trigger();
        });
    }
    on_listening(bound).map_err(ServerError::Serve)?;
    // The engine goes down as soon as the signal fires, concurrently with the serve loop's wait for
    // open requests: that is what turns a pending step into a 503 instead of a hang (step 2).
    let grace = settings.server.shutdown_grace;
    let engine = tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            shutdown.triggered().await;
            // `EngineThread::shutdown` blocks while it joins the thread (a plugin tick that never
            // returns would block it for good), so the wait is bounded by the grace period.
            let join = tokio::task::spawn_blocking(move || thread.shutdown());
            if tokio::time::timeout(grace, join).await.is_err() {
                tracing::warn!("the engine thread did not stop within the shutdown grace period");
            }
        }
    });
    let served = Server::new(handle, settings, shutdown.clone())
        .serve(listener)
        .await;
    // An I/O error ends serving without a signal: stop the stream tasks and the engine too.
    shutdown.trigger();
    // The engine's own result (a thread that had already died) does not change how serving ended.
    drop(engine.await);
    served.map_err(ServerError::Serve)
}

/// The OS signals that mean "stop", all registered by [`install`](Self::install) so that none is
/// left to the default action once the server has announced its address.
struct Signals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(not(unix))]
    ctrl_c: tokio::signal::windows::CtrlC,
    #[cfg(not(unix))]
    ctrl_close: tokio::signal::windows::CtrlClose,
}

impl Signals {
    fn install() -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Ok(Self {
                interrupt: signal(SignalKind::interrupt())?,
                terminate: signal(SignalKind::terminate())?,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                ctrl_c: tokio::signal::windows::ctrl_c()?,
                ctrl_close: tokio::signal::windows::ctrl_close()?,
            })
        }
    }

    /// Completes at the first SIGINT or SIGTERM (Ctrl-C and console close, off Unix).
    async fn wait(self) {
        #[cfg(unix)]
        {
            let (mut interrupt, mut terminate) = (self.interrupt, self.terminate);
            tokio::select! {
                _ = interrupt.recv() => {}
                _ = terminate.recv() => {}
            }
        }
        #[cfg(not(unix))]
        {
            let (mut ctrl_c, mut ctrl_close) = (self.ctrl_c, self.ctrl_close);
            tokio::select! {
                _ = ctrl_c.recv() => {}
                _ = ctrl_close.recv() => {}
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::Signals;

    /// SIGINT sent after `install` and before anything polls `wait` is not lost: registration happens
    /// in `install`, not on the first poll (a lazily registered handler leaves the default action,
    /// which ends the process, in this window).
    #[tokio::test]
    async fn sigint_between_install_and_wait_is_delivered() {
        let signals = Signals::install().expect("install");
        let sent = std::process::Command::new("kill")
            .args(["-INT", &std::process::id().to_string()])
            .status()
            .expect("kill");
        assert!(sent.success());
        tokio::time::timeout(std::time::Duration::from_secs(30), signals.wait())
            .await
            .expect("the signal was delivered");
    }
}
