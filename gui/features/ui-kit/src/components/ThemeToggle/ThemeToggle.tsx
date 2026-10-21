import type { ReactElement } from "react";
import { defineMessages } from "../../i18n/defineMessages.ts";
import { useTheme } from "../../theme/ThemeProvider.tsx";
import { Toggle } from "../Toggle/Toggle.tsx";

const t = defineMessages({ label: "Day theme" });

export interface ThemeToggleProps {
  className?: string;
}

/** Switches between the Ops (dark) and Day (light) themes. Must be inside a `ThemeProvider`. */
export function ThemeToggle({ className }: ThemeToggleProps): ReactElement {
  const { resolved, setPreference } = useTheme();
  return (
    <Toggle
      {...(className ? { className } : {})}
      label={t("label")}
      checked={resolved === "light"}
      onChange={(next) => setPreference(next ? "light" : "dark")}
    />
  );
}
