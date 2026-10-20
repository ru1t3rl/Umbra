// Public API of @umbra/ui-kit. Components arrive in WI-4..WI-6.

export type { IconProps } from "./components/Icon/Icon.tsx";
export { Icon } from "./components/Icon/Icon.tsx";
export type { MessageParams } from "./i18n/defineMessages.ts";
export { defineMessages } from "./i18n/defineMessages.ts";
export type { ResolvedTheme, ThemePreference } from "./theme/model.ts";
export type { ThemeContextValue, ThemeProviderProps } from "./theme/ThemeProvider.tsx";
export { ThemeProvider, useTheme } from "./theme/ThemeProvider.tsx";
export * from "./tokens/tokens.ts";
