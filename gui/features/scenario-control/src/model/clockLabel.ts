import type { Clock } from "@umbra/engine-stream";
import { t } from "../messages.ts";
import { formatScale } from "./clockScale.ts";

export function clockLabel(clock: Clock): string {
  switch (clock.mode) {
    case "afap":
      return t("clock.afap");
    case "realtime":
      return t("clock.realtime");
    case "stepped":
      return t("clock.stepped");
    case "scaled":
      return t("clock.scaledLabel", { scale: formatScale(clock.scale) });
    default:
      return t("clock.unknown");
  }
}
