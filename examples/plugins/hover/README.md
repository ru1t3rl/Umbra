# hover (example plugin, motion model)

A third-party-style Umbra plugin (M7, ADR 0056): a Rust `cdylib` built with `umbra-sdk` only and
`#![forbid(unsafe_code)]`. Plugin `example.plugin.hover` 0.1.0, ABI 0.2.

| Capability | Kind | Params |
|---|---|---|
| `example.motion.hover` | `motion_model` | `height_agl_m` (1..500, default 20), `max_speed_mps` (default 15), `max_climb_mps` (default 5), `max_accel_mps2` (default 4) |

A multirotor-style kinematic model. Horizontal speed and heading follow `SetSpeed`, `SetHeading`,
`MoveTo` and `Hold` (with no motion intent the current horizontal velocity is kept, speed changes by at
most `max_accel_mps2 * dt`). The height above the ellipsoid moves toward the terrain height below the
entity plus `height_agl_m` at most `max_climb_mps` per second: the terrain comes from the step call
(`ctx.terrain()`), so the model follows slopes as a built-in would. The model is stateless
(REQ-M-4) and its orientation is a unit quaternion (yaw only).

Status: M7 WI-2 skeleton; the behavior lands in WI-17. Build with `cargo xtask build-plugins` (from
WI-17) and use it from a template: `[motion] model = "example.motion.hover"`.
