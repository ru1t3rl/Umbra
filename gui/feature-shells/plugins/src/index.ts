// Public API of @umbra/shell-plugins: the route object the app composes (code-split, ASM-312).

export const route = {
  path: "/plugins",
  label: "Plugins",
  icon: "extension",
  nav: true,
  lazy: async () => ({
    Component: (await import("./components/PluginsPage/PluginsPage.tsx")).default,
  }),
} as const;
