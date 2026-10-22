# ADR 0059: Plugin-side math: `umbra_sdk::math` mirrors `umbra_core::math` (amends ADR 0003's "single point")

- **Status**: Accepted by the human 2026-10-22 (proposed by architect, M7 WI-2 review). It amends accepted
  ADR 0003 (D-21 era determinism scope, "one `umbra-core` math module ... the single point where `libm`
  would be swapped in").
- **Requirement IDs**: REQ-K-4, A-2, REQ-A-3, REQ-SDK-1, REQ-SDK-3, NFR-2, ADR 0003, ADR 0050, ADR 0056
- **Affected**: `umbra-sdk` (new `math` module), `clippy.toml` (reason text), a parity test in
  `umbra-plugin-host/tests` (or `umbra-engine/tests`), `examples/plugins/*`, `umbra new-plugin` templates,
  `docs/plugin-guide.md`

## Context
ADR 0003 makes the `umbra-core` math module the only place that calls non-exact float functions
(`sin`, `atan2`, `powf`, and so on). The workspace `clippy.toml` bans the direct `f64` methods
(`disallowed-methods`), so the module is the one swap point for `libm` if cross-OS determinism is ever
needed.

M7's example plugins need trigonometry (hover heading, radar elevation gate, bearing condition, dispatch
offset). They are workspace members, so the lint applies to them. They may depend only on `umbra-sdk`,
and the SDK only on `umbra-plugin-api` (REQ-A-3, CLAUDE.md rule 4), so `umbra_core::math` is out of reach.
Third-party plugins have the same need, and ADR 0003 already noted that plugins "cannot be forced" to
follow the rule.

## Options
1. **`umbra_sdk::math` mirrors `umbra_core::math` (chosen).**
   - Shape: same function names and `f64` signatures (`wrap_pi` takes and returns `f64` radians, because
     the SDK has no `Radians`), implemented the same way.
   - One module-level `#![expect(clippy::disallowed_methods, reason = "ADR 0003, ADR 0059: SDK mirror of
     umbra_core::math")]`.
   - A parity test in a crate that can see both asserts bit-identical results.
   - Pros: plugins get the same discipline and results as the engine; one exception site per side;
     REQ-A-3 kept.
   - Cons: two swap points instead of one. The parity test ties them together.
2. **A local `#[expect]` in each example and generated plugin.**
   - Pros: no SDK API.
   - Cons: the exception repeats in every plugin and template (DRY); plugins drift from engine math.
3. **Re-export `umbra_core::math` from the SDK, or move math into a shared crate both depend on.**
   - Cons: the SDK would link an engine crate (breaks REQ-A-3 and CLAUDE.md rule 4), or a new shared crate
     would be needed in the dependency rules for 16 small functions.
4. **A host math service over the ABI.**
   - Pros: exactly one implementation.
   - Cons: an FFI call per operation; a new ABI surface for pure functions.

## Decision
Option 1.
- **Module:** `umbra_sdk::math` exposes `sin`, `cos`, `sin_cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
  `hypot`, `cbrt`, `powi`, `powf`, `exp`, `ln` and `wrap_pi`, with the same semantics and implementations
  as `umbra_core::math`. It has the one module-level `#![expect]` above and nothing else is exempt.
- **Lint text:** `clippy.toml` reasons read "REQ-K-4, ADR 0003: use umbra_core::math (umbra_sdk::math in
  plugins)".
- **Parity test:** `umbra-plugin-host/tests/sdk_math_parity.rs` compares every function bit for bit on
  fixed edge cases (±0, subnormals, ±π boundaries, large arguments, NaN as a canonical class) and on a
  proptest sample. Both swap points must change together (a `libm` switch changes both modules in one
  commit), and the parity test enforces it.
- **Users:** the example plugins (WI-17..WI-19) and the `umbra new-plugin` templates (WI-20) use
  `umbra_sdk::math`. `docs/plugin-guide.md` tells third parties to use it.
- **Plan placement:** delivered in M7 WI-10 (SDK work). WI-17 and WI-20 depend on it.

## Consequences
- **Easier:** plugins produce the engine's float results on the same OS (A-2) without lint exceptions.
- **Harder:** a second module to maintain in lockstep; a `libm` change touches two crates (guarded by the
  parity test).
- **Migration:** none. ADR 0003 gains a note that the single point becomes "one point per side of the ABI,
  kept identical by test".
- **Revisit:** if cross-OS determinism is adopted (both modules switch to `libm` together), or if a shared
  dependency-free math crate becomes acceptable under the dependency rules.
