import type { KeyboardEvent, ReactElement, ReactNode } from "react";
import "./EntityList.css";

export interface EntityListProps {
  label: string;
  className?: string;
  children?: ReactNode;
}

/**
 * A listbox of `EntityRow`s. Each row stays a Tab stop (as in the reference); Up, Down, Home and End
 * additionally move focus between rows. Moving focus never changes the selection. Virtualization and
 * a single roving tab stop arrive with the real list in M8 (ASM-337).
 */
export function EntityList({ label, className, children }: EntityListProps): ReactElement {
  const classes = ["vt-list", className].filter(Boolean).join(" ");

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>): void {
    const options = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="option"]')];
    const current = options.findIndex((o) => o === document.activeElement);
    if (current === -1) return;
    const last = options.length - 1;
    let to: number;
    switch (e.key) {
      case "ArrowDown":
        to = Math.min(current + 1, last);
        break;
      case "ArrowUp":
        to = Math.max(current - 1, 0);
        break;
      case "Home":
        to = 0;
        break;
      case "End":
        to = last;
        break;
      default:
        return;
    }
    e.preventDefault();
    options[to]?.focus();
  }

  return (
    <div className={classes} role="listbox" aria-label={label} onKeyDown={onKeyDown}>
      {children}
    </div>
  );
}
