import { createContext } from "react";

export interface NavItem {
  /** Absolute path, matched exactly against `currentPath`. */
  path: string;
  label: string;
  /** Material Symbols ligature name; decorative. */
  icon?: string;
}

export interface NavigationContextValue {
  items: readonly NavItem[];
  currentPath: string;
  navigate: (to: string) => void;
}

/** The only router seam features see (ADR 0046): the app supplies `navigate`, ui-kit never imports a router. */
export const NavigationContext = createContext<NavigationContextValue | undefined>(undefined);
