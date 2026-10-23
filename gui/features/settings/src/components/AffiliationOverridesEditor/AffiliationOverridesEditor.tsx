import { Button, Select, Table, type TableColumn, TextField } from "@umbra/ui-kit";
import { type ReactElement, useEffect, useId, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings.ts";
import { useUpdateSettings } from "../../hooks/useUpdateSettings.ts";
import { t } from "../../messages.ts";
import {
  AFFILIATIONS,
  commitForce,
  isAffiliation,
  mapFromRows,
  type OverrideRow,
  type RowError,
  rejudgeDuplicates,
  rowsFromMap,
} from "../../model/overrides.ts";
import { MAX_OVERRIDES } from "../../model/settings.ts";
import "./AffiliationOverridesEditor.css";

type Line = OverrideRow & { actions: string };

const ERROR_TEXT: Record<RowError, string> = {
  empty: t("overrides.errorEmpty"),
  duplicate: t("overrides.errorDuplicate"),
  reserved: t("overrides.errorReserved"),
  tooLong: t("overrides.errorTooLong"),
};
const OPTIONS = AFFILIATIONS.map((value) => ({ value, label: t(`affiliation.${value}`) }));

/**
 * Edits `settings.map.affiliationOverrides`: the stored map is exactly the valid rows (ASM-995 f). The rows live in
 * a ref as well as in state, so a handler that runs late (the blur of a Force field that is being removed) reads
 * the current rows, not the ones of its render.
 */
export function AffiliationOverridesEditor(): ReactElement {
  const update = useUpdateSettings();
  const ids = useId();
  const initial = useSettings().map.affiliationOverrides;
  const [rows, setRows] = useState<OverrideRow[]>(() => rowsFromMap(initial));
  const current = useRef(rows);
  const nextId = useRef(rows.length);
  const focusTarget = useRef<string | null>(null);

  const forceId = (id: number): string => `${ids}-force-${id}`;
  const addId = `${ids}-add`;

  // Moves focus after the render that made its target exist (a new row, the Add button after a removal).
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs after each change of the rows
  useEffect(() => {
    if (focusTarget.current === null) return;
    document.getElementById(focusTarget.current)?.focus();
    focusTarget.current = null;
  }, [rows]);

  const apply = (changed: OverrideRow[]): void => {
    const next = rejudgeDuplicates(changed);
    current.current = next;
    setRows(next);
    update({ map: { affiliationOverrides: mapFromRows(next) } });
  };

  const add = (): void => {
    const id = nextId.current++;
    focusTarget.current = forceId(id);
    const row: OverrideRow = { id, text: "", key: null, affiliation: "unknown", error: null };
    const next = [...current.current, row];
    current.current = next;
    setRows(next);
  };

  const remove = (id: number): void => {
    focusTarget.current = addId;
    apply(current.current.filter((r) => r.id !== id));
  };

  const edit = (id: number, patch: Partial<OverrideRow>): void => {
    if (!current.current.some((r) => r.id === id)) return;
    const next = current.current.map((r) => (r.id === id ? { ...r, ...patch } : r));
    current.current = next;
    setRows(next);
  };

  const commit = (id: number, text: string): void => {
    if (!current.current.some((r) => r.id === id)) return;
    apply(commitForce(current.current, id, text));
  };

  const rowId = (id: number): string => `${ids}-row-${id}`;
  const rowText = (id: number): string =>
    t("overrides.row", { row: rows.findIndex((r) => r.id === id) + 1, count: rows.length });

  const columns: TableColumn<Line>[] = [
    {
      key: "text",
      label: t("overrides.colForce"),
      render: (row) => (
        <>
          <span id={rowId(row.id)} className="vt-visually-hidden">
            {rowText(row.id)}
          </span>
          <TextField
            id={forceId(row.id)}
            aria-describedby={rowId(row.id)}
            label={t("overrides.force")}
            value={row.text}
            invalid={row.error !== null}
            {...(row.error ? { help: ERROR_TEXT[row.error] } : {})}
            onChange={(e) => edit(row.id, { text: e.target.value })}
            onBlur={(e) => commit(row.id, e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit(row.id, e.currentTarget.value);
            }}
          />
        </>
      ),
    },
    {
      key: "affiliation",
      label: t("overrides.colAffiliation"),
      render: (row) => (
        <Select
          label={t("overrides.affiliation")}
          aria-describedby={rowId(row.id)}
          options={OPTIONS}
          value={row.affiliation}
          onChange={(value) => {
            if (!isAffiliation(value)) return;
            apply(current.current.map((r) => (r.id === row.id ? { ...r, affiliation: value } : r)));
          }}
        />
      ),
    },
    {
      key: "actions",
      label: t("overrides.colActions"),
      render: (row) => (
        <Button
          variant="ghost"
          size="sm"
          icon="delete"
          aria-label={
            row.text.trim() === ""
              ? t("overrides.removeNew")
              : t("overrides.remove", { force: row.text.trim() })
          }
          onClick={() => remove(row.id)}
        />
      ),
    },
  ];

  return (
    <div className="overrides">
      <Table
        caption={t("overrides.caption")}
        columns={columns}
        rows={rows.map((r) => ({ ...r, actions: "" }))}
        rowKey={(r) => String(r.id)}
        empty={t("overrides.empty")}
        dense
      />
      <Button id={addId} icon="add" disabled={rows.length >= MAX_OVERRIDES} onClick={add}>
        {t("overrides.add")}
      </Button>
    </div>
  );
}
