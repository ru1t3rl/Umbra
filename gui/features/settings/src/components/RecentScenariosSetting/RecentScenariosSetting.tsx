import { Button } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { useSettings } from "../../hooks/useSettings.ts";
import { useUpdateSettings } from "../../hooks/useUpdateSettings.ts";
import { t } from "../../messages.ts";
import "./RecentScenariosSetting.css";

function status(count: number): string {
  if (count === 0) return t("recent.none");
  return count === 1 ? t("recent.one") : t("recent.many", { count });
}

/** The recent scenario list: how many there are and a button that clears it. */
export function RecentScenariosSetting(): ReactElement {
  const count = useSettings().scenarios.recent.length;
  const update = useUpdateSettings();
  return (
    <div className="recent-setting">
      <span>{status(count)}</span>
      <Button disabled={count === 0} onClick={() => update({ scenarios: { recent: [] } })}>
        {t("recent.clear")}
      </Button>
    </div>
  );
}
