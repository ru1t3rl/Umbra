//! Reading a scenario file into a [`Scenario`] (ADR 0010, ASM-058, ASM-060).

use std::collections::BTreeMap;
use std::path::PathBuf;

use cgf_core::{
    ComponentId, ComponentKind, Meters, MetersPerSecond, Radians, TemplateId, TickRate,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::loader::Loader;
use crate::position::read_position;
use crate::reader::Reader;
use crate::resolve::{ComponentRef, LoadedTemplates, missing_template_notes};
use crate::scenario::{ClockSetting, Placement, RoutePlan, Scenario};
use crate::source::SourceFile;
use crate::template::{RawComponent, Version, check_version, component};
use crate::value::{LEntry, LKind, LTable, LValue, Origin, parse};
use cgf_core::Position;

const TOP_KEYS: [&str; 4] = ["schema_version", "scenario", "terrain", "entities"];
const SCENARIO_KEYS: [&str; 7] = [
    "name",
    "seed",
    "tick_rate_hz",
    "clock",
    "time_scale",
    "duration_s",
    "template_paths",
];
const ENTITY_KEYS: [&str; 8] = [
    "template",
    "name",
    "group",
    "position",
    "heading_deg",
    "speed_mps",
    "overrides",
    "route",
];
const ROUTE_KEYS: [&str; 3] = ["waypoints", "arrival_radius_m", "cruise_speed_mps"];
const DEFAULT_TERRAIN: &str = "builtin.terrain.flat";
const TIME_SCALE_RANGE: (f64, f64) = (1e-3, 1e6);

/// The `template_paths` strings of a scenario file, leniently: problems are reported by the full
/// read, not here.
pub(crate) fn template_paths(file: &SourceFile) -> Vec<(String, Origin)> {
    let (table, _) = parse(file);
    let paths = table
        .as_ref()
        .and_then(|t| t.get("scenario"))
        .and_then(|v| match v.kind() {
            LKind::Table(t) => t.get("template_paths"),
            _ => None,
        });
    match paths.map(LValue::kind) {
        Some(LKind::Array(items)) => items
            .iter()
            .filter_map(|v| match v.kind() {
                LKind::String(s) => Some((s.clone(), v.origin().clone())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether the file has a top-level `scenario` key (ASM-053).
pub(crate) fn is_scenario(file: &SourceFile) -> bool {
    parse(file).0.is_some_and(|t| t.get("scenario").is_some())
}

struct Settings {
    name: String,
    seed: u64,
    tick_rate: TickRate,
    clock: ClockSetting,
    duration_ticks: u64,
}

impl Loader<'_> {
    /// Loads the templates and reads the scenario; every diagnostic is returned unsorted.
    /// `unread` lists template paths that could not be read; unknown template names get a note
    /// about them.
    pub(crate) fn build_scenario(
        &self,
        file: &SourceFile,
        templates: &[SourceFile],
        unread: &[PathBuf],
    ) -> (Option<Scenario>, Vec<Diagnostic>) {
        let loaded = self.load_templates(templates, unread);
        self.build_scenario_with(file, &loaded, unread)
    }

    /// Reads the scenario against templates that are already loaded.
    pub(crate) fn build_scenario_with(
        &self,
        file: &SourceFile,
        loaded: &LoadedTemplates,
        unread: &[PathBuf],
    ) -> (Option<Scenario>, Vec<Diagnostic>) {
        let mut diagnostics = loaded.diagnostics.clone();
        let (scenario, own) = self.read_scenario(file, loaded, unread);
        diagnostics.extend(own);
        let scenario = if diagnostics.is_empty() {
            scenario
        } else {
            None
        };
        (scenario, diagnostics)
    }

    pub(crate) fn read_scenario(
        &self,
        file: &SourceFile,
        loaded: &LoadedTemplates,
        unread: &[PathBuf],
    ) -> (Option<Scenario>, Vec<Diagnostic>) {
        let (table, mut diagnostics) = parse(file);
        let Some(table) = table else {
            return (None, diagnostics);
        };
        if diagnostics.iter().any(|d| d.code == DiagnosticCode::Syntax) {
            return (None, diagnostics);
        }
        let mut reader = Reader::new(file);
        if matches!(check_version(&mut reader, &table), Version::Newer) {
            return (None, reader.into_diagnostics());
        }
        reader.unknown_keys(&table, &TOP_KEYS);
        let scenario = self.read_body(&mut reader, &table, file, loaded, unread);
        diagnostics.extend(reader.into_diagnostics());
        (scenario, diagnostics)
    }

    fn read_body(
        &self,
        reader: &mut Reader<'_>,
        table: &LTable,
        file: &SourceFile,
        loaded: &LoadedTemplates,
        unread: &[PathBuf],
    ) -> Option<Scenario> {
        let section = table.get("scenario");
        if section.is_none() {
            let message = "missing required table `[scenario]`".to_string();
            reader.error(DiagnosticCode::MissingKey, message, table.origin());
        }
        let settings = section
            .and_then(|s| reader.table("scenario", s))
            .and_then(|t| read_settings(reader, t));
        let fallback = section.map_or(table.origin(), LValue::origin);
        let terrain = self.read_terrain(reader, table, fallback);
        let entities = self.read_entities(reader, table, loaded, unread);
        let settings = settings?;
        Some(Scenario {
            name: settings.name,
            source: file.path().to_path_buf(),
            seed: settings.seed,
            tick_rate: settings.tick_rate,
            clock: settings.clock,
            duration_ticks: settings.duration_ticks,
            terrain: terrain?,
            templates: loaded.set.clone(),
            entities: entities?,
        })
    }

    fn read_terrain(
        &self,
        reader: &mut Reader<'_>,
        table: &LTable,
        fallback: &Origin,
    ) -> Option<ComponentRef> {
        let section = match table.get("terrain") {
            Some(v) => Some(reader.table("terrain", v)?),
            None => None,
        };
        let origin = section.map_or(fallback, LTable::origin);
        if let Some(section) = section {
            reader.unknown_keys(section, &["provider", "params"]);
        }
        let provider = match section.and_then(|s| s.get("provider")) {
            Some(v) => reader
                .id("terrain.provider", v, ComponentId::new)
                .map(|id| (id, v.origin().clone())),
            None => ComponentId::new(DEFAULT_TERRAIN)
                .ok()
                .map(|id| (id, origin.clone())),
        };
        let params = match section.and_then(|s| s.get("params")) {
            Some(v) => reader.table("terrain.params", v).cloned(),
            None if section.is_some_and(|s| s.get("provider").is_some()) => {
                Some(LTable::from_entries(origin.clone(), BTreeMap::new()))
            }
            None => Some(default_terrain_params(origin)),
        };
        let (id, at) = provider?;
        let mut out = Vec::new();
        let Some(params) = params else {
            // The parameters are unreadable, but an unknown provider is still reported.
            self.check_registered(ComponentKind::Terrain, &id, &at, &mut out);
            reader.extend(out);
            return None;
        };
        let json = self.check_component(ComponentKind::Terrain, &id, &at, &params, &mut out);
        reader.extend(out);
        Some(ComponentRef { id, params: json? })
    }

    fn read_entities(
        &self,
        reader: &mut Reader<'_>,
        table: &LTable,
        loaded: &LoadedTemplates,
        unread: &[PathBuf],
    ) -> Option<Vec<Placement>> {
        let Some(value) = table.get("entities") else {
            return Some(Vec::new());
        };
        let items = reader.array("entities", value)?;
        let mut names: BTreeMap<String, Origin> = BTreeMap::new();
        let placements: Vec<Option<Placement>> = items
            .iter()
            .map(|item| self.read_entity(reader, item, loaded, unread, &mut names))
            .collect();
        placements.into_iter().collect()
    }

    fn read_entity(
        &self,
        reader: &mut Reader<'_>,
        value: &LValue,
        loaded: &LoadedTemplates,
        unread: &[PathBuf],
        names: &mut BTreeMap<String, Origin>,
    ) -> Option<Placement> {
        let table = reader.table("entities", value)?;
        reader.unknown_keys(table, &ENTITY_KEYS);
        let template = entity_template(reader, table, loaded, unread);
        let name = optional_string(reader, table, "name");
        let group = optional_string(reader, table, "group");
        check_duplicate_name(reader, table, names);
        let position = required(reader, table, "position").and_then(|v| read_position(reader, v));
        let heading = match table.get("heading_deg") {
            Some(v) => reader.number("heading_deg", v).map(Radians::from_degrees),
            None => Some(Radians::from_degrees(0.0)),
        };
        let speed = match table.get("speed_mps") {
            Some(v) => non_negative(reader, "speed_mps", v).map(MetersPerSecond),
            None => Some(MetersPerSecond(0.0)),
        };
        let overrides = match table.get("overrides") {
            None => Some(None),
            Some(v) => read_overrides(reader, v).map(Some),
        };
        let route = match table.get("route") {
            None => Some(None),
            Some(v) => read_route(reader, v).map(Some),
        };
        // The override is checked even when something else about the entity is wrong.
        let motion = if let Some(t) = template {
            overrides
                .as_ref()
                .and_then(|o| self.placement_motion(reader, t, o.as_ref()))
        } else {
            if let Some(Some(over)) = &overrides {
                self.check_override_alone(reader, over);
            }
            None
        };
        let (template, position) = (template?, position?);
        let (heading, speed) = (heading?, speed?);
        let (route, motion) = (route?, motion?);
        Some(Placement {
            template: template.id.clone(),
            name,
            group,
            position,
            heading,
            speed,
            motion,
            route,
        })
    }

    /// What an override says by itself when its entity's template is unknown: the model ID and
    /// the parameters it sets (a missing one may come from the template).
    fn check_override_alone(&self, reader: &mut Reader<'_>, over: &RawComponent) {
        let Some((id, at)) = &over.id else { return };
        let mut out = Vec::new();
        if self.check_registered(ComponentKind::MotionModel, id, at, &mut out) {
            self.check_own_params(
                ComponentKind::MotionModel,
                id,
                over.params.as_ref(),
                &mut out,
            );
        }
        reader.extend(out);
    }

    /// The template's motion merged with the entity's overrides and checked again; unchanged
    /// templates are not rebuilt.
    fn placement_motion(
        &self,
        reader: &mut Reader<'_>,
        template: &crate::resolve::ResolvedTemplate,
        overrides: Option<&RawComponent>,
    ) -> Option<ComponentRef> {
        let Some(over) = overrides.filter(|o| o.id.is_some() || o.params.is_some()) else {
            return Some(template.motion.clone());
        };
        let mut id = (
            template.motion.id.clone(),
            template.located.id_origin.clone(),
        );
        let mut entries: BTreeMap<String, LEntry> = template
            .located
            .params
            .iter()
            .map(|(k, e)| (k.to_string(), e.clone()))
            .collect();
        let mut origin = template.located.params.origin().clone();
        if let Some((new_id, at)) = &over.id {
            if *new_id != id.0 {
                // The parameters belong to the new model: errors are located in the scenario.
                entries.clear();
                origin = over.origin.clone();
            }
            id = (new_id.clone(), at.clone());
        }
        if let Some(params) = &over.params {
            entries.extend(params.iter().map(|(k, e)| (k.to_string(), e.clone())));
            origin = params.origin().clone();
        }
        let merged = LTable::from_entries(origin, entries);
        let mut out = Vec::new();
        let json =
            self.check_component(ComponentKind::MotionModel, &id.0, &id.1, &merged, &mut out);
        reader.extend(out);
        Some(ComponentRef {
            id: id.0,
            params: json?,
        })
    }
}

fn default_terrain_params(origin: &Origin) -> LTable {
    let height = LValue::synthetic(origin, LKind::Float(0.0));
    let mut entries = BTreeMap::new();
    entries.insert("height_m".to_string(), LEntry::synthetic(origin, height));
    LTable::from_entries(origin.clone(), entries)
}

fn optional_string(reader: &mut Reader<'_>, table: &LTable, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|v| reader.string(key, v))
        .map(str::to_string)
}

fn check_duplicate_name(
    reader: &mut Reader<'_>,
    table: &LTable,
    names: &mut BTreeMap<String, Origin>,
) {
    let Some(value) = table.get("name") else {
        return;
    };
    let LKind::String(name) = value.kind() else {
        return;
    };
    if name.is_empty() {
        return;
    }
    match names.get(name) {
        Some(first) => {
            let message = format!("entity name `{name}` is used more than once");
            let note_at = first.location();
            reader.push_diagnostic(
                Diagnostic::new(
                    DiagnosticCode::DuplicateName,
                    message,
                    Some(value.origin().location()),
                )
                .with_note("first used here", Some(note_at)),
            );
        }
        None => {
            names.insert(name.clone(), value.origin().clone());
        }
    }
}

/// A number that must be finite and not negative.
fn non_negative(reader: &mut Reader<'_>, name: &str, value: &LValue) -> Option<f64> {
    let n = reader.number(name, value)?;
    if n.is_finite() && n >= 0.0 {
        Some(n)
    } else {
        let message = format!("`{name}` must not be negative, found {n}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        None
    }
}

/// A number that must be finite and greater than zero.
fn positive(reader: &mut Reader<'_>, name: &str, value: &LValue) -> Option<f64> {
    let n = reader.number(name, value)?;
    if n.is_finite() && n > 0.0 {
        Some(n)
    } else {
        let message = format!("`{name}` must be greater than 0, found {n}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        None
    }
}

/// The template an entity names; an unknown name is reported (with a note about files that may
/// define it) unless the template exists but is unusable, whose error is already reported.
fn entity_template<'l>(
    reader: &mut Reader<'_>,
    table: &LTable,
    loaded: &'l LoadedTemplates,
    unread: &[PathBuf],
) -> Option<&'l crate::resolve::ResolvedTemplate> {
    let Some(value) = table.get("template") else {
        let message = "missing required key `template`".to_string();
        reader.error(DiagnosticCode::MissingKey, message, table.origin());
        return None;
    };
    let id = reader.id("template", value, TemplateId::new)?;
    if let Some(template) = loaded.set.get(&id) {
        return Some(template);
    }
    if !loaded.unusable.contains(&id) {
        let message = format!("template `{id}` is not defined");
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::UnknownTemplate,
            message,
            Some(value.origin().location()),
        );
        for note in missing_template_notes(&loaded.unreadable, unread) {
            diagnostic = diagnostic.with_note(note, None);
        }
        reader.push_diagnostic(diagnostic);
    }
    None
}

fn read_settings(reader: &mut Reader<'_>, table: &LTable) -> Option<Settings> {
    reader.unknown_keys(table, &SCENARIO_KEYS);
    let name =
        required(reader, table, "name").and_then(|v| reader.string("name", v).map(str::to_string));
    let seed = required(reader, table, "seed").and_then(|v| read_seed(reader, v));
    let tick_rate = required(reader, table, "tick_rate_hz").and_then(|v| read_tick_rate(reader, v));
    let clock = read_clock(reader, table);
    let duration = required(reader, table, "duration_s");
    let duration_ticks = match (duration, tick_rate) {
        (Some(v), Some(rate)) => read_duration(reader, v, rate),
        (Some(v), None) => positive(reader, "duration_s", v).and(None),
        _ => None,
    };
    read_template_paths(reader, table);
    Some(Settings {
        name: name?,
        seed: seed?,
        tick_rate: tick_rate?,
        clock: clock?,
        duration_ticks: duration_ticks?,
    })
}

fn required<'t>(reader: &mut Reader<'_>, table: &'t LTable, key: &str) -> Option<&'t LValue> {
    let value = table.get(key);
    if value.is_none() {
        let message = format!("missing required key `{key}`");
        reader.error(DiagnosticCode::MissingKey, message, table.origin());
    }
    value
}

fn read_seed(reader: &mut Reader<'_>, value: &LValue) -> Option<u64> {
    let seed = reader.integer("seed", value)?;
    u64::try_from(seed).ok().or_else(|| {
        let message = format!("`seed` must not be negative, found {seed}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        None
    })
}

fn read_tick_rate(reader: &mut Reader<'_>, value: &LValue) -> Option<TickRate> {
    let hz = reader.integer("tick_rate_hz", value)?;
    let rate = u32::try_from(hz).ok().and_then(TickRate::new);
    if rate.is_none() {
        let message =
            format!("`tick_rate_hz` must be a positive whole number of hertz, found {hz}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    rate
}

/// `ceil(duration_s * hz)`, except that a product within two ULPs (of the product) of a whole number of at least 1
/// is that number, so `0.1 s` at 30 Hz is 3 ticks, not 4 (ASM-066). A tiny positive duration is
/// one tick, never zero.
fn read_duration(reader: &mut Reader<'_>, value: &LValue, rate: TickRate) -> Option<u64> {
    let seconds = positive(reader, "duration_s", value)?;
    let product = seconds * f64::from(rate.hz());
    let nearest = product.round();
    let ticks =
        if nearest >= 1.0 && (product - nearest).abs() <= 2.0 * (product.next_up() - product) {
            nearest
        } else {
            product.ceil().max(1.0)
        };
    // 2^64 as f64; anything at or above does not fit in u64.
    if ticks >= 18_446_744_073_709_551_616.0 {
        let message = format!("`duration_s` of {seconds} s is {ticks} ticks, which does not fit");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked to be whole and within [1, 2^64) above"
    )]
    Some(ticks as u64)
}

fn read_clock(reader: &mut Reader<'_>, table: &LTable) -> Option<ClockSetting> {
    let scale = table.get("time_scale");
    let Some(clock) = table.get("clock") else {
        return match scale {
            Some(v) => {
                let message = "`time_scale` needs `clock = \"scaled\"`".to_string();
                reader.error(DiagnosticCode::InvalidValue, message, v.origin());
                None
            }
            None => Some(ClockSetting::AsFastAsPossible),
        };
    };
    let Some(mode) = reader.string("clock", clock) else {
        // A bad `time_scale` is reported even when the clock itself is unreadable.
        if let Some(v) = scale {
            reader.number("time_scale", v);
        }
        return None;
    };
    if mode == "scaled" {
        return read_scaled(reader, clock, scale);
    }
    let setting = match mode {
        "afap" => ClockSetting::AsFastAsPossible,
        "realtime" => ClockSetting::RealTime,
        "stepped" => ClockSetting::Stepped,
        other => {
            let message =
                format!("unknown clock `{other}` (expected afap, realtime, stepped or scaled)");
            reader.error(DiagnosticCode::InvalidValue, message, clock.origin());
            return None;
        }
    };
    if let Some(v) = scale {
        let message = format!("`time_scale` is only valid with `clock = \"scaled\"`, not `{mode}`");
        reader.error(DiagnosticCode::InvalidValue, message, v.origin());
        return None;
    }
    Some(setting)
}

fn read_scaled(
    reader: &mut Reader<'_>,
    clock: &LValue,
    scale: Option<&LValue>,
) -> Option<ClockSetting> {
    let Some(scale) = scale else {
        let message = "`clock = \"scaled\"` needs `time_scale`".to_string();
        reader.error(DiagnosticCode::InvalidValue, message, clock.origin());
        return None;
    };
    let factor = reader.number("time_scale", scale)?;
    let (low, high) = TIME_SCALE_RANGE;
    if (low..=high).contains(&factor) {
        Some(ClockSetting::Scaled(factor))
    } else {
        let message = format!("`time_scale` must be within [{low}, {high}], found {factor}");
        reader.error(DiagnosticCode::InvalidValue, message, scale.origin());
        None
    }
}

fn read_template_paths(reader: &mut Reader<'_>, table: &LTable) {
    if let Some(value) = table.get("template_paths")
        && let Some(items) = reader.array("template_paths", value)
    {
        for item in items {
            let _ = reader.string("template_paths entry", item);
        }
    }
}

/// `[entities.overrides]`: only `motion` (`model`, `params`) in v1.
fn read_overrides(reader: &mut Reader<'_>, value: &LValue) -> Option<RawComponent> {
    let table = reader.table("overrides", value)?;
    reader.unknown_keys(table, &["motion"]);
    match table.get("motion") {
        Some(v) => component(reader, "overrides.motion", "model", v),
        None => Some(RawComponent {
            origin: table.origin().clone(),
            id: None,
            params: None,
        }),
    }
}

fn read_route(reader: &mut Reader<'_>, value: &LValue) -> Option<RoutePlan> {
    let table = reader.table("route", value)?;
    reader.unknown_keys(table, &ROUTE_KEYS);
    let waypoints = required(reader, table, "waypoints").and_then(|v| read_waypoints(reader, v));
    let radius = required(reader, table, "arrival_radius_m")
        .and_then(|v| positive(reader, "arrival_radius_m", v));
    let speed = required(reader, table, "cruise_speed_mps")
        .and_then(|v| positive(reader, "cruise_speed_mps", v));
    Some(RoutePlan {
        waypoints: waypoints?,
        arrival_radius: Meters(radius?),
        cruise_speed: MetersPerSecond(speed?),
    })
}

fn read_waypoints(reader: &mut Reader<'_>, value: &LValue) -> Option<Vec<Position>> {
    let items = reader.array("waypoints", value)?;
    if items.is_empty() {
        let message = "`waypoints` needs at least one waypoint".to_string();
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return None;
    }
    let points: Vec<Option<Position>> = items.iter().map(|v| read_position(reader, v)).collect();
    points.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use cgf_core::Registry;
    use proptest::prelude::*;

    use super::*;

    fn run(text: &str) {
        let registry = Registry::new();
        if let Ok(loader) = Loader::new(&registry) {
            let _ = loader.build_scenario(&SourceFile::new("s.toml", text), &[], &[]);
        }
    }

    #[test]
    fn duration_that_is_a_float_artifact_of_a_whole_tick_count_does_not_round_up() {
        // 0.1 s at 30 Hz is 3.0000000000000004 as a float product: 3 ticks, not 4 (ASM-066).
        let text = "schema_version = 1\n[scenario]\nname = \"n\"\nseed = 1\ntick_rate_hz = 30\n\
                    duration_s = 0.1\n[terrain]\nprovider = \"a.b\"\n";
        let file = SourceFile::new("s.toml", text);
        let (table, _) = parse(&file);
        let mut reader = Reader::new(&file);
        let settings = table
            .as_ref()
            .and_then(|t| t.get("scenario"))
            .and_then(|v| {
                reader
                    .table("scenario", v)
                    .and_then(|t| read_settings(&mut reader, t))
            });

        assert_eq!(settings.map(|s| s.duration_ticks), Some(3));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// NFR-2: no scenario file content makes the reader panic.
        #[test]
        fn scenario_reader_never_panics(text in "\\PC{0,300}") {
            run(&text);
        }

        #[test]
        fn scenario_reader_never_panics_on_scenario_shaped_text(
            text in "(schema_version|scenario|entities|position|route|waypoints|overrides|motion|params|terrain|clock|seed|duration_s|tick_rate_hz|ecef_m|lat_deg|[a-z.]{1,5}|\"[a-z.]{0,6}\"|-?[0-9]{1,3}(\\.[0-9]{1,2})?|[=\\[\\]{}\\n ,.])*"
        ) {
            run(&text);
        }
    }
}
