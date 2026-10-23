import { isApiProblem } from "@umbra/data";
import { Alert } from "@umbra/ui-kit";
import type { ReactElement } from "react";

export interface ProblemAlertProps {
  /** The rejection of a request. */
  error: unknown;
  /** The title when the error is not a problem document of the engine. */
  fallbackTitle: string;
}

/** A failed request as a danger Alert: the engine's problem title and detail (docs/api.md "Errors"). */
export function ProblemAlert({ error, fallbackTitle }: ProblemAlertProps): ReactElement {
  return (
    <Alert tone="danger" title={isApiProblem(error) ? error.title : fallbackTitle}>
      {isApiProblem(error) ? error.detail : error instanceof Error ? error.message : ""}
    </Alert>
  );
}
