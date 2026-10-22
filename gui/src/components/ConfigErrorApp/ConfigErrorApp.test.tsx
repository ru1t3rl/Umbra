import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { ConfigErrorApp } from "./ConfigErrorApp.tsx";

afterEach(cleanup);

test("the config error app shows the configuration alert", () => {
  render(<ConfigErrorApp />);
  expect(screen.getByRole("alert").textContent).toContain(
    "Configuration could not be read. Check /config.json.",
  );
});
