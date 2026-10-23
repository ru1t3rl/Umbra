//! AT-2 (M4 WI-4): the generated `umbra_plugin_api.h` compiles as strict C11 and agrees with the Rust
//! layout (REQ-SDK-2, REQ-P-1, REQ-P-2, ADR 0019 R-M6/R-M7, ADR 0023 item 3).
//!
//! `generated_offset_and_size_asserts_compile` writes one `_Static_assert` per struct size, alignment,
//! field offset, field size and field type, typedef width and constant, all taken from Rust
//! (`offset_of!`, `size_of`, `type_name`, the constants themselves) through
//! `umbra-plugin-api/tests/support/abi_layout.rs`, so no number is copied by hand. Flags:
//! `-std=c11 -pedantic -Wall -Wextra -Werror`, MSVC `/std:c11 /W4 /WX`. Needs a C compiler (a gate
//! prerequisite, `docs/build.md`).

#![forbid(unsafe_code)]

#[path = "../../umbra-plugin-api/tests/support/abi_layout.rs"]
mod abi_layout;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn include_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../umbra-plugin-api/include")
}

/// The target triple of this test binary, for `cc` (which normally reads it from a build script's
/// environment). Only the ABI's platforms (ASM-120: 64-bit Linux, Windows, macOS) are mapped exactly:
/// Linux is assumed `-gnu` and Windows `-msvc`; musl, MinGW and other targets are out of v0 scope.
fn target_triple() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => format!("{arch}-unknown-linux-gnu"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        "macos" => format!("{arch}-apple-darwin"),
        os => format!("{arch}-unknown-{os}"),
    }
}

