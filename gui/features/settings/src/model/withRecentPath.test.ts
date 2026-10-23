import { expect, test } from "bun:test";
import { withRecentPath } from "./withRecentPath.ts";

test("a new path goes first", () => {
  expect(withRecentPath(["a", "b"], "c")).toEqual(["c", "a", "b"]);
});
test("a known path moves to the front without a duplicate", () => {
  expect(withRecentPath(["a", "b", "c"], "b")).toEqual(["b", "a", "c"]);
});
test("works on an empty list and does not mutate its input", () => {
  const recent = ["a"] as const;
  expect(withRecentPath([], "x")).toEqual(["x"]);
  withRecentPath(recent, "z");
  expect(recent).toEqual(["a"]);
});
test("the list is capped like the settings parser caps it (10)", () => {
  const ten = Array.from({ length: 10 }, (_, i) => `p${i}`);
  const next = withRecentPath(ten, "new");
  expect(next).toHaveLength(10);
  expect(next[0]).toBe("new");
  expect(next).not.toContain("p9");
});
