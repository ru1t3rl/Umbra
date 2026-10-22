# ADR 0051: Built-in air and sea kinematic models

- **Status**: Proposed (architect, M7 plan `docs/plans/M7.md`, 2026-10-22)
- **Requirement IDs**: REQ-M-1, REQ-M-2, REQ-M-4, REQ-C-3, REQ-C-4, REQ-C-6, REQ-K-4, REQ-T-3, ADR 0003,
  ADR 0016, D-15, D-33
- **Affected**: `umbra-motion` (new `air`, `sea`, private `steer` modules, factories, schemas),
  `umbra-engine` (`builtin_registry`/`standard_registry` registration), `docs/templates.md`

## Context
REQ-M-2 names `air.kinematic` (speed, altitude, climb/bank limits) and `sea.kinematic` (speed, turn radius,
constant surface height). Intent set v1 (D-33) has no altitude intent; `MoveTo` carries an ECEF point
(so a height) and routes carry geodetic waypoints. `builtin.ground.kinematic` is M1 code under the A-001
freeze, and its golden hashes must not move. All non-exact float functions go through `umbra_core::math`
(ADR 0003). Models are stateless across ticks (core contract).

## Options
1. **Two new models with their own parameter sets; altitude from the `MoveTo` height; a new private
   steering helper shared by air and sea; ground untouched (chosen).** Pros: no intent-set or ABI change;
   ground byte-identical; DRY between the two new models. Cons: ground and the helper duplicate a little
   heading logic.
2. **Add `SetAltitude` (intent set v2).** Pros: explicit altitude control. Cons: intent set and ABI bump,
   SDK and host changes, no acceptance need; additive later.
3. **Refactor ground onto the shared helper.** Pros: DRY. Cons: risks the M1-M6 goldens for no gain.

## Decision
Option 1.
- `builtin.air.kinematic`, params (units in key names): `max_speed_mps`, `min_speed_mps` (default 0, must
  be < max), `max_accel_mps2`, `max_climb_rate_mps`, `max_descent_rate_mps` (default = climb),
  `max_bank_deg` (0 < b < 90), `max_turn_rate_dps` (optional cap), `min_altitude_agl_m` (default 0).
  Horizontal: speed changes at `max_accel` toward the cruise speed (`SetSpeed`, else `max_speed` for a
  `MoveTo`), clamped to `[min_speed, max_speed]`; turn rate `g * tan(max_bank) / max(v, 1 m/s)`, capped by
  `max_turn_rate`. Vertical: the target height is the `MoveTo` destination's ellipsoidal height, else the
  current height; climb or descent limited by the rates; never below terrain + `min_altitude_agl`
  (`TerrainProvider::height_at`). `Hold`: decelerate to `min_speed`, keep heading and height. Orientation:
  heading from the horizontal velocity, pitch = flight-path angle, roll = the bank used in this step.
- `builtin.sea.kinematic`, params: `max_speed_mps`, `max_accel_mps2`, `min_turn_radius_m`,
  `surface_height_m` (default 0, ellipsoidal). Turn rate `v / min_turn_radius` (no turn at rest), no
  reversing, height pinned to `surface_height_m` (terrain ignored, `MoveTo` height ignored), pitch and roll
  0. `Hold` decelerates to a stop.
- Intent reduction for both follows the ground rules (ASM-030, ADR 0016: last of `MoveTo`/`SetHeading`
  steers, `Hold` wins, non-finite values ignored or "stop").
- Both pass `motion::contract::check_motion_model`, property tests (limits hold every step, determinism)
  and reference cases computed by hand; JSON Schemas via `umbra schema` and the registry (REQ-T-3).
- Registered in `builtin_registry()` and `standard_registry()` as additive entries (catalog and component
  lists grow; no frozen test pins their exact length; WI sweep checks).

## Consequences
- Easier: aircraft and ships in templates with no code; the GUI's template browser (M8) gets their schemas
  from `GET /components` with no contract change.
- Harder: two more models to keep within limits; the air model's bank is instantaneous (no roll rate),
  documented.
- Revisit with a `SetAltitude` intent (intent set v2), roll-rate limits, or rigid-body models (REQ-M-3).
