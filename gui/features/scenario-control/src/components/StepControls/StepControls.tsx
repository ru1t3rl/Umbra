import { useStepRun } from "@umbra/data";
import { useRunStatus, useStreamStatus } from "@umbra/engine-stream";
import { Button, TextField } from "@umbra/ui-kit";
import { type ReactElement, useId, useState } from "react";
import { useCommandProblem } from "../../hooks/useCommandProblem.ts";
import { t } from "../../messages.ts";
import { parseStepCount, STEP_PRESETS } from "../../model/parseStepCount.ts";
import { problemText } from "../../model/problemText.ts";
import { availableCommands, disabledReason } from "../../model/runCommands.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./StepControls.css";

/**
 * The step presets, the "Tick count" field and "Step", for a running stepped clock (ASM-810); nothing otherwise.
 * `RunControls` shows it in its panel layout; a shell with a bar puts it elsewhere (a clock panel). A failed step
 * shows the engine's detail in an Alert beside the group.
 */
export function StepControls(): ReactElement | null {
  const run = useRunStatus();
  const streamOpen = useStreamStatus() === "open";
  const reasonId = useId();
  const { problem, report, shared } = useCommandProblem();
  const [countText, setCountText] = useState("1");
  const step = useStepRun();

  const visible = run?.state === "running" && run.clock.mode === "stepped";
  if (!visible) return null;

  const available = availableCommands(run, streamOpen);
  const reason = disabledReason(run, "step", streamOpen);
  const describedBy = reason ? { "aria-describedby": reasonId } : {};
  const off = !available.step || step.isPending;
  const count = parseStepCount(countText);
  const send = (n: number): void => {
    report(null);
    step.mutate(
      { data: { count: n } },
      { onError: (error: unknown) => report(problemText(error)) },
    );
  };

  return (
    <div className="umbra-step-controls">
      {/* biome-ignore lint/a11y/useSemanticElements: a div with role=group, like the ui-kit ChipGroup (ASM-335) */}
      <div className="umbra-step-controls-row" role="group" aria-label={t("step.label")}>
        {STEP_PRESETS.map((n) => (
          <Button
            key={n}
            size="sm"
            disabled={off}
            {...describedBy}
            aria-label={t(n === 1 ? "step.preset.one" : "step.preset.many", { count: n })}
            onClick={() => send(n)}
          >
            {String(n)}
          </Button>
        ))}
        <TextField
          label={t("step.count")}
          help={t("step.countHelp")}
          mono
          inputMode="numeric"
          value={countText}
          invalid={count === null}
          onChange={(e) => setCountText(e.target.value)}
        />
        <Button
          size="sm"
          disabled={off || count === null}
          {...describedBy}
          onClick={() => count !== null && send(count)}
        >
          {t("cmd.step")}
        </Button>
        {reason ? (
          <span hidden id={reasonId}>
            {reason}
          </span>
        ) : null}
      </div>
      {problem && !shared ? <ProblemAlert problem={problem} /> : null}
    </div>
  );
}
