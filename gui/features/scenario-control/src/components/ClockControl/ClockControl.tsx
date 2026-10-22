import { type ClockSetting, useSetClock } from "@umbra/data";
import { useRunStatus, useStreamStatus } from "@umbra/engine-stream";
import { Chip, ChipGroup } from "@umbra/ui-kit";
import { type ReactElement, useId, useState } from "react";
import { t } from "../../messages.ts";
import { clockLabel } from "../../model/clockLabel.ts";
import { type ProblemText, problemText } from "../../model/problemText.ts";
import { availableCommands, disabledReason } from "../../model/runCommands.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import { ScaleEditor } from "../ScaleEditor/ScaleEditor.tsx";
import "./ClockControl.css";

type Mode = "afap" | "realtime" | "stepped" | "scaled";
const MODES: readonly Mode[] = ["afap", "realtime", "stepped", "scaled"];

const MODE_CLOCK: Record<Exclude<Mode, "scaled">, ClockSetting> = {
  afap: { mode: "afap" },
  realtime: { mode: "realtime" },
  stepped: { mode: "stepped" },
};

/**
 * The clock mode chips (sent at once) and, for a scaled clock, the logarithmic scale slider with a numeric field
 * for the full contract range (ASM-811). Off in idle and stopped (the engine would answer invalid_state).
 */
export function ClockControl(): ReactElement {
  const run = useRunStatus();
  const streamOpen = useStreamStatus() === "open";
  const reasonId = useId();
  const [problem, setProblem] = useState<ProblemText | null>(null);
  const setClock = useSetClock();
  const reason = disabledReason(run, "setClock", streamOpen);
  const disabled = !availableCommands(run, streamOpen).setClock || setClock.isPending;

  const send = (data: ClockSetting): void => {
    setProblem(null);
    setClock.mutate({ data }, { onError: (error: unknown) => setProblem(problemText(error)) });
  };
  const choose = (mode: Mode): void => {
    if (run?.clock.mode === mode) return;
    send(mode === "scaled" ? { mode, scale: 1 } : MODE_CLOCK[mode]);
  };

  return (
    <div className="umbra-clock-control">
      {run ? (
        <p className="umbra-clock-control-status">
          {t("clock.status", { label: clockLabel(run.clock) })}
        </p>
      ) : null}
      <ChipGroup label={t("clock.mode")} {...(reason ? { "aria-describedby": reasonId } : {})}>
        {MODES.map((mode) => (
          <Chip
            key={mode}
            selected={run?.clock.mode === mode}
            disabled={disabled}
            onClick={() => choose(mode)}
          >
            {t(`clock.${mode}`)}
          </Chip>
        ))}
      </ChipGroup>
      {reason ? (
        <p className="umbra-clock-control-status" id={reasonId}>
          {reason}
        </p>
      ) : null}
      {run?.clock.mode === "scaled" ? (
        <ScaleEditor
          key={run.clock.scale}
          current={run.clock.scale}
          disabled={disabled}
          onApply={(scale) => send({ mode: "scaled", scale })}
        />
      ) : null}
      {problem ? <ProblemAlert problem={problem} /> : null}
    </div>
  );
}
