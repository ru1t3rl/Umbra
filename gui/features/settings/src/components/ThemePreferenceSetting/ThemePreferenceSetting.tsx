import { parsePreference, Select, useTheme } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";

const OPTIONS = [
  { value: "dark", label: t("theme.dark") },
  { value: "light", label: t("theme.light") },
  { value: "system", label: t("theme.system") },
] as const;

/** The theme preference; the ThemeProvider stays the one writer of where it is stored (ASM-301, ASM-995 b). */
export function ThemePreferenceSetting(): ReactElement {
  const { preference, setPreference } = useTheme();
  return (
    <Select
      label={t("theme.label")}
      options={OPTIONS}
      value={preference}
      onChange={(value) => {
        const next = parsePreference(value);
        if (next) setPreference(next);
      }}
    />
  );
}
