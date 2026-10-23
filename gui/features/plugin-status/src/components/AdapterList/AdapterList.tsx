import { Badge, Card, Table, type TableColumn, Toggle } from "@umbra/ui-kit";
import { type ReactElement, useId } from "react";
import { useAdapterSwitch } from "../../hooks/useAdapterSwitch.ts";
import { useAdaptersQuery } from "../../hooks/useAdaptersQuery.ts";
import { t } from "../../messages.ts";
import { type CounterRow, counterRows } from "../../model/counterRows.ts";
import { IdleState } from "../IdleState/IdleState.tsx";
import { LoadingState } from "../LoadingState/LoadingState.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./AdapterList.css";

const columns: readonly TableColumn<CounterRow>[] = [
  { key: "label", label: t("adapters.colCounter") },
  { key: "value", label: t("adapters.colValue"), mono: true, align: "end" },
];

/** The network adapters of the loaded scenario with an enable switch and the live counters (M8 WI-13). */
export function AdapterList(): ReactElement | null {
  const { idle, query, noScenario } = useAdaptersQuery();
  const switcher = useAdapterSwitch(query.refetch);
  const idPrefix = useId();
  if (idle || noScenario) return <IdleState />;
  const alert = query.isError ? (
    <ProblemAlert error={query.error} failed="adapters.failed" />
  ) : null;
  if (query.data === undefined) return alert ?? <LoadingState />;
  const adapters = query.data;
  if (adapters.length === 0)
    return (
      <>
        {alert}
        <p className="umbra-adapter-empty type-body">{t("adapters.empty")}</p>
      </>
    );
  return (
    <div className="umbra-adapter-list">
      {alert}
      {adapters.map((adapter, index) => {
        const failure = switcher.errors.get(adapter.name);
        const reasonId =
          adapter.faulted && adapter.fault_reason ? `${idPrefix}-reason-${index}` : undefined;
        return (
          <Card key={adapter.name} title={adapter.name}>
            <div className="umbra-adapter-head">
              <Toggle
                label={t("adapters.enabled")}
                aria-label={t("adapters.enabledName", { name: adapter.name })}
                checked={adapter.enabled}
                disabled={adapter.faulted || switcher.busy.has(adapter.name)}
                {...(reasonId ? { "aria-describedby": reasonId } : {})}
                onChange={(next) => void switcher.toggle(adapter.name, next)}
              />
              {adapter.faulted ? <Badge tone="danger">{t("status.faulted")}</Badge> : null}
            </div>
            {adapter.faulted && adapter.fault_reason ? (
              <p id={reasonId} className="umbra-adapter-reason type-body">
                {adapter.fault_reason}
              </p>
            ) : null}
            {switcher.errors.has(adapter.name) ? (
              <ProblemAlert error={failure} failed="adapters.switchFailed" />
            ) : null}
            <dl className="umbra-adapter-facts type-body">
              <dt>{t("adapters.adapter")}</dt>
              <dd>{adapter.adapter}</dd>
              <dt>{t("adapters.plugin")}</dt>
              <dd>{adapter.plugin ?? t("adapters.builtIn")}</dd>
              <dt>{t("adapters.ghosts")}</dt>
              <dd>{adapter.ghosts}</dd>
              <dt>{t("adapters.keySpace")}</dt>
              <dd>
                {t("adapters.keySpaceValue", {
                  base: adapter.key_space.base,
                  count: adapter.key_space.count,
                })}
              </dd>
            </dl>
            <Table
              caption={t("adapters.counters", { name: adapter.name })}
              columns={columns}
              rows={counterRows(adapter.counters)}
              rowKey={(row) => row.key}
              dense
            />
          </Card>
        );
      })}
    </div>
  );
}
