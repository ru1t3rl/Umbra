import {
  AppNav,
  CommandBar,
  type NavItem,
  NavigationProvider,
  Select,
  type SelectOption,
  Table,
  type TableColumn,
} from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";

interface CapabilityRow {
  id: string;
  kind: string;
  plugin: string;
  faults: number;
}

const COLUMNS: TableColumn<CapabilityRow>[] = [
  { key: "id", label: t("table.col-capability"), mono: true },
  { key: "kind", label: t("table.col-kind") },
  { key: "plugin", label: t("table.col-plugin") },
  { key: "faults", label: t("table.col-faults"), align: "end" },
];

const ROWS: CapabilityRow[] = [
  { id: "example.behavior.wander", kind: "behavior", plugin: "example-wander", faults: 0 },
  { id: "builtin.ground.kinematic", kind: "motion", plugin: "builtin", faults: 0 },
  { id: "example.sensor.radar", kind: "sensor", plugin: "example-radar", faults: 2 },
];

const FORMATS: SelectOption[] = [
  { value: "wgs84", label: t("select.wgs84") },
  { value: "utm", label: t("select.utm") },
  { value: "mgrs", label: t("select.mgrs") },
  { value: "bng", label: t("select.bng"), disabled: true },
];

const PAGES: NavItem[] = [
  { path: "/", label: t("nav.live-map"), icon: "map" },
  { path: "/templates", label: t("nav.templates"), icon: "dataset" },
  { path: "/plugins", label: t("nav.plugins"), icon: "extension" },
];

/** ADR 0073 additions: Table, Select and AppNav with a local, router-free navigation state. */
export function NavDemo(): ReactElement {
  const [format, setFormat] = useState("wgs84");
  const [path, setPath] = useState("/");
  return (
    <div className="showcase-col">
      <div id="showcase-table">
        <Table
          caption={t("table.caption")}
          columns={COLUMNS}
          rows={ROWS}
          rowKey={(r) => r.id}
          empty={t("table.empty")}
        />
      </div>
      <div id="showcase-select">
        <Select
          label={t("select.label")}
          help={t("select.help")}
          options={FORMATS}
          value={format}
          onChange={setFormat}
        />
      </div>
      <div id="showcase-appnav">
        <NavigationProvider items={PAGES} currentPath={path} navigate={setPath}>
          <CommandBar title={t("nav.bar-title")}>
            <AppNav />
          </CommandBar>
        </NavigationProvider>
      </div>
    </div>
  );
}
