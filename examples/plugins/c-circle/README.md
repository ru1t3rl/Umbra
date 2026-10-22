# c-circle (C example plugin)

An Umbra plugin written in plain C11 against the generated header
`crates/umbra-plugin-api/include/umbra_plugin_api.h` (M4 acceptance 3, REQ-SDK-2, REQ-P-1, ADR 0023
item 3). It shows that the plugin boundary is a C ABI: nothing here depends on Rust or on `umbra-sdk`.
Plugin `example.plugin.c_circle` 0.1.0, ABI 0.1.

| Capability | Kind | Params | Interface |
|---|---|---|---|
| `example.behavior.c_circle` | `behavior` | `speed_mps` (number > 0, default 5), `turn_rate_dps` (number in [-3600, 3600], default 6) | no bindable params, no outputs; Running forever; no `halt_running` entry |

Every tick the behavior emits `SET_SPEED(speed_mps)` and `SET_HEADING(heading)` with
`heading = turn_rate_dps * tick / rate_hz` degrees, wrapped into [0, 360) and converted to radians. The
heading is a function of simulation time only, so the entity drives a circle of radius
`speed_mps / turn_rate_rad_per_s` (10 m/s at 12 deg/s: about 48 m, once every 30 s). Positive turn rates
turn clockwise seen from above.

## Build and run
```
cargo xtask build-plugins            # or --release; writes target/plugins/c-circle/ (and wander/)
cargo run -p umbra-cli -- list-plugins --plugin-path target/plugins
cargo run -p umbra-cli -- run examples/scenarios/plugin_c_circle/scenario.toml --plugin-path target/plugins
```

`build-plugins` compiles `plugin.c` with the `cc` crate and the same strict flags the host tests use:

| Compiler | Flags |
|---|---|
| GCC, Clang | `-std=c11 -pedantic -Wall -Wextra -Werror -ffp-contract=off -fvisibility=hidden -shared -fPIC` (`-O2` with `--release`) |
| MSVC | `/std:c11 /W4 /WX /fp:precise /LD` (`/O2` with `--release`) |

By hand on Linux, from the repository root:
```
cc -std=c11 -pedantic -Wall -Wextra -Werror -ffp-contract=off -fvisibility=hidden -shared -fPIC \
   -I crates/umbra-plugin-api/include examples/plugins/c-circle/plugin.c -o libc_circle.so
nm -D --defined-only libc_circle.so      # exactly one symbol: umbra_plugin_entry
```
A package is a directory holding the library and this `plugin.toml` (ADR 0022). The host reads the
manifest before it opens the library; `id`, `version` and `abi` must equal the descriptor in
`plugin.c`.

## Walkthrough of `plugin.c`
The file is in seven numbered sections; read it top to bottom next to this list.

1. **Layout checks.** A few `_Static_assert`s on struct sizes and one offset. The ABI is 64-bit only
   and has no implicit padding (every gap is a named `reserved_N` field), so these numbers are the same
   with GCC, Clang and MSVC. The complete set of asserts is generated from the Rust definitions and
   compiled by the host tests (`c_header.rs`); a plugin does not need its own.
2. **Helpers.** `HOST_STRUCT_OK` is the reader rule of ADR 0019 for structs the host writes: the major
   in `abi_version` must equal `UMBRA_ABI_VERSION_MAJOR` and `struct_size` must be at least the size
   this file was compiled against. The minor is never compared (a newer host writes the plugin's
   minor anyway). `ELEMENT_OK` is the stricter rule for elements of host-written arrays (`items`):
   `struct_size` must **equal** this file's size, because the plugin indexes the array with its own
   `sizeof` and the host writes array elements in the plugin's minor (R-B3, the same check the Rust
   SDK makes, ASM-128). `fail` reports a message through the call's error sink and returns the error code;
   the host copies the message during the call. `g_host` is the only global: the host API pointer,
   set by `init` and cleared by `shutdown`, because one process may unload and load the library again.
3. **Params.** The host passes the params as compact JSON text: an `UmbraStr` (pointer plus length,
   UTF-8, **not** NUL-terminated, borrowed for the call). The small parser accepts what the schema
   allows (an object with `speed_mps` and `turn_rate_dps`, or `null`) and rejects anything else with
   `UMBRA_ERR_PARAMS`, which the host turns into a build error naming the capability. Numbers are
   checked against the JSON grammar first and then copied into a NUL-terminated buffer for `strtod`,
   so the parser never reads past `len`. The parser assumes the "C" numeric locale (`.` as the decimal
   point, which a Rust host keeps): if an embedder calls `setlocale` with a locale that uses `,`,
   `strtod` stops early, the end check fails and `configure` returns `UMBRA_ERR_PARAMS` (a build error),
   so a number is never misread. A real plugin would use a JSON library.
