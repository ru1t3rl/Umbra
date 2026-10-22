import type { ReactElement, ReactNode } from "react";
import { useSessionInvalidation } from "../../hooks/useSessionInvalidation.ts";

export interface SessionInvalidatorProps {
  children: ReactNode;
}

/** Boundary that keeps engine data fresh across run sessions; it renders its children (ASM-885). */
export function SessionInvalidator({ children }: SessionInvalidatorProps): ReactElement {
  useSessionInvalidation();
  return <>{children}</>;
}
