// Public API of @umbra/runtime-config: runtime configuration model, loader and React provider.

export type { RuntimeConfigProviderProps } from "./components/RuntimeConfigProvider.tsx";
export { RuntimeConfigProvider, useRuntimeConfig } from "./components/RuntimeConfigProvider.tsx";
export type { RuntimeConfig } from "./model/config.ts";
export {
  ConfigError,
  DEFAULT_CONFIG,
  loadRuntimeConfig,
  parseRuntimeConfig,
} from "./model/config.ts";
