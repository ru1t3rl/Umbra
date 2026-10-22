import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { type ReactElement, useRef } from "react";
import { barInsetFrom, useBarInset } from "./useBarInset.ts";

afterEach(cleanup);

const style = (values: Record<string, string>) => ({
  getPropertyValue: (name: string) => values[name] ?? "",
});

describe("barInsetFrom", () => {
  test("margin above, bar height and margin below add up", () => {
    expect(
      barInsetFrom(
        style({ "margin-top": "16px", "--vt-cmdbar-height": " 48px", "margin-bottom": "16px" }),
      ),
    ).toBe(80);
  });
  test("missing or bad values count as 0", () => {
    expect(barInsetFrom(style({}))).toBe(0);
    expect(barInsetFrom(style({ "--vt-cmdbar-height": "auto", "margin-top": "-4px" }))).toBe(0);
  });
});

test("the hook reads the first child of the wrapper", () => {
  function Probe(): ReactElement {
    const ref = useRef<HTMLDivElement>(null);
    const inset = useBarInset(ref);
    return (
      <div ref={ref}>
        <header
          style={{ marginTop: 10, marginBottom: 6, ["--vt-cmdbar-height" as string]: "40px" }}
        />
        <output>{inset}</output>
      </div>
    );
  }
  render(<Probe />);
  expect(screen.getByRole("status")).toHaveTextContent("56");
});
