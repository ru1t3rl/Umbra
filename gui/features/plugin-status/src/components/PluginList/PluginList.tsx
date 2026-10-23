import { Badge, Card, Table, type TableColumn } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { usePluginsQuery } from "../../hooks/usePluginsQuery.ts";
import { t } from "../../messages.ts";
import { kindLabel } from "../../model/kindLabel.ts";
import { IdleState } from "../IdleState/IdleState.tsx";
import { LoadingState } from "../LoadingState/LoadingState.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./PluginList.css";

interface CapabilityRow {
  kind: string;
  id: string;
  version: string;
  faulted: boolean;
}

const columns: readonly TableColumn<CapabilityRow>[] = [
  { key: "kind", label: t("plugins.colKind") },
  { key: "id", label: t("plugins.id"), mono: true },
  { key: "version", label: t("plugins.version"), mono: true },
  {
    key: "faulted",
    label: t("plugins.colStatus"),
    render: (row) =>
      row.faulted ? <Badge tone="danger">{t("status.faulted")}</Badge> : t("status.ok"),
  },
];

/** The loaded plugins, one Card each with its capabilities (M8 WI-13). */
export function PluginList(): ReactElement | null {
  const { idle, query } = usePluginsQuery();
  if (idle) return <IdleState />;
  const alert = query.isError ? <ProblemAlert error={query.error} failed="plugins.failed" /> : null;
  if (query.data === undefined) return alert ?? <LoadingState />;
  const plugins = query.data.plugins;
  if (plugins.length === 0)
    return (
      <>
        {alert}
        <p className="umbra-status-empty type-body">{t("plugins.empty")}</p>
      </>
    );
  return (
    <div className="umbra-plugin-list">
      {alert}
      {plugins.map((plugin) => (
        <Card key={plugin.id} title={plugin.name}>
          <dl className="umbra-plugin-facts type-body">
            <dt>{t("plugins.id")}</dt>
            <dd>{plugin.id}</dd>
            <dt>{t("plugins.version")}</dt>
            <dd>{plugin.version}</dd>
            <dt>{t("plugins.abi")}</dt>
            <dd>{t("plugins.abiValue", { major: plugin.abi_major, minor: plugin.abi_minor })}</dd>
            <dt>{t("plugins.root")}</dt>
            <dd>{plugin.root}</dd>
            <dt>{t("plugins.origin")}</dt>
            <dd>{plugin.origin}</dd>
          </dl>
          <Table
            caption={t("plugins.capabilities", { name: plugin.name })}
            columns={columns}
            rows={plugin.capabilities.map((c) => ({
              kind: kindLabel(c.kind),
              id: c.id,
              version: c.version,
              faulted: c.faulted,
            }))}
            rowKey={(row) => row.id}
            empty={t("plugins.noCapabilities")}
            dense
          />
        </Card>
      ))}
    </div>
  );
}
