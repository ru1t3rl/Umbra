import { type ReactElement, type ReactNode, useMemo, useState } from "react";
import { CommandProblemContext } from "../../context/CommandProblemContext.ts";
import type { ProblemText } from "../../model/problemText.ts";

/**
 * One slot for the failed command's problem, shared by `RunControls` and `StepControls` below it: a new command of
 * either kind clears the error of the other ("until the next command"), and `RunControls` shows it once. A shell with
 * a bar and a separate step group wraps both in it; `RunControls` in its panel layout adds one itself.
 */
export function CommandProblemProvider({ children }: { children?: ReactNode }): ReactElement {
  const [problem, report] = useState<ProblemText | null>(null);
  const value = useMemo(() => ({ problem, report }), [problem]);
  return <CommandProblemContext value={value}>{children}</CommandProblemContext>;
}
