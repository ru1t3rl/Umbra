// WI-8 failing tests (skill write-frontend-tests). Assumed API:
//   <ScenarioLoader recentPaths?={string[]} onLoaded={(path) => void} />  sends POST /api/v1/scenario (useLoadScenario).
//   Markup: TextField "Scenario path"; Select "Recent scenarios" (only when recentPaths is not empty; choosing one
//   fills the path); Select "Clock override" (options "Scenario default", "Real time", "As fast as possible",
//   "Stepped", "Scaled"; default sends no clock); TextField "Scale factor" only for Scaled; TextField "Seed"
//   (optional, ^[0-9]{1,20}$, an invalid seed shows an inline error and sends nothing); Button "Load scenario".
//   Failure: scenario_invalid -> a Problem Alert plus a list (role list, name "Diagnostics") with one item per
//   diagnostic: "file:line:column", code, message, a severity Badge ("error" when absent) and notes.
//   Other problems -> Alert with title and detail. Success with warnings -> Alert (warning) titled
//   "Scenario loaded with warnings", the same diagnostics list, a button "Dismiss". There is no Unload here.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import {
  deferred,
  type FakeEngine,
  fakeEngine,
  mount,
  problemReply,
} from "../../controlsTesting.tsx";
import { ScenarioLoader } from "./ScenarioLoader.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const info = (extra: Record<string, unknown> = {}): Response =>
  new Response(
    JSON.stringify({
      session: 1,
      name: "walking_skeleton",
      path: "scenarios/walking_skeleton/scenario.toml",
      seed: "42",
      tick_rate_hz: 30,
      tick_rate_source: "scenario",
      duration_ticks: 7200,
      entity_count: 3,
      ...extra,
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );

const PATH = "scenarios/walking_skeleton/scenario.toml";
const posts = (e: FakeEngine) => e.calls.filter((c) => c.method === "POST");
const typePath = (value = PATH): void => {
  fireEvent.change(screen.getByRole("textbox", { name: "Scenario path" }), { target: { value } });
};
const submit = (): void => {
  fireEvent.click(screen.getByRole("button", { name: "Load scenario" }));
};

describe("request body", () => {
  // Catches: extra fields (clock, seed) sent when the user set none.
  test("a path only sends just the path", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
    const call = posts(engine)[0];
    expect(call?.path).toBe("/api/v1/scenario");
    expect(call?.body).toEqual({ path: PATH });
  });

  // Catches: the override dropped or sent in the wrong shape.
  test("a clock override is sent as a ClockSetting", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    fireEvent.change(screen.getByRole("combobox", { name: "Clock override" }), {
      target: { value: "stepped" },
    });
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
    expect(posts(engine)[0]?.body).toEqual({ path: PATH, clock: { mode: "stepped" } });
  });

  // Catches: a scaled override without its scale.
  test("a scaled override carries the scale factor", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    fireEvent.change(screen.getByRole("combobox", { name: "Clock override" }), {
      target: { value: "scaled" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Scale factor" }), {
      target: { value: "4" },
    });
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
    expect(posts(engine)[0]?.body).toEqual({ path: PATH, clock: { mode: "scaled", scale: 4 } });
  });

  // Catches: the seed converted to a number (loses precision above 2^53).
  test("a seed is sent as a decimal string, even above 2^53", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    fireEvent.change(screen.getByRole("textbox", { name: "Seed" }), {
      target: { value: "18446744073709551615" },
    });
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
    expect(posts(engine)[0]?.body).toEqual({ path: PATH, seed: "18446744073709551615" });
  });
});

