import { type RefObject, useEffect, useLayoutEffect, useRef, useState } from "react";

/** Reports the height of an element; `cleanup` is called on unmount. */
export type ObserveSize = (el: Element, onSize: (height: number) => void) => () => void;

/** Default size observer: ResizeObserver on the element's content box. */
export const observeResize: ObserveSize = (el, onSize) => {
  if (typeof ResizeObserver === "undefined") return () => {};
  const ro = new ResizeObserver((entries) => {
    const last = entries[entries.length - 1];
    if (last) onSize(last.contentRect.height);
  });
  ro.observe(el);
  return () => ro.disconnect();
};

/**
 * Viewport height of `ref`. It is measured synchronously before the first paint, then kept current by
 * `observe`; 0 until something reports (ASM-873). `observe` is held in a ref, so a new function identity
 * does not resubscribe.
 */
export function useElementHeight(
  ref: RefObject<Element | null>,
  observe: ObserveSize = observeResize,
): number {
  const [height, setHeight] = useState(0);
  const observeRef = useRef(observe);
  useLayoutEffect(() => {
    observeRef.current = observe;
  });
  useLayoutEffect(() => {
    // clientHeight excludes borders, like the observer's contentRect (`.vt-vlist` has no padding).
    const h = ref.current?.clientHeight ?? 0;
    if (h > 0) setHeight(h);
  }, [ref]);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    return observeRef.current(el, setHeight);
  }, [ref]);
  return height;
}
