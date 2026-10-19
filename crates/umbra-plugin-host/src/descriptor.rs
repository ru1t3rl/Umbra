//! Copying and validating the plugin descriptor (REQ-P-2..4, REQ-P-9, ADR 0019 "Header", "Bounds",
//! "Values", ADR 0020 "Metadata", ASM-119).
//!
//! The host reads the descriptor exactly once per image lifecycle, right after the entry calls, and
//! copies everything it needs into owned values; it never re-reads plugin memory afterwards. Before
//! any (pointer, length) becomes a slice: pointer non-null when the length is non-zero, arrays of
//! structs 8-byte aligned, lengths within the caps (and so within `isize::MAX`), `checked_mul` for
//! `count * stride`. Every headed struct is checked for its major (equal to the host's; a newer
//! minor is an ABI mismatch too) and its `struct_size` (at least the minor-0 size, at most the
//! host's size for the declared minor, a multiple of 8) before anything past the header is read;
//! only `struct_size` bytes are read, newer fields are absent. Errors name the field as
//! `descriptor.<f>`, `capabilities[<i>].<f>` or `capabilities[<i>].vtable.<f>` (ASM-125 b3).

#![expect(
    unsafe_code,
    reason = "reads the plugin-written descriptor, capabilities and vtables (ADR 0019 bounds)"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::mem::{MaybeUninit, size_of};

use umbra_core::{BbType, ComponentId, ComponentKind, ParamSchema, PluginId, Version};
use umbra_plugin_api::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_CAPABILITY_BEHAVIOR,
    UMBRA_MAX_CAPABILITIES, UMBRA_MAX_ID_LEN, UMBRA_MAX_METADATA_LEN, UMBRA_MAX_SCHEMA_LEN,
    UmbraBehavior, UmbraBehaviorBatch, UmbraBehaviorVTable, UmbraCapability, UmbraCode,
    UmbraErrorSink, UmbraHostApi, UmbraInstance, UmbraIntent, UmbraPluginDescriptor, UmbraStr,
    UmbraValue, UmbraValueSink,
};

use crate::error::PluginError;

/// Cap on the plugin name and version texts (not ABI constants; ASM-135).
const MAX_TEXT_LEN: usize = 4096;
/// The reserved namespace of built-in components (ADR 0002).
const RESERVED_PREFIX: &str = "builtin.";
/// Alignment required of every array of headed structs and of every struct pointer (ADR 0019).
const STRUCT_ALIGN: usize = 8;

/// The size of each struct per ABI minor of this host, index = minor (append-only, ADR 0019 R-B3).
const DESCRIPTOR_SIZES: [usize; HOST_MINORS] = [size_of::<UmbraPluginDescriptor>()];
const CAPABILITY_SIZES: [usize; HOST_MINORS] = [size_of::<UmbraCapability>()];
const VTABLE_SIZES: [usize; HOST_MINORS] = [size_of::<UmbraBehaviorVTable>()];
/// Number of minors this host knows (`0..=UMBRA_ABI_VERSION_MINOR`).
const HOST_MINORS: usize = UMBRA_ABI_VERSION_MINOR as usize + 1;

/// The host's own `(major, minor)`.
pub(crate) const HOST_ABI: (u32, u32) = (UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR);

/// `init` of a plugin.
pub(crate) type InitFn =
    unsafe extern "C" fn(host: *const UmbraHostApi, err: *const UmbraErrorSink) -> UmbraCode;
/// `shutdown` of a plugin.
pub(crate) type ShutdownFn = unsafe extern "C" fn();

/// The copied, validated descriptor of one image lifecycle.
#[derive(Debug, Clone)]
pub(crate) struct Descriptor {
    /// Plugin id.
    pub(crate) id: PluginId,
    /// Display name.
    pub(crate) name: String,
    /// Plugin version.
    pub(crate) version: Version,
    /// The ABI the plugin requires (its declared `(major, minor)`).
    pub(crate) abi: (u32, u32),
    /// Optional `init`.
    pub(crate) init: Option<InitFn>,
    /// Optional `shutdown`.
    pub(crate) shutdown: Option<ShutdownFn>,
    /// Capabilities in descriptor order.
    pub(crate) capabilities: Vec<Capability>,
}

