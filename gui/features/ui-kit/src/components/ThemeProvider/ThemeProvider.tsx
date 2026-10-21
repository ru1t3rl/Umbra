import {
  type ReactElement,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ThemeContext } from "../../context/ThemeContext.ts";
import {
  parsePreference,
  resolveTheme,
  THEME_STORAGE_KEY,
  type ThemePreference,
} from "../../theme/model.ts";

export interface ThemeProviderProps {
  defaultPreference?: ThemePreference;
  storage?: Pick<Storage, "getItem" | "setItem">;
  children?: ReactNode;
}

const DARK_QUERY = "(prefers-color-scheme: dark)";

function defaultStorage(): Pick<Storage, "getItem" | "setItem"> | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined; // storage blocked: the preference lives in memory only
  }
}

function hasMatchMedia(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function";
}

function systemPrefersDark(): boolean {
  return hasMatchMedia() ? window.matchMedia(DARK_QUERY).matches : true;
}

export function ThemeProvider({
  defaultPreference = "dark",
  storage,
  children,
}: ThemeProviderProps): ReactElement {
  // A ref keeps setPreference stable when callers pass an inline storage object.
  const storeRef = useRef(storage ?? defaultStorage());
  storeRef.current = storage ?? storeRef.current;
  const [preference, setPreferenceState] = useState<ThemePreference>(() => {
    try {
      return parsePreference(storeRef.current?.getItem(THEME_STORAGE_KEY)) ?? defaultPreference;
    } catch {
      return defaultPreference;
    }
  });
  const [systemDark, setSystemDark] = useState<boolean>(systemPrefersDark);

  useEffect(() => {
    if (preference !== "system" || !hasMatchMedia()) return;
    const mql = window.matchMedia(DARK_QUERY);
    const onChange = () => setSystemDark(window.matchMedia(DARK_QUERY).matches);
    onChange();
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [preference]);

  const resolved = resolveTheme(preference, systemDark);

  useLayoutEffect(() => {
    document.documentElement.dataset.theme = resolved;
  }, [resolved]);

  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next);
    try {
      storeRef.current?.setItem(THEME_STORAGE_KEY, next);
    } catch {
      // quota or blocked storage: keep the in-memory choice
    }
  }, []);

  const value = useMemo(
    () => ({ preference, resolved, setPreference }),
    [preference, resolved, setPreference],
  );
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
