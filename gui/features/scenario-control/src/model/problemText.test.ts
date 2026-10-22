import { expect, test } from "bun:test";
import { ApiProblem } from "@umbra/data";
import { problemText } from "./problemText.ts";

// Catches: the engine's detail replaced by a generic text, or a plain Error losing its message.
test("an ApiProblem gives its title and detail", () => {
  const p = new ApiProblem({
    code: "invalid_state",
    status: 409,
    title: "Invalid state",
    detail: "paused",
  });
  expect(problemText(p)).toEqual({ title: "Invalid state", detail: "paused" });
});

test("any other error gives a generic title and its message", () => {
  expect(problemText(new Error("boom"))).toEqual({ title: "Something went wrong", detail: "boom" });
  expect(problemText("x")).toEqual({ title: "Something went wrong", detail: "" });
});
