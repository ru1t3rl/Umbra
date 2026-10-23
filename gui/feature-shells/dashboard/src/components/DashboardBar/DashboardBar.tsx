import { CommandProblemProvider, RunControls, RunStatusBar } from "@umbra/scenario-control";
import { AppNav, CommandBar, ThemeToggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import type { FitLevel } from "../../model/fitLevel.ts";
import { ClockMenu } from "../ClockMenu/ClockMenu.tsx";
import { ScenarioButton } from "../ScenarioButton/ScenarioButton.tsx";

export interface DashboardBarProps {
  /** How compact the bar must be to fit one row (`useBarFit`, four levels, ASM-992): 1 shows the page navigation as icons, 2 also drops the title and the labels of Clock, Scenario and theme, 3 also shows the lifecycle buttons as icons. */
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
    <CommandBar floating title={fit >= 2 ? "" : t("page.title")}>
      <CommandProblemProvider>
        <AppNav iconOnly={fit >= 1} />
        <RunStatusBar />
        <RunControls
          layout="bar"
          iconOnly={fit === 3}
          problemContainer={problemContainer}
          onLoadRequested={onLoadRequested}
        />
        <ClockMenu
          open={clockOpen}
          onToggle={onToggleClock}
          onClose={onCloseClock}
          compact={fit >= 2}
        />
        <ScenarioButton
          open={scenarioOpen}
          onToggle={onToggleScenario}
          triggerId={scenarioTriggerId}
          panelId={scenarioPanelId}
          compact={fit >= 2}
        />
      </CommandProblemProvider>
      <ThemeToggle iconOnly={fit >= 2} />
    </CommandBar>
  );
}
