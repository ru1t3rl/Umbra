import { useContext } from "react";
import { NavigationContext, type NavigationContextValue } from "../context/NavigationContext.ts";

export function useNavigation(): NavigationContextValue {
  const ctx = useContext(NavigationContext);
  if (!ctx)
    throw new Error("navigation hooks and components must be used inside <NavigationProvider>");
  return ctx;
}