/// Compiles `source` (one translation unit, written to a fresh directory under `CARGO_TARGET_TMPDIR`)
/// against the header with strict C11 flags. `Err` carries the compiler's output (stdout and stderr;
/// MSVC reports on stdout).
fn compile(name: &str, source: &str) -> Result<(), String> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("c-header")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let file = dir.join(format!("{name}.c"));
    std::fs::write(&file, source).map_err(|e| format!("write {}: {e}", file.display()))?;

    let triple = target_triple();
    let mut build = cc::Build::new();
    build
        .cargo_metadata(false)
        .cargo_warnings(false)
        .emit_rerun_if_env_changed(false)
        .target(&triple)
        .host(&triple)
        .opt_level(0)
        .debug(false)
        .out_dir(&dir)
        .include(include_dir())
        // `-Wall -Wextra -Werror` (GNU, Clang), `-W4 -WX` (MSVC). Never `warnings(false)`: it adds `-w`.
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true)
        .std("c11");
    let msvc = build
        .try_get_compiler()
        .map_err(|e| e.to_string())?
        .is_like_msvc();
    if !msvc {
        build.flag("-pedantic");
    }
    // The tool carries every flag above; run it ourselves to capture the diagnostics.
    let mut cmd: Command = build
        .try_get_compiler()
        .map_err(|e| e.to_string())?
        .to_command();
    let object = dir.join(format!("{name}.o"));
    if msvc {
        cmd.arg("/c")
            .arg(&file)
            .arg(format!("/Fo{}", object.display()));
    } else {
        cmd.arg("-c").arg(&file).arg("-o").arg(&object);
    }
    let out = cmd.output().map_err(|e| format!("spawn compiler: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// One `_Static_assert` line.
fn assert_line(out: &mut String, condition: &str) {
    let message = condition.replace('"', "'");
    let _ = writeln!(out, "_Static_assert({condition}, \"{message}\");");
}

/// The C type a Rust field type (`type_name`) must have, for the `_Generic` check; `None` for types
/// `_Generic` cannot check directly (arrays decay, function pointers are checked by size and by the
/// compiled descriptor test).
fn c_type(rust: &str) -> Option<String> {
    let prim = |t: &str| -> Option<String> {
        let c = match t {
            "f64" => "double",
            "i64" => "int64_t",
            "u64" => "uint64_t",
            "i32" => "int32_t",
            "u32" => "uint32_t",
            "u8" => "uint8_t",
            "usize" => "size_t",
            "core::ffi::c_void" => "void",
            _ => {
                let last = t.rsplit("::").next()?;
                return last.starts_with("Umbra").then(|| last.to_string());
            }
        };
        Some(c.to_string())
    };
    if let Some(inner) = rust.strip_prefix("*const ") {
        return prim(inner).map(|c| format!("const {c} *"));
    }
    if let Some(inner) = rust.strip_prefix("*mut ") {
        return prim(inner).map(|c| format!("{c} *"));
    }
    if rust.starts_with('[') || rust.starts_with("core::option::Option") {
        return None;
    }
    prim(rust)
}

/// Pinned field counts per ABI minor, index = minor (ASM-641): 112 fields in ABI 0.0 (the M4 layout
/// table), 38 appended by ABI 0.1 (M6, ADR 0033), 80 appended by ABI 0.2 (M7, ADR 0050). A minor bump
/// appends one entry; an appended field changes the sum.
const FIELDS_BY_MINOR: &[usize] = &[112, 38, 80];
// One entry per minor of the header: a minor bump without its entry fails to compile.
const _: () =
    assert!(FIELDS_BY_MINOR.len() == umbra_plugin_api::UMBRA_ABI_VERSION_MINOR as usize + 1);

/// The C file with every layout fact from Rust; returns it, the number of field offset asserts and the
/// number of field type asserts.
fn generated_asserts() -> (String, usize, usize) {
    let mut out = String::from(
        "/* Generated by umbra-plugin-host/tests/c_header.rs from Rust offset_of!/size_of (R-M7). */\n\
         #include <stddef.h>\n\
         #include \"umbra_plugin_api.h\"\n\n",
    );
    let (mut offsets, mut types) = (0, 0);
    for s in abi_layout::structs() {
        let t = s.name;
        assert_line(&mut out, &format!("sizeof({t}) == {}", s.size));
        assert_line(&mut out, &format!("_Alignof({t}) == {}", s.align));
        for f in &s.fields {
            let field = format!("(({t} *)0)->{}", f.name);
            assert_line(
                &mut out,
                &format!("offsetof({t}, {}) == {}", f.name, f.offset),
            );
            assert_line(&mut out, &format!("sizeof({field}) == {}", f.size));
            offsets += 1;
            if let Some(c) = c_type(f.ty) {
                assert_line(&mut out, &format!("_Generic({field}, {c}: 1, default: 0)"));
                types += 1;
            }
        }
    }
    for (t, size) in abi_layout::typedefs() {
        assert_line(&mut out, &format!("sizeof({t}) == {size}"));
    }
    for (c, value) in abi_layout::constants() {
        assert_line(&mut out, &format!("{c} == {value}"));
    }
    (out, offsets, types)
}

/// The header alone (included twice, to exercise the include guard) is clean strict C11.
#[test]
fn header_compiles_as_strict_c11() {
    let source = "#include \"umbra_plugin_api.h\"\n#include \"umbra_plugin_api.h\"\n";

    if let Err(e) = compile("header_only", source) {
        panic!("umbra_plugin_api.h does not compile as strict C11:\n{e}");
    }
}

/// R-M7: every size, alignment, field offset, field size and scalar or pointer field type, typedef
/// width and constant that Rust sees holds in C.
#[test]
fn generated_offset_and_size_asserts_compile() {
    let (source, offsets, types) = generated_asserts();
    // Pinned: the M4 layout table has 112 fields (ABI 0.0); an appended field changes this number.
    // ABI 0.1 (M6, ADR 0033) appends four structs with 38 fields, ABI 0.2 (M7, ADR 0050) twelve with
    // 80: 230, the sum of `FIELDS_BY_MINOR`.
    assert_eq!(offsets, FIELDS_BY_MINOR.iter().sum::<usize>());
    assert!(types >= 90, "only {types} field types checked");

    if let Err(e) = compile("layout_asserts", &source) {
        panic!("C layout differs from Rust:\n{e}\n--- source ---\n{source}");
    }
}

/// The hand-kept type lists agree: the type names the header declares (`} Name;`, `typedef .. Name;`,
/// `typedef ..(*Name)(..);`) are exactly the support table's structs and typedefs plus the two opaque
/// handles. A type missing from `cbindgen.toml`'s `[export] include` fails here.
#[test]
fn header_declares_exactly_the_abi_types() {
    let path = include_dir().join("umbra_plugin_api.h");
    let header =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut declared = BTreeSet::new();
    for line in header.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("} ").and_then(|r| r.strip_suffix(';')) {
            declared.insert(rest.trim().to_string());
        } else if line.starts_with("typedef ") && line.ends_with(';') {
            let name = match line.split_once("(*") {
                Some((_, rest)) => rest.split(')').next().unwrap_or(""),
                None => line.trim_end_matches(';').rsplit(' ').next().unwrap_or(""),
            };
            declared.insert(name.to_string());
        }
    }
    let mut expected: BTreeSet<String> = abi_layout::structs()
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    expected.extend(abi_layout::typedefs().iter().map(|(n, _)| (*n).to_string()));
    expected.extend(["UmbraBehavior".to_string(), "UmbraInstance".to_string()]);
    expected.insert("UmbraNetAdapter".to_string());
    expected.extend(
        [
            "UmbraMotionModel",
            "UmbraSensor",
            "UmbraCondition",
            "UmbraAction",
            "UmbraActionInstance",
        ]
        .map(String::from),
    );

    assert_eq!(declared, expected);
}

/// Control: a wrong number makes the compile fail, so the assert file is really checked.
#[test]
fn a_wrong_layout_assert_fails_to_compile() {
    let source = "#include \"umbra_plugin_api.h\"\n\
                  _Static_assert(sizeof(UmbraEntityView) == 161, \"deliberately wrong\");\n";

    let err = compile("wrong_assert", source).err();

    assert!(
        err.as_deref()
            .is_some_and(|e| e.contains("deliberately wrong")),
        "expected the failing assert to be reported, got {err:?}"
    );
}

/// Control: the strict flags are applied (an unused variable is an error under `-Wall -Werror` and
/// `/W4 /WX`, MSVC C4189), so "compiles cleanly" means without warnings.
#[test]
fn a_warning_fails_the_compile() {
    let source = "#include \"umbra_plugin_api.h\"\n\
                  int umbra_probe(void);\n\
                  int umbra_probe(void) { int unused = 0; return 0; }\n";

    let err = compile("warning_is_error", source).err();

    assert!(
        err.as_deref()
            .is_some_and(|e| e.contains("unused") || e.contains("C4189")),
        "expected an unused-variable error, got {err:?}"
    );
}

/// R-M6: the handles stay incomplete in C, so a plugin can never take their size or dereference them.
#[test]
fn opaque_handles_are_incomplete() {
    for handle in ["UmbraBehavior", "UmbraInstance"] {
        let source = format!(
            "#include \"umbra_plugin_api.h\"\n\
             size_t umbra_probe(void);\n\
             size_t umbra_probe(void) {{ return sizeof({handle}); }}\n"
        );

        assert!(
            compile(&format!("sizeof_{handle}"), &source).is_err(),
            "sizeof({handle}) compiled: the handle is not incomplete"
        );
    }
}

/// REQ-P-1: the header declares `umbra_plugin_entry`, so a definition with another signature (here a
/// wrong return type) does not compile.
#[test]
fn entry_with_a_wrong_signature_fails_to_compile() {
    let source = "#include \"umbra_plugin_api.h\"\n\
                  UMBRA_EXPORT const UmbraHostApi *umbra_plugin_entry(const UmbraHostApi *host) {\n\
                  \x20   return host;\n\
                  }\n";

    let err = compile("wrong_entry", source).err();

    assert!(
        err.as_deref()
            .is_some_and(|e| e.contains("umbra_plugin_entry")),
        "expected a conflicting-types error for umbra_plugin_entry, got {err:?}"
    );
}

/// R-M6, RR-m1: `UMBRA_STR` (expression), `UMBRA_STR_INIT` (static initializer), `UMBRA_EXPORT` on the
/// entry definition (declared by the header) and the incomplete handle types compile cleanly in a
/// static descriptor of the shape a C plugin writes.
#[test]
fn umbra_str_export_and_handles_compile_cleanly() {
    let source = r#"#include "umbra_plugin_api.h"

static UmbraCode probe_configure(UmbraStr params, UmbraBehavior **out, const UmbraErrorSink *err) {
    (void)params;
    (void)err;
    *out = NULL;
    return UMBRA_ERR_UNSUPPORTED;
}

static UmbraCode probe_halt(UmbraInstance *instance) {
    (void)instance;
    return UMBRA_OK;
}

static const UmbraBehaviorVTable PROBE_VTABLE = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraBehaviorVTable),
    probe_configure, NULL, NULL, NULL, NULL, probe_halt, NULL, NULL, NULL,
};

