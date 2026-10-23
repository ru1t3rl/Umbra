# @umbra/shell-templates

Shell (page composition, no logic): the template browser page at `/templates` (M8 WI-15).

- `route` = `{ path: "/templates", label: "Templates", icon: "category", nav: true, lazy }`; the app registers it in `src/router.tsx` (ASM-312). It appears in the page navigation (`AppNav`).
- Layout: a docked `CommandBar` (`AppNav`, `ThemeToggle`), then two columns: `TemplateList` (320 px) and `TemplateDetail` of the selected template. `useTemplateSelection()` of `@umbra/template-browser` holds the selection (it follows the scenario, ASM-993); with none selected the shell shows its own hint line. The bar has no visible title (the current navigation link names the page; the `h1` is visually hidden).
- Depends on `@umbra/ui-kit` and `@umbra/template-browser` only; `@umbra/data` and `@umbra/engine-stream` are devDependencies of the tests (`src/testing.tsx`).
- Screenshot and axe target of `e2e/templates.spec.ts` (Ops and Day baselines under `e2e/__screenshots__/linux/`). The lazy chunk has a size budget in `src/e2eHook.build.test.ts` (ASM-991).
