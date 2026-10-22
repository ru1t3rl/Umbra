//! Network adapters: engine reports to wire adapters (REQ-X-5, ADR 0035).

use umbra_api_types as api;
use umbra_engine::handle::AdapterReport;

/// The wire adapter. The engine's counters keep their meaning; the fault reason is the manager's
/// text, which names no path.
#[must_use]
pub fn adapter(report: &AdapterReport) -> api::AdapterDto {
    let s = &report.status;
    api::AdapterDto {
        name: s.name.clone(),
        adapter: report.adapter.as_str().to_owned(),
        plugin: report.plugin.clone(),
        enabled: s.enabled,
        faulted: s.faulted,
        fault_reason: s.fault_reason.clone(),
        key_space: api::KeySpaceDto {
            base: s.key_space.base,
            count: s.key_space.count,
        },
        ghosts: s.ghosts as u64,
        counters: api::AdapterCountersDto {
            published: s.published,
            received: s.received,
            echo_dropped: s.echo_dropped,
            unmapped: s.unmapped,
            keys_exhausted: s.keys_exhausted,
            ghosts_capped: s.ghosts_capped,
            control_ignored: s.control_ignored,
            world_errors: s.world_errors,
            unsupported: s.unsupported,
            invalid_state: s.invalid_state,
            tx_dropped: s.counters.tx_dropped,
            rx_dropped: s.counters.rx_dropped,
            decode_errors: s.counters.decode_errors,
            io_errors: s.counters.io_errors,
        },
    }
}
