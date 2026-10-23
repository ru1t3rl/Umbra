import { AppNav, NavigationProvider } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";
import { PAGES } from "../../samplePages.ts";

/** AppNav `iconOnly`: the variant a crowded command bar uses (icons with a name and a tooltip). No bar of its own: the page keeps its five command bars. */
export function NavIconDemo(): ReactElement {
  const [path, setPath] = useState("/");
  return (
    <div id="showcase-appnav-icons" className="showcase-scroll-x">
      <NavigationProvider items={PAGES} currentPath={path} navigate={setPath}>
        <AppNav iconOnly label={t("nav.icons-label")} />
      </NavigationProvider>
    </div>
  );
}
