import { afterEach, expect, test } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { useTheme } from "./useTheme.ts";

afterEach(cleanup);

function Probe() {
  return <p>{useTheme().resolved}</p>;
}

test("useTheme outside a provider throws", () => {
  const err = console.error;
  console.error = () => {};
  try {
    expect(() => render(<Probe />)).toThrow("useTheme must be used inside <ThemeProvider>");
  } finally {
    console.error = err;
  }
});
