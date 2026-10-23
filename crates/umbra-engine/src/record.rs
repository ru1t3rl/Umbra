//! The engine recorder: writes the run's inputs and periodic state hashes to a recording
//! (REQ-K-7, REQ-K-4, REQ-K-6, REQ-K-8, ADR 0053, ASM-624..626, ASM-633, ASM-732..736).
//!
//! The engine thread owns the recorder; a writer thread of `umbra-record` owns the file. A
//! [`Recorder`] is created from a loaded scenario (the header: seed, tick rate and source, file and
//! library digests, adapters) and attached to an [`Engine`], which then records, in tick order:
//! commands as they are applied, the messages each network adapter delivered and its faults (through
//! the manager's `InboundTap`), a `hash` per the cadence, and the `end`. Recording only reads: with
//! and without a recorder every per-tick hash is the same.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::hash::fnv1a64;
use umbra_core::net::{KeySpace, NetMessage};
use umbra_core::{ControlAction, StateHash, StateHashVersion};
use umbra_kernel::ClockMode;
use umbra_net::InboundTap;
use umbra_plugin_host::PluginHost;
use umbra_record::{
    AdapterRef, CommandRecord, DEFAULT_QUEUE, EndRecord, FaultRecord, FileDigest, Header,
    InboundRecord, PluginRef, Record, RecordError, RecordedCommand, ScenarioRef, Writer,
};
use umbra_templates::{Scenario, TickRateSource, scenario_inputs};

use crate::engine::Engine;
use crate::plugins::LoadedScenario;

/// How and where to record.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecordOptions {
    /// The recording file (recommended suffix `.umbrarec.jsonl`, ASM-622).
    pub path: PathBuf,
    /// Hash cadence in ticks; `None` means the tick rate (once per simulated second, ASM-624).
    /// Zero is raised to 1.
    pub hash_every: Option<u64>,
    /// The state hash encoding of the `hash` records and the header (ADR 0055).
    pub state_hash: StateHashVersion,
    /// Refuse to replace an existing file (`serve --record-dir` never truncates a recording);
    /// default `false`, which truncates like `run --record`.
    pub create_new: bool,
}

impl RecordOptions {
    /// Options for `path` with the default cadence and state hash v2.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            hash_every: None,
            state_hash: StateHashVersion::V2,
            create_new: false,
        }
    }
}

/// Where a recording stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordingState {
    /// Records are being written.
    Active,
    /// The run ended and the file is complete.
    Finished,
    /// A write failed; the file is incomplete (ASM-625).
    Failed,
}

/// A live view of the recorder (REST `RunStatus.recording`, M7 WI-16).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecordingStatus {
    /// The state.
    pub state: RecordingState,
    /// The recording file.
    pub file: PathBuf,
    /// Records handed to the writer so far, the header not counted (ASM-733).
    pub records: u64,
    /// The last tick that has a record.
    pub last_tick: u64,
}

/// The recorder an [`Engine`] feeds through [`Engine::attach_recorder`]: the header and the open
/// file, ready to be written once the engine's adapters are known.
#[derive(Debug)]
pub struct Recorder {
    options: RecordOptions,
    header: Header,
    file: File,
}

impl Recorder {
    /// Creates the file and its header from the loaded scenario at `scenario_path` and its
    /// plugins, with `seed` the effective seed of the run (`None` = the scenario's).
    ///
    /// # Errors
    /// [`RecordError::Io`] if the file cannot be created or a scenario or library file cannot be
    /// read for its digest; [`RecordError::Unsupported`] for a state hash version this format
    /// cannot name.
    pub fn create(
        options: RecordOptions,
        scenario_path: &Path,
        loaded: &LoadedScenario,
        seed: Option<u64>,
    ) -> Result<Self, RecordError> {
        Self::from_parts(
            options,
            scenario_path,
            &loaded.scenario,
            &loaded.plugins,
            seed,
        )
    }

    /// [`Recorder::create`] from the parts of a load (a session keeps them apart).
    pub(crate) fn from_parts(
        options: RecordOptions,
        scenario_path: &Path,
        scenario: &Scenario,
        plugins: &PluginHost,
        seed: Option<u64>,
    ) -> Result<Self, RecordError> {
        let version = version_number(options.state_hash)?;
        // Everything that can fail is done before the file is opened, so a failure leaves an
        // existing file alone (ASM-734).
        let scenario_ref = scenario_ref(scenario_path, &options.path)?;
        let plugin_refs = plugin_refs(plugins)?;
        refuse_inputs(&options.path, &scenario_ref, plugins)?;
        let file = open_new(&options)?;
        let header = Header {
            umbra: env!("CARGO_PKG_VERSION").to_owned(),
            scenario: scenario_ref,
            seed: seed.unwrap_or(scenario.seed),
            tick_rate_hz: scenario.tick_rate.hz(),
            tick_rate_source: tick_rate_source(scenario.tick_rate_source).to_owned(),
            state_hash_version: version,
            hash_every: cadence(&options, scenario),
            on_plugin_fault: scenario.on_plugin_fault.name().to_owned(),
            plugins: plugin_refs,
            network_adapters: adapter_refs(scenario),
        };
        Ok(Self {
            options,
            header,
            file,
        })
    }
}

