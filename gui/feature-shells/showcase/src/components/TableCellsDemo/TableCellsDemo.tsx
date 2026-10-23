import { Badge, Table, type TableColumn } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";

interface StatusRow {
  id: string;
  kind: string;
  faulted: boolean;
}

const COLUMNS: TableColumn<StatusRow>[] = [
  { key: "id", label: t("cells.col-capability"), mono: true },
  { key: "kind", label: t("cells.col-kind") },
  {
    key: "faulted",
    label: t("cells.col-status"),
    render: (row) =>
      row.faulted ? <Badge tone="danger">{t("cells.faulted")}</Badge> : t("cells.ok"),
  },
];

const ROWS: StatusRow[] = [
  { id: "example.sensor.radar", kind: "sensor", faulted: false },
  { id: "example.behavior.wander", kind: "behavior", faulted: true },
  { id: "builtin.ground.kinematic", kind: "motion", faulted: false },
];

/** `TableColumn.render` (ASM-961): a Badge inside a cell, at the normal and the dense row height. */
export function TableCellsDemo(): ReactElement {
  return (
    <div id="showcase-table-cells" className="showcase-col showcase-col-wide">
      <div className="showcase-scroll-x">
        <Table caption={t("cells.caption")} columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />
      </div>
      <div className="showcase-scroll-x">
        <Table
          caption={t("cells.caption-dense")}
          columns={COLUMNS}
          rows={ROWS}
          rowKey={(r) => r.id}
          dense
        />
      </div>
    </div>
  );
}
