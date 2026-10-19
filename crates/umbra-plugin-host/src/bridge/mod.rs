//! The behavior bridge: plugin behavior capabilities as core `Factory<dyn Behavior>`,
//! `Behavior`, `BehaviorInstance` and `BehaviorBatch` (REQ-B-4, REQ-B-5, REQ-B-9, REQ-B-10,
//! REQ-P-5..7, ADR 0019, ADR 0020, ADR 0021).
//!
//! - `factory`: the registered factory (`configure`) and the host's build scope (RR-m2).
//! - `behavior`: the configured behavior (`instantiate`, `batch`).
//! - `instance`: the instance (`tick` as a batch of one, `halt*`, `set_input`, `output`).
//! - `batch`: the per-host capability and its batch call (whole-batch discard, faults).
//! - `marshal`: scratch buffers and conversions between core values and ABI structs.
//! - `sinks`: the intent and value sinks (host callbacks).
//!
//! Every call into a plugin goes through `LoadedImage::call` (re-entry check, blocking per-image
//! call lock, call record); handles are opaque, never dereferenced by the host.

pub(crate) mod batch;
pub(crate) mod behavior;
pub(crate) mod factory;
pub(crate) mod instance;
pub(crate) mod marshal;
pub(crate) mod sinks;
