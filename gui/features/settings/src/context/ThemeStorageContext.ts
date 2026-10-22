import { createContext } from "react";

export type ThemeStorage = Pick<Storage, "getItem" | "setItem">;

export const ThemeStorageContext = createContext<ThemeStorage>({
  getItem: () => null,
  setItem: () => {},
});
