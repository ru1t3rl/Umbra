# Network scenario fixtures (M6 WI-8)

Used by `scenario_network.rs` with an in-test network adapter `test.net.loopback` (`port` integer).

- `clean/`: a valid `[[network_adapters]]` entry; no warnings.
- `warning/`: valid, but `timeout_s` (2.0) is not above the default `heartbeat_s` (5.0): loads, and
  `validate` reports a `suspicious_value` warning (ASM-290).

`../network_errors/` (a sibling, so directory validation of `network/` stays clean): one error
(`max = 0`) next to a short `timeout_s`; `validate` reports the error and no warning.

`../network_template_error/`: a valid scenario (short `timeout_s`) whose template set has an error;
`validate` reports the error and no warning (only clean scenarios contribute warnings).
