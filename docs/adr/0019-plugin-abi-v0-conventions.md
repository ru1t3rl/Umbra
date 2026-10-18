# ADR 0019: Plugin ABI v0 conventions

- **Status**: Accepted (human project owner, 2026-10-18; proposed by the architect in the M4 plan `docs/plans/M4.md`)
- **Requirement IDs**: REQ-P-1, REQ-P-2, REQ-P-3, REQ-P-6, REQ-P-9, REQ-P-11, REQ-SDK-2, D-6, D-35 (C5), NFR-2
- **Affected**: `umbra-plugin-api` (new), `umbra-plugin-host`, `umbra-sdk`, `umbra_plugin_api.h`, C/C++ plugin authors

## Context
M4 fixes the C ABI; after it REQ-P-2 (append-only within a major) makes every layout choice expensive.
The spec says what must exist (one entry symbol, `abi_version`/`struct_size` first, descriptor contents,
UTF-8 pointer+length strings, free-by-allocator, version checks) but not the rules that make a C ABI safe
to read from Rust: how enums and booleans cross, how versions are packed, which structs carry the header,
which thread calls what, and how long pointers live. ADR 0018 C5 fixed the names (`umbra_`, `Umbra`, `UMBRA_`).

## Options
1. **Literal C conventions, integers only (chosen).** Every value written by the other side is a fixed-width
   integer, float or pointer; enums are `u32` with `UMBRA_*` constants; booleans are `u8`. Pros: no Rust UB
   when a C plugin writes an unknown value (a Rust `enum` or `bool` with an invalid bit pattern is UB);
   cbindgen output is plain C. Cons: the Rust side validates and converts every field.
2. **`#[repr(C)]` Rust enums and `bool` in the ABI.** Pros: nicer Rust. Cons: UB on any out-of-range value
   from C; tagged unions in C are awkward. Rejected.
3. **Serialize everything (JSON or a byte format) per call.** Pros: trivially versioned, out-of-process ready.
   Cons: allocation and parsing per tick per entity, against REQ-B-4 and NFR-1. Kept only for params,
   metadata and `Custom` payloads (REQ-P-6).

## Decision
Option 1, with these rules (they bind every later kind):
- **Version.** `UMBRA_ABI_VERSION_MAJOR = 0`, `UMBRA_ABI_VERSION_MINOR = 0` at M4; `UMBRA_ABI_VERSION =
  major << 16 | minor` (`u32`). Major 0 is a real major: REQ-P-2 and REQ-P-9 apply to it exactly (no
  "0.x may break" exception); going to 1 is a deliberate break with its own ADR. Each new kind or appended
  field bumps the minor (`docs/plugin-abi-changelog.md`).
- **Header.** Every struct that can grow starts with `u32 abi_version; u32 struct_size;` (the version the
  writer was compiled against, and `sizeof` the writer's struct). Readers require `struct_size >=` the
  size of that struct at minor 0, read only fields inside `struct_size`, and treat missing newer fields as
  absent. Exempt, frozen for the major, never grown: the value primitives `UmbraStr {ptr, len}`,
  `UmbraVec3 {x, y, z}`, `UmbraQuat {w, x, y, z}`. Arrays of headed structs use the first element's
  `struct_size` as the stride, and all elements of one array have the same size.
- **Values.** Enums are `u32` typedefs with `UMBRA_*` constants (kinds, statuses, intent kinds, value
  types, log levels); an unknown value is an error, never a cast. Booleans are `u8` (0 or 1, other values
  rejected). Entity IDs are `u64`, 0 = none (ASM-017). Strings are `UmbraStr` (UTF-8, not
  NUL-terminated; `len == 0` allows a null `ptr`); invalid UTF-8 is an error. Every function-pointer
  field is declared nullable in Rust (`Option<unsafe extern "C" fn ..>`), because a null written by C
  into a non-nullable Rust function pointer is undefined behavior; the reader checks that required
  entries are non-null, and optional entries are documented as such.
- **Results.** Fallible calls return `UmbraCode` (`i32`): `UMBRA_OK = 0`, `UMBRA_ERR_PANIC`,
  `UMBRA_ERR_INVALID_ARGUMENT`, `UMBRA_ERR_PARAMS`, `UMBRA_ERR_UNSUPPORTED`, `UMBRA_ERR_INTERNAL`
  (values fixed in `umbra-plugin-api`). A message goes through a host-provided `UmbraErrorSink`
  (copied by the host during the call).
- **Memory (REQ-P-6).** Nothing allocated by one side is freed by the other. Data from host to plugin is
  borrowed for the duration of the call. Data from plugin to host goes through host-provided sinks that
  copy (intents, outputs, error messages), so the v0 host API needs no allocator. Plugin state lives in
  opaque plugin-owned handles (`UmbraBehavior`, `UmbraInstance`) with explicit destroy callbacks.
- **Pointer lifetime (REQ-P-11).** Every pointer the host passes is valid only during that call, except
  the `UmbraHostApi*` given to the entry point, which stays valid until `shutdown` returns. No other
  shared address space is assumed, so an out-of-process or WASM host can marshal the same calls.
- **Threads.** The host never calls one plugin concurrently (it serialises calls per plugin) but may call
  it from different threads over time. Plugins may not call back into the host from their own threads.
- **Panics and unwinding.** No unwind crosses the boundary in either direction: the SDK catches panics in
  every exported function (REQ-P-7), the host catches them in every callback it exports. Plugins must be
  built with `panic = "unwind"` to get error codes; with `abort`, or on a crash, the process dies
  (REQ-P-11, documented).
- **Entry (REQ-P-1).** One symbol, `const UmbraPluginDescriptor* umbra_plugin_entry(const UmbraHostApi*
  host)`; it may be called more than once and must return the same static descriptor.

## Consequences
- Easier: C, C++ and C# authors get a plain header; Rust never reads an invalid enum or `bool`; growth
  is a minor bump with size checks; the batched calls stay allocation-free.
- Harder: hand-written validation for every incoming field (host) and outgoing field (SDK); layout pin
  tests (`size_of`/`offset_of`) and C `_Static_assert`s must be updated deliberately with each bump.
- Revisit for ABI 1.0 (before external users depend on it) or if an out-of-process host needs a
  different lifetime rule.