describe("validation before sending", () => {
  // Catches: a request the contract pattern ^[0-9]{1,20}$ rejects (or silently coerces).
  test.each(["-1", "1.5", "abc", "123456789012345678901", "1e3", " 7"])(
    "seed %j is refused inline and nothing is sent",
    (seed) => {
      engine = fakeEngine(() => info());
      mount(<ScenarioLoader onLoaded={() => {}} />);
      typePath();
      fireEvent.change(screen.getByRole("textbox", { name: "Seed" }), { target: { value: seed } });
      submit();
      expect(posts(engine)).toHaveLength(0);
      expect(screen.getByRole("textbox", { name: "Seed" })).toHaveAttribute("aria-invalid", "true");
    },
  );

  // Catches: an empty path request that the server can only answer with a 400.
  test("an empty path sends nothing", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    submit();
    expect(posts(engine)).toHaveLength(0);
  });

  // Catches: the boundary values of the pattern being refused.
  test.each(["0", "7", "12345678901234567890"])("seed %j is accepted", async (seed) => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    fireEvent.change(screen.getByRole("textbox", { name: "Seed" }), { target: { value: seed } });
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
  });
});

describe("recent paths", () => {
  // Catches: recent paths not offered, or picking one not filling the field.
  test("choosing a recent path fills the path field", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader recentPaths={["a/one.toml", "b/two.toml"]} onLoaded={() => {}} />);
    const select = screen.getByRole("combobox", { name: "Recent scenarios" });
    expect(
      within(select)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(expect.arrayContaining(["a/one.toml", "b/two.toml"]));
    fireEvent.change(select, { target: { value: "b/two.toml" } });
    expect(screen.getByRole("textbox", { name: "Scenario path" })).toHaveValue("b/two.toml");
  });

  // Catches: an empty, useless Select.
  test("no recent paths means no recent Select", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    expect(screen.queryByRole("combobox", { name: "Recent scenarios" })).toBeNull();
  });
});

