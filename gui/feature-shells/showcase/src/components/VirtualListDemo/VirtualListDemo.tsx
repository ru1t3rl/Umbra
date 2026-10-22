import { Button, VirtualList } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";

const ENTITY_COUNT = 10_000;
const LOG_START = 2_000;

/** ADR 0072: a 10,000-row listbox and a following log, both windowed; data is synthetic and index-derived. */
export function VirtualListDemo(): ReactElement {
  const [active, setActive] = useState<number | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [logCount, setLogCount] = useState(LOG_START);
  return (
    <div className="showcase-col">
      <div id="showcase-vlist-listbox" className="showcase-vlist">
        <VirtualList
          role="listbox"
          label={t("vlist.listbox")}
          count={ENTITY_COUNT}
          rowHeight="row-md"
          getRowId={(i) => `showcase-ent-${i}`}
          renderRow={(i) => t("vlist.entity", { id: String(1000 + i), n: i + 1 })}
          activeIndex={active}
          onActiveIndexChange={setActive}
          selectedIndex={selected}
          onActivate={setSelected}
        />
      </div>
      <div id="showcase-vlist-log" className="showcase-vlist">
        <VirtualList
          role="log"
          label={t("vlist.log")}
          count={logCount}
          rowHeight="row-sm"
          getRowId={(i) => `showcase-log-${i}`}
          renderRow={(i) => t("vlist.event", { n: i + 1 })}
          followEnd
        />
      </div>
      <div className="showcase-row">
        <Button size="sm" onClick={() => setLogCount((n) => n + 1)}>
          {t("vlist.add-event")}
        </Button>
      </div>
    </div>
  );
}
