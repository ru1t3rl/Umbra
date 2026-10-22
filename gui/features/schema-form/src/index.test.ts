import { expect, test } from "bun:test";
import * as api from "./index.ts";

test("the public API of @umbra/schema-form is SchemaForm only (the model stays internal, ADR 0070)", () => {
  // Catches: leaking toFieldTree/unitFromKey (a second consumer would couple to internals) or losing SchemaForm.
  expect(Object.keys(api).sort()).toEqual(["SchemaForm"]);
  expect(typeof api.SchemaForm).toBe("function");
});
