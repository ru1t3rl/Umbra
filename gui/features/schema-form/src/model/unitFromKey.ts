/** Unit by key suffix (ASM-818). Longest suffix first; the match is case sensitive and needs the underscore. */
const SUFFIXES: readonly (readonly [string, string])[] = [
  ["_mps2", "m/s²"],
  ["_mps", "m/s"],
  ["_dps", "°/s"],
  ["_deg", "°"],
  ["_hz", "Hz"],
  ["_m", "m"],
  ["_s", "s"],
];

export function unitFromKey(key: string): string | undefined {
  for (const [suffix, unit] of SUFFIXES) {
    if (key.length > suffix.length && key.endsWith(suffix)) return unit;
  }
  return undefined;
}
