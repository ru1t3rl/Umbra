//! AT-15 (M7 WI-2 and WI-7, host descriptor rows): the capability kinds of ABI 0.2 at the descriptor
//! level (REQ-P-2, REQ-P-4, REQ-P-9, ADR 0019, ADR 0033, ADR 0050). Descriptors forged in test code and
//! loaded with `PluginHost::load_static`.
//!
//! Kinds 2 (motion model), 4 (sensor), 6 (condition) and 7 (action) need a descriptor declaring minor 2
//! or newer; a descriptor declaring minor 0 or 1 that names one is rejected naming the kind
//! (`Descriptor { field: "capabilities[i].kind" }` as for kind 3; the WI-2 rows also accept the
//! `Unsupported` answer of an ABI 0.1 host), and kind 5 (`component`, no defined semantics, ASM-620) is
//! rejected at every minor. At minor 2 the host validates the schema, the metadata (empty for motion
//! models and sensors, `blackboard_params` for conditions and actions, ASM-643) and the kind's vtable of
//! every capability, and only then reports the first capability of a kind it has no bridge for as
//! `Unsupported`, naming its index and kind (WI-7, ASM-708). Motion models and sensors are bridged
//! since WI-8, conditions and actions since WI-9, so all four load; since WI-9 only a reserved or
//! unknown kind reaches `Unsupported` (ASM-724).
#![expect(unsafe_code, reason = "forges raw ABI structs")]
#![expect(
    clippy::cast_possible_truncation,
    reason = "ABI struct sizes are small"
)]

#[macro_use]
mod support;

use std::ffi::c_void;
use std::mem::size_of;

use support::forge::{Parts, capability, leak_str, parts};
use umbra_core::ComponentKind;
use umbra_plugin_api::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_CAPABILITY_ACTION,
    UMBRA_CAPABILITY_COMPONENT, UMBRA_CAPABILITY_CONDITION, UMBRA_CAPABILITY_MOTION_MODEL,
    UMBRA_CAPABILITY_SENSOR, UMBRA_ERR_UNSUPPORTED, UmbraAction, UmbraActionInstance,
    UmbraActionVTable, UmbraCapability, UmbraCapabilityKind, UmbraCode, UmbraCondition,
    UmbraConditionVTable, UmbraContactSink, UmbraErrorSink, UmbraIntentSink, UmbraKinematicState,
    UmbraLeafCall, UmbraMotionModel, UmbraMotionStep, UmbraMotionVTable, UmbraPluginDescriptor,
    UmbraSenseCall, UmbraSensor, UmbraSensorVTable, UmbraStatus, UmbraStr,
};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost, PluginInfo};

const ORIGIN: &str = "forged-kinds";

/// The kinds ABI 0.2 adds, with their names.
const NEW_KINDS: [(&str, UmbraCapabilityKind); 4] = [
    ("motion model", UMBRA_CAPABILITY_MOTION_MODEL),
    ("sensor", UMBRA_CAPABILITY_SENSOR),
    ("condition", UMBRA_CAPABILITY_CONDITION),
    ("action", UMBRA_CAPABILITY_ACTION),
];

/// `major << 16 | minor` of this host's major.
const fn abi(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
}

/// A one-capability plugin whose capability has `kind` (behind the behavior vtable, which no check
/// before the kind rule reads), the descriptor and the capability declaring `minor`.
fn kind_parts(kind: UmbraCapabilityKind, minor: u32) -> Parts {
    let mut p = parts();
    p.desc.abi_version = abi(minor);
    p.caps[0].abi_version = abi(minor);
    p.caps[0].kind = kind;
    p
}

macro_rules! load {
    ($p:expr) => {{
        let f = forged!($p);
        let mut host = PluginHost::new(HostOptions::default());
        host.load_static(ORIGIN, f.entry).cloned()
    }};
}

/// Whether `err` rejects capability kind `kind`: the reserved-kind answer of ABI 0.0/0.1 hosts
/// (`Unsupported`) or the kind-rule answer (`Descriptor` at `capabilities[0].kind`).
fn rejects_the_kind(err: &PluginError, kind: UmbraCapabilityKind) -> bool {
    match err {
        PluginError::Unsupported { kind: k, .. } => *k == kind,
        PluginError::Descriptor { field, .. } => field == "capabilities[0].kind",
        _ => false,
    }
}

/// Asserts a `Descriptor` error naming `field` and the origin (`label` names the row).
fn assert_descriptor_field(err: &PluginError, field: &str, label: &str) {
    assert!(
        matches!(err, PluginError::Descriptor { field: f, .. } if f == field),
        "{label}: expected a descriptor error at {field}, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains(ORIGIN) && msg.contains(field),
        "{label}: {msg}"
    );
}