static const UmbraCapability PROBE_CAPABILITIES[1] = {{
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraCapability), UMBRA_CAPABILITY_BEHAVIOR, 0,
    UMBRA_STR_INIT("example.behavior.probe"), UMBRA_STR_INIT(""), UMBRA_STR_INIT("{}"),
    UMBRA_STR_INIT(""), &PROBE_VTABLE,
}};

static const UmbraPluginDescriptor PROBE_DESCRIPTOR = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraPluginDescriptor),
    UMBRA_STR_INIT("example.plugin.probe"), UMBRA_STR_INIT("Probe"), UMBRA_STR_INIT("0.1.0"),
    PROBE_CAPABILITIES, 1, NULL, NULL,
};

UMBRA_EXPORT const UmbraPluginDescriptor *umbra_plugin_entry(const UmbraHostApi *host) {
    UmbraPluginEntry self = umbra_plugin_entry;
    UmbraStr expression = UMBRA_STR("expression");
    (void)self;
    (void)host;
    return expression.len == 10 ? &PROBE_DESCRIPTOR : NULL;
}
"#;

    if let Err(e) = compile("macros_and_handles", source) {
        panic!("UMBRA_STR/UMBRA_STR_INIT/UMBRA_EXPORT/handles do not compile cleanly:\n{e}");
    }
}

