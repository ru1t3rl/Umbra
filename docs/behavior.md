<!--
Doc-test convention (WI-17): every fenced block whose info string is exactly `toml file=<relative path>`
is a complete file. The doc test (crates/cgf-cli/tests/cli_behavior.rs) writes all of them, at their
relative paths, into one fresh directory and runs `cgf-cli validate <dir>`, which must print
`ok: 8 files checked`. They form one example set (a scenario, two templates, five graphs), so IDs must
stay unique across all `file=` blocks, and adding or removing one changes the expected count. Blocks
that are deliberately partial or invalid use plain `toml` and are not extracted.
-->

# Behavior graph reference (format v1)

Source of truth: spec section 6 and amendment A-001 (D-26, D-27), ADRs 0011-0017 (D-28..D-34) and
ASM-070..ASM-110 in `docs/assumptions.md`. The readers in `crates/cgf-templates` and the interpreter in
`crates/cgf-behavior` are authoritative where this page and the JSON Schema
`crates/cgf-templates/schemas/behavior.v1.schema.json` (printed by `cgf-cli schema behavior`) differ.
Templates and scenarios are described in `docs/templates.md`.

A **behavior graph** is a behavior tree whose nodes may also be finite state machines (`fsm`) and calls
of other graphs (`subgraph`). Each Owned entity with a behavior gets its own instance of the graph; every
tick the instance is evaluated once and emits **intents** (move here, hold, fire, ...), which the motion
model turns into movement. A behavior never changes entity state directly (REQ-B-1).

