export const STREAM_PATH = "/api/v1/stream?encoding=json";

/**
 * The stream URL for an engine URL from the runtime config: http -> ws, https -> wss, plus the stream
 * path. An empty `engineUrl` means the page origin (`pageOrigin`, e.g. `location.origin`).
 */
export function streamUrl(engineUrl: string, pageOrigin: string): string {
  const base = (engineUrl === "" ? pageOrigin : engineUrl).replace(/\/+$/, "");
  const ws = base.replace(/^http(s?):/i, (_m, s: string) => `ws${s}:`);
  return `${ws}${STREAM_PATH}`;
}
