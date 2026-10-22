import { afterEach, expect, mock, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Select, type SelectOption } from "./Select.tsx";

afterEach(cleanup);

const options: SelectOption[] = [
  { value: "wgs84", label: "WGS 84" },
  { value: "utm", label: "UTM" },
  { value: "mgrs", label: "MGRS", disabled: true },
];

test("the label names the combobox and clicking the label focuses it", async () => {
  const user = userEvent.setup();
  render(<Select label="Coordinates" options={options} value="wgs84" onChange={() => {}} />);
  const select = screen.getByRole("combobox", { name: "Coordinates" });
  await user.click(screen.getByText("Coordinates"));
  expect(select).toHaveFocus();
  expect(screen.getByText("Coordinates")).toHaveClass("vt-label");
});

test("options render with their labels and values; disabled is honoured", () => {
  render(<Select label="C" options={options} value="wgs84" onChange={() => {}} />);
  const rendered = screen.getAllByRole("option");
  expect(rendered.map((o) => o.textContent)).toEqual(["WGS 84", "UTM", "MGRS"]);
  expect(screen.getByRole("option", { name: "UTM" })).toHaveValue("utm");
  expect(screen.getByRole("option", { name: "MGRS" })).toBeDisabled();
});

test("the value prop selects the matching option", () => {
  render(<Select label="C" options={options} value="utm" onChange={() => {}} />);
  expect(screen.getByRole("combobox")).toHaveValue("utm");
});

test("choosing an option calls onChange with the option value", async () => {
  const user = userEvent.setup();
  const onChange = mock((_value: string) => {});
  render(<Select label="C" options={options} value="wgs84" onChange={onChange} />);
  await user.selectOptions(screen.getByRole("combobox", { name: "C" }), "utm");
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange.mock.calls[0]?.[0]).toBe("utm");
});

test("tab focuses the select and picking by label changes the selection (arrow keys: Playwright)", async () => {
  const user = userEvent.setup();
  const onChange = mock((_value: string) => {});
  render(<Select label="C" options={options} value="wgs84" onChange={onChange} />);
  await user.tab();
  expect(screen.getByRole("combobox")).toHaveFocus();
  await user.selectOptions(screen.getByRole("combobox"), "UTM"); // by label, as a screen-reader user would pick
  expect(onChange.mock.calls.at(-1)?.[0]).toBe("utm");
});

test("help text describes the select without joining its name", () => {
  render(
    <Select
      label="C"
      help="Shown in the readout."
      options={options}
      value="utm"
      onChange={() => {}}
    />,
  );
  const select = screen.getByRole("combobox", { name: "C" });
  expect(select).toHaveAccessibleDescription("Shown in the readout.");
  expect(screen.getByText("Shown in the readout.")).toHaveClass("vt-help");
});

test("invalid sets aria-invalid and the invalid class", () => {
  const { container, rerender } = render(
    <Select label="C" options={options} value="utm" onChange={() => {}} />,
  );
  expect(screen.getByRole("combobox")).not.toHaveAttribute("aria-invalid");
  expect(container.firstElementChild).toHaveClass("vt-field");
  rerender(<Select label="C" options={options} value="utm" onChange={() => {}} invalid />);
  expect(screen.getByRole("combobox")).toHaveAttribute("aria-invalid", "true");
  expect(container.firstElementChild).toHaveClass("vt-field", "vt-field-invalid");
});

test("disabled blocks interaction and does not call onChange", async () => {
  const user = userEvent.setup();
  const onChange = mock((_value: string) => {});
  render(<Select label="C" options={options} value="wgs84" onChange={onChange} disabled />);
  expect(screen.getByRole("combobox")).toBeDisabled();
  await user.selectOptions(screen.getByRole("combobox"), "utm");
  expect(onChange).not.toHaveBeenCalled();
});

test("two selects get distinct ids", () => {
  render(
    <>
      <Select label="A" options={options} value="utm" onChange={() => {}} />
      <Select label="B" options={options} value="utm" onChange={() => {}} />
    </>,
  );
  expect(screen.getByRole("combobox", { name: "A" }).id).not.toBe(
    screen.getByRole("combobox", { name: "B" }).id,
  );
});
