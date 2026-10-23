import { type ReactElement, useContext } from "react";
import { CommandProblemContext } from "../../context/CommandProblemContext.ts";
import { CommandProblemProvider } from "../CommandProblemProvider/CommandProblemProvider.tsx";
import { RunControlsBody, type RunControlsProps } from "../RunControlsBody/RunControlsBody.tsx";
import "./RunControls.css";

export type { RunControlsProps };

/**
 * Load, Start, Pause, Resume, Stop and Unload, plus the step controls for a running stepped clock. Every
 * button follows the run state of the stream (the only truth, no optimistic update, ASM-907); the one azure
 * button is the next lifecycle step (ASM-809). A failed command shows the engine's detail until the next one.
 */
export function RunControls(props: RunControlsProps): ReactElement {
  const shared = useContext(CommandProblemContext) !== null;
  return shared ? (
    <RunControlsBody {...props} />
  ) : (
    <CommandProblemProvider>
      <RunControlsBody {...props} />
    </CommandProblemProvider>
  );
}
