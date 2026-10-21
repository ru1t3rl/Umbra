//! The in-process loopback adapter: endpoints of one hub deliver to each other (REQ-N-1, ADR 0030).
//!
//! Semantics: every endpoint started from one hub delivers each published message to every other
//! endpoint's bounded queue (dropped and counted at the receiver when full); delivery is
//! synchronous inside `publish`, so a later `poll` sees it and two engines stepped in lockstep are
//! deterministic. The `params` keys `key_space` (`[base, count]`) and `echo` (bool, also deliver to
//! the sender) are read at build.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetError, NetMessage, NetworkAdapter,
    NetworkAdapterConfig,
};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, SimTime};

/// The parameter schema shared by every loopback factory.
const PARAM_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "key_space": {
      "type": "array",
      "items": { "type": "integer", "minimum": 0 },
      "minItems": 2,
      "maxItems": 2
    },
    "echo": { "type": "boolean" }
  },
  "additionalProperties": false
}"#;

/// A poisoned lock only means another thread panicked mid-push; the queue is still a valid queue.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One started endpoint.
#[derive(Debug)]
struct Endpoint {
    /// Unique within the hub (start order).
    token: usize,
    echo: bool,
    queue: Mutex<VecDeque<NetMessage>>,
    rx_dropped: AtomicU64,
    open: AtomicBool,
}

/// A set of loopback endpoints that hear each other.
#[derive(Debug)]
pub struct LoopbackHub {
    capacity: usize,
    capture: Option<Mutex<Vec<(u64, NetMessage)>>>,
    /// Open endpoints in start order.
    endpoints: Mutex<Vec<Arc<Endpoint>>>,
    started: AtomicUsize,
}

impl LoopbackHub {
    fn make(capacity: usize, capture: bool) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            capture: capture.then(|| Mutex::new(Vec::new())),
            endpoints: Mutex::new(Vec::new()),
            started: AtomicUsize::new(0),
        })
    }

    /// A hub whose endpoints queue at most `capacity` inbound messages each. It keeps no capture
    /// list (a long run would grow it without bound, ASM-279).
    #[must_use]
    pub fn new(capacity: usize) -> Arc<Self> {
        Self::make(capacity, false)
    }

    /// Like [`LoopbackHub::new`], and records every published message for [`LoopbackHub::captured`].
    #[must_use]
    pub fn with_capture(capacity: usize) -> Arc<Self> {
        Self::make(capacity, true)
    }

    /// A factory registered under `id`; every `build` creates one endpoint of this hub, with
    /// `key_space` unless the params give one.
    #[must_use]
    pub fn factory(
        self: &Arc<Self>,
        id: ComponentId,
        key_space: KeySpace,
    ) -> Arc<dyn Factory<dyn NetworkAdapterConfig>> {
        Arc::new(LoopbackFactory {
            hub: Arc::clone(self),
            id,
            key_space,
            schema: ParamSchema::from_json_str(PARAM_SCHEMA)
                .unwrap_or_else(|e| unreachable!("the constant schema is valid JSON: {e}")),
        })
    }

    /// Every message published on the hub so far, with the key-space base of the publishing
    /// endpoint (its identity; factory ids are shared), in publish order. Empty unless the hub
    /// was made with [`LoopbackHub::with_capture`] (debugging aid, ASM-238, ASM-279).
    #[must_use]
    pub fn captured(&self) -> Vec<(u64, NetMessage)> {
        self.capture
            .as_ref()
            .map_or_else(Vec::new, |c| lock(c).clone())
    }

    /// How many endpoints have been started (the validate dry run keeps this at 0).
    #[must_use]
    pub fn starts(&self) -> usize {
        self.started.load(Ordering::SeqCst)
    }

    /// Delivers `out` from endpoint `sender` to the open endpoints in start order.
    fn deliver(&self, sender: usize, base: u64, out: &[NetMessage]) {
        if let Some(capture) = &self.capture {
            lock(capture).extend(out.iter().map(|m| (base, m.clone())));
        }
        for ep in lock(&self.endpoints).iter() {
            if (ep.token == sender && !ep.echo) || !ep.open.load(Ordering::SeqCst) {
                continue;
            }
            let mut queue = lock(&ep.queue);
            for m in out {
                if queue.len() < self.capacity {
                    queue.push_back(m.clone());
                } else {
                    ep.rx_dropped.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }
}

struct LoopbackFactory {
    hub: Arc<LoopbackHub>,
    id: ComponentId,
    key_space: KeySpace,
    schema: ParamSchema,
}

/// `[base, count]` of two non-negative integers.
fn parse_key_space(v: &ParamValue) -> Option<KeySpace> {
    match v.as_array()?.as_slice() {
        [base, count] => Some(KeySpace {
            base: base.as_u64()?,
            count: count.as_u64()?,
        }),
        _ => None,
    }
}

impl Factory<dyn NetworkAdapterConfig> for LoopbackFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        let key_space = match params.get("key_space") {
            None => self.key_space,
            Some(v) => parse_key_space(v).ok_or_else(|| {
                BuildError::new("key_space must be [base, count], two non-negative integers")
                    .at("/key_space")
            })?,
        };
        let echo = match params.get("echo") {
            None => false,
            Some(v) => v
                .as_bool()
                .ok_or_else(|| BuildError::new("echo must be a boolean").at("/echo"))?,
        };
        Ok(Arc::new(LoopbackConfig {
            hub: Arc::clone(&self.hub),
            key_space,
            echo,
            started: AtomicBool::new(false),
        }))
    }
}

