import { DataProvider } from "@umbra/data";
import { type EngineStreamConnection, EngineStreamProvider } from "@umbra/engine-stream";
import { type SettingsBackend, SettingsProvider } from "@umbra/settings";
import { ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { type DataRouter, RouterProvider } from "react-router";
import { SessionInvalidator } from "../SessionInvalidator/SessionInvalidator.tsx";
import { StreamRateApplier } from "../StreamRateApplier/StreamRateApplier.tsx";

export interface AppProps {
  /** Created once by the bootstrap after the config loaded (tests inject a memory router). */
  router: DataRouter;
  /** Created once by the bootstrap after the config loaded; tests inject one with a fake socket. */
  connection: EngineStreamConnection;
  /** Default: the localStorage backend. The desktop app injects its own (WI-22). */
  settingsBackend?: SettingsBackend;
}

export function App({ router, connection, settingsBackend }: AppProps): ReactElement {
  return (
    <SettingsProvider backend={settingsBackend}>
      <ThemeProvider>
        <DataProvider>
          <EngineStreamProvider connection={connection}>
            <StreamRateApplier />
            <SessionInvalidator>
              <RouterProvider router={router} />
            </SessionInvalidator>
          </EngineStreamProvider>
        </DataProvider>
      </ThemeProvider>
    </SettingsProvider>
  );
}
