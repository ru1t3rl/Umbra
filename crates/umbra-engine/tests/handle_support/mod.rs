//! Shared helpers of the M5 engine-handle tests (AT-3, AT-4, AT-5; step 4). New in M5; the older
//! `common` and `plugin_support` modules are not edited.
//!
//! Techniques used by several tests:
//! - `FrozenWallClock`: wall time that never moves, so a real-time or scaled run holds still at its
//!   current tick (the engine thread polls, it does not sleep). This keeps a run in `running`
//!   without a race against the scenario's duration.
//! - `jam`: a reply callback runs on the engine thread, so one that blocks holds the thread while
//!   the test fills the command queue (queue-full rows) and releases it afterwards.
//! - Failure guards: every wait on another thread has a generous timeout that only turns a hang
//!   into a failure; no test asserts how long anything took.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]
#![expect(clippy::panic, reason = "helpers fail by panicking")]

pub mod trace;

use core::time::Duration;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::PluginId;
use umbra_engine::PluginLog;
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{Frame, RunState, SubscribeOptions, Subscription};
use umbra_engine::handle::{Command, EngineHandle, EngineThread, HandleConfig, HandleError, Reply};
use umbra_engine::{ClockMode, ManualWallClock, WallClock};

/// How long a test waits for another thread before it declares a hang.
pub const GUARD: Duration = Duration::from_secs(30);

/// Serialises the tests of one binary that load real plugin libraries (rule m8).
pub static PLUGIN_TEST_LOCK: Mutex<()> = Mutex::new(());

pub fn plugin_lock() -> MutexGuard<'static, ()> {
    PLUGIN_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Collects the plugin log messages of the sessions of a handle (`HandleConfig::plugin_log`).
#[derive(Default)]
pub struct PluginRecorder(Mutex<Vec<String>>);

impl PluginLog for PluginRecorder {
    fn log(&self, _plugin: &PluginId, _level: umbra_plugin_host::LogLevel, message: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(message.to_owned());
    }
}

impl PluginRecorder {
    /// A recorder, and the same recorder as the `HandleConfig` hook.
    pub fn new() -> (Arc<Self>, Arc<dyn PluginLog>) {
        let recorder = Arc::new(Self::default());
        let hook: Arc<dyn PluginLog> = recorder.clone();
        (recorder, hook)
    }

    pub fn messages(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn count(&self, message: &str) -> usize {
        self.messages().iter().filter(|m| *m == message).count()
    }
}

/// Wall time that never advances; waiting yields instead of sleeping.
pub struct FrozenWallClock;

impl WallClock for FrozenWallClock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }

    fn sleep_until(&self, _deadline: Duration) {
        std::thread::yield_now();
    }
}

pub fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios")
}

pub fn example(name: &str) -> PathBuf {
    examples().join(name).join("scenario.toml")
}

pub fn walking_skeleton() -> PathBuf {
    example("walking_skeleton")
}

/// A config over the standard registry on a manual wall clock that advances 0.5 ms per read.
pub fn config() -> HandleConfig {
    let mut config = HandleConfig::new(standard_registry().expect("the standard registry builds"));
    config.wall = Arc::new(|| -> Box<dyn WallClock> {
        Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
            500,
        )))
    });
    config
}

/// A config whose sessions use [`FrozenWallClock`].
pub fn frozen_config() -> HandleConfig {
    let mut config = config();
    config.wall = Arc::new(|| -> Box<dyn WallClock> { Box::new(FrozenWallClock) });
    config
}

pub fn start_engine(config: HandleConfig) -> (EngineHandle, EngineThread) {
    EngineHandle::spawn(config).unwrap_or_else(|e| panic!("spawn the engine thread: {e}"))
}

/// An idle engine on the default test config.
pub fn engine() -> (EngineHandle, EngineThread) {
    start_engine(config())
}

/// A copy of `from` (a scenario directory) under the target tmp dir, with `edit` applied to its
/// `scenario.toml`.
pub fn scenario_copy(from: &Path, tag: &str, edit: impl FnOnce(String) -> String) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let to = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "handle-scenarios/{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    drop(std::fs::remove_dir_all(&to));
    copy_dir(from, &to);
    let file = to.join("scenario.toml");
    let text = std::fs::read_to_string(&file).expect("read the scenario");
    std::fs::write(&file, edit(text)).expect("write the scenario");
    file
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy");
        }
    }
}

