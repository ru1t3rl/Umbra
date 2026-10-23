// WI-10 / ASM-938 (written before the decoder change): a decoded frame says whether it is a control frame
// (`cause: "control"`, sent after a command), as `control: boolean`. Unknown causes stay ignored (ASM-359).
import { describe, expect, test } from "bun:test";
import { entity, frame } from "../testing.ts";
import { decodeJson } from "./decode.ts";

const dec = (m: unknown) => decodeJson(JSON.stringify(m));
const controlOf = (cause: unknown): boolean | undefined => {
  const d = dec(frame(0, 1, [entity(1)], { cause }));
  if (d.kind !== "frame") throw new Error("expected a frame");
  return (d as { control?: boolean }).control;
};

describe("decodeJson cause", () => {
  test('cause "control" is a control frame', () => {
    expect(controlOf("control")).toBe(true);
  });
  test('cause "tick" is not', () => {
    expect(controlOf("tick")).toBe(false);
  });
  test("an unknown, missing or mistyped cause is not a control frame, and the frame still decodes", () => {
    for (const cause of ["replay", "CONTROL", "", 1, null, undefined, { a: 1 }]) {
      expect(controlOf(cause)).toBe(false);
    }
  });
});
