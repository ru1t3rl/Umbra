import { afterEach, expect } from "bun:test";
import * as matchers from "@testing-library/jest-dom/matchers";
import { cleanup } from "@testing-library/react";

expect.extend(matchers);

// bun has no global afterEach for RTL's auto-cleanup, so a test file without its own cleanup leaked
// rendered DOM into later files (human decision 2026-10-23, M8 WI-10, ASM-941). cleanup is idempotent.
afterEach(cleanup);
