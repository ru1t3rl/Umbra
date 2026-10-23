/**
 * The app version: `version` of `gui/package.json`, put into the build by `vite.config.ts` as
 * `import.meta.env.VITE_UMBRA_APP_VERSION` (ASM-996). Outside a Vite build (unit tests) it is the package's
 * own placeholder version.
 */
const injected: unknown = import.meta.env.VITE_UMBRA_APP_VERSION;

export const APP_VERSION: string = typeof injected === "string" ? injected : "0.0.0";
