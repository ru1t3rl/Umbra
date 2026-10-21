import type { MapView } from "./MapView.ts";

/**
 * Read-only inspection for the e2e build only (ASM-309): WebGL markers are not DOM, so a test reads the
 * layer through `window.__UMBRA_E2E__.map`. Called only behind `import.meta.env.VITE_UMBRA_E2E`; the
 * production build contains neither this code nor the global's name (tested).
 */
export function registerE2eHook(view: MapView): () => void {
  const hook = Object.freeze({ map: view.inspect() });
  Object.defineProperty(window, "__UMBRA_E2E__", {
    value: hook,
    configurable: true,
    enumerable: false,
    writable: false,
  });
  return () => {
    // Only our own registration: a newer view (StrictMode remount) may have replaced it already.
    if (Reflect.get(window, "__UMBRA_E2E__") === hook)
      Reflect.deleteProperty(window, "__UMBRA_E2E__");
  };
}
