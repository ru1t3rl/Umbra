import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { EngineStreamProvider } from "@umbra/engine-stream";
import { frame, hello, rig } from "../../testing.ts";
import { ConnectionAlert } from "./ConnectionAlert.tsx";

afterEach(cleanup);

function setup() {
  const r = rig();
  render(
    <EngineStreamProvider connection={r.connection}>
      <ConnectionAlert />
    </EngineStreamProvider>,
  );
  return r;
}

test("connecting shows an info status, open hides it, a drop shows the warning alert", () => {
  const r = setup();
  expect(screen.getByRole("status", { name: "Connecting to the engine" })).toBeInTheDocument();
  act(() => r.push(hello()));
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  act(() => r.drop());
  const alert = screen.getByRole("alert", { name: "Engine not reachable" });
  expect(alert).toHaveTextContent("Reconnecting.");
  act(() => r.reconnect());
  act(() => r.push(hello()));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("an incompatible server asks for a reload", () => {
  const r = setup();
  act(() => r.push(hello("umbra.stream.v2")));
  const alert = screen.getByRole("alert", { name: "Engine version not supported" });
  expect(alert).toHaveTextContent("Reload the page");
});

test("a frame after hello keeps it hidden", () => {
  const r = setup();
  act(() => r.push(hello(), frame(0, 1)));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
});

test("a client close is a terminal warning that asks for a reload", () => {
  const r = setup();
  act(() => r.push(hello()));
  act(() => r.connection.close());
  const alert = screen.getByRole("alert", { name: "Connection closed" });
  expect(alert).toHaveTextContent("Reload the page");
});
