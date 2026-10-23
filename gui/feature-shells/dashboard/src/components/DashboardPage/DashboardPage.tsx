import { useApplySubscription, useEntitySource } from "@umbra/engine-stream";
import { LiveMap, type LiveMapHandle } from "@umbra/map-view";
import { useRuntimeConfig } from "@umbra/runtime-config";
import { ConnectionAlert, RunFaultAlert } from "@umbra/scenario-control";
import { useAffiliationOf, useRecordRecentScenario, useSettings } from "@umbra/settings";
import { type CSSProperties, type ReactElement, useCallback, useId, useRef, useState } from "react";
import { useBarFit } from "../../hooks/useBarFit.ts";
import { useBarInset } from "../../hooks/useBarInset.ts";
import { t } from "../../messages.ts";
import { DashboardBar } from "../DashboardBar/DashboardBar.tsx";
import { EntitiesPanel } from "../EntitiesPanel/EntitiesPanel.tsx";
import { InspectorPanel } from "../InspectorPanel/InspectorPanel.tsx";
import { ScenarioCentre } from "../ScenarioCentre/ScenarioCentre.tsx";
import { ScenarioPanel } from "../ScenarioPanel/ScenarioPanel.tsx";
import "./DashboardPage.css";

/**
 * Layout and wiring only (ADR 0078): the map is the page. A floating glass bar on top (directly in the body so it
 * stays the page banner), a left panel (entities, log), a right panel (the inspector, while an entity is selected),
 * the scenario loader in the centre while idle, the alerts under the bar. One `useState` holds the selection.
 * The page never reads the run status: the components that need it subscribe on their own, so a frame does not
 * re-render the map element (ASM-970).
 */
export default function DashboardPage(): ReactElement {
  const source = useEntitySource();
  const { basemap } = useRuntimeConfig();
  const settings = useSettings();
  const recordRecent = useRecordRecentScenario();
  const affiliationOf = useAffiliationOf();
  useApplySubscription({ max_rate_hz: settings.stream.maxRateHz });
  const barRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<LiveMapHandle>(null);
  const centreRef = useRef<HTMLDivElement>(null);
  const inset = useBarInset(barRef);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  // At most one of the bar's popovers is open, so there are never more than three floating panels.
  const [popover, setPopover] = useState<"clock" | "scenario" | null>(null);
  const [problemSlot, setProblemSlot] = useState<HTMLElement | null>(null);
  const fit = useBarFit(barRef);
  const ids = useId();
  const scenarioTriggerId = `${ids}-scenario-button`;
  const scenarioPanelId = `${ids}-scenario-panel`;

  const recent = settings.scenarios.recent;
  const onLoaded = useCallback(
    (path: string): void => {
      recordRecent(path);
      setPopover(null);
    },
    [recordRecent],
  );
  const onCenter = useCallback((id: number): void => void mapRef.current?.centerOn(id), []);
  const onToggleScenario = useCallback(
    (): void => setPopover((p) => (p === "scenario" ? null : "scenario")),
    [],
  );
  const onToggleClock = useCallback(
    (): void => setPopover((p) => (p === "clock" ? null : "clock")),
    [],
  );
  const onCloseClock = useCallback((): void => setPopover((p) => (p === "clock" ? null : p)), []);
  const onCloseScenario = useCallback(
    (): void => setPopover((p) => (p === "scenario" ? null : p)),
    [],
  );
  // The idle "Load" command focuses the loader Card (ASM-977).
  const onLoadRequested = useCallback(
    (): void => centreRef.current?.querySelector<HTMLElement>("input, select, button")?.focus(),
    [],
  );

  const insetStyle = { "--dashboard-inset-top": `${inset}px` } as CSSProperties;
  return (
    <div className="dashboard" style={insetStyle}>
      <div className="dashboard-bar" ref={barRef}>
        <DashboardBar
          fit={fit}
          clockOpen={popover === "clock"}
          onToggleClock={onToggleClock}
          onCloseClock={onCloseClock}
          scenarioOpen={popover === "scenario"}
          onToggleScenario={onToggleScenario}
          scenarioTriggerId={scenarioTriggerId}
          scenarioPanelId={scenarioPanelId}
          onLoadRequested={onLoadRequested}
          problemContainer={problemSlot}
        />
      </div>
      <EntitiesPanel
        source={source}
        selectedId={selectedId}
        onSelect={setSelectedId}
        affiliationOf={affiliationOf}
      />
      <InspectorPanel
        source={source}
        selectedId={selectedId}
        onSelect={setSelectedId}
        onCenter={onCenter}
        affiliationOf={affiliationOf}
      />
      <main className="dashboard-map" style={insetStyle}>
        <h1 className="vt-visually-hidden">{t("page.title")}</h1>
        <LiveMap
          ref={mapRef}
          source={source}
          insets={{ top: inset }}
          selectedId={selectedId}
          onSelect={setSelectedId}
          affiliationOf={affiliationOf}
          {...(basemap ? { basemapStyleUrl: basemap.styleUrl } : {})}
        />
        <ScenarioCentre ref={centreRef} recentPaths={recent} onLoaded={onLoaded} />
        <div className="dashboard-alert">
          <ConnectionAlert />
          <RunFaultAlert />
          <div className="dashboard-problem" ref={setProblemSlot} />
          <ScenarioPanel
            open={popover === "scenario"}
            onClose={onCloseScenario}
            recentPaths={recent}
            onLoaded={onLoaded}
            triggerId={scenarioTriggerId}
            panelId={scenarioPanelId}
          />
        </div>
      </main>
    </div>
  );
}
