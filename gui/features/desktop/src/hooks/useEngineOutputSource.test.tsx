import { expect, test } from "bun:test";
import { renderHook } from "@testing-library/react";
import { useEngineOutputSource } from "./useEngineOutputSource.ts";

test("the stub has no engine output source (web build)", () => {
  const { result } = renderHook(() => useEngineOutputSource());
  expect(result.current).toBeUndefined();
});
