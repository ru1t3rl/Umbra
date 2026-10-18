# ADR 0019: Plugin ABI v0 conventions

- **Status**: Accepted (human project owner, 2026-10-18; proposed by the architect in the M4 plan `docs/plans/M4.md`).
  Amended 2026-10-18 after the ABI design review (approved by the human): rules changed in place are marked
  "(amended: R..)" with the review finding; summary at the end.
- **Requirement IDs**: REQ-P-1, REQ-P-2, REQ-P-3, REQ-P-6, REQ-P-7, REQ-P-9, REQ-P-11, REQ-SDK-2, D-6, D-35 (C5), NFR-2
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
  field bumps the minor (`docs/plugin-abi-changelog.md`). *(amended: R-M10)* ABI 0.0 is **provisional**
  until the C example (M4 WI-14) passes: until then the M4 work items may change the layout without a new
  ADR (plan, pins and changelog updated together); the field-by-field freeze review is part of the M4
  close-out, after which these rules are binding.
- **Platforms.** *(amended: R-M7)* v0 supports 64-bit targets only (x86_64 and aarch64; Linux, Windows,
  macOS best effort), where `extern "C"` equals `extern "system"`. `umbra-plugin-api` asserts
  `size_of::<usize>() == 8` at compile time and the header asserts `sizeof(void*) == 8`. 32-bit is not
  supported.
- **Header.** Every struct that can grow starts with `u32 abi_version; u32 struct_size;` (written as two
  literal fields, not a nested struct: the version the writer was compiled against, and `sizeof` the
  writer's struct). *(amended: R-B1)* **No implicit padding, internal or trailing**: fields are ordered and
  explicit `reserved_N` fields (writers zero them, readers ignore them) fill every gap, so that every byte
  below `struct_size` belongs to a named field and a field appended in a later minor can never be confused
  with padding; every headed struct's size is a multiple of 8. *(amended: R-M2)* A reader requires
  `struct_size` to be at least the struct's minor-0 size, at most the reader's own size for that struct's
  declared minor, and a multiple of 8, and the major in that struct's own `abi_version` to equal the
  reader's major; it reads only fields inside `struct_size` and treats missing newer fields as absent.
  Readers never compare `abi_version` for equality. *(amended: R-B3)* **The host writes every struct it
  passes to a plugin in the layout of the plugin's declared minor** (from an append-only size-per-minor
  table in the host), so a plugin may index host-written arrays with its own `sizeof`. Arrays a plugin
  writes for the host use the first element's `struct_size` as the stride, and all elements of one array
  have the same size. Exempt from the header, frozen for the major, never grown: the value primitives
  `UmbraStr {ptr, len}` (16 bytes), `UmbraVec3 {x, y, z}` (24), `UmbraQuat {w, x, y, z}` (32).
- **Appended fields and entries.** *(amended: review minor)* Every field or vtable entry appended in a later
  minor is optional, with a documented meaning when absent (or zero). The SDK writes its own minor, so a
  plugin built with a newer SDK needs a host of at least that minor even if it uses no new feature
  (recorded in the changelog; a "minimum required minor" field may be appended later).
- **Values.** Enums are `u32` typedefs with `UMBRA_*` constants (kinds, statuses, intent kinds, value
  types, log levels); an unknown value is an error, never a cast. Booleans are `u8` (0 or 1, other values
  rejected). Entity IDs are `u64`, 0 = none (ASM-017). Strings are `UmbraStr` (UTF-8, not
  NUL-terminated; `len == 0` allows a null `ptr`); invalid UTF-8 is an error. Every function-pointer
  field is declared nullable in Rust (`Option<unsafe extern "C" fn ..>`), because a null written by C
  into a non-nullable Rust function pointer is undefined behavior; the reader checks that required
  entries are non-null, and optional entries are documented as such. *(amended: R-B2)* Out-values the
  plugin writes for the host travel in separate plain arrays, never inside a struct the host also reads,
  and every out-enum has `0 = UNSET`, so "not written" is detectable. *(amended: review minor)* Values
  crossing as `UmbraValue` keep the core `BbValue` invariants (finite numbers, non-zero entity, booleans
  0/1, valid UTF-8).
- **Bounds.** *(amended: R-M2)* Before turning any (pointer, length) into a slice the reader requires
  `len <= isize::MAX`, a non-null pointer when `len > 0`, 8-byte alignment for arrays of structs, and a
  `checked_mul` for `count * stride`. Caps (constants in `umbra-plugin-api`): at most 256 capabilities per
  plugin, IDs at most `MAX_ID_LEN` (128) bytes, a param schema at most 1 MiB, metadata at most 64 KiB, at
  most 64 intents per batch item per call (`UMBRA_MAX_INTENTS_PER_ITEM`); error messages are truncated to
  4 KiB at a character boundary, decoded lossily. The host copies the descriptor, capabilities, their
  strings and the vtable prefix it knows (`min(struct_size, own size)`) into owned values at load and
  never re-reads that plugin memory.
- **Results.** Fallible calls return `UmbraCode` (`i32`): `UMBRA_OK = 0`, `UMBRA_ERR_PANIC = 1`,
  `UMBRA_ERR_INVALID_ARGUMENT = 2`, `UMBRA_ERR_PARAMS = 3`, `UMBRA_ERR_UNSUPPORTED = 4`,
  `UMBRA_ERR_INTERNAL = 5`, `UMBRA_ERR_STATE = 6` (a call outside its lifecycle or call scope; amended:
  R-B4). A message goes through a host-provided `UmbraErrorSink` (copied by the host during the call;
  amended: review minor: the first message of a call wins, later ones are ignored).
