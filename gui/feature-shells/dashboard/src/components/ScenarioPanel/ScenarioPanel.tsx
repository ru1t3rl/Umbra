import { useRunState } from "@umbra/engine-stream";
import { ScenarioLoader } from "@umbra/scenario-control";
import { Button, Panel } from "@umbra/ui-kit";
import { type ReactElement, useEffect, useRef } from "react";
import { usePopover } from "../../hooks/usePopover.ts";
import { t } from "../../messages.ts";
import "./ScenarioPanel.css";

export interface ScenarioPanelProps {
  open: boolean;
  onClose: () => void;
  recentPaths: readonly string[];
  onLoaded: (path: string) => void;
  triggerId: string;
  panelId: string;
}

/** The loader in a Panel, opened by the bar's Scenario button outside `idle` (idle shows the centre Card instead). */
export function ScenarioPanel({
  open,
  onClose,
  recentPaths,
  onLoaded,
  triggerId,
  panelId,
}: ScenarioPanelProps): ReactElement | null {
  const state = useRunState();
  const panelRef = useRef<HTMLDivElement>(null);
  const shown = open && state !== null && state !== "idle";
  usePopover({ open: shown, onClose, triggerId, panelRef });
  // The run went idle (another client unloaded, or this one): clear the popover, or it would come back by itself
  // when the next scenario loads (ASM-988).
  useEffect(() => {
    if (open && state === "idle") onClose();
  }, [open, state, onClose]);
  if (!shown) return null;
  return (
    <div className="dashboard-scenario" id={panelId} ref={panelRef} tabIndex={-1}>
      <Panel
        title={t("scenario.title")}
        width="100%"
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon="close"
            aria-label={t("scenario.close")}
            onClick={onClose}
          />
        }
      >
        <ScenarioLoader recentPaths={recentPaths} onLoaded={onLoaded} submitVariant="tonal" />
      </Panel>
    </div>
  );
}
