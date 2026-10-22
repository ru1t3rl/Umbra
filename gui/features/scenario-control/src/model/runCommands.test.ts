// WI-7 / ASM-809: the table mirrors docs/api.md "Commands and states" (ADR 0026).
// Assumed API (model/runCommands.ts):
//   type RunCommand = "load" | "unload" | "start" | "pause" | "resume" | "stop" | "step" | "setClock"
//   type RunLike = { state: RunState; clock: Clock; step_pending: boolean }   // engine-stream RunStatus subset
//   RUN_COMMANDS: readonly RunCommand[]            (display order: load, start, pause, resume, stop, unload, step, setClock)
//   availableCommands(run: RunLike | undefined): Record<RunCommand, boolean>
//   primaryCommand(run: RunLike | undefined): RunCommand | null
//   disabledReason(run: RunLike | undefined, command: RunCommand): string | null   (null when enabled)
import { describe, expect, test } from "bun:test";
import {
  availableCommands,
  disabledReason,
  primaryCommand,
  RUN_COMMANDS,
  type RunCommand,
  type RunLike,
} from "./runCommands.ts";

const run = (state: string, clock: RunLike["clock"] = { mode: "realtime" }, step_pending = false) =>
  ({ state, clock, step_pending }) as RunLike;

const enabled = (r: RunLike | undefined): RunCommand[] =>
  RUN_COMMANDS.filter((c) => availableCommands(r)[c]);

describe("availableCommands (one row per state of the docs/api.md table)", () => {
  // Catches: a cell of the table flipped (for example start allowed while running).
  test.each([
    ["idle", ["load"]],
    ["ready", ["load", "start", "stop", "unload", "setClock"]],
    ["running", ["load", "pause", "stop", "unload", "setClock"]],
    ["paused", ["load", "resume", "stop", "unload", "setClock"]],
    ["stopped", ["load", "unload"]],
  ] as const)("%s", (state, expected) => {
    expect(new Set(enabled(run(state)))).toEqual(new Set(expected));
  });

  // Catches: step offered for a non-stepped clock (`not_stepped`), or while one is pending.
  test("step needs a running stepped clock with no step pending", () => {
    expect(availableCommands(run("running", { mode: "stepped" })).step).toBe(true);
    for (const clock of [{ mode: "afap" }, { mode: "realtime" }, { mode: "scaled", scale: 2 }]) {
      expect(availableCommands(run("running", clock as RunLike["clock"])).step).toBe(false);
    }
    expect(availableCommands(run("running", { mode: "stepped" }, true)).step).toBe(false);
  });

  // Catches: step enabled in ready or paused (invalid_state).
  test("step is off outside running even with a stepped clock", () => {
    for (const state of ["idle", "ready", "paused", "stopped"]) {
      expect(availableCommands(run(state, { mode: "stepped" })).step).toBe(false);
    }
  });

  // Catches: buttons live before the first status (a command could only 409 or fail to connect).
  test("no run status yet disables everything", () => {
    expect(enabled(undefined)).toEqual([]);
  });

  // Catches: guessing for a state of a newer server (ASM-359 maps it to "unknown").
  test("an unknown state disables everything", () => {
    expect(enabled(run("unknown"))).toEqual([]);
  });

  // Catches: the table keys drifting from the type (a command missing from the record).
  test("the record has a boolean for every command", () => {
    expect(Object.keys(availableCommands(run("idle"))).sort()).toEqual([...RUN_COMMANDS].sort());
    expect(new Set(RUN_COMMANDS).size).toBe(8);
  });
});

describe("primaryCommand (ASM-809: the next lifecycle step)", () => {
  // Catches: wrong azure button per state.
  test.each([
    ["idle", "load"],
    ["ready", "start"],
    ["running", "pause"],
    ["paused", "resume"],
    ["stopped", "load"],
  ] as const)("%s -> %s", (state, command) => {
    expect(primaryCommand(run(state))).toBe(command);
  });

  // Catches: a primary that is itself disabled.
  test("the primary command is always available", () => {
    for (const state of ["idle", "ready", "running", "paused", "stopped"]) {
      const c = primaryCommand(run(state));
      expect(c && availableCommands(run(state))[c]).toBe(true);
    }
  });

  // Catches: an azure button with nothing to run.
  test("none without a status or for an unknown state", () => {
    expect(primaryCommand(undefined)).toBeNull();
    expect(primaryCommand(run("unknown"))).toBeNull();
  });
});

describe("disabledReason", () => {
  // Catches: a reason for an enabled command, or a disabled command without one (a mute button).
  test("null exactly when the command is available", () => {
    const cases = [
      undefined,
      run("idle"),
      run("ready"),
      run("running"),
      run("running", { mode: "stepped" }),
      run("running", { mode: "stepped" }, true),
      run("paused"),
      run("stopped"),
      run("unknown"),
    ];
    for (const r of cases) {
      for (const c of RUN_COMMANDS) {
        const reason = disabledReason(r, c);
        if (availableCommands(r)[c]) expect(reason).toBeNull();
        else expect(reason).toBeString();
        if (reason !== null) expect(reason.length).toBeGreaterThan(3);
      }
    }
  });

  // Catches: one generic text for every cause.
  test("the reason names the cause", () => {
    expect(disabledReason(run("idle"), "start")).toBe("No scenario is loaded.");
    expect(disabledReason(undefined, "start")).toBe("Waiting for the engine.");
    expect(disabledReason(run("stopped"), "pause")).toBe(
      "The run has stopped. Load a scenario again.",
    );
    expect(disabledReason(run("running"), "start")).toBe("Start needs a ready scenario.");
    expect(disabledReason(run("ready"), "pause")).toBe("Pause needs a running scenario.");
    expect(disabledReason(run("running"), "resume")).toBe("Resume needs a paused scenario.");
    expect(disabledReason(run("running"), "step")).toBe("Step needs a stepped clock.");
    expect(disabledReason(run("running", { mode: "stepped" }, true), "step")).toBe(
      "A step is already running.",
    );
    expect(disabledReason(run("paused", { mode: "stepped" }), "step")).toBe(
      "Step needs a running scenario.",
    );
    expect(disabledReason(run("stopped"), "setClock")).toBe(
      "The run has stopped. Load a scenario again.",
    );
  });
});
