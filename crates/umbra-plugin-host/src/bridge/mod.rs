//! The bridges: plugin behavior capabilities as core `Factory<dyn Behavior>`, `Behavior`,
//! `BehaviorInstance` and `BehaviorBatch` (REQ-B-4, REQ-B-5, REQ-B-9, REQ-B-10, REQ-P-5..7, ADR 0019,
//! ADR 0020, ADR 0021), and plugin network adapters as core `Factory<dyn NetworkAdapterConfig>`,
//! `NetworkAdapterConfig` and `NetworkAdapter` (REQ-N-1, REQ-P-6..8, REQ-P-11, ADR 0033).
//!
//! - `factory`: the registered factory (`configure`) and the host's build scope (RR-m2).
//! - `behavior`: the configured behavior (`instantiate`, `batch`).
//! - `instance`: the instance (`tick` as a batch of one, `halt*`, `set_input`, `output`).
//! - `batch`: the per-host capability and its batch call (whole-batch discard, faults).
//! - `marshal`: scratch buffers and conversions between core values and ABI structs.
//! - `sinks`: the intent, value and network message sinks (host callbacks).
//! - `network`: the network adapter factory, configuration and running adapter.
//!
//! Every call into a plugin goes through `LoadedImage::call` (re-entry check, blocking per-image
//! call lock, call record); handles are opaque, never dereferenced by the host.

pub(crate) mod batch;
pub(crate) mod behavior;
pub(crate) mod factory;
pub(crate) mod instance;
pub(crate) mod marshal;
pub(crate) mod network;
pub(crate) mod sinks;

use std::sync::Arc;

use batch::PluginCapability;
use network::NetCapability;

use crate::descriptor::KindVTable;
use crate::fault::{CapabilityKey, HostState};
use crate::lifecycle::LoadedImage;

/// One capability of one host, by kind: what the registered factory holds.
pub(crate) enum HostCapability {
    /// A behavior capability.
    Behavior(Arc<PluginCapability>),
    /// A network adapter capability (ABI 0.1).
    Network(Arc<NetCapability>),
}

impl HostCapability {
    /// Capability `index` of `image`, as host `host` (fault key `key`) sees it.
    pub(crate) fn new(
        image: &Arc<LoadedImage>,
        index: usize,
        key: CapabilityKey,
        host: &Arc<HostState>,
    ) -> Self {
        let image = Arc::clone(image);
        let host = Arc::clone(host);
        match image.descriptor().capabilities[index].vtable {
            KindVTable::Behavior(vtable) => Self::Behavior(Arc::new(PluginCapability::new(
                image, index, key, host, vtable,
            ))),
            KindVTable::Network(vtable) => Self::Network(Arc::new(PluginCapability::new(
                image, index, key, host, vtable,
            ))),
        }
    }
}
