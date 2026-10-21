import { expect, test } from "bun:test";
import { streamUrl } from "./url.ts";

test("an empty engine URL means the page origin", () => {
  expect(streamUrl("", "http://localhost:5173")).toBe(
    "ws://localhost:5173/api/v1/stream?encoding=json",
  );
  expect(streamUrl("", "https://umbra.test")).toBe("wss://umbra.test/api/v1/stream?encoding=json");
});
test("http becomes ws, https becomes wss, a trailing slash is dropped", () => {
  expect(streamUrl("http://127.0.0.1:7878/", "http://x")).toBe(
    "ws://127.0.0.1:7878/api/v1/stream?encoding=json",
  );
  expect(streamUrl("https://e.test", "http://x")).toBe("wss://e.test/api/v1/stream?encoding=json");
});
