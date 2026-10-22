import { defineMessages } from "@umbra/ui-kit";

export const t = defineMessages({
  "readout.simTime": "Sim time",
  "readout.tick": "Tick",
  "readout.entities": "Entities",
  "readout.none": "—",
  "run.idle": "Idle",
  "run.ready": "Ready",
  "run.running": "Running",
  "run.paused": "Paused",
  "run.stopped": "Stopped",
  "run.unknown": "Unknown",
  "run.label": "Run state:",
  "connection.connecting.title": "Connecting to the engine",
  "connection.connecting.body": "Live data appears when the connection is up.",
  "connection.reconnecting.title": "Engine not reachable",
  "connection.reconnecting.body": "Reconnecting. Live data resumes when the engine answers.",
  "connection.closed.title": "Connection closed",
  "connection.closed.body": "The live connection was closed. Reload the page to reconnect.",
  "connection.incompatible.title": "Engine version not supported",
  "connection.incompatible.body":
    "This page and the engine use different stream versions. Reload the page. If this message stays, use a GUI version that matches the engine.",
});