/// One copied, validated capability.
#[derive(Debug, Clone)]
pub(crate) struct Capability {
    /// Always `Behavior` in ABI v0 (other kinds are rejected as unsupported).
    pub(crate) kind: ComponentKind,
    /// Component id.
    pub(crate) id: ComponentId,
    /// Capability version (the plugin's when the capability leaves it empty).
    pub(crate) version: Version,
    /// The param schema (a JSON object; the ADR 0009 subset is checked by the template loader,
    /// ASM-114).
    pub(crate) schema: ParamSchema,
    /// The schema's top-level `description` (empty without one).
    pub(crate) description: String,
    /// The behavior metadata (ADR 0020).
    pub(crate) metadata: Metadata,
    /// The behavior vtable, copied.
    pub(crate) vtable: BehaviorVTable,
}

/// Behavior metadata (ADR 0020): types of interface params and outputs, result text.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Metadata {
    /// Interface params and their types.
    pub(crate) param_types: BTreeMap<String, BbType>,
    /// Output ports and their types.
    pub(crate) outputs: BTreeMap<String, BbType>,
    /// Free-text result semantics.
    pub(crate) result: Option<String>,
}

/// The behavior vtable entries this host knows, copied from the plugin (required ones are
/// non-null).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BehaviorVTable {
    pub(crate) configure: unsafe extern "C" fn(
        params: UmbraStr,
        out: *mut *mut UmbraBehavior,
        err: *const UmbraErrorSink,
    ) -> UmbraCode,
    pub(crate) destroy_behavior: unsafe extern "C" fn(behavior: *mut UmbraBehavior),
    pub(crate) instantiate: unsafe extern "C" fn(
        behavior: *mut UmbraBehavior,
        rate_hz: u32,
        out: *mut *mut UmbraInstance,
        err: *const UmbraErrorSink,
    ) -> UmbraCode,
    pub(crate) destroy_instance: unsafe extern "C" fn(instance: *mut UmbraInstance),
    pub(crate) tick_batch: unsafe extern "C" fn(
        batch: *const UmbraBehaviorBatch,
        err: *const UmbraErrorSink,
    ) -> UmbraCode,
    pub(crate) halt: unsafe extern "C" fn(instance: *mut UmbraInstance) -> UmbraCode,
    pub(crate) halt_running:
        Option<unsafe extern "C" fn(instance: *mut UmbraInstance) -> UmbraCode>,
    pub(crate) set_input: Option<
        unsafe extern "C" fn(
            instance: *mut UmbraInstance,
            param: UmbraStr,
            value: *const UmbraValue,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    pub(crate) output: Option<
        unsafe extern "C" fn(
            instance: *mut UmbraInstance,
            port: UmbraStr,
            sink: *const UmbraValueSink,
        ) -> UmbraCode,
    >,
}

/// A headed ABI struct the host reads from plugin memory.
///
/// # Safety
/// Implementors are `#[repr(C)]`, start with `abi_version: u32, struct_size: u32`, and the all-zero
/// bit pattern is a valid value (integers, raw pointers, `UmbraStr`, `Option` of function pointers
/// only).
pub(crate) unsafe trait Headed: Sized {}

// SAFETY: `#[repr(C)]`, header first; fields are integers, `UmbraStr`, a raw pointer and `Option`s
// of function pointers, for all of which zero is valid (null / `None`).
unsafe impl Headed for UmbraPluginDescriptor {}
// SAFETY: as above; integers, `UmbraStr`s and a raw pointer.
unsafe impl Headed for UmbraCapability {}
// SAFETY: as above; integers and `Option`s of function pointers.
unsafe impl Headed for UmbraBehaviorVTable {}
// SAFETY: as above; integers, an `f64`, `UmbraVec3` (three `f64`) and `UmbraStr`s.
unsafe impl Headed for UmbraIntent {}
// SAFETY: as above; integers, `f64`s, a byte array, `UmbraStr` and `UmbraVec3`.
unsafe impl Headed for UmbraValue {}

/// Context for error construction: the origin of the load.
struct Ctx<'a> {
    origin: &'a str,
}

