import { type ReactElement, type ReactNode, useMemo } from "react";
import type { NavItem } from "../../context/NavigationContext.ts";
import { NavigationContext } from "../../context/NavigationContext.ts";

export type { NavItem };

export interface NavigationProviderProps {
  items: readonly NavItem[];
  currentPath: string;
  navigate: (to: string) => void;
  children: ReactNode;
}

export function NavigationProvider({
  items,
  currentPath,
  navigate,
  children,
}: NavigationProviderProps): ReactElement {
  const value = useMemo(() => ({ items, currentPath, navigate }), [items, currentPath, navigate]);
  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}