/// R-M6 for the opaque handles of ABI 0.1 and 0.2 (ADR 0050, ASM-679): each stays incomplete in C, so a
/// plugin can never take its size or dereference it.
#[test]
fn abi_0_2_handles_are_incomplete() {
    for handle in [
        "UmbraNetAdapter",
        "UmbraMotionModel",
        "UmbraSensor",
        "UmbraCondition",
        "UmbraAction",
        "UmbraActionInstance",
    ] {
        let source = format!(
            "#include \"umbra_plugin_api.h\"\n\
             size_t umbra_probe(void);\n\
             size_t umbra_probe(void) {{ return sizeof({handle}); }}\n"
        );

        let err = compile(&format!("sizeof_{handle}"), &source).err();

        assert!(
            err.as_deref().is_some_and(|e| e.contains(handle)),
            "sizeof({handle}) compiled or failed for another reason: {err:?}"
        );
    }
}

/// A C plugin with one capability of each ABI 0.2 kind (for
/// `abi_0_2_kinds_compile_cleanly_in_a_static_descriptor`).
const ABI_0_2_KINDS_PROBE: &str = r#"#include "umbra_plugin_api.h"

static UmbraCode m_configure(UmbraStr params, UmbraMotionModel **out, const UmbraErrorSink *err) {
    (void)params;
    (void)err;
    *out = NULL;
    return UMBRA_ERR_UNSUPPORTED;
}
static void m_destroy(UmbraMotionModel *model) { (void)model; }
static UmbraCode m_step(UmbraMotionModel *model, const UmbraMotionStep *step,
                        UmbraKinematicState *out, const UmbraErrorSink *err) {
    double height = 0.0;
    (void)model;
    (void)err;
    if (step->terrain->height_at(step->terrain->ctx, &step->state->position, &height) != UMBRA_OK) {
        return UMBRA_ERR_INTERNAL;
    }
    out->position = step->state->position;
    out->velocity = step->state->velocity;
    out->orientation = step->state->orientation;
    out->position.z += height * step->dt * (double)step->rate_hz * (double)step->tick;
    return step->intent_count > 0 && step->intents[0].kind == UMBRA_INTENT_HOLD ? UMBRA_OK : UMBRA_OK;
}

