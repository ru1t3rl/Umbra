//! Reading `[[network_adapters]]` (ADR 0030, ASM-227, ASM-290).

use std::collections::BTreeMap;
use std::path::PathBuf;

use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::NetEntityType;
use umbra_core::{ComponentId, ComponentKind, Radians, Seconds, TemplateId};

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::loader::Loader;
use crate::reader::Reader;
use crate::resolve::{LoadedTemplates, missing_template_notes};
use crate::scenario::{GhostSettings, NetworkAdapterSpec, PublishSettings};
use crate::scenario_read::{non_negative, positive, required};
use crate::template::is_adapter_name;
use crate::value::{LKind, LTable, LValue, Origin};

const ENTRY_KEYS: [&str; 6] = ["adapter", "name", "enabled", "params", "publish", "ghosts"];
const PUBLISH_KEYS: [&str; 6] = [
    "position_threshold_m",
    "orientation_threshold_deg",
    "heartbeat_s",
    "min_interval_s",
    "dead_reckoning",
    "control",
];
const GHOST_KEYS: [&str; 5] = [
    "timeout_s",
    "max",
    "max_inbound_per_tick",
    "default_template",
    "type_map",
];

/// The adapters of a scenario file and the warnings found while reading them.
pub(crate) struct NetworkRead {
    pub(crate) adapters: Vec<NetworkAdapterSpec>,
    pub(crate) warnings: Vec<Diagnostic>,
}

/// What the template lookups of one scenario need.
struct Templates<'a> {
    loaded: &'a LoadedTemplates,
    unread: &'a [PathBuf],
}

impl Loader<'_> {
    /// Reads `[[network_adapters]]`; an absent key is an empty list. `None` when any entry has an
    /// error (reported through `reader`).
    pub(crate) fn read_network_adapters(
        &self,
        reader: &mut Reader<'_>,
        table: &LTable,
        loaded: &LoadedTemplates,
        unread: &[PathBuf],
    ) -> Option<NetworkRead> {
        let Some(value) = table.get("network_adapters") else {
            return Some(NetworkRead {
                adapters: Vec::new(),
                warnings: Vec::new(),
            });
        };
        let items = reader.array("network_adapters", value)?;
        let templates = Templates { loaded, unread };
        let mut names: BTreeMap<String, Origin> = BTreeMap::new();
        let mut warnings = Vec::new();
        let entries: Vec<Option<NetworkAdapterSpec>> = items
            .iter()
            .map(|item| self.read_entry(reader, item, &templates, &mut names, &mut warnings))
            .collect();
        let adapters: Option<Vec<_>> = entries.into_iter().collect();
        Some(NetworkRead {
            adapters: adapters?,
            warnings,
        })
    }

    fn read_entry(
        &self,
        reader: &mut Reader<'_>,
        value: &LValue,
        templates: &Templates<'_>,
        names: &mut BTreeMap<String, Origin>,
        warnings: &mut Vec<Diagnostic>,
    ) -> Option<NetworkAdapterSpec> {
        let table = reader.table("network_adapters", value)?;
        reader.unknown_keys(table, &ENTRY_KEYS);
        let adapter = required(reader, table, "adapter").and_then(|v| {
            reader
                .id("adapter", v, ComponentId::new)
                .map(|id| (id, v.origin()))
        });
        let name = required(reader, table, "name").and_then(|v| read_name(reader, v, names));
        let enabled = match table.get("enabled") {
            Some(v) => reader.boolean("enabled", v),
            None => Some(true),
        };
        let params = self.read_adapter_params(reader, table, adapter.as_ref());
        let publish = table
            .get("publish")
            .map_or(Some(PublishSettings::default()), |v| {
                read_publish(reader, v)
            });
        let ghosts = match table.get("ghosts") {
            Some(v) => read_ghosts(
                reader,
                v,
                templates,
                publish_heartbeat(table).as_ref(),
                warnings,
            ),
            None => Some(GhostSettings::default()),
        };
        Some(NetworkAdapterSpec {
            adapter: adapter?.0,
            name: name?,
            enabled: enabled?,
            params: params?,
            publish: publish?,
            ghosts: ghosts?,
        })
    }

    /// `params` against the adapter's schema (and a trial build), like a template's motion
    /// parameters; absent is an empty table.
    fn read_adapter_params(
        &self,
        reader: &mut Reader<'_>,
        table: &LTable,
        adapter: Option<&(ComponentId, &Origin)>,
    ) -> Option<umbra_core::ParamValue> {
        let params = match table.get("params") {
            Some(v) => reader.table("params", v).cloned(),
            None => Some(LTable::from_entries(
                table.origin().clone(),
                BTreeMap::new(),
            )),
        };
        let (id, at) = adapter?;
        let mut out = Vec::new();
        let Some(params) = params else {
            // The parameters are unreadable, but an unknown adapter is still reported.
            self.check_registered(ComponentKind::NetworkAdapter, id, at, &mut out);
            reader.extend(out);
            return None;
        };
        let json = self.check_component(ComponentKind::NetworkAdapter, id, at, &params, &mut out);
        reader.extend(out);
        json
    }
}

