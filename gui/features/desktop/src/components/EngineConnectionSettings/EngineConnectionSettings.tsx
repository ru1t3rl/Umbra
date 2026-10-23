import { useRuntimeConfig } from "@umbra/runtime-config";
import { TextField } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";

/** Stub until WI-22 (ADRs 0075, 0076): the engine URL of the runtime config, read-only (the page origin when empty). */
export function EngineConnectionSettings(): ReactElement {
  const { engineUrl } = useRuntimeConfig();
  return (
    <TextField
      label={t("engineUrl.label")}
      help={t("engineUrl.help")}
      readOnly
      value={engineUrl || window.location.origin}
    />
  );
}
