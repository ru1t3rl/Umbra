import { useContext, useState } from "react";
import { CommandProblemContext } from "../context/CommandProblemContext.ts";
import type { ProblemText } from "../model/problemText.ts";

export interface CommandProblem {
  problem: ProblemText | null;
  report(problem: ProblemText | null): void;
  /** True inside a `CommandProblemProvider`: the problem is shown by `RunControls`, not by the reporter. */
  shared: boolean;
}

/** The shared problem slot when there is a provider, else a private one. */
export function useCommandProblem(): CommandProblem {
  const shared = useContext(CommandProblemContext);
  const [own, setOwn] = useState<ProblemText | null>(null);
  return shared
    ? { problem: shared.problem, report: shared.report, shared: true }
    : { problem: own, report: setOwn, shared: false };
}
