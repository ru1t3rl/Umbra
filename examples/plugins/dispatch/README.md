# dispatch (example plugin, action)

A third-party-style Umbra plugin (M7, ADR 0056): a Rust `cdylib` built with `umbra-sdk` only and
`#![forbid(unsafe_code)]`. Plugin `example.plugin.dispatch` 0.1.0, ABI 0.2.

| Capability | Kind | Params | Blackboard |
|---|---|---|---|
| `example.action.dispatch` | `action` | `template` and `done_key` (required), `delay_s` (default 2), `offset_forward_m` (default 20) | writes `done_key` (`bool`) |

Like `builtin.action.wait` it is Running for `ceil(delay_s * hz)` ticks from its first tick; on the next
tick it emits `Spawn { template, position, heading }` ahead of the entity (ADR 0052), writes
`done_key = true` and succeeds. `halt` resets it, so a graph that halts it before the delay (for example
under a `timeout`) never spawns.

Status: M7 WI-2 skeleton; the behavior lands in WI-18. Use it as a graph node:
`type = "action"`, `ref = "example.action.dispatch"`.
