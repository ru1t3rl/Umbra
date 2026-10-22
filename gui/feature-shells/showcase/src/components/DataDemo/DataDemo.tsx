import { AffiliationTag, Badge, EntityList, EntityRow, Readout } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";

const ROWS = [
  {
    key: "a11",
    name: t("sample.alpha-1-1"),
    id: t("sample.id-1021"),
    affiliation: "friend",
    echelon: t("sample.plt"),
    meta: t("sample.31u-ft-12345-67890-047-32-km"),
    status: { label: t("sample.running"), tone: "success" },
  },
  {
    key: "a12",
    name: t("sample.alpha-1-2"),
    id: t("sample.id-1022"),
    affiliation: "friend",
    echelon: t("sample.plt"),
    meta: t("sample.31u-ft-12411-67702-051-30-km"),
    status: { label: t("sample.running"), tone: "success" },
  },
  {
    key: "r21",
    name: t("sample.red-recon-2"),
    id: t("sample.id-4410"),
    affiliation: "hostile",
    echelon: t("sample.sec"),
    meta: t("sample.31u-ft-15890-69120-230-0-km-"),
    status: { label: t("sample.stale"), tone: "neutral" },
    stale: true,
  },
  {
    key: "u01",
    name: t("sample.unidentified-track"),
    id: t("sample.id-0007"),
    affiliation: "unknown",
    meta: t("sample.31u-ft-14002-68877-180-12-km"),
    status: { label: t("sample.faulted"), tone: "danger" },
  },
] as const;

export function DataDemo(): ReactElement {
  const [selected, setSelected] = useState("a11");
  return (
    <>
      <div className="showcase-row">
        <Readout
          label={t("sample.scenario-clock")}
          value={t("sample.14-32-07")}
          unit={t("sample.z")}
          live
        />
        <Readout
          label={t("sample.entities")}
          value={t("sample.48")}
          live
          delta={t("sample.3-since-14-30")}
          trend="up"
        />
        <Readout label={t("sample.sim-rate")} value={t("sample.1-4")} />
        <Readout
          label={t("sample.pdu-rate")}
          value={t("sample.312")}
          unit={t("sample.s")}
          live
          delta={t("sample.12-vs-nominal")}
          trend="down"
        />
      </div>
      <div className="showcase-row">
        <Badge tone="success">{t("sample.running")}</Badge>
        <Badge tone="info">{t("sample.paused")}</Badge>
        <Badge tone="warning">{t("sample.degraded")}</Badge>
        <Badge tone="danger">{t("sample.faulted")}</Badge>
        <Badge>{t("sample.stale")}</Badge>
        <Badge tone="danger">{t("sample.destroyed")}</Badge>
      </div>
      <div className="showcase-row">
        <AffiliationTag affiliation="friend" />
        <AffiliationTag affiliation="hostile" />
        <AffiliationTag affiliation="neutral" />
        <AffiliationTag affiliation="unknown" />
        <AffiliationTag affiliation="friend">2 MECH BDE</AffiliationTag>
        <AffiliationTag affiliation="hostile">{t("sample.bn")}</AffiliationTag>
      </div>
      <EntityList label={t("sample.entities")}>
        {ROWS.map(({ key, ...row }) => (
          <EntityRow
            key={key}
            {...row}
            selected={selected === key}
            onSelect={() => setSelected(key)}
          />
        ))}
      </EntityList>
    </>
  );
}
