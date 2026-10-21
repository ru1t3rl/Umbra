import { describe, expect, spyOn, test } from "bun:test";
import { ConfigError, DEFAULT_CONFIG, loadRuntimeConfig, parseRuntimeConfig } from "./config.ts";

const respond = (body: string, status = 200): typeof fetch =>
  (async () => new Response(body, { status })) as unknown as typeof fetch;

describe("loadRuntimeConfig", () => {
  test("an empty object gives the defaults", async () => {
    expect(await loadRuntimeConfig(respond("{}"))).toEqual(DEFAULT_CONFIG);
  });

  test("an empty engineUrl stays empty and means the same origin", async () => {
    const config = await loadRuntimeConfig(respond('{"engineUrl":""}'));
    expect(config.engineUrl).toBe("");
  });

  test("a missing file (404) warns and gives the defaults", async () => {
    const warn = spyOn(console, "warn").mockImplementation(() => {});
    try {
      expect(await loadRuntimeConfig(respond("", 404))).toEqual(DEFAULT_CONFIG);
      expect(warn).toHaveBeenCalledWith("config.json not found (404); using same-origin defaults");
    } finally {
      warn.mockRestore();
    }
  });

  test("the file is fetched under the base URL", async () => {
    const urls: string[] = [];
    const spy = (async (url: string) => {
      urls.push(url);
      return new Response("{}");
    }) as unknown as typeof fetch;
    await loadRuntimeConfig(spy);
    await loadRuntimeConfig(spy, "/umbra/");
    expect(urls).toEqual(["/config.json", "/umbra/config.json"]);
  });

  test("an engineUrl is passed through", async () => {
    const config = await loadRuntimeConfig(respond('{"engineUrl":"http://127.0.0.1:7878"}'));
    expect(config.engineUrl).toBe("http://127.0.0.1:7878");
  });

  test("basemap.styleUrl is passed through", async () => {
    const config = await loadRuntimeConfig(
      respond('{"engineUrl":"","basemap":{"styleUrl":"https://tiles.example/style.json"}}'),
    );
    expect(config.basemap).toEqual({ styleUrl: "https://tiles.example/style.json" });
  });

  test("invalid JSON is rejected", async () => {
    await expect(loadRuntimeConfig(respond("{nope"))).rejects.toBeInstanceOf(ConfigError);
  });

  test("a server error and a network failure are rejected", async () => {
    await expect(loadRuntimeConfig(respond("", 500))).rejects.toBeInstanceOf(ConfigError);
    const failing = (async () => {
      throw new TypeError("offline");
    }) as unknown as typeof fetch;
    await expect(loadRuntimeConfig(failing)).rejects.toBeInstanceOf(ConfigError);
  });
});

describe("parseRuntimeConfig", () => {
  test.each(["", "http://127.0.0.1:7878", "https://engine.example/base"])(
    "accepts the engineUrl %p",
    (engineUrl) => {
      expect(parseRuntimeConfig({ engineUrl }).engineUrl).toBe(engineUrl);
    },
  );

  test.each([
    ["null", null],
    ["an array", []],
    ["a string", "x"],
    ["a numeric engineUrl", { engineUrl: 5 }],
    ["a relative engineUrl", { engineUrl: "/engine" }],
    ["a host without scheme", { engineUrl: "engine:7878" }],
    ["a ws engineUrl", { engineUrl: "ws://engine:7878" }],
    ["a javascript engineUrl", { engineUrl: "javascript:alert(1)" }],
    ["a string basemap", { basemap: "x" }],
    ["a basemap without styleUrl", { basemap: {} }],
    ["an empty styleUrl", { basemap: { styleUrl: "" } }],
    ["a numeric styleUrl", { basemap: { styleUrl: 1 } }],
  ])("rejects %s", (_name, raw) => {
    expect(() => parseRuntimeConfig(raw)).toThrow(ConfigError);
  });
});
