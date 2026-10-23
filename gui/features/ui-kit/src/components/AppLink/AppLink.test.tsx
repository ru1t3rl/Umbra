import { afterEach, expect, test } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NavigationProvider } from "../NavigationProvider/NavigationProvider.tsx";
import { AppLink } from "./AppLink.tsx";

afterEach(cleanup);

function renderLink(calls: string[]) {
  return render(
    <NavigationProvider items={[]} currentPath="/" navigate={(to) => calls.push(to)}>
      <AppLink to="/settings">Settings</AppLink>
    </NavigationProvider>,
  );
}

test("renders an anchor whose href is the target", () => {
  renderLink([]);
  expect(screen.getByRole("link", { name: "Settings" })).toHaveAttribute("href", "/settings");
});

test("a plain click calls navigate and prevents the browser navigation", async () => {
  const calls: string[] = [];
  renderLink(calls);
  const link = screen.getByRole("link", { name: "Settings" });
  const notPrevented = fireEvent.click(link); // false when preventDefault was called
  expect(calls).toEqual(["/settings"]);
  expect(notPrevented).toBe(false);
});

test("a user-event click calls navigate once", async () => {
  const user = userEvent.setup();
  const calls: string[] = [];
  renderLink(calls);
  await user.click(screen.getByRole("link", { name: "Settings" }));
  expect(calls).toEqual(["/settings"]);
});

test("a ctrl click does not call navigate and keeps the browser default", () => {
  const calls: string[] = [];
  renderLink(calls);
  const notPrevented = fireEvent.click(screen.getByRole("link"), { ctrlKey: true });
  expect(calls).toEqual([]);
  expect(notPrevented).toBe(true);
});

test("a meta click does not call navigate and keeps the browser default", () => {
  const calls: string[] = [];
  renderLink(calls);
  const notPrevented = fireEvent.click(screen.getByRole("link"), { metaKey: true });
  expect(calls).toEqual([]);
  expect(notPrevented).toBe(true);
});

test("shift, alt and middle-button clicks are left to the browser", () => {
  const calls: string[] = [];
  renderLink(calls);
  const link = screen.getByRole("link");
  fireEvent.click(link, { shiftKey: true });
  fireEvent.click(link, { altKey: true });
  fireEvent.click(link, { button: 1 });
  expect(calls).toEqual([]);
});

test("native anchor attributes and className pass through", () => {
  render(
    <NavigationProvider items={[]} currentPath="/" navigate={() => {}}>
      <AppLink to="/x" className="extra" title="Hint">
        X
      </AppLink>
    </NavigationProvider>,
  );
  const link = screen.getByRole("link", { name: "X" });
  expect(link).toHaveClass("extra");
  expect(link).toHaveAttribute("title", "Hint");
});

test("a caller onClick runs exactly once on a plain click", () => {
  let n = 0;
  render(
    <NavigationProvider items={[]} currentPath="/" navigate={() => {}}>
      <AppLink to="/x" onClick={() => n++}>
        X
      </AppLink>
    </NavigationProvider>,
  );
  fireEvent.click(screen.getByRole("link"));
  expect(n).toBe(1);
});

test("a caller onClick that prevents default suppresses navigate", () => {
  const calls: string[] = [];
  render(
    <NavigationProvider items={[]} currentPath="/" navigate={(to) => calls.push(to)}>
      <AppLink to="/x" onClick={(e) => e.preventDefault()}>
        X
      </AppLink>
    </NavigationProvider>,
  );
  fireEvent.click(screen.getByRole("link"));
  expect(calls).toEqual([]);
});

test("target _blank and download are left to the browser; _self still navigates", () => {
  const calls: string[] = [];
  render(
    <NavigationProvider items={[]} currentPath="/" navigate={(to) => calls.push(to)}>
      <AppLink to="/a" target="_blank">
        A
      </AppLink>
      <AppLink to="/b" download>
        B
      </AppLink>
      <AppLink to="/c" target="_self">
        C
      </AppLink>
    </NavigationProvider>,
  );
  expect(fireEvent.click(screen.getByRole("link", { name: "A" }))).toBe(true);
  expect(fireEvent.click(screen.getByRole("link", { name: "B" }))).toBe(true);
  expect(calls).toEqual([]);
  fireEvent.click(screen.getByRole("link", { name: "C" }));
  expect(calls).toEqual(["/c"]);
});

// WI-17 review n2 (ASM-997): the link colour is the design system's, not a page's override.
test("a link carries the ui-kit link class and keeps the class it is given", () => {
  render(
    <NavigationProvider items={[]} currentPath="/" navigate={() => {}}>
      <AppLink to="/settings" className="type-body">
        Settings
      </AppLink>
    </NavigationProvider>,
  );
  const cls = screen.getByRole("link", { name: "Settings" }).getAttribute("class") ?? "";
  expect(cls.split(" ")).toContain("vt-applink");
  expect(cls.split(" ")).toContain("type-body");
});
