import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { hello, rig } from "../testing.ts";
import { useStreamStatus } from "./useStreamStatus.ts";

afterEach(cleanup);

test("useStreamStatus follows the connection", () => {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  function Status(): ReactElement {
    return <p data-testid="status">{useStreamStatus()}</p>;
  }
  render(
    <EngineStreamProvider connection={connection}>
      <Status />
    </EngineStreamProvider>,
  );
  expect(screen.getByTestId("status").textContent).toBe("open");
  act(() => r.socket().drop());
  expect(screen.getByTestId("status").textContent).toBe("reconnecting");
});
