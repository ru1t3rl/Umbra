import { createContext } from "react";
import type { ProblemText } from "../model/problemText.ts";

export interface CommandProblemValue {
  /** The failed command's problem, or null. One slot for every command of the provider's subtree. */
  problem: ProblemText | null;
  report(problem: ProblemText | null): void;
}

/** Null outside a `CommandProblemProvider`: each component then keeps its own problem. */
export const CommandProblemContext = createContext<CommandProblemValue | null>(null);
