// Public API of @umbra/shell-templates: the route object the app composes (code-split, ASM-312).

export const route = {
  path: "/templates",
  label: "Templates",
  icon: "category",
  nav: true,
  lazy: async () => ({
    Component: (await import("./components/TemplatesPage/TemplatesPage.tsx")).default,
  }),
} as const;
