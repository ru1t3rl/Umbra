# air_sea

The M7 air and sea example (ADR 0051, WI-6): one entity per motion model, near 52 N 4 E on flat terrain.

| Entity | Template | Route |
|---|---|---|
| `aircraft-1` | `aircraft.light` (`builtin.air.kinematic`, 40 to 120 m/s, 10 m/s climb, 25 degrees of bank) | four 5 km legs at 100 m/s, climbing 300 -> 800 -> 1300 -> 1500 -> 1500 m (ellipsoidal) |
| `ship-1` | `ship.patrol_boat` (`builtin.sea.kinematic`, 10 m/s, 80 m smallest radius, height 0) | a 400 m square at 8 m/s |
| `truck-1` | `truck.generic` (`builtin.ground.kinematic`) | a 600 m square at 15 m/s |

The climb finishes between 120 s and 140 s of the 300 s run (the aircraft levels at 1300 m until it
has passed waypoint 1), before the last waypoint, because `Hold` keeps the height. The aircraft cannot
stop (minimum speed 40 m/s): at the last waypoint it keeps flying around it at 1500 m, alternating
between slowing down and speeding up (45 to 100 m/s, up to about 1.5 km from the waypoint). The
ship and the truck stop. Waypoints were computed with 1 degree of latitude = 111 200 m and longitude
scaled by cos(latitude).

```
cargo run -p umbra-cli -- validate examples/scenarios/air_sea/scenario.toml
cargo run -p umbra-cli -- run examples/scenarios/air_sea/scenario.toml --ticks 300 --print-every 30
```