impl Ctx<'_> {
    fn err(&self, field: impl Into<String>, message: impl Into<String>) -> PluginError {
        PluginError::Descriptor {
            origin: self.origin.to_owned(),
            field: field.into(),
            message: message.into(),
        }
    }

    /// Checks a struct pointer: non-null and 8-byte aligned.
    fn pointer<T>(&self, ptr: *const T, field: &str) -> Result<(), PluginError> {
        if ptr.is_null() {
            return Err(self.err(field, "is a null pointer"));
        }
        if !(ptr as usize).is_multiple_of(STRUCT_ALIGN) {
            return Err(self.err(field, "is not 8-byte aligned"));
        }
        Ok(())
    }

    /// Checks a header (ADR 0019): the major equals the host's and the minor is not newer
    /// (REQ-P-9), then the size is within `[minor-0 size, size of the declared minor]` and a
    /// multiple of 8. Returns the size to read.
    fn header(
        &self,
        prefix: &str,
        (abi_version, struct_size): (u32, u32),
        sizes: [usize; HOST_MINORS],
    ) -> Result<usize, PluginError> {
        let plugin = (abi_version >> 16, abi_version & 0xFFFF);
        if plugin.0 != HOST_ABI.0 || plugin.1 > HOST_ABI.1 {
            return Err(PluginError::AbiMismatch {
                origin: self.origin.to_owned(),
                field: format!("{prefix}.abi_version"),
                plugin,
                host: HOST_ABI,
            });
        }
        let size = struct_size as usize;
        let (min, max) = (sizes[0], sizes[plugin.1 as usize]);
        if size < min || size > max || !size.is_multiple_of(STRUCT_ALIGN) {
            return Err(self.err(
                format!("{prefix}.struct_size"),
                format!(
                    "is {size} bytes; ABI {}.{} needs a multiple of 8 between {min} and {max}",
                    plugin.0, plugin.1
                ),
            ));
        }
        Ok(size)
    }

    /// Copies the bytes of a string after the cap, null and length checks, and checks UTF-8.
    fn text(&self, s: UmbraStr, cap: usize, field: &str) -> Result<String, PluginError> {
        if s.len > cap {
            return Err(self.err(field, format!("is {} bytes; max is {cap}", s.len)));
        }
        // SAFETY: the length is within the cap (far below `isize::MAX`); `bytes` refuses a null
        // pointer with a non-zero length. The plugin keeps descriptor strings valid and unchanged
        // until `shutdown` (ADR 0019); they are copied right here and never read again.
        let bytes = unsafe { crate::host_api::bytes(s) }
            .ok_or_else(|| self.err(field, format!("is a null pointer with length {}", s.len)))?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| self.err(field, format!("is not valid UTF-8 ({e})")))
    }
}

/// Reads the header of the struct at `ptr`.
///
/// # Safety
/// `ptr` is non-null, 8-byte aligned and points to at least 8 readable bytes (a header).
pub(crate) unsafe fn read_header<T: Headed>(ptr: *const T) -> (u32, u32) {
    let words = ptr.cast::<[u32; 2]>();
    // SAFETY: by the caller's contract; `[u32; 2]` needs 4-byte alignment and 8 bytes.
    let [abi_version, struct_size] = unsafe { words.read() };
    (abi_version, struct_size)
}

/// Copies the first `size` bytes of the struct at `ptr` into a zeroed `T` (newer fields absent).
///
/// # Safety
/// `ptr` points to at least `size` readable bytes; `size <= size_of::<T>()` (checked by `header`).
pub(crate) unsafe fn read_prefix<T: Headed>(ptr: *const T, size: usize) -> T {
    debug_assert!(size <= size_of::<T>());
    let mut value = MaybeUninit::<T>::zeroed();
    // SAFETY: the source has `size` readable bytes (caller), the destination is a local of
    // `size_of::<T>() >= size` bytes, and the two do not overlap.
    unsafe {
        std::ptr::copy_nonoverlapping(
            ptr.cast::<u8>(),
            value.as_mut_ptr().cast::<u8>(),
            size.min(size_of::<T>()),
        );
    }
    // SAFETY: `T: Headed` makes all-zero a valid value, and the copied prefix holds plugin-written
    // values of the same field types (integers, pointers, nullable function pointers: every bit
    // pattern a C writer can produce for them is valid; enums and booleans are integers, ADR 0019).
    unsafe { value.assume_init() }
}