/// Asserts the answer for a fully valid capability of a kind without a host bridge: `Unsupported`
/// naming the capability's index (`capabilities[<index>].kind`), id and kind, also in the message.
fn assert_unsupported(err: &PluginError, index: usize, id: &str, kind: UmbraCapabilityKind) {
    let field = format!("capabilities[{index}].kind");
    assert!(
        matches!(
            err,
            PluginError::Unsupported { field: f, capability, kind: k, .. }
                if *f == field && capability == id && *k == kind
        ),
        "expected Unsupported at {field} for {id} (kind {kind}), got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains(ORIGIN)
            && msg.contains(&field)
            && msg.contains(id)
            && msg.contains(&format!("kind {kind}")),
        "{msg}"
    );
}

// ---------------------------------------------------------------- forged vtables of the new kinds

unsafe extern "C" fn motion_configure(
    _: UmbraStr,
    _: *mut *mut UmbraMotionModel,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn motion_destroy(_: *mut UmbraMotionModel) {}
unsafe extern "C" fn motion_step(
    _: *mut UmbraMotionModel,
    _: *const UmbraMotionStep,
    _: *mut UmbraKinematicState,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn sensor_configure(
    _: UmbraStr,
    _: *mut *mut UmbraSensor,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn sensor_destroy(_: *mut UmbraSensor) {}
unsafe extern "C" fn sensor_sense(
    _: *mut UmbraSensor,
    _: *const UmbraSenseCall,
    _: *const UmbraContactSink,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn condition_configure(
    _: UmbraStr,
    _: *mut *mut UmbraCondition,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn condition_destroy(_: *mut UmbraCondition) {}
unsafe extern "C" fn condition_check(
    _: *mut UmbraCondition,
    _: *const UmbraLeafCall,
    _: *mut u8,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn action_configure(
    _: UmbraStr,
    _: *mut *mut UmbraAction,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn action_destroy(_: *mut UmbraAction) {}
unsafe extern "C" fn action_instantiate(
    _: *mut UmbraAction,
    _: u32,
    _: *mut *mut UmbraActionInstance,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn action_destroy_instance(_: *mut UmbraActionInstance) {}
unsafe extern "C" fn action_tick(
    _: *mut UmbraActionInstance,
    _: *const UmbraLeafCall,
    _: *const UmbraIntentSink,
    _: *mut UmbraStatus,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}
unsafe extern "C" fn action_halt(_: *mut UmbraActionInstance) -> UmbraCode {
    UMBRA_ERR_UNSUPPORTED
}

/// A param schema with the one string param `key` that a leaf's blackboard param can name.
const KEY_SCHEMA: &str = r#"{"type":"object","properties":{"key":{"type":"string"}}}"#;
/// Leaf metadata declaring `key` as a readable `bool` key.
const KEY_METADATA: &str =
    r#"{"blackboard_params":[{"param":"key","type":"bool","access":"read"}],"result":"r"}"#;

/// A required vtable entry's name and an edit that nulls it.
type NullEntry<V> = (&'static str, fn(&mut V));

/// One vtable type of the ABI 0.2 kinds, with a complete valid value (every entry set, header of
/// minor 2) and its required entries.
trait NewKind: Sized + 'static {
    /// The capability kind.
    const KIND: UmbraCapabilityKind;
    /// The kind's name in row labels.
    const NAME: &'static str;
    /// The capability id of the forged capability.
    const ID: &'static str;
    /// A metadata text the kind accepts (empty for motion models and sensors).
    const METADATA: &'static str;
    /// A schema the metadata agrees with.
    const SCHEMA: &'static str;
    /// The complete vtable of minor 2.
    fn valid() -> Self;
    /// `abi_version` and `struct_size`.
    fn header(&mut self) -> (&mut u32, &mut u32);
    /// Every required entry with an edit that nulls it.
    fn required() -> Vec<NullEntry<Self>>;
}

impl NewKind for UmbraMotionVTable {
    const KIND: UmbraCapabilityKind = UMBRA_CAPABILITY_MOTION_MODEL;
    const NAME: &'static str = "motion model";
    const ID: &'static str = "test.motion.forge";
    const METADATA: &'static str = "";
    const SCHEMA: &'static str = r#"{"type":"object"}"#;
    fn valid() -> Self {
        Self {
            abi_version: abi(2),
            struct_size: size_of::<Self>() as u32,
            configure: Some(motion_configure),
            destroy: Some(motion_destroy),
            step: Some(motion_step),
        }
    }
    fn header(&mut self) -> (&mut u32, &mut u32) {
        (&mut self.abi_version, &mut self.struct_size)
    }
    fn required() -> Vec<NullEntry<Self>> {
        vec![
            ("configure", |v| v.configure = None),
            ("destroy", |v| v.destroy = None),
            ("step", |v| v.step = None),
        ]
    }
}

impl NewKind for UmbraSensorVTable {
    const KIND: UmbraCapabilityKind = UMBRA_CAPABILITY_SENSOR;
    const NAME: &'static str = "sensor";
    const ID: &'static str = "test.sensor.forge";
    const METADATA: &'static str = "";
    const SCHEMA: &'static str = r#"{"type":"object"}"#;
    fn valid() -> Self {
        Self {
            abi_version: abi(2),
            struct_size: size_of::<Self>() as u32,
            configure: Some(sensor_configure),
            destroy: Some(sensor_destroy),
            sense: Some(sensor_sense),
        }
    }
    fn header(&mut self) -> (&mut u32, &mut u32) {
        (&mut self.abi_version, &mut self.struct_size)
    }
    fn required() -> Vec<NullEntry<Self>> {
        vec![
            ("configure", |v| v.configure = None),
            ("destroy", |v| v.destroy = None),
            ("sense", |v| v.sense = None),
        ]
    }
}

impl NewKind for UmbraConditionVTable {
    const KIND: UmbraCapabilityKind = UMBRA_CAPABILITY_CONDITION;
    const NAME: &'static str = "condition";
    const ID: &'static str = "test.cond.forge";
    const METADATA: &'static str = KEY_METADATA;
    const SCHEMA: &'static str = KEY_SCHEMA;
    fn valid() -> Self {
        Self {
            abi_version: abi(2),
            struct_size: size_of::<Self>() as u32,
            configure: Some(condition_configure),
            destroy: Some(condition_destroy),
            check: Some(condition_check),
        }
    }
    fn header(&mut self) -> (&mut u32, &mut u32) {
        (&mut self.abi_version, &mut self.struct_size)
    }
    fn required() -> Vec<NullEntry<Self>> {
        vec![
            ("configure", |v| v.configure = None),
            ("destroy", |v| v.destroy = None),
            ("check", |v| v.check = None),
        ]
    }
}

impl NewKind for UmbraActionVTable {
    const KIND: UmbraCapabilityKind = UMBRA_CAPABILITY_ACTION;
    const NAME: &'static str = "action";
    const ID: &'static str = "test.action.forge";
    const METADATA: &'static str = KEY_METADATA;
    const SCHEMA: &'static str = KEY_SCHEMA;
    fn valid() -> Self {
        Self {
            abi_version: abi(2),
            struct_size: size_of::<Self>() as u32,
            configure: Some(action_configure),
            destroy_action: Some(action_destroy),
            instantiate: Some(action_instantiate),
            destroy_instance: Some(action_destroy_instance),
            tick: Some(action_tick),
            halt: Some(action_halt),
        }
    }
    fn header(&mut self) -> (&mut u32, &mut u32) {
        (&mut self.abi_version, &mut self.struct_size)
    }
    fn required() -> Vec<NullEntry<Self>> {
        vec![
            ("configure", |v| v.configure = None),
            ("destroy_action", |v| v.destroy_action = None),
            ("instantiate", |v| v.instantiate = None),
            ("destroy_instance", |v| v.destroy_instance = None),
            ("tick", |v| v.tick = None),
            ("halt", |v| v.halt = None),
        ]
    }
}

/// A plugin with `index` behaviors followed by a `V` capability with `metadata`, `schema` and the vtable
/// `V::valid()` after `edit`; the descriptor and every header declare minor 2.
fn new_kind_at<V: NewKind>(
    index: usize,
    metadata: &str,
    schema: &str,
    edit: impl FnOnce(&mut V),
) -> Parts {
    let mut p = parts();
    p.desc.abi_version = abi(2);
    p.vtable.abi_version = abi(2);
    p.caps.clear();
    for i in 0..index {
        p.add_capability(&format!("test.behavior.forge{i}"));
    }
    p.add_capability(V::ID);
    for cap in &mut p.caps {
        cap.abi_version = abi(2);
    }
    let cap = &mut p.caps[index];
    cap.kind = V::KIND;
    cap.metadata = leak_str(metadata);
    cap.param_schema = leak_str(schema);
    let mut vt = V::valid();
    edit(&mut vt);
    let vt: &'static V = Box::leak(Box::new(vt));
    cap.vtable = std::ptr::from_ref(vt).cast::<c_void>();
    p
}

/// [`new_kind_at`] at index 0 with the kind's own metadata and schema.
fn new_kind<V: NewKind>(edit: impl FnOnce(&mut V)) -> Parts {
    new_kind_at::<V>(0, V::METADATA, V::SCHEMA, edit)
}

/// Appends a behavior capability `id` of minor 2 behind the forged behavior vtable (or a null vtable)
/// without re-linking the other capabilities' vtables.
fn push_behavior<'a>(p: &'a mut Parts, id: &str, vtable: bool) -> &'a mut UmbraCapability {
    let mut cap = capability(id);
    cap.abi_version = abi(2);
    if vtable {
        cap.vtable = (&raw const *p.vtable).cast::<c_void>();
    }
    p.caps.push(cap);
    p.desc.capabilities = p.caps.as_ptr();
    p.desc.capability_count = p.caps.len();
    p.caps
        .last_mut()
        .unwrap_or_else(|| unreachable!("just pushed"))
}

/// One forged plugin; the label carries what the row expects, separated by `|`.
type Row = (String, Parts);

/// The `n`th `|`-separated part of a row label.
fn part(label: &str, n: usize) -> &str {
    label
        .split('|')
        .nth(n)
        .unwrap_or_else(|| panic!("{label}: no part {n}"))
}

/// A row expecting a load with the valid `V` capability at `index`: label `name|index`.
fn valid_row<V: NewKind>(index: usize, note: &str, p: Parts) -> Row {
    (format!("{} {note}|{index}", V::NAME), p)
}

/// Asserts the load a row of [`valid_row`] expects: the capability at its index has the row kind's id
/// and kind (WI-9: every new kind is bridged).
fn assert_loaded_row(label: &str, loaded: Result<PluginInfo, PluginError>) {
    let info = loaded.unwrap_or_else(|e| panic!("{label}: {e}"));
    let index: usize = part(label, 1)
        .parse()
        .unwrap_or_else(|e| panic!("{label}: {e}"));
    let (id, kind) = if label.starts_with("motion") {
        ("test.motion.forge", ComponentKind::MotionModel)
    } else if label.starts_with("sensor") {
        ("test.sensor.forge", ComponentKind::Sensor)
    } else if label.starts_with("condition") {
        ("test.cond.forge", ComponentKind::Condition)
    } else {
        ("test.action.forge", ComponentKind::Action)
    };
    let cap = &info.capabilities[index];
    assert_eq!((cap.id.as_str(), cap.kind), (id, kind), "{label}");
}

/// `$rows::<V>()` for every new vtable type, concatenated (a macro: the generic functions run once per
/// kind, and the test that loads the rows is the only `forged!` site).
macro_rules! each_kind {
    ($rows:ident) => {{
        let mut all: Vec<Row> = Vec::new();
        all.extend($rows::<UmbraMotionVTable>());
        all.extend($rows::<UmbraSensorVTable>());
        all.extend($rows::<UmbraConditionVTable>());
        all.extend($rows::<UmbraActionVTable>());
        all
    }};
}

// ---------------------------------------------------------------- the kind rule (WI-2)

#[test]
fn a_new_kind_in_a_minor_1_descriptor_is_rejected_naming_the_kind() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 1)).expect_err(name);

        assert!(rejects_the_kind(&err, kind), "{name}: {err:?}");
        assert!(err.to_string().contains(ORIGIN), "{name}: {err}");
    }
}

#[test]
fn a_new_kind_in_a_minor_0_descriptor_is_rejected_naming_the_kind() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 0)).expect_err(name);

        assert!(rejects_the_kind(&err, kind), "{name}: {err:?}");
    }
}

