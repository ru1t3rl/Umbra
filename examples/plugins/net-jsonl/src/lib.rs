//! `example-net-jsonl`: the REQ-N-8 recording network adapter as an example plugin (M7 WI-19,
//! REQ-N-8, REQ-N-1, REQ-SDK-3, ADR 0033, ADR 0056, ASM-637).
//!
//! Plugin `example.plugin.net_jsonl` 0.1.0 (ABI 0.1), built with `umbra-sdk` only and
//! `#![forbid(unsafe_code)]`. One network adapter, `example.net.jsonl`:
//!
//! - params `mode` (`"record"` or `"playback"`, required), `path` (string, required: the JSON Lines
//!   file), `queue` (integer in [1, 1000000], default 4096: the bounded outbound queue of record mode);
//!   the key space is `(1_000_000, 1_000_000)` in both modes;
//! - **record**: `publish` hands every message to a bounded queue that its own writer thread drains to
//!   `path` (created or truncated at `start`), one line per message in publish order:
//!   `{"tick": <now.tick>, "message": <message>}` with `<message>` one of
//!   `{"type":"entity_state","key":N,"entity_type":null|"..","force":null|"..","name":null|"..",
//!   "position":[x,y,z],"velocity":[..],"acceleration":[..],"angular_velocity":[..],
//!   "orientation":{"yaw":..,"pitch":..,"roll":..},"dead_reckoning":"<snake_case variant, e.g. static, velocity_rotation>"}`,
//!   `{"type":"entity_removed","key":N}`,
//!   `{"type":"fire","shooter":N,"target":null|N,"weapon":"..","position":[..]}`,
//!   `{"type":"detonation","shooter":null|N,"target":null|N,"position":[..]}` and
//!   `{"type":"simulation_control","action":"start"|"pause"|"resume"|"stop"}`; a full queue drops the
//!   message and counts it in `tx_dropped` (it is a debugging aid, never blocking the tick); a write
//!   error counts in `io_errors`; `stop` flushes the queue and joins the writer;
//! - **playback**: `start` reads `path` completely (no thread); `poll` at tick `t` delivers, in file
//!   order, the messages recorded at ticks `<= t` that were not delivered yet, at most `max` per call; a
//!   line that does not parse counts in `decode_errors`; nothing is published anywhere;
//! - its encoding is its own: the SDK cannot see `umbra-record` (REQ-A-3), and the engine recorder
//!   (ADR 0053) is the lossless replay input.
//!
//! Status: M7 WI-2 skeleton. The contract above is what
//! `crates/umbra-engine/tests/plugin_net_jsonl.rs` checks; WI-19 implements it.

#![forbid(unsafe_code)]

use umbra_sdk::{
    ConfigureInput, Counters, Error, Inbox, KeySpace, NetMessage, NetworkAdapter,
    NetworkAdapterConfig, NetworkAdapterFactory, SimTime, export_plugin,
};

/// The capability `example.net.jsonl`.
struct JsonlFactory;

/// A configured adapter that has not started.
struct JsonlConfig;

/// A started adapter.
struct Jsonl;

impl NetworkAdapterFactory for JsonlFactory {
    const ID: &'static str = "example.net.jsonl";
    // ADR 0009 subset; the top-level `description` is the catalog text.
    const PARAM_SCHEMA: &'static str = r#"{
  "title": "example.net.jsonl parameters",
  "description": "Recording adapter: `record` writes every published message to a JSON Lines file, `playback` replays such a file as inbound messages at the recorded ticks.",
  "type": "object",
  "properties": {
    "mode": { "type": "string", "enum": ["record", "playback"],
      "description": "`record` writes `path`, `playback` reads it." },
    "path": { "type": "string", "minLength": 1,
      "description": "The JSON Lines file." },
    "queue": { "type": "integer", "minimum": 1, "maximum": 1000000, "default": 4096,
      "description": "Record mode: messages queued for the writer before new ones are dropped." }
  },
  "required": ["mode", "path"],
  "additionalProperties": false
}"#;
    type Config = JsonlConfig;

    fn configure(input: &ConfigureInput) -> Result<JsonlConfig, Error> {
        let _ = input;
        unimplemented!("M7 WI-19")
    }
}

impl NetworkAdapterConfig for JsonlConfig {
    type Adapter = Jsonl;

    fn key_space(&self) -> KeySpace {
        unimplemented!("M7 WI-19")
    }

    fn start(&mut self, now: SimTime) -> Result<Jsonl, Error> {
        let _ = now;
        unimplemented!("M7 WI-19")
    }
}

impl NetworkAdapter for Jsonl {
    fn publish(&mut self, now: SimTime, out: &[NetMessage<'_>]) -> Result<(), Error> {
        let _ = (now, out);
        unimplemented!("M7 WI-19")
    }

    fn poll(&mut self, now: SimTime, inbox: &mut Inbox<'_>) -> Result<(), Error> {
        let _ = (now, inbox);
        unimplemented!("M7 WI-19")
    }

    fn counters(&self) -> Counters {
        unimplemented!("M7 WI-19")
    }

    fn stop(&mut self) {
        unimplemented!("M7 WI-19")
    }
}

export_plugin! {
    id: "example.plugin.net_jsonl",
    name: "JSON Lines network example",
    version: "0.1.0",
    network_adapters: [JsonlFactory],
}