/// `name`: `[a-z][a-z0-9_]*` (the grammar of the templates' `[network.<name>]` tables), unique.
fn read_name(
    reader: &mut Reader<'_>,
    value: &LValue,
    names: &mut BTreeMap<String, Origin>,
) -> Option<String> {
    let name = reader.string("name", value)?;
    if !is_adapter_name(name) {
        let message = format!("`name` is not valid: found `{name}` (use [a-z][a-z0-9_]*)");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return None;
    }
    if let Some(first) = names.get(name) {
        let message = format!("network adapter name `{name}` is used more than once");
        reader.push_diagnostic(
            Diagnostic::new(
                DiagnosticCode::DuplicateName,
                message,
                Some(value.origin().location()),
            )
            .with_note("first used here", Some(first.location())),
        );
        return None;
    }
    names.insert(name.to_string(), value.origin().clone());
    Some(name.to_string())
}

/// The `heartbeat_s` value of an entry's `[publish]`, if it is a readable number (read again
/// silently: its errors are reported by [`read_publish`]).
fn publish_heartbeat(entry: &LTable) -> Option<(f64, Origin)> {
    let LKind::Table(publish) = entry.get("publish")?.kind() else {
        return None;
    };
    let value = publish.get("heartbeat_s")?;
    let seconds = match value.kind() {
        LKind::Float(f) if f.is_finite() && *f > 0.0 => *f,
        #[expect(clippy::cast_precision_loss, reason = "TOML integers become f64")]
        LKind::Integer(i) if *i > 0 => *i as f64,
        _ => return None,
    };
    Some((seconds, value.origin().clone()))
}

fn read_publish(reader: &mut Reader<'_>, value: &LValue) -> Option<PublishSettings> {
    let table = reader.table("publish", value)?;
    reader.unknown_keys(table, &PUBLISH_KEYS);
    let default = PublishSettings::default();
    let position = number_or(
        reader,
        table,
        "position_threshold_m",
        default.position_threshold_m,
        positive,
    );
    let orientation = number_or(
        reader,
        table,
        "orientation_threshold_deg",
        default.orientation_threshold.0.to_degrees(),
        positive,
    );
    let heartbeat = number_or(reader, table, "heartbeat_s", default.heartbeat.0, positive);
    let min_interval = number_or(
        reader,
        table,
        "min_interval_s",
        default.min_interval.0,
        non_negative,
    );
    let model = match table.get("dead_reckoning") {
        Some(v) => read_model(reader, v),
        None => Some(default.model),
    };
    let control = match table.get("control") {
        Some(v) => reader.boolean("control", v),
        None => Some(default.control),
    };
    Some(PublishSettings {
        position_threshold_m: position?,
        orientation_threshold: Radians::from_degrees(orientation?),
        heartbeat: Seconds(heartbeat?),
        min_interval: Seconds(min_interval?),
        model: model?,
        control: control?,
    })
}

