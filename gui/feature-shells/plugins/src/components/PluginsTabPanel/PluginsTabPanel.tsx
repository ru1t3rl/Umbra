import { AdapterList, ComponentList, PluginList } from "@umbra/plugin-status";
import type { ReactElement } from "react";
import type { PluginsTab } from "../../types.ts";

/** The content of one tab; only the open tab is mounted, so only its data is requested. */
export function PluginsTabPanel({ tab }: { tab: PluginsTab }): ReactElement {
  switch (tab) {
    case "plugins":
      return <PluginList />;
    case "adapters":
      return <AdapterList />;
    case "components":
      return <ComponentList />;
  }
}
