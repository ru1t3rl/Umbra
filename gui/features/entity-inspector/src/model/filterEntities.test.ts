// WI-10 (plan "Model: filterEntities(entities, query, chips)", ASM-805): written before
// model/filterEntities.ts exists. Chips: `forces` (OR within), `owned` and `remote` (OR within); the three
// groups and the text AND together; an empty group filters nothing (ASM-937 b).
// Catches: case-sensitive search, a search that misses `#id`/template, AND instead of OR within a group,
// "unknown" ownership matched by a chip, a new array when nothing is filtered (10,000-entity churn).
import { describe, expect, test } from "bun:test";
import { summary } from "../inspectorTesting.tsx";
import { type EntityChips, filterEntities, NO_CHIPS } from "./filterEntities.ts";

const entities = [
  summary(1, { name: "Alpha One", template: "tank", force: "blue" }),
  summary(2, { name: "Bravo", template: "Truck", force: "red" }),
  summary(3, { template: "tank", force: "red", ownership: "remote", remote_source: "peer-a" }),
  summary(12, { name: "Delta", force: null, ownership: "unknown" }),
];
const ids = (list: readonly { id: number }[]) => list.map((e) => e.id);
const chips = (c: Partial<EntityChips>): EntityChips => ({ ...NO_CHIPS, ...c });

describe("filterEntities", () => {
  test("no query and no chips returns every entity in the frame order", () => {
    expect(ids(filterEntities(entities, "", NO_CHIPS))).toEqual([1, 2, 3, 12]);
    expect(NO_CHIPS).toEqual({ forces: [], owned: false, remote: false });
  });
  test("with nothing to filter it returns the same array (no copy of 10,000 entities)", () => {
    expect(filterEntities(entities, "", NO_CHIPS)).toBe(entities);
    expect(filterEntities(entities, "   ", NO_CHIPS)).toBe(entities);
  });
  test("the text matches the name, case-insensitively, as a substring", () => {
    expect(ids(filterEntities(entities, "alp", NO_CHIPS))).toEqual([1]);
    expect(ids(filterEntities(entities, "BRAVO", NO_CHIPS))).toEqual([2]);
  });
  test("the text matches the template", () => {
    expect(ids(filterEntities(entities, "tank", NO_CHIPS))).toEqual([1, 3]);
    expect(ids(filterEntities(entities, "truck", NO_CHIPS))).toEqual([2]);
  });
  test("the text matches the id, with or without the hash", () => {
    expect(ids(filterEntities(entities, "#12", NO_CHIPS))).toEqual([12]);
    expect(ids(filterEntities(entities, "12", NO_CHIPS))).toEqual([12]);
  });
  test("the text is trimmed; no match gives an empty list", () => {
    expect(ids(filterEntities(entities, "  bravo ", NO_CHIPS))).toEqual([2]);
    expect(filterEntities(entities, "zzz", NO_CHIPS)).toEqual([]);
  });
  test("force chips are OR'd within the group", () => {
    expect(ids(filterEntities(entities, "", chips({ forces: ["red"] })))).toEqual([2, 3]);
    expect(ids(filterEntities(entities, "", chips({ forces: ["red", "blue"] })))).toEqual([
      1, 2, 3,
    ]);
  });
  test("the owned chip keeps only owned entities; the remote chip only remote ones", () => {
    expect(ids(filterEntities(entities, "", chips({ owned: true })))).toEqual([1, 2]);
    expect(ids(filterEntities(entities, "", chips({ remote: true })))).toEqual([3]);
  });
  test("owned and remote together keep both, but not entities of unknown ownership", () => {
    expect(ids(filterEntities(entities, "", chips({ owned: true, remote: true })))).toEqual([
      1, 2, 3,
    ]);
  });
  test("the groups and the text are AND'd", () => {
    expect(ids(filterEntities(entities, "tank", chips({ forces: ["red"] })))).toEqual([3]);
    expect(ids(filterEntities(entities, "tank", chips({ forces: ["red"], owned: true })))).toEqual(
      [],
    );
  });
  test("does not mutate its input", () => {
    const copy = [...entities];
    filterEntities(entities, "tank", chips({ remote: true }));
    expect(entities).toEqual(copy);
  });
});
