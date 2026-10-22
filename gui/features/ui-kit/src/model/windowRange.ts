export interface WindowRange {
  start: number;
  /** Exclusive. */
  end: number;
}

/** Rows to render for a scroll offset: the visible rows padded by `overscan` on both sides (ASM-871). */
export function windowRange(
  scrollTop: number,
  viewport: number,
  rowHeight: number,
  count: number,
  overscan: number,
): WindowRange {
  if (count <= 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const maxTop = Math.max(0, count * rowHeight - viewport);
  const top = Math.min(Math.max(0, scrollTop), maxTop);
  const start = Math.max(0, Math.floor(top / rowHeight) - overscan);
  const end = Math.min(count, Math.ceil((top + viewport) / rowHeight) + overscan);
  return { start: Math.min(start, end), end };
}