/// `walking_skeleton` (30 Hz) with another duration in seconds.
pub fn skeleton_lasting(seconds: f64) -> PathBuf {
    let dir = examples().join("walking_skeleton");
    scenario_copy(&dir, "skeleton", |text| {
        let edited = text.replace("duration_s = 240.0", &format!("duration_s = {seconds:?}"));
        assert_ne!(edited, text, "the duration line was not found");
        edited
    })
}

/// Two seconds, 60 ticks at 30 Hz.
pub fn short_skeleton() -> PathBuf {
    skeleton_lasting(2.0)
}

/// A run that cannot reach its duration within a test.
pub fn long_skeleton() -> PathBuf {
    skeleton_lasting(1.0e6)
}

/// A subscription that keeps everything: a huge queue, state hashes on.
pub fn lossless() -> SubscribeOptions {
    let mut options = SubscribeOptions::new();
    options.capacity = NonZeroUsize::new(1_000_000).expect("non-zero");
    options.with_state_hash = true;
    options
}

/// Drains `sub`, waiting for more, until a frame satisfies `done`; returns every frame seen.
pub fn collect_until(sub: &Subscription, mut done: impl FnMut(&Frame) -> bool) -> Vec<Arc<Frame>> {
    let mut all = Vec::new();
    loop {
        let drained = sub.wait(GUARD);
        assert!(!drained.frames.is_empty(), "timed out waiting for a frame");
        let finished = drained.frames.iter().any(|f| done(f));
        all.extend(drained.frames);
        if finished {
            return all;
        }
    }
}

pub fn until_stopped(sub: &Subscription) -> Vec<Arc<Frame>> {
    collect_until(sub, |f| f.run.state == RunState::Stopped)
}

pub fn state_of(handle: &EngineHandle) -> RunState {
    handle.status().expect("status").run.state
}

/// A reply that forwards its result to the returned receiver.
pub fn capture<T: Send + 'static>() -> (Reply<T>, mpsc::Receiver<Result<T, HandleError>>) {
    let (tx, rx) = mpsc::channel();
    let reply = Reply::new(move |result| {
        let _ = tx.send(result);
    });
    (reply, rx)
}

pub fn receive<T>(rx: &mpsc::Receiver<Result<T, HandleError>>) -> Result<T, HandleError> {
    rx.recv_timeout(GUARD)
        .unwrap_or_else(|e| panic!("no reply: {e}"))
}

/// Holds the engine thread inside a reply callback until released or dropped.
pub struct Jam {
    release: mpsc::Sender<()>,
}

impl Jam {
    pub fn release(self) {
        let _ = self.release.send(());
    }
}

/// Blocks the engine thread; returns once it is inside the callback, with the queue empty.
pub fn jam(handle: &EngineHandle) -> Jam {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let reply = Reply::new(move |_result| {
        let _ = entered_tx.send(());
        let _ = release_rx.recv();
    });
    handle
        .submit(Command::Status { reply })
        .expect("submit the jamming command");
    entered_rx
        .recv_timeout(GUARD)
        .expect("the engine thread entered the jam");
    Jam {
        release: release_tx,
    }
}

/// Loads `path` with the stepped clock, so the run advances only when stepped.
pub fn load_stepped(handle: &EngineHandle, path: PathBuf) {
    handle
        .load(path, None, Some(ClockMode::Stepped))
        .unwrap_or_else(|e| panic!("load: {e}"));
}

/// Runs `work` on its own thread and fails, instead of hanging, when it does not finish within
/// [`GUARD`].
pub fn finishes<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(GUARD)
        .unwrap_or_else(|e| panic!("did not finish: {e}"))
}

/// Drops `value` here. Used where the type has no destructor yet (a stub), so that
/// `drop(value)` would be flagged; the call still reads as "this ends now".
#[expect(
    clippy::needless_pass_by_value,
    reason = "the point is to take ownership and drop"
)]
pub fn release<T>(value: T) {
    let _ = value;
}
