// WI-14 review round 2 (ASM-988): the compact (icon-only) theme switch has a showcase entry.
import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { ThemeProvider } from "@umbra/ui-kit";
import ShowcasePage from "./ShowcasePage.tsx";

afterEach(cleanup);

test("the floating bar sample shows an icon-only theme switch next to the docked bar's labelled one", () => {
  render(
    <ThemeProvider>
      <ShowcasePage />
    </ThemeProvider>,
  );
  const compact = screen.getByRole("switch", { name: "Compact theme switch" });
  expect(compact).not.toHaveTextContent("Day theme");
  expect(compact.closest(".vt-cmdbar-floating")).not.toBeNull();
  expect(screen.getByRole("switch", { name: "Day theme" })).toHaveTextContent("Day theme");
});
