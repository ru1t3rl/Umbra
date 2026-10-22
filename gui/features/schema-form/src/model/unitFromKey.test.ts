import { describe, expect, test } from "bun:test";
import { unitFromKey } from "./unitFromKey.ts";

// Assumed API: unitFromKey(key: string): string | undefined (ASM-818, ASM-897).
describe("unitFromKey (ASM-818)", () => {
  test.each([
    ["range_m", "m"],
    ["max_speed_mps", "m/s"],
    ["max_accel_mps2", "m/s²"],
    ["fov_deg", "°"],
    ["max_turn_rate_dps", "°/s"],
    ["scan_period_s", "s"],
    ["rate_hz", "Hz"],
  ])("%s has the unit %s", (key, unit) => {
    // Catches: a missing suffix row or a swapped unit.
    expect(unitFromKey(key)).toBe(unit);
  });

  test("the longest suffix wins: _mps2 is not _mps and _mps is not _m", () => {
    // Catches: matching with endsWith("_m")/("_s") before the longer suffixes.
    expect(unitFromKey("a_mps2")).toBe("m/s²");
    expect(unitFromKey("a_mps")).toBe("m/s");
    expect(unitFromKey("a_dps")).toBe("°/s");
  });

  test.each([
    "probability",
    "target_key",
    "m",
    "_m",
    "range_km",
    "rangem",
    "speed_ms",
    "RANGE_M",
    "",
  ])("%p has no unit", (key) => {
    // Catches: matching without the underscore, an unknown suffix, or case-insensitive matching.
    expect(unitFromKey(key)).toBeUndefined();
  });
});