/// Opens the recording file: truncating, or only if it does not exist yet (`create_new`).
fn open_new(options: &RecordOptions) -> Result<File, RecordError> {
    let mut open = std::fs::OpenOptions::new();
    open.write(true);
    if options.create_new {
        open.create_new(true);
    } else {
        open.create(true).truncate(true);
    }
    Ok(open.open(&options.path)?)
}

/// A recording must not overwrite a file the run is made of: the scenario files and the plugin
/// libraries (compared canonically). The recording path may not exist yet.
fn refuse_inputs(
    recording: &Path,
    scenario: &ScenarioRef,
    plugins: &PluginHost,
) -> Result<(), RecordError> {
    let target = match recording.file_name() {
        Some(name) if !recording.exists() => {
            canonical(recording.parent().unwrap_or_else(|| Path::new(""))).join(name)
        }
        _ => canonical(recording),
    };
    let libraries = (0..plugins.plugins().len()).filter_map(|i| plugins.library_path(i));
    let inputs = scenario
        .files
        .iter()
        .map(|f| f.path.as_path())
        .chain(libraries);
    for input in inputs {
        if canonical(input) == target {
            return Err(RecordError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "the recording path {} is an input of the run and would be overwritten",
                    recording.display()
                ),
            )));
        }
    }
    Ok(())
}

/// The header number of a state hash version (ADR 0055); an unknown one cannot be recorded.
fn version_number(version: StateHashVersion) -> Result<u32, RecordError> {
    match version {
        StateHashVersion::V2 => Ok(2),
        StateHashVersion::V3 => Ok(3),
        other => Err(RecordError::Unsupported(format!(
            "state hash version {other:?}"
        ))),
    }
}

fn cadence(options: &RecordOptions, scenario: &Scenario) -> u64 {
    options
        .hash_every
        .unwrap_or_else(|| u64::from(scenario.tick_rate.hz()))
        .max(1)
}

fn tick_rate_source(source: TickRateSource) -> &'static str {
    match source {
        TickRateSource::Scenario => "scenario",
        TickRateSource::Default => "default",
        _ => "unknown",
    }
}

/// The scenario file as given (canonical), relative to the recording's directory when possible
/// (ASM-633), and the digest of every file it was loaded from (ASM-623).
fn scenario_ref(scenario: &Path, recording: &Path) -> Result<ScenarioRef, RecordError> {
    let path = canonical(scenario);
    let directory = canonical(recording.parent().unwrap_or_else(|| Path::new("")));
    let inputs = scenario_inputs(&path);
    let mut rest: Vec<PathBuf> = inputs.files.iter().map(|f| canonical(f)).collect();
    rest.sort();
    rest.dedup();
    rest.retain(|f| *f != path);
    let files = std::iter::once(path.clone())
        .chain(rest)
        .map(|file| {
            let bytes = std::fs::read(&file)?;
            Ok(FileDigest {
                fnv1a64: fnv1a64(&bytes),
                path: file,
            })
        })
        .collect::<Result<Vec<_>, RecordError>>()?;
    Ok(ScenarioRef {
        relative_path: relative_to(&path, &directory),
        path,
        files,
    })
}

