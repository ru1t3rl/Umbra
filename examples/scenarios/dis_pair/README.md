# dis_pair: two Umbra instances exchanging entities over DIS

Provenance: written for M6 WI-16 (REQ-N-6, M6 acceptance 1), modelled on `examples/scenarios/walking_skeleton`
(same motion model and route shape). Two scenarios share `templates/`:

| | A (`a/scenario.toml`) | B (`b/scenario.toml`) |
|---|---|---|
| DIS site / application | 1 / 1 | 1 / 2 |
| Receives on | `127.0.0.1:47001` | `127.0.0.1:47002` |
| Sends to | `127.0.0.1:47002` | `127.0.0.1:47001` |
| Force | `friendly` | `opposing` |
| Entities | `a-tank`, `a-truck` | `b-tank`, `b-truck` |

Both use DIS version 7, unicast, the real-time clock and a 10 s duration. The templates declare
`[network.dis] entity_type` (tank `1.1.225.1.1.0.0`, truck `1.1.225.4.1.0.0`); that token is how each side
maps the other's Entity State PDUs back to a template. The `friendly` and `opposing` forces are in the DIS
adapter's default force table (1 and 2).

## Run it

Build the binary and package the DIS plugin once, then start both instances (two terminals, within a second of each other):

```sh
cargo build -p umbra-cli          # builds target/debug/umbra
cargo xtask build-plugins         # packages the dis plugin into target/plugins
target/debug/umbra run examples/scenarios/dis_pair/a/scenario.toml --plugin-path target/plugins --log-level info
target/debug/umbra run examples/scenarios/dis_pair/b/scenario.toml --plugin-path target/plugins --log-level info
```

(Instead of the binary you can use `cargo run -p umbra-cli -- run <scenario> --plugin-path target/plugins
--log-level info`; start the second one while the first is running.)

After 10 s each prints a line like

```text
info: net dis: published=21 received=21 ghosts=2 echo_dropped=0 unmapped=0 tx_dropped=0 rx_dropped=0 ...
```

`ghosts=2` is the peer's two entities mirrored here; `received` > 0 shows the PDUs arrived. If the ports are in
use, change them in both files (`bind` of one is the `destinations` entry of the other).

`crates/umbra-cli/tests/cli_dis_pair.rs` runs exactly these two files (with two free ports substituted) as
two processes; `crates/umbra-engine/tests/dis_exchange.rs` covers the same exchange for v6, v7, broadcast and
multicast, and DIS next to a second adapter.

## Optional manual check with Wireshark (decision H8)

Not part of any gate. With both instances running:

1. Capture on the loopback interface (`lo`) with the filter `udp port 47001 or udp port 47002`.
2. Right-click a packet, "Decode As...", UDP port 47001 and 47002 -> `DIS`.
3. Each datagram is one Entity State PDU: version 7, PDU type 1 (Entity State), exercise 1, force 1 or 2,
   entity ID `1:1:n` or `1:2:n`, location in ECEF metres (about 3.9e6, 0.3e6, 5.0e6 near 52 N 4 E), the
   marking `a-tank`, `a-truck`, `b-tank` or `b-truck`. The tanks' positions change from packet to packet; the
   trucks repeat once a second (heartbeat).
4. Wireshark must show no "malformed packet" for any of them.

Opt-in capture for an independent decoder: see the `UMBRA_DIS_CAPTURE_DIR` note in
`crates/umbra-engine/tests/dis_exchange.rs`.
