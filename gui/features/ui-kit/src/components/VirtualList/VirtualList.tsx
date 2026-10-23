import {
  type KeyboardEvent,
  type MouseEvent,
  type ReactElement,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { type ObserveSize, useElementHeight } from "../../hooks/useElementHeight.ts";
import { nextActiveIndex, scrollToReveal } from "../../model/listboxKeys.ts";
import { ROW_HEIGHT_PX, type RowHeightToken } from "../../model/rowHeights.ts";
import "./VirtualList.css";
import { type WindowRange, windowRange } from "../../model/windowRange.ts";

interface CommonProps {
  /** Accessible name of the list. */
  label: string;
  count: number;
  /** Fixed row height, resolved once from a token (ADR 0072). */
  rowHeight: RowHeightToken;
  /** Presentational content only: no role, button or aria-selected; the list owns the option semantics. */
  renderRow: (index: number) => ReactNode;
  getRowId: (index: number) => string;
  /** Rows rendered beyond the visible ones on each side. Default 8. */
  overscan?: number;
  /** Reports the viewport height; default uses ResizeObserver. Tests inject a fake. */
  observeSize?: ObserveSize;
  className?: string;
  /**
   * Window the rows (default true). False mounts every row with the same classes, ids, ARIA and keyboard model,
   * for short lists whose rows must all be in the DOM (ASM-953); switching it keeps the same element.
   */
  virtualize?: boolean;
}

interface ListboxProps extends CommonProps {
  role: "listbox";
  /** The row the keyboard is on (aria-activedescendant). */
  activeIndex: number | null;
  /** The chosen row (aria-selected). Selection does not follow focus (ASM-836). */
  selectedIndex?: number | null;
  onActiveIndexChange: (index: number | null) => void;
  onActivate: (index: number) => void;
}

interface LogProps extends CommonProps {
  role: "log";
  /** Keep the newest row in view until the user scrolls away from the end. */
  followEnd?: boolean;
}

export type VirtualListProps = ListboxProps | LogProps;

const DEFAULT_OVERSCAN = 8;

const sameRange = (a: WindowRange, b: WindowRange): boolean =>
  a.start === b.start && a.end === b.end;

/**
 * Fixed-height windowed list (ADR 0072): renders only the visible rows plus overscan inside a
 * sizer as tall as the whole list. The scroller is the role element and the single tab stop;
 * a listbox names its active row with aria-activedescendant and marks the selected row with
 * aria-selected. A log is not live (the app provides a throttled status region instead).
 */
export function VirtualList(props: VirtualListProps): ReactElement {
  const { label, count, rowHeight, renderRow, getRowId, observeSize, className } = props;
  const overscan = props.overscan ?? DEFAULT_OVERSCAN;
  const virtualize = props.virtualize ?? true;
  const rh = ROW_HEIGHT_PX[rowHeight];
  const ref = useRef<HTMLDivElement>(null);
  const viewport = useElementHeight(ref, observeSize);
  const top = useRef(0);
  const [range, setRange] = useState<WindowRange>(() => windowRange(0, 0, rh, count, overscan));
  const following = useRef(true);

  const listbox = props.role === "listbox" ? props : null;
  const followEnd = props.role === "log" && props.followEnd === true;
  const active =
    listbox && listbox.activeIndex !== null && listbox.activeIndex < count
      ? listbox.activeIndex
      : null;
  const selected = listbox?.selectedIndex ?? null;
  const reportActive = listbox?.onActiveIndexChange;
  const staleActive = listbox !== null && listbox.activeIndex !== null && active === null;

  /** Recompute the window; the state only changes when the range does (no render per scroll event). */
  const refresh = useCallback(() => {
    const next = windowRange(top.current, viewport, rh, count, overscan);
    setRange((prev) => (sameRange(prev, next) ? prev : next));
  }, [viewport, rh, count, overscan]);

  const scrollTo = useCallback(
    (value: number) => {
      const el = ref.current;
      if (!el) return;
      el.scrollTop = value;
      top.current = value;
      refresh();
    },
    [refresh],
  );

  useLayoutEffect(() => {
    refresh();
  }, [refresh]);

  useLayoutEffect(() => {
    if (followEnd && following.current && viewport > 0)
      scrollTo(Math.max(0, count * rh - viewport));
  }, [followEnd, count, rh, viewport, scrollTo]);

  useEffect(() => {
    if (staleActive) reportActive?.(null);
  }, [staleActive, reportActive]);

  const onScroll = (): void => {
    const el = ref.current;
    if (!el) return;
    top.current = el.scrollTop;
    refresh();
    // Before the viewport is measured the end cannot be told: leave the follow state alone.
    if (viewport > 0) following.current = count * rh - viewport - el.scrollTop <= rh;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>): void => {
    if (!listbox || count === 0) return;
    if (e.key === "Enter") {
      if (active === null) return;
      e.preventDefault();
      listbox.onActivate(active);
      return;
    }
    const next = nextActiveIndex(e.key, active, count, Math.floor(viewport / rh));
    if (next === null) return;
    e.preventDefault();
    listbox.onActiveIndexChange(next);
    const current = ref.current?.scrollTop ?? top.current;
    const revealed = scrollToReveal(next, rh, current, viewport);
    if (revealed !== current) scrollTo(revealed);
  };

  const onClick = (e: MouseEvent<HTMLDivElement>): void => {
    if (!listbox) return;
    const row = (e.target as Element).closest("[data-index]");
    const index = Number(row?.getAttribute("data-index"));
    if (!row || Number.isNaN(index)) return;
    listbox.onActiveIndexChange(index);
    listbox.onActivate(index);
  };

  const indexes: number[] = [];
  const first = virtualize ? range.start : 0;
  const end = virtualize ? Math.min(range.end, count) : count;
  for (let i = first; i < end; i++) indexes.push(i);
  // The active row stays mounted so aria-activedescendant never points at nothing.
  if (virtualize && active !== null && (active < range.start || active >= range.end)) {
    if (active < range.start) indexes.unshift(active);
    else indexes.push(active);
  }

  const rowRole = listbox ? "option" : "listitem";
  const rows = indexes.map((i) => (
    // biome-ignore lint/a11y/useAriaPropsSupportedByRole: aria-selected is set only when the role is option
    <div
      key={i}
      id={getRowId(i)}
      className="vt-vlist-row type-data"
      role={rowRole}
      aria-selected={listbox ? i === selected : undefined}
      aria-setsize={count}
      aria-posinset={i + 1}
      data-index={i}
      data-active={i === active ? "" : undefined}
      style={{ top: i * rh, height: rh }}
    >
      {renderRow(i)}
    </div>
  ));

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions lint/a11y/useAriaPropsSupportedByRole: the role is listbox or log (a union); keys are handled here for aria-activedescendant
    <div
      ref={ref}
      role={props.role}
      aria-label={label}
      aria-live={listbox ? undefined : "off"}
      aria-activedescendant={active === null ? undefined : getRowId(active)}
      tabIndex={listbox && count === 0 ? -1 : 0}
      className={["vt-vlist", `vt-vlist-${props.role}`, className].filter(Boolean).join(" ")}
      onScroll={onScroll}
      onKeyDown={onKeyDown}
    >
      {/* biome-ignore lint/a11y/noStaticElementInteractions lint/a11y/useKeyWithClickEvents: one delegated pointer handler; the keyboard is on the scroller */}
      <div
        data-virtual-sizer
        role={listbox ? undefined : "list"}
        className="vt-vlist-sizer"
        style={{ height: count * rh }}
        onClick={onClick}
      >
        {rows}
      </div>
    </div>
  );
}
