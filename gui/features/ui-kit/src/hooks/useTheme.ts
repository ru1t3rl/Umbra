import { useContext } from "react";
import { ThemeContext, type ThemeContextValue } from "../context/ThemeContext.ts";

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside <ThemeProvider>");
  return ctx;
}