- **Memory (REQ-P-6).** Nothing allocated by one side is freed by the other. Data from host to plugin is
  borrowed for the duration of the call. Data from plugin to host goes through host-provided sinks that
  copy (intents, outputs, error messages), so the v0 host API needs no allocator. Plugin state lives in
  opaque plugin-owned handles (`UmbraBehavior`, `UmbraInstance`) with explicit destroy callbacks; the host
  never dereferences them.
- **Tokens instead of host pointers.** *(amended: R-B4)* Every host context a plugin can hold is an opaque
  `u64` token, never a host pointer: `host_ctx` (one per loaded image), sink contexts and RNG tokens (one
  per batch item and call). The host checks each token against its process table and the plugin's active
  call record (slot and generation); a stale, foreign or out-of-call token returns an error code
  (`UMBRA_ERR_STATE` or `UMBRA_ERR_INVALID_ARGUMENT`) and faults the capability, it is never dereferenced.
- **Pointer lifetime (REQ-P-11).** Every data pointer the host passes is valid only during that call.
  *(amended: R-B4)* The `UmbraHostApi*` given to the entry point points at a process-lifetime static (one
  per loaded image, never freed), so it is valid forever; calls through it outside the plugin's lifecycle
  (before `init`, after `shutdown`) return `UMBRA_ERR_STATE`. No other shared address space is assumed, so
  an out-of-process or WASM host can marshal the same calls.
- **Threads and re-entry.** The host never calls one plugin concurrently (it serialises calls per plugin)
  but may call it from different threads over time; C plugins must not rely on thread-local state.
  Plugins may not call back into the host from their own threads. *(amended: review minor)* Host callbacks
  are leaf calls: they never call into any plugin. A plugin call that would re-enter the same plugin is
  detected (in-call flag or `try_lock`) and is a fault, never a deadlock; a poisoned lock is handled
  explicitly (the capability is faulted).
- **Panics and unwinding.** No unwind crosses the boundary in either direction: the SDK catches panics in
  every exported function (REQ-P-7), the host catches them in every callback it exports. Plugins must be
  built with `panic = "unwind"` to get error codes; with `abort`, or on a crash, the process dies
  (REQ-P-11, documented). *(amended: R-M1)* Functions stay `extern "C"` (not `"C-unwind"`): since Rust
  1.81 a panic escaping an `extern "C"` function aborts the process, which is the backstop. C++ exceptions
  escaping into the host are undefined behavior, so plugin functions must be `noexcept`; `longjmp` across
  host frames is forbidden; a panic in `Drop` during unwinding aborts. The SDK extracts the panic message
  and then drops the payload inside a nested `catch_unwind` (or forgets it), so a panicking payload `Drop`
  cannot escape. If the SDK's `umbra_plugin_entry` panics it returns null.
- **Entry (REQ-P-1).** One symbol, `const UmbraPluginDescriptor* umbra_plugin_entry(const UmbraHostApi*
  host)`; it may be called more than once and must return the same pointer to the same static descriptor
  (amended: R-M2; a different pointer on a second call is a load error). The host calls it once per
  loaded image per process (ADR 0021, amended: R-B5).
- **Header text.** *(amended: R-M6)* The header must compile with `-std=c11 -Wall -Wextra -Werror -pedantic`
  and MSVC `/W4 /WX`: opaque handles are declared as incomplete types (`typedef struct UmbraBehavior
  UmbraBehavior;`), never zero-length arrays; `usize` maps to `size_t` (not `uintptr_t`); it provides
  `UMBRA_EXPORT` (dllexport / default visibility) and `UMBRA_STR("literal")` for C authors (avoids the
  `sizeof("x")` off-by-one and `char*`/`uint8_t*` sign warnings). Line endings are LF.

## Consequences
- Easier: C, C++ and C# authors get a plain header; Rust never reads an invalid enum or `bool`; growth
  is a minor bump with size checks that cannot be fooled by padding; the batched calls stay
  allocation-free; no plugin can make the host dereference a stale host pointer.
- Harder: hand-written validation for every incoming field (host) and outgoing field (SDK); reserved
  fields; a size-per-minor table in the host; layout pin tests and generated C `_Static_assert`s must be
  updated deliberately with each bump.
- Revisit for ABI 1.0 (before external users depend on it), for 32-bit targets, or if an out-of-process
  host needs a different lifetime rule.

## Amended 2026-10-18 after the ABI design review (approved by the human)
Findings of the plugin-abi-engineer's review applied above: R-B1 no implicit padding, explicit
`reserved_N`, sizes multiples of 8; R-B2 out-values in plain arrays with `0 = UNSET`; R-B3 host writes in
the plugin's declared minor, readers check only major and `struct_size`; R-B4 opaque `u64` tokens for every
host context, process-lifetime `UmbraHostApi`, `UMBRA_ERR_STATE`; R-B5 one entry call per image (details
in ADR 0021); R-M1 unwinding rules (C++ exceptions, `longjmp`, payload drop, entry panic returns null); R-M2
bounds, caps and copy-at-load; R-M6 header portability (`UMBRA_EXPORT`, `UMBRA_STR`, incomplete handle
types, `size_t`, LF); R-M7 64-bit only, generated C layout asserts (plan WI-4); R-M10 ABI 0.0 provisional
until WI-14; minors: optional appended fields, SDK writes its own minor, re-entry and poisoning, first
error message wins, `BbValue` invariants; nits: literal header fields, `UMBRA_STR`.
