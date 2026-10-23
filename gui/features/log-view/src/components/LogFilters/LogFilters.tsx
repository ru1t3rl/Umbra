import type { JournalLevel, JournalSourceName } from "@umbra/engine-stream";
import { Chip, ChipGroup, TextField } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import type { LogFilter } from "../../model/filterRecords.ts";
import "./LogFilters.css";

const LEVELS: readonly {
  level: JournalLevel;
  label: "level.all" | "level.warning" | "level.error";
}[] = [
  { level: "info", label: "level.all" },
  { level: "warning", label: "level.warning" },
  { level: "error", label: "level.error" },
];

export interface LogFiltersProps {
  /** The id of the group, for `aria-controls` on the button that shows it. */
  id: string;
  filter: LogFilter;
  onChange: (filter: LogFilter) => void;
  /** The sources to offer, in display order. */
  sources: readonly JournalSourceName[];
}

/** The search field and the Level and Sources chip groups. */
export function LogFilters({ id, filter, onChange, sources }: LogFiltersProps): ReactElement {
  const toggle = (source: JournalSourceName): void =>
    onChange({
      ...filter,
      sources: filter.sources.includes(source)
        ? filter.sources.filter((s) => s !== source)
        : [...filter.sources, source],
    });
  return (
    <div id={id} className="umbra-log-filters">
      <TextField
        label={t("log.search")}
        value={filter.text}
        onChange={(e) => onChange({ ...filter, text: e.target.value })}
      />
      <ChipGroup label={t("log.level")}>
        {LEVELS.map(({ level, label }) => (
          <Chip
            key={level}
            selected={filter.level === level}
            onClick={() => onChange({ ...filter, level })}
          >
            {t(label)}
          </Chip>
        ))}
      </ChipGroup>
      <ChipGroup label={t("log.sources")}>
        {sources.map((source) => (
          <Chip
            key={source}
            selected={filter.sources.includes(source)}
            onClick={() => toggle(source)}
          >
            {t(`source.${source}`)}
          </Chip>
        ))}
      </ChipGroup>
    </div>
  );
}
