//! Contract checks every [`NetworkAdapterConfig`] implementation must
//! pass (REQ-N-1, ADR 0030).
//!
//! [`network_adapter_contract_tests!`](crate::network_adapter_contract_tests) generates the tests
//! for one implementation from the checks [`check_start_once`], [`check_after_stop`] and
//! [`check_poll_max`] (or [`check_poll_max_fed`] when the adapter can be fed messages).

use std::sync::Arc;

use super::{AdapterStart, NetError, NetMessage, NetworkAdapter, NetworkAdapterConfig};
use crate::event::ControlAction;
use crate::time::{DEFAULT_TICK_RATE_HZ, SimTime, TickRate};

fn t0() -> Result<SimTime, String> {
    TickRate::new(DEFAULT_TICK_RATE_HZ)
        .map(SimTime::start)
        .ok_or_else(|| "default tick rate is invalid".to_owned())
}

fn start_ctx() -> Result<AdapterStart, String> {
    Ok(AdapterStart { now: t0()? })
}

/// A second `start` on the same configuration returns [`NetError::AlreadyStarted`].
///
/// # Errors
/// A description of the violation.
pub fn check_start_once(config: &dyn NetworkAdapterConfig) -> Result<(), String> {
    let mut first = config
        .start(&start_ctx()?)
        .map_err(|e| format!("first start failed: {e}"))?;
    let second = config.start(&start_ctx()?);
    first.stop();
    match second {
        Err(NetError::AlreadyStarted) => Ok(()),
        Err(other) => Err(format!(
            "second start returned {other:?}, want AlreadyStarted"
        )),
        Ok(mut extra) => {
            extra.stop();
            Err("second start succeeded".to_owned())
        }
    }
}

/// After `stop`, `publish` and `poll` return errors without panicking, `stop` can be repeated,
/// and `poll` appends nothing.
///
/// # Errors
/// A description of the violation.
pub fn check_after_stop(config: &dyn NetworkAdapterConfig) -> Result<(), String> {
    let mut adapter = config
        .start(&start_ctx()?)
        .map_err(|e| format!("start failed: {e}"))?;
    adapter.stop();
    adapter.stop();
    let now = t0()?;
    let msg = NetMessage::SimulationControl(ControlAction::Pause);
    if adapter.publish(now, &[msg]).is_ok() {
        return Err("publish after stop returned Ok".to_owned());
    }
    let mut into = Vec::new();
    if adapter.poll(now, 8, &mut into).is_ok() {
        return Err("poll after stop returned Ok".to_owned());
    }
    if !into.is_empty() {
        return Err("poll after stop appended messages".to_owned());
    }
    Ok(())
}

/// `poll` appends at most `max` messages, for `max` of 0, 1 and 2, and keeps what was in `into`.
/// With no inbound traffic this only proves the bound loosely; use [`check_poll_max_fed`] for
/// adapters that can be fed.
///
/// # Errors
/// A description of the violation.
pub fn check_poll_max(config: &dyn NetworkAdapterConfig) -> Result<(), String> {
    poll_max(config, None)
}

/// A feed: hands the started adapter three inbound messages to deliver.
pub type Feed<'a> = &'a dyn Fn(&mut dyn NetworkAdapter, &[NetMessage]);

/// Like [`check_poll_max`], but `feed` first queues three messages, and `poll` must then return
/// exactly `max` of them for `max` of 0, 1 and 2 (so the three are consumed in order).
///
/// # Errors
/// A description of the violation.
pub fn check_poll_max_fed(config: &dyn NetworkAdapterConfig, feed: Feed<'_>) -> Result<(), String> {
    poll_max(config, Some(feed))
}

fn poll_max(config: &dyn NetworkAdapterConfig, feed: Option<Feed<'_>>) -> Result<(), String> {
    let mut adapter = config
        .start(&start_ctx()?)
        .map_err(|e| format!("start failed: {e}"))?;
    if let Some(feed) = feed {
        let queued = [
            ControlAction::Start,
            ControlAction::Pause,
            ControlAction::Resume,
        ]
        .map(NetMessage::SimulationControl);
        feed(adapter.as_mut(), &queued);
    }
    let result = poll_max_rounds(adapter.as_mut(), feed.is_some());
    adapter.stop();
    result
}