/// WI-7: the kind rule of an ABI 0.2 host names the field and the minor the kind needs.
#[test]
fn a_new_kind_in_a_minor_1_descriptor_is_a_descriptor_error_at_the_kind() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 1)).expect_err(name);

        assert_descriptor_field(&err, "capabilities[0].kind", name);
        assert!(err.to_string().contains("minor 2"), "{name}: {err}");
    }
}

#[test]
fn the_component_kind_is_rejected_in_a_minor_1_descriptor() {
    let err = load!(kind_parts(UMBRA_CAPABILITY_COMPONENT, 1)).expect_err("kind 5 is reserved");

    assert!(
        rejects_the_kind(&err, UMBRA_CAPABILITY_COMPONENT),
        "{err:?}"
    );
}

#[test]
fn the_component_kind_is_rejected_in_a_minor_2_descriptor() {
    let err = load!(kind_parts(UMBRA_CAPABILITY_COMPONENT, 2)).expect_err("kind 5 stays reserved");

    assert!(
        rejects_the_kind(&err, UMBRA_CAPABILITY_COMPONENT),
        "{err:?}"
    );
    assert_unsupported(&err, 0, "test.behavior.forge", UMBRA_CAPABILITY_COMPONENT);
}

/// ADR 0050: at minor 2 the header and kind rules pass. With the behavior vtable (80 bytes) behind the
/// capability, the kind's own vtable check rejects it: every new vtable is smaller than 80 bytes at
/// minor 2, so the error is the vtable's `struct_size`, never `Unsupported` (Minor 5 of the WI-2
/// review).
#[test]
fn a_new_kind_in_a_minor_2_descriptor_passes_the_header_and_the_kind_checks() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 2)).expect_err(name);

        assert_descriptor_field(&err, "capabilities[0].vtable.struct_size", name);
    }
}

