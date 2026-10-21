import { Alert, ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";

export function ConfigErrorApp(): ReactElement {
  return (
    <ThemeProvider>
      <Alert tone="danger" title={t("app.configError")} />
    </ThemeProvider>
  );
}