static UmbraCode s_configure(UmbraStr params, UmbraSensor **out, const UmbraErrorSink *err) {
    (void)params;
    (void)err;
    *out = NULL;
    return UMBRA_ERR_UNSUPPORTED;
}
static void s_destroy(UmbraSensor *sensor) { (void)sensor; }
static UmbraCode s_sense(UmbraSensor *sensor, const UmbraSenseCall *call,
                         const UmbraContactSink *sink, const UmbraErrorSink *err) {
    size_t i;
    uint8_t visible = 0;
    (void)sensor;
    (void)err;
    (void)call->rng;
    for (i = 0; i < call->candidate_count; i++) {
        const UmbraTarget *t = &call->candidates[i];
        UmbraContact contact = {.abi_version = UMBRA_ABI_VERSION,
                                .struct_size = (uint32_t)sizeof(UmbraContact)};
        if (t->ownership != UMBRA_OWNERSHIP_OWNED && t->ownership != UMBRA_OWNERSHIP_REMOTE) {
            return UMBRA_ERR_INVALID_ARGUMENT;
        }
        if (call->terrain->line_of_sight(call->terrain->ctx, &call->observer->position, &t->position,
                                         &visible) != UMBRA_OK || !visible) {
            continue;
        }
        contact.entity = t->id;
        contact.position = t->position;
        contact.velocity = t->velocity;
        contact.force = t->force;
        if (sink->push(sink->ctx, &contact) != UMBRA_OK) {
            return UMBRA_ERR_INTERNAL;
        }
    }
    return UMBRA_OK;
}

static UmbraCode c_configure(UmbraStr params, UmbraCondition **out, const UmbraErrorSink *err) {
    (void)params;
    (void)err;
    *out = NULL;
    return UMBRA_ERR_UNSUPPORTED;
}
static void c_destroy(UmbraCondition *condition) { (void)condition; }
static UmbraCode c_check(UmbraCondition *condition, const UmbraLeafCall *call, uint8_t *out,
                         const UmbraErrorSink *err) {
    (void)condition;
    (void)err;
    (void)call->rng;
    if (*out != UMBRA_CONDITION_UNSET) {
        return UMBRA_ERR_INVALID_ARGUMENT;
    }
    if (call->blackboard->get(call->blackboard->ctx, UMBRA_STR("key"), NULL) != UMBRA_OK) {
        return UMBRA_ERR_INTERNAL;
    }
    *out = call->entity->id > call->tick + call->rate_hz ? 1 : 0;
    return UMBRA_OK;
}

