# bearing (example plugin, condition)

A third-party-style Umbra plugin (M7, ADR 0056): a Rust `cdylib` built with `umbra-sdk` only and
`#![forbid(unsafe_code)]`. Plugin `example.plugin.bearing` 0.1.0, ABI 0.2.

| Capability | Kind | Params | Blackboard |
|---|---|---|---|
| `example.cond.bearing_within` | `condition` | `target_key` (required), `max_bearing_deg` (default 30) | reads `target_key` (`entity`) |

Success iff the entity held in `target_key` is a current contact whose direction is within
`max_bearing_deg` of the entity's heading (horizontal, inclusive). Failure when the key is unset or
the target is not a contact. The key is declared in the metadata (`blackboard_params`), so the host
grants exactly that key, read only (ADR 0050).

Status: M7 WI-2 skeleton; the behavior lands in WI-18. Use it as a graph node:
`type = "condition"`, `ref = "example.cond.bearing_within"`.