Contents: [files and discovery](#files-and-discovery), [complete example](#complete-example),
[file format](#file-format), [evaluation](#evaluation-order-and-node-semantics), [halt](#halt),
[FSM rules](#fsm-rules), [interface and blackboard](#interface-and-blackboard),
[subgraphs](#subgraphs), [load-time checks](#load-time-checks), [built-in catalog](#built-in-catalog),
[perception](#perception), [intents](#intents-and-what-they-do-not-do-yet),
[diagnostics](#diagnostics), [determinism](#determinism), [embedding](#embedding-stack-size).

## Files and discovery
- One graph per file. A file is a graph exactly when its name ends in `.behavior.toml`
  (case-sensitive), wherever templates are searched: under a scenario's `template_paths`, in a directory
  given to `cgf-cli validate`, or given to `validate` directly (ADR 0012, ASM-074). A single graph file
  is validated together with its direct siblings, so a call to a sibling resolves.
- A graph's `id` is a component ID (ADR 0002) and is a valid template `[behavior] ref` and `subgraph`
  target. IDs are global: a graph may not reuse the ID of any registered component (`duplicate_id`),
  including the built-in graphs.
- Built-in graphs are the same kind of file, embedded in `cgf-engine` and registered with provenance
  `builtin`; graph files of a scenario have provenance `file`; plugin behaviors (M4) `plugin`. A
  `subgraph` node cannot tell them apart (REQ-B-9).

> **Migration note (M3).** From M3 a file whose name ends in `.behavior.toml` is a graph, whatever it
> contains. An M2 template or scenario saved under such a name is no longer read as a template or a
> scenario; rename it (for example `tank.toml`). `load_scenario` on such a file reports `not_a_scenario`
> ("is a behavior graph"). Every other file name is classified exactly as in M2.

## Complete example
This set validates with `cgf-cli validate <dir>` and runs with `cgf-cli run <dir>/scenario.toml`. A blue
tank patrols its route; when a hostile comes within 1200 m it approaches it and fires every 4 s with a
60 percent chance; a red truck drives its route. It uses every node type.

```toml file=scenario.toml
schema_version = 1

[scenario]
name = "behavior_doc_example"
seed = 1
tick_rate_hz = 30
duration_s = 60.0
template_paths = ["templates", "behaviors"]

[[entities]]
template = "doc.tank.sentry"
name = "blue-1"
force = "blue"
position = { lat_deg = 52.0, lon_deg = 4.0 }
heading_deg = 90.0
[entities.route]
arrival_radius_m = 5.0
cruise_speed_mps = 8.0
waypoints = [ { lat_deg = 52.0, lon_deg = 4.01 } ]

[[entities]]
template = "doc.truck.target"
name = "red-1"
force = "red"
position = { lat_deg = 52.005, lon_deg = 4.01 }
heading_deg = 270.0
[entities.route]
arrival_radius_m = 5.0
cruise_speed_mps = 10.0
waypoints = [ { lat_deg = 52.005, lon_deg = 3.99 } ]
```

The tank template names the graph and gives it one parameter; the sensor is what gives the tank
contacts at all:

```toml file=templates/sentry.toml
schema_version = 1
id = "doc.tank.sentry"
description = "Tank that patrols its route and strikes hostiles"

[motion]
model = "builtin.ground.kinematic"
params = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }

[behavior]
ref = "doc.behavior.sentry"
params = { engage_range_m = 1200.0 }

[[sensors]]
kind = "builtin.visual"
params = { range_m = 1500.0, fov_deg = 360.0 }
```

```toml file=templates/target.toml
schema_version = 1
id = "doc.truck.target"

[motion]
model = "builtin.ground.kinematic"
params = { max_speed_mps = 15.0, max_turn_rate_dps = 30.0, max_accel_mps2 = 2.0 }

[behavior]
ref = "doc.behavior.drive"
```

The smallest useful graph: one action. A template `[behavior] ref` must name a behavior, not an action,
so "just follow the route" is a one-node graph (there is no built-in follow-route behavior, ASM-077):

```toml file=behaviors/drive.behavior.toml
schema_version = 1
id = "doc.behavior.drive"
version = "1.0.0"
description = "Follow the initial route and hold at its end"
root = "go"

[nodes.go]
type = "action"
ref = "builtin.action.follow_route"
```

The top-level graph. The reactive selector `main` re-checks `strike` every tick, so a hostile pre-empts
the patrol, and the patrol resumes when `scan` fails:

```toml file=behaviors/sentry.behavior.toml
# Patrol the route; strike the nearest hostile within `engage_range_m`.
schema_version = 1
id = "doc.behavior.sentry"
version = "1.2.0"
description = "Patrol the route; strike the nearest hostile within range"
root = "main"

[interface]
params = { engage_range_m = { type = "number", exclusiveMinimum = 0, default = 1000.0, description = "Distance at which a hostile is engaged, in meters" },
           fire_range_m   = { type = "number", exclusiveMinimum = 0, default = 600.0 } }
result = "Running while patrolling or striking; Success when the route is complete and no hostile is in range"

[blackboard]
target = "entity"

[nodes.main]
type = "selector"
children = ["strike", "patrol"]

[nodes.strike]
type = "sequence"
children = ["scan", "attack"]

[nodes.scan]
type = "subgraph"
ref = "doc.subgraph.scan"
version = "^1"
params = { range_m = { param = "engage_range_m" } }
outputs = { found = "target" }

[nodes.attack]
type = "subgraph"
ref = "doc.subgraph.strike"
version = "^1.0"
params = { target = { key = "target" }, fire_range_m = { param = "fire_range_m" } }

[nodes.patrol]
type = "action"
ref = "builtin.action.follow_route"

[editor]
positions = { main = [0, 0], strike = [-150, 120], patrol = [150, 120] }
```

A subgraph with an output port. It writes the nearest hostile into its own key `nearest`; the caller
copies the port `found` into its key `target` after every tick. `enemy_in_range` leaves the key
unchanged when it fails, so after a Failure `found` still holds the last hostile seen (the caller above
only uses it after a Success):

```toml file=behaviors/scan.behavior.toml
schema_version = 1
id = "doc.subgraph.scan"
version = "1.0.0"
description = "Report the nearest hostile contact within range"
root = "look"

[interface]
params = { range_m = { type = "number", exclusiveMinimum = 0, default = 1000.0 } }
outputs = { found = { type = "entity", from = "nearest" } }
result = "Success when a hostile is within range_m (port found holds it); Failure otherwise"

[blackboard]
nearest = "entity"

[nodes.look]
type = "condition"
ref = "builtin.cond.enemy_in_range"
params = { range_m = { param = "range_m" }, store_as = "nearest" }
```

A subgraph with an embedded FSM. `target` is a binding-only `entity` param: callers must bind it with
`{ key = .. }`, and inside the graph it is the read-only blackboard key `target`:

```toml file=behaviors/strike.behavior.toml
# Approach the bound target, then fire at it every 4 s (chance `aggression`) while it is in range.
schema_version = 1
id = "doc.subgraph.strike"
version = "1.0.0"
description = "Approach the bound target, then fire at it while it is within fire range"
root = "modes"

[interface]
params = { target             = { type = "entity", description = "The entity to strike; bind it with { key = .. }" },
           fire_range_m       = { type = "number", exclusiveMinimum = 0, default = 600.0 },
           approach_speed_mps = { type = "number", exclusiveMinimum = 0, default = 8.0 },
           aggression         = { type = "number", minimum = 0, maximum = 1, default = 0.6 } }
result = "Failure when the target is unset, lost, or not reached within 60 s; otherwise Running"

[nodes.modes]
type = "fsm"
initial = "approach"
states = { approach = "close_in", engage = "shoot" }
exits = { gave_up = "failure" }

[[nodes.modes.transitions]]
from = "approach"
to = "engage"
when = "in_range"

[[nodes.modes.transitions]]
from = "approach"
to = "gave_up"
on = "failure"

[[nodes.modes.transitions]]
from = "engage"
to = "approach"
when = "out_of_range"

[nodes.in_range]
type = "condition"
ref = "builtin.cond.target_in_range"
params = { target_key = "target", range_m = { param = "fire_range_m" } }

[nodes.out_of_range]
type = "inverter"
child = "in_range_again"

[nodes.in_range_again]
type = "condition"
ref = "builtin.cond.target_in_range"
params = { target_key = "target", range_m = { param = "fire_range_m" } }

[nodes.close_in]
type = "timeout"
duration_s = 60.0
child = "go"

[nodes.go]
type = "action"
ref = "builtin.action.move_to"
params = { target_key = "target", speed_mps = { param = "approach_speed_mps" }, arrival_radius_m = { param = "fire_range_m" } }

[nodes.shoot]
type = "selector"
children = ["volley", "hold_fire"]

[nodes.volley]
type = "cooldown"
duration_s = 4.0
child = "aim_and_fire"

[nodes.aim_and_fire]
type = "sequence"
children = ["roll", "fire"]

[nodes.roll]
type = "condition"
ref = "builtin.cond.chance"
params = { probability = { param = "aggression" } }

[nodes.fire]
type = "action"
ref = "builtin.action.fire"
params = { target_key = "target", weapon = "doc.weapon.cannon" }

[nodes.hold_fire]
type = "action"
ref = "builtin.action.wait"
params = { duration_s = 0.5 }
```

How the strike runs: in `approach`, the guard `in_range` is checked first each tick; while it fails,
`close_in` drives `go` toward the target. If the target is lost (`move_to` fails) or 60 s pass
(`timeout`), the state fails and `on = "failure"` takes the exit `gave_up`, so the FSM returns Failure.
In `engage`, `shoot` tries `volley` first: the cooldown lets `aim_and_fire` run at most once every 4 s
(one `chance` draw, then `Fire` on success), and `hold_fire` holds position in between. When the caller
pre-empts the strike (its `scan` fails), the cooling cooldown keeps its timer (see [Halt](#halt)).

A graph that shows the remaining node types (`repeat`, `parallel`, `succeed_always`, `timer_elapsed`).
It is validated with the set but not used by a template:

```toml file=behaviors/drill.behavior.toml
# From 2 s of run time on, repeat forever: hold for 3 s, then drive along the route for at most 90 s.
schema_version = 1
id = "doc.behavior.drill"
version = "0.1.0"
root = "forever"

[nodes.forever]
type = "repeat"
child = "drill"

[nodes.drill]
type = "sequence"
memory = true
children = ["warm_up", "leg"]

[nodes.warm_up]
type = "parallel"
policy = "all"
children = ["pause_3", "settle"]

[nodes.pause_3]
type = "repeat"
count = 3
child = "pause"

[nodes.pause]
type = "action"
ref = "builtin.action.wait"
params = { duration_s = 1.0 }

[nodes.settle]
type = "condition"
ref = "builtin.cond.timer_elapsed"
params = { duration_s = 2.0 }

[nodes.leg]
type = "succeed_always"
child = "limited"

[nodes.limited]
type = "timeout"
duration_s = 90.0
child = "drive"

[nodes.drive]
type = "action"
ref = "builtin.action.follow_route"
```

Before 2 s of run time `settle` fails, so the `parallel` (policy `all`) fails at once and halts
`pause_3`; the whole tree fails and starts again on the next tick. A larger example is
`examples/scenarios/patrol_and_engage/` (the M3 acceptance scenario).

## File format
TOML 1.1 with the M2 rules (`docs/templates.md`): datetimes and `nan`/`inf` are rejected everywhere, unknown
keys are errors everywhere except inside `[editor]`, units are key suffixes (`_m`, `_s`, `_mps`, `_deg`).
Names of nodes, states, exits, blackboard keys, interface params and output ports follow one grammar:
`[a-z][a-z0-9_]*`, at most 128 bytes (`invalid_value` otherwise).

### Top level
| Key | Type | Notes |
|---|---|---|
| `schema_version` | integer | required, `1` (same rules as templates, REQ-T-5: newer is `unsupported_schema_version` and the file is not checked further) |
| `id` | string | required, component ID (ADR 0002), unique among graphs and registered components |
| `version` | string | required, semantic version `MAJOR.MINOR.PATCH` (pre-release tags allowed), REQ-B-14; otherwise `invalid_version` |
| `description` | string | optional; shown in the catalog |
| `root` | string | required, the name of the root node |
| `[interface]` | table | optional: `params`, `outputs`, `result` (below); absent means no params and no outputs |
| `[blackboard]` | table | optional: `<key> = "<type>"`, the graph's own keys (below) |
| `[nodes.<name>]` | tables | required (at least the root): one table per node, with `type` and the settings of that type |
| `[editor]` | table | optional, any contents (layout for editors); checked to be a table with valid values, then dropped: it never reaches the engine, the state hash or replay (REQ-B-16) |

### `[interface]`
| Key | Type | Notes |
|---|---|---|
| `params` | table | `<name> = { type = .., <keywords>, default? }`, one fragment per param |
| `outputs` | table | `<port> = { type = "<bb type>", from = "<key>" }`: after every tick the caller can copy key `from` (a key of this graph, of the port's type) |
| `result` | string | free text: what Success, Failure and Running of this graph mean (REQ-B-10); shown by the catalog, names nothing |

A param fragment's `type` is one of `boolean`, `number`, `integer`, `string` (the JSON Schema names) or
`entity`, `position` (binding-only, see [Interface and blackboard](#interface-and-blackboard)); `bool`,
`object` and `array` are `invalid_value`. A literal-capable fragment may use the ADR 0009 keywords
(`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `enum`, `const`, `minLength`, `maxLength`,
`oneOf`, ..., plus the annotations `description`, `title`, `examples`); `default` must satisfy the
fragment. A binding-only fragment allows only `type` and `description`, and no `default`.

### `[blackboard]`
`<key> = "<type>"` with type `bool`, `number`, `integer`, `string`, `entity` or `position` (note `bool`
here and in output ports, `boolean` in interface fragments, ASM-102). A key may not repeat an interface
param name (`duplicate_name`): every param already is a key.

### Node settings
Every node table has `type` (required) and the settings of its type; any other key is `unknown_key`.

| `type` | Settings | Notes |
|---|---|---|
| `sequence` | `children` (array of node names, at least one), `memory` (bool, default `false`) | |
| `selector` | `children`, `memory` (default `false`) | |
| `parallel` | `children`, `policy` (`"all"` or `"any"`, default `"all"`) | |
| `inverter` | `child` (node name) | |
| `succeed_always` | `child` | |
| `repeat` | `child`, `count` (integer 1 .. 2^32-1, optional; absent = forever) | |
| `cooldown` | `child`, `duration_s` (number > 0, required) | |
| `timeout` | `child`, `duration_s` (number > 0, required) | |
| `condition` | `ref` (a registered condition ID), `params` (table, optional) | |
| `action` | `ref` (a registered action ID), `params` (table, optional) | |
| `subgraph` | `ref` (a behavior ID), `version` (semver range, optional), `params` (table, optional), `outputs` (table `<port> = "<parent key>"`, optional) | |
| `fsm` | `initial` (state name), `states` (table `<state> = "<node>"`), `exits` (table `<exit> = "success" \| "failure"`, optional), `[[nodes.<fsm>.transitions]]` (array, optional) | each transition: `from` (state), `to` (state or exit), and exactly one of `when = "<guard node>"` or `on = "success" \| "failure"` |

In `params`, two one-key tables are reserved forms, never literals (ADR 0012, ADR 0014, ASM-076):
- `{ param = "<name>" }`: replaced at build time by the value of this graph's interface param `<name>`
  (in `condition`, `action` and `subgraph` params; it stands for a whole value, also inside an array or
  table value);
- `{ key = "<key>" }`: a run-time input binding to a key of this graph's blackboard (only as a whole,
  top-level value of a `subgraph` node's `params`; anywhere else it is `invalid_binding`).

A table with more keys (`{ key = "x", other = 1 }`) or a non-string value (`{ key = 5 }`) is an ordinary
literal. Leaf params that name blackboard keys are plain strings (`target_key = "target"`,
`store_as = "target"`), never these forms. There is no expression language (REQ-B-15): a string such as
`"range < 500"` is just a string, and a number param given one is `param_wrong_type`.

### Graph shape
The nodes form a **tree** from `root` (ASM-075): every node except the root has exactly one parent (FSM
states and guards count as parents); a name no `[nodes.*]` table defines is `unknown_node`, a node
reached twice `node_reused`, a node that reaches its own ancestor `node_cycle`, a node not reachable from
`root` `unreachable_node`. Nodes nest at most 128 levels (`MAX_NODE_DEPTH`, the root is level 1;
`node_too_deep`). Composites need at least one child.

## Evaluation order and node semantics
Every node returns **Success**, **Failure** or **Running** (REQ-B-8). The order is fixed (ADR 0013,
ASM-070):
1. Entities: Owned entities in `EntityId` order (Remote entities are never ticked, REQ-K-5). Each
   behavior sees its entity's state, route progress and contacts as they were before the tick (the
   Perception phase runs first), so no entity sees another's decisions of the same tick.
2. Within an entity: one depth-first pass from the root every tick, children in array order. No node is
   ticked twice in one tick, and there are no unbounded loops (`repeat` ticks its child at most once per
   tick).
3. Intents are appended in evaluation order; the motion model resolves them (`Hold` wins; otherwise the
   last `MoveTo` or `SetHeading` and the last `SetSpeed` win; no intent means slow to a stop; ASM-030,
   ASM-094).

The root is ticked every tick whatever it returned last time; a finished tree starts again from the root
on the next tick. The last status is visible as `EntitySnapshot::status`.

**Composites** (reactive by default; `memory = true` to resume):

| Node | Semantics |
|---|---|
| `sequence` | Ticks children in order until one returns Failure or Running (returns that) or all succeeded (Success). |
| `selector` | Ticks children in order until one returns Success or Running (returns that) or all failed (Failure). |
| reactive (default) | Starts at the first child every tick, so an earlier child can pre-empt a later Running one. A child that was Running last tick and is not reached this tick (an earlier child decided or is Running) is **halted**. |
| `memory = true` | Resumes at the child that was Running; starts from the first child again after the composite finished or was halted. Use it for step-by-step chains ("roll, fire, reload"). |
| `parallel` | Ticks every unfinished child each tick, in order; finished results are kept until the parallel finishes or is halted. `all`: Failure as soon as one child fails (the Running ones are halted), Success when all succeeded. `any`: Success as soon as one succeeds (the Running ones are halted), Failure when all failed. |

**Decorators** (durations become whole ticks once per instance: `n = ceil(duration_s * hz)` with the
ASM-066 two-ULP guard, at least 1, ASM-078; tick `k` runs at time `k`):

| Node | Semantics |
|---|---|
| `inverter` | Swaps Success and Failure; Running passes. |
| `succeed_always` | Failure becomes Success; Success and Running pass. |
| `repeat` | Ticks its child once per tick. A child Success counts and the node returns Running until `count` successes (then Success, and the count restarts); without `count` it never succeeds. A child Failure is Failure and resets the count. |
| `cooldown` | After its child finishes (Success or Failure) at tick `t`, returns Failure without ticking the child at `t+1 .. t+n-1` and ticks it again at `t+n`. A cooling cooldown is not Running. |
| `timeout` | A child that started at tick `s` is ticked at `s .. s+n-1`; if it is still Running at `s+n`, it is halted and the node returns Failure. |

**Leaves:** a `condition` returns Success or Failure, never Running, and emits no intents (it may write
the blackboard, e.g. `store_as`). An `action` may return any status and emits intents. Both are resolved
by ID in the registry and built with their (substituted) `params`.

**`subgraph`:** see [Subgraphs](#subgraphs). **`fsm`:** see [FSM rules](#fsm-rules).

## Halt
Halting abandons a Running activation. It follows ADR 0013 as amended on 2026-10-17 and 2026-10-18
(ASM-095, ASM-096). Blackboards are never cleared by a halt (ADR 0014).

- A node is **Running** when the last time it was ticked it returned Running and it has not been halted
  since. A cooling `cooldown` returns Failure, so it is not Running.
- **Parent halt** (what the tree does by itself): a reactive composite skipping or pre-empting a child, a
  `parallel` deciding, a `timeout` expiring, an FSM guard transition, a `subgraph` node whose required
  input is unset, and everything these reach. It is a **no-op on any node that is not Running**. A halted
  Running node resets its own state (a memory composite's position, a `parallel`'s slots, a `repeat`'s
  count, a `timeout`'s start, an FSM back to `initial`), tells a Running action to halt (a `wait` restarts
  its count), and passes the halt on to its **Running** children only. Finished children keep their state.
  So **a cooling cooldown keeps its timer**: in `sequence [enemy_near, selector [cooldown(fire), aim]]`,
  losing and regaining the contact while `aim` runs does not let `fire` skip its cooldown.
- **Instance halt** (`BehaviorInstance::halt`, an API call; the kernel does not issue one during a run in
  M3): every node returns to the state it had when the instance was created, cooldown timers included;
  Running actions are told to halt. The next tick starts fresh.
- **Through a subgraph:** a parent halt of a Running `subgraph` node calls `halt_running` on the child
  instance, which for a graph child is the same Running-only halt from its root, so the inlined and the
  subgraph forms behave identically. An instance halt of the caller is an instance halt of the child.
  A child behavior that does not override `halt_running` (built-in or plugin behaviors written in code)
  gets the full reset.

## FSM rules
An `fsm` node has states (each runs one node, which may itself be an `fsm`, giving hierarchy), optional
exits, and transitions in file order (ADR 0012, ADR 0013, ASM-072, ASM-097). Per tick, at most one
guard transition and one completion transition fire:
1. **Guards:** the `when` transitions of the current state are evaluated in file order; the first whose
   guard returns Success fires. The current state's node gets a parent halt (a no-op unless Running).
   If the target is an exit, the FSM returns the exit's status and resets to `initial` without ticking
   anything else; otherwise the new state becomes current.
2. **State:** the current state's node is ticked once (the new one, if a guard fired).
3. **Completion:** if it returned Success or Failure, the first `on` transition of that state with that
   status fires: to an exit (the FSM returns the exit's status and resets to `initial`) or to a state (its
   node is first ticked on the next tick). A state node that finished with no matching `on` simply starts
   again on the next tick.
4. Otherwise, and in every case that does not enter an exit, the FSM returns **Running**. An FSM without
   exits never finishes on its own.

Halting an FSM (parent or instance halt) resets it to `initial`. Exits and states share one namespace
(`duplicate_name`); there are no reserved names; `running` is not a valid exit or `on` value.

Load-time rules (REQ-B-12): `initial` must name a state (`no_initial_state`); every state must be
reachable from `initial` through transitions (`unreachable_state`); `from`, `to` must name a state and a
state or exit (`invalid_value`); a guard's subtree may contain only `condition`, `inverter`, `sequence`
and `selector` nodes (`invalid_guard`), so a guard never returns Running and cannot hold an action or a
subgraph.

Things to know:
- **A guard to the current state restarts it.** A `when` transition from a state to itself halts and
  re-enters that state (ASM-097(b)). If its guard is true every tick, the state restarts every tick: a
  `wait` in it never finishes and a memory sequence never gets past its first Running step. Guard only
  the transitions into a state, or make the guard false once the state is entered.

  ```toml
  # Pitfall: while `enemy_seen` is true, `engage` is restarted on every tick.
  [[nodes.modes.transitions]]
  from = "engage"
  to = "engage"
  when = "enemy_seen"
  ```
- **A guard node belongs to one transition.** Guards count as parents, so one guard used by two
  transitions is `node_reused` at the second `when`. Copy the guard node for each transition (as
  `in_range` and `in_range_again` in the example). Reusable guards are an M8b candidate.

  ```toml
  # Invalid: `in_range` is the guard of two transitions (node_reused).
  [[nodes.modes.transitions]]
  from = "patrol"
  to = "engage"
  when = "in_range"

  [[nodes.modes.transitions]]
  from = "search"
  to = "engage"
  when = "in_range"
  ```
- **Guards run before the state and may write the blackboard.** A `store_as` in a guard writes even when
  that guard (or a later part of its subtree) fails, the write is visible to the state ticked in the same
  tick, and guards after the first Success are not evaluated.

## Interface and blackboard
A graph has one **blackboard** per instance: a typed key-value store (ADR 0014, ASM-071). Keys are
declared, values start unset, a write of the wrong type or to an undeclared key is refused (validation
makes that impossible for the built-in leaves). It lives as long as the instance and is not cleared by a
halt. Types: `bool`, `number` (finite `f64`), `integer` (`i64`), `string`, `entity`, `position` (ECEF).

The **interface** declares what a caller can pass in and read out:
- Every interface param is also a **read-only key** of the same name and type in the graph's own
  blackboard. Literal and default values are set when the instance is created; a bound value is copied
  in before every tick. A leaf `store_as` or a call-site output that writes a param key is
  `invalid_binding`.
- **Literal-capable params** (`boolean`, `number`, `integer`, `string`) form the graph's JSON Schema:
  `required` = the params without `default`, no other keys allowed. Callers give them literally, via
  `{ param = .. }`, or (in `subgraph` calls) bind them with `{ key = .. }`.
- **Binding-only params** (`entity`, `position`) have no literal form, no `default` and are therefore
  always required. They can only be bound with `{ key = .. }`, so a graph that has one can only be called
  as a subgraph; as a template behavior it is `missing_param` (see `docs/templates.md`). Inside the graph
  they are read through the blackboard, e.g. `target_key = "target"`. They are listed in the catalog
  with their type (`param_types`).
- **Outputs** expose one of the graph's keys under a port name; a caller may copy it into one of its own
  keys (`outputs = { <port> = "<parent key>" }`).

The two forms in call-site `params` (ADR 0014):

| Form | When | Where allowed | Rules |
|---|---|---|---|
| `{ param = "x" }` | build time, once | `condition`, `action`, `subgraph` params | `x` is an interface param of the enclosing graph; its type must fit the target property (`integer` satisfies `number`; `entity`/`position` satisfy nothing); the substituted value is checked against the target's schema at every call site |
| `{ key = "k" }` | before every tick | top-level values of `subgraph` params only | `k` is a key of the calling graph (its `[blackboard]` or its own read-only param keys); its type must equal the param's type (`integer` may feed `number`); the target must not use that param as `{ param = .. }` (its value is needed at build time); a registered behavior's param must be declared bindable (`param_types`) |

Rule of thumb: a param that a graph uses as `{ param = .. }` is fixed per instance and cannot be bound;
a param read through its key can be bound and change every tick. Bound values can be passed further
down: a child may bind its own param key to a grandchild's param.

**Integer to number.** When an `integer` key is bound to a `number` param, the value is converted with
`i as f64` on every copy (ASM-088). Integers beyond 2^53 (9 007 199 254 740 992) in magnitude lose
precision: 2^53 + 1 arrives as 2^53. An `integer` param receives the integer unchanged.

**Template `[behavior] params`** are literals only: both forms are `invalid_binding` there (a top-level
behavior has no parent blackboard and no enclosing interface).

## Subgraphs
A `subgraph` node calls another behavior by ID: a graph of the same load, a built-in graph, or (from M4) a
plugin behavior; they are indistinguishable at the call site (REQ-B-9). Each `subgraph` node in each
entity's instance owns a separate child instance with its own node state and blackboard (REQ-B-11):
the child never sees the caller's blackboard; data crosses only through params in and outputs out.

**Versions** (ADR 0017, ASM-085): `version` is a Cargo-style range (`semver::VersionReq`): `"^1.0"`
(>= 1.0.0, < 2.0.0), `"~1.2"` (>= 1.2.0, < 1.3.0), `"=1.0.0"`, `">=1.1, <2"`. Absent means any version.
A pre-release version (`1.1.0-rc.1`) matches only a range that names a pre-release of the same version.
A bad range is `invalid_version`; a target whose version does not satisfy it is `version_mismatch` with a
note at the target's `version`. Versioning rule for authors (REQ-B-14, not machine-checked): removing or
renaming a param or port, or changing a type or meaning, needs a major bump; a new optional param or a new
port is minor.

**Per tick of a `subgraph` node** (ADR 0013, ASM-088, ASM-099(d)):
1. If a required bound param's parent key is unset, the node halts a Running child (parent halt) and
   returns Failure without ticking it. An optional bound param whose parent key is unset is unset in the
   child (its `default` is not used).
2. Every bound input is copied from the parent key into the child's read-only key (in param-name order;
   unset copies as unset; `integer` to `number` converted).
3. The child is ticked once; its status is the node's status.
4. Every bound output port is copied into its parent key (in port-name order), including unset.

A parent key can be the target of at most one output binding in a graph (`invalid_binding` for the
second); any number of inputs may read it.

**Limits:** a `subgraph` chain holds at most **16 graphs**, the calling root graph included (17 is
`subgraph_too_deep` at the `ref` of the call made by the 16th graph; ASM-073); embedders can change the
loader's limit with `LoaderOptions::max_subgraph_depth`, but registration always applies 16. Inside one
graph, nodes nest at most **128 levels** (`node_too_deep`). Graphs that call each other in a loop are
`subgraph_cycle` (once per strongly connected group, at the first `ref` of the shortest cycle through its
smallest ID, with one note per remaining edge). Depth counts behaviors, the same way at load time (`cgf-cli validate`) and at registration: a call to a
registered graph (for example `builtin.fsm.patrol_and_engage`, which itself calls
`builtin.subgraph.engage_target`) adds that graph's whole longest chain, one level per behavior, and a
registered behavior that calls no subgraph (a built-in leaf-like behavior, a plugin behavior) adds 1. So
15 of your graphs ending in `builtin.fsm.patrol_and_engage` are 17 levels and `subgraph_too_deep` at the
call that crosses the limit, with a note that names the registered chain; what `validate` accepts also
instantiates (ASM-099 e).

## Load-time checks
`cgf-cli validate` and every scenario load run the M2 pipeline over graph files too (A-001): all errors of
all files in one sorted list, located at file, line and column, no follow-on errors from a graph that is
broken (syntax or reader errors, a duplicate ID): calls to it get no further checks. In order:
1. Per file: syntax, `schema_version`, keys and types, names, node settings, FSM structure, the interface
   fragments.
2. The set: duplicate IDs (between files, and against registered components), tree shape, FSM
   reachability and guards, `[blackboard]` against the interface, `outputs.from`.
3. Leaves: the `ref` is a registered condition or action of the right kind (`unknown_id`, "`x` is an
   action, not a condition"); params against the leaf's schema with `{ param }` references substituted by
   the enclosing param's `default`; key params (`target_key`, `store_as`) name declared keys of the right
   type; then a trial build.
4. Subgraph calls: target exists (`unknown_id`), version, cycles, depth, bindings (`invalid_binding`,
   `missing_param` for an unbound binding-only param), then the literal params (after substituting the
   caller's references) against the target's interface (`missing_param`, `unknown_param`,
   `param_wrong_type`, `param_out_of_range`); a param given by binding counts as given.
5. Templates: a `[behavior] ref` to a graph is checked like any behavior (`docs/templates.md`).

**Values are followed down the chain** (ASM-076, ASM-106(d), ASM-107(d)): a value a caller supplies (a
template literal, or a call-site literal or reference) is substituted for the callee's `{ param }`
references and checked again wherever it lands: the callee's leaves, and the params of the callee's own
`subgraph` calls, and so on down the chain (registered behaviors by their schema and a dry run). An error
is reported at the original supplying value, with a note at the `{ param }` reference and, below the first
callee, a note naming the chain. A value the callee's interface already refused is reported once and not
followed. A param that has no default and is not supplied is unknown at load; the values that are known
are still checked.

**Accepted limit.** Each supplied value is followed on its own; the callee's other params are their
defaults or unknown. So a rule that combines **two passed-through values from different calls** (a
`oneOf` across two params, or a registered behavior's build that rejects a combination) is checked at load
only when both values come from the same call. Otherwise it is checked only if the component's own build
enforces it, at spawn time, without a file location (ASM-076, ASM-106(d)). None of the built-in leaves has
a rule across params, so this concerns registered behaviors and plugin components.

## Built-in catalog
`cgf-cli schema component <ID>` prints the parameter schema of any of these (the authoritative source);
`cgf-cli schema behavior` prints the graph file schema. Built-in leaves and node types carry provenance
`builtin` (read-only) and the package version (0.1.0 at M3); the built-in graphs have version `1.0.0`.
Unknown params are errors for all of them. "Contacts" are the entities this entity's sensors see this
tick; without a sensor there are none.

### Conditions
| ID | Params | Success when |
|---|---|---|
| `builtin.cond.enemy_in_range` | `range_m` (number > 0, required), `store_as` (string: an `entity` key of `[blackboard]`, optional) | a hostile contact is within `range_m` (3D range, inclusive). Hostile = both entities have a force and the forces differ; an entity without a force is never hostile. With `store_as`, the nearest hostile (ties: lowest entity id) is written to that key; the key is left unchanged on Failure. |
| `builtin.cond.target_in_range` | `target_key` (string: an `entity` key, read only, required), `range_m` (number > 0, required) | the entity in `target_key` is a current contact within `range_m` (inclusive; forces do not matter). Failure when the key is unset or the entity is not a contact. |
| `builtin.cond.timer_elapsed` | `duration_s` (number > 0, required) | the run time (since tick 0, not since the node last ran) is at least `duration_s`, rounded up to whole ticks (`now.tick() >= n`). For a relative delay use `builtin.action.wait`. |
| `builtin.cond.chance` | `probability` (number in [0, 1], required) | one draw `u` in [0, 1) from the entity's own random stream per evaluation, Success iff `u < probability` (0 never, 1 always succeeds). |

### Actions
| ID | Params | Emits | Status |
|---|---|---|---|
| `builtin.action.follow_route` | none | `FollowRoute` on every tick with a non-empty route, also the completing one | Running until the last waypoint is reached, then Success (and the entity holds there); Failure, with no intent, without a route. |
| `builtin.action.move_to` | `target_key` (entity key, read only), `speed_mps` (> 0), `arrival_radius_m` (> 0), all required | `SetSpeed(speed_mps)` then `MoveTo(contact position)` while Running; nothing on Success | Running while the target is farther than `arrival_radius_m`, Success within it; Failure when the key is unset or the target is not a current contact. |
| `builtin.action.wait` | `duration_s` (> 0, required) | `Hold` while Running | Running for `n` ticks from its own first tick, Success on tick `n + 1` (no intent on that tick); starts over after it finished or was halted. |
| `builtin.action.fire` | `target_key` (entity key, read only), `weapon` (a namespaced ID, e.g. `example.weapon.main_gun`), both required | `Fire { target, weapon }` | Success in the same tick; Failure, without firing, when the key is unset or the target is not a current contact. Recorded only, see [Intents](#intents-and-what-they-do-not-do-yet). |

`follow_route` follows the entity's initial scenario route once (no looping). Route progress belongs to
the kernel and advances only on ticks where `FollowRoute` is expanded, so after an interruption the
entity heads for the waypoint it was going to. There is no `health_below` condition: there is no health
model in v1 (spec 1.3).

### Sensor `builtin.visual`
| Param | Bound | Notes |
|---|---|---|
| `range_m` | > 0, <= 100000, required | 3D detection distance, inclusive |
| `fov_deg` | > 0, <= 360, required | horizontal field of view centred on the heading, edge inclusive; 360 sees all around |
| `eye_height_m` | >= 0, default 2 | height of both entities' eyes for the line-of-sight test |

See [Perception](#perception) for the rules and the `eye_height_m = 0` note.

### Built-in graphs
Both are embedded in `cgf-engine` (`crates/cgf-engine/behaviors/`) and registered as behaviors with
provenance `builtin`, version `1.0.0`; they use only built-in leaves.

**`builtin.fsm.patrol_and_engage`** (the spec 7.1 template's behavior): follow the initial route; engage
the nearest hostile that comes within `engage_range_m`.

| Param | Type | Default |
|---|---|---|
| `aggression` | number in [0, 1] | 0.5: the chance of firing on each ready tick |
| `engage_range_m` | number > 0 | 3000 |
| `fire_range_m` | number > 0 | 1500 |

Structure: reactive selector `[engage, patrol]`; `engage` = sequence `[enemy_in_range(range_m =
engage_range_m, store_as = target), subgraph builtin.subgraph.engage_target(target = { key = "target" },
fire_range_m, aggression)]`; `patrol` = `follow_route`. Blackboard `target: entity`. Result: "Running
while patrolling or engaging; Success when the route is complete and no hostile is in range".

**`builtin.subgraph.engage_target`**: approach the target handed in by the caller and fire at it while it
is in fire range. Needs a `{ key }` binding for `target`, so it cannot be a template behavior.

| Param | Type | Default |
|---|---|---|
| `target` | entity, binding-only | required |
| `fire_range_m` | number > 0 | 1500 |
| `approach_speed_mps` | number > 0 | 8 |
| `aggression` | number in [0, 1] | 0.5 |

Structure: an FSM with `approach` (`move_to` the target at `approach_speed_mps`, arrival radius
`fire_range_m`) and `engage` (memory sequence `chance(aggression)`, `fire` with weapon
`builtin.weapon.default`, `wait` 4 s); guard transitions on `target_in_range(fire_range_m)` and its
inverse; no exits. It returns Failure when its required input `target` is unset (the subgraph node does
that); while the target is set it is Running, also when the target is no longer a contact (then
`move_to` fails each tick and the `approach` state restarts), so a caller should stop calling it when
the target is lost, as `builtin.fsm.patrol_and_engage` does through `enemy_in_range`.

### Node types in the catalog
The twelve node types above are catalog entries too (provenance `builtin`, with their settings schema).
The catalog (`cgf_core::Catalog`) lists node types, conditions, actions and behaviors with ID,
description, provenance, read-only flag, version, parameter schema, binding-only param types, outputs and
result text (REQ-B-17); it is exposed over REST in M5.

## Perception
The kernel's Perception phase runs before the Behavior phase (ADR 0015, ASM-080, ASM-108):
- Observers are **Owned entities with at least one sensor**, in `EntityId` order; targets are all other
  entities, Owned and Remote, as they were before the tick. **Contacts require a sensor**: an entity
  without one has no contacts, so `enemy_in_range`, `target_in_range`, `move_to` and `fire` all fail.
- Contacts from several sensors are merged per target (the first sensor in template order wins) and
  sorted by entity id. A contact carries the target's id, position, velocity, force and 3D range.
- `builtin.visual` detects perfectly (no probability, no random draw): the 3D distance is at most
  `range_m`; the horizontal bearing to the target is within `fov_deg / 2` of the observer's heading (a
  target straight above or below counts as inside); and the terrain gives line of sight between the two
  positions, each raised by `eye_height_m` along the local vertical. The checks run in that order, so the
  expensive line-of-sight test only runs for targets in range and field of view.
- **`eye_height_m = 0` on smooth ground never sees anything.** With both points on the surface, the
  straight line between them dips below it, by the Earth's curvature even on flat terrain, so two
  entities at height 0 with `eye_height_m = 0` are never contacts at any range. Keep the default (2 m) or
  another positive value.
- The default line-of-sight test samples the straight segment at fixed spacing (about every 20 m, 2 to
  512 points) against the terrain height. Perception is O(n^2) in the number of entities in M3; a spatial
  index comes with M9 profiling.

## Intents and what they do not do yet
The intent set is version 1 (`INTENT_SET_VERSION = 1`, ADR 0016): `MoveTo`, `SetSpeed`, `Hold`,
`FollowRoute`, `SetHeading`, `Fire`, `Spawn`, `Despawn`, `Custom`. The built-in actions emit `FollowRoute`,
`SetSpeed`, `MoveTo`, `Hold` and `Fire`. `FollowRoute` is replaced by the kernel, in place, with the route
stand-in's intents for that tick (`SetSpeed` + `MoveTo` toward the next waypoint, or `Hold` at the end);
later `FollowRoute`s in the same tick are dropped, and without a route it is dropped. `SetHeading` is
steered to by `builtin.ground.kinematic` (no built-in action emits it yet).

**`Fire`, `Spawn`, `Despawn` and `Custom` are recorded, not applied, until M5** (human decision
2026-10-17): they appear in `EntitySnapshot::intents` for that tick and nowhere else. `Fire` does not
damage, destroy or even notify its target, produces no event and changes no state; `Spawn` creates no
entity; `Despawn` removes nothing; `Custom` reaches no consumer. They are not part of the state hash.
`cgf-cli run` does not print intents; embedders read them from the snapshot. An entity whose behavior
emits no intent in a tick gets none (it slows to a stop); an Owned entity without a behavior follows its
route exactly as in M2.

## Diagnostics
Same format, sorting and rules as templates (`docs/templates.md`): `path:line:col: error[code]: message`
plus `note:` lines, all files together, sorted, no duplicates. Examples:
```
behaviors/a.behavior.toml:19:7: error[subgraph_cycle]: subgraph cycle: `example.a` -> `example.b` -> `example.a`
  note: `example.b` references `example.a` (behaviors/b.behavior.toml:12:7)
behaviors/p.behavior.toml:31:11: error[version_mismatch]: `example.subgraph.attack` 1.0.0 does not satisfy `^2.0`
  note: defined in behaviors/attack.behavior.toml:4:11
```

Every code the graph loader can emit (positions per ASM-092(b)):

| Code | Meaning in a graph file |
|---|---|
| `io` | The file could not be read |
| `syntax` | TOML syntax error; the file is not checked further and calls to its ID get no follow-on errors |
| `unsupported_value` | A datetime, a non-finite float or an integer outside 64 bits (also inside `[editor]`) |
| `unknown_key` | A key the format does not define, at any level except inside `[editor]` (a binding-only fragment allows only `type` and `description`) |
| `missing_key` | A required key is absent: `id`, `version`, `root`, `nodes`, `schema_version`, a node's `type`, `ref`, `child`, `children`, `duration_s`, `states`, a transition's `from`/`to`, a fragment's `type`, a port's `type`/`from` |
| `wrong_type` | A value of the wrong TOML type |
| `invalid_value` | A value out of range or otherwise invalid: unknown node `type`, name grammar or length, empty `children`, `count` out of range, `duration_s` <= 0, bad `policy`, unknown blackboard or port type, an interface type other than the six (`bool`, `object`, `array`), an unsupported schema keyword in a fragment, a `default` that breaks its fragment or is given for a binding-only param, a transition `from`/`to` that names no state or exit, a transition without exactly one of `when`/`on`, an exit or `on` value other than `success`/`failure` |
| `invalid_id` | `id` or a `ref` breaks the ADR 0002 grammar (at the offending character) |
| `unsupported_schema_version` | `schema_version` newer than 1; the rest of the file is not checked |
| `duplicate_id` | A second graph file with the same ID (note at the first), or a graph ID that a registered component of any kind already has, built-in graphs included (note names the kind) |
| `duplicate_name` | A state and an exit with one name (at the exit), or a `[blackboard]` key that repeats an interface param |
| `unknown_id` | A leaf or subgraph `ref` that is not registered and no graph of the load; also a ref of the wrong kind ("`x` is an action, not a condition"); notes list candidates and unreadable files |
| `missing_param` | A required param is missing: in a leaf's params, at a `subgraph` call (a binding-only param without a `{ key }` binding included), or at a template's `[behavior] params` |
| `param_wrong_type` | A param value of the wrong type (leaf, call site or template; also after `{ param }` substitution) |
| `param_out_of_range` | A param value outside its bounds or length limits |
| `unknown_param` | A param the leaf's schema or the target's interface does not define |
| `param_invalid` | An `enum`/`const`/`oneOf` violation, or a combination the component's trial build rejected |
| `not_a_scenario` | `load_scenario` was given a `*.behavior.toml` file |
| `unknown_node` | A `root`, child, state or guard name that no `[nodes.*]` table defines |
| `node_reused` | A node reached a second time (two parents; FSM states and guards count as parents) |
| `node_cycle` | A node that reaches one of its own ancestors |
| `unreachable_node` | A node not reachable from `root` (at its `[nodes.<name>]` header) |
| `node_too_deep` | A node nested deeper than 128 levels (at the reference that makes level 129) |
| `invalid_version` | `version` is not a semantic version, or a `subgraph` `version` is not a range |
| `version_mismatch` | The target's version does not satisfy the call's range (note at the target's `version`) |
| `subgraph_cycle` | Graphs call each other in a loop (once per cycle group) |
| `subgraph_too_deep` | A `subgraph` chain longer than 16 behaviors, registered chains included (note names the registered chain) |
| `no_initial_state` | An `fsm` without `initial`, or whose `initial` names no state |
| `unreachable_state` | An FSM state that no chain of transitions from `initial` reaches (at the state key) |
| `invalid_binding` | A `{ key }` form outside a `subgraph` node's top-level params (also in leaf or template params); a `{ param }` form in template params; a `{ param }` naming an undeclared param or one of an incompatible type; a `{ key }` binding to an undeclared key or one of another type; binding a param that the target uses as `{ param }` (note at the reference) or that a registered behavior does not declare bindable; an output binding to an unknown port, to an undeclared, read-only or wrongly typed key, or to a key another output already writes; an `outputs.from` that is undeclared or of another type; a `target_key`/`store_as` naming an undeclared key, a key of the wrong type, or (for `store_as`) a read-only param key |
| `invalid_guard` | A guard subtree with a node other than `condition`, `inverter`, `sequence`, `selector` (at the `when` value, naming the first such node) |

`unknown_template` and `inheritance_cycle` are template-only.

## Determinism
Behavior runs are reproducible with the same scenario and seed in every clock mode (REQ-K-4, REQ-B-16;
gate 4 runs `examples/scenarios/patrol_and_engage`):
- Behaviors may use only simulation time, perception and the entity's own random stream. `chance` is the
  only built-in leaf that draws; it draws exactly once per evaluation from the entity's hashed `EntityRng`
  stream (ADR 0006), so changing the seed changes the shots and nothing else leaks in.
- Order is fixed: entities in id order, depth-first in array order, transitions in file order, contacts
  sorted by id, nearest hostile ties broken by id, subgraph inputs and outputs copied in name order. All
  maps are ordered (`BTreeMap`); there is no wall clock and no `HashMap` iteration.
- Each entity sees only pre-tick state of the others and has its own contact list, intent buffer and
  random stream, so the visiting order cannot change results.
- Durations are whole ticks, fixed when the instance is created; `timer_elapsed` uses the absolute tick.
- `[editor]` contents never reach the engine: changing layout changes nothing.
- The state hash stays version 2: node state, blackboards, contacts and intents are not hashed (their
  effects reach the hash through motion and the random streams). The determinism gate therefore also
  compares per-tick intents and contacts (ASM-081, ASM-087).

## Embedding: stack size
The interpreter ticks the tree by recursion, about two frames per node level and more per subgraph level.
The worst case the limits allow (16 graphs x 128 levels) needs about 1.25-1.5 MiB of stack in a debug
build and 384-512 KiB in a release build (ASM-098, measured by `crates/cgf-behavior/tests/stack_budget.rs`
on Linux). So:
- the thread that calls `Kernel::tick`, `Engine::step` or `Engine::run` needs at least **2 MiB**; spawn it
  with an explicit stack of **4 MiB or more** (`std::thread::Builder::stack_size`);
- do not rely on a process main thread: it has 8 MiB on Linux but only 1 MiB on Windows;
- `cgf-cli` runs its command on a thread with 8 MiB; the M5 server's tick thread must get 4 MiB or more
  (`docs/milestones.md`, M5).