struct LoopbackConfig {
    hub: Arc<LoopbackHub>,
    key_space: KeySpace,
    echo: bool,
    started: AtomicBool,
}

impl NetworkAdapterConfig for LoopbackConfig {
    fn key_space(&self) -> KeySpace {
        self.key_space
    }

    fn start(&self, _ctx: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        if self.started.swap(true, Ordering::SeqCst) {
            return Err(NetError::AlreadyStarted);
        }
        let endpoint = Arc::new(Endpoint {
            token: self.hub.started.fetch_add(1, Ordering::SeqCst),
            echo: self.echo,
            queue: Mutex::new(VecDeque::new()),
            rx_dropped: AtomicU64::new(0),
            open: AtomicBool::new(true),
        });
        lock(&self.hub.endpoints).push(Arc::clone(&endpoint));
        Ok(Box::new(LoopbackAdapter {
            hub: Arc::clone(&self.hub),
            endpoint,
            base: self.key_space.base,
        }))
    }
}

struct LoopbackAdapter {
    hub: Arc<LoopbackHub>,
    endpoint: Arc<Endpoint>,
    base: u64,
}

impl LoopbackAdapter {
    fn stopped() -> NetError {
        NetError::Faulted("loopback endpoint stopped".to_owned())
    }
}

impl NetworkAdapter for LoopbackAdapter {
    fn publish(&mut self, _now: SimTime, out: &[NetMessage]) -> Result<(), NetError> {
        if !self.endpoint.open.load(Ordering::SeqCst) {
            return Err(Self::stopped());
        }
        self.hub.deliver(self.endpoint.token, self.base, out);
        Ok(())
    }

    fn poll(
        &mut self,
        _now: SimTime,
        max: usize,
        into: &mut Vec<NetMessage>,
    ) -> Result<(), NetError> {
        if !self.endpoint.open.load(Ordering::SeqCst) {
            return Err(Self::stopped());
        }
        let mut queue = lock(&self.endpoint.queue);
        let n = max.min(queue.len());
        into.extend(queue.drain(..n));
        Ok(())
    }

    fn counters(&self) -> AdapterCounters {
        let mut counters = AdapterCounters::default();
        counters.rx_dropped = self.endpoint.rx_dropped.load(Ordering::SeqCst);
        counters
    }

    fn stop(&mut self) {
        if self.endpoint.open.swap(false, Ordering::SeqCst) {
            lock(&self.hub.endpoints).retain(|e| e.token != self.endpoint.token);
            lock(&self.endpoint.queue).clear();
        }
    }
}

impl Drop for LoopbackAdapter {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constant_schema_parses_and_bad_params_are_build_errors() {
        let hub = LoopbackHub::new(4);
        let id = ComponentId::new("test.net.loopback").unwrap();
        let factory = hub.factory(id, KeySpace { base: 0, count: 1 });
        assert!(factory.param_schema().as_json().is_object());
        for bad in [
            serde_json::json!({"key_space": [1]}),
            serde_json::json!({"key_space": [1, -2]}),
            serde_json::json!({"key_space": "x"}),
            serde_json::json!({"echo": 1}),
        ] {
            assert!(factory.build(&bad).is_err(), "{bad}");
        }
    }
}
