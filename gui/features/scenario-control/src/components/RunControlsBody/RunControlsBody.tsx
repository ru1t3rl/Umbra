import { usePauseRun, useResumeRun, useStartRun, useStopRun, useUnloadScenario } from "@umbra/data";
import { useRunStatus, useStreamStatus } from "@umbra/engine-stream";
import { Button } from "@umbra/ui-kit";
import { type ReactElement, useId } from "react";
import { createPortal } from "react-dom";
import { useCommandProblem } from "../../hooks/useCommandProblem.ts";
import { t } from "../../messages.ts";
import { problemText } from "../../model/problemText.ts";
import {
  availableCommands,
  disabledReason,
  primaryCommand,
  type RunCommand,
} from "../../model/runCommands.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import { StepControls } from "../StepControls/StepControls.tsx";

export interface RunControlsProps {
  /** Load only asks the shell to open the loader (WI-8); it sends nothing itself. */
  onLoadRequested?: () => void;
  /**
   * `panel` (default): the lifecycle row, the step group for a running stepped clock and the command problem, in a
   * column. `bar`: only the lifecycle row, for a one-line command bar; the step group is `StepControls` (put it in a
   * panel) and the command problem is rendered into `problemContainer` (inline when it is not given).
   */
  layout?: "panel" | "bar";
  /** Where the `bar` layout renders the failed command's Alert (a shell's alert column). */
  problemContainer?: HTMLElement | null;
  /**
   * `bar` layout only, the second compact step: every button is an icon with an accessible name and a `title`
   * (for a bar that is still too narrow with labels). The panel layout ignores it.
   */
  iconOnly?: boolean;
}

const ICONS: Record<"load" | "start" | "pause" | "resume" | "stop" | "unload", string> = {
  load: "folder_open",
  start: "play_arrow",
  pause: "pause",
  resume: "play_circle",
  stop: "stop",
  unload: "eject",
};

/** The lifecycle row and, per layout, the step group and the command problem. See `RunControls`. */
export function RunControlsBody({
  onLoadRequested,
  layout = "panel",
  problemContainer = null,
  iconOnly = false,
}: RunControlsProps): ReactElement {
  const run = useRunStatus();
  const streamOpen = useStreamStatus() === "open";
  const idBase = useId();
  const { problem, report } = useCommandProblem();
  const start = useStartRun();
  const pause = usePauseRun();
  const resume = useResumeRun();
  const stop = useStopRun();
  const unload = useUnloadScenario();

  const bar = layout === "bar";
  const available = availableCommands(run, streamOpen);
  const primary = primaryCommand(run);
  const pending: Partial<Record<RunCommand, boolean>> = {
    start: start.isPending,
    pause: pause.isPending,
    resume: resume.isPending,
    stop: stop.isPending,
    unload: unload.isPending,
  };
  const handlers = { onError: (error: unknown) => report(problemText(error)) };
  const send: Partial<Record<RunCommand, () => void>> = {
    load: () => onLoadRequested?.(),
    start: () => start.mutate(undefined, handlers),
    pause: () => pause.mutate(undefined, handlers),
    resume: () => resume.mutate(undefined, handlers),
    stop: () => stop.mutate(undefined, handlers),
    unload: () => unload.mutate(undefined, handlers),
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
    const shared = {
      variant:
        primary === command
          ? "filled"
          : command === "stop" || command === "unload"
            ? "danger"
            : "tonal",
      disabled: off,
      "aria-describedby": reasonOf(command) ? reasonId(command) : undefined,
      onClick: () => {
        report(null);
        send[command]?.();
      },
    } as const;
    return (
      <>
        {bar && iconOnly ? (
          <Button
            {...shared}
            icon={ICONS[command]}
            aria-label={t(`cmd.${command}`)}
            title={t(`cmd.${command}`)}
          />
        ) : (
          <Button {...shared}>{t(`cmd.${command}`)}</Button>
        )}
        {reason(command)}
      </>
    );
  };

  return (
    <div className={bar ? "umbra-run-controls umbra-run-controls-bar" : "umbra-run-controls"}>
      {/* biome-ignore lint/a11y/useSemanticElements: a div with role=group, like the ui-kit ChipGroup (ASM-335) */}
      <div className="umbra-run-controls-row" role="group" aria-label={t("controls.label")}>
        {commandButton("load")}
        {commandButton("start")}
        {commandButton("pause")}
        {commandButton("resume")}
        {commandButton("stop")}
        {commandButton("unload")}
      </div>
      {bar ? null : <StepControls />}
      {problem && (!bar || !problemContainer) ? <ProblemAlert problem={problem} /> : null}
      {problem && bar && problemContainer
        ? createPortal(<ProblemAlert problem={problem} />, problemContainer)
        : null}
    </div>
  );
}
