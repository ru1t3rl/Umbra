import { expect, test } from "bun:test";
import type { StreamStatus } from "@umbra/engine-stream";
import { connectionMessage } from "./connectionMessage.ts";

test("open has no message", () => {
  expect(connectionMessage("open")).toBeNull();
});

test("each other status has a tone and text", () => {
  expect(connectionMessage("connecting")).toEqual({
    tone: "info",
    title: "Connecting to the engine",
    body: "Live data appears when the connection is up.",
  });
  expect(connectionMessage("reconnecting")).toEqual({
    tone: "warning",
    title: "Engine not reachable",
    body: "Reconnecting. Live data resumes when the engine answers.",
  });
  expect(connectionMessage("closed")?.tone).toBe("warning");
  expect(connectionMessage("closed")?.body).toContain("Reload the page");
});

test("incompatible is danger and says to reload (ASM-356)", () => {
  const m = connectionMessage("incompatible");
  expect(m?.tone).toBe("danger");
  expect(m?.body).toContain("Reload the page");
});

test("an unknown future status is never hidden", () => {
  expect(connectionMessage("future" as StreamStatus)?.tone).toBe("warning");
});

test("no text has an exclamation mark", () => {
  for (const s of ["connecting", "reconnecting", "closed", "incompatible"] as const) {
    const m = connectionMessage(s);
    expect(`${m?.title}${m?.body}`).not.toContain("!");
  }
});