/// Copies and validates the descriptor at `ptr` (non-null).
///
/// # Safety
/// `ptr` is what the plugin's entry returned: if aligned, it points to a descriptor of at least its
/// `struct_size` bytes, and every pointer in it is valid as documented in `umbra-plugin-api`
/// (trusted plugin code, NFR-4). The host checks everything it can (null, alignment, sizes, caps,
/// UTF-8).
pub(crate) unsafe fn copy(
    ptr: *const UmbraPluginDescriptor,
    origin: &str,
) -> Result<Descriptor, PluginError> {
    let cx = Ctx { origin };
    cx.pointer(ptr, "descriptor")?;
    // SAFETY: non-null and aligned (checked); a descriptor has at least a header (caller).
    let size = cx.header("descriptor", unsafe { read_header(ptr) }, DESCRIPTOR_SIZES)?;
    // SAFETY: the plugin wrote `struct_size` bytes; `size` is checked against the host's size.
    let raw = unsafe { read_prefix(ptr, size) };

    let id_text = cx.text(raw.id, UMBRA_MAX_ID_LEN, "descriptor.id")?;
    let id = PluginId::new(&id_text).map_err(|e| cx.err("descriptor.id", e.to_string()))?;
    if id_text.starts_with(RESERVED_PREFIX) {
        return Err(PluginError::ReservedId {
            origin: origin.to_owned(),
            id: id_text,
        });
    }
    let name = cx.text(raw.name, MAX_TEXT_LEN, "descriptor.name")?;
    let version_text = cx.text(raw.version, MAX_TEXT_LEN, "descriptor.version")?;
    let version = Version::parse(&version_text).map_err(|e| {
        cx.err(
            "descriptor.version",
            format!("{version_text:?} is not a semantic version: {e}"),
        )
    })?;
    // SAFETY: forwarded caller contract for the capability array.
    let capabilities = unsafe { capabilities(&cx, &raw, &version) }?;
    let abi_version = raw.abi_version;
    Ok(Descriptor {
        id,
        name,
        version,
        abi: (abi_version >> 16, abi_version & 0xFFFF),
        init: raw.init,
        shutdown: raw.shutdown,
        capabilities,
    })
}

