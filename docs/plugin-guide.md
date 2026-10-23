# Plugin guide

How to write, build, package and run native Umbra plugins against **ABI 0.0** (M4), **0.1** (M6,
ADR 0033) and **0.2** (M7, ADR 0050). A plugin is a 64-bit dynamic library that exports one symbol,
`umbra_plugin_entry`, and offers **capabilities**: `behavior`, from ABI 0.1 `network adapter`
([network adapters](#writing-a-network-adapter-in-rust-abi-01)), and from ABI 0.2 `motion model`,
`sensor`, `condition` and `action` ([ABI 0.2 kinds](#motion-models-sensors-conditions-and-actions-in-rust-abi-02)). Once loaded, a plugin behavior is used exactly like any registered
behavior: as a template `[behavior] ref` and as a `subgraph` target (D-8, REQ-B-9).

Decisions behind this guide:
[ADR 0019](adr/0019-plugin-abi-v0-conventions.md) (ABI conventions),
[ADR 0020](adr/0020-behavior-plugin-kind-and-batching.md) (behavior kind, batching),
[ADR 0021](adr/0021-host-api-v0-lifecycle-and-fault-policy.md) (host API, lifecycle, faults),
[ADR 0022](adr/0022-plugin-discovery-packaging-and-scenario-keys.md) (discovery, packages, scenario keys),
[ADR 0023](adr/0023-plugin-tooling-crates-and-supply-chain.md) (tooling, crates, supply chain);
details in `docs/assumptions.md` ASM-111..ASM-148. Reference material: the header
`crates/umbra-plugin-api/include/umbra_plugin_api.h` (generated, every field documented), the SDK rustdoc
(`cargo doc -p umbra-sdk --open`), `docs/plugin-abi-changelog.md`.

Examples (the code this guide walks through; it is not copied here):
- Rust: [`examples/plugins/wander`](../examples/plugins/wander/src/lib.rs), run by
  `examples/scenarios/plugin_wander`;
- C: [`examples/plugins/c-circle`](../examples/plugins/c-circle/plugin.c) and its
  [README](../examples/plugins/c-circle/README.md), run by `examples/scenarios/plugin_c_circle`.

Contents: [trust model](#trust-model-and-in-process-risk), [packages](#packages-discovery-and-plugintoml),
[build and run](#build-and-run), [Rust SDK](#writing-a-plugin-in-rust-umbra-sdk),
[network adapters](#writing-a-network-adapter-in-rust-abi-01),
[ABI 0.2 kinds](#motion-models-sensors-conditions-and-actions-in-rust-abi-02), [C](#writing-a-plugin-in-c), [ABI rules](#abi-rules), [tokens and lifetimes](#tokens-and-lifetimes),
[threads](#threads-and-re-entry), [panics](#panics-c-exceptions-and-longjmp), [faults](#faults),
[determinism](#determinism), [state and reload](#state-reload-and-thread-locals),
[params](#params-schemas-and-bindable-params), [behaviors in graphs](#plugin-behaviors-in-graphs),
[caps](#caps), [not in v0](#not-in-v0), [platforms](#platforms).

## Trust model and in-process risk
Plugins are **trusted code** (REQ-P-11, NFR-4). They run in the engine process, with its memory and
permissions; there is no sandbox (out-of-process and WASM hosts are not in v1, spec 1.3). Loading a
package runs its code: the entry point and `init` run at load, and `umbra validate` loads plugins and
dry-runs their `configure`. A crash, an abort, memory corruption, a C++ exception or a `longjmp` across
the boundary takes the whole process down. Only load plugins you would link into the engine.

What the host does protect against: it validates every value a plugin writes (sizes, versions, enums,
UTF-8, bounds, caps), never dereferences plugin handles, gives plugins opaque tokens instead of host
pointers, and turns error codes and caught panics into faults that follow the scenario's policy
(ADR 0019, ADR 0021). The ABI passes only data pointers that are valid for one call, so an
out-of-process host can be added later without changing it (REQ-P-11).

## Packages, discovery and `plugin.toml`
A **plugin path** is a directory. Each immediate subdirectory is one **package**: exactly one library
with the platform extension (`.so`, `.dll`, `.dylib`, matched by extension only) and optionally a
`plugin.toml` (ADR 0022).

```
target/plugins/           <- the plugin path given to --plugin-path
  wander/                 <- one package
    libexample_wander.so
    plugin.toml
  c-circle/
    libc_circle.so
    plugin.toml
```

- **Order** (deterministic, REQ-K-4): CLI `--plugin-path` entries first, then the scenario's
  `plugin_paths`, duplicates dropped by canonical path; inside a path, packages by the UTF-8 bytes of
  their directory names. A non-UTF-8 name is a `Discovery` error on every OS (ASM-121).
- **Errors** (load errors, exit 1): a missing plugin directory, zero or two libraries in a package, the
  same plugin ID from two libraries (any paths), the same library reached twice in one host (two paths
  or a symlink; found before its entry is called). Symlinked packages and libraries are followed;
  identity is the canonical library path (ASM-136).
- **`plugin.toml`** is optional and checked **before** the library is opened:

  | Key | Notes |
  |---|---|
  | `schema_version` | `1`, required |
  | `id` | plugin ID (ADR 0002 grammar), required |
  | `version` | semantic version, required |
  | `abi` | `"<major>.<minor>"`, required; a major that differs or a newer minor than the host's is rejected without opening the library (REQ-P-9) |
  | `name` | optional |

  Unknown keys are errors. After loading, the descriptor's `id`, `version` and ABI must equal the
  manifest (`ManifestMismatch`). See [`examples/plugins/wander/plugin.toml`](../examples/plugins/wander/plugin.toml).

**Load sequence** (ADR 0021): read and check `plugin.toml`; open the library (Linux
`RTLD_NOW | RTLD_LOCAL`, Windows `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`,
always an absolute path, ADR 0023) or reuse an image already loaded in this process; call
`umbra_plugin_entry` exactly twice (both calls must return the same pointer); copy and validate the
whole descriptor; call `init`; register every capability in the one registry with
`Provenance::Plugin` (all or nothing). Any failure is a load error naming the package and the field
(for example `capabilities[0].vtable.halt`).

## Build and run
```
cargo xtask build-plugins [--release]      # writes target/plugins/<name>/ for every example (gate 12)
umbra list-plugins --plugin-path target/plugins [--json]
umbra validate examples/scenarios/plugin_wander --plugin-path target/plugins
umbra run examples/scenarios/plugin_wander/scenario.toml --plugin-path target/plugins
```
- `build-plugins` builds `example-wander` with cargo and compiles `c-circle/plugin.c` with the `cc`
  crate, then writes one package per plugin (library plus the example's `plugin.toml`) under the cargo
  target directory (`docs/build.md`). Your own plugin needs only the layout above; `build-plugins` knows
  the examples only.
- `--plugin-path DIR` (repeatable) is accepted by `run`, `validate` and `list-plugins`. A scenario names
  its own directories with `[scenario] plugin_paths` (relative to the scenario file) and its fault
  policy with `on_plugin_fault` (`docs/templates.md`). The example scenarios have no `plugin_paths`,
  because a build directory is not portable (ASM-113).
- `list-plugins [SCENARIO]` prints `<id> <version> abi=<major>.<minor> path=<path>`, one indented
  `<kind> <id> <version>` line per capability and `N plugin(s)`; `--json` prints
  `{"plugins":[{"id","name","version","abi","path","capabilities":[{"kind","id","version"}]}]}` (ASM-146).
- `run` logs one `info: loaded plugin <id> <version> (abi <major>.<minor>) from <path>` line per plugin
  on stderr with `--log-level info`; stdout is unchanged. Plugin `log` messages go to the engine log
  (target `umbra_plugin`, field `plugin`).
- **Exit codes**: 0 success (also a run under `disable_capability` in which a capability faulted);
  1 invalid input, a load error or a plugin fault under `abort`; 2 usage error.

## Writing a plugin in Rust (`umbra-sdk`)
The SDK (REQ-SDK-1, D-7) gives safe traits; it owns every `unsafe` block, the descriptor, the vtables
and the panic guards. Read [`examples/plugins/wander/src/lib.rs`](../examples/plugins/wander/src/lib.rs)
next to this list.

1. **Crate.** A `cdylib` (`[lib] crate-type = ["cdylib"]`, see the example's `Cargo.toml`) that depends
   on `umbra-sdk` only and starts with `#![forbid(unsafe_code)]`. You never write `unsafe`: the entry
   point that `export_plugin!` generates is an `unsafe extern "C" fn` whose `unsafe` comes from the
   SDK's macro, which `forbid(unsafe_code)` accepts (ASM-116). Keep the default `panic = "unwind"`
   ([panics](#panics-c-exceptions-and-longjmp)).
2. **Factory** (`WanderFactory`, trait `BehaviorFactory`): `ID` is the capability's component ID (ADR
   0002; `builtin.*` is refused); `PARAM_SCHEMA` is the params' JSON Schema in the ADR 0009 subset, whose
   top-level `description` becomes the catalog text; optional `VERSION` (default: the plugin version).
   `interface()` declares bindable params, outputs and the result text: wander declares `speed_mps`
   (number) bindable, output `turns` (integer) and "Running forever". `configure` turns the params JSON
   (`Params::value`) into the configured behavior. Apply the schema's defaults yourself and reject bad
   values with `Error::params` (wander's `number` closure); the host turns that into a build error.
3. **Configured behavior** (`Wander`, trait `Behavior`): `instantiate(rate_hz)` creates one instance
   per entity. Do rate-dependent work here, once: wander converts `turn_every_s` into a tick count with
   its `ticks_for` ([period-to-ticks rule](#determinism)).
4. **Instance** (`WanderInstance`, trait `BehaviorInstance`):
   - `tick(ctx, entity, out)` returns `Status` and appends intents with `out.push(Intent::..)`.
     `ctx.tick()` and `ctx.rate_hz()` are the time; `ctx.rng().next_f64()` / `next_u64()` draw from the
     entity's own stream through the host (wander's heading draw). `entity` is the read-only pre-tick
     view (position, velocity, orientation, force, route, `contacts()`).
   - `halt()` is the instance halt, a full reset; it must be idempotent and safe before the first tick
     (wander resets timer, heading and `turns`). `halt_running()` defaults to `halt()`
     ([halt vs halt_running](#plugin-behaviors-in-graphs)).
   - `set_input(param, value)` receives bound params (`None` unsets: wander falls back to the configured
     speed); `output(port)` reports outputs (`turns`). Declare both in `interface()`; the SDK puts
     `set_input`/`output` in the vtable only when params/outputs are declared (ADR 0020).
   - An `Err` from `set_input`, a refused `push` or a refused draw is a **fault** of the capability
     ([faults](#faults)); wander documents that a non-positive bound `speed_mps` faults it.
5. **Export** once per crate with `export_plugin! { id, name, version, behaviors: [..] }` (end of the
   example). Optional `init: fn() -> Result<(), umbra_sdk::Error>` and `shutdown: fn()` run once per
   lifecycle; reset any global state in `init` ([state and reload](#state-reload-and-thread-locals)).
   `umbra_sdk::log(Level, &str)` logs through the host (a no-op outside a lifecycle).
6. **Package**: the library plus a `plugin.toml` whose `id`, `version` and `abi = "0.0"` match the
   macro (wander's `plugin.toml`).

What the SDK does for you (crate docs of `umbra-sdk`): validates every host struct before any trait
call (nulls, alignment, major, `struct_size`, exact element stride, bounds, UTF-8, enums, value
invariants), catches every panic, writes the minor of each capability's kind into everything it
produces for it (behavior 0, network adapter 1, motion model, sensor, condition and action 2; public as
`umbra_sdk::BEHAVIOR_MINOR` and so on) and the highest of them into the descriptor (ADR 0033,
ASM-403, ASM-406), tags handles per capability, and ends a `tick_batch` call at the first refused draw or
intent (ASM-128..ASM-131).

## Writing a network adapter in Rust (ABI 0.1)
A network adapter connects a scenario to a network (REQ-N-1, ADR 0030, ADR 0033): the engine hands it
the owned entities' states and events (`publish`) and takes the remote ones from it (`poll`). It is an
ordinary capability of kind 3 and needs a host of ABI 0.1 or newer. The crate docs of `umbra-sdk` have a
complete minimal adapter; the fixture `crates/umbra-test-plugin/src/network.rs` (a mirror) is a fuller
one, and `crates/umbra-net-dis` (the DIS adapter, [`docs/net/dis.md`](net/dis.md)) is a complete real one:
UDP sockets, two I/O threads, bounded queues, built on the SDK only. What the engine does around an adapter
(publish policy, ghosts, identity and key spaces, echo handling, several adapters at once, determinism,
faults, the control API) and the rules every adapter follows are in the network adapter guide,
[`docs/net/README.md`](net/README.md); this section covers the SDK side only.

1. **Traits** (they mirror the engine's `NetworkAdapterConfig` and `NetworkAdapter` and the ABI
   lifecycle `configure -> key_space -> start -> (publish | poll | counters)* -> stop -> destroy`):
   - `NetworkAdapterFactory`: `ID`, `PARAM_SCHEMA`, optional `VERSION` as for behaviors, and
     `configure(&ConfigureInput)`. The input is the ADR 0030 wrapper: `name()` (the scenario's adapter
     name), `params()` (validated against `PARAM_SCHEMA` by the host) and `templates()` (each template's
     `[network.<name>]` table, by template ID). Refuse bad params with `Error::params`; the scenario
     check reports it at the adapter's `params` table, naming the capability. **No I/O here**: a dry run
     (`umbra validate`) builds without starting.
   - `NetworkAdapterConfig`: `key_space()` (built with `KeySpace::new(base, count)`, which refuses an
     empty or overflowing range; local entities get keys in `(base, base + count]`) and
     `start(&mut self, SimTime) -> Adapter`, where sockets are opened and I/O threads started. The SDK
     calls `start` at most once.
   - `NetworkAdapter`: `publish(now, &[NetMessage])`, `poll(now, &mut Inbox)`, optional `counters()`
     (tx dropped, rx dropped, decode errors, I/O errors) and `stop()`, which must join every thread the
     adapter started. The SDK calls `stop` at most once and always before the adapter drops (also when
     the host destroys a started adapter without `stop`), and drops the adapter before its
     configuration.
   Host-free tests: `SimTime::new(tick, rate_hz)` and `ConfigureInput::from_parts(name, params,
   templates)` are public, and `umbra_sdk::serde_json` re-exports the JSON crate the params use, so an
   adapter's `configure`, `start` and `publish` can be driven in a plain test (`poll` needs a host sink).
   `umbra_sdk::MANAGER_TEMPLATE_KEYS` lists the keys of a template's `[network.<name>]` table that the engine
   itself reads (`entity_type`); every other key is the adapter's to define and validate.
2. **Messages** (`NetMessage`: entity state, entity removed, fire, detonation, run control) borrow their
   strings; ECEF metres, SI units, orientation as yaw, pitch, roll in radians, `None` for "not sent".
   Keep what you need from `publish` as owned data (the borrow ends with the call).
3. **Polling.** Push received messages with `inbox.push(&message)`, at most `inbox.remaining()` per call
   (the host's `max`, capped at `UMBRA_MAX_NET_POLL`); keep the rest for the next poll. `push` checks
   the message first (`NetMessage::validate`: subject keys never 0, `Some(NetKey(0))` and `Some("")` are
   refused, finite numbers, strings at most `UMBRA_MAX_ID_LEN` bytes, a non-empty `FIRE` weapon): an
   invalid message or a full inbox is an `Err` that faults nothing and sends nothing, so a garbled
   datagram can be counted as a decode error. The host additionally checks two grammars (ADR 0002)
   that the SDK does not, so check them before pushing (map other values to `None`, or count the
   datagram as a decode error): a `force` is one ID segment, `[a-z][a-z0-9_]*` (for example `blue`,
   `opfor_2`); a `weapon` is a namespaced ID, `segment ("." segment)+` with the same segment rule, so at
   least two segments (for example `acme.cannon_120`); both at most `UMBRA_MAX_ID_LEN` (128) bytes, no
   upper case, hyphens or spaces.
   (The DIS adapter's `decode_errors` also counts datagrams of another exercise and Fire PDUs of a munition
   in no `weapons` entry; on a shared network these are normal traffic, not a fault.) A message the host refuses faults the capability, and the `poll`
   returns the host's code whatever the adapter returns. The `Inbox` cannot outlive the call or
   leave its thread (the sink token is valid only during the call, REQ-P-11).
4. **Threads and blocking** (ADR 0033): `publish` and `poll` never block on I/O; move data to or from
   bounded queues and count what you drop in `counters()`. Your I/O threads never call the host API
   (`umbra_sdk::log` from them is refused by the host and does nothing) and must have ended when `stop`
   (or the drop) returns, so the library can unload.
5. **Determinism** (REQ-K-4): use the `SimTime` of the call (`tick()`, `rate_hz()`, `seconds()`), never a
   wall clock; what arrives from a real network is outside the determinism contract, but the adapter
   must not add randomness of its own.
6. **Export** with `network_adapters: [..]` after the optional `behaviors: [..]` in `export_plugin!`.
   The descriptor then declares ABI 0.1, so the package's `plugin.toml` says `abi = "0.1"`; a
   behavior-only plugin built with the same SDK keeps declaring 0.0 and still loads in an ABI 0.0 host
   (the required-minor rule, ADR 0033, ASM-245).
7. **Test** the adapter against the engine's contract suite (`umbra_core::network_adapter_contract_tests!`,
   with `feed = ..` when it receives) through the real host: a plugin crate may depend on `umbra-sdk` only,
   so the run lives in a crate that loads the built library, as `crates/umbra-engine/tests/dis_plugin.rs`
   does for DIS. Add golden packets, malformed-input property tests and a two-instance exchange
   ([`docs/net/README.md`](net/README.md), "Writing an adapter").

What the SDK enforces for a network adapter (ASM-411..ASM-413): every host-written batch and message is
validated before `publish` runs (only the fields the message's kind uses; malformed input is
`UMBRA_ERR_INVALID_ARGUMENT`); `publish` and `poll` before `start` or after `stop`, and a second `start`,
are `UMBRA_ERR_STATE` without a trait call; every entry is panic-guarded (a panic in `key_space`,
`counters`, `stop` or `destroy`, which have no error sink, is logged at error level); messages it writes
carry minor 1 with unused fields zero; the network capability's metadata is empty (ABI 0.1 defines none).
How the host and engine treat faults (`abort` ends the run after the tick, `disable_capability` disables
that adapter while the others go on) is in [faults](#faults) and the [list below](#not-in-v0).

## Motion models, sensors, conditions and actions in Rust (ABI 0.2)
ABI 0.2 (ADR 0050) adds four kinds that plug into the same places as the built-ins: a **motion model**
(kind 2) moves entities like `builtin.ground.kinematic`, a **sensor** (kind 4) produces contacts like
`builtin.visual`, and a **condition** (kind 6) and an **action** (kind 7) are behavior-graph leaves like
`builtin.cond.*` and `builtin.action.*`. A plugin that exports any of them declares ABI 0.2, so its
`plugin.toml` says `abi = "0.2"`; behaviors and network adapters in the same plugin keep their own minors
(0 and 1), and a plugin without an ABI 0.2 kind keeps declaring 0.0 or 0.1 (ADR 0033). The SDK side is
complete. The host registers motion models and sensors like built-ins (M7 WI-8); the bridges of
conditions and actions land in WI-9, and until then the host validates a plugin that exports one
completely and then rejects it as unsupported, naming `capabilities[<i>].kind` ([not in v0](#not-in-v0)). The fixture
`crates/umbra-test-plugin/src/kinds.rs` (feature `kinds`) has a small example of every kind; the example
plugins `hover`, `radar`, `bearing` and `dispatch` follow (M7 WI-17, WI-18).

Shared rules of the four kinds:
- **Traits.** Each has `ID`, `SCHEMA` (the params' JSON Schema, ADR 0009 subset, whose top-level
  `description` is the catalog text), an optional `VERSION` (default: the plugin version) and
  `configure(params: &str)`, which receives the params as JSON text (parse it with `umbra_sdk::serde_json`
  and refuse bad values with `Error::params`; the host reports a build error naming the capability).
  Conditions and actions also have `METADATA` (below).
- **One call per item, not batched** (ASM-612): one `step` per entity and tick, one `sense` per observer,
  sensor and tick, one `check` or `tick` per node tick.
- **Services come with the call** (ADR 0050, H2): the terrain in a motion step or a sense call
  (`ctx.terrain()`), the blackboard in a condition check or an action tick (`bb`). They borrow the call,
  so they cannot be kept or sent to another thread, and the host resolves their tokens against the call
  (a stale or foreign token faults the capability, REQ-P-11). A raw-ABI plugin that calls a service from
  another thread, or after its call returned, gets `UMBRA_ERR_STATE` and no answer (ADR 0019 RR-M2;
  no call is faulted then, since none can be blamed). Terrain answers come from the scenario's terrain
  provider of that very call, converted from ECEF as a built-in converts, so a plugin sees exactly what a
  built-in sees (REQ-C-6, ASM-722).
- **Refusals end the call.** A refused terrain query, blackboard access, RNG draw, contact or intent
  returns an `Err` naming what was refused, and the whole call then returns the host's code whether or
  not the plugin looks at the error: the host has faulted the capability, so a result built afterwards
  (a motion state, a condition answer, an action status) is never reported. Propagate with `?`.
- **Faults** (ADR 0050; the host side of motion models and sensors since WI-8, of conditions and actions
  from WI-9): a panic or an `Err` faults the capability; motion keeps the entity's position and
  orientation with zero velocity, a sensor gives no contacts that tick, a condition answers false, an action
  reports Failure with that call's intents discarded. `abort` ends the run after the tick
  ([faults](#faults)).
- **Determinism**: time is `ctx.tick()` and `ctx.rate_hz()` (a motion step has no time: its `tick()` is
  0 and its `rate_hz()` is `1 / dt` rounded, ASM-720); randomness only from `ctx.rng()` (sensors:
  the observer's stream; leaves: the entity's stream); trigonometry only through
  [`umbra_sdk::math`](#determinism).

**Motion model** (`MotionModel`, list under `motion_models:`).
`step(&self, ctx, state, intents, dt) -> KinematicState` advances one entity by `dt` seconds. `state` is
the pre-step ECEF position, velocity and orientation quaternion `[w, x, y, z]` (body FRD to ECEF);
`intents` are the entity's intents of this tick in emission order. Like the core trait it is stateless
across ticks (`&self`): equal inputs must give bit-identical outputs. The host refuses a result that is
not finite or whose orientation is not a unit quaternion within 1e-9 (the SDK passes the result on
unchanged). Terrain: `ctx.terrain().height_at(p)` (metres above the ellipsoid under `p`; the point's own
height is ignored, and a provider may answer a non-finite height), `normal_at(p)` (`[east, north, up]`)
and `line_of_sight(a, b)`. Motion models have no metadata and no randomness.

**Sensor** (`Sensor`, list under `sensors:`).
`sense(&self, ctx, observer, candidates, out)` runs for one observer against every other entity in id
order, as they were before the tick (`TargetView`: id, kinematic state, force, `Ownership::Owned` or
`Remote`). Push a `Contact` per perceived candidate with `out.push(&contact)`; the contact must name a
candidate id (the host refuses others), have a finite position, velocity and range (range at least 0)
and an empty or valid force name, at most `UMBRA_MAX_CONTACTS_PER_SENSE` (4096) per call; one refused
contact discards all contacts of that call (ASM-719). The kernel sorts and deduplicates contacts as it
does for built-ins. Draw from `ctx.rng()` in a fixed order (for example one draw per candidate that passed your
gates, in candidate order). Sensors have no metadata.

**Condition** (`Condition`, list under `conditions:`).
`check(&self, ctx, entity, bb) -> Result<bool, Error>` answers for the entity's pre-tick view
(`EntityView`, as for behaviors). `METADATA` declares the blackboard keys the leaf may touch:
`{"blackboard_params": [{"param": "key", "type": "bool", "access": "read"}], "result": "text"}`
(ASM-643, ASM-709); empty text means none. Each entry names a param of `SCHEMA` (a string property)
whose value is the key's name, its type (`bool`, `number`, `integer`, `string`, `entity`, `position`)
and its access (`read` or `write`). The host grants exactly those keys with that type and access;
read your key's name from the params in `configure` and use it with `bb.get(&key)` (`Ok(None)` when
unset) and `bb.set(&key, Some(value))` (`None` unsets). Any other key, a wrong type, a non-finite
number or a write to a `read` key is refused. The metadata also becomes the catalog entry
(`Provenance::Plugin`).

**Action** (`Action` and `ActionInstance`, list under `actions:`).
`Action::instantiate(rate_hz)` makes one `ActionInstance` per activation; convert periods to ticks here
([period-to-ticks rule](#determinism)). `tick(&mut self, ctx, entity, bb, out) -> Result<Status, Error>`
runs once per node tick: `Running` until done, then `Success` or `Failure`; push intents with
`out.push(..)` (events leave as `Intent::Custom`, ADR 0028). `halt(&mut self)` is a full reset, idempotent
and safe before the first tick. `METADATA` and the blackboard are as for conditions. Instances of one
configured action must not share state (REQ-B-11).

**Export** with the lists after the older two, each optional, in this order:
`export_plugin! { id, name, version, behaviors: [..], network_adapters: [..], motion_models: [..],
sensors: [..], conditions: [..], actions: [..], init, shutdown }`.

What the SDK enforces for these kinds: every call record (state, intents, targets, entity view, the
services and sinks) is validated before the trait runs, and the structs new in ABI 0.2 must declare
minor 2 or newer (malformed input is `UMBRA_ERR_INVALID_ARGUMENT`); handles are tagged per capability
(an action passed as an instance is refused); every entry is panic-guarded (a panic in an action's `halt`,
which has no error sink, is logged at error level); contacts, intents, blackboard values and the value
sink of a blackboard read carry minor 2; a blackboard read accepts exactly one report from the host and
refuses a late, second or unknown-token one (each read has its own entry, so reads on several threads
at once stay apart); results are written only when the call succeeded; an action's `instantiate` refuses
a tick rate of 0.

## Writing a plugin in C
Include `umbra_plugin_api.h` only; compile as C11 with strict warnings, as
[`examples/plugins/c-circle`](../examples/plugins/c-circle/README.md) does
(`-std=c11 -pedantic -Wall -Wextra -Werror -ffp-contract=off -fvisibility=hidden -shared -fPIC`; MSVC
`/std:c11 /W4 /WX /fp:precise /LD`). Its README walks through `plugin.c` section by section; the rules
that matter most:

- **`UMBRA_EXPORT`** on `umbra_plugin_entry` and nothing else; make every other function and object
  `static`. The header declares the entry, so the compiler checks your definition against its type. The
  entry may be called more than once and must return the same pointer to a static descriptor; it should
  not use the host API (callbacks before `init` return `UMBRA_ERR_STATE`).
- **Strings** are `UmbraStr` (pointer plus length, UTF-8, not NUL-terminated). Use
  `UMBRA_STR_INIT("literal")` in static initializers (descriptor, capabilities) and `UMBRA_STR("literal")`
  in expressions (error messages); both avoid the `sizeof` off-by-one and sign warnings. Strings the host
  passes you are not NUL-terminated: copy before calling `strtod` and friends.
- **Writing structs** for the host: set `abi_version = UMBRA_ABI_VERSION`, `struct_size =
  sizeof(type)`, zero everything else first (`= {0}` or designated initializers), so reserved and unused
  fields are 0. Vtable entries you do not name stay null.
- **Reading structs** the host wrote: check the major and the size before reading any other field.
  c-circle has two macros, and the difference matters:
  - `HOST_STRUCT_OK(ptr, type)` for a single struct (batch, sinks, host API): non-null, major equals
    `UMBRA_ABI_VERSION_MAJOR`, `struct_size >= sizeof(type)`. Never compare the minor.
  - `ELEMENT_OK(ptr, type)` for an **element of a host-written array** (`items`): `struct_size ==
    sizeof(type)` exactly, because you index the array with your own `sizeof`; the host writes array
    elements in your minor, so any other size means your stride is wrong (ASM-128, ASM-148 b2).
- **Statuses**: write one `UMBRA_STATUS_*` per item into `batch->statuses[i]`; the host pre-filled
  `UMBRA_STATUS_UNSET` and treats an unset status as a fault.
- **Metadata** is JSON text you write by hand (`{"param_types":{..},"outputs":{..},"result":".."}`, every
  key optional, unknown keys rejected); `set_input` must be non-null iff `param_types` is non-empty,
  `output` iff `outputs` is (ADR 0020, ASM-135).
- **Memory**: `malloc` your handles in `configure`/`instantiate` and free them in the matching
  `destroy_*`; never free host memory, and the host never frees yours.

## ABI rules
ABI 0.0 is defined in Rust (`crates/umbra-plugin-api`); the header is generated from it (gate 11). The
layouts are final since the C example passed (WI-14) and are frozen at the M4 close-out; from then on
REQ-P-2 applies in full (`docs/plugin-abi-changelog.md`).

- **Versions.** `UMBRA_ABI_VERSION = major << 16 | minor`. Major 0 is a real major: a different major
  is incompatible. A plugin whose required minor is newer than the host's is rejected (REQ-P-9).
  Every appended field, vtable entry or kind bumps the minor and is optional, with a documented meaning
  when absent. A descriptor declares the lowest minor that defines every capability kind it exports
  (behavior 0, network adapter 1, motion model, sensor, condition and action 2; ADR 0033, ADR 0050,
  amending ADR 0019's "the SDK writes its own minor"). A
  kind's minor is the newest minor that changed a struct on its call surface; a capability, its vtable
  and every struct written during its calls (intents, values) carry that minor, never one above the
  descriptor's (the host refuses them otherwise). So a behavior-only plugin built with a newer SDK
  still runs in an older host. A capability of a kind newer than the descriptor's minor is rejected
  naming `capabilities[<i>].kind`. The current ABI is 0.2 (`docs/plugin-abi-changelog.md`).
- **Header and padding.** Every struct that can grow starts with `uint32_t abi_version; uint32_t
  struct_size;`. No struct has implicit padding, internal or trailing: every gap is a named
  `reserved_N` field that writers zero and readers ignore, and every headed size is a multiple of 8.
  The value primitives `UmbraStr` (16 bytes), `UmbraVec3` (24) and `UmbraQuat` (32) have no header and
  never grow.
- **Readers check only the major and `struct_size`**: `struct_size` at least the struct's minor-0 size,
  a multiple of 8, and at most the reader's own size for that minor; the major in that struct's own
  `abi_version` equal to the reader's. Never compare `abi_version` for equality; read only fields inside
  `struct_size` and treat missing newer fields as absent.
- **The host writes every struct it passes to a plugin in the layout of the plugin's declared minor**,
  so a plugin may index host-written arrays with its own `sizeof` (exact stride, see `ELEMENT_OK`).
  Exception: `UmbraHostApi` is passed to the entry before the host knows your minor, so it is written in
  the host's own (newest) layout (ASM-134 a); read only the prefix your minor knows and never compare
  its `struct_size` for equality (callbacks appended in later minors, e.g. M7, are simply ignored). Arrays a plugin writes for the host (the capabilities) use the first
  element's `struct_size` as the stride, and all elements must have that size.
- **Values.** Enums are `uint32_t` with `UMBRA_*` constants (0 and unknown values are errors, never
  cast). Booleans are `uint8_t` 0 or 1. Entity IDs are `uint64_t`, 0 = none. Numbers crossing as
  `UmbraValue` must be finite; strings valid UTF-8. Function pointers are nullable; required entries
  are checked at load.
- **Out-values** travel in separate plain arrays (the `statuses` array) whose enums have `0 = UNSET`,
  never inside a struct the host also reads.
- **Results** are `UmbraCode`: `UMBRA_OK` 0, `ERR_PANIC` 1, `ERR_INVALID_ARGUMENT` 2, `ERR_PARAMS` 3,
  `ERR_UNSUPPORTED` 4, `ERR_INTERNAL` 5, `ERR_STATE` 6 (outside a lifecycle or call scope). Messages go
  through the call's `UmbraErrorSink`; the first one of a call wins.
- **64-bit only** (x86_64, aarch64): the header asserts `sizeof(void*) == 8` (ASM-120).

## Tokens and lifetimes
- **Tokens, not pointers.** Every host context you hold is an opaque `uint64_t` (`UmbraToken`): the
  host API's `host_ctx` (one per image lifecycle), the sink contexts (one per call) and the RNG token
  of each batch item (one per item per call). Pass them back unchanged; the host checks them against
  the calling thread's active call and never dereferences them (ADR 0019).
- **Use a token only in the call that gave it to you.** A token from a finished call, from another
  thread, or after `shutdown` is refused (`UMBRA_ERR_STATE`) and, inside a call, faults the capability.
- **Own token only.** Draw for item `i` with `items[i].rng`. Drawing with another item's token of the
  same call cannot be detected; it draws that entity's stream and breaks the per-entity independence
  results rely on (ADR 0021). The SDK always uses the right token.
- **Data pointers** the host passes (batch, items, views, routes, contacts, params text, values,
  sinks) are valid only during that call; copy what you need to keep.
- **Your data** goes to the host only through sinks that copy during the call (intents, output values,
  error messages). The descriptor, capabilities, vtables and their strings must stay valid and
  unchanged until `shutdown` returns (statics); the host copies them at load.
- **Handles** (`UmbraBehavior*`, `UmbraInstance*`) are yours; the host only stores and passes them back
  and calls `destroy_*` for each, instances before the behavior they came from, also after a fault.
- **The `UmbraHostApi*`** points at a process-lifetime struct, but calls through it work only between
  `init` and the end of `shutdown`; a reloaded image gets a new one with a new `host_ctx`.

## Threads and re-entry
- The host never calls one image concurrently: calls are serialised by a blocking per-image lock, so
  other threads (two engines, parallel tests) wait and never change results. Calls may come from
  different threads over time.
- Never call the host from your own threads: a callback without an active call on that thread returns
  `UMBRA_ERR_STATE`.
- Host callbacks are leaf calls: they never call into any plugin. Calling back into the host is fine;
  re-entering the same image from the same thread is a fault, never a deadlock.
- Do not rely on thread-local state ([state and reload](#state-reload-and-thread-locals)).

## Panics, C++ exceptions and `longjmp`
- **Rust**: the SDK runs every exported function under `catch_unwind`; a panic becomes
  `UMBRA_ERR_PANIC` with its message (to the error sink, or the host log at error level for calls
  without one) and faults the capability. A panic while the entry builds the descriptor makes the entry
  return null (a load error). Panics in `destroy_*` and `shutdown` are logged and swallowed.
  Build with `panic = "unwind"` (the default): with `panic = "abort"` any panic kills the process. A
  panic in `Drop` during unwinding aborts too.
- **C++**: an exception escaping into the host is undefined behavior. Make every function you expose
  `noexcept` and catch everything inside. (The header compiles as C; a C++ check is planned for M9.)
- **`longjmp`** across host frames, in either direction, is forbidden.
- A crash, `abort` or `exit` in plugin code ends the process ([trust model](#trust-model-and-in-process-risk)).

## Faults
| What | When | Result |
|---|---|---|
| Load error | bad manifest or descriptor, entry null or unstable, `init` fails or panics | the load fails (exit 1); nothing of the plugin is registered |
| Build error | `configure`/`instantiate` returns an error, panics, or returns OK with a null handle | not a fault (R-M9): a `configure` error at `validate` is a diagnostic at the value (`param_invalid`); at spawn the engine build fails (`EngineError::BehaviorBuild` for `configure`) |
| **Fault** | during a run: a non-OK code from `tick_batch`, `halt`, `halt_running`, `set_input` or `output`; a panic; a status left `UNSET` or out of range; a refused intent (bad kind or value, item index `>= count`, more than 64 for one item); an invalid output value or a second output; token or callback misuse (faults even if you then return `UMBRA_OK`) | the capability is faulted, see below |

On a fault (ADR 0020, ADR 0021, ASM-140):
- The host logs it once at error level (plugin, capability, call, message) and marks the capability
  faulted **for the engine that made the call**. Another engine in the same process, even one sharing
  the image, is unaffected; a run never depends on earlier runs.
- **Whole-batch discard**: the faulting `tick_batch` call is discarded as a whole: every item returns
  Failure and none of its intents are kept. RNG draws made before the fault **stay consumed**: the
  entity streams advanced and that is part of the state hash.
- From then on that engine skips the capability: items return Failure without a call, `configure` and
  `instantiate` fail, `halt`/`halt_running` do nothing, `set_input` is not called, `output` is unset.
  `destroy_*` is still called. In a graph, a faulted `subgraph` child returns Failure to its caller.
- **`on_plugin_fault = "abort"`** (default): the engine finishes the faulting tick (a well-defined
  state), then `Engine::run`/`step` return `EngineError::PluginFault`; `umbra run` prints the fault
  and exits 1. **`"disable_capability"`**: the run continues without the capability (its entities get
  Failure and no intents, so they slow to a stop) and exits 0. At the faulting tick both policies have
  the same state hash.
- **Network adapters** (ABI 0.1, ADR 0033): a non-OK `publish`, `poll`, `counters` or `stop`, a panic,
  or a message the host refuses faults the capability; the messages of a faulted `poll` are discarded
  whole and afterwards only `stop` and `destroy` are called. Under `abort` the run ends after the tick
  with `EngineError::PluginFault` naming the plugin, the capability and the call; under
  `disable_capability` the manager disables that adapter (its ghosts time out). The fault belongs to the
  capability in that host, so other adapters configured from the same capability (one capability under
  several scenario names) fault at their next call and are disabled too; adapters of other capabilities
  and images, and built-in ones, keep running (ASM-408 c, ASM-410). A non-OK
  `configure`, `key_space` or `start` is a build or start error, not a fault.

## Determinism
Runs must be reproducible for the same scenario, seed and plugin versions (A-2, REQ-K-4). The host
makes its side deterministic (load order, call order, groups ordered by first entity ID, per-engine
fault state, whole-batch discard); your plugin must do the same. Nothing below is enforceable
in-process; gate 4 checks the example plugin, not yours.

- **Randomness only from the host**: `rng_next_u64`/`rng_next_f64` (SDK: `ctx.rng()`) with the
  item's own token. No private RNG, no seeding from time, addresses or the OS.
- **No wall clock**: time is `tick` and `rate_hz` of the call (simulation time = `tick / rate_hz`).
- **No dependence on call order or grouping**: results may depend only on per-handle state
  ([state and reload](#state-reload-and-thread-locals)).
- **Periods in seconds to ticks**: copy the core's rule (`TickRate::ticks_for`, ASM-066, ASM-078):
  `ceil(seconds * hz)`, at least 1, except that a product within two ULPs of a whole number counts as
  that number (0.1 s at 30 Hz is 3 ticks, not 4). The wander example's `ticks_for` is a copy you can
  take as is (Rust) or port step by step (C). Convert once, in `instantiate`. The SDK has no helper in
  M4; one (for example `umbra_sdk::ticks_for`) is to be considered at M7.
- **Floating point**: determinism holds on the same OS and build, not across platforms (ADR 0003).
  Compile C with `-ffp-contract=off` (GCC/Clang) or `/fp:precise` (MSVC) so no compiler fuses a
  multiply and an add into an FMA; keep `-ffast-math` off. Prefer exact IEEE operations; transcendental
  functions from the platform `libm` may differ between platforms.
- **`umbra_sdk::math`** (ADR 0059): in Rust, call `sin`, `cos`, `sin_cos`, `tan`, `asin`, `acos`, `atan`,
  `atan2`, `hypot`, `cbrt`, `powi`, `powf`, `exp`, `ln` and `wrap_pi` (plain `f64` radians, result in
  `(-pi, pi]`) from this module, never the `f64` methods (the workspace `clippy.toml` bans those). It
  mirrors the engine's `umbra_core::math` bit for bit (a parity test in the host keeps them identical), so
  a plugin computes exactly what a built-in would on the same OS. `+ - * /`, `sqrt`, `abs`, `floor` are exact and fine
  anywhere.
- **Ordered containers** in Rust (`BTreeMap`, `Vec`), never iteration over `HashMap`/`HashSet`.

## State, reload and thread-locals
- **Per-handle state contract** (ADR 0021, RR-m3): a plugin's results may depend only on per-handle
  state (its configured behavior and instance handles, the call's arguments and its RNG draws), never on
  global state shared across handles, images, hosts or engines. Two engines in one process share one
  image; items of one call may come from different configured behaviors; grouping and call order are
  an engine detail. Globals that never influence results (the host API pointer c-circle keeps) are fine.
- **`init` must reset global state** (RR-M1): after the last handle is released and `shutdown` has run,
  the same process may load the image again: the entry is called twice and `init` runs again, with a new
  `UmbraHostApi*` and `host_ctx`. With `keep_loaded` (or when the OS keeps the library mapped) your
  statics still hold their old values, so `init` must set them, not assume zero. The SDK resets its own
  state; use the `init:` hook of `export_plugin!` for yours.
- **No thread-locals with destructors** (ASM-142): calls arrive on varying threads, and glibc never
  unmaps a library that registered a thread-local destructor (closing it is a crash risk on macOS). In
  Rust avoid `thread_local!` values that need `Drop`, use a `static` with `Mutex`/`OnceLock` as the
  SDK does; in C avoid `_Thread_local` state altogether (ADR 0019).

## Params, schemas and bindable params
- **Schema subset.** `param_schema` must use the ADR 0009 keyword subset (`docs/templates.md`,
  "Component parameter schemas"), with the ASM-068 limits. The host only checks at load that it is a
  JSON object; a schema outside the subset makes every scenario load with the plugin fail with
  `LoaderError::Schema` naming the capability (ASM-114).
- **Validation.** Template and call-site params are checked against your schema and then dry-run
  through `configure`, so `umbra validate` catches what spawning would, located at the value (REQ-T-3).
- **Cross-param rules (the ASM-106 d narrowing).** A value passed down a subgraph chain is followed
  and checked on its own; a rule that combines two values passed through from different calls (a
  `oneOf` across params, or a `configure` that rejects a combination such as `lo > hi`) is checked at
  load only when both values come from the same call, otherwise only by your `configure` at spawn time,
  as `EngineError::BehaviorBuild` without a file location. Prefer per-param bounds in the schema; keep
  the cross-param check in `configure` as the backstop.
- **Bindable params** are declared in the metadata's `param_types` (SDK: `Interface::param`). A
  bindable param that is also a schema property can be given literally or bound with `{ key = .. }`
  in a `subgraph` call; its JSON `type` must match. A `param_types` entry that is **not** a schema
  property is **binding-only**: it has no literal form and is always required. `entity` and `position`
  params are always binding-only (listing them in the schema is a load error). A behavior with a
  binding-only param can be called only as a `subgraph` with a binding; as a template behavior it is
  `missing_param` (ASM-107, ASM-142).
- In template `[behavior] params` the forms `{ key }` and `{ param }` are `invalid_binding`, as for any
  behavior.
- v0 metadata has no `subgraphs` key, so a plugin behavior counts as one level of subgraph depth
  (ASM-099 e).

## Plugin behaviors in graphs
- **Two uses.** As a template behavior (`[behavior] ref = "<capability id>"`) and as a `subgraph` target
  (`ref`, optional `version` range, `params` with literals or `{ key }` bindings, `outputs`). Graphs
  cannot tell plugin, built-in and file behaviors apart (REQ-B-9). See
  `examples/scenarios/plugin_wander/behaviors/wander_graph.behavior.toml` and `docs/behavior.md`.
- **`halt` vs `halt_running`.** `halt` is the **instance halt**: a full reset to the state after
  `instantiate`; it must be idempotent and safe when not Running (ASM-096 c). `halt_running` is the
  **parent halt** a calling graph sends to a Running child (a `timeout` expiring, a reactive composite
  pre-empting, an FSM guard, an unset required input). In C, a null `halt_running` makes the host call
  `halt` (c-circle); in Rust the trait default is `halt()`. Override it only if some state should
  survive a parent halt, as graph instances keep a cooling `cooldown` (ADR 0013 amendment).
- **Bound inputs and outputs.** Before every tick of a `subgraph` node the host calls `set_input` for
  each bound param (value or unset), after it `output` for each bound port. The host checks the declared
  type and finiteness before calling; a non-OK `set_input` or an output of the wrong type is a fault.
- **Batching only for top-level use.** Entities whose template behavior is your capability are ticked
  together: one `tick_batch` call per capability per tick, items in entity-ID order (REQ-B-4). A plugin
  behavior used as a `subgraph` child is ticked inside its caller's graph as a batch of one. Write
  `tick_batch` so that each item gets exactly what a single tick would.

## Caps
| Limit | Value | Over the limit |
|---|---|---|
| Capabilities per plugin | 256 (`UMBRA_MAX_CAPABILITIES`) | load error |
| Plugin and capability ID | 128 bytes (`UMBRA_MAX_ID_LEN`) | load error |
| Param schema | 1 MiB (`UMBRA_MAX_SCHEMA_LEN`) | load error |
| Metadata | 64 KiB (`UMBRA_MAX_METADATA_LEN`) | load error |
| Plugin name, version text | 4096 bytes each (host-internal) | load error |
| Intents per batch item per call | 64 (`UMBRA_MAX_INTENTS_PER_ITEM`) | the 65th is refused, fault |
| Error message | 4 KiB (`UMBRA_MAX_ERROR_LEN`) | truncated at a character boundary |

## Intents, events and the API (M5, ADR 0028)
A plugin behavior's `Fire` and `Custom` intents become canonical events: in each tick's Events phase, for
every Owned entity in `EntityId` order, its `Fire` and `Custom` intents (in emission order) become
`SimEvent::Fire` and `SimEvent::Custom` (the `source` is the emitting entity and `kind` the component id of
the `Custom` intent). They reach the stream as `fire` and `custom` events (`docs/api.md`, "The stream"),
best-effort: a slow client can lose some, and `dropped` and `events_truncated` say so. Events change no
state and are not part of the state hash, so a plugin cannot influence a run through them.
`Spawn` and `Despawn` intents are applied by the engine right after the tick (M7, ADR 0052): the entity is
created from the template with the emitter's force and acts from the next tick; `Despawn` removes the
emitting entity (its instance is destroyed on the engine thread). Limits: 256 spawn intents per tick, 100,000
Owned entities; the excess and spawns of unknown templates are dropped, not faults. The plugin list of the loaded
scenario, with capabilities and a stopping fault, is `GET /api/v1/plugins`. Loading a scenario again (or
`POST /api/v1/scenario` over a running session) starts a new plugin lifecycle: `init` runs again.

## Not in v0
- The `component` kind (5) stays reserved (ASM-620); a capability of that kind is rejected as
  unsupported. The motion model (2), sensor (4), condition (6) and action (7) kinds are defined in ABI 0.2
  (ADR 0050): their structs are in the header and the host validates their descriptor entries (schema,
  metadata, vtable). Motion models and sensors are registered like built-ins (M7 WI-8); until the
  bridges of conditions and actions land (WI-9) a plugin that exports one is then rejected as
  unsupported, naming `capabilities[<i>].kind`. The network adapter
  kind (3) is defined in ABI 0.1 (ADR 0033): its structs are in the header, the host validates its
  descriptor entries and registers it as a network adapter through the host bridge (M6 WI-11: `configure`
  and `key_space` at build, `start` once, `publish`/`poll` on the tick path, `counters` read after each
  `poll` and before `stop`, `stop` and `destroy` exactly once; a non-OK `publish`, `poll`, `counters` or
  `stop`, an invalid pushed message or more than `max` pushes faults the capability, after which only
  `stop` and `destroy` are called; a non-OK `configure`, `key_space` or `start` is a build or start
  error, not a fault). Host-written strings respect `UMBRA_MAX_ID_LEN`: a longer display `name` arrives
  cut at a UTF-8 char boundary, a longer `entity_type` arrives empty (ASM-409). Several network adapters
  may run at once, from one plugin or several and one capability under several names; each has its own
  handle, key space and sink (ASM-410). The SDK traits are in
  [network adapters](#writing-a-network-adapter-in-rust-abi-01) (WI-12).
- Host API beyond `log` and RNG: none. Terrain and blackboard access are per-call services of the ABI 0.2
  kinds, and events leave as `Custom` intents (ADR 0050, H2; REQ-P-5 is staged); there is no memory
  helper and no runtime registration call (the descriptor is the only registration).
- Saving and restoring instance state (replay, M7); hot reload; plugin-level configuration in scenarios;
  selecting individual plugins inside a plugin directory; plugin signing. (The REST plugin listing, `GET /api/v1/plugins`, exists since M5.)
- `umbra new-plugin` scaffolding (M7); the C# NativeAOT sample and a C++ check of the header (M9).
- Sandboxed or out-of-process plugins (not in v1, spec 1.3).
- 32-bit targets.

## Platforms
- **64-bit only** (x86_64, aarch64); the ABI layout is the same on Linux, Windows and macOS.
- **Linux** is the verified platform: every gate runs there.
- **Windows x64 and macOS are untested in M4** (human decision 2026-10-19; NFR-3 unverified): the MSVC
  compile and `/LD` packaging of C plugins, `.dll` loading with the flags above, Windows abort exit
  codes and macOS linking and unloading were written and reviewed by reading only.
- **Windows long paths**: the host loads by absolute path and rewrites a verbatim `\\?\` path to its
  plain form, so a library path longer than `MAX_PATH` (260 characters) loads only where Windows
  long-path support is enabled. Keep plugin directories short.
- **Windows dependencies** of a plugin DLL are searched in the plugin's own directory and the default
  search directories only (`LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`: the
  application directory, `System32`, directories added with `AddDllDirectory`; never `PATH`); ship them
  in the package. On Linux every symbol must
  resolve at load (`RTLD_NOW`), and plugin symbols are not visible to other plugins (`RTLD_LOCAL`).
- **Unloading**: if closing a library crashes on a platform (thread-local destructors, macOS risk),
  embedders can set `HostOptions::keep_loaded` (ADR 0021); `init` must then reset state, as above.
