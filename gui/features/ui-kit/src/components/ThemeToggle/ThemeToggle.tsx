import type { ReactElement } from "react";
import { useTheme } from "../../hooks/useTheme.ts";
import { defineMessages } from "../../i18n/defineMessages.ts";
import { Icon } from "../Icon/Icon.tsx";
import { Toggle } from "../Toggle/Toggle.tsx";
import "./ThemeToggle.css";

const t = defineMessages({ label: "Day theme" });

export interface ThemeToggleProps {
  className?: string;
  /** No visible label (a compact bar): an icon of the current theme and a title instead; the name stays "Day theme". */
  iconOnly?: boolean;
  /** The accessible name; "Day theme" by default (a page with a second switch needs another one). */
  label?: string;
}

/** Switches between the Ops (dark) and Day (light) themes. Must be inside a `ThemeProvider`. */
export function ThemeToggle({ className, iconOnly, label }: ThemeToggleProps): ReactElement {
  const { resolved, setPreference } = useTheme();
  const name = label ?? t("label");
  const toggle = (
    <Toggle
      {...(className ? { className } : {})}
      {...(iconOnly ? { "aria-label": name } : { label: name })}
      checked={resolved === "light"}
      onChange={(next) => setPreference(next ? "light" : "dark")}
    />
  );
  if (!iconOnly) return toggle;
  return (
    <span className="vt-theme-toggle" title={name}>
      <Icon name={resolved === "light" ? "light_mode" : "dark_mode"} />
      {toggle}
    </span>
  );
}
