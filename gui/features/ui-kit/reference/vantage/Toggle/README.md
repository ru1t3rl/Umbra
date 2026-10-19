# Toggle

A Toggle switches one setting on or off with immediate effect: a map layer, recording, a sim option. If the change needs a Save button, use a checkbox in a form instead.

**You provide** `checked`, `onChange(next)`, a `label` (sentence case, states the on condition: "Record session") and optionally `disabled`.

**Do**: label every toggle; place the label to the right; apply the setting the moment it flips.

**Don't**: use a toggle for mutually exclusive options (Chips); hide the label and rely on the thumb position.

Track 44×24, `line-strong` outline at rest; on, the track fills `azure` and the thumb turns `on-azure`. `radius-full`, `duration-fast`.
