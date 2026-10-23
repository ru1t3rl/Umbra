import { type ReactElement, useContext } from "react";
import { CommandProblemContext } from "../../context/CommandProblemContext.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";

/**
 * Shows the failed command's problem of the enclosing `CommandProblemProvider`; nothing without a provider or a
 * problem. `RunControls` already shows it, so this is for a provider that holds only `StepControls` (or a shell
 * that wants the Alert somewhere else than a `problemContainer`).
 */
export function CommandProblemAlert(): ReactElement | null {
  const shared = useContext(CommandProblemContext);
  return shared?.problem ? <ProblemAlert problem={shared.problem} /> : null;
}
