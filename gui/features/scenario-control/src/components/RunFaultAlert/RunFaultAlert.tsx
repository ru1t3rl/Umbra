import { type RunStatus, useRunStatus } from "@umbra/engine-stream";
import { Alert } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import "./RunFaultAlert.css";

type Row = readonly [label: string, value: string];

interface Fault {
  title: string;
  rows: readonly Row[];
}

function faultOf(run: RunStatus): Fault | null {
  if (run.state !== "stopped") return null;
  switch (run.stop_reason) {
    case undefined:
    case null:
    case "command":
    case "duration":
      return null;
    case "plugin_fault": {
      const f = run.fault;
      return {
        title: t("fault.plugin.title"),
        rows: f
          ? [
              [t("fault.plugin"), f.plugin],
              [t("fault.capability"), f.capability],
              [t("fault.call"), f.call],
              [t("fault.message"), f.message],
            ]
          : [],
      };
    }
    case "network_fault": {
      const f = run.network_fault;
      return {
        title: t("fault.network.title"),
        rows: f
          ? [
              [t("fault.adapter"), f.adapter],
              [t("fault.reason"), f.reason],
            ]
          : [],
      };
    }
    case "error":
      return { title: t("fault.error.title"), rows: [] };
    default:
      return {
        title: t("fault.other.title", { word: run.stop_reason_raw ?? t("fault.unknownWord") }),
        rows: [],
      };
  }
}

/**
 * A danger Alert for a run that stopped because of a fault (plugin or network), an error or a reason this client
 * does not know (shown with the word as received). Nothing for a normal end or while the run is not stopped.
 */
export function RunFaultAlert(): ReactElement | null {
  const run = useRunStatus();
  const fault = run ? faultOf(run) : null;
  if (!fault) return null;
  return (
    <Alert tone="danger" title={fault.title}>
      {fault.rows.length > 0 ? (
        <dl className="umbra-fault-rows">
          {fault.rows.map(([label, value]) => (
            <div key={label} className="umbra-fault-row">
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
    </Alert>
  );
}
