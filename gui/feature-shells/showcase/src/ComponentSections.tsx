import {
  AffiliationTag,
  Alert,
  Badge,
  Button,
  Card,
  Chip,
  ChipGroup,
  CommandBar,
  EntityList,
  EntityRow,
  Panel,
  Readout,
  Slider,
  Tabs,
  TextField,
  Toggle,
} from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "./messages.ts";
import { Section } from "./Section.tsx";

function ButtonsDemo(): ReactElement {
  const [rate, setRate] = useState(t("sample.1-4"));
  const [layers, setLayers] = useState({ tracks: true, grid: false });
  const [history, setHistory] = useState(true);
  const [record, setRecord] = useState(false);
  return (
    <>
      <div className="showcase-row">
        <Button variant="filled" icon="play_arrow">
          {t("sample.start-exercise")}
        </Button>
        <Button>{t("sample.assign-route")}</Button>
        <Button variant="outline">{t("sample.export-log")}</Button>
        <Button variant="ghost">{t("sample.cancel")}</Button>
        <Button variant="danger" icon="delete">
          {t("sample.remove-entity")}
        </Button>
        <Button variant="outline" icon="settings" aria-label={t("sample.settings")} />
        <Button size="sm">{t("sample.small")}</Button>
        <Button disabled>{t("sample.disabled")}</Button>
      </div>
      <div className="showcase-row">
        <ChipGroup label={t("sample.time-rate")}>
          {RATES.slice(0, 4).map((r) => (
            <Chip key={r} selected={rate === r} onClick={() => setRate(r)}>
              {r}
            </Chip>
          ))}
        </ChipGroup>
        <ChipGroup label={t("sample.map-layers")}>
          <Chip
            glass
            icon="timeline"
            selected={layers.tracks}
            onClick={() => setLayers({ ...layers, tracks: !layers.tracks })}
          >
            {t("sample.tracks")}
          </Chip>
          <Chip
            glass
            icon="grid_on"
            selected={layers.grid}
            onClick={() => setLayers({ ...layers, grid: !layers.grid })}
          >
            {t("sample.mgrs-grid")}
          </Chip>
        </ChipGroup>
      </div>
      <div className="showcase-row">
        <Toggle checked={history} onChange={setHistory} label={t("sample.show-historic-tracks")} />
        <Toggle checked={record} onChange={setRecord} label={t("sample.record-session")} />
        <Toggle checked disabled label={t("sample.terrain-loaded")} />
      </div>
    </>
  );
}

const RATES = [
  t("sample.1-1"),
  t("sample.1-2"),
  t("sample.1-4"),
  t("sample.1-8"),
  t("sample.1-16"),
];

function InputsDemo(): ReactElement {
  const [rate, setRate] = useState(2);
  const [hz, setHz] = useState(60);
  const [tab, setTab] = useState("orbat");
  return (
    <>
      <div className="showcase-col">
        <TextField
          label={t("sample.callsign")}
          placeholder={t("sample.e-g-alpha-1-1")}
          help={t("sample.shown-on-the-map-label")}
        />
        <TextField
          label={t("sample.position-mgrs")}
          mono
          defaultValue={t("sample.31u-ft-12345-67890")}
        />
        <TextField
          label={t("sample.march-speed")}
          mono
          defaultValue={t("sample.140-km-h")}
          invalid
          help={t("sample.exceeds-the-platform-maximum")}
        />
      </div>
      <div className="showcase-col">
        <Slider
          label={t("sample.time-rate")}
          min={0}
          max={4}
          step={1}
          value={rate}
          onChange={(e) => setRate(Number(e.target.value))}
          format={(v) => RATES[v] ?? String(v)}
        />
        <Slider
          label={t("sample.update-rate")}
          min={1}
          max={120}
          value={hz}
          onChange={(e) => setHz(Number(e.target.value))}
          format={(v) => `${v} Hz`}
        />
      </div>
      <Tabs
        label={t("sample.sidebar")}
        value={tab}
        onChange={setTab}
        items={[
          { value: "orbat", label: t("sample.order-of-battle"), count: 48 },
          { value: "orders", label: t("sample.orders"), count: 6 },
          { value: "events", label: t("sample.event-log"), count: 1203 },
          { value: "settings", label: t("sample.settings") },
        ]}
      />
    </>
  );
}