/// The canonical form of `path`, or `path` as given if it cannot be resolved (a read of it then
/// reports the problem). The empty path is the working directory.
fn canonical(path: &Path) -> PathBuf {
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `path` as a path relative to the directory `base`; `None` when they share no root.
fn relative_to(path: &Path, base: &Path) -> Option<PathBuf> {
    let (path, base): (Vec<_>, Vec<_>) = (path.components().collect(), base.components().collect());
    if path.first() != base.first() {
        return None;
    }
    let common = path.iter().zip(&base).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    base[common..].iter().for_each(|_| out.push(".."));
    path[common..].iter().for_each(|c| out.push(c));
    Some(out)
}

/// The plugins of the host with the digest of their library bytes; a plugin linked into the process
/// has no file, so the digest is that of its origin text (ASM-734).
fn plugin_refs(host: &PluginHost) -> Result<Vec<PluginRef>, RecordError> {
    host.plugins()
        .iter()
        .enumerate()
        .map(|(index, info)| {
            let digest = match host.library_path(index) {
                Some(library) => fnv1a64(&std::fs::read(library)?),
                None => fnv1a64(info.origin.as_bytes()),
            };
            Ok(PluginRef {
                id: info.id.to_string(),
                version: info.version.to_string(),
                abi: info.abi,
                library_fnv1a64: digest,
            })
        })
        .collect()
}

/// The adapters in scenario order; the key spaces are filled in when the recorder is attached.
fn adapter_refs(scenario: &Scenario) -> Vec<AdapterRef> {
    scenario
        .network_adapters
        .iter()
        .map(|spec| AdapterRef {
            name: spec.name.clone(),
            adapter: spec.adapter.to_string(),
            key_space: KeySpace { base: 0, count: 0 },
        })
        .collect()
}

/// The recorded form of a run-control action (informational, ASM-626); `None` for an action this
/// version does not know.
pub(crate) fn informational(action: ControlAction) -> Option<RecordedCommand> {
    match action {
        ControlAction::Start => Some(RecordedCommand::Start),
        ControlAction::Pause => Some(RecordedCommand::Pause),
        ControlAction::Resume => Some(RecordedCommand::Resume),
        ControlAction::Stop => Some(RecordedCommand::Stop),
        ControlAction::Step(n) => Some(RecordedCommand::Step(n)),
        _ => None,
    }
}

/// The spelling of a clock mode in a `set_clock` record (the `--clock` spelling).
pub(crate) fn clock_name(mode: ClockMode) -> String {
    match mode {
        ClockMode::AsFastAsPossible => "afap".to_owned(),
        ClockMode::RealTime => "realtime".to_owned(),
        ClockMode::Stepped => "stepped".to_owned(),
        ClockMode::Scaled(scale) => format!("scaled={}", scale.get()),
    }
}

/// Everything the recorder keeps while a run is recorded; shared by the engine and the tap, both
/// used on the engine thread only, so the lock is never contended.
struct Core {
    /// `None` after `finish`, or if the file could not be started.
    writer: Option<Writer>,
    state: RecordingState,
    file: PathBuf,
    version: StateHashVersion,
    hash_every: u64,
    /// Records handed to the writer, the header not counted.
    records: u64,
    last_tick: u64,
    failure: Option<String>,
    /// A command or an inbound batch asks for a hash at the end of the next tick (ASM-624).
    force_hash: bool,
    /// The last tick a `hash` record was written for.
    hashed: Option<u64>,
    /// The boundary of the last command and the next `seq` there.
    boundary: Option<u64>,
    next_seq: u64,
}

impl Core {
    fn new(file: PathBuf, version: StateHashVersion, hash_every: u64) -> Self {
        Self {
            writer: None,
            state: RecordingState::Active,
            file,
            version,
            hash_every,
            records: 0,
            last_tick: 0,
            failure: None,
            force_hash: false,
            hashed: None,
            boundary: None,
            next_seq: 0,
        }
    }

    fn open(options: &RecordOptions, header: &Header, file: File) -> Self {
        let mut core = Self::new(options.path.clone(), options.state_hash, header.hash_every);
        match Writer::from_sink(BufWriter::new(file), header, DEFAULT_QUEUE) {
            Ok(writer) => core.writer = Some(writer),
            Err(error) => core.fail(&error),
        }
        core
    }

    /// A recorder that never started: `Failed` from the first moment, naming the intended file.
    fn failed(file: PathBuf, error: &RecordError) -> Self {
        let mut core = Self::new(file, StateHashVersion::V2, 1);
        core.fail(error);
        core
    }

    /// Marks the recording failed and says so once (ASM-625). The simulation goes on.
    fn fail(&mut self, error: &RecordError) {
        if self.failure.is_none() {
            let message = match error {
                RecordError::Failed(message) => message.clone(),
                other => other.to_string(),
            };
            tracing::error!(file = %self.file.display(), "recording failed: {message}");
            self.failure = Some(message);
        }
        self.state = RecordingState::Failed;
    }

    /// Hands a record to the writer; `true` only if it was accepted. Nothing is written once the
    /// recording failed or finished.
    fn push(&mut self, record: Record) -> bool {
        let Some(writer) = self.writer.as_ref().filter(|_| self.failure.is_none()) else {
            return false;
        };
        match writer.push(record) {
            Ok(()) => {
                self.records += 1;
                true
            }
            Err(error) => {
                self.fail(&error);
                false
            }
        }
    }

    fn command(&mut self, before_tick: u64, command: RecordedCommand) {
        let seq = if self.boundary == Some(before_tick) {
            self.next_seq + 1
        } else {
            0
        };
        let record = Record::Command(CommandRecord {
            before_tick,
            seq,
            command,
        });
        // The boundary and `seq` advance only with an accepted record, like everything else.
        if self.push(record) {
            (self.boundary, self.next_seq) = (Some(before_tick), seq);
            self.force_hash = true;
            self.last_tick = self.last_tick.max(before_tick);
        }
    }

    fn inbound(&mut self, tick: u64, adapter: &str, messages: &[NetMessage]) {
        if messages.is_empty() {
            return;
        }
        let Some(writer) = self.writer.as_ref().filter(|_| self.failure.is_none()) else {
            return;
        };
        let mut pushed = 0_u64;
        let result = push_split(
            &mut |record| {
                writer.push(record)?;
                pushed += 1;
                Ok(())
            },
            tick,
            adapter,
            messages,
        );
        self.records += pushed;
        match result {
            Ok(()) => {
                self.force_hash = true;
                self.last_tick = self.last_tick.max(tick);
            }
            Err(error) => self.fail(&error),
        }
    }

    fn fault(&mut self, tick: u64, adapter: &str, call: &str) {
        let record = Record::Fault(FaultRecord {
            tick,
            adapter: adapter.to_owned(),
            call: call.to_owned(),
        });
        if self.push(record) {
            self.last_tick = self.last_tick.max(tick);
        }
    }

    fn hash(&mut self, tick: u64, hash: StateHash) {
        if self.push(Record::Hash { tick, hash }) {
            self.hashed = Some(tick);
            self.last_tick = self.last_tick.max(tick);
        }
    }

    fn after_tick(&mut self, tick: u64, hash: impl FnOnce(StateHashVersion) -> StateHash) {
        if self.state != RecordingState::Active || self.failure.is_some() {
            return;
        }
        if self.force_hash || tick.is_multiple_of(self.hash_every) {
            self.force_hash = false;
            self.hash(tick, hash(self.version));
        }
    }

    fn status(&self) -> RecordingStatus {
        RecordingStatus {
            state: self.state,
            file: self.file.clone(),
            records: self.records,
            last_tick: self.last_tick,
        }
    }

    fn finish(
        &mut self,
        tick: u64,
        reason: &str,
        hash: impl FnOnce(StateHashVersion) -> StateHash,
    ) -> Result<RecordingStatus, RecordError> {
        if self.state == RecordingState::Active {
            let hash = hash(self.version);
            if tick > 0 && self.hashed != Some(tick) {
                self.hash(tick, hash);
            }
            if self.push(Record::End(EndRecord {
                tick,
                reason: reason.to_owned(),
                hash,
            })) {
                self.last_tick = self.last_tick.max(tick);
            }
        }
        self.close();
        match &self.failure {
            Some(message) => Err(RecordError::Failed(message.clone())),
            None => Ok(self.status()),
        }
    }

    /// Flushes and joins the writer; the outcome decides `Finished` or `Failed`.
    fn close(&mut self) {
        if let Some(writer) = self.writer.take() {
            match writer.finish() {
                Ok(_) if self.failure.is_none() => self.state = RecordingState::Finished,
                Ok(_) => {}
                Err(error) => self.fail(&error),
            }
        }
    }
}

/// Pushes the messages as one `inbound` record, or, if the line is too long for the format
/// (`LineTooLong`, ASM-727 i), as two halves recursively: several records of the same tick, in
/// order. A single message that is still too long is an error.
fn push_split(
    push: &mut impl FnMut(Record) -> Result<(), RecordError>,
    tick: u64,
    adapter: &str,
    messages: &[NetMessage],
) -> Result<(), RecordError> {
    let record = Record::Inbound(InboundRecord {
        tick,
        adapter: adapter.to_owned(),
        messages: messages.to_vec(),
    });
    match push(record) {
        Err(RecordError::LineTooLong { .. }) if messages.len() > 1 => {
            let (first, second) = messages.split_at(messages.len() / 2);
            push_split(push, tick, adapter, first)?;
            push_split(push, tick, adapter, second)
        }
        other => other,
    }
}

/// The engine's handle on its recorder, also held by the network manager's tap.
#[derive(Clone)]
pub(crate) struct Live(Arc<Mutex<Core>>);

impl Live {
    fn core(&self) -> MutexGuard<'_, Core> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn command(&self, before_tick: u64, command: RecordedCommand) {
        self.core().command(before_tick, command);
    }

    pub(crate) fn after_tick(&self, tick: u64, hash: impl FnOnce(StateHashVersion) -> StateHash) {
        self.core().after_tick(tick, hash);
    }

    pub(crate) fn status(&self) -> RecordingStatus {
        self.core().status()
    }

    pub(crate) fn finish(
        &self,
        tick: u64,
        reason: &str,
        hash: impl FnOnce(StateHashVersion) -> StateHash,
    ) -> Result<RecordingStatus, RecordError> {
        self.core().finish(tick, reason, hash)
    }

    fn failed(file: PathBuf, error: &RecordError) -> Self {
        Self(Arc::new(Mutex::new(Core::failed(file, error))))
    }

    fn tap(&self) -> Box<dyn InboundTap> {
        Box::new(Tap(self.clone()))
    }
}

