import "@umbra/ui-kit/styles.css";
import { configureEngineUrl } from "@umbra/data";
import { connectEngineStream, streamUrl } from "@umbra/engine-stream";
import { loadRuntimeConfig, RuntimeConfigProvider } from "@umbra/runtime-config";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App, ConfigErrorApp } from "./App.tsx";
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
    const connection = connectEngineStream({
      url: streamUrl(config.engineUrl, window.location.origin),
    });
    root.render(
      <StrictMode>
        <RuntimeConfigProvider config={config}>
          <App router={createAppRouter()} connection={connection} />
        </RuntimeConfigProvider>
      </StrictMode>,
    );
  } catch (error) {
    showConfigError(error);
  }
}, showConfigError);
