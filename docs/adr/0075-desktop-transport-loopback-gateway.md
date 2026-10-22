# ADR 0075: Desktop transport: a loopback gateway in the Tauri app, same origin as the web build

- **Status**: Proposed (frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22); needs a human decision (H-2), because option 2 would need an engine change
- **Requirement IDs**: D-13, REQ-G-6, REQ-A-4, REQ-X-8, spec section 13 ("Tauri shell loading the same build"), ADR 0047 (no engine or contract change in M8), `docs/api.md` ("Confinement, Host and Origin": Tauri origins are M8 work)
- **Affected**: `gui/src-tauri` (new), `@umbra/desktop`, `docs/api.md` (one note at close-out), decision row D-85

## Context
The Tauri webview serves the bundled build from `http://tauri.localhost` (Windows) or
`tauri://localhost` (Linux, macOS). From there, every REST call to `http://127.0.0.1:<port>` is
cross-origin. The server sends no CORS headers. Its stream upgrade accepts only a loopback `Origin` or
an exact `[server] allowed_origins` entry, and its `Host` allow-list protects against DNS rebinding.

The web build and the e2e harness avoid all of this with a same-origin proxy: Vite dev and preview
forward `/api` with `ws: true`, and the engine URL in `/config.json` is empty. ADR 0047 forbids M8 to
change `crates/` or `api/`.

## Options
1. **A small loopback gateway inside the Tauri app (chosen).** It is a Rust HTTP server in
   `src-tauri` on `127.0.0.1:0` that:
   - serves the embedded `dist/`;
   - answers `/config.json` with `{"engineUrl":""}`;
   - reverse-proxies `/api/*`, including the WebSocket upgrade, to the engine (managed or remote,
     ADR 0076).

   The window loads `http://127.0.0.1:<port>/`. The frontend code and config are byte-identical to the
   web build, and the server sees a loopback `Origin` and `Host`.
   - Pros: no engine change; works with remote engines without reconfiguring them; the same proxy
     semantics the e2e suite already proves.
   - Cons: about 300 lines of Rust we own, with tests; one more loopback port.
2. **Engine-side CORS plus the Tauri origins.** A new `[server] cors_origins` with preflight handling,
   and the Tauri origins in `allowed_origins`. Tauri then loads the build from its custom protocol and
   calls the engine directly.
   - Pros: no proxy.
   - Cons: an engine and contract-documentation change (a gap request against M7's branch); a larger
     security surface on a server without authentication (REQ-X-8); every remote engine must list the
     origin; web and desktop get different code paths.
3. **Tauri `plugin-http` and `plugin-websocket` behind injected `fetch` and socket factories** (the data
   mutator and `connectEngineStream` already accept injection).
   - Pros: no proxy, no engine change.
   - Cons: two transports to test; the plugin's scope configuration per engine URL; plugin-websocket's
     API is not `WebSocket`-compatible (an adapter we own); `Origin` behaviour differs by plugin
     version; e2e of the web path does not cover the desktop path.

## Decision
Option 1, pending the human's choice (H-2).
- The gateway lives in `gui/src-tauri/src/gateway/`. It uses the async HTTP stack Tauri already
  brings: `tokio` plus `hyper`/`axum` and `tokio-tungstenite`, versions recorded in the WI.
- It forwards method, path, query, body and the `Content-Type`, `Accept`, `Upgrade`,
  `Sec-WebSocket-*` headers. It sets `Host` to the target authority, keeps the browser's loopback
  `Origin`, and streams bodies without buffering past `body_limit_bytes`.
- It binds only to loopback, serves only its embedded assets, and refuses any path outside `/`,
  `/assets/*`, `/config.json`, `/umbra_icon.svg` and `/api/*` (404). It logs to the engine-process log
  source (ADR 0071).
- Tauri IPC is granted only to the gateway origin through a remote capability with the runtime port
  (ASM-839); the CSP allows `self` and `ws://127.0.0.1:<port>` only.
- `docs/api.md` keeps its server rules. At close-out a one-line note says that the desktop app reaches
  the engine through a loopback gateway, so no Tauri origin is needed.

## Consequences
- Easier: one frontend build and one proven transport for web, e2e and desktop; no engine work.
- Harder: Rust code under `gui/` with its own tests, lints and licence check (ADR 0077).
- Revisit when the engine gains authentication or CORS for other reasons (then option 2 may be
  simpler), or when the gateway's latency shows up in the 10,000-entity stream measurement.
