# radar (example plugin, sensor)

A third-party-style Umbra plugin (M7, ADR 0056): a Rust `cdylib` built with `umbra-sdk` only and
`#![forbid(unsafe_code)]`. Plugin `example.plugin.radar` 0.1.0, ABI 0.2.

| Capability | Kind | Params |
|---|---|---|
| `example.sensor.radar` | `sensor` | `range_m` (required), `min_elevation_deg` (default -90), `max_elevation_deg` (default 90), `antenna_height_m` (default 2), `detect_probability` (default 1) |

For each candidate (id order) inside the range and the elevation gate and with terrain line of sight
(`ctx.terrain().line_of_sight`, both ends raised by `antenna_height_m` along the vertical like
`builtin.visual`'s eye height), the radar draws one `next_f64()` from the observer's own stream
(`ctx.rng()`, REQ-K-4) and reports a contact iff the draw is below `detect_probability`. A candidate
rejected by a gate draws nothing.

Status: M7 WI-2 skeleton; the behavior lands in WI-17. Use it from a template:
`[[sensors]] kind = "example.sensor.radar"`.
