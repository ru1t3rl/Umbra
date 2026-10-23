import { expect, test } from "bun:test";
import * as api from "./index.ts";

test("the public API of @umbra/desktop", () => {
  expect(Object.keys(api).sort()).toEqual(["useEngineOutputSource"]);
});
