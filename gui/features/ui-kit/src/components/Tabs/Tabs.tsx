import { type KeyboardEvent, type ReactElement, useRef } from "react";
import "./Tabs.css";

export interface TabItem {
  value: string;
  label: string;
  /** Mono count for lists that change at runtime; above 9999 it shows "9999+". */
  count?: number;
}

export interface TabsProps {
  items: TabItem[];
  value: string;
  onChange?: (value: string) => void;
  /** Accessible name of the tablist. */
  label?: string;
  /** Derives ids: tab `${id}-tab-${value}` with `aria-controls` `${id}-panel-${value}`; give the panel that id. */
  id?: string;
  className?: string;
}

const COUNT_MAX = 9999;

export function Tabs({ items, value, onChange, label, id, className }: TabsProps): ReactElement {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const selectedIndex = items.findIndex((it) => it.value === value);
  const tabbable = selectedIndex === -1 ? 0 : selectedIndex;

  function move(to: number): void {
    const target = items[to];
    if (!target) return;
    refs.current[to]?.focus();
    onChange?.(target.value);
  }

  function onKeyDown(e: KeyboardEvent<HTMLButtonElement>, index: number): void {
    const last = items.length - 1;
    switch (e.key) {
      case "ArrowRight":
        move(index === last ? 0 : index + 1);
        break;
      case "ArrowLeft":
        move(index === 0 ? last : index - 1);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(last);
        break;
      default:
        return;
    }
    e.preventDefault();
  }

  const classes = ["vt-tabs", className].filter(Boolean).join(" ");
  return (
    <div className={classes} role="tablist" aria-label={label}>
      {items.map((it, i) => (
        <button
          key={it.value}
          ref={(el) => {
            refs.current[i] = el;
          }}
          type="button"
          role="tab"
          id={id ? `${id}-tab-${it.value}` : undefined}
          aria-controls={id ? `${id}-panel-${it.value}` : undefined}
          aria-selected={it.value === value}
          tabIndex={i === tabbable ? 0 : -1}
          className="vt-tab"
          onClick={() => onChange?.(it.value)}
          onKeyDown={(e) => onKeyDown(e, i)}
        >
          {it.label}
          {it.count != null ? (
            <>
              {" "}
              <span className="vt-tab-count">
                {it.count > COUNT_MAX ? `${COUNT_MAX}+` : it.count}
              </span>
            </>
          ) : null}
        </button>
      ))}
    </div>
  );
}
