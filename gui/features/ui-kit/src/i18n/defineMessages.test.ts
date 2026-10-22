import { describe, expect, test } from "bun:test";
import { defineMessages } from "./defineMessages.ts";

const t = defineMessages({
  hello: "Hello {name}",
  count: "{n} of {total} selected, {n} again",
  plain: "Plain",
});

describe("defineMessages", () => {
  test("returns the English text", () => {
    expect(t("plain")).toBe("Plain");
  });
  test("interpolates string and number params, every occurrence", () => {
    expect(t("hello", { name: "Ada" })).toBe("Hello Ada");
    expect(t("count", { n: 2, total: 5 })).toBe("2 of 5 selected, 2 again");
  });
  test("an unknown param is left visible", () => {
    expect(t("hello")).toBe("Hello {name}");
    expect(t("hello", { other: "x" })).toBe("Hello {name}");
  });
  test("a param value containing a placeholder is not re-expanded", () => {
    expect(t("hello", { name: "{name}" })).toBe("Hello {name}");
  });
  test("replacement patterns in a param stay literal", () => {
    expect(t("hello", { name: "$&" })).toBe("Hello $&");
  });
  test("an inherited property name returns the key", () => {
    expect(t("constructor" as "plain")).toBe("constructor");
  });
});
