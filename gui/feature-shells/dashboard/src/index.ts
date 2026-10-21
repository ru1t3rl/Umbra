// Public API of @umbra/shell-dashboard: the route object the app composes (code-split, ASM-312).

export const route = {
  path: "/",
  label: "Live map",
  lazy: async () => ({
    Component: (await import("./components/DashboardPage/DashboardPage.tsx")).default,
  }),
} as const;
