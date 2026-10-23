import { Badge } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { type DiagnosticView, MAX_DIAGNOSTICS } from "../../model/diagnostics.ts";
import "./DiagnosticList.css";

/**
 * Located diagnostics in server order: location, code, message, a severity Badge and notes (ASM-813). At most
 * `MAX_DIAGNOSTICS` are listed, then one line counts the rest (ASM-931). Not a live region itself: render it
 * beside the Alert, not inside.
 */
export function DiagnosticList({
  items,
}: {
  items: readonly DiagnosticView[];
}): ReactElement | null {
  if (items.length === 0) return null;
  const shown = items.slice(0, MAX_DIAGNOSTICS);
  const hidden = items.length - shown.length;
  return (
    <div className="umbra-diagnostics">
      <ul className="umbra-diagnostics-list" aria-label={t("diagnostics.label")}>
        {shown.map((d, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static, server-ordered list; code and location can repeat
          <li key={`${d.code}:${d.location}:${i}`} className="umbra-diagnostic">
            <span className="umbra-diagnostic-head">
              <Badge tone={d.severity === "warning" ? "warning" : "danger"}>
                {t(`diagnostics.severity.${d.severity}`)}
              </Badge>
              {d.location ? <code className="type-data">{d.location}</code> : null}
              <code className="type-data">{d.code}</code>
            </span>
            <span className="umbra-diagnostic-message">{d.message}</span>
            {d.notes.map((n, j) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: static notes; locations can repeat
              <span key={`${n.location}:${j}`} className="umbra-diagnostic-note type-caption">
                {n.location ? `${n.location}: ${n.message}` : n.message}
              </span>
            ))}
          </li>
        ))}
      </ul>
      {hidden > 0 ? (
        <p className="umbra-diagnostic-more type-caption">
          {t("diagnostics.more", { count: hidden })}
        </p>
      ) : null}
    </div>
  );
}
