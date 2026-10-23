// WI-10 / ASM-880 (written before the prop exists): `EntityRow presentational` renders the same content as
// the row (affiliation tag with its sr-only word, name, id, meta, status badge, stale) WITHOUT the button and
// without role option, so it can sit inside a VirtualList option. The default (no prop) stays the EntityList
// button option (EntityRow.test.tsx). ASM-937 (e).
import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { VirtualList } from "../VirtualList/VirtualList.tsx";
import { EntityRow, type EntityRowProps } from "./EntityRow.tsx";

afterEach(cleanup);

const base: EntityRowProps = {
  name: "Alpha 1-1",
  id: "#12",
  affiliation: "hostile",
  meta: "tank",
  status: { label: "Running", tone: "success" },
  presentational: true,
};

test("a presentational row has no button and no option role", () => {
  render(<EntityRow {...base} />);
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByRole("option")).toBeNull();
});

test("it still shows the affiliation word, the name, the id, the meta and the status", () => {
  const { container } = render(<EntityRow {...base} />);
  expect(container).toHaveTextContent("Hostile");
  expect(container).toHaveTextContent("Alpha 1-1");
  expect(container).toHaveTextContent("#12");
  expect(container).toHaveTextContent("tank");
  expect(container).toHaveTextContent("Running");
});

test("stale replaces the status with the word Stale and dims the row", () => {
  const { container } = render(<EntityRow {...base} stale />);
  expect(container).toHaveTextContent("Stale");
  expect(container).not.toHaveTextContent("Running");
  expect(container.querySelector(".vt-entity-stale")).not.toBeNull();
});

test("it uses the entity row classes so the density and tokens are the same", () => {
  const { container } = render(<EntityRow {...base} />);
  expect(container.querySelector(".vt-entity")).not.toBeNull();
  expect(container.querySelector(".vt-entity-name")).toHaveTextContent("Alpha 1-1");
});

test("inside a VirtualList the list owns the single option per row", () => {
  render(
    <VirtualList
      role="listbox"
      label="Entities"
      count={2}
      rowHeight="row-lg"
      getRowId={(i) => `r-${i}`}
      renderRow={() => <EntityRow {...base} />}
      activeIndex={null}
      onActiveIndexChange={() => {}}
      onActivate={() => {}}
      observeSize={(_el, onSize) => {
        onSize(400);
        return () => {};
      }}
    />,
  );
  expect(screen.getAllByRole("option")).toHaveLength(2);
  expect(screen.queryAllByRole("button")).toHaveLength(0);
});

test("without the prop the row is still the EntityList button option", () => {
  render(<EntityRow {...base} presentational={undefined} />);
  expect(screen.getByRole("option")).toBeInTheDocument();
});
