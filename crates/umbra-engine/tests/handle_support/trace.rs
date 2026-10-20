//! A process-wide `tracing` capture for the handle tests: plugin log events (target
//! `umbra_plugin`, the host's default log) and declarations of spans named `tick` (the engine thread's per-tick span). Installed once per test binary; tests that read it hold [`super::plugin_lock`] when they
//! compare positions, so concurrent tests do not interleave plugin events.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

/// A span as declared: its name, level and declared field names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanSeen {
    pub name: String,
    pub level: Level,
    pub fields: Vec<String>,
}

#[derive(Default)]
struct Store {
    plugin_messages: Mutex<Vec<String>>,
    spans: Mutex<Vec<SpanSeen>>,
    next_id: AtomicU64,
}

/// The handle to what was captured so far.
#[derive(Clone)]
pub struct Capture(Arc<Store>);

struct Collector(Arc<Store>);

struct Message(Option<String>);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

impl Subscriber for Collector {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let metadata = attributes.metadata();
        // Only the handle's `tick` span is kept: the kernel declares several spans per tick.
        if metadata.name() == "tick" {
            self.0
                .spans
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(SpanSeen {
                    name: metadata.name().to_owned(),
                    level: *metadata.level(),
                    fields: metadata
                        .fields()
                        .iter()
                        .map(|f| f.name().to_owned())
                        .collect(),
                });
        }
        Id::from_u64(self.0.next_id.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() != "umbra_plugin" {
            return;
        }
        let mut message = Message(None);
        event.record(&mut message);
        if let Some(text) = message.0 {
            self.0
                .plugin_messages
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(text);
        }
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// Installs the collector as the global default (once) and returns its handle.
pub fn install() -> Capture {
    static CAPTURE: OnceLock<Capture> = OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let store = Arc::new(Store::default());
            tracing::subscriber::set_global_default(Collector(Arc::clone(&store)))
                .expect("no other global subscriber in a test binary");
            Capture(store)
        })
        .clone()
}

impl Capture {
    /// The plugin log messages so far, in order.
    pub fn plugin_messages(&self) -> Vec<String> {
        self.0
            .plugin_messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The spans declared so far.
    pub fn spans(&self) -> Vec<SpanSeen> {
        self.0
            .spans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
