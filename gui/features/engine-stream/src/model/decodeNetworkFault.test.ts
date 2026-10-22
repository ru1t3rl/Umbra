// WI-7: RunFaultAlert needs `run.network_fault` (API 1.1.0, already in the stream schema) to survive decoding.
// Assumed change: runOf copies `network_fault` ({ adapter, reason }, both strings) like it does `fault`; a malformed
// object is dropped; sameFault-style change detection includes it (a changed network_fault on the same state makes a
// journal run record, ASM-896).
import { describe, expect, test } from "bun:test";
import { frame, hello, run } from "../testing.ts";
import { decodeJson } from "./decode.ts";

const dec = (m: unknown) => decodeJson(JSON.stringify(m));
const NF = { adapter: "dis-main", reason: "socket closed" };

describe("run.network_fault", () => {
  // Catches: the field dropped by the decoder (the alert could never show an adapter).
  test("is kept on a frame run block", () => {
    const d = dec(
      frame(0, 1, [], {
        run: run(1, { state: "stopped", stop_reason: "network_fault", network_fault: NF }),
      }),
    );
    expect(d.kind).toBe("frame");
    if (d.kind === "frame") expect(d.run?.network_fault).toEqual(NF);
  });

  // Catches: a malformed object passed through unchecked.
  test.each([[{ adapter: "a" }], [{ reason: "r" }], [{ adapter: 1, reason: "r" }], ["x"], [null]])(
    "drops a malformed value %j",
    (bad) => {
      const d = dec(frame(0, 1, [], { run: run(1, { network_fault: bad }) }));
      if (d.kind === "frame") expect(d.run?.network_fault ?? null).toBeNull();
      else throw new Error("expected a frame");
    },
  );

  // Catches: a hello with nothing to do with it breaking (sanity for the shared helper).
  test("a hello still decodes", () => {
    expect(dec(hello()).kind).toBe("hello");
  });
});