#[test]
fn a_minor_2_descriptor_with_a_behavior_loads() {
    let mut p = parts();
    p.desc.abi_version = abi(2);
    p.caps[0].abi_version = abi(2);
    p.vtable.abi_version = abi(2);
    p.link();

    let info = load!(p).expect("an ABI 0.2 host accepts a minor 2 descriptor");

    assert_eq!(info.abi, (0, 2));
}

// ---------------------------------------------------------------- Unsupported comes last (WI-7)

fn valid_rows<V: NewKind>() -> Vec<Row> {
    vec![
        valid_row::<V>(0, "first", new_kind::<V>(|_| {})),
        valid_row::<V>(
            2,
            "after two behaviors",
            new_kind_at::<V>(2, V::METADATA, V::SCHEMA, |_| {}),
        ),
    ]
}

/// A fully valid capability of each new kind passes every descriptor check and loads as a capability
/// of its kind (motion models and sensors bridged since WI-8, conditions and actions since WI-9).
#[test]
fn a_valid_capability_of_each_new_kind_loads_as_its_kind() {
    let rows = each_kind!(valid_rows);
    assert_eq!(rows.len(), 8);
    for (label, p) in rows {
        let loaded = load!(p);

        assert_loaded_row(&label, loaded);
    }
}

/// With two capabilities of kinds without a bridge (since WI-9: a reserved and an unknown kind), the
/// first one is named.
#[test]
fn the_first_capability_without_a_bridge_is_named() {
    let mut p = reserved_first(UMBRA_CAPABILITY_COMPONENT);
    push_behavior(&mut p, "test.unknown.later", true).kind = 99;

    let err = load!(p).expect_err("two unbridged capabilities");

    assert_unsupported(&err, 0, "test.behavior.forge", UMBRA_CAPABILITY_COMPONENT);
}

