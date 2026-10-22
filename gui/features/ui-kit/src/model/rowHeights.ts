import type { RowHeightToken } from "../tokens/tokens.ts";

export type { RowHeightToken };

/**
 * Fixed row heights in whole pixels (ADR 0072). Mirrors the CSS tokens `--row-sm`, `--row-md` and `--row-lg`
 * in tokens.json; `rowHeightTokens.test.ts` keeps the two equal (ASM-872).
 */
export const ROW_HEIGHT_PX: Readonly<Record<RowHeightToken, number>> = {
  "row-sm": 28,
  "row-md": 36,
  "row-lg": 56,
};
