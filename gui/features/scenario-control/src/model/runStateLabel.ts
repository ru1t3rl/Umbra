import type { RunStatus } from "@umbra/engine-stream";
import type { Tone } from "@umbra/ui-kit";
import { t } from "../messages.ts";

export interface RunStateLabel {
  label: string;
  tone: Tone;
  /** Only a running scenario gets the cyan live value. */
  live: boolean;
}

/**
 * The run-state Badge word and tone (ASM-319). Stopped is neutral only for no reason, `command` or
 * `duration` and no fault, anything else is danger; `"unknown"` (a state this client does not know, ASM-359) is a neutral "Unknown".
 */
export function runStateLabel(
  state: RunStatus["state"],
  stopReason?: RunStatus["stop_reason"],
  fault?: RunStatus["fault"],
): RunStateLabel {
  switch (state) {
    case "idle":
      return { label: t("run.idle"), tone: "neutral", live: false };
    case "ready":
      return { label: t("run.ready"), tone: "info", live: false };
    case "running":
      return { label: t("run.running"), tone: "success", live: true };
    case "paused":
      return { label: t("run.paused"), tone: "warning", live: false };
    case "stopped":
      return {
        label: t("run.stopped"),
        tone: stoppedBenignly(stopReason, fault) ? "neutral" : "danger",
        live: false,
      };
    case "unknown":
      return { label: t("run.unknown"), tone: "neutral", live: false };
    default: {
      const unhandled: never = state;
      void unhandled; // a state added to the contract: fall back to "Unknown"
      return { label: t("run.unknown"), tone: "neutral", live: false };
    }
  }
}

/** Allowlist: only these reasons are benign; a fault, an error, "unknown" and any future reason are danger. */
function stoppedBenignly(stopReason: RunStatus["stop_reason"], fault: RunStatus["fault"]): boolean {
  if (fault) return false;
  return (
    stopReason === undefined ||
    stopReason === null ||
    stopReason === "command" ||
    stopReason === "duration"
  );
}
