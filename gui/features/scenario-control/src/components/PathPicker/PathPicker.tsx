import { Select } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import type { ScenarioSuggestion } from "../../model/exampleScenarios.ts";

/** A Select that fills the path; it shows the option equal to the path, else the placeholder (keyboard-safe). */
export function PathPicker({
  label,
  placeholder,
  items,
  path,
  onPick,
}: {
  label: string;
  placeholder: string;
  items: readonly ScenarioSuggestion[];
  path: string;
  onPick: (path: string) => void;
}): ReactElement | null {
  if (items.length === 0) return null;
  return (
    <Select
      label={label}
      value={items.some((i) => i.path === path) ? path : ""}
      options={[
        { value: "", label: placeholder, disabled: true },
        ...items.map((i) => ({ value: i.path, label: i.label ?? i.path })),
      ]}
      onChange={(value) => value && onPick(value)}
    />
  );
}