fn poll_max_rounds(adapter: &mut dyn NetworkAdapter, fed: bool) -> Result<(), String> {
    let now = t0()?;
    let marker = NetMessage::SimulationControl(ControlAction::Stop);
    for max in 0..=2usize {
        let mut into = vec![marker.clone()];
        adapter
            .poll(now, max, &mut into)
            .map_err(|e| format!("poll(max={max}) failed: {e}"))?;
        let got = into.len().saturating_sub(1);
        if into.first() != Some(&marker) || got > max || (fed && got != max) {
            return Err(format!("poll(max={max}) appended {got} messages"));
        }
    }
    Ok(())
}

/// Lets the contract macro take a concrete configuration or an `Arc<dyn NetworkAdapterConfig>`.
pub trait AsAdapterConfig {
    /// The configuration as a trait object.
    fn as_adapter_config(&self) -> &dyn NetworkAdapterConfig;
}

impl<T: NetworkAdapterConfig> AsAdapterConfig for T {
    fn as_adapter_config(&self) -> &dyn NetworkAdapterConfig {
        self
    }
}

impl AsAdapterConfig for Arc<dyn NetworkAdapterConfig> {
    fn as_adapter_config(&self) -> &dyn NetworkAdapterConfig {
        self.as_ref()
    }
}

/// Generates the `#[test]`s of the adapter contract for the configuration built by `$make`
/// (an expression evaluated once per test, so every test gets a fresh, not yet started one). It
/// may be a concrete configuration or an `Arc<dyn NetworkAdapterConfig>`.
///
/// The second form, `feed = $feed`, takes an expression of type
/// `Fn(&mut dyn NetworkAdapter, &[NetMessage])` that makes the adapter deliver the given
/// messages to `poll`; the poll test then checks `max` exactly instead of loosely. Adapters
/// with an inbound path must use it.
///
/// The `attrs = [#[ignore = "reason"]], ...` form puts the attributes on every generated test
/// (ASM-280). The generated test names are fixed, so each call needs its own `mod`.
///
/// ```ignore
/// mod loopback { umbra_core::network_adapter_contract_tests!(MyConfig::new()); }
/// ```
#[macro_export]
macro_rules! network_adapter_contract_tests {
    (attrs = [$(#[$m:meta])*], $make:expr) => {
        $crate::network_adapter_contract_tests!(@tests [$(#[$m])*] $make, |c| {
            $crate::net::contract::check_poll_max(c)
        });
    };
    (attrs = [$(#[$m:meta])*], $make:expr, feed = $feed:expr) => {
        $crate::network_adapter_contract_tests!(@tests [$(#[$m])*] $make, |c| {
            $crate::net::contract::check_poll_max_fed(c, &$feed)
        });
    };
    ($make:expr) => {
        $crate::network_adapter_contract_tests!(attrs = [], $make);
    };
    ($make:expr, feed = $feed:expr) => {
        $crate::network_adapter_contract_tests!(attrs = [], $make, feed = $feed);
    };
    (@tests [$(#[$m:meta])*] $make:expr, $poll:expr) => {
        $(#[$m])*
        #[test]
        fn contract_start_twice_is_already_started() {
            let config = $make;
            let c = $crate::net::contract::AsAdapterConfig::as_adapter_config(&config);
            assert_eq!($crate::net::contract::check_start_once(c), Ok(()));
        }

        $(#[$m])*
        #[test]
        fn contract_publish_and_poll_after_stop_error() {
            let config = $make;
            let c = $crate::net::contract::AsAdapterConfig::as_adapter_config(&config);
            assert_eq!($crate::net::contract::check_after_stop(c), Ok(()));
        }

        $(#[$m])*
        #[test]
        fn contract_poll_respects_max() {
            let config = $make;
            let c = $crate::net::contract::AsAdapterConfig::as_adapter_config(&config);
            let check: &dyn Fn(&dyn $crate::net::NetworkAdapterConfig) -> Result<(), String> =
                &$poll;
            assert_eq!(check(c), Ok(()));
        }
    };
}
