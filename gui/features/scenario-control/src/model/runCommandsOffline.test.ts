// WI-7 review: a stale run status must not enable commands while the stream is not open (ASM-504).
import { expect, test } from "bun:test";
import {
  availableCommands,
  disabledReason,
  primaryCommand,
  RUN_COMMANDS,
  type RunLike,
} from "./runCommands.ts";

const running = { state: "running", clock: { mode: "stepped" }, step_pending: false } as RunLike;

// Catches: a command enabled from the last status after the connection dropped.
test("with a closed stream every command is off, with the offline reason", () => {
  for (const c of RUN_COMMANDS) {
    expect(availableCommands(running, false)[c]).toBe(false);
    expect(disabledReason(running, c, false)).toBe("Waiting for the engine connection.");
  }
});

// Catches: the default changing (the one-argument form is the open stream).
test("an open stream is the default", () => {
  expect(availableCommands(running).pause).toBe(true);
  expect(availableCommands(running, true).pause).toBe(true);
  expect(disabledReason(running, "pause", true)).toBeNull();
});

// Catches: an azure button with nothing to run offline is fine (primary is about state), but must never be enabled.
test("the primary command is unchanged but unavailable offline", () => {
  const p = primaryCommand(running);
  expect(p).toBe("pause");
  expect(p && availableCommands(running, false)[p]).toBe(false);
});
