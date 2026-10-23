import { isApiProblem } from "@umbra/data";
import { Alert } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { type MessageKey, t } from "../../messages.ts";

export interface ProblemAlertProps {
  /** The rejection of a request. */
  error: unknown;
  /** The message used as title when the error is not a problem document of the engine. */
  failed: MessageKey;
}

/** A failed request as a danger Alert: the engine's problem title and detail (docs/api.md "Errors"). */
export function ProblemAlert({ error, failed }: ProblemAlertProps): ReactElement {
  return (
    <Alert tone="danger" title={isApiProblem(error) ? error.title : t(failed)}>
      {isApiProblem(error) ? error.detail : error instanceof Error ? error.message : ""}
    </Alert>
  );
}
