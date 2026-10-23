import { useRunStatus } from "@umbra/engine-stream";
import { Badge } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { useAdaptersQuery } from "../../hooks/useAdaptersQuery.ts";
import { usePluginsQuery } from "../../hooks/usePluginsQuery.ts";
import { t } from "../../messages.ts";
import { faultRows, faultTypeLabel } from "../../model/faultRows.ts";
import { IdleState } from "../IdleState/IdleState.tsx";
import { LoadingState } from "../LoadingState/LoadingState.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./FaultSummary.css";

/**
 * Every fault of the engine in one place: plugin, network, faulted capabilities and adapters (M8 WI-13). It shows
 * what it has: the stream's faults at once, the REST ones as their reads arrive; a failed read is an alert above
 * the rows. "No faults" needs both reads to have succeeded (ASM-967).
 */
export function FaultSummary(): ReactElement {
  const run = useRunStatus();
  const plugins = usePluginsQuery();
  const adapters = useAdaptersQuery();
  if (plugins.idle) return <IdleState />;
  const pluginsFailed = plugins.query.isError;
  const adaptersFailed = adapters.query.isError && !adapters.noScenario;
  const pluginData = plugins.query.data;
  // no_scenario is an empty list, not a failure (ASM-958)
  const adapterData = adapters.noScenario ? [] : adapters.query.data;
  const rows = faultRows(pluginData, run, adapterData);
  const succeeded =
    pluginData !== undefined && !pluginsFailed && adapterData !== undefined && !adaptersFailed;
  return (
    <>
      {pluginsFailed ? <ProblemAlert error={plugins.query.error} failed="faults.failed" /> : null}
      {adaptersFailed ? <ProblemAlert error={adapters.query.error} failed="faults.failed" /> : null}
      {rows.length > 0 ? (
        <ul className="umbra-fault-list" aria-label={t("faults.label")}>
          {rows.map((row) => (
            <li key={row.key} className="umbra-fault-row type-body">
              <Badge tone="danger">{faultTypeLabel(row.type)}</Badge>
              <span className="umbra-fault-subject">{row.subject}</span>
              {row.detail ? <span className="umbra-fault-detail">{row.detail}</span> : null}
            </li>
          ))}
        </ul>
      ) : succeeded ? (
        <Badge tone="success">{t("faults.none")}</Badge>
      ) : pluginsFailed || adaptersFailed ? null : (
        <LoadingState />
      )}
    </>
  );
}
