# Badge

A Badge is one capitalised status word with a dot: Running, Paused, Degraded, Faulted, Stale, Destroyed. The word is the signal; the colour only reinforces it.

**You provide** the word as children and a `tone`: `success`, `info`, `warning`, `danger` or `neutral` (default).

**Do**: use the status vocabulary from the content fundamentals; keep to one word, two at most; place the badge at the end of a row or under a card title.

**Don't**: use a Badge as a counter (use the Tabs `count` or mono text); use `danger` for anything that is not a fault or loss; rely on the colour alone.

Height 20px, `radius-xs`, `-dim` ground with the status colour as text, 6px dot in the same colour.
