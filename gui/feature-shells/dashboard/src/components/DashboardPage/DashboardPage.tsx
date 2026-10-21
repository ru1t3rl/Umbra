import { useEntitySource } from "@umbra/engine-stream";
import { LiveMap } from "@umbra/map-view";
import { useRuntimeConfig } from "@umbra/runtime-config";
import { ConnectionAlert, RunStatusBar } from "@umbra/scenario-control";
import { CommandBar, ThemeToggle } from "@umbra/ui-kit";
import { type CSSProperties, type ReactElement, useRef } from "react";
import { useBarInset } from "../../hooks/useBarInset.ts";
import { t } from "../../messages.ts";
import "./DashboardPage.css";

/**
 * Layout and wiring only: the map is the page. A floating glass bar on top (status Readouts, theme switch),
 * the connection banner below it. The bar sits directly in the body so it stays the page banner landmark.
 */
export default function DashboardPage(): ReactElement {
  const source = useEntitySource();
  const { basemap } = useRuntimeConfig();
  const barRef = useRef<HTMLDivElement>(null);
  const inset = useBarInset(barRef);
  return (
    <>
      <div className="dashboard-bar" ref={barRef}>
        <CommandBar floating title={t("page.title")}>
          <RunStatusBar />
          <ThemeToggle />
        </CommandBar>
      </div>
      <main
        className="dashboard-map"
        style={{ "--dashboard-inset-top": `${inset}px` } as CSSProperties}
      >
        <h1 className="vt-visually-hidden">{t("page.title")}</h1>
        <LiveMap
          source={source}
          insets={{ top: inset }}
          {...(basemap ? { basemapStyleUrl: basemap.styleUrl } : {})}
        />
        <div className="dashboard-alert">
          <ConnectionAlert />
        </div>
      </main>
    </>
  );
}
