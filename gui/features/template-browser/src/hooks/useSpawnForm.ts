import { isApiProblem, useSpawnEntity } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import { type FormEvent, type RefObject, useLayoutEffect, useRef, useState } from "react";
import {
  canSpawn,
  EMPTY_VALUES,
  FIELDS,
  type FieldName,
  type FormProblems,
  type FormValues,
  validateSpawn,
} from "../model/spawnForm.ts";

export type SpawnResult =
  | { readonly kind: "spawned"; readonly id: number }
  | { readonly kind: "failed"; readonly error: unknown };

export interface SpawnFormState {
  readonly values: FormValues;
  readonly problems: FormProblems;
  readonly result: SpawnResult | null;
  /** The engine refused the name as a duplicate (marks the Name field). */
  readonly duplicateName: boolean;
  readonly enabled: boolean;
  readonly pending: boolean;
  setValue(field: FieldName, value: string): void;
  submit(event: FormEvent): void;
}

/**
 * The state and actions of the spawn form (ASM-949): values, validation on submit, the `POST /entities` request
 * with a ref guard against two clicks in one act, and the result. A different template clears Name, the result and
 * the problems and keeps the rest; an answer for a previous template is dropped (a generation counter that a layout
 * effect bumps when the template changes, so no ref is written during render).
 *
 * Focus moves in a layout effect after the commit that marks the field invalid or explains the duplicate name, so a
 * screen reader hears the field already described (ASM-954). `formRef` is the form to search for the field.
 */
export function useSpawnForm(
  templateId: string,
  formRef: RefObject<HTMLFormElement | null>,
): SpawnFormState {
  const spawn = useSpawnEntity();
  const enabled = canSpawn(useRunStatus()?.state);
  const [values, setValues] = useState<FormValues>(EMPTY_VALUES);
  const [problems, setProblems] = useState<FormProblems>({});
  const [result, setResult] = useState<SpawnResult | null>(null);
  const [forTemplate, setForTemplate] = useState(templateId);
  const [focusRequest, setFocusRequest] = useState<{ field: FieldName } | null>(null);
  const generation = useRef(0);
  const inFlight = useRef(false);

  if (forTemplate !== templateId) {
    setForTemplate(templateId);
    setValues((v) => ({ ...v, name: "" }));
    setProblems({});
    setResult(null);
    setFocusRequest(null);
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: the template id is the trigger, not a value read
  useLayoutEffect(() => {
    // Every template change (and the mount) starts a new generation: older answers are stale.
    generation.current += 1;
  }, [templateId]);

  useLayoutEffect(() => {
    if (!focusRequest) return;
    formRef.current?.querySelector<HTMLInputElement>(`[name="${focusRequest.field}"]`)?.focus();
    setFocusRequest(null);
  }, [focusRequest, formRef]);

  const submit = (event: FormEvent): void => {
    event.preventDefault();
    if (!enabled || inFlight.current) return;
    setResult(null);
    const checked = validateSpawn(values);
    if (!checked.ok) {
      setProblems(checked.problems);
      const first = FIELDS.find((f) => checked.problems[f] !== undefined);
      if (first) setFocusRequest({ field: first });
      return;
    }
    setProblems({});
    inFlight.current = true;
    const mine = generation.current;
    spawn.mutate(
      { data: checked.body(templateId) },
      {
        onSuccess: (detail) => {
          if (mine === generation.current) setResult({ kind: "spawned", id: detail.summary.id });
        },
        onError: (error) => {
          if (mine !== generation.current) return;
          setResult({ kind: "failed", error });
          if (isApiProblem(error) && error.code === "duplicate_name")
            setFocusRequest({ field: "name" });
        },
        onSettled: () => {
          inFlight.current = false;
        },
      },
    );
  };

  return {
    values,
    problems,
    result,
    duplicateName:
      result?.kind === "failed" &&
      isApiProblem(result.error) &&
      result.error.code === "duplicate_name",
    enabled,
    pending: spawn.isPending,
    setValue: (field, value) => setValues((v) => ({ ...v, [field]: value })),
    submit,
  };
}
