import { expect, test } from "bun:test";
import { fieldId } from "./fieldId.ts";

test("ids start with the prefix and escape everything except letters, digits and underscore", () => {
  // Catches: raw keys in ids (spaces, dots).
  expect(fieldId("f", ["a b"])).toBe("f-a~20~b");
});
test("a field x and the help element of a field named x-help cannot collide", () => {
  // Catches: "-" kept raw, so `${id}-help` of field x equals the id of field "x-help".
  expect(`${fieldId("f", ["x"])}-help`).not.toBe(fieldId("f", ["x-help"]));
});
test("the path [a.b] differs from [a, b]", () => {
  // Catches: "." kept raw in a segment.
  expect(fieldId("f", ["a.b"])).not.toBe(fieldId("f", ["a", "b"]));
});
