import { useRunStatus, useStreamStatus } from "@umbra/engine-stream";
import { Badge, Readout } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { formatSimTime } from "../../model/formatSimTime.ts";
import { runStateLabel } from "../../model/runStateLabel.ts";
import { EntityCountReadout } from "../EntityCountReadout/EntityCountReadout.tsx";

/**
 * Read-only run status as `CommandBar` children (a fragment the shell drops into the bar): sim time, tick,
 * entity count and the run-state Badge. No controls here (M8).
 */
export function RunStatusBar(): ReactElement {
  const run = useRunStatus();
  const open = useStreamStatus() === "open";
  const state = run ? runStateLabel(run.state, run.stop_reason, run.fault) : undefined;
  return (
    <>
      <Readout
        label={t("readout.simTime")}
        value={formatSimTime(run?.time_s)}
        live={open && state?.live === true}
      />
      <Readout label={t("readout.tick")} value={run?.tick ?? t("readout.none")} />
      <EntityCountReadout />
      {state ? (
        <span role="status">
          <span className="vt-visually-hidden">{t("run.label")} </span>
          <Badge tone={state.tone}>{state.label}</Badge>
        </span>
      ) : null}
    </>
  );
}