/// Reports the manager's inbound side to the recorder; observes only (ADR 0053).
struct Tap(Live);

impl InboundTap for Tap {
    fn polled(&mut self, tick: u64, adapter: &str, messages: &[NetMessage]) {
        self.0.core().inbound(tick, adapter, messages);
    }

    fn faulted(&mut self, tick: u64, adapter: &str, call: &str) {
        self.0.core().fault(tick, adapter, call);
    }
}

impl Engine {
    /// Attaches the recorder: from now on commands, inbound messages, faults and hashes are
    /// recorded in tick order (ADR 0053). Attach before the first tick, after the engine is built
    /// (the header takes the adapters' key spaces from the live network link). A failure to start
    /// writing does not stop anything: the recording is `Failed` and
    /// [`Engine::finish_recording`] reports it.
    ///
    /// # Errors
    /// [`RecordError::Unsupported`] if the engine has a recorder already (it is never replaced).
    pub fn attach_recorder(&mut self, recorder: Recorder) -> Result<(), RecordError> {
        if self.recorder.is_some() {
            return Err(RecordError::Unsupported(
                "the engine already has a recorder".to_owned(),
            ));
        }
        let Recorder {
            options,
            mut header,
            file,
        } = recorder;
        for status in self.network.iter().flat_map(crate::net::NetLink::status) {
            if let Some(adapter) = header
                .network_adapters
                .iter_mut()
                .find(|a| a.name == status.name)
            {
                adapter.key_space = status.key_space;
            }
        }
        let live = Live(Arc::new(Mutex::new(Core::open(&options, &header, file))));
        if let Some(link) = self.network.as_mut() {
            link.set_tap(live.tap());
        }
        self.recorder = Some(live);
        Ok(())
    }

