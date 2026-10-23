// WI-10 (plan "Model: entityRowModel(entity, affiliationOf)"): written before model/entityRowModel.ts exists.
// Shape (ASM-937 c): { id, name, idLabel, affiliation, meta, status, remote }.
// Catches: a missing name shown as empty, the id repeated for unnamed entities, the force passed wrongly to
// affiliationOf, a remote flag for "unknown" ownership, a status or template shown as the string "null".
import { describe, expect, test } from "bun:test";
import { summary } from "../inspectorTesting.tsx";
import { entityRowModel } from "./entityRowModel.ts";

const friend = () => "friend" as const;

describe("entityRowModel", () => {
  test("a named entity shows its name and the #id beside it", () => {
    const row = entityRowModel(summary(7, { name: "Alpha" }), friend);
    expect(row.id).toBe(7);
    expect(row.name).toBe("Alpha");
    expect(row.idLabel).toBe("#7");
  });
  test("an unnamed entity is named #id and shows no second id", () => {
    for (const name of [undefined, null, ""]) {
      const row = entityRowModel(summary(7, { name }), friend);
      expect(row.name).toBe("#7");
      expect(row.idLabel).toBeNull();
    }
  });
  test("the template is the meta line; no template gives null", () => {
    expect(entityRowModel(summary(1, { template: "tank" }), friend).meta).toBe("tank");
    expect(entityRowModel(summary(1, { template: null }), friend).meta).toBeNull();
    expect(entityRowModel(summary(1), friend).meta).toBeNull();
  });
  test("the status is the behavior status word; none gives null", () => {
    expect(entityRowModel(summary(1, { status: "moving" }), friend).status).toBe("moving");
    expect(entityRowModel(summary(1, { status: null }), friend).status).toBeNull();
  });
  test("the affiliation comes from affiliationOf(force), with the entity's force", () => {
    const seen: (string | null | undefined)[] = [];
    const row = entityRowModel(summary(1, { force: "red" }), (force) => {
      seen.push(force);
      return "hostile";
    });
    expect(row.affiliation).toBe("hostile");
    expect(seen).toEqual(["red"]);
  });
  test("only ownership remote is remote: owned and unknown are not", () => {
    expect(entityRowModel(summary(1, { ownership: "remote" }), friend).remote).toBe(true);
    expect(entityRowModel(summary(1, { ownership: "owned" }), friend).remote).toBe(false);
    expect(entityRowModel(summary(1, { ownership: "unknown" }), friend).remote).toBe(false);
  });
});
