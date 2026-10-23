import { useRunState } from "@umbra/engine-stream";
import { Button } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";

export interface ScenarioButtonProps {
  open: boolean;
  onToggle: () => void;
  triggerId: string;
  panelId: string;
  /** Icon-only button (a compact bar). */
  compact: boolean;
}

/**
 * Opens the scenario loader in a Panel. Shown only once the run state is known and is not `idle`: in idle the loader
 * Card is already in the centre, and with an unknown state a loader would post to an engine we know nothing of (ASM-970).
 */
export function ScenarioButton({
  open,
  onToggle,
  triggerId,
  panelId,
  compact,
}: ScenarioButtonProps): ReactElement | null {
  const state = useRunState();
  if (state === null || state === "idle") return null;
  const common = {
    id: triggerId,
    variant: "tonal",
    icon: "upload_file",
    "aria-expanded": open,
    "aria-controls": open ? panelId : undefined,
    onClick: onToggle,
  } as const;
  return compact ? (
    <Button {...common} aria-label={t("scenario.title")} />
  ) : (
    <Button {...common}>{t("scenario.title")}</Button>
  );
}
