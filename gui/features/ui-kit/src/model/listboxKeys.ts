/** Next active index for a navigation key, or null when the key is not one (ASM-874). */
export function nextActiveIndex(
  key: string,
  active: number | null,
  count: number,
  pageSize: number,
): number | null {
  if (count <= 0) return null;
  const last = count - 1;
  const from = active ?? -1;
  const clamp = (n: number) => Math.min(last, Math.max(0, n));
  switch (key) {
    case "ArrowDown":
      return clamp(from + 1);
    case "ArrowUp":
      return clamp(from - 1);
    case "PageDown":
      return clamp(from + Math.max(1, pageSize));
    case "PageUp":
      return clamp(from - Math.max(1, pageSize));
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return null;
  }
}

/** Scroll offset that brings row `index` fully into view, or `top` when it already is. */
export function scrollToReveal(
  index: number,
  rowHeight: number,
  top: number,
  viewport: number,
): number {
  const rowTop = index * rowHeight;
  if (rowTop < top) return rowTop;
  const rowBottom = rowTop + rowHeight;
  if (rowBottom > top + viewport) return Math.max(0, rowBottom - viewport);
  return top;
}