/// WI-8, WI-9: a bridged capability of each new kind before a capability without a bridge (since
/// WI-9: a reserved kind) does not hide it.
#[test]
fn a_bridged_capability_first_does_not_hide_a_later_unbridged_one() {
    for first in ["motion", "sensor", "condition", "action"] {
        let mut p = match first {
            "motion" => new_kind::<UmbraMotionVTable>(|_| {}),
            "sensor" => new_kind::<UmbraSensorVTable>(|_| {}),
            "condition" => new_kind::<UmbraConditionVTable>(|_| {}),
            _ => new_kind::<UmbraActionVTable>(|_| {}),
        };
        push_behavior(&mut p, "test.component.later", true).kind = UMBRA_CAPABILITY_COMPONENT;

        let err = load!(p).expect_err(first);

        assert_unsupported(&err, 1, "test.component.later", UMBRA_CAPABILITY_COMPONENT);
    }
}

/// A plugin whose capability 0 has the reserved or unknown `kind` (behind the behavior vtable), the
/// descriptor and the capability declaring minor 2.
fn reserved_first(kind: UmbraCapabilityKind) -> Parts {
    let mut p = kind_parts(kind, 2);
    p.vtable.abi_version = abi(2);
    p
}

/// ASM-708 (review Minor 1): a reserved kind (5) is deferred like the kinds without a bridge, so an
/// earlier capability without a bridge is the one named (an unknown kind since WI-9 bridged every new
/// kind).
#[test]
fn a_reserved_kind_after_an_unbridged_capability_does_not_hide_it() {
    let mut p = reserved_first(99);
    push_behavior(&mut p, "test.component.later", true).kind = UMBRA_CAPABILITY_COMPONENT;

    let err = load!(p).expect_err("unknown kind, then component");

    assert_unsupported(&err, 0, "test.behavior.forge", 99);
}

/// ASM-708 (review Minor 1): a reserved (5) or unknown (99) kind first does not mask a broken later
/// capability: the later problem is reported as itself.
#[test]
fn a_reserved_or_unknown_kind_first_does_not_mask_a_broken_later_capability() {
    for kind in [UMBRA_CAPABILITY_COMPONENT, 99] {
        let mut p = reserved_first(kind);
        push_behavior(&mut p, "test.behavior.later", true).param_schema = leak_str("{");

        let err = load!(p).expect_err("reserved kind, then a broken schema");

        assert_descriptor_field(
            &err,
            "capabilities[1].param_schema",
            &format!("kind {kind}"),
        );
    }
}

/// ASM-708: a reserved or unknown kind alone is still `Unsupported` at its index, after its id,
/// version and schema were checked (its metadata and vtable are not read: their layout is undefined).
#[test]
fn a_reserved_or_unknown_kind_is_checked_up_to_its_schema() {
    for kind in [UMBRA_CAPABILITY_COMPONENT, 99] {
        let mut ok = reserved_first(kind);
        ok.caps[0].vtable = std::ptr::null();
        ok.caps[0].metadata = leak_str("not json");
        let err = load!(ok).expect_err("reserved kind");
        assert_unsupported(&err, 0, "test.behavior.forge", kind);

        let mut bad_schema = reserved_first(kind);
        bad_schema.caps[0].param_schema = leak_str("[1]");
        let err = load!(bad_schema).expect_err("reserved kind with a bad schema");
        assert_descriptor_field(
            &err,
            "capabilities[0].param_schema",
            &format!("kind {kind}"),
        );

        let mut bad_version = reserved_first(kind);
        bad_version.caps[0].version = leak_str("one");
        let err = load!(bad_version).expect_err("reserved kind with a bad version");
        assert_descriptor_field(&err, "capabilities[0].version", &format!("kind {kind}"));
    }
}

