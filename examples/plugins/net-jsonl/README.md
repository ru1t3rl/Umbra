# net-jsonl (example plugin, recording network adapter)

A third-party-style Umbra plugin (M7, REQ-N-8, ADR 0056): a Rust `cdylib` built with `umbra-sdk` only and
`#![forbid(unsafe_code)]`. Plugin `example.plugin.net_jsonl` 0.1.0, ABI 0.1.

| Capability | Kind | Params |
|---|---|---|
| `example.net.jsonl` | `network_adapter` | `mode` (`record` or `playback`) and `path` (required), `queue` (default 4096) |

`record` writes every published message as one JSON line `{"tick": N, "message": {"type": ..}}` from
its own writer thread (bounded queue: a full queue drops the message and counts it in `tx_dropped`);
`playback` reads such a file at `start` and delivers each message at the first poll of its recorded
tick. The encoding is the adapter's own (the SDK cannot see `umbra-record`); the engine recorder
(ADR 0053) is the lossless replay input.

Status: M7 WI-2 skeleton; the behavior lands in WI-19.