    /// Records that recording to `file` could not be started: the recorder is `Failed` from the
    /// start, so the status says so (ADR 0057). Nothing happens if a recorder is attached.
    pub(crate) fn attach_failed_recorder(&mut self, file: PathBuf, error: &RecordError) {
        if self.recorder.is_none() {
            self.recorder = Some(Live::failed(file, error));
        }
    }

    /// Writes the final `hash` (if the last tick has none) and the `end` record, closes the file
    /// and returns the status at the end. `None` without a recorder. Calling it again returns the
    /// same outcome.
    ///
    /// # Errors
    /// [`RecordError::Failed`] if any write failed during the run.
    pub fn finish_recording(
        &mut self,
        reason: &str,
    ) -> Result<Option<RecordingStatus>, RecordError> {
        let Some(live) = self.recorder.as_ref() else {
            return Ok(None);
        };
        let tick = self.kernel.now().tick();
        live.finish(tick, reason, |version| self.hash_in(version))
            .map(Some)
    }

    /// The recorder's live status, `None` without a recorder.
    #[must_use]
    pub fn recording(&self) -> Option<RecordingStatus> {
        self.recorder.as_ref().map(Live::status)
    }

    /// Records a command applied now, between ticks (`before_tick` = ticks completed); a no-op
    /// without a recorder.
    pub(crate) fn record_command(&self, command: RecordedCommand) {
        if let Some(live) = &self.recorder {
            live.command(self.kernel.now().tick(), command);
        }
    }

    /// Hashes after a tick when the cadence or a command asks for it; a no-op without a recorder.
    pub(crate) fn record_tick(&self) {
        if let Some(live) = &self.recorder {
            live.after_tick(self.kernel.now().tick(), |version| self.hash_in(version));
        }
    }

    /// The state hash in `version`; V2 is the kernel's default encoding.
    fn hash_in(&self, version: StateHashVersion) -> StateHash {
        match version {
            StateHashVersion::V2 => self.kernel.state_hash(),
            other => self.kernel.state_hash_with(other),
        }
    }
}

#[cfg(test)]
mod tests;
