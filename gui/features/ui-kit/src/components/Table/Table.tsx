import type { ReactElement, ReactNode } from "react";
import "../shared/Label.css";
import "../shared/VisuallyHidden.css";
import "./Table.css";

export interface TableColumn<Row> {
  key: keyof Row & string;
  label: string;
  align?: "start" | "end";
  /** Mono cells for IDs, coordinates and times. */
  mono?: boolean;
  /** Custom cell content (a Badge, a link). Default: `String(row[key])`, empty for null and undefined. */
  render?: (row: Row) => ReactNode;
}

export interface TableProps<Row> {
  /** Accessible name of the table. */
  caption: string;
  columns: readonly TableColumn<Row>[];
  rows: readonly Row[];
  rowKey: (row: Row) => string;
  /** Keep the caption for assistive technology only. */
  captionHidden?: boolean;
  /** Shown in place of the body rows when there are none. */
  empty?: string;
  dense?: boolean;
  className?: string;
}

function cellClass(col: { align?: "start" | "end"; mono?: boolean }): string | undefined {
  return (
    [col.mono && "vt-table-mono", col.align === "end" && "vt-table-end"]
      .filter(Boolean)
      .join(" ") || undefined
  );
}

export function Table<Row>({
  caption,
  columns,
  rows,
  rowKey,
  empty,
  dense,
  captionHidden,
  className,
}: TableProps<Row>): ReactElement {
  const classes = ["vt-table", dense && "vt-table-dense", className].filter(Boolean).join(" ");
  return (
    <table className={classes}>
      <caption className={captionHidden ? "vt-visually-hidden" : "vt-table-caption"}>
        {caption}
      </caption>
      <thead>
        <tr>
          {columns.map((c) => (
            <th
              key={c.key}
              scope="col"
              className={["vt-label", c.align === "end" && "vt-table-end"]
                .filter(Boolean)
                .join(" ")}
            >
              {c.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && empty ? (
          <tr>
            <td colSpan={columns.length} className="vt-table-empty">
              {empty}
            </td>
          </tr>
        ) : (
          rows.map((row) => (
            <tr key={rowKey(row)}>
              {columns.map((c) => (
                <td key={c.key} className={cellClass(c)}>
                  {c.render ? c.render(row) : row[c.key] == null ? "" : String(row[c.key])}
                </td>
              ))}
            </tr>
          ))
        )}
      </tbody>
    </table>
  );
}
