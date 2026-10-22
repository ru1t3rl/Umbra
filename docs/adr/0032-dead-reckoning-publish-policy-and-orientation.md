# ADR 0032: Dead reckoning, publish policy and world-frame orientation

- **Status**: Accepted by the human 2026-10-20 (proposed by the architect, M6 plan `docs/plans/M6.md`, 2026-10-20)
- **Requirement IDs**: REQ-N-2, REQ-N-4, REQ-N-6 (dead reckoning, ECEF conversion), REQ-K-2, REQ-K-4,
  REQ-K-5, REQ-C-3, D-15, D-16, ADR 0003, ADR 0007
- **Affected**: `umbra-core` (`dead_reckoning` module, `WorldEuler` conversions in `coords`/`geo`),
  `umbra-kernel` (remote track, Ingest-phase system, `spawn_remote`, `update_remote`), `umbra-net`
  (`policy`)

## Context
Ghosts are dead-reckoned only (D-16, REQ-K-5) and the publisher sends updates when the receiver's
extrapolation would drift past a threshold, with a heartbeat (REQ-N-2). Both sides must use the same
extrapolation, so it must live where the kernel (ghosts) and `umbra-net` (policy) can both reach it
without either depending on the other. Positions are already ECEF (D-15), as DIS and RPR-FOM use; the
orientation in the protocols is three Euler angles from the ECEF axes, while Umbra stores a body FRD to
ECEF quaternion. Non-exact float functions may only be called through `umbra_core::math` (ADR 0003),
which a plugin cannot reach (REQ-A-3).

## Options
1. **Extrapolation and conversions in `umbra-core`, ghost extrapolation in the kernel Ingest phase,
   policy in `umbra-net`, adapters exchange world-frame Euler angles (chosen).**
2. **Ghosts as entities with a "dead reckoning" motion model.** Motion runs only for Owned entities
   (REQ-K-5); making it run for Remote ones blurs D-16. Rejected.
3. **Quaternion on the wire to plugins.** Every adapter then needs its own trigonometry outside
   `umbra_core::math` (a determinism and DRY problem). Rejected.

## Decision
Option 1.
- **Models** (`umbra_core::dead_reckoning::DeadReckoning`, neutral names; each adapter maps them to its
  codes): `Static`, `Velocity` (constant velocity, world), `VelocityRotation`, `AccelerationRotation`,
  `Acceleration`, and the body-frame variants `BodyVelocity`, `BodyVelocityRotation`,
  `BodyAccelerationRotation`, `BodyAcceleration`. Function `extrapolate(basis: &RemoteState, elapsed:
  Seconds) -> KinematicState`: position `p + v t (+ a t^2 / 2)`, velocity `v (+ a t)`, orientation rotated
  by `angular_velocity * t` (body axes, exact exponential map of a constant rate) for the rotating
  variants, unchanged otherwise. World-frame models are exact. Body-frame models are, in M6, computed as
  their world-frame counterparts after rotating the body acceleration into ECEF with the basis
  orientation (ASM-229); `elapsed` is capped at `[ghosts] timeout_s`.
- **Remote state** (`umbra_core::dead_reckoning::RemoteState { state: KinematicState, acceleration: Vec3,
  angular_velocity: Vec3, model: DeadReckoning }`).
- **Kernel** (additive): a `RemoteTrack { basis: RemoteState, at: SimTime }` component on Remote
  entities; `Kernel::spawn_remote(RemoteInit) -> Result<EntityId, KernelError>` (ownership `Remote`,
  motion model for identity only, force, no behavior or sensors), `Kernel::update_remote(id, RemoteState)
  -> Result<(), KernelError>` (`UnknownEntity`, new `NotRemote` for an Owned id: REQ-K-5 guard), basis
  time = the current tick's start. The Ingest phase sets each tracked Remote entity's kinematic state to
  `extrapolate(basis, now - at)` in `EntityId` order, before Perception. The track is not hashed; the
  state it produces is (state hash stays v2; no golden contains a Remote entity, ASM-230).
- **Orientation** (`umbra_core`): `WorldEuler { yaw, pitch, roll }` and `Quaternion::to_world_euler` /
  `Quaternion::from_world_euler`, the z-y'-x'' decomposition of the body FRD to ECEF rotation, through
  `umbra_core::math`; tested against hand-computed cases (axes, gimbal edges at pitch +-90 degrees) and a
  round-trip property. Adapters only copy the three angles (DIS `psi, theta, phi`; RPR `Orientation`).
- **Publish policy** (`umbra_net::policy`, per adapter, deterministic, simulation time only):
  `[publish]` keys `position_threshold_m` (default 1.0), `orientation_threshold_deg` (3.0),
  `heartbeat_s` (5.0), `min_interval_s` (0.0 = every tick allowed), `dead_reckoning` (`velocity`;
  published models: `static`, `velocity`, `acceleration`; the rotating ones are accepted inbound only in
  M6, ASM-231), `control` (false: publish run-control actions). For each Owned entity, in `EntityId`
  order, after the tick: publish if it was never published, or the time since its last update reached
  the heartbeat, or the threshold is passed (`|p_true - p_dr|` or the angle between `q_true` and `q_dr`,
  where `dr` = `extrapolate(last sent, now - sent_at)` with the same function the receiver uses), and
  `min_interval_s` has passed. Acceleration for `acceleration` is the finite difference of the last two
  ticks' velocities. Removed owned entities publish `EntityRemoved` once. Nothing is published while
  paused (no ticks).
- **Units and frames at the edge** (REQ-C-4): canonical messages are SI and ECEF; timestamps are
  derived from `SimTime` by the adapter (no wall clock; ASM-237).

## Consequences
- Easier: sender and receiver extrapolate identically by construction; adapters need no trigonometry and
  stay in safe, simple code; the publish rate adapts to motion.
- Harder: ghosts jump to the received state at each update (no smoothing; ASM-234); body-frame models
  are approximated in M6.
- Revisit if interoperability tests show body-frame models in use (exact Annex-style integration), if
  smoothing is wanted for display, or when M7 hashes v3.
