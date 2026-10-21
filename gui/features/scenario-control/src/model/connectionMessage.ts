import type { StreamStatus } from "@umbra/engine-stream";
import type { AlertTone } from "@umbra/ui-kit";
import { t } from "../messages.ts";

export interface ConnectionMessage {
  tone: AlertTone;
  title: string;
  body: string;
}

/**
 * What the connection banner says, or `null` when the stream is open. Says what happened, then what to
 * do. `incompatible` (ASM-356) never recovers by itself, so it tells the user to reload and is danger.
 */
export function connectionMessage(status: StreamStatus): ConnectionMessage | null {
  switch (status) {
    case "open":
      return null;
    case "connecting":
      return {
        tone: "info",
        title: t("connection.connecting.title"),
        body: t("connection.connecting.body"),
      };
    case "reconnecting":
      return {
        tone: "warning",
        title: t("connection.reconnecting.title"),
        body: t("connection.reconnecting.body"),
      };
    case "closed":
      return {
        tone: "warning",
        title: t("connection.closed.title"),
        body: t("connection.closed.body"),
      };
    case "incompatible":
      return {
        tone: "danger",
        title: t("connection.incompatible.title"),
        body: t("connection.incompatible.body"),
      };
    default: {
      const unhandled: never = status;
      void unhandled;
      // A status added later: show it as a reconnect rather than hiding a broken connection.
      return {
        tone: "warning",
        title: t("connection.reconnecting.title"),
        body: t("connection.reconnecting.body"),
      };
    }
  }
}
