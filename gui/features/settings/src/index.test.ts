import { expect, test } from "bun:test";
import * as api from "./index.ts";

test("the public API of @umbra/settings (plan section 3 table)", () => {
  for (const name of [
    "SettingsProvider",
    "useSettings",
    "useUpdateSettings",
    "useAffiliationOf",
    "defaultAffiliationOf",
    "parseSettings",
    "DEFAULT_SETTINGS",
    "createLocalStorageBackend",
    "useThemeStorage",
    // WI-17: the setting editors the settings shell lays out
    "AffiliationOverridesEditor",
    "StreamRateSetting",
    "JournalCapacitySetting",
    "ThemePreferenceSetting",
    "RecentScenariosSetting",
  ]) {
    expect(typeof (api as Record<string, unknown>)[name]).not.toBe("undefined");
  }
});