function CommandBarDemo(): ReactElement {
  const [paused, setPaused] = useState(false);
  return (
    <>
      <CommandBar
        title={t("sample.exercise-northern-shield")}
        subtitle={t("sample.run-3-48-entities")}
        clock={t("sample.14-32-07z")}
        paused={paused}
      >
        <Chip selected>1:4</Chip>
        {paused ? <Badge tone="info">{t("sample.paused")}</Badge> : null}
        <Button
          variant="outline"
          size="sm"
          icon={paused ? "play_arrow" : "pause"}
          onClick={() => setPaused(!paused)}
        >
          {paused ? t("sample.resume") : t("sample.pause")}
        </Button>
        <Button variant="filled" size="sm" icon="flag">
          {t("sample.end-run")}
        </Button>
      </CommandBar>
      <CommandBar
        floating
        title={t("sample.northern-shield")}
        subtitle={t("sample.review")}
        clock={t("sample.09-12-44z")}
        paused
      >
        <Badge tone="info">{t("sample.paused")}</Badge>
        <Button
          variant="ghost"
          size="sm"
          icon="skip_previous"
          aria-label={t("sample.previous-bookmark")}
        />
        <Button variant="tonal" size="sm" icon="play_arrow">
          {t("sample.play")}
        </Button>
        <Button variant="ghost" size="sm" icon="skip_next" aria-label={t("sample.next-bookmark")} />
      </CommandBar>
    </>
  );
}

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

function DataDemo(): ReactElement {
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

export function ComponentSections(): ReactElement {
  return (
    <div id="showcase-components" className="showcase-group">
      <Section id="buttons" heading={t("buttons.heading")}>
        <ButtonsDemo />
      </Section>
      <Section id="inputs" heading={t("inputs.heading")}>
        <InputsDemo />
      </Section>
      <Section id="panel" heading={t("panel.heading")}>
        <div className="showcase-row">
          <Panel
            title={t("sample.movement-orders")}
            actions={
              <>
                <Button
                  size="sm"
                  variant="ghost"
                  icon="open_in_full"
                  aria-label={t("sample.expand")}
                />
                <Button size="sm" variant="ghost" icon="close" aria-label={t("sample.close")} />
              </>
            }
          >
            <div className="showcase-col showcase-flush">
              <p className="showcase-muted">Alpha Company, 2nd Mechanised Brigade</p>
              <TextField
                label={t("sample.destination")}
                mono
                defaultValue={t("sample.31u-ft-12345-67890")}
              />
              <div className="showcase-row showcase-end">
                <Button variant="ghost" size="sm">
                  {t("sample.discard")}
                </Button>
                <Button variant="filled" size="sm">
                  {t("sample.issue-order")}
                </Button>
              </div>
            </div>
          </Panel>
          <Card
            title={t("sample.2nd-mechanised-brigade")}
            subtitle={t("sample.48-entities-3-battalions")}
          >
            <div className="showcase-row showcase-flush">
              <AffiliationTag affiliation="friend">{t("sample.bde")}</AffiliationTag>
              <Badge tone="success">{t("sample.running")}</Badge>
            </div>
          </Card>
          <Card
            level={3}
            title={t("sample.red-force-reserve")}
            subtitle={t("sample.12-entities-held")}
          >
            <div className="showcase-row showcase-flush">
              <AffiliationTag affiliation="hostile">{t("sample.bn")}</AffiliationTag>
              <Badge tone="info">{t("sample.paused")}</Badge>
            </div>
          </Card>
          <Card
            variant="outline"
            title={t("sample.add-formation")}
            subtitle={t("sample.from-template-or-empty")}
          />
        </div>
      </Section>
      <Section id="commandbar" heading={t("commandbar.heading")}>
        <CommandBarDemo />
      </Section>
      <Section id="data" heading={t("data.heading")}>
        <DataDemo />
      </Section>
      <Section id="alerts" heading={t("alerts.heading")}>
        <div className="showcase-col">
          <Alert
            tone="danger"
            title={t("sample.route-rejected")}
            action={
              <Button size="sm" variant="ghost">
                {t("sample.edit-route")}
              </Button>
            }
          >
            {t("sample.the-destination-lies-outside")}
          </Alert>
          <Alert
            tone="warning"
            title={t("sample.federate-latency-480-ms")}
            action={
              <Button size="sm" variant="ghost">
                {t("sample.details")}
              </Button>
            }
          >
            {t("sample.updates-from-the-flight-simu")}
          </Alert>
          <Alert tone="success" title={t("sample.recording-started")}>
            {t("sample.session-is-being-written-to-")}
          </Alert>
          <Alert tone="info" title={t("sample.12-entities-paused")}>
            {t("sample.resume-the-run-to-continue-t")}
          </Alert>
        </div>
      </Section>
    </div>
  );
}
