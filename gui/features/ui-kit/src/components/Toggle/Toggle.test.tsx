import { afterEach, expect, spyOn, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toggle } from "./Toggle.tsx";

afterEach(cleanup);

test("is a switch named by its label, reflecting checked", () => {
  const { rerender } = render(<Toggle checked={false} label="Record session" />);
  const sw = screen.getByRole("switch", { name: "Record session" });
  expect(sw).toHaveClass("vt-toggle");
  expect(sw).toHaveAttribute("aria-checked", "false");
  expect(sw.querySelector(".vt-toggle-track .vt-toggle-thumb")).not.toBeNull();
  rerender(<Toggle checked label="Record session" />);
  expect(sw).toHaveAttribute("aria-checked", "true");
});

test("Space and Enter request the opposite value; click too", async () => {
  const user = userEvent.setup();
  const seen: boolean[] = [];
  render(<Toggle checked={false} label="Layer" onChange={(n) => seen.push(n)} />);
  await user.tab();
  await user.keyboard(" ");
  await user.keyboard("{Enter}");
  await user.click(screen.getByRole("switch"));
  expect(seen).toEqual([true, true, true]);
});

test("a checked toggle requests false", async () => {
  const user = userEvent.setup();
  const seen: boolean[] = [];
  render(<Toggle checked label="Layer" onChange={(n) => seen.push(n)} />);
  await user.click(screen.getByRole("switch"));
  expect(seen).toEqual([false]);
});

test("disabled blocks input", async () => {
  const user = userEvent.setup();
  let calls = 0;
  render(<Toggle checked={false} disabled label="Layer" onChange={() => calls++} />);
  expect(screen.getByRole("switch")).toBeDisabled();
  await user.click(screen.getByRole("switch"));
  expect(calls).toBe(0);
});

test("a label-less toggle is named by aria-label", () => {
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  render(<Toggle checked={false} aria-label="Day theme" />);
  expect(screen.getByRole("switch", { name: "Day theme" })).toBeInTheDocument();
  expect(warn).not.toHaveBeenCalled();
  warn.mockRestore();
});

test("an unnamed toggle warns in development only", () => {
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  // @ts-expect-error deliberately violates the type to exercise the runtime guard
  render(<Toggle checked={false} />);
  expect(warn).toHaveBeenCalledTimes(1);
  expect(String(warn.mock.calls[0]?.[0])).toContain("label");
  cleanup();
  warn.mockClear();
  const prev = process.env.NODE_ENV;
  process.env.NODE_ENV = "production";
  try {
    // @ts-expect-error deliberately violates the type to exercise the runtime guard
    render(<Toggle checked={false} />);
  } finally {
    if (prev === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = prev;
  }
  expect(warn).not.toHaveBeenCalled();
  warn.mockRestore();
});

test("aria-describedby is forwarded to the switch", () => {
  render(
    <>
      <Toggle label="Enabled" checked aria-describedby="h" />
      <p id="h">Turns it on.</p>
    </>,
  );
  expect(screen.getByRole("switch", { name: "Enabled" })).toHaveAccessibleDescription(
    "Turns it on.",
  );
});
