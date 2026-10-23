import { isApiProblem, useRemoveEntity } from "@umbra/data";
import { Alert, Button } from "@umbra/ui-kit";
import { type ReactElement, useRef } from "react";
import { t } from "../../messages.ts";

export interface RemoveEntityControlProps {
  id: number;
  /** The entity left the simulation: nothing to remove. */
  disabled: boolean;
}

/** Remove (danger) with an in-flight guard (one control per selected id: the caller keys it by id); a problem from the engine (for example 409 `entity_remote`) shows inline. */
export function RemoveEntityControl({ id, disabled }: RemoveEntityControlProps): ReactElement {
  const remove = useRemoveEntity();
  // A ref, not the rendered pending flag: two clicks inside one act see the same render.
  const inFlight = useRef(false);
  const error: unknown = remove.error;
  return (
    <>
      <Button
        variant="danger"
        size="sm"
        disabled={disabled || remove.isPending}
        onClick={() => {
          if (inFlight.current) return;
          inFlight.current = true;
          remove.mutate(
            { id },
            {
              onSettled: () => {
                inFlight.current = false;
              },
            },
          );
        }}
      >
        {t("inspector.remove")}
      </Button>
      {remove.isError ? (
        <Alert
          tone="danger"
          title={isApiProblem(error) ? error.title : t("inspector.removeFailed")}
        >
          {isApiProblem(error) ? error.detail : error instanceof Error ? error.message : ""}
        </Alert>
      ) : null}
    </>
  );
}
