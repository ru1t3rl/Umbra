import { type NavItem, NavigationProvider } from "@umbra/ui-kit";
import { type ReactElement, useCallback } from "react";
import { Outlet, useLocation, useNavigate } from "react-router";

export interface NavigationBridgeProps {
  items: readonly NavItem[];
}

/** Maps the router into ui-kit's navigation context (ADR 0073); pages render `AppNav` themselves. */
export function NavigationBridge({ items }: NavigationBridgeProps): ReactElement {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const go = useCallback((to: string) => void navigate(to), [navigate]);
  return (
    <NavigationProvider items={items} currentPath={pathname} navigate={go}>
      <Outlet />
    </NavigationProvider>
  );
}
