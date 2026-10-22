import { expect, test } from "bun:test";
import { DEFAULT_JOURNAL_CAPACITY } from "@umbra/engine-stream";
import { DEFAULT_SETTINGS } from "@umbra/settings";

test("the settings default journal capacity equals engine-stream's default (ADR 0071)", () => {
  // Mutation caught: the two defaults drifting apart.
  expect(DEFAULT_SETTINGS.journal.capacity).toBe(DEFAULT_JOURNAL_CAPACITY);
});
