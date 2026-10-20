export type ThemePreference = "dark" | "light" | "system";
export type ResolvedTheme = "dark" | "light";

export const THEME_STORAGE_KEY = "umbra.theme";

export function parsePreference(value: string | null | undefined): ThemePreference | undefined {
  return value === "dark" || value === "light" || value === "system" ? value : undefined;
}

/** `system` follows the OS; the other preferences are already a resolved theme (ASM-301). */
export function resolveTheme(
  preference: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (preference === "system") return systemPrefersDark ? "dark" : "light";
  return preference;
}
