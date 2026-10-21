import { expect, test } from "bun:test";
import { render, screen } from "@testing-library/react";
import { useRuntimeConfig } from "./useRuntimeConfig.ts";

function Probe() {
  const config = useRuntimeConfig();
  return <p>{`${config.engineUrl}|${config.basemap?.styleUrl ?? "none"}`}</p>;
}

test("without a provider the defaults apply", () => {
  render(<Probe />);
  expect(screen.getByText("|none")).toBeTruthy();
});
