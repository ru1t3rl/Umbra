// Public API of @umbra/shell-settings: the route object the app composes (code-split, ASM-312).

export const route = {
  path: "/settings",
  label: "Settings",
  icon: "settings",
  nav: true,
  lazy: async () => ({
    Component: (await import("./components/SettingsPage/SettingsPage.tsx")).default,
  }),
} as const;
