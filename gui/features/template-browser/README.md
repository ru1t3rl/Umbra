# @umbra/template-browser

Domain feature (tier `domain`): the templates of the loaded scenario, one template in detail and a spawn form (M8 WI-12). Depends on `@umbra/data` (`useListTemplates`, `useGetTemplate`, `useListComponents`, `useSpawnEntity`), `@umbra/engine-stream` (run state), `@umbra/schema-form` (read-only parameter forms) and `@umbra/ui-kit`.

## Public API
- `TemplateList({ selectedId, onSelect })` (`TemplateListProps`): a search field "Search templates", a polite "N of M templates" status and a listbox "Templates". Each row holds the id, the description, the motion model and the behavior ("No behavior" when none). The search is a case-insensitive substring match over id, description, motion model and behavior. The list is a ui-kit `VirtualList` that windows its rows only above 50 (filtered) templates (`virtualize`, ASM-862); the element stays the same when a filter crosses 50. One tab stop, arrows, Home, End, PageUp, PageDown, Enter and click; `aria-selected` marks `selectedId`. A failed request is a danger Alert with the engine's title and detail.
- `TemplateDetail({ templateId })` (`TemplateDetailProps`): nothing for `null`. A heading with the id, the description, the inheritance chain (template first, root last), the defining file, then one section per component (motion model, sensors in template order, behavior) with a read-only `SchemaForm` and the spawn form.
  - The header and the spawn form belong to the `templateId` prop, not to the loaded data: switching to a template that is not cached shows "Loading template" and keeps what was typed (Name is cleared).
  - A component that `GET /components` does not list, or a components request that finally failed, shows its parameters as JSON (ASM-816).
- The spawn form is private: template preset, Latitude and Longitude (required, range-checked on submit), Altitude, Heading, Speed, Name, Force, Group; problems from the engine inline; success is announced in a polite status region that stays mounted (ASM-949, ASM-954).

## Run-state rules (ASM-948)
- `idle` shows "Load a scenario to browse its templates" with an "Open the dashboard" link to `/`, and requests nothing.
- Every other state, including `stopped` and an unknown state (no frame yet), requests normally.
- The spawn button is enabled in `ready`, `running` and `paused` only; the fields stay editable.

## Components cache
`GET /components` (no kind filter) is requested once per session: `staleTime` and `gcTime` are infinite, so remounts and other templates reuse the answer. The app invalidates it on a session change through `useInvalidateEngineQueries` (ASM-884). The data layer's default retry applies (transient failures retry, 4xx do not). `GET /templates` is requested once per mount of the list.

## Providers
Needs an `EngineStreamProvider`, a `DataProvider` and a `NavigationProvider` (the idle link) above it.

Pure model (not exported): `componentIndex`, `paramsSections`, `filterTemplates`, `spawnForm` (parsing, validation, request body, `canSpawn`).
