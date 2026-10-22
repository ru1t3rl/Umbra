// Which run commands the engine accepts in which state: mirrors docs/api.md "Commands and states" (ADR 0026,
// ASM-809). The server stays the authority (it answers 409); this only keeps the buttons honest.
import type { RunStatus } from "@umbra/engine-stream";
import { t } from "../messages.ts";

export type RunCommand =
  | "load"
  | "unload"
  | "start"
  | "pause"
  | "resume"
  | "stop"
  | "step"
  | "setClock";

/** Display order. */
export const RUN_COMMANDS: readonly RunCommand[] = [
  "load",
  "start",
  "pause",
  "resume",
  "stop",
  "unload",
  "step",
  "setClock",
];

export type RunLike = Pick<RunStatus, "state" | "clock" | "step_pending">;

type KnownState = "idle" | "ready" | "running" | "paused" | "stopped";
const known = (state: RunStatus["state"]): state is KnownState => state !== "unknown";

const IN_STATES: Record<Exclude<RunCommand, "step">, readonly KnownState[]> = {
  load: ["idle", "ready", "running", "paused", "stopped"],
  unload: ["ready", "running", "paused", "stopped"],
  start: ["ready"],
  pause: ["running"],
  resume: ["paused"],
  stop: ["ready", "running", "paused"],
  setClock: ["ready", "running", "paused"],
};

function allowed(run: RunLike | undefined, command: RunCommand, streamOpen: boolean): boolean {
  if (!streamOpen || !run || !known(run.state)) return false;
  if (command === "step") {
    return run.state === "running" && run.clock.mode === "stepped" && !run.step_pending;
  }
  return IN_STATES[command].includes(run.state);
}

/**
 * `streamOpen` is false while the stream is connecting, reconnecting, closed or incompatible: the last run status
 * is then stale and no command is offered (same rule as the RunStatusBar live flag, ASM-504).
 */
export function availableCommands(
  run: RunLike | undefined,
  streamOpen = true,
): Record<RunCommand, boolean> {
  return {
    load: allowed(run, "load", streamOpen),
    unload: allowed(run, "unload", streamOpen),
    start: allowed(run, "start", streamOpen),
    pause: allowed(run, "pause", streamOpen),
    resume: allowed(run, "resume", streamOpen),
    stop: allowed(run, "stop", streamOpen),
    step: allowed(run, "step", streamOpen),
    setClock: allowed(run, "setClock", streamOpen),
  };
}

/** The one filled button: the next lifecycle step. */
export function primaryCommand(run: RunLike | undefined): RunCommand | null {
  if (!run) return null;
  switch (run.state) {
    case "idle":
    case "stopped":
      return "load";
    case "ready":
      return "start";
    case "running":
      return "pause";
    case "paused":
      return "resume";
    default:
      return null;
  }
}

const NEEDED_STATE: Partial<Record<RunCommand, KnownState>> = {
  start: "ready",
  pause: "running",
  resume: "paused",
  step: "running",
};
const STATE_WORD: Record<KnownState, Parameters<typeof t>[0]> = {
  idle: "run.idle",
  ready: "run.ready",
  running: "run.running",
  paused: "run.paused",
  stopped: "run.stopped",
};

/** Why a command is off; null exactly when it is available. */
export function disabledReason(
  run: RunLike | undefined,
  command: RunCommand,
  streamOpen = true,
): string | null {
  if (allowed(run, command, streamOpen)) return null;
  if (!streamOpen) return t("disabled.offline");
  if (!run) return t("disabled.waiting");
  if (!known(run.state)) return t("disabled.unknownState");
  if (run.state === "idle") return t("disabled.noScenario");
  if (run.state === "stopped") return t("disabled.stopped");
  if (command === "step" && run.state === "running") {
    return run.clock.mode !== "stepped" ? t("disabled.needsStepped") : t("disabled.stepRunning");
  }
  const needed = NEEDED_STATE[command];
  return t("disabled.needs", {
    command: t(`cmd.${command}`),
    state: t(STATE_WORD[needed ?? "running"]).toLowerCase(),
  });
}