/// The capability array of `raw`.
///
/// # Safety
/// As [`copy`].
unsafe fn capabilities(
    cx: &Ctx<'_>,
    raw: &UmbraPluginDescriptor,
    plugin_version: &Version,
) -> Result<Vec<Capability>, PluginError> {
    let count = raw.capability_count;
    if count > UMBRA_MAX_CAPABILITIES {
        return Err(cx.err(
            "descriptor.capability_count",
            format!("is {count}; max is {UMBRA_MAX_CAPABILITIES}"),
        ));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = raw.capabilities;
    if base.is_null() {
        return Err(cx.err(
            "descriptor.capabilities",
            format!("is a null pointer with capability_count {count}"),
        ));
    }
    cx.pointer(base, "descriptor.capabilities")?;
    // SAFETY: non-null, aligned, `count > 0`: the first element's header is readable (caller).
    let stride = cx.header(
        "capabilities[0]",
        unsafe { read_header(base) },
        CAPABILITY_SIZES,
    )?;
    if count
        .checked_mul(stride)
        .is_none_or(|total| isize::try_from(total).is_err())
    {
        return Err(cx.err("descriptor.capabilities", "array size overflows"));
    }
    let mut out: Vec<Capability> = Vec::with_capacity(count);
    let mut seen = BTreeSet::new();
    for index in 0..count {
        let prefix = format!("capabilities[{index}]");
        // SAFETY: `index * stride` is within the checked array size; the plugin promises `count`
        // elements of `stride` bytes (caller). `stride` is a multiple of 8, so alignment is kept.
        let ptr = unsafe { base.byte_add(index * stride) };
        // SAFETY: inside the array (above); aligned and non-null.
        let header = unsafe { read_header(ptr) };
        let size = cx.header(&prefix, header, CAPABILITY_SIZES)?;
        if size != stride {
            return Err(cx.err(
                format!("{prefix}.struct_size"),
                format!("is {size} bytes but capabilities[0] is {stride} (one array, one stride)"),
            ));
        }
        // SAFETY: `size` bytes of this element are readable (caller), `size` checked.
        let cap = unsafe { read_prefix(ptr, size) };
        // SAFETY: forwarded caller contract for the strings and the vtable of this capability.
        let capability = unsafe { capability(cx, &prefix, &cap, plugin_version) }?;
        if !seen.insert(capability.id.clone()) {
            return Err(cx.err(
                format!("{prefix}.id"),
                format!("duplicate capability {} in one plugin", capability.id),
            ));
        }
        out.push(capability);
    }
    Ok(out)
}

/// One capability.
///
/// # Safety
/// As [`copy`], for the strings and the vtable `cap` points to.
unsafe fn capability(
    cx: &Ctx<'_>,
    prefix: &str,
    cap: &UmbraCapability,
    plugin_version: &Version,
) -> Result<Capability, PluginError> {
    let id_field = format!("{prefix}.id");
    let id_text = cx.text(cap.id, UMBRA_MAX_ID_LEN, &id_field)?;
    let id = ComponentId::new(&id_text).map_err(|e| cx.err(&id_field, e.to_string()))?;
    if id_text.starts_with(RESERVED_PREFIX) {
        return Err(PluginError::ReservedId {
            origin: cx.origin.to_owned(),
            id: id_text,
        });
    }
    if cap.kind != UMBRA_CAPABILITY_BEHAVIOR {
        return Err(PluginError::Unsupported {
            origin: cx.origin.to_owned(),
            capability: id_text,
            kind: cap.kind,
        });
    }
    let version_field = format!("{prefix}.version");
    let version_text = cx.text(cap.version, MAX_TEXT_LEN, &version_field)?;
    let version = if version_text.is_empty() {
        plugin_version.clone()
    } else {
        Version::parse(&version_text).map_err(|e| {
            cx.err(
                &version_field,
                format!("{version_text:?} is not a semantic version: {e}"),
            )
        })?
    };
    let schema_field = format!("{prefix}.param_schema");
    let schema_text = cx.text(cap.param_schema, UMBRA_MAX_SCHEMA_LEN, &schema_field)?;
    let schema = ParamSchema::from_json_str(&schema_text)
        .map_err(|e| cx.err(&schema_field, e.to_string()))?;
    let description = schema
        .as_json()
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or_default()
        .to_owned();
    let metadata_field = format!("{prefix}.metadata");
    let metadata_text = cx.text(cap.metadata, UMBRA_MAX_METADATA_LEN, &metadata_field)?;
    let metadata = metadata(cx, &metadata_field, &metadata_text)?;
    let vtable_field = format!("{prefix}.vtable");
    // SAFETY: forwarded caller contract for the vtable pointer.
    let vtable = unsafe { vtable(cx, &vtable_field, cap.vtable.cast()) }?;
    check_entries(cx, &vtable_field, &metadata, &vtable)?;
    check_schema(cx, &metadata_field, &metadata, &schema)?;
    Ok(Capability {
        kind: ComponentKind::Behavior,
        id,
        version,
        schema,
        description,
        metadata,
        vtable,
    })
}

/// Copies the behavior vtable at `ptr`.
///
/// # Safety
/// As [`copy`]: a non-null, aligned `ptr` points to a vtable of at least its `struct_size` bytes.
unsafe fn vtable(
    cx: &Ctx<'_>,
    field: &str,
    ptr: *const UmbraBehaviorVTable,
) -> Result<BehaviorVTable, PluginError> {
    cx.pointer(ptr, field)?;
    // SAFETY: non-null and aligned (checked); the plugin's vtable has a header (caller).
    let size = cx.header(field, unsafe { read_header(ptr) }, VTABLE_SIZES)?;
    // SAFETY: `size` bytes are readable (caller) and checked against the host's size.
    let raw = unsafe { read_prefix(ptr, size) };
    let required = |entry: bool, name: &str| {
        if entry {
            Ok(())
        } else {
            Err(cx.err(format!("{field}.{name}"), "required entry is null"))
        }
    };
    let (
        Some(configure),
        Some(destroy_behavior),
        Some(instantiate),
        Some(destroy_instance),
        Some(tick_batch),
        Some(halt),
    ) = (
        raw.configure,
        raw.destroy_behavior,
        raw.instantiate,
        raw.destroy_instance,
        raw.tick_batch,
        raw.halt,
    )
    else {
        required(raw.configure.is_some(), "configure")?;
        required(raw.destroy_behavior.is_some(), "destroy_behavior")?;
        required(raw.instantiate.is_some(), "instantiate")?;
        required(raw.destroy_instance.is_some(), "destroy_instance")?;
        required(raw.tick_batch.is_some(), "tick_batch")?;
        return Err(cx.err(format!("{field}.halt"), "required entry is null"));
    };
    Ok(BehaviorVTable {
        configure,
        destroy_behavior,
        instantiate,
        destroy_instance,
        tick_batch,
        halt,
        halt_running: raw.halt_running,
        set_input: raw.set_input,
        output: raw.output,
    })
}

/// Parses behavior metadata (ADR 0020); empty text is no metadata.
fn metadata(cx: &Ctx<'_>, field: &str, text: &str) -> Result<Metadata, PluginError> {
    if text.is_empty() {
        return Ok(Metadata::default());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| cx.err(field, format!("is not JSON: {e}")))?;
    let serde_json::Value::Object(map) = value else {
        return Err(cx.err(field, "must be a JSON object"));
    };
    let mut out = Metadata::default();
    for (key, value) in &map {
        match key.as_str() {
            "param_types" => out.param_types = ports(cx, &format!("{field}.param_types"), value)?,
            "outputs" => out.outputs = ports(cx, &format!("{field}.outputs"), value)?,
            "result" => {
                let text = value
                    .as_str()
                    .ok_or_else(|| cx.err(format!("{field}.result"), "must be a string"))?;
                out.result = Some(text.to_owned());
            }
            other => {
                return Err(cx.err(
                    field,
                    format!("unknown key {other:?}; expected param_types, outputs, result"),
                ));
            }
        }
    }
    Ok(out)
}

/// `{name: bbtype}` with `[a-z][a-z0-9_]*` names and the six blackboard type names.
fn ports(
    cx: &Ctx<'_>,
    field: &str,
    value: &serde_json::Value,
) -> Result<BTreeMap<String, BbType>, PluginError> {
    let map = value
        .as_object()
        .ok_or_else(|| cx.err(field, "must be a JSON object of name to type"))?;
    let mut out = BTreeMap::new();
    for (name, ty) in map {
        let at = format!("{field}.{name}");
        umbra_core::validate_name(name).map_err(|e| cx.err(&at, e.to_string()))?;
        let ty = ty.as_str().and_then(BbType::from_name).ok_or_else(|| {
            cx.err(
                &at,
                format!("type {ty} is not one of bool, number, integer, string, entity, position"),
            )
        })?;
        out.insert(name.clone(), ty);
    }
    Ok(out)
}

/// ADR 0020: `set_input` present iff `param_types` is non-empty, `output` iff `outputs` is.
fn check_entries(
    cx: &Ctx<'_>,
    field: &str,
    metadata: &Metadata,
    vtable: &BehaviorVTable,
) -> Result<(), PluginError> {
    let rule = |declared: bool, present: bool, entry: &str, key: &str| {
        if declared == present {
            return Ok(());
        }
        let message = if declared {
            format!("the metadata declares {key}, so the entry is required")
        } else {
            format!("present without {key} in the metadata")
        };
        Err(cx.err(format!("{field}.{entry}"), message))
    };
    rule(
        !metadata.param_types.is_empty(),
        vtable.set_input.is_some(),
        "set_input",
        "param_types",
    )?;
    rule(
        !metadata.outputs.is_empty(),
        vtable.output.is_some(),
        "output",
        "outputs",
    )
}

/// ADR 0020: `param_types` agree with the schema. A literal type must match its property's `type`
/// when the schema has the property; `entity` and `position` are binding-only and must not be in
/// the schema (ADR 0014). Params absent from the schema are binding-only.
fn check_schema(
    cx: &Ctx<'_>,
    field: &str,
    metadata: &Metadata,
    schema: &ParamSchema,
) -> Result<(), PluginError> {
    let properties = schema.as_json().get("properties");
    for (name, ty) in &metadata.param_types {
        let Some(property) = properties.and_then(|p| p.get(name)) else {
            continue;
        };
        let at = format!("{field}.param_types.{name}");
        let expected = match ty {
            BbType::Bool => "boolean",
            BbType::Number => "number",
            BbType::Integer => "integer",
            BbType::String => "string",
            _ => {
                return Err(cx.err(
                    at,
                    format!("{ty:?} params are binding-only and must not be in the param schema"),
                ));
            }
        };
        let agrees = match property.get("type") {
            None => true,
            Some(serde_json::Value::String(t)) => t == expected,
            Some(serde_json::Value::Array(ts)) => ts.iter().any(|t| t == expected),
            Some(_) => false,
        };
        if !agrees {
            return Err(cx.err(
                at,
                format!(
                    "declared {ty:?} but the schema property has type {}",
                    property
                        .get("type")
                        .map(ToString::to_string)
                        .unwrap_or_default()
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> Ctx<'static> {
        Ctx { origin: "unit" }
    }

    #[test]
    fn size_tables_cover_every_host_minor() {
        assert_eq!(DESCRIPTOR_SIZES.len(), HOST_MINORS);
        assert_eq!(DESCRIPTOR_SIZES[0], size_of::<UmbraPluginDescriptor>());
        assert_eq!(CAPABILITY_SIZES[0], size_of::<UmbraCapability>());
        assert_eq!(VTABLE_SIZES[0], size_of::<UmbraBehaviorVTable>());
    }

    #[test]
    fn header_checks_major_minor_and_size() {
        let c = cx();
        let own = DESCRIPTOR_SIZES[0];
        let own32 = u32::try_from(own).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(c.header("d", (0, own32), DESCRIPTOR_SIZES).ok(), Some(own));
        assert!(matches!(
            c.header("d", (1 << 16, own32), DESCRIPTOR_SIZES),
            Err(PluginError::AbiMismatch { field, .. }) if field == "d.abi_version"
        ));
        assert!(matches!(
            c.header("d", (1, own32), DESCRIPTOR_SIZES),
            Err(PluginError::AbiMismatch { .. })
        ));
        for bad in [0, 8, own32 - 4, own32 + 8] {
            assert!(matches!(
                c.header("d", (0, bad), DESCRIPTOR_SIZES),
                Err(PluginError::Descriptor { field, .. }) if field == "d.struct_size"
            ));
        }
    }

    #[test]
    fn read_prefix_zero_fills_fields_past_the_size() {
        let vt = UmbraBehaviorVTable {
            abi_version: 0,
            struct_size: 16,
            configure: None,
            destroy_behavior: None,
            instantiate: None,
            destroy_instance: None,
            tick_batch: None,
            halt: None,
            halt_running: None,
            set_input: None,
            output: None,
        };
        // SAFETY: a valid local vtable; 8 bytes is the header only.
        let copy = unsafe { read_prefix(&raw const vt, 8) };
        assert_eq!(copy.struct_size, 16);
        assert!(copy.configure.is_none());
    }

    #[test]
    fn metadata_parses_and_rejects_bad_shapes() {
        let c = cx();
        let m = metadata(
            &c,
            "m",
            r#"{"param_types":{"a":"number"},"outputs":{"b":"entity"},"result":"r"}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.param_types.get("a"), Some(&BbType::Number));
        assert_eq!(m.outputs.get("b"), Some(&BbType::Entity));
        assert_eq!(m.result.as_deref(), Some("r"));
        for (text, field) in [
            ("[]", "m"),
            (r#"{"result":1}"#, "m.result"),
            (r#"{"outputs":[]}"#, "m.outputs"),
            (r#"{"outputs":{"B":"bool"}}"#, "m.outputs.B"),
            (r#"{"param_types":{"a":"float"}}"#, "m.param_types.a"),
        ] {
            let err = metadata(&c, "m", text).err();
            assert!(
                matches!(&err, Some(PluginError::Descriptor { field: f, .. }) if f == field),
                "{text}: {err:?}"
            );
        }
    }

    #[test]
    fn binding_only_types_must_not_be_in_the_schema() {
        let c = cx();
        let schema = ParamSchema::from_json_str(concat!(
            r#"{"type":"object","properties":{"t":{"type":"integer"},"#,
            r#""n":{"type":["number","null"]}}}"#
        ))
        .unwrap_or_else(|e| panic!("{e}"));
        let mut m = Metadata::default();
        m.param_types.insert("n".to_owned(), BbType::Number);
        m.param_types.insert("free".to_owned(), BbType::String);
        assert!(check_schema(&c, "m", &m, &schema).is_ok());
        m.param_types.insert("t".to_owned(), BbType::Entity);
        assert!(check_schema(&c, "m", &m, &schema).is_err());
    }
}