/// A number checked by `check` when the key is present, `default` otherwise.
fn number_or(
    reader: &mut Reader<'_>,
    table: &LTable,
    key: &str,
    default: f64,
    check: fn(&mut Reader<'_>, &str, &LValue) -> Option<f64>,
) -> Option<f64> {
    match table.get(key) {
        Some(v) => check(reader, key, v),
        None => Some(default),
    }
}

/// The models the tracker publishes (ASM-231); rotating and body-frame models are inbound only.
fn read_model(reader: &mut Reader<'_>, value: &LValue) -> Option<DeadReckoning> {
    let name = reader.string("dead_reckoning", value)?;
    let model = match name {
        "static" => DeadReckoning::Static,
        "velocity" => DeadReckoning::Velocity,
        "acceleration" => DeadReckoning::Acceleration,
        other => {
            let message = format!(
                "`dead_reckoning` must be \"static\", \"velocity\" or \"acceleration\", found \"{other}\""
            );
            reader.error(DiagnosticCode::InvalidValue, message, value.origin());
            return None;
        }
    };
    Some(model)
}

fn read_ghosts(
    reader: &mut Reader<'_>,
    value: &LValue,
    templates: &Templates<'_>,
    heartbeat: Option<&(f64, Origin)>,
    warnings: &mut Vec<Diagnostic>,
) -> Option<GhostSettings> {
    let table = reader.table("ghosts", value)?;
    reader.unknown_keys(table, &GHOST_KEYS);
    let default = GhostSettings::default();
    let timeout = table
        .get("timeout_s")
        .map(|v| (positive(reader, "timeout_s", v), v));
    let max = optional_count(reader, table, "max", default.max);
    let inbound = optional_count(
        reader,
        table,
        "max_inbound_per_tick",
        default.max_inbound_per_tick,
    );
    let default_template = match table.get("default_template") {
        Some(v) => template_id(reader, "default_template", v, templates).map(Some),
        None => Some(None),
    };
    let type_map = match table.get("type_map") {
        Some(v) => read_type_map(reader, v, templates),
        None => Some(Vec::new()),
    };
    if let Some((Some(seconds), v)) = &timeout {
        warn_short_timeout(warnings, *seconds, heartbeat, v.origin());
    }
    let timeout = match timeout {
        Some((seconds, _)) => seconds.map(Seconds),
        None => Some(default.timeout),
    };
    Some(GhostSettings {
        timeout: timeout?,
        max: max?,
        max_inbound_per_tick: inbound?,
        default_template: default_template?,
        type_map: type_map?,
    })
}

/// A ghost that times out before the next heartbeat would expire between two publications of a
/// peer using the same settings. Only a warning: peers choose their own heartbeat (ASM-290).
fn warn_short_timeout(
    warnings: &mut Vec<Diagnostic>,
    timeout: f64,
    heartbeat: Option<&(f64, Origin)>,
    at: &Origin,
) {
    let (beat, source) = match heartbeat {
        Some((h, _)) => (*h, "`heartbeat_s`"),
        None => (
            PublishSettings::default().heartbeat.0,
            "the default `heartbeat_s`",
        ),
    };
    if timeout <= beat {
        let message = format!(
            "`timeout_s` of {timeout} is not above {source} of {beat}: ghosts may expire between heartbeats"
        );
        let mut warning = Diagnostic::warning(
            DiagnosticCode::SuspiciousValue,
            message,
            Some(at.location()),
        );
        if let Some((_, written)) = heartbeat {
            warning = warning.with_note("`heartbeat_s` is set here", Some(written.location()));
        }
        warnings.push(warning);
    }
}

/// An integer `>= 1` that fits in `usize`, or `default` when the key is absent.
fn optional_count(
    reader: &mut Reader<'_>,
    table: &LTable,
    key: &str,
    default: usize,
) -> Option<usize> {
    let Some(value) = table.get(key) else {
        return Some(default);
    };
    let n = reader.integer(key, value)?;
    let count = usize::try_from(n).ok().filter(|&c| c >= 1);
    if count.is_none() {
        let message = format!("`{key}` must be at least 1, found {n}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    count
}

/// Token to template overrides, ascending by token (the table iterates in key order).
fn read_type_map(
    reader: &mut Reader<'_>,
    value: &LValue,
    templates: &Templates<'_>,
) -> Option<Vec<(NetEntityType, TemplateId)>> {
    let table = reader.table("type_map", value)?;
    let pairs: Vec<Option<(NetEntityType, TemplateId)>> = table
        .iter()
        .map(|(token, entry)| {
            if token.is_empty() {
                let message = "`type_map` has an empty entity type token".to_string();
                reader.error(DiagnosticCode::InvalidValue, message, entry.key_origin());
                return None;
            }
            let name = format!("type_map.\"{token}\"");
            let template = template_id(reader, &name, entry.value(), templates)?;
            Some((NetEntityType::new(token), template))
        })
        .collect();
    pairs.into_iter().collect()
}

/// A template id that the load defines; an unknown one is `unknown_template` at the value, with
/// the same notes as an entity's `template`.
fn template_id(
    reader: &mut Reader<'_>,
    name: &str,
    value: &LValue,
    templates: &Templates<'_>,
) -> Option<TemplateId> {
    let id = reader.id(name, value, TemplateId::new)?;
    if templates.loaded.set.get(&id).is_some() {
        return Some(id);
    }
    if !templates.loaded.unusable.contains(&id) {
        let message = format!("template `{id}` is not defined");
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::UnknownTemplate,
            message,
            Some(value.origin().location()),
        );
        for note in missing_template_notes(&templates.loaded.unreadable, templates.unread) {
            diagnostic = diagnostic.with_note(note, None);
        }
        reader.push_diagnostic(diagnostic);
    }
    None
}