4. **The circle.** `heading_rad` is a pure function of the params, the tick and the rate. The plugin
   uses no wall clock and no RNG of its own (REQ-K-4); a plugin that needs randomness draws it from the
   entity's stream with `host->rng_next_f64(host->host_ctx, item->rng, &out)`, using the token of the
   batch item it is ticking. `wrap_degrees` writes every floating-point step as its own statement and
   the build passes `-ffp-contract=off`, so no compiler fuses a multiply and an add into an FMA, which
   would make results depend on the CPU. The code needs no `libm`.
5. **The vtable.** `UmbraBehavior` and `UmbraInstance` are incomplete types in the header: the host
   only stores and passes back the pointers, and this file decides what they are (a `malloc`ed
   `Circle`). The rules the functions follow:
   - `configure` and `instantiate` write their result through `out` and return `UMBRA_OK`, or return an
     error code with a message; on error `*out` stays null. Every handle they create is freed by the
     matching `destroy_*` function in this library (memory is freed by the side that allocated it).
   - `tick_batch` checks the batch and the intent sink, then, for each item, pushes its intents with
     the item's index and writes one status into the host-owned `statuses` array. The host pre-fills
     `UMBRA_STATUS_UNSET`; a status left unset is a plugin fault. Intents are built zeroed (`{0}`), so
     unused fields and `reserved_0` are zero, with this file's own `abi_version` and `struct_size`.
     If a push is refused the function returns that code: the host has already recorded the reason,
     faults the capability and discards the whole batch.
   - `halt` is the full reset of an instance; it must be idempotent and safe before the first tick.
     This instance has no state between ticks, so it only checks its argument.
   - The vtable is a `static const` built with designated initializers, so every entry not named is
     null. `halt_running` is null on purpose: the host then calls `halt` when a parent halts the
     behavior (ADR 0020; the test `c_example_without_halt_running_gets_halt`). `set_input` and `output`
     are null because the metadata declares no `param_types` and no `outputs`: an entry is present if
     and only if the metadata declares its map.
   - Callbacks into C cannot unwind, so there is nothing to catch. A C++ plugin must make every
     function it exposes `noexcept`, and no plugin may `longjmp` across the boundary (ADR 0019).
6. **The descriptor.** `UMBRA_STR_INIT("...")` makes an `UmbraStr` from a string literal in a static
   initializer (`UMBRA_STR("...")` is the expression form, used for messages). The capability carries
   its id, an empty version (meaning: the plugin's version), the params schema (JSON Schema, ADR 0009
   subset, whose top-level `description` becomes the catalog text), the metadata
   (`{"result":"Running forever"}`; ADR 0020 keys `param_types`, `outputs`, `result`, every key
   optional, unknown keys rejected) and the vtable. Every struct starts with `abi_version =
   UMBRA_ABI_VERSION` and `struct_size = sizeof(that struct)`; reserved fields are left zero. The
   descriptor and everything it points to are statics that live as long as the library, as the host
   requires until `shutdown` returns. `init` stores the host API and logs `c_circle ready` at debug
   level through it; `shutdown` forgets it.
7. **The entry.** `umbra_plugin_entry` is the only exported symbol (REQ-P-1): every other function
   and object is `static`, and `UMBRA_EXPORT` (`__declspec(dllexport)` on Windows,
   `visibility("default")` elsewhere) exports it even when the build hides everything else
   (`-fvisibility=hidden`). The header declares the entry, so the compiler checks this definition
   against the ABI's entry type. It returns the same descriptor pointer on every call. It does not use
   the host API: host callbacks made before `init` are refused with `UMBRA_ERR_STATE`.

## What the host checks
After calling `umbra_plugin_entry` (twice, to check that the descriptor pointer is stable) and before
`init`, the host validates everything above: the manifest, the descriptor's ABI version (major equal,
minor not newer), every `struct_size`, the ids and their grammar, UTF-8 and size caps of every string,
that the schema is a JSON object (the supported keyword subset is checked when the scenario loader
starts, as `LoaderError::Schema`, ASM-114), that the metadata matches the vtable, and that the
required vtable entries are non-null. A mistake is a load
error naming the package and the field (for example `capabilities[0].vtable.halt`). During a run a
non-OK return, an unset status or a refused intent is a plugin fault handled by the scenario's
`on_plugin_fault` policy (`abort` by default, or `disable_capability`).
