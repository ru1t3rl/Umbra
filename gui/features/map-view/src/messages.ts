import { defineMessages } from "@umbra/ui-kit";

export const t = defineMessages({
  "map.label": "Live map",
  "map.count.one": "{n} entity on the map",
  "map.count.other": "{n} entities on the map",
  "map.error.title": "The map could not start",
  "map.error.colors":
    "The theme colours could not be read. They are read again when the theme changes; if it persists, reload the page.",
  "map.error.webgl": "The map needs WebGL, which this device does not provide.",
  "map.error.lostTitle": "The map lost its graphics context",
  "map.error.lostBody": "It resumes by itself when the device restores the graphics context.",
});
