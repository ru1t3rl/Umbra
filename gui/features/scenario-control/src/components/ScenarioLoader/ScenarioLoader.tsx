import { isApiProblem, type ScenarioInfo, useLoadScenario } from "@umbra/data";
import { Alert, Button, Select, TextField } from "@umbra/ui-kit";
import { type FormEvent, type ReactElement, useState } from "react";
import { t } from "../../messages.ts";
import { type DiagnosticView, toDiagnosticViews } from "../../model/diagnostics.ts";
import { EXAMPLE_SCENARIOS, type ScenarioSuggestion } from "../../model/exampleScenarios.ts";
import { buildLoadRequest, type ClockChoice, validateLoadForm } from "../../model/loadRequest.ts";
import { type ProblemText, problemText } from "../../model/problemText.ts";
import { DiagnosticList } from "../DiagnosticList/DiagnosticList.tsx";
import { PathPicker } from "../PathPicker/PathPicker.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./ScenarioLoader.css";

export interface ScenarioLoaderProps {
  /** Recently loaded paths (settings), newest first; a Select fills the path field. */
  recentPaths?: readonly string[];
  /** Called with the path once the engine accepted the scenario. */
  onLoaded: (path: string) => void;
  /**
   * Scenarios offered in a "Suggested scenarios" Select that fills the path field; any path can still be typed.
   * Defaults to the shipped examples. The shape is that of a scenario listing (contract gap G-2, `{ path, name }`
   * rows map to `{ path, label }`), so the shell passes the listing here later without a change (ASM-812, ASM-931).
   */
  suggestions?: readonly ScenarioSuggestion[];
}

const CLOCK_OPTIONS: readonly {
  value: ClockChoice;
  key: "loader.clock.default" | "clock.realtime" | "clock.afap" | "clock.stepped" | "clock.scaled";
}[] = [
  { value: "", key: "loader.clock.default" },
  { value: "realtime", key: "clock.realtime" },
  { value: "afap", key: "clock.afap" },
  { value: "stepped", key: "clock.stepped" },
  { value: "scaled", key: "clock.scaled" },
];

interface Failure extends ProblemText {
  diagnostics: DiagnosticView[];
}

/**
 * Load a scenario by path with optional clock and seed overrides (ASM-812). Validation happens before sending and
 * is shown inline; a failure shows the engine's detail and, for `scenario_invalid`, the located diagnostics below
 * it; warnings of a successful load stay in a dismissible Alert (ASM-813). The submit button is disabled while a
 * request is in flight. Unload lives in RunControls.
 */
export function ScenarioLoader({
  recentPaths = [],
  onLoaded,
  suggestions = EXAMPLE_SCENARIOS,
}: ScenarioLoaderProps): ReactElement {
  const [path, setPath] = useState("");
  const [clock, setClock] = useState<ClockChoice>("");
  const [scaleText, setScaleText] = useState("1");
  const [seedText, setSeedText] = useState("");
  const [submitted, setSubmitted] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [warnings, setWarnings] = useState<DiagnosticView[]>([]);
  const load = useLoadScenario();

  const form = { path, clock, scaleText, seedText };
  const errors = validateLoadForm(form);
  const pathInvalid = submitted && errors.path;

  const submit = (event: FormEvent): void => {
    event.preventDefault();
    setSubmitted(true);
    const data = buildLoadRequest(form);
    if (!data) return;
    setFailure(null);
    setWarnings([]);
    load.mutate(
      { data },
      {
        onSuccess: (info: ScenarioInfo | null | undefined) => {
          setWarnings(toDiagnosticViews(info?.warnings));
          onLoaded(data.path);
        },
        onError: (error: unknown) =>
          setFailure({
            ...problemText(error),
            diagnostics: isApiProblem(error) ? toDiagnosticViews(error.diagnostics) : [],
          }),
      },
    );
  };

  return (
    <form className="umbra-scenario-loader" onSubmit={submit} noValidate>
      <PathPicker
        label={t("loader.recent")}
        placeholder={t("loader.recentPlaceholder")}
        items={recentPaths.map((p) => ({ path: p }))}
        path={path}
        onPick={setPath}
      />
      <PathPicker
        label={t("loader.suggested")}
        placeholder={t("loader.suggestedPlaceholder")}
        items={suggestions}
        path={path}
        onPick={setPath}
      />
      <TextField
        label={t("loader.path")}
        help={pathInvalid ? t("loader.pathError") : t("loader.pathHelp")}
        mono
        autoComplete="off"
        spellCheck={false}
        value={path}
        invalid={pathInvalid}
        onChange={(e) => setPath(e.target.value)}
      />
      <div className="umbra-scenario-loader-row">
        <Select
          label={t("loader.clock")}
          value={clock}
          options={CLOCK_OPTIONS.map((o) => ({ value: o.value, label: t(o.key) }))}
          onChange={(value) => setClock(value as ClockChoice)}
        />
        {clock === "scaled" ? (
          <TextField
            label={t("clock.scaleFactor")}
            help={errors.scale ? t("loader.scaleError") : t("clock.scaleHelp")}
            mono
            inputMode="decimal"
            value={scaleText}
            invalid={errors.scale}
            onChange={(e) => setScaleText(e.target.value)}
          />
        ) : null}
        <TextField
          label={t("loader.seed")}
          help={errors.seed ? t("loader.seedError") : t("loader.seedHelp")}
          mono
          inputMode="numeric"
          autoComplete="off"
          value={seedText}
          invalid={errors.seed}
          onChange={(e) => setSeedText(e.target.value)}
        />
      </div>
      <div className="umbra-scenario-loader-row">
        <Button type="submit" variant="filled" disabled={load.isPending}>
          {t("loader.submit")}
        </Button>
      </div>
      {failure ? (
        <>
          <ProblemAlert problem={failure} />
          <DiagnosticList items={failure.diagnostics} />
        </>
      ) : null}
      {warnings.length > 0 ? (
        <>
          <Alert
            tone="warning"
            title={t("loader.warnings")}
            action={
              <Button size="sm" variant="ghost" onClick={() => setWarnings([])}>
                {t("loader.dismiss")}
              </Button>
            }
          />
          <DiagnosticList items={warnings} />
        </>
      ) : null}
    </form>
  );
}
