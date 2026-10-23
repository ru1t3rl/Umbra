import type { NavItem } from "@umbra/ui-kit";
import { t } from "./messages.ts";

/** The sample navigation entries of the AppNav demos. */
export const PAGES: NavItem[] = [
  { path: "/", label: t("nav.live-map"), icon: "map" },
  { path: "/templates", label: t("nav.templates"), icon: "dataset" },
  { path: "/plugins", label: t("nav.plugins"), icon: "extension" },
];
