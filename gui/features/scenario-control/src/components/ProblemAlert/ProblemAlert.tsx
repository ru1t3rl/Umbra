import { Alert } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import type { ProblemText } from "../../model/problemText.ts";

/** The failed command: the engine's title and detail in a danger Alert (role alert). */
export function ProblemAlert({ problem }: { problem: ProblemText }): ReactElement {
  return (
    <Alert tone="danger" title={problem.title}>
      {problem.detail}
    </Alert>
  );
}
