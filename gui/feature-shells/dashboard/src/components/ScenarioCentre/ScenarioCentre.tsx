import { useRunState } from "@umbra/engine-stream";
import { ScenarioLoader } from "@umbra/scenario-control";
import { Card } from "@umbra/ui-kit";
import type { ReactElement, Ref } from "react";
import { t } from "../../messages.ts";
import "./ScenarioCentre.css";

export interface ScenarioCentreProps {
  ref?: Ref<HTMLDivElement>;
  recentPaths: readonly string[];
  onLoaded: (path: string) => void;
}

/** While the run is `idle`, the loader sits in a tonal Card in the centre of the page (ASM-970). */
export function ScenarioCentre({
  ref,
  recentPaths,
  onLoaded,
}: ScenarioCentreProps): ReactElement | null {
  if (useRunState() !== "idle") return null;
  return (
    <div className="dashboard-centre" ref={ref}>
      <Card title={t("scenario.title")}>
        <ScenarioLoader recentPaths={recentPaths} onLoaded={onLoaded} submitVariant="tonal" />
      </Card>
    </div>
  );
}
