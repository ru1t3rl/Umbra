import { CommandProblemProvider, RunControls, RunStatusBar } from "@umbra/scenario-control";
import { AppNav, CommandBar, ThemeToggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import type { FitLevel } from "../../model/fitLevel.ts";
import { ClockMenu } from "../ClockMenu/ClockMenu.tsx";
import { ScenarioButton } from "../ScenarioButton/ScenarioButton.tsx";

export interface DashboardBarProps {
  /** How compact the bar must be to fit one row (`useBarFit`): 1 drops the title and labels, 2 also labels the lifecycle buttons with icons. */
  fit: FitLevel;
  clockOpen: boolean;
  onToggleClock: () => void;
  onCloseClock: () => void;
  scenarioOpen: boolean;
  onToggleScenario: () => void;
  scenarioTriggerId: string;
  scenarioPanelId: string;
  /** The idle run's "Load" command: focuses the scenario loader. */
  onLoadRequested: () => void;
  /** Where a failed run command renders its Alert (the page's alert column). */
  problemContainer: HTMLElement | null;
}

/** The floating glass bar: page navigation, run status, run controls, clock, scenario and theme. One row. */
export function DashboardBar({
  fit,
  clockOpen,
  onToggleClock,
  onCloseClock,
  scenarioOpen,
  onToggleScenario,
  scenarioTriggerId,
  scenarioPanelId,
  onLoadRequested,
  problemContainer,
}: DashboardBarProps): ReactElement {
  return (
    <CommandBar floating title={fit >= 1 ? "" : t("page.title")}>
      <CommandProblemProvider>
        <AppNav />
        <RunStatusBar />
        <RunControls
          layout="bar"
          iconOnly={fit === 2}
          problemContainer={problemContainer}
          onLoadRequested={onLoadRequested}
        />
        <ClockMenu
          open={clockOpen}
          onToggle={onToggleClock}
          onClose={onCloseClock}
          compact={fit >= 1}
        />
        <ScenarioButton
          open={scenarioOpen}
          onToggle={onToggleScenario}
          triggerId={scenarioTriggerId}
          panelId={scenarioPanelId}
          compact={fit >= 1}
        />
      </CommandProblemProvider>
      <ThemeToggle iconOnly={fit >= 1} />
    </CommandBar>
  );
}
