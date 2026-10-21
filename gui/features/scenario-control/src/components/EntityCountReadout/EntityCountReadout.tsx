import { Readout } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { useEntityCount } from "../../hooks/useEntityCount.ts";
import { t } from "../../messages.ts";

/** Its own component so a frame that changes only the run does not re-render it, and vice versa. */
export function EntityCountReadout(): ReactElement {
  const count = useEntityCount();
  return (
    <Readout
      label={t("readout.entities")}
      value={count === undefined ? t("readout.none") : count}
    />
  );
}
