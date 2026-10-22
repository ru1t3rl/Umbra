// WI-7. Assumed API (model/clockLabel.ts): clockLabel(clock: Clock): string  (engine-stream Clock, incl. { mode: "unknown" })
//   afap "As fast as possible", realtime "Real time", stepped "Stepped", scaled "Scaled ×<scale>", unknown "Unknown clock".
import { expect, test } from "bun:test";
import { clockLabel } from "./clockLabel.ts";

// Catches: a mode word changed or swapped.
test("each mode has its label", () => {
  expect(clockLabel({ mode: "afap" })).toBe("As fast as possible");
  expect(clockLabel({ mode: "realtime" })).toBe("Real time");
  expect(clockLabel({ mode: "stepped" })).toBe("Stepped");
});

// Catches: the scale dropped from the label, or printed with float noise.
test("scaled shows the factor", () => {
  expect(clockLabel({ mode: "scaled", scale: 4 })).toBe("Scaled ×4");
  expect(clockLabel({ mode: "scaled", scale: 0.5 })).toBe("Scaled ×0.5");
  expect(clockLabel({ mode: "scaled", scale: 1000000 })).toBe("Scaled ×1000000");
});

// Catches: a throw or an empty label for a mode of a newer minor version.
test("an unknown mode gets a neutral label", () => {
  expect(clockLabel({ mode: "unknown" })).toBe("Unknown clock");
});
