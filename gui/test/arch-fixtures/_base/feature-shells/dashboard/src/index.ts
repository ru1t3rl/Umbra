import { inspector } from "@umbra/inspector";
import { mapView } from "@umbra/map-view";
import { page } from "./page.ts";

export const route = { path: "/", parts: [mapView, inspector, page] };