/// Review nit 3: a valid unbridged capability (an action, since WI-8 bridged motion) first does not
/// mask a later condition's bad metadata.
#[test]
fn an_unbridged_capability_first_does_not_mask_a_later_bad_leaf_metadata() {
    let mut p = new_kind::<UmbraActionVTable>(|_| {});
    let mut condition = new_kind_at::<UmbraConditionVTable>(
        0,
        r#"{"blackboard_params":[{"param":"key","type":"bool","access":"read_write"}]}"#,
        KEY_SCHEMA,
        |_| {},
    );
    p.caps.push(condition.caps.remove(0));
    p.desc.capabilities = p.caps.as_ptr();
    p.desc.capability_count = p.caps.len();

    let err = load!(p).expect_err("motion, then a condition with bad metadata");

    assert_descriptor_field(
        &err,
        "capabilities[1].metadata.blackboard_params[0].access",
        "condition",
    );
}

fn later_problem_rows<V: NewKind>() -> Vec<Row> {
    let mut bad_schema = new_kind::<V>(|_| {});
    push_behavior(&mut bad_schema, "test.behavior.later", true).param_schema = leak_str("{");
    let mut duplicate = new_kind::<V>(|_| {});
    push_behavior(&mut duplicate, V::ID, true);
    let mut null_vtable = new_kind::<V>(|_| {});
    push_behavior(&mut null_vtable, "test.behavior.later", false);
    vec![
        (
            format!("{}|capabilities[1].param_schema", V::NAME),
            bad_schema,
        ),
        (format!("{}|capabilities[1].id", V::NAME), duplicate),
        (format!("{}|capabilities[1].vtable", V::NAME), null_vtable),
    ]
}

/// Minor 5 (WI-2 review): `Unsupported` is reported only after every capability passed every check,
/// so a problem in a later capability (a bad schema, a duplicate id, a null vtable) is reported as that
/// problem, not as the earlier capability's missing bridge.
#[test]
fn unsupported_is_reported_only_after_every_capability_passed_its_checks() {
    let rows = each_kind!(later_problem_rows);
    assert_eq!(rows.len(), 12);
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, part(&label, 1), &label);
    }
}

// ---------------------------------------------------------------- vtables (WI-2 review, Major 2)

fn null_entry_rows<V: NewKind>() -> Vec<Row> {
    V::required()
        .into_iter()
        .map(|(entry, edit)| {
            (
                format!("{}|capabilities[0].vtable.{entry}", V::NAME),
                new_kind::<V>(edit),
            )
        })
        .collect()
}

#[test]
fn every_required_entry_of_each_new_vtable_is_checked() {
    let rows = each_kind!(null_entry_rows);
    assert_eq!(rows.len(), 3 + 3 + 3 + 6, "every required entry has a row");
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, part(&label, 1), &label);
    }
}

fn size_rows<V: NewKind>() -> Vec<Row> {
    let own = size_of::<V>() as u32;
    [8, own - 8, own - 4, own + 4, own + 8]
        .into_iter()
        .map(|size| {
            (
                format!("{} struct_size {size}", V::NAME),
                new_kind::<V>(move |v| *v.header().1 = size),
            )
        })
        .collect()
}

/// ADR 0019: a new vtable's `struct_size` is at least its size at minor 2 (the minor it exists since),
/// at most the host's size for the declared minor, and a multiple of 8.
#[test]
fn a_new_vtable_below_or_above_its_minor_2_size_is_rejected() {
    let rows = each_kind!(size_rows);
    assert_eq!(rows.len(), 20);
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, "capabilities[0].vtable.struct_size", &label);
    }
}

fn minor_1_header_rows<V: NewKind>() -> Vec<Row> {
    vec![(
        V::NAME.to_owned(),
        new_kind::<V>(|v| *v.header().0 = abi(1)),
    )]
}

/// ADR 0033: a new vtable exists since minor 2, so a header declaring minor 1 is refused at its
/// `abi_version`.
#[test]
fn a_new_vtable_declaring_minor_1_is_rejected() {
    for (label, p) in each_kind!(minor_1_header_rows) {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, "capabilities[0].vtable.abi_version", &label);
        assert!(err.to_string().contains("since ABI 0.2"), "{label}: {err}");
    }
}

fn newer_header_rows<V: NewKind>() -> Vec<Row> {
    vec![(
        V::NAME.to_owned(),
        new_kind::<V>(|v| *v.header().0 = abi(UMBRA_ABI_VERSION_MINOR + 1)),
    )]
}

#[test]
fn a_new_vtable_of_a_newer_minor_is_an_abi_mismatch() {
    for (label, p) in each_kind!(newer_header_rows) {
        let err = load!(p).expect_err(&label);

        assert!(
            matches!(&err, PluginError::AbiMismatch { field, .. } if field == "capabilities[0].vtable.abi_version"),
            "{label}: {err:?}"
        );
    }
}

fn null_vtable_rows<V: NewKind>() -> Vec<Row> {
    let mut p = new_kind::<V>(|_| {});
    p.caps[0].vtable = std::ptr::null();
    vec![(V::NAME.to_owned(), p)]
}

