# Tabs

Tabs switch between sibling views of one object: the sections of a sidebar, the pages of a scenario editor. The active tab is marked by a 3px `azure` indicator.

**You provide** `items` (`value`, `label`, optional mono `count`), the current `value` and `onChange`.

**Do**: keep labels to one or two words in sentence case; show counts for lists that change at runtime; keep tabs in a fixed order.

**Don't**: use tabs for actions or for navigation between unrelated pages; exceed six tabs (move the rest into a menu); show a count above 9999, show "9999+".

Height 40px, `space-3` padding, `line` hairline underneath, hover paints `state-hover`.
