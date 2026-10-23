// WI-10 (plan "Model: formatPosition (degrees with 6 decimals, alt_m with 1), formatHeading, formatSpeed"):
// written before model/format.ts exists. Numbers only: units are the Readout's `unit` prop (ASM-937 a).
// Catches: wrong decimals, "-0.0", NaN or Infinity reaching the screen, an out-of-range heading.
import { describe, expect, test } from "bun:test";
import { formatHeading, formatPosition, formatRoute, formatSpeed, NO_VALUE } from "./format.ts";

describe("formatPosition", () => {
  test("degrees have 6 decimals and altitude has 1", () => {
    expect(formatPosition({ lat_deg: 10.1234567, lon_deg: 20, alt_m: 12.46 })).toEqual({
      lat: "10.123457",
      lon: "20.000000",
      alt: "12.5",
    });
  });
  test("negative values keep their sign", () => {
    expect(formatPosition({ lat_deg: -33.5, lon_deg: -70.25, alt_m: -3 })).toEqual({
      lat: "-33.500000",
      lon: "-70.250000",
      alt: "-3.0",
    });
  });
  test("a value that rounds to zero has no minus sign", () => {
    expect(formatPosition({ lat_deg: -0.0000001, lon_deg: 0, alt_m: -0.04 })).toEqual({
      lat: "0.000000",
      lon: "0.000000",
      alt: "0.0",
    });
  });
  test("a value that is not a finite number is the no-value dash, not NaN", () => {
    expect(NO_VALUE).toBe("—");
    const p = formatPosition({ lat_deg: Number.NaN, lon_deg: Number.POSITIVE_INFINITY, alt_m: 1 });
    expect(p.lat).toBe(NO_VALUE);
    expect(p.lon).toBe(NO_VALUE);
    expect(p.alt).toBe("1.0");
  });
});

describe("formatHeading", () => {
  test("one decimal, degrees clockwise from north", () => {
    expect(formatHeading(90)).toBe("90.0");
    expect(formatHeading(47.26)).toBe("47.3");
  });
  test("is normalised into 0 up to (not including) 360", () => {
    expect(formatHeading(-90)).toBe("270.0");
    expect(formatHeading(360)).toBe("0.0");
    expect(formatHeading(725)).toBe("5.0");
  });
  test("a value that is not finite is the no-value dash", () => {
    expect(formatHeading(Number.NaN)).toBe(NO_VALUE);
  });
});

describe("formatSpeed", () => {
  test("metres per second with one decimal", () => {
    expect(formatSpeed(5)).toBe("5.0");
    expect(formatSpeed(12.345)).toBe("12.3");
    expect(formatSpeed(0)).toBe("0.0");
  });
  test("a value that is not finite is the no-value dash", () => {
    expect(formatSpeed(Number.POSITIVE_INFINITY)).toBe(NO_VALUE);
  });
});

describe("formatRoute", () => {
  test("is the next waypoint index over the number of waypoints, as given by the stream", () => {
    expect(formatRoute({ next: 2, len: 5 })).toBe("2 / 5");
    expect(formatRoute({ next: 5, len: 5 })).toBe("5 / 5");
  });
  test("no route is the no-value dash", () => {
    expect(formatRoute(undefined)).toBe(NO_VALUE);
    expect(formatRoute(null)).toBe(NO_VALUE);
  });
});