#[test]
fn a_null_new_vtable_is_rejected() {
    for (label, p) in each_kind!(null_vtable_rows) {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, "capabilities[0].vtable", &label);
    }
}

// ---------------------------------------------------------------- metadata (WI-2 review, Major 2)

/// ADR 0050: motion models and sensors define no metadata; any text is refused.
#[test]
fn metadata_on_a_motion_model_or_sensor_is_rejected() {
    let rows: Vec<Row> = vec![
        (
            "motion model".into(),
            new_kind_at::<UmbraMotionVTable>(0, r#"{"result":"r"}"#, KEY_SCHEMA, |_| {}),
        ),
        (
            "sensor".into(),
            new_kind_at::<UmbraSensorVTable>(0, "{}", KEY_SCHEMA, |_| {}),
        ),
    ];
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, "capabilities[0].metadata", &label);
    }
}

/// Malformed leaf metadata, each with the field it must be reported at.
const BAD_LEAF_METADATA: [(&str, &str); 14] = [
    ("{", "capabilities[0].metadata"),
    ("[]", "capabilities[0].metadata"),
    (
        r#"{"param_types":{"key":"bool"}}"#,
        "capabilities[0].metadata",
    ),
    (r#"{"result":1}"#, "capabilities[0].metadata.result"),
    (
        r#"{"blackboard_params":{}}"#,
        "capabilities[0].metadata.blackboard_params",
    ),
    (
        r#"{"blackboard_params":["key"]}"#,
        "capabilities[0].metadata.blackboard_params[0]",
    ),
    (
        r#"{"blackboard_params":[{"type":"bool","access":"read"}]}"#,
        "capabilities[0].metadata.blackboard_params[0].param",
    ),
    (
        r#"{"blackboard_params":[{"param":"Key","type":"bool","access":"read"}]}"#,
        "capabilities[0].metadata.blackboard_params[0].param",
    ),
    (
        r#"{"blackboard_params":[{"param":"key","type":"float","access":"read"}]}"#,
        "capabilities[0].metadata.blackboard_params[0].type",
    ),
    (
        r#"{"blackboard_params":[{"param":"key","access":"read"}]}"#,
        "capabilities[0].metadata.blackboard_params[0].type",
    ),
    (
        r#"{"blackboard_params":[{"param":"key","type":"bool","access":"read_write"}]}"#,
        "capabilities[0].metadata.blackboard_params[0].access",
    ),
    (
        r#"{"blackboard_params":[{"param":"key","type":"bool"}]}"#,
        "capabilities[0].metadata.blackboard_params[0].access",
    ),
    (
        r#"{"blackboard_params":[{"param":"key","type":"bool","access":"read","x":1}]}"#,
        "capabilities[0].metadata.blackboard_params[0]",
    ),
    (
        r#"{"blackboard_params":[{"param":"key","type":"bool","access":"read"},{"param":"key","type":"number","access":"write"}]}"#,
        "capabilities[0].metadata.blackboard_params[1].param",
    ),
];

fn bad_metadata_rows<V: NewKind>() -> Vec<Row> {
    if V::METADATA.is_empty() {
        return Vec::new();
    }
    BAD_LEAF_METADATA
        .iter()
        .map(|(text, field)| {
            (
                format!("{}|{field}|{text}", V::NAME),
                new_kind_at::<V>(0, text, KEY_SCHEMA, |_| {}),
            )
        })
        .collect()
}

/// ASM-643, ASM-709: malformed `blackboard_params` (not an array, entries that are not objects, missing
/// or invalid names, unknown types or `access`, unknown keys, duplicates), unknown top-level keys and a
/// non-string `result` are descriptor errors at the offending field, for conditions and actions alike.
#[test]
fn malformed_leaf_metadata_is_rejected_at_its_field() {
    let rows = each_kind!(bad_metadata_rows);
    assert_eq!(rows.len(), 2 * BAD_LEAF_METADATA.len());
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, part(&label, 1), &label);
    }
}

/// Schemas that disagree with `KEY_METADATA`: the blackboard param `key` names a blackboard key, so it
/// must be a property of the param schema and, when the property is typed, a string (ASM-709).
const DISAGREEING_SCHEMAS: [&str; 4] = [
    r#"{"type":"object","properties":{"key":{"type":"integer"}}}"#,
    r#"{"type":"object","properties":{"key":{"type":["number","null"]}}}"#,
    r#"{"type":"object","properties":{"other":{"type":"string"}}}"#,
    r#"{"type":"object"}"#,
];

fn disagreeing_schema_rows<V: NewKind>() -> Vec<Row> {
    if V::METADATA.is_empty() {
        return Vec::new();
    }
    DISAGREEING_SCHEMAS
        .iter()
        .map(|schema| {
            (
                format!("{} {schema}", V::NAME),
                new_kind_at::<V>(0, KEY_METADATA, schema, |_| {}),
            )
        })
        .collect()
}

