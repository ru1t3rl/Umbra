import { isApiProblem } from "@umbra/data";
import { t } from "../messages.ts";

export interface ProblemText {
  title: string;
  detail: string;
}

/** The title and detail of a failed command: the engine's problem document, or the error's own message. */
export function problemText(error: unknown): ProblemText {
  if (isApiProblem(error)) return { title: error.title, detail: error.detail };
  return { title: t("problem.unknown"), detail: error instanceof Error ? error.message : "" };
}
