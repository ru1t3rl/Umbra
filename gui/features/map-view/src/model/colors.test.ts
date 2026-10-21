import { describe, expect, test } from "bun:test";
import { resolveMapColors } from "./colors.ts";

const TOKENS: Record<string, string> = {
  "surface-0": "#000001",
  "surface-1": "#000002",
  line: "#000003",
  "line-strong": "#000004",
  "ink-muted": "#000005",
  "symbol-frame": "#000006",
  "affil-friend": "#000007",
  "affil-hostile": "#000008",
  "affil-neutral": "#000009",
  "affil-unknown": "#00000a",
  "opacity-track": "0.55",
};
// DOM-free: the model only needs getPropertyValue.
const source = (tokens: Record<string, string>) => ({
  getPropertyValue: (name: string) => tokens[name.replace(/^--/, "")] ?? "",
});

describe("resolveMapColors", () => {
  test("reads every token, trimming whitespace", () => {
    expect(resolveMapColors(source({ ...TOKENS, line: "  #000003 " }))).toEqual({
      surface0: "#000001",
      surface1: "#000002",
      line: "#000003",
      lineStrong: "#000004",
      inkMuted: "#000005",
      symbolFrame: "#000006",
      affiliation: {
        friend: "#000007",
        hostile: "#000008",
        neutral: "#000009",
        unknown: "#00000a",
      },
      opacityTrack: 0.55,
    });
  });
  test("every missing token throws and names it", () => {
    for (const token of Object.keys(TOKENS)) {
      const { [token]: _gone, ...rest } = TOKENS;
      expect(() => resolveMapColors(source(rest))).toThrow(`--${token}`);
    }
  });
  test("opacity-track must be a number in 0..1", () => {
    for (const bad of ["half", "1.5", "-0.1", "NaN", "0.5x"]) {
      expect(() => resolveMapColors(source({ ...TOKENS, "opacity-track": bad }))).toThrow(
        "opacity-track",
      );
    }
    expect(resolveMapColors(source({ ...TOKENS, "opacity-track": "1" })).opacityTrack).toBe(1);
    expect(resolveMapColors(source({ ...TOKENS, "opacity-track": "0" })).opacityTrack).toBe(0);
  });
  test("a real computed style works too (happy-dom)", () => {
    const el = document.createElement("div");
    for (const [k, v] of Object.entries(TOKENS)) el.style.setProperty(`--${k}`, v);
    document.body.append(el);
    expect(resolveMapColors(getComputedStyle(el)).surface0).toBe("#000001");
    el.remove();
  });
});
