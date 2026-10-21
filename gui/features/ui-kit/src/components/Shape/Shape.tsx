import type { Affiliation } from "../../types.ts";

const DIAMOND = "M10 1.5 L18.5 10 L10 18.5 L1.5 10 Z";
const QUATREFOIL =
  "M6 2.5 h8 a4 4 0 0 1 4 4 v8 a4 4 0 0 1 -4 4 h-8 a4 4 0 0 1 -4 -4 v-8 a4 4 0 0 1 4 -4 z";

/** APP-6D frame shape: rectangle friend, diamond hostile, square neutral, quatrefoil unknown. */
export function Shape({ affiliation, className }: { affiliation: Affiliation; className: string }) {
  switch (affiliation) {
    case "friend":
      return (
        <rect
          className={
            className === "vt-affil-fill" ? `${className} vt-affil-frame-shape` : className
          }
          x={2}
          y={4}
          width={16}
          height={12}
          rx={1}
        />
      );
    case "hostile":
      return <path className={className} d={DIAMOND} />;
    case "neutral":
      return <rect className={className} x={3} y={3} width={14} height={14} />;
    case "unknown":
      return <path className={className} d={QUATREFOIL} />;
  }
}
