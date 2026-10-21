import { DataProvider } from "@umbra/data";
import { type EngineStreamConnection, EngineStreamProvider } from "@umbra/engine-stream";
import { ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { type DataRouter, RouterProvider } from "react-router";

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
