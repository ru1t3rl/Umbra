import type { ReactElement } from "react";
import { useNavigation } from "../../hooks/useNavigation.ts";
import { AppLink } from "../AppLink/AppLink.tsx";
import { Icon } from "../Icon/Icon.tsx";
import "./AppNav.css";

export interface AppNavProps {
  /** Accessible name of the landmark. */
  label?: string;
  className?: string;
  /**
   * Show only the icon of each link (a link without an icon keeps its text). The link keeps its accessible name
   * (`aria-label`) and gets a `title`.
   */
  iconOnly?: boolean;
}

export function AppNav({
  label = "Pages",
  className,
  iconOnly = false,
}: AppNavProps): ReactElement {
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
              {...(iconOnly && item.icon ? { "aria-label": item.label, title: item.label } : {})}
            >
              {item.icon ? <Icon name={item.icon} /> : null}
              {iconOnly && item.icon ? null : item.label}
            </AppLink>
          </li>
        ))}
      </ul>
    </nav>
  );
}
