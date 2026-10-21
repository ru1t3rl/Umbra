import { DataProvider } from "@umbra/data";
import { Alert, ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { type DataRouter, RouterProvider } from "react-router";
import { t } from "./messages.ts";

export interface AppProps {
  /** Created once by the bootstrap after the config loaded (tests inject a memory router). */
  router: DataRouter;
}

export function App({ router }: AppProps): ReactElement {
  return (
    <ThemeProvider>
      <DataProvider>
        <RouterProvider router={router} />
      </DataProvider>
    </ThemeProvider>
  );
}

export function ConfigErrorApp(): ReactElement {
  return (
    <ThemeProvider>
      <Alert tone="danger" title={t("app.configError")} />
    </ThemeProvider>
  );
}
