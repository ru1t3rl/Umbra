import { type RefObject, useLayoutEffect, useState } from "react";

const px = (value: string): number => {
  const n = Number.parseFloat(value);
  return Number.isFinite(n) && n > 0 ? n : 0;
};

/** The space a bar covers from the top of the viewport: its margin above and below plus its height. */
export function barInsetFrom(style: Pick<CSSStyleDeclaration, "getPropertyValue">): number {
  return (
    px(style.getPropertyValue("margin-top")) +
    px(style.getPropertyValue("--vt-cmdbar-height")) +
    px(style.getPropertyValue("margin-bottom"))
  );
}

/**
 * Pixels the floating CommandBar covers from the top, read from the rendered bar (its ui-kit height
 * `--vt-cmdbar-height` and its margins), so no 48px is repeated here. 0 until measured.
 */
export function useBarInset(ref: RefObject<HTMLElement | null>): number {
  const [inset, setInset] = useState(0);
  useLayoutEffect(() => {
    const bar = ref.current?.firstElementChild;
    if (!(bar instanceof HTMLElement)) return;
    const measure = (): void => setInset(barInsetFrom(getComputedStyle(bar)));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(bar);
    return () => observer.disconnect();
  }, [ref]);
  return inset;
}
