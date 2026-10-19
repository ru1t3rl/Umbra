# Slider

A Slider sets a continuous or stepped numeric value where the operator cares about the position in the range more than the exact digit: time rate, update rate, track history length, layer opacity.

**You provide** a `label`, `min`, `max`, `step`, `value`, `onChange` (native input event) and a `format` function for the live value shown in `cyan` mono.

**Do**: show the formatted value with its unit; snap stepped sliders to the scenario's allowed steps; pair with a TextField when exact entry matters.

**Don't**: use a Slider for fewer than four discrete options (Chips); animate the value label.

Track 4px `line-strong`, thumb 20px `azure` with a `surface-0` ring; focus uses `shadow-focus` because a range thumb cannot take an outline.
