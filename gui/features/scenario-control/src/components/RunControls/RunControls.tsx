import {
  usePauseRun,
  useResumeRun,
  useStartRun,
  useStepRun,
  useStopRun,
  useUnloadScenario,
} from "@umbra/data";
import { useRunStatus, useStreamStatus } from "@umbra/engine-stream";
import { Button, TextField } from "@umbra/ui-kit";
import { type ReactElement, useId, useState } from "react";
import { t } from "../../messages.ts";
import { parseStepCount, STEP_PRESETS } from "../../model/parseStepCount.ts";
import { type ProblemText, problemText } from "../../model/problemText.ts";
import {
  availableCommands,
  disabledReason,
  primaryCommand,
  type RunCommand,
} from "../../model/runCommands.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./RunControls.css";

export interface RunControlsProps {
  /** Load only asks the shell to open the loader (WI-8); it sends nothing itself. */
  onLoadRequested?: () => void;
}

/**
 * Load, Start, Pause, Resume, Stop and Unload, plus the step controls for a running stepped clock. Every
 * button follows the run state of the stream (the only truth, no optimistic update, ASM-907); the one azure
 * button is the next lifecycle step (ASM-809). A failed command shows the engine's detail until the next one.
 */
export function RunControls({ onLoadRequested }: RunControlsProps): ReactElement {
  const run = useRunStatus();
  const streamOpen = useStreamStatus() === "open";
  const idBase = useId();
  const [problem, setProblem] = useState<ProblemText | null>(null);
  const [countText, setCountText] = useState("1");
  const start = useStartRun();
  const pause = usePauseRun();
  const resume = useResumeRun();
  const stop = useStopRun();
  const unload = useUnloadScenario();
  const step = useStepRun();

  const available = availableCommands(run, streamOpen);
  const primary = primaryCommand(run);
  const pending: Partial<Record<RunCommand, boolean>> = {
    start: start.isPending,
    pause: pause.isPending,
    resume: resume.isPending,
    stop: stop.isPending,
    unload: unload.isPending,
  };
  const handlers = { onError: (error: unknown) => setProblem(problemText(error)) };
  const send: Partial<Record<RunCommand, () => void>> = {
    load: () => onLoadRequested?.(),
    start: () => start.mutate(undefined, handlers),
    pause: () => pause.mutate(undefined, handlers),
    resume: () => resume.mutate(undefined, handlers),
    stop: () => stop.mutate(undefined, handlers),
    unload: () => unload.mutate(undefined, handlers),
  };
  const sendStep = (count: number): void => {
    setProblem(null);
    step.mutate({ data: { count } }, handlers);
  };

  const reasonId = (command: RunCommand): string => `${idBase}-${command}-why`;
  const reasonOf = (command: RunCommand): string | null => disabledReason(run, command, streamOpen);
  const reason = (command: RunCommand): ReactElement | null => {
    const text = reasonOf(command);
    return text ? (
      <span hidden id={reasonId(command)}>
        {text}
      </span>
    ) : null;
  };

  const commandButton = (command: Exclude<RunCommand, "step" | "setClock">): ReactElement => {
    const off = !available[command] || pending[command] === true;
    return (
      <>
        <Button
          variant={
            primary === command
              ? "filled"
              : command === "stop" || command === "unload"
                ? "danger"
                : "tonal"
          }
          disabled={off}
          aria-describedby={reasonOf(command) ? reasonId(command) : undefined}
          onClick={() => {
            setProblem(null);
            send[command]?.();
          }}
        >
          {t(`cmd.${command}`)}
        </Button>
        {reason(command)}
      </>
    );
  };

  const stepReason = disabledReason(run, "step", streamOpen);
  const stepReasonId = `${idBase}-step-why`;
  const stepDescribedBy = stepReason ? { "aria-describedby": stepReasonId } : {};
  const stepVisible = run?.state === "running" && run.clock.mode === "stepped";
  const stepOff = !available.step || step.isPending;
  const count = parseStepCount(countText);

  return (
    <div className="umbra-run-controls">
      {/* biome-ignore lint/a11y/useSemanticElements: a div with role=group, like the ui-kit ChipGroup (ASM-335) */}
      <div className="umbra-run-controls-row" role="group" aria-label={t("controls.label")}>
        {commandButton("load")}
        {commandButton("start")}
        {commandButton("pause")}
        {commandButton("resume")}
        {commandButton("stop")}
        {commandButton("unload")}
      </div>
      {stepVisible ? (
        // biome-ignore lint/a11y/useSemanticElements: see above
        <div className="umbra-run-controls-row" role="group" aria-label={t("step.label")}>
          {STEP_PRESETS.map((n) => (
            <Button
              key={n}
              size="sm"
              disabled={stepOff}
              {...stepDescribedBy}
              aria-label={t(n === 1 ? "step.preset.one" : "step.preset.many", { count: n })}
              onClick={() => sendStep(n)}
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
            disabled={stepOff || count === null}
            {...stepDescribedBy}
            onClick={() => count !== null && sendStep(count)}
          >
            {t("cmd.step")}
          </Button>
          {stepReason ? (
            <span hidden id={stepReasonId}>
              {stepReason}
            </span>
          ) : null}
        </div>
      ) : null}
      {problem ? <ProblemAlert problem={problem} /> : null}
    </div>
  );
}
