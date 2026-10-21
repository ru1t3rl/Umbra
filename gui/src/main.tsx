import "@umbra/ui-kit/styles.css";
import { configureEngineUrl } from "@umbra/data";
import { loadRuntimeConfig, RuntimeConfigProvider } from "@umbra/runtime-config";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App, ConfigErrorApp } from "./App.tsx";
import { createAppRouter } from "./router.tsx";

const container = document.getElementById("root");
if (!container) throw new Error("missing #root element");
const root = createRoot(container);

loadRuntimeConfig(fetch.bind(globalThis), import.meta.env.BASE_URL).then(
  (config) => {
    configureEngineUrl(config.engineUrl);
    root.render(
      <StrictMode>
        <RuntimeConfigProvider config={config}>
          <App router={createAppRouter()} />
        </RuntimeConfigProvider>
      </StrictMode>,
    );
  },
  (error: unknown) => {
    console.error(error);
    root.render(
      <StrictMode>
        <ConfigErrorApp />
      </StrictMode>,
    );
  },
);