#[test]
fn a_blackboard_param_that_disagrees_with_the_schema_is_rejected() {
    let rows = each_kind!(disagreeing_schema_rows);
    assert_eq!(rows.len(), 2 * DISAGREEING_SCHEMAS.len());
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(
            &err,
            "capabilities[0].metadata.blackboard_params[0].param",
            &label,
        );
    }
}

/// Leaf metadata forms that are valid: they pass every check and load (WI-9).
const GOOD_LEAF_METADATA: [(&str, &str); 5] = [
    ("", KEY_SCHEMA),
    (r#"{"result":"only a result"}"#, KEY_SCHEMA),
    (r#"{"blackboard_params":[]}"#, r#"{"type":"object"}"#),
    (
        r#"{"blackboard_params":[{"param":"key","type":"entity","access":"write"}]}"#,
        r#"{"type":"object","properties":{"key":{"type":["string","null"]}}}"#,
    ),
    (
        r#"{"blackboard_params":[{"param":"key","type":"position","access":"read"}]}"#,
        r#"{"type":"object","properties":{"key":{"minLength":1}}}"#,
    ),
];

fn good_metadata_rows<V: NewKind>() -> Vec<Row> {
    if V::METADATA.is_empty() {
        return Vec::new();
    }
    GOOD_LEAF_METADATA
        .iter()
        .map(|(metadata, schema)| {
            valid_row::<V>(0, metadata, new_kind_at::<V>(0, metadata, schema, |_| {}))
        })
        .collect()
}

#[test]
fn valid_leaf_metadata_forms_pass_every_check() {
    let rows = each_kind!(good_metadata_rows);
    assert_eq!(rows.len(), 2 * GOOD_LEAF_METADATA.len());
    for (label, p) in rows {
        let loaded = load!(p);

        assert_loaded_row(&label, loaded);
    }
}

/// The new kinds' param schema is checked like every other kind's.
#[test]
fn a_new_kind_param_schema_is_still_checked() {
    let rows: Vec<Row> = vec![
        (
            "motion model".into(),
            new_kind_at::<UmbraMotionVTable>(0, "", "[1]", |_| {}),
        ),
        (
            "action".into(),
            new_kind_at::<UmbraActionVTable>(0, KEY_METADATA, "{", |_| {}),
        ),
    ];
    for (label, p) in rows {
        let err = load!(p).expect_err(&label);

        assert_descriptor_field(&err, "capabilities[0].param_schema", &label);
    }
}

// ---------------------------------------------------------------- sizes per minor (WI-2 review, Major 2)

/// ADR 0019 R-B3: at every minor this host knows, a descriptor and a capability of their own size load;
/// one word more than the host's size at that minor is refused at `struct_size`.
#[test]
fn descriptor_and_capability_sizes_are_checked_per_minor() {
    let desc = size_of::<UmbraPluginDescriptor>() as u32;
    let cap = size_of::<UmbraCapability>() as u32;
    for minor in 0..=UMBRA_ABI_VERSION_MINOR {
        let at = |p: &mut Parts| {
            p.desc.abi_version = abi(minor);
            p.caps[0].abi_version = abi(minor);
            p.vtable.abi_version = abi(minor);
        };
        let mut ok = parts();
        at(&mut ok);
        let info = load!(ok).unwrap_or_else(|e| panic!("minor {minor}: {e}"));
        assert_eq!(info.abi, (0, minor));

        let mut big_desc = parts();
        at(&mut big_desc);
        big_desc.desc.struct_size = desc + 8;
        let err = load!(big_desc).expect_err("descriptor too big");
        assert_descriptor_field(&err, "descriptor.struct_size", &format!("minor {minor}"));

        let mut big_cap = parts();
        at(&mut big_cap);
        big_cap.caps[0].struct_size = cap + 8;
        let err = load!(big_cap).expect_err("capability too big");
        assert_descriptor_field(
            &err,
            "capabilities[0].struct_size",
            &format!("minor {minor}"),
        );
    }
}

/// ADR 0033: the descriptor's minor is the ceiling of every nested header. A capability declaring
/// minor 2 inside a descriptor declaring minor 1 is refused at its `abi_version`; a condition whose
/// capability declares minor 1 is refused by the kind rule before its (minor 2) vtable is read.
#[test]
fn nested_headers_of_minor_2_need_a_minor_2_descriptor() {
    let mut cap = parts();
    cap.desc.abi_version = abi(1);
    cap.caps[0].abi_version = abi(2);
    let err = load!(cap).expect_err("capability newer than its descriptor");
    assert_descriptor_field(&err, "capabilities[0].abi_version", "capability");

    let mut vt = new_kind::<UmbraConditionVTable>(|_| {});
    vt.desc.abi_version = abi(1);
    vt.caps[0].abi_version = abi(1);
    let err = load!(vt).expect_err("a condition in a minor 1 descriptor");
    assert_descriptor_field(&err, "capabilities[0].kind", "condition");
}
