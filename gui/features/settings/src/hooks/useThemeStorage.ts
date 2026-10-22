import { useContext } from "react";
import { type ThemeStorage, ThemeStorageContext } from "../context/ThemeStorageContext.ts";

/**
 * A stable `Storage`-like view of `appearance.theme` for `ThemeProvider.storage` (desktop, WI-22). Only
 * `umbra.theme` maps; other keys read null and are ignored on write. Writes go through the provider's
 * single update path, so there is one writer of the settings record. Inert outside a provider.
 */
export function useThemeStorage(): ThemeStorage {
  return useContext(ThemeStorageContext);
}