describe("success", () => {
  // Catches: the shell never learning which path loaded (it records the recent path).
  test("onLoaded is called with the path after the server answers", async () => {
    engine = fakeEngine(() => info());
    const loaded: string[] = [];
    mount(<ScenarioLoader onLoaded={(p) => loaded.push(p)} />);
    typePath();
    submit();
    await waitFor(() => expect(loaded).toEqual([PATH]));
  });

  // Catches: onLoaded firing before or without a successful load.
  test("onLoaded is not called when the load fails", async () => {
    engine = fakeEngine(() => problemReply(403, "path_not_allowed", "Not allowed", "nope"));
    const loaded: string[] = [];
    mount(<ScenarioLoader onLoaded={(p) => loaded.push(p)} />);
    typePath();
    submit();
    await screen.findByRole("alert");
    expect(loaded).toEqual([]);
  });

  // Catches: warnings dropped, or an Alert that cannot be dismissed.
  test("warnings show in a dismissible Alert", async () => {
    engine = fakeEngine(() =>
      info({
        warnings: [
          {
            code: "unused_template",
            message: "template tank is never spawned",
            file: "scenario.toml",
            line: 4,
            column: 1,
            severity: "warning",
          },
        ],
      }),
    );
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Scenario loaded with warnings")).toBeTruthy();
    // The diagnostics list sits beside the live-region Alert, not inside it (ASM-931).
    const list = screen.getByRole("list", { name: "Diagnostics" });
    expect(alert.contains(list)).toBe(false);
    expect(within(list).getByText("template tank is never spawned")).toBeTruthy();
    expect(within(list).getByText("unused_template")).toBeTruthy();
    fireEvent.click(within(alert).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("template tank is never spawned")).toBeNull();
  });

  // Catches: an empty warnings Alert after a clean load.
  test("a clean load shows no Alert", async () => {
    engine = fakeEngine(() => info());
    const loaded: string[] = [];
    mount(<ScenarioLoader onLoaded={(p) => loaded.push(p)} />);
    typePath();
    submit();
    await waitFor(() => expect(loaded).toHaveLength(1));
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("failures", () => {
  const invalid = (): Response =>
    new Response(
      JSON.stringify({
        type: "urn:umbra:problem:scenario_invalid",
        code: "scenario_invalid",
        title: "The scenario is invalid",
        status: 422,
        detail: "2 problems in scenario",
        diagnostics: [
          {
            code: "unknown_template",
            message: "template tnak does not exist",
            file: "scenario.toml",
            line: 12,
            column: 5,
            severity: "error",
            notes: [
              { message: "did you mean tank?", file: "templates/tank.toml", line: 1, column: 1 },
            ],
          },
          {
            code: "missing_field",
            message: "field name is required",
            file: "scenario.toml",
            line: 3,
          },
        ],
      }),
      { status: 422, headers: { "content-type": "application/problem+json" } },
    );

  // Catches: the diagnostics collapsed into the detail string, losing locations.
  test("scenario_invalid lists each diagnostic with location, code, message, severity and notes", async () => {
    engine = fakeEngine(invalid);
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const list = await screen.findByRole("list", { name: "Diagnostics" });
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    const first = within(items[0] as HTMLElement);
    expect(first.getByText("scenario.toml:12:5")).toBeTruthy();
    expect(first.getByText("unknown_template")).toBeTruthy();
    expect(first.getByText("template tnak does not exist")).toBeTruthy();
    expect(first.getByText("error")).toBeTruthy();
    expect(first.getByText(/did you mean tank\?/)).toBeTruthy();
    expect(first.getByText(/templates\/tank\.toml:1:1/)).toBeTruthy();
  });

  // Catches: severity absent rendered as nothing (API 1.1.0: a missing field reads error), or a missing column.
  test("a diagnostic without severity reads error and a missing column is left out", async () => {
    engine = fakeEngine(invalid);
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const list = await screen.findByRole("list", { name: "Diagnostics" });
    const second = within(within(list).getAllByRole("listitem")[1] as HTMLElement);
    expect(second.getByText("error")).toBeTruthy();
    expect(second.getByText("scenario.toml:3")).toBeTruthy();
  });

  // Catches: diagnostics reordered (server order is the file order, ASM-813).
  test("diagnostics keep the server order", async () => {
    engine = fakeEngine(invalid);
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const list = await screen.findByRole("list", { name: "Diagnostics" });
    const codes = within(list)
      .getAllByRole("listitem")
      .map((li) => (li.textContent ?? "").includes("unknown_template"));
    expect(codes).toEqual([true, false]);
  });

  // Catches: warning diagnostics shown with a danger Badge (or vice versa).
  test("a warning diagnostic in a failure keeps its warning severity", async () => {
    engine = fakeEngine(
      () =>
        new Response(
          JSON.stringify({
            type: "urn:umbra:problem:scenario_invalid",
            code: "scenario_invalid",
            title: "The scenario is invalid",
            status: 422,
            detail: "d",
            diagnostics: [{ code: "w1", message: "careful", severity: "warning" }],
          }),
          { status: 422, headers: { "content-type": "application/problem+json" } },
        ),
    );
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const list = await screen.findByRole("list", { name: "Diagnostics" });
    expect(within(list).getByText("warning")).toBeTruthy();
  });

  // Catches: the detail hidden for path problems (the user must see what was refused).
  test("path_not_allowed shows its title and detail", async () => {
    engine = fakeEngine(() =>
      problemReply(
        403,
        "path_not_allowed",
        "The path is outside the allowed directories",
        "path not allowed: ../etc/passwd",
      ),
    );
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath("../etc/passwd");
    submit();
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("The path is outside the allowed directories")).toBeTruthy();
    expect(within(alert).getByText("path not allowed: ../etc/passwd")).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Diagnostics" })).toBeNull();
  });

  // Catches: a stale error left after the next attempt succeeds.
  test("a successful retry clears the previous error", async () => {
    let n = 0;
    engine = fakeEngine(() =>
      ++n === 1 ? problemReply(403, "path_not_allowed", "Denied", "no") : info(),
    );
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    await screen.findByRole("alert");
    submit();
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });
});

describe("scope", () => {
  // Catches: Unload leaking into the loader (it lives in RunControls, WI-8).
  test("there is no Unload button", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />, { state: "ready" });
    expect(screen.queryByRole("button", { name: "Unload" })).toBeNull();
  });
});

describe("suggested paths (ASM-812, ASM-931)", () => {
  const options = (name: string): (string | null)[] =>
    within(screen.getByRole("combobox", { name }))
      .getAllByRole("option")
      .map((o) => o.textContent);

  // Catches: the examples not offered while no scenario listing exists (G-2).
  test("the four shipped examples are suggested by default and fill the path", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    expect(options("Suggested scenarios")).toEqual(
      expect.arrayContaining([
        "scenarios/walking_skeleton/scenario.toml",
        "scenarios/patrol_and_engage/scenario.toml",
        "scenarios/plugin_wander/scenario.toml",
        "scenarios/plugin_c_circle/scenario.toml",
      ]),
    );
    fireEvent.change(screen.getByRole("combobox", { name: "Suggested scenarios" }), {
      target: { value: "scenarios/plugin_wander/scenario.toml" },
    });
    expect(screen.getByRole("textbox", { name: "Scenario path" })).toHaveValue(
      "scenarios/plugin_wander/scenario.toml",
    );
  });

  // Catches: a future scenario listing (G-2) that cannot replace the examples.
  test("suggestions (path, optional label) replaces the examples and an empty list hides the Select", () => {
    engine = fakeEngine(() => info());
    mount(
      <ScenarioLoader suggestions={[{ path: "x/y.toml", label: "Why" }]} onLoaded={() => {}} />,
    );
    expect(options("Suggested scenarios")).toContain("Why");
    expect(options("Suggested scenarios")).not.toContain(
      "scenarios/walking_skeleton/scenario.toml",
    );
    cleanup();
    mount(<ScenarioLoader suggestions={[]} onLoaded={() => {}} />);
    expect(screen.queryByRole("combobox", { name: "Suggested scenarios" })).toBeNull();
  });

  // Catches: a suggestion that replaces the free path (the Select must not be the only way in).
  test("a typed path still works with suggestions present", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath("custom/own.toml");
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
    expect(posts(engine)[0]?.body).toEqual({ path: "custom/own.toml" });
  });
});

