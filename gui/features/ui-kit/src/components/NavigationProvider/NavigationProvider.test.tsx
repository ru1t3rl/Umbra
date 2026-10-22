import { afterEach, expect, test } from "bun:test";
import { cleanup, render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { useAppNavigate } from "../../hooks/useAppNavigate.ts";
import { NavigationProvider } from "./NavigationProvider.tsx";

afterEach(cleanup);

test("useAppNavigate returns the navigate function the provider was given", () => {
  const calls: string[] = [];
  const navigate = (to: string) => calls.push(to);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <NavigationProvider items={[]} currentPath="/" navigate={navigate}>
      {children}
    </NavigationProvider>
  );
  const { result } = renderHook(() => useAppNavigate(), { wrapper });
  result.current("/plugins");
  expect(calls).toEqual(["/plugins"]);
});

test("a component can navigate from a button through the hook", async () => {
  const user = userEvent.setup();
  const calls: string[] = [];
  function Go() {
    const navigate = useAppNavigate();
    return (
      <button type="button" onClick={() => navigate("/settings")}>
        Go
      </button>
    );
  }
  render(
    <NavigationProvider items={[]} currentPath="/" navigate={(to) => calls.push(to)}>
      <Go />
    </NavigationProvider>,
  );
  await user.click(screen.getByRole("button", { name: "Go" }));
  expect(calls).toEqual(["/settings"]);
});

test("useAppNavigate outside a provider fails loudly instead of doing nothing", () => {
  const original = console.error;
  console.error = () => {};
  try {
    expect(() => renderHook(() => useAppNavigate())).toThrow();
  } finally {
    console.error = original;
  }
});
