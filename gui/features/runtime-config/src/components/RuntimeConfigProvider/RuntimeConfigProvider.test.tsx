import { expect, test } from "bun:test";
import { render, screen } from "@testing-library/react";
import { useRuntimeConfig } from "../../hooks/useRuntimeConfig.ts";
import { RuntimeConfigProvider } from "./RuntimeConfigProvider.tsx";

function Probe() {
  const config = useRuntimeConfig();
  return <p>{`${config.engineUrl}|${config.basemap?.styleUrl ?? "none"}`}</p>;
}

test("useRuntimeConfig returns the provided config", () => {
  render(
    <RuntimeConfigProvider config={{ engineUrl: "http://e", basemap: { styleUrl: "s.json" } }}>
      <Probe />
    </RuntimeConfigProvider>,
  );
  expect(screen.getByText("http://e|s.json")).toBeTruthy();
});
