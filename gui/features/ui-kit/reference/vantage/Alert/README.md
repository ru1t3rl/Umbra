# Alert

An Alert is an inline banner about the simulation's state: a rejected order, a degraded federate, a confirmation. It says what happened, then what to do, and offers one action.

**You provide** a `tone` (`danger`, `warning`, `success`, `info`), a `title` (what happened, in sentence case, with the number if there is one), body copy as children (what to do next) and optionally one `action` (a `ghost` or `tonal` Button, size `sm`).

**Do**: anchor map alerts under the CommandBar and stack at most two; send the rest to the event log; use `danger` only for faults, losses and rejected commands.

**Don't**: add a coloured left border (the tinted ground is the signal); use more than one action; auto-dismiss a `danger` alert.

Width 480px, `radius-md`, `-dim` ground, title and glyph in the status colour, body in `ink-muted`.
