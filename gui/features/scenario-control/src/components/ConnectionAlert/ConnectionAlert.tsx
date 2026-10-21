import { useStreamStatus } from "@umbra/engine-stream";
import { Alert } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { connectionMessage } from "../../model/connectionMessage.ts";

/** A banner for a stream that is not open; nothing while it is. The shell anchors it below the CommandBar. */
export function ConnectionAlert(): ReactElement | null {
  const message = connectionMessage(useStreamStatus());
  return message ? (
    <Alert tone={message.tone} title={message.title}>
      {message.body}
    </Alert>
  ) : null;
}
