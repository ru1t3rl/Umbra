import { afterEach, expect, test } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { ReactElement } from "react";
import { useConnection } from "./useConnection.ts";

afterEach(cleanup);

test("a hook outside the provider throws a clear error", () => {
  function Lonely(): ReactElement {
    return <p>{useConnection().getState().status}</p>;
  }
  const spy = console.error;
  console.error = () => {};
  try {
    expect(() => render(<Lonely />)).toThrow("EngineStreamProvider is missing");
  } finally {
    console.error = spy;
  }
});
