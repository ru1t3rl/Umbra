import { DataProvider } from "@umbra/data";
import { type EngineStreamConnection, EngineStreamProvider } from "@umbra/engine-stream";
import { Alert, ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { type DataRouter, RouterProvider } from "react-router";
import { t } from "./messages.ts";

export interface AppProps {
  /** Created once by the bootstrap after the config loaded (tests inject a memory router). */
  router: DataRouter;
  /** Created once by the bootstrap after the config loaded; tests inject one with a fake socket. */
  connection: EngineStreamConnection;
}

export function App({ router, connection }: AppProps): ReactElement {
  return (
    <ThemeProvider>
      <DataProvider>
        <EngineStreamProvider connection={connection}>
          <RouterProvider router={router} />
        </EngineStreamProvider>
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
