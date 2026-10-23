import { useEngineOutputSource } from "@umbra/desktop";
import type { JournalSourceName } from "@umbra/engine-stream";
import { useJournalSource } from "@umbra/engine-stream";
import { Badge, Button, VirtualList } from "@umbra/ui-kit";
import { type ReactElement, useId, useMemo, useState } from "react";
import { useRecords } from "../../hooks/useRecords.ts";
import { useThrottledCount } from "../../hooks/useThrottledCount.ts";
import { t } from "../../messages.ts";
import { filterRecords, type LogFilter, NO_FILTER } from "../../model/filterRecords.ts";
import { formatRecord } from "../../model/formatRecord.ts";
import { toJsonLines } from "../../model/toJsonLines.ts";
import { saveJsonLines } from "../../saveJsonLines.ts";
import { LogFilters } from "../LogFilters/LogFilters.tsx";
import "./LogView.css";

export interface LogViewProps {
  /** The accessible name of the log. */
  label: string;
  /** Hide the search field and the chips behind a Filters button (they stay in force while hidden). */
  compact?: boolean;
}

const sourceLabel = (source: JournalSourceName): string => t(`source.${source}`);

const BASE_SOURCES: readonly JournalSourceName[] = ["event", "run", "connection", "gui"];

/**
 * The engine log: the journal (and, on desktop, the engine process output) as a virtualized, end-following
 * log with level, source and text filters and a JSON Lines download (ADR 0071, ASM-944). It reads the
 * journal imperatively, so only this component updates when records arrive.
 */
export function LogView({ label, compact = false }: LogViewProps): ReactElement {
  const journal = useJournalSource();
  const engine = useEngineOutputSource();
  const sources = useMemo(() => (engine ? [journal, engine] : [journal]), [journal, engine]);
  const records = useRecords(sources);
  const [filter, setFilter] = useState<LogFilter>(NO_FILTER);
  const [open, setOpen] = useState(false);
  const prefix = useId();
  const shown = useMemo(() => filterRecords(records, filter, sourceLabel), [records, filter]);
  const count = useThrottledCount(shown.length, records.length, filter);
  const filtersId = useId();
  // A pressed Engine process chip stays while the source is gone, so the user can see why and release it.
  const offered = useMemo<readonly JournalSourceName[]>(
    () =>
      engine || filter.sources.includes("engine-process")
        ? [...BASE_SOURCES, "engine-process"]
        : BASE_SOURCES,
    [engine, filter.sources],
  );

  return (
    <div className="umbra-log">
      {compact ? (
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={open}
          aria-controls={open ? filtersId : undefined}
          onClick={() => setOpen((o) => !o)}
        >
          {t("log.filters")}
        </Button>
      ) : null}
      {!compact || open ? (
        <LogFilters id={filtersId} filter={filter} onChange={setFilter} sources={offered} />
      ) : null}
      <div className="umbra-log-bar">
        <p className="umbra-log-count type-caption" role="status" aria-live="polite">
          {t("log.count", { shown: count.shown, total: count.total })}
        </p>
        <Button
          variant="outline"
          size="sm"
          icon="download"
          disabled={shown.length === 0}
          onClick={() => saveJsonLines(toJsonLines(shown))}
        >
          {t("log.download")}
        </Button>
      </div>
      {shown.length === 0 ? (
        <p className="umbra-log-empty type-body">
          {records.length === 0 ? t("log.empty") : t("log.noMatch")}
        </p>
      ) : null}
      <VirtualList
        role="log"
        label={label}
        count={shown.length}
        rowHeight="row-sm"
        followEnd
        getRowId={(i) => `${prefix}-${shown[i]?.source ?? "x"}-${shown[i]?.seq ?? i}`}
        className="umbra-log-rows"
        renderRow={(i) => {
          const record = shown[i];
          if (!record) return null;
          return (
            <span className="umbra-log-row">
              {record.level === "info" ? null : (
                <Badge tone={record.level === "error" ? "danger" : "warning"}>
                  {t(record.level === "error" ? "badge.error" : "badge.warning")}
                </Badge>
              )}
              <span className="umbra-log-text">{formatRecord(record)}</span>
            </span>
          );
        }}
      />
    </div>
  );
}
