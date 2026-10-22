import "@umbra/map-view/styles.css";
import "@umbra/ui-kit/styles.css";
import { configureEngineUrl } from "@umbra/data";
import { connectEngineStream, streamUrl } from "@umbra/engine-stream";
import { loadRuntimeConfig, RuntimeConfigProvider } from "@umbra/runtime-config";
import { createLocalStorageBackend, parseSettings } from "@umbra/settings";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./components/App/App.tsx";
import { ConfigErrorApp } from "./components/ConfigErrorApp/ConfigErrorApp.tsx";
import { createAppRouter } from "./router.tsx";

const container = document.getElementById("root");
if (!container) throw new Error("missing #root element");
const root = createRoot(container);

function showConfigError(error: unknown): void {
  console.error(error);
  root.render(
    <StrictMode>
      <ConfigErrorApp />
    </StrictMode>,
  );
}

loadRuntimeConfig(fetch.bind(globalThis), import.meta.env.BASE_URL).then((config) => {
  try {
    configureEngineUrl(config.engineUrl);
    // The connection is created before SettingsProvider mounts, so read the journal capacity from the same
    // backend synchronously; the provider then loads the same value. A change applies on next start (ASM-896).
    const settingsBackend = createLocalStorageBackend();
    const connection = connectEngineStream({
      url: streamUrl(config.engineUrl, window.location.origin),
      journalCapacity: parseSettings(settingsBackend.read(), () => {}).journal.capacity,
    });
    root.render(
      <StrictMode>
        <RuntimeConfigProvider config={config}>
          <App
            router={createAppRouter()}
            connection={connection}
            settingsBackend={settingsBackend}
          />
        </RuntimeConfigProvider>
      </StrictMode>,
    );
  } catch (error) {
    showConfigError(error);
  }
}, showConfigError);
