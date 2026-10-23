# M7 WI-6 review, round 1 (2026-10-23): REQUEST CHANGES

Temporary file: the open findings of the first code review of WI-6 (`builtin.air.kinematic` and the
`air_sea` example), saved at the pause so the next session can fix them. Delete it when WI-6 is
committed as reviewed (record the outcome in the M7 running log instead).

Reviewer evidence (read-only; on the second PC's scratchpad, not in the repo): adversarial vertical
runs, a 200k-case per-step limit fuzz, airsweep with six adversarial sets, 22 mutants, an example probe.
Re-create equivalents as needed.

## Blocker
**B1. A `min_speed = 0` aircraft (the schema default) cannot change height at rest, and its landing
snap ends any climb.** `crates/umbra-motion/src/air.rs:284-288, 389-404`.
- `vertical()` clamps the climb rate to `new_speed / sqrt(2)`, so at speed 0 the rate is 0. The snap fires
  on horizontal distance only and sets `vertical = here.height`.
- Measured (60/0/5/30 deg, climb 10, 30 Hz, 600 s): at rest, destination straight up +500 m: stays at
  1000.0 m forever. At rest, 50 m away and +1000 m: lands at 1047.5 m and stops. 30 m/s, 2 km away,
  +1000 m: stops at 1411 m. 30 m/s, 2 km away, -900 m: stops at 589 m.
- Floor: a stopped `min_speed = 0` aircraft below terrain + `min_altitude_agl` never climbs.
- Contradicts ADR 0051 (target height = MoveTo ellipsoidal height, never below terrain + agl), ASM-702,
  the `min_altitude_agl_m` schema text, docs/templates.md, REQ-M-2.
- Tests miss it: every arrival test in `tests/air_intents.rs` and the airsweep use level destinations.
- Fix (record an ASM; architect if unclear): make arrival 3D for `min_speed = 0`: snap only when the height
  error is within one vertical step, and decouple the vertical rate from horizontal speed for a
  hover-capable aircraft (e.g. clamp to `max_climb_rate` alone when `min_speed == 0`; heading already
  comes from the orientation), or keep a minimum speed while a height error remains. Add tests:
  destination straight up; 50 m away and +1000 m; stopped aircraft below the floor; a sweep with
  non-level destinations.

## Major
**M1. The WI-1 acceptance test no longer proves the aircraft reached its last waypoint.**
`crates/umbra-engine/tests/air_sea_example.rs:122-131` (ASM-705, approved by the human).
- The WI-1 test was wrong and the code is NOT missing a "route complete" state: `route::steer`
  (`crates/umbra-kernel/src/systems/route.rs:19-35`) never advances `next` past `len - 1`; the established
  convention is `next + 1 == len` (`behavior_swap.rs:103`). Plan WI-6 and ADR 0051 ask for no latched state.
- But for the aircraft `next + 1 == len` only proves waypoint index 2 was reached, and the height check is
  vacuous (waypoints 2 and 3 are both at 1500 m). The aircraft does reach waypoint 3 (first `Hold` at tick
  6589), but nothing asserts it.
- Fix: step tick by tick and assert the aircraft's intents contained `Hold` at some tick (or horizontal
  distance to waypoint 3 <= 150 m), with height within 1 m at that tick.

**M2. The documented loiter does not match the behaviour.** ASM-704, the docs/templates.md air section
and the comment at `air_sea_example.rs:128-130` say "loiters over the last waypoint at its minimum
speed". Measured: after the first `Hold` the aircraft strays up to 1529 m and its speed varies 45..99 m/s
(the follower alternates `Hold` with `SetSpeed(100) + MoveTo`). Fix: document the real pattern
(excursions of about `R(cruise) + cruise^2/a`, speeds between min and cruise).

## Minor
- **m1 untested guards (mutants survived):** `Some(error) if new_speed > 0.0` (air.rs:359; add a
  `SetHeading` test at speed 0 with `min_speed = 0`: no pivot); `floor_radius > 0.0 &&` (air.rs:348;
  one-step test, `min_speed = 0`, destination 0.5 m behind at 10 m/s: it turns); non-finite terrain branch
  (air.rs:278; a `+inf` terrain height from a plugin would climb forever: test `+inf` and NaN).
  Equivalent, no action: `>=` vs `>` at 90 deg (air.rs:243); `sin_e > 0.0` (air.rs:248; add a comment that
  correctness relies on `f64::min` NaN semantics).
- **m2:** ASM-701, docs/templates.md, the step doc (air.rs:312-313) and the module doc omit the landing
  exception: the `min_speed = 0` landing step changes speed by up to `2 * max_accel * dt` (as ground
  documents at ground.rs:222-225). State it.
- **m3:** the ASM-699 path bound is not a bound: 60/59/1/5 deg/30 Hz gives path/bound 1.024 (v0 60,
  d 6086 m, bearing 60 deg); tests pass only thanks to ASM-706's factor 2. Call it an empirical envelope
  quoting 1.024, or fix the formula (stopping overshoot at `R(vmax)` when `vmin ~ vmax`).
- **m4:** `route::is_complete` is position-based, so for an aircraft that leaves the arrival radius a
  `FollowRoute` behavior flaps Success/Running. Record in ASM-704 for M8/behavior authors.

## Nits
- docs/build.md row 4: "...`disable_capability`. from M7 (WI-6..." -> "From".
- ASM-705: "about 170 m above the ellipsoid at 30 km" should read about 1170 m above the ellipsoid, i.e.
  about 70 m above the intended 1100 m (the test comment at tests/air.rs:217-219 is right).
- Example comment "climb is done after 120 s": 1500 m is reached between 120 and 140 s (levels at 1300 m
  until waypoint 1 is passed).
- ADR 0051 says heading from horizontal velocity; the code uses the orientation (air.rs:333), correct at
  rest. Mention in ASM-701.

## Checked and fine
- `min_speed > 0` arrival: no orbit or non-arrival in any adversarial set (min speed 0.5/0.01, bank
  85..89.5 deg, dt 2 s, step > radius, vmin ~ vmax, 0.01 m/s^2 engine); destinations straight up (+3000 m)
  and 100 m away at +20000 m are reached at full climb rate, then flown over.
- Per-step limits (200k fuzz): turn, climb/descent (`min(rate, v/sqrt 2)`), no drop below `min_speed`
  while flying, all outputs finite.
- ground.rs and sea.rs behaviour unchanged (sea only lost `inside_turn_circle`, moved byte-identical).
- Determinism and panic safety; OutOfRange rejects NaN and `max_bank >= 90 deg` and NaN `min_speed`.
- Gate 4 `motion_determinism` addition is consistent and non-vacuous.
- All other mutants killed (every snap guard, idle rule, clamps, lookahead, braking, margin, radius branch).