static UmbraCode a_configure(UmbraStr params, UmbraAction **out, const UmbraErrorSink *err) {
    (void)params;
    (void)err;
    *out = NULL;
    return UMBRA_ERR_UNSUPPORTED;
}
static void a_destroy(UmbraAction *action) { (void)action; }
static UmbraCode a_instantiate(UmbraAction *action, uint32_t rate_hz, UmbraActionInstance **out,
                               const UmbraErrorSink *err) {
    (void)action;
    (void)rate_hz;
    (void)err;
    *out = NULL;
    return UMBRA_ERR_UNSUPPORTED;
}
static void a_destroy_instance(UmbraActionInstance *instance) { (void)instance; }
static UmbraCode a_tick(UmbraActionInstance *instance, const UmbraLeafCall *call,
                        const UmbraIntentSink *intents, UmbraStatus *out, const UmbraErrorSink *err) {
    UmbraIntent hold = {.abi_version = UMBRA_ABI_VERSION, .struct_size = (uint32_t)sizeof(UmbraIntent),
                        .kind = UMBRA_INTENT_HOLD};
    (void)instance;
    (void)err;
    if (call->blackboard->set(call->blackboard->ctx, UMBRA_STR("key"), NULL) != UMBRA_OK ||
        intents->push(intents->ctx, 0, &hold) != UMBRA_OK) {
        return UMBRA_ERR_INTERNAL;
    }
    *out = UMBRA_STATUS_SUCCESS;
    return UMBRA_OK;
}
static UmbraCode a_halt(UmbraActionInstance *instance) {
    (void)instance;
    return UMBRA_OK;
}

static const UmbraMotionVTable MOTION = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraMotionVTable), m_configure, m_destroy, m_step,
};
static const UmbraSensorVTable SENSOR = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraSensorVTable), s_configure, s_destroy, s_sense,
};
static const UmbraConditionVTable CONDITION = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraConditionVTable), c_configure, c_destroy, c_check,
};
static const UmbraActionVTable ACTION = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraActionVTable), a_configure, a_destroy, a_instantiate,
    a_destroy_instance, a_tick, a_halt,
};

static const UmbraCapability CAPABILITIES[4] = {
    {UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraCapability), UMBRA_CAPABILITY_MOTION_MODEL, 0,
     UMBRA_STR_INIT("example.motion.probe"), UMBRA_STR_INIT(""), UMBRA_STR_INIT("{}"),
     UMBRA_STR_INIT(""), &MOTION},
    {UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraCapability), UMBRA_CAPABILITY_SENSOR, 0,
     UMBRA_STR_INIT("example.sensor.probe"), UMBRA_STR_INIT(""), UMBRA_STR_INIT("{}"),
     UMBRA_STR_INIT(""), &SENSOR},
    {UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraCapability), UMBRA_CAPABILITY_CONDITION, 0,
     UMBRA_STR_INIT("example.cond.probe"), UMBRA_STR_INIT(""), UMBRA_STR_INIT("{}"),
     UMBRA_STR_INIT(""), &CONDITION},
    {UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraCapability), UMBRA_CAPABILITY_ACTION, 0,
     UMBRA_STR_INIT("example.action.probe"), UMBRA_STR_INIT(""), UMBRA_STR_INIT("{}"),
     UMBRA_STR_INIT(""), &ACTION},
};

static const UmbraPluginDescriptor DESCRIPTOR = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraPluginDescriptor),
    UMBRA_STR_INIT("example.plugin.kinds_probe"), UMBRA_STR_INIT("Kinds probe"),
    UMBRA_STR_INIT("0.1.0"), CAPABILITIES, 4, NULL, NULL,
};

UMBRA_EXPORT const UmbraPluginDescriptor *umbra_plugin_entry(const UmbraHostApi *host) {
    (void)host;
    return &DESCRIPTOR;
}
"#;

/// ADR 0050 (ASM-679): a static descriptor exporting one capability of each ABI 0.2 kind, every vtable
/// initialised in field order with functions of the declared types, compiles cleanly as strict C11.
/// The functions use the call records, services and sinks, so their field names and types are checked
/// from C too. Compile-only: nothing is loaded.
#[test]
fn abi_0_2_kinds_compile_cleanly_in_a_static_descriptor() {
    let source = ABI_0_2_KINDS_PROBE;

    if let Err(e) = compile("abi_0_2_kinds", source) {
        panic!("a descriptor with the ABI 0.2 kinds does not compile cleanly:\n{e}");
    }
}
