// Public API of @umbra/shell-showcase: the route object the app composes (code-split, ASM-312).

export const route = {
  path: "/showcase",
  label: "Design system",
  icon: "palette",
  nav: false,
  lazy: async () => ({
    Component: (await import("./components/ShowcasePage/ShowcasePage.tsx")).default,
  }),
} as const;
