---
name: add-plugin-kind
description: Add a new plugin capability kind (e.g. sensor, terrain provider) through the single shared plugin mechanism. Use when extending the plugin ABI with a new kind; never create a separate loader.
---

# Add a plugin capability kind

Requirements: REQ-P-1..11, REQ-SDK-1..4, D-6/D-7/D-8. Owner: `plugin-abi-engineer`; design review: `architect`.

1. **Core trait** in `cgf-core`: small, documented, with a contract test suite (`trait_contract_tests!`) that any implementation can run.
2. **ABI**: add the kind to the `CgfCapabilityKind` enum and a new `#[repr(C)]` vtable in `cgf-plugin-api`. Start with `abi_version` + `struct_size`. Append-only. Design calls to be **batched** where per-entity. Bump the ABI minor version; write the compatibility note in `docs/plugin-abi-changelog.md`.
3. **Host bridge** in `cgf-plugin-host`: adapter implementing the core trait by calling the vtable; panic/error code handling; fault policy; lifecycle (`configure -> init -> call* -> shutdown`); size and null checks.
4. **Registry**: register under the same registry used by built-ins so templates reference it by ID with no special cases (REQ-T-1).
5. **SDK** in `cgf-sdk`: idiomatic Rust trait, wrapper generating the vtable with `catch_unwind` guards, support in `export_plugin!`.
6. **Param schema**: capability exposes JSON Schema for params (REQ-T-3).
7. **Example plugin** in `examples/plugins/<kind>-example` and a minimal C example if the kind is meant for non-Rust authors. Regenerate `cgf_plugin_api.h`.
8. **Tests**: run the contract suite against built-in and example implementations; add the plugin failure matrix cases for this kind; determinism test with the plugin loaded.
9. **Docs**: `docs/plugin-guide.md` section, template reference update.
10. Run `review-change` and `verify-and-build`.

Do not change existing struct layouts. If a break is truly needed, stop and write an ADR proposing an ABI major bump.