describe("pickers follow the path (keyboard)", () => {
  // Catches: a Select pinned to "" (a closed select fires change on arrow keys, so options 2..n were unreachable).
  test("the selected option reflects the path, so successive changes reach later options", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader recentPaths={["a.toml", "b.toml", "c.toml"]} onLoaded={() => {}} />);
    const select = (): HTMLSelectElement =>
      screen.getByRole("combobox", { name: "Recent scenarios" }) as HTMLSelectElement;
    expect(select().value).toBe("");
    for (const next of ["a.toml", "b.toml", "c.toml"]) {
      fireEvent.change(select(), { target: { value: next } });
      expect(select().value).toBe(next);
      expect(screen.getByRole("textbox", { name: "Scenario path" })).toHaveValue(next);
    }
    typePath("typed.toml");
    expect(select().value).toBe("");
  });
});

describe("empty path", () => {
  // Catches: a silent submit (nothing sent and nothing said).
  test("submitting without a path says so and marks the field", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    const field = screen.getByRole("textbox", { name: "Scenario path" });
    expect(field).not.toHaveAttribute("aria-invalid", "true");
    submit();
    expect(posts(engine)).toHaveLength(0);
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Enter a scenario path.")).toBeTruthy();
    typePath();
    expect(field).not.toHaveAttribute("aria-invalid", "true");
  });

  // Catches: a whitespace-only path accepted.
  test("a whitespace path counts as empty and a padded path is trimmed", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath("   ");
    submit();
    expect(posts(engine)).toHaveLength(0);
    typePath(`  ${PATH}  `);
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
    expect(posts(engine)[0]?.body).toEqual({ path: PATH });
  });
});

