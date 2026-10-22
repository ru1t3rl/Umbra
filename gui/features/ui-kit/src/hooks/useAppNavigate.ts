import { useNavigation } from "./useNavigation.ts";

/** Returns the app's navigate function; throws outside a <NavigationProvider>. */
export function useAppNavigate(): (to: string) => void {
  return useNavigation().navigate;
}
