import type { ReactElement, ReactNode } from "react";

export interface SectionProps {
  id: string;
  heading: string;
  children: ReactNode;
}

export function Section({ id, heading, children }: SectionProps): ReactElement {
  return (
    <section className="showcase-section" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="type-title-sm">
        {heading}
      </h3>
      {children}
    </section>
  );
}
