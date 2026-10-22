import type { ReactElement } from "react";
import { useNavigation } from "../../hooks/useNavigation.ts";
import { AppLink } from "../AppLink/AppLink.tsx";
import { Icon } from "../Icon/Icon.tsx";
import "./AppNav.css";

export interface AppNavProps {
  /** Accessible name of the landmark. */
  label?: string;
  className?: string;
}

export function AppNav({ label = "Pages", className }: AppNavProps): ReactElement {
  const { items, currentPath } = useNavigation();
  const classes = ["vt-nav", className].filter(Boolean).join(" ");
  return (
    <nav className={classes} aria-label={label}>
      <ul className="vt-nav-list">
        {items.map((item) => (
          <li key={item.path}>
            <AppLink
              to={item.path}
              className="vt-nav-link"
              aria-current={item.path === currentPath ? "page" : undefined}
            >
              {item.icon ? <Icon name={item.icon} /> : null}
              {item.label}
            </AppLink>
          </li>
        ))}
      </ul>
    </nav>
  );
}