describe("specific input errors", () => {
  // Catches: an invalid seed with only a generic help text.
  test("an invalid seed replaces the help with the specific error", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    const seed = screen.getByRole("textbox", { name: "Seed" });
    expect(seed).toHaveAccessibleDescription("Optional. 0 to 18446744073709551615.");
    fireEvent.change(seed, { target: { value: "x" } });
    expect(seed).toHaveAccessibleDescription(
      "Seed must be a whole number from 0 to 18446744073709551615.",
    );
  });

  // Catches: a scaled override with a bad scale being sent or silently accepted.
  test("a bad scale is refused with its error and nothing is sent", () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    fireEvent.change(screen.getByRole("combobox", { name: "Clock override" }), {
      target: { value: "scaled" },
    });
    const scale = screen.getByRole("textbox", { name: "Scale factor" });
    fireEvent.change(scale, { target: { value: "0" } });
    submit();
    expect(posts(engine)).toHaveLength(0);
    expect(scale).toHaveAttribute("aria-invalid", "true");
    expect(scale).toHaveAccessibleDescription("Scale must be a number from 0.001 to 1,000,000.");
  });

  // Catches: seeds above u64 (the server refuses them).
  test("the largest u64 seed is accepted and one more is refused", async () => {
    engine = fakeEngine(() => info());
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    const seed = screen.getByRole("textbox", { name: "Seed" });
    fireEvent.change(seed, { target: { value: "18446744073709551616" } });
    submit();
    expect(posts(engine)).toHaveLength(0);
    expect(seed).toHaveAttribute("aria-invalid", "true");
    fireEvent.change(seed, { target: { value: "18446744073709551615" } });
    submit();
    await waitFor(() => expect(posts(engine as FakeEngine)).toHaveLength(1));
  });
});

describe("in flight and resubmit", () => {
  // Catches: a double submit while the first request is still running.
  test("the submit button is disabled while the request is in flight", async () => {
    const gate = deferred<Response>();
    engine = fakeEngine(() => gate.promise as unknown as Response);
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const button = screen.getByRole("button", { name: "Load scenario" });
    await waitFor(() => expect(button.hasAttribute("disabled")).toBe(true));
    fireEvent.click(button);
    expect(posts(engine)).toHaveLength(1);
    gate.resolve(info());
    await waitFor(() => expect(button.hasAttribute("disabled")).toBe(false));
  });

  // Catches: warnings of the previous load staying visible while the next one runs or fails.
  test("warnings are cleared when the next load starts", async () => {
    let n = 0;
    const gate = deferred<Response>();
    engine = fakeEngine(() => {
      n += 1;
      if (n === 1) return info({ warnings: [{ code: "w", message: "old warning" }] });
      return gate.promise as unknown as Response;
    });
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    await screen.findByText("old warning");
    submit();
    try {
      // A boolean: a failing toBeNull would pretty-print a whole DOM node.
      await waitFor(() => expect(screen.queryByText("old warning") === null).toBe(true));
    } finally {
      gate.resolve(info());
    }
  });
});

describe("untrusted success bodies", () => {
  // Catches: a crash on a warnings field that is not an array, or on a null body.
  test.each([
    ["warnings is not an array", { warnings: "oops" }],
    ["warnings holds junk", { warnings: [null, 3, "x", { code: 1, message: 2 }] }],
  ])("%s: no Alert and onLoaded still runs", async (_name, extra) => {
    engine = fakeEngine(() => info(extra));
    const loaded: string[] = [];
    mount(<ScenarioLoader onLoaded={(p) => loaded.push(p)} />);
    typePath();
    submit();
    await waitFor(() => expect(loaded).toEqual([PATH]));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("a null body still reports the load", async () => {
    engine = fakeEngine(
      () => new Response("null", { status: 200, headers: { "content-type": "application/json" } }),
    );
    const loaded: string[] = [];
    mount(<ScenarioLoader onLoaded={(p) => loaded.push(p)} />);
    typePath();
    submit();
    await waitFor(() => expect(loaded).toEqual([PATH]));
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("diagnostic cap", () => {
  // Catches: thousands of list items from a hostile scenario.
  test("at most 50 diagnostics are listed and the rest is counted", async () => {
    const many = Array.from({ length: 60 }, (_, i) => ({ code: `c${i}`, message: `m${i}` }));
    engine = fakeEngine(
      () =>
        new Response(
          JSON.stringify({
            type: "urn:umbra:problem:scenario_invalid",
            code: "scenario_invalid",
            title: "The scenario is invalid",
            status: 422,
            detail: "d",
            diagnostics: many,
          }),
          { status: 422, headers: { "content-type": "application/problem+json" } },
        ),
    );
    mount(<ScenarioLoader onLoaded={() => {}} />);
    typePath();
    submit();
    const list = await screen.findByRole("list", { name: "Diagnostics" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(50);
    expect(screen.getByText("and 10 more not shown")).toBeTruthy();
    expect(screen.getByRole("alert")).not.toContainElement(list);
  });
});
