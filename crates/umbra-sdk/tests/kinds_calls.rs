//! AT-17 (M7 WI-10, call-level rows; ASM-667): the trampolines of the four ABI 0.2 kinds (ADR 0050)
//! driven through the raw vtables of a plugin built with `export_plugin!`, the test playing the host
//! (`kinds_harness`). For each kind: the call record reaches the trait as the safe types (time,
//! state, intents, targets, entity view); the terrain and blackboard wrappers forward the call's own
//! token and arguments; a panic becomes `UMBRA_ERR_PANIC` with its message; an `Error` becomes its code
//! and message; a host refusal ends the call with the host's code even when the plugin ignores it;
//! malformed host input is `UMBRA_ERR_INVALID_ARGUMENT` before any trait call; and every struct the SDK
//! writes during the calls (contacts, intents, values, the value sink) carries the kind's minor (2)
//! and its own size (ADR 0033).
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "the test harness plays the C side with small numbers"
)]

mod kinds_harness;

use std::cell::RefCell;
use std::mem::size_of;

use kinds_harness::{
    BB_CTX, CONTACT_CTX, GetMode, HOST_CTX, HostValue, INTENT_CTX, NORMAL, RNG_TOKEN, TERRAIN_CTX,
    action_configure, blackboard, board_value, capability, check, condition_configure, contacts,
    draws, get_mode, halt, headers, height_of, host_api, instantiate, instantiate_at, intents,
    kinematic, leaf_call, logs, los_byte, motion_configure, motion_step, raw_intent, refuse, reset,
    second_report, sense, sense_call, sensor_configure, services, set_board, sink_tokens, step,
    target, terrain, tick, view, visible, vtable,
};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_CAPABILITY_ACTION,
    UMBRA_CAPABILITY_CONDITION, UMBRA_CAPABILITY_MOTION_MODEL, UMBRA_CAPABILITY_SENSOR,
    UMBRA_CONDITION_UNSET, UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC,
    UMBRA_ERR_PARAMS, UMBRA_ERR_STATE, UMBRA_INTENT_CUSTOM, UMBRA_INTENT_DESPAWN,
    UMBRA_INTENT_FIRE, UMBRA_INTENT_FOLLOW_ROUTE, UMBRA_INTENT_HOLD, UMBRA_INTENT_MOVE_TO,
    UMBRA_INTENT_SET_HEADING, UMBRA_INTENT_SET_SPEED, UMBRA_INTENT_SPAWN, UMBRA_LOG_ERROR,
    UMBRA_OK, UMBRA_OWNERSHIP_OWNED, UMBRA_OWNERSHIP_REMOTE, UMBRA_STATUS_RUNNING,
    UMBRA_STATUS_SUCCESS, UMBRA_STATUS_UNSET, UmbraActionVTable, UmbraConditionVTable,
    UmbraContact, UmbraIntent, UmbraMotionVTable, UmbraPluginDescriptor, UmbraSensorVTable,
    UmbraStr, UmbraValue, UmbraValueSink,
};
use umbra_sdk::{
    Action, ActionInstance, BlackboardAccess, Condition, Contact, Contacts, EntityView, Error,
    Intent, Intents, KinematicState, LeafCtx, MotionCtx, MotionModel, Ownership, SenseCtx, Sensor,
    Status, TargetView, Value, export_plugin, serde_json,
};

// ---------------------------------------------------------------- what the plugin saw

thread_local! {
    static SAW: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn saw(what: impl Into<String>) {
    SAW.with(|s| s.borrow_mut().push(what.into()));
}

fn seen() -> Vec<String> {
    SAW.with(|s| s.borrow().clone())
}

fn clear() {
    reset();
    SAW.with(|s| s.borrow_mut().clear());
}

/// The `mode` (and `key`) of a probe's params.
fn mode_of(params: &str) -> Result<(String, String), Error> {
    let value: serde_json::Value =
        serde_json::from_str(params).map_err(|e| Error::params(format!("params: {e}")))?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mode = field("mode");
    match mode.as_str() {
        "bad" => Err(Error::params("unknown mode bad")),
        "panic_configure" => panic!("boom configure"),
        _ => Ok((mode, field("key"))),
    }
}

// ---------------------------------------------------------------- the plugin

struct MotionProbe {
    mode: String,
}

impl MotionModel for MotionProbe {
    const ID: &'static str = "test.motion.probe";
    const SCHEMA: &'static str = r#"{"type":"object","description":"Motion probe"}"#;
    fn configure(params: &str) -> Result<Self, Error> {
        Ok(Self {
            mode: mode_of(params)?.0,
        })
    }
    fn step(
        &self,
        ctx: &MotionCtx<'_>,
        state: &KinematicState,
        intents: &[Intent<'_>],
        dt: f64,
    ) -> KinematicState {
        saw(format!(
            "step tick={} rate={} dt={dt} p={:?} v={:?} q={:?}",
            ctx.tick(),
            ctx.rate_hz(),
            state.position,
            state.velocity,
            state.orientation
        ));
        for intent in intents {
            saw(format!("{intent:?}"));
        }
        let p = state.position;
        match self.mode.as_str() {
            "panic" => panic!("boom step"),
            "non_unit" => KinematicState {
                orientation: [2.0, 0.0, 0.0, 0.0],
                ..*state
            },
            "ignore_refusal" => {
                let _ = ctx.terrain().height_at(p);
                KinematicState {
                    velocity: [9.0; 3],
                    ..*state
                }
            }
            _ => {
                let terrain = ctx.terrain();
                let h = terrain.height_at(p).unwrap_or(-1.0);
                let n = terrain.normal_at(p).unwrap_or([0.0; 3]);
                let up = [p[0], p[1], p[2] + 100.0];
                let los = terrain.line_of_sight(p, up).map_or(-1.0, f64::from);
                KinematicState {
                    position: [
                        p[0] + state.velocity[0] * dt,
                        p[1] + state.velocity[1] * dt,
                        p[2] + state.velocity[2] * dt,
                    ],
                    velocity: [h, n[2], los],
                    orientation: state.orientation,
                }
            }
        }
    }
}

struct SensorProbe {
    mode: String,
}

fn ownership(o: Ownership) -> &'static str {
    match o {
        Ownership::Owned => "owned",
        Ownership::Remote => "remote",
    }
}

fn contact_of<'a>(c: &TargetView<'a>, range_m: f64) -> Contact<'a> {
    Contact {
        entity: c.id,
        position: c.state.position,
        velocity: c.state.velocity,
        force: c.force,
        range_m,
    }
}

impl Sensor for SensorProbe {
    const ID: &'static str = "test.sensor.probe";
    const SCHEMA: &'static str = r#"{"type":"object","description":"Sensor probe"}"#;
    fn configure(params: &str) -> Result<Self, Error> {
        Ok(Self {
            mode: mode_of(params)?.0,
        })
    }
    fn sense(
        &self,
        ctx: &mut SenseCtx<'_>,
        observer: &TargetView<'_>,
        candidates: &[TargetView<'_>],
        out: &mut Contacts<'_>,
    ) -> Result<(), Error> {
        saw(format!(
            "sense tick={} rate={} observer={} {} {:?} {:?}",
            ctx.tick(),
            ctx.rate_hz(),
            observer.id,
            ownership(observer.ownership),
            observer.force,
            observer.state
        ));
        for c in candidates {
            saw(format!(
                "candidate {} {} {:?} {:?}",
                c.id,
                ownership(c.ownership),
                c.force,
                c.state
            ));
        }
        match self.mode.as_str() {
            "panic" => panic!("boom sense"),
            "error" => Err(Error::new("sense failed on purpose")),
            "ignore_refusal" => {
                if let Some(c) = candidates.first() {
                    let _ = out.push(&contact_of(c, 1.0));
                }
                Ok(())
            }
            "los" => {
                for c in candidates {
                    if ctx
                        .terrain()
                        .line_of_sight(observer.state.position, c.state.position)?
                    {
                        out.push(&contact_of(c, 0.0))?;
                    }
                }
                Ok(())
            }
            _ => {
                for c in candidates {
                    let draw = ctx.rng().next_f64();
                    out.push(&contact_of(c, draw))?;
                }
                Ok(())
            }
        }
    }
}

struct ConditionProbe {
    mode: String,
    key: String,
}

fn record_leaf(what: &str, ctx: &mut LeafCtx<'_>, entity: &EntityView<'_>) {
    saw(format!(
        "{what} tick={} rate={} entity={} p={:?} v={:?} force={:?}",
        ctx.tick(),
        ctx.rate_hz(),
        entity.id,
        entity.position,
        entity.velocity,
        entity.force
    ));
}

impl Condition for ConditionProbe {
    const ID: &'static str = "test.cond.probe";
    const SCHEMA: &'static str = r#"{"type":"object","description":"Condition probe"}"#;
    const METADATA: &'static str =
        r#"{"blackboard_params":[{"param":"key","type":"bool","access":"read"}]}"#;
    fn configure(params: &str) -> Result<Self, Error> {
        let (mode, key) = mode_of(params)?;
        Ok(Self { mode, key })
    }
    fn check(
        &self,
        ctx: &mut LeafCtx<'_>,
        entity: &EntityView<'_>,
        bb: &mut BlackboardAccess<'_>,
    ) -> Result<bool, Error> {
        record_leaf("check", ctx, entity);
        match self.mode.as_str() {
            "panic" => panic!("boom check"),
            "error" => Err(Error::params("check failed on purpose")),
            "write" => {
                bb.set(&self.key, Some(Value::Number(2.5)))?;
                Ok(false)
            }
            "unset" => {
                bb.set(&self.key, None)?;
                Ok(true)
            }
            "rng" => {
                saw(format!("draw {}", ctx.rng().next_u64()));
                Ok(true)
            }
            "ignore_refusal" => {
                let _ = bb.get(&self.key);
                Ok(true)
            }
            "read_twice" => {
                let first = bb.get(&self.key)?;
                let second = bb.get(&self.key)?;
                saw(format!("read twice {first:?} {second:?}"));
                Ok(first == second)
            }
            "two_refusals" => {
                let _ = bb.get(&self.key);
                let _ = bb.set(&self.key, Some(Value::Bool(true)));
                Ok(true)
            }
            _ => {
                let value = bb.get(&self.key)?;
                saw(format!("read {value:?}"));
                Ok(value == Some(Value::Bool(true)))
            }
        }
    }
}

struct ActionProbe {
    mode: String,
    key: String,
}

struct ActionProbeInstance {
    mode: String,
    key: String,
    n: u32,
}

impl Drop for ActionProbe {
    fn drop(&mut self) {
        saw("drop action");
    }
}

impl Drop for ActionProbeInstance {
    fn drop(&mut self) {
        saw("drop instance");
    }
}

impl Action for ActionProbe {
    const ID: &'static str = "test.action.probe";
    const SCHEMA: &'static str = r#"{"type":"object","description":"Action probe"}"#;
    const METADATA: &'static str =
        r#"{"blackboard_params":[{"param":"key","type":"bool","access":"write"}]}"#;
    type Instance = ActionProbeInstance;
    fn configure(params: &str) -> Result<Self, Error> {
        let (mode, key) = mode_of(params)?;
        Ok(Self { mode, key })
    }
    fn instantiate(&self, rate_hz: u32) -> Result<ActionProbeInstance, Error> {
        if self.mode == "bad_rate" {
            return Err(Error::params(format!("rate {rate_hz} refused")));
        }
        Ok(ActionProbeInstance {
            mode: self.mode.clone(),
            key: self.key.clone(),
            n: 0,
        })
    }
}

impl ActionInstance for ActionProbeInstance {
    fn tick(
        &mut self,
        ctx: &mut LeafCtx<'_>,
        entity: &EntityView<'_>,
        bb: &mut BlackboardAccess<'_>,
        out: &mut Intents<'_>,
    ) -> Result<Status, Error> {
        record_leaf("tick", ctx, entity);
        match self.mode.as_str() {
            "error" => Err(Error::new("tick failed on purpose")),
            "panic_tick" => {
                out.push(Intent::SetSpeed(1.0))?;
                panic!("boom tick")
            }
            "custom" => {
                out.push(Intent::Custom {
                    kind: "test.event.ping",
                    payload_json: "{}",
                })?;
                bb.set(&self.key, Some(Value::Bool(true)))?;
                Ok(Status::Success)
            }
            "ignore_refusal" => {
                let _ = out.push(Intent::Hold);
                Ok(Status::Success)
            }
            _ => {
                self.n += 1;
                if self.n <= 2 {
                    out.push(Intent::SetSpeed(f64::from(self.n)))?;
                    Ok(Status::Running)
                } else {
                    self.n = 0;
                    Ok(Status::Success)
                }
            }
        }
    }
    fn halt(&mut self) {
        assert!(self.mode != "panic_halt", "boom halt");
        saw("halt");
        self.n = 0;
    }
}

export_plugin! {
    id: "test.plugin.kinds_calls",
    name: "kinds calls",
    version: "0.1.0",
    motion_models: [MotionProbe],
    sensors: [SensorProbe],
    conditions: [ConditionProbe],
    actions: [ActionProbe],
}

// ---------------------------------------------------------------- calling it

fn descriptor() -> &'static UmbraPluginDescriptor {
    let host = Box::leak(Box::new(host_api()));
    // SAFETY: `host` is a leaked, well-formed host API (process lifetime).
    let ptr = unsafe { umbra_plugin_entry(host) };
    assert!(!ptr.is_null(), "entry returned null");
    // SAFETY: the SDK's descriptor is a process-lifetime static.
    unsafe { &*ptr }
}

fn motion_vt() -> &'static UmbraMotionVTable {
    vtable(capability(
        descriptor(),
        "test.motion.probe",
        UMBRA_CAPABILITY_MOTION_MODEL,
    ))
}

fn sensor_vt() -> &'static UmbraSensorVTable {
    vtable(capability(
        descriptor(),
        "test.sensor.probe",
        UMBRA_CAPABILITY_SENSOR,
    ))
}

fn condition_vt() -> &'static UmbraConditionVTable {
    vtable(capability(
        descriptor(),
        "test.cond.probe",
        UMBRA_CAPABILITY_CONDITION,
    ))
}

fn action_vt() -> &'static UmbraActionVTable {
    vtable(capability(
        descriptor(),
        "test.action.probe",
        UMBRA_CAPABILITY_ACTION,
    ))
}

fn params(mode: &str, key: &str) -> String {
    format!(r#"{{"mode":"{mode}","key":"{key}"}}"#)
}

const fn minor(abi_version: u32) -> u32 {
    abi_version & 0xFFFF
}

/// Asserts that every struct the SDK wrote carried the ABI 0.2 kinds' minor and its own size.
fn assert_written_in_minor_2(sizes: &[(&str, usize)]) {
    let written = headers();
    assert!(!written.is_empty(), "the call wrote no struct");
    for h in &written {
        assert_eq!(
            (h.abi_version >> 16, minor(h.abi_version)),
            (UMBRA_ABI_VERSION_MAJOR, 2),
            "{h:?}"
        );
        let want = sizes
            .iter()
            .find(|(what, _)| *what == h.what)
            .unwrap_or_else(|| panic!("unexpected struct {h:?}"))
            .1;
        assert_eq!(h.struct_size as usize, want, "{h:?}");
    }
}

// ---------------------------------------------------------------- motion

fn motion(mode: &str) -> *mut umbra_sdk::abi::UmbraMotionModel {
    let (code, model, err) = motion_configure(motion_vt(), &params(mode, ""));
    assert_eq!((code, err), (UMBRA_OK, None));
    assert!(!model.is_null());
    model
}

#[test]
fn given_a_motion_step_when_called_then_the_trait_sees_the_call_and_its_result_is_written_to_out() {
    clear();
    let model = motion("echo");
    let state = kinematic([6_378_000.0, 1000.0, 50.0], [3.0, 0.0, -6.0]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 12);

    let (code, out, err) = step(motion_vt(), model, &call);

    assert_eq!((code, err), (UMBRA_OK, None));
    let dt = 1.0 / 30.0;
    assert_eq!(
        [out.position.x, out.position.y, out.position.z],
        [6_378_000.0 + 3.0 * dt, 1000.0, 50.0 - 6.0 * dt]
    );
    let p = [6_378_000.0, 1000.0, 50.0];
    assert_eq!(
        [out.velocity.x, out.velocity.y, out.velocity.z],
        [
            height_of(p),
            NORMAL[2],
            f64::from(u8::from(visible(p, [p[0], p[1], p[2] + 100.0])))
        ]
    );
    assert_eq!(
        (out.abi_version, out.struct_size),
        (state.abi_version, state.struct_size),
        "the header of `out` is the host's"
    );
    assert_eq!(
        seen(),
        [format!(
            "step tick=12 rate=30 dt={dt} p={p:?} v={:?} q={:?}",
            [3.0, 0.0, -6.0],
            [1.0, 0.0, 0.0, 0.0]
        )]
    );
}

/// The intents of the tick reach the trait converted, in order (one of each kind).
#[test]
fn given_host_intents_when_stepped_then_every_kind_reaches_the_trait_in_order() {
    clear();
    let model = motion("echo");
    let state = kinematic([6_378_000.0, 0.0, 0.0], [0.0; 3]);
    let terrain = terrain();
    let mut all = vec![
        raw_intent(UMBRA_INTENT_MOVE_TO),
        raw_intent(UMBRA_INTENT_FOLLOW_ROUTE),
        raw_intent(UMBRA_INTENT_SET_SPEED),
        raw_intent(UMBRA_INTENT_SET_HEADING),
        raw_intent(UMBRA_INTENT_HOLD),
        raw_intent(UMBRA_INTENT_FIRE),
        raw_intent(UMBRA_INTENT_SPAWN),
        raw_intent(UMBRA_INTENT_DESPAWN),
        raw_intent(UMBRA_INTENT_CUSTOM),
    ];
    all[0].position = kinds_harness::v3([1.0, 2.0, 3.0]);
    all[2].value = 4.5;
    all[3].value = 0.25;
    all[5].entity = 7;
    all[5].id = kinds_harness::s("test.weapon.gun");
    all[6].id = kinds_harness::s("test.template.unit");
    all[6].position = kinds_harness::v3([9.0, 8.0, 7.0]);
    all[6].value = 1.5;
    all[8].id = kinds_harness::s("test.component.note");
    all[8].payload = kinds_harness::s("{\"n\":1}");
    let call = motion_step(&state, &all, &terrain, 1);

    let (code, _, err) = step(motion_vt(), model, &call);

    assert_eq!((code, err), (UMBRA_OK, None));
    let want = [
        Intent::MoveTo([1.0, 2.0, 3.0]),
        Intent::FollowRoute,
        Intent::SetSpeed(4.5),
        Intent::SetHeading(0.25),
        Intent::Hold,
        Intent::Fire {
            target: 7,
            weapon: "test.weapon.gun",
        },
        Intent::Spawn {
            template: "test.template.unit",
            position: [9.0, 8.0, 7.0],
            heading: 1.5,
        },
        Intent::Despawn,
        Intent::Custom {
            kind: "test.component.note",
            payload_json: "{\"n\":1}",
        },
    ]
    .map(|i| format!("{i:?}"));
    assert_eq!(seen()[1..], want);
}

/// ADR 0050: each terrain query goes to the service of the call with its token and the point.
#[test]
fn given_the_terrain_wrappers_when_queried_then_each_forwards_the_calls_token_and_points() {
    clear();
    let model = motion("echo");
    let p = [6_378_000.0, 500.0, 20.0];
    let state = kinematic(p, [0.0; 3]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 1);

    let (code, _, _) = step(motion_vt(), model, &call);

    assert_eq!(code, UMBRA_OK);
    let calls: Vec<_> = services()
        .into_iter()
        .map(|c| (c.ctx, c.call, c.points))
        .collect();
    assert_eq!(
        calls,
        [
            (TERRAIN_CTX, "height_at", vec![p]),
            (TERRAIN_CTX, "normal_at", vec![p]),
            (
                TERRAIN_CTX,
                "line_of_sight",
                vec![p, [p[0], p[1], p[2] + 100.0]]
            ),
        ]
    );
}

#[test]
fn given_a_terrain_refusal_the_model_ignores_when_stepped_then_the_hosts_code_and_out_untouched() {
    clear();
    let model = motion("ignore_refusal");
    let state = kinematic([6_378_000.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 1);
    refuse("height_at", UMBRA_ERR_STATE);

    let (code, out, err) = step(motion_vt(), model, &call);

    assert_eq!(code, UMBRA_ERR_STATE);
    assert!(err.expect("a message").contains("height_at"));
    assert_eq!(
        [out.velocity.x, out.velocity.y, out.velocity.z],
        [1.0, 2.0, 3.0],
        "the result of a refused call is not written"
    );
}

/// A line-of-sight answer other than 0 or 1 is malformed host output.
#[test]
fn given_a_line_of_sight_answer_that_is_not_0_or_1_when_stepped_then_invalid_argument() {
    clear();
    let model = motion("echo");
    let state = kinematic([6_378_000.0, 0.0, 0.0], [0.0; 3]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 1);
    los_byte(2);

    let (code, _, err) = step(motion_vt(), model, &call);

    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert!(err.expect("a message").contains("line_of_sight"));
}

#[test]
fn given_a_panic_in_step_when_stepped_then_umbra_err_panic_with_its_message_and_out_untouched() {
    clear();
    let model = motion("panic");
    let state = kinematic([6_378_000.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 1);

    let (code, out, err) = step(motion_vt(), model, &call);

    assert_eq!(code, UMBRA_ERR_PANIC);
    assert!(err.expect("a message").contains("boom step"));
    assert_eq!(out.position.x, 6_378_000.0);
}

/// The SDK passes the model's result on; whether it is valid is the host's decision (ADR 0050).
#[test]
fn given_a_non_unit_result_when_stepped_then_it_is_written_for_the_host_to_judge() {
    clear();
    let model = motion("non_unit");
    let state = kinematic([6_378_000.0, 0.0, 0.0], [0.0; 3]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 1);

    let (code, out, _) = step(motion_vt(), model, &call);

    assert_eq!(code, UMBRA_OK);
    assert_eq!(out.orientation.w, 2.0);
}

#[test]
fn given_failing_configures_when_called_then_code_and_message() {
    clear();
    let vt = motion_vt();

    let (code, model, err) = motion_configure(vt, &params("bad", ""));
    assert_eq!((code, model.is_null()), (UMBRA_ERR_PARAMS, true));
    assert_eq!(err.as_deref(), Some("unknown mode bad"));

    let (code, model, err) = motion_configure(vt, &params("panic_configure", ""));
    assert_eq!((code, model.is_null()), (UMBRA_ERR_PANIC, true));
    assert!(err.expect("a message").contains("boom configure"));

    let bad = [0xFF_u8, 0xFE];
    let err_sink = kinds_harness::err_sink();
    let mut out = std::ptr::null_mut();
    let f = vt.configure.expect("configure");
    // SAFETY: a live 2-byte array that is not UTF-8; the SDK must refuse it before parsing.
    let code = unsafe {
        f(
            UmbraStr {
                ptr: bad.as_ptr(),
                len: 2,
            },
            &raw mut out,
            &raw const err_sink,
        )
    };
    assert_eq!((code, out.is_null()), (UMBRA_ERR_INVALID_ARGUMENT, true));
}

/// R-M2: malformed call records are refused before the trait runs.
#[test]
fn given_malformed_step_records_when_stepped_then_invalid_argument_before_the_trait() {
    let model = motion("echo");
    let state = kinematic([6_378_000.0, 0.0, 0.0], [0.0; 3]);
    let good_terrain = terrain();
    let mut old_state = kinematic([6_378_000.0, 0.0, 0.0], [0.0; 3]);
    old_state.abi_version = UMBRA_ABI_VERSION_MAJOR << 16 | 1; // defined in minor 2
    let mut short_intent = raw_intent(UMBRA_INTENT_HOLD);
    short_intent.struct_size -= 8;
    let mut unknown_intent = raw_intent(UMBRA_INTENT_HOLD);
    unknown_intent.kind = 99;
    let mut no_height = terrain();
    no_height.height_at = None;

    let mut cases: Vec<(&str, umbra_sdk::abi::UmbraMotionStep)> = Vec::new();
    cases.push(("old state", motion_step(&old_state, &[], &good_terrain, 1)));
    let short = [short_intent];
    cases.push((
        "short intent",
        motion_step(&state, &short, &good_terrain, 1),
    ));
    let unknown = [unknown_intent];
    cases.push((
        "unknown intent",
        motion_step(&state, &unknown, &good_terrain, 1),
    ));
    cases.push((
        "terrain without height_at",
        motion_step(&state, &[], &no_height, 1),
    ));
    let mut null_state = motion_step(&state, &[], &good_terrain, 1);
    null_state.state = std::ptr::null();
    let mut null_terrain = motion_step(&state, &[], &good_terrain, 1);
    null_terrain.terrain = std::ptr::null();
    let mut zero_rate = motion_step(&state, &[], &good_terrain, 1);
    zero_rate.rate_hz = 0;
    let mut null_intents = motion_step(&state, &[], &good_terrain, 1);
    null_intents.intent_count = 2;
    cases.push(("null terrain", null_terrain));
    cases.push(("zero rate", zero_rate));
    cases.push(("null intents with a count", null_intents));
    let mut nan_dt = motion_step(&state, &[], &good_terrain, 1);
    nan_dt.dt = f64::NAN;
    cases.push(("NaN dt", nan_dt));
    let mut inf_dt = motion_step(&state, &[], &good_terrain, 1);
    inf_dt.dt = f64::INFINITY;
    cases.push(("infinite dt", inf_dt));
    // A newer minor may grow a struct, but array elements need the SDK's own stride.
    let mut newer_intent = raw_intent(UMBRA_INTENT_HOLD);
    newer_intent.abi_version = UMBRA_ABI_VERSION_MAJOR << 16 | (UMBRA_ABI_VERSION_MINOR + 1);
    newer_intent.struct_size += 8;
    let newer = [newer_intent];
    cases.push((
        "newer, larger intent",
        motion_step(&state, &newer, &good_terrain, 1),
    ));

    for (what, call) in &cases {
        clear();
        let (code, _, _) = step(motion_vt(), model, call);
        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{what}");
        assert_eq!(seen(), Vec::<String>::new(), "{what}: the trait ran");
    }

    clear();
    let err = kinds_harness::err_sink();
    let f = motion_vt().step.expect("step");
    let call = motion_step(&state, &[], &good_terrain, 1);
    let mut out = kinematic([0.0; 3], [0.0; 3]);
    // SAFETY: a null state (refused before reading), a null `out` (refused before writing) and a
    // null call record.
    let codes = unsafe {
        [
            f(model, &raw const null_state, &raw mut out, &raw const err),
            f(model, &raw const call, std::ptr::null_mut(), &raw const err),
            f(model, std::ptr::null(), &raw mut out, &raw const err),
        ]
    };
    assert_eq!(codes, [UMBRA_ERR_INVALID_ARGUMENT; 3]);
    assert_eq!(seen(), Vec::<String>::new());
}

/// A handle of another capability is refused, never reinterpreted.
#[test]
fn given_a_sensor_handle_when_passed_to_step_then_it_is_refused() {
    clear();
    let (code, sensor, _) = sensor_configure(sensor_vt(), &params("all", ""));
    assert_eq!(code, UMBRA_OK);
    let state = kinematic([6_378_000.0, 0.0, 0.0], [0.0; 3]);
    let terrain = terrain();
    let call = motion_step(&state, &[], &terrain, 1);

    let (code, _, err) = step(motion_vt(), sensor.cast(), &call);

    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert!(err.expect("a message").contains("not a motion model"));
    assert_eq!(seen(), Vec::<String>::new());
}

// ---------------------------------------------------------------- sensor

fn sensor(mode: &str) -> *mut umbra_sdk::abi::UmbraSensor {
    let (code, sensor, err) = sensor_configure(sensor_vt(), &params(mode, ""));
    assert_eq!((code, err), (UMBRA_OK, None));
    sensor
}

fn world() -> (
    umbra_sdk::abi::UmbraTarget,
    Vec<umbra_sdk::abi::UmbraTarget>,
) {
    (
        target(1, [10.0, 0.0, 5.0], UMBRA_OWNERSHIP_OWNED, "blue"),
        vec![
            target(2, [20.0, 0.0, 9.0], UMBRA_OWNERSHIP_OWNED, "red"),
            target(3, [30.0, 0.0, 1.0], UMBRA_OWNERSHIP_REMOTE, ""),
        ],
    )
}

#[test]
fn given_a_sense_call_then_the_targets_reach_the_trait_and_contacts_go_to_the_sink_in_minor_2() {
    clear();
    let sensor = sensor("all");
    let (observer, candidates) = world();
    let terrain = terrain();
    let call = sense_call(&observer, &candidates, &terrain, 7);

    let (code, err) = sense(sensor_vt(), sensor, &call);

    assert_eq!((code, err), (UMBRA_OK, None));
    let state = |id: u64, p: [f64; 3]| KinematicState {
        position: p,
        velocity: [f64::from(id as u32), 0.0, 0.0],
        orientation: [1.0, 0.0, 0.0, 0.0],
    };
    assert_eq!(
        seen(),
        [
            format!(
                "sense tick=7 rate=30 observer=1 owned Some(\"blue\") {:?}",
                state(1, [10.0, 0.0, 5.0])
            ),
            format!(
                "candidate 2 owned Some(\"red\") {:?}",
                state(2, [20.0, 0.0, 9.0])
            ),
            format!("candidate 3 remote None {:?}", state(3, [30.0, 0.0, 1.0])),
        ]
    );
    assert_eq!(
        draws(),
        [(HOST_CTX, RNG_TOKEN), (HOST_CTX, RNG_TOKEN)],
        "one draw per candidate from the call's stream"
    );
    let pushed: Vec<_> = contacts()
        .into_iter()
        .map(|c| (c.ctx, c.entity, c.position, c.velocity, c.force, c.range_m))
        .collect();
    let draw = |n: u64| ((RNG_TOKEN * 1000 + n) % 1000) as f64 / 1000.0;
    assert_eq!(
        pushed,
        [
            (
                CONTACT_CTX,
                2,
                [20.0, 0.0, 9.0],
                [2.0, 0.0, 0.0],
                "red".to_owned(),
                draw(0)
            ),
            (
                CONTACT_CTX,
                3,
                [30.0, 0.0, 1.0],
                [3.0, 0.0, 0.0],
                String::new(),
                draw(1)
            ),
        ]
    );
    assert_written_in_minor_2(&[("contact", size_of::<UmbraContact>())]);
}

#[test]
fn given_the_los_sensor_when_sensing_then_line_of_sight_forwards_the_terrain_token() {
    clear();
    let sensor = sensor("los");
    let (observer, candidates) = world();
    let terrain = terrain();
    let call = sense_call(&observer, &candidates, &terrain, 7);

    let (code, _) = sense(sensor_vt(), sensor, &call);

    assert_eq!(code, UMBRA_OK);
    let calls: Vec<_> = services()
        .into_iter()
        .map(|c| (c.ctx, c.call, c.points))
        .collect();
    assert_eq!(
        calls,
        [
            (
                TERRAIN_CTX,
                "line_of_sight",
                vec![[10.0, 0.0, 5.0], [20.0, 0.0, 9.0]]
            ),
            (
                TERRAIN_CTX,
                "line_of_sight",
                vec![[10.0, 0.0, 5.0], [30.0, 0.0, 1.0]]
            ),
        ]
    );
    let ids: Vec<u64> = contacts().iter().map(|c| c.entity).collect();
    assert_eq!(ids, [2], "only the visible candidate");
}

#[test]
fn given_a_contact_refusal_the_sensor_ignores_when_sensing_then_the_hosts_code() {
    clear();
    let sensor = sensor("ignore_refusal");
    let (observer, candidates) = world();
    let terrain = terrain();
    let call = sense_call(&observer, &candidates, &terrain, 7);
    refuse("contact", UMBRA_ERR_STATE);

    let (code, err) = sense(sensor_vt(), sensor, &call);

    assert_eq!(code, UMBRA_ERR_STATE);
    assert!(err.expect("a message").contains("contact"));
}

#[test]
fn given_an_error_or_a_panic_in_sense_when_sensing_then_code_and_message() {
    let (observer, candidates) = world();
    let terrain = terrain();
    let call = sense_call(&observer, &candidates, &terrain, 7);

    clear();
    let (code, err) = sense(sensor_vt(), sensor("error"), &call);
    assert_eq!(code, UMBRA_ERR_INTERNAL);
    assert_eq!(err.as_deref(), Some("sense failed on purpose"));

    clear();
    let (code, err) = sense(sensor_vt(), sensor("panic"), &call);
    assert_eq!(code, UMBRA_ERR_PANIC);
    assert!(err.expect("a message").contains("boom sense"));
}

#[test]
fn given_malformed_sense_records_when_sensing_then_invalid_argument_before_the_trait() {
    let sensor = sensor("all");
    let (observer, candidates) = world();
    let terrain = terrain();
    let mut bad_ownership = world().1;
    bad_ownership[1].ownership = 3;
    let mut zero_id = world().1;
    zero_id[1].id = 0;
    let mut short = world().1;
    short[1].struct_size -= 8;
    let bad_utf8 = [0xFF_u8, 0xFE];
    let mut bad_force = world().1;
    bad_force[0].force = UmbraStr {
        ptr: bad_utf8.as_ptr(),
        len: 2,
    };
    let mut observer_zero = world().0;
    observer_zero.id = 0;
    // A newer minor may grow a struct, but array elements need the SDK's own stride.
    let mut newer = world().1;
    newer[0].abi_version = UMBRA_ABI_VERSION_MAJOR << 16 | (UMBRA_ABI_VERSION_MINOR + 1);
    newer[0].struct_size += 8;

    let mut cases = vec![
        (
            "ownership 3",
            sense_call(&observer, &bad_ownership, &terrain, 1),
        ),
        (
            "candidate id 0",
            sense_call(&observer, &zero_id, &terrain, 1),
        ),
        (
            "short candidate",
            sense_call(&observer, &short, &terrain, 1),
        ),
        (
            "newer, larger candidate",
            sense_call(&observer, &newer, &terrain, 1),
        ),
        (
            "force not UTF-8",
            sense_call(&observer, &bad_force, &terrain, 1),
        ),
        (
            "observer id 0",
            sense_call(&observer_zero, &candidates, &terrain, 1),
        ),
    ];
    let mut null_observer = sense_call(&observer, &candidates, &terrain, 1);
    null_observer.observer = std::ptr::null();
    cases.push(("null observer", null_observer));

    for (what, call) in &cases {
        clear();
        let (code, err) = sense(sensor_vt(), sensor, call);
        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{what}");
        if *what == "candidate id 0" {
            let err = err.expect("a message");
            assert!(
                err.contains("call.candidates[1].id"),
                "the indexed path: {err}"
            );
        }
        assert_eq!(seen(), Vec::<String>::new(), "{what}: the trait ran");
    }

    clear();
    let call = sense_call(&observer, &candidates, &terrain, 1);
    let mut sink = kinds_harness::contact_sink();
    sink.push = None;
    let err = kinds_harness::err_sink();
    let f = sensor_vt().sense.expect("sense");
    // SAFETY: a sink without `push`, which the SDK must refuse before the trait runs.
    let code = unsafe { f(sensor, &raw const call, &raw const sink, &raw const err) };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(seen(), Vec::<String>::new());
}

// ---------------------------------------------------------------- condition

fn condition(mode: &str, key: &str) -> *mut umbra_sdk::abi::UmbraCondition {
    let (code, condition, err) = condition_configure(condition_vt(), &params(mode, key));
    assert_eq!((code, err), (UMBRA_OK, None));
    condition
}

#[test]
fn given_a_read_condition_when_checked_then_the_blackboard_answer_decides_and_the_token_is_forwarded()
 {
    let condition = condition("read", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    let mut answers = Vec::new();
    for stored in [Some(HostValue::Bool(1)), Some(HostValue::Bool(0)), None] {
        clear();
        if let Some(v) = stored {
            set_board("armed", v);
        }
        let (code, out, err) = check(condition_vt(), condition, &call);
        assert_eq!((code, err), (UMBRA_OK, None));
        let calls: Vec<_> = services()
            .into_iter()
            .map(|c| (c.ctx, c.call, c.key))
            .collect();
        assert_eq!(calls, [(BB_CTX, "bb_get", "armed".to_owned())]);
        assert_written_in_minor_2(&[("value_sink", size_of::<UmbraValueSink>())]);
        answers.push((out, seen()[1].clone()));
    }

    assert_eq!(
        answers,
        [
            (1, "read Some(Bool(true))".to_owned()),
            (0, "read Some(Bool(false))".to_owned()),
            (0, "read None".to_owned()),
        ]
    );
}

#[test]
fn given_every_value_type_on_the_board_when_read_then_it_converts() {
    let condition = condition("read", "k");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    let mut read = Vec::new();
    for v in [
        HostValue::Number(2.5),
        HostValue::Integer(-4),
        HostValue::Text("alpha".to_owned()),
        HostValue::Entity(9),
        HostValue::Position([1.0, 2.0, 3.0]),
    ] {
        clear();
        set_board("k", v);
        let (code, _, _) = check(condition_vt(), condition, &call);
        assert_eq!(code, UMBRA_OK);
        read.push(seen()[1].clone());
    }

    assert_eq!(
        read,
        [
            "read Some(Number(2.5))",
            "read Some(Integer(-4))",
            "read Some(String(\"alpha\"))",
            "read Some(Entity(9))",
            "read Some(Position([1.0, 2.0, 3.0]))",
        ]
    );
}

#[test]
fn given_a_write_condition_when_checked_then_set_forwards_the_key_and_a_minor_2_value() {
    clear();
    let condition = condition("write", "level");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);

    let (code, out, err) = check(condition_vt(), condition, &call);

    assert_eq!((code, out, err), (UMBRA_OK, 0, None));
    let calls: Vec<_> = services()
        .into_iter()
        .map(|c| (c.ctx, c.call, c.key, c.value))
        .collect();
    assert_eq!(
        calls,
        [(
            BB_CTX,
            "bb_set",
            "level".to_owned(),
            Some(HostValue::Number(2.5))
        )]
    );
    assert_eq!(board_value("level"), Some(HostValue::Number(2.5)));
    assert_written_in_minor_2(&[("value", size_of::<UmbraValue>())]);
}

#[test]
fn given_an_unset_condition_when_checked_then_set_passes_null() {
    clear();
    set_board("level", HostValue::Bool(1));
    let condition = condition("unset", "level");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);

    let (code, out, _) = check(condition_vt(), condition, &call);

    assert_eq!((code, out), (UMBRA_OK, 1));
    let calls: Vec<_> = services()
        .into_iter()
        .map(|c| (c.call, c.key, c.value))
        .collect();
    assert_eq!(calls, [("bb_set", "level".to_owned(), None)]);
    assert_eq!(board_value("level"), None);
}

#[test]
fn given_a_leaf_call_when_checked_then_time_entity_and_the_calls_rng_token_reach_the_trait() {
    clear();
    let condition = condition("rng", "");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 44);

    let (code, out, _) = check(condition_vt(), condition, &call);

    assert_eq!((code, out), (UMBRA_OK, 1));
    assert_eq!(
        seen(),
        [
            "check tick=44 rate=30 entity=5 p=[1.0, 2.0, 3.0] v=[4.0, 5.0, 6.0] force=Some(\"blue\")"
                .to_owned(),
            format!("draw {}", RNG_TOKEN * 1000),
        ]
    );
    assert_eq!(draws(), [(HOST_CTX, RNG_TOKEN)]);
}

#[test]
fn given_a_blackboard_refusal_the_condition_ignores_when_checked_then_the_hosts_code_and_no_result()
{
    clear();
    let condition = condition("ignore_refusal", "secret");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    refuse("bb_get", UMBRA_ERR_STATE);

    let (code, out, err) = check(condition_vt(), condition, &call);

    assert_eq!((code, out), (UMBRA_ERR_STATE, UMBRA_CONDITION_UNSET));
    assert!(err.expect("a message").contains("secret"), "names the key");
}

#[test]
fn given_an_error_or_a_panic_in_check_when_checked_then_code_message_and_no_result() {
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);

    clear();
    let (code, out, err) = check(condition_vt(), condition("error", ""), &call);
    assert_eq!((code, out), (UMBRA_ERR_PARAMS, UMBRA_CONDITION_UNSET));
    assert_eq!(err.as_deref(), Some("check failed on purpose"));

    clear();
    let (code, out, err) = check(condition_vt(), condition("panic", ""), &call);
    assert_eq!((code, out), (UMBRA_ERR_PANIC, UMBRA_CONDITION_UNSET));
    assert!(err.expect("a message").contains("boom check"));
}

/// The host's `get` must report exactly one well-formed value through the SDK's sink.
#[test]
fn given_a_misbehaving_blackboard_get_when_checked_then_invalid_argument() {
    let condition = condition("read", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);

    clear();
    get_mode(GetMode::Silent);
    let (code, out, _) = check(condition_vt(), condition, &call);
    assert_eq!(
        (code, out),
        (UMBRA_ERR_INVALID_ARGUMENT, UMBRA_CONDITION_UNSET)
    );

    clear();
    get_mode(GetMode::Malformed);
    let (code, _, _) = check(condition_vt(), condition, &call);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);

    clear();
    set_board("armed", HostValue::Bool(1));
    get_mode(GetMode::Twice);
    let (code, out, _) = check(condition_vt(), condition, &call);
    assert_eq!(
        second_report(),
        Some(UMBRA_ERR_STATE),
        "a second report is refused"
    );
    assert_eq!((code, out), (UMBRA_OK, 1), "the first report counts");
}

/// A report under another token than the read's (a forged or foreign token) is refused and does not
/// answer the read.
#[test]
fn given_a_report_under_a_foreign_token_when_checked_then_refused_and_the_read_unanswered() {
    clear();
    let condition = condition("read", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    set_board("armed", HostValue::Bool(1));
    get_mode(GetMode::ForgedToken);

    let (code, out, err) = check(condition_vt(), condition, &call);

    assert_eq!(second_report(), Some(UMBRA_ERR_STATE));
    assert_eq!(
        (code, out),
        (UMBRA_ERR_INVALID_ARGUMENT, UMBRA_CONDITION_UNSET)
    );
    assert!(err.expect("a message").contains("no value"));
}

/// Two reads of one call get different tokens; replaying the first read's token during the second
/// is refused and does not answer it.
#[test]
fn given_two_reads_in_one_call_when_the_first_token_is_replayed_then_refused_and_the_tokens_differ()
{
    clear();
    let condition = condition("read_twice", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    set_board("armed", HostValue::Bool(1));
    get_mode(GetMode::ReplayFirst);

    let (code, out, err) = check(condition_vt(), condition, &call);

    assert_eq!((code, out, err), (UMBRA_OK, 1, None));
    let tokens = sink_tokens();
    assert_eq!(tokens.len(), 2);
    assert_ne!(tokens[0], tokens[1], "each read has a fresh token");
    assert_eq!(
        second_report(),
        Some(UMBRA_ERR_STATE),
        "the replay is refused"
    );
    assert_eq!(
        seen()[1],
        "read twice Some(Bool(true)) Some(Bool(true))",
        "the second read got its own answer"
    );
}

/// The first refusal of a call decides its code and message, whatever is refused after it.
#[test]
fn given_two_refusals_with_different_codes_when_checked_then_the_first_is_reported() {
    clear();
    let condition = condition("two_refusals", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    refuse("bb_get", UMBRA_ERR_STATE);
    refuse("bb_set", UMBRA_ERR_INTERNAL);

    let (code, out, err) = check(condition_vt(), condition, &call);

    assert_eq!((code, out), (UMBRA_ERR_STATE, UMBRA_CONDITION_UNSET));
    let err = err.expect("a message");
    assert!(err.contains("get of `armed`"), "{err}");
    assert!(!err.contains("set of"), "{err}");
}

/// Blackboard reads on two threads at once (two engines, or in-process tests) each get their own
/// answer: the SDK keeps one entry per read, not one slot.
#[test]
fn given_two_reads_in_flight_on_two_threads_when_checked_then_each_gets_its_own_answer() {
    clear();
    let a = condition("read", "a") as usize;
    let b = condition("read", "b") as usize;
    let run = move |handle: usize, key: &'static str, value: u8| {
        std::thread::spawn(move || {
            reset();
            set_board(key, HostValue::Bool(value));
            get_mode(GetMode::Concurrent);
            let entity = view(5);
            let bb = blackboard();
            let call = leaf_call(&entity, &bb, 3);
            let (code, out, err) = check(condition_vt(), handle as *mut _, &call);
            (code, out, err)
        })
    };
    let (ta, tb) = (run(a, "a", 1), run(b, "b", 0));

    let answers = (ta.join().expect("thread a"), tb.join().expect("thread b"));

    assert_eq!(answers, ((UMBRA_OK, 1, None), (UMBRA_OK, 0, None)));
}

/// A read the host never answered ends with the call too: its token is refused afterwards.
#[test]
fn given_an_unanswered_read_when_its_sink_is_called_after_the_call_then_refused() {
    clear();
    let condition = condition("read", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    get_mode(GetMode::Silent);
    let (code, _, _) = check(condition_vt(), condition, &call);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    let (ctx, set) = kinds_harness::last_value_sink();

    let value = kinds_harness::raw_value(&HostValue::Bool(1), "");
    // SAFETY: the SDK's sink function, called with its token after its call ended.
    let late = unsafe { set(ctx, &raw const value) };

    assert_eq!(late, UMBRA_ERR_STATE);
}

/// A value-sink callback outside a `get` (a stale token) is refused, never trusted.
#[test]
fn given_the_sdks_value_sink_when_called_after_its_get_then_refused() {
    clear();
    let condition = condition("read", "armed");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 3);
    let (code, _, _) = check(condition_vt(), condition, &call);
    assert_eq!(code, UMBRA_OK);
    let (ctx, set) = kinds_harness::last_value_sink();

    let value = kinds_harness::raw_value(&HostValue::Bool(1), "");
    // SAFETY: the SDK's sink function, called with its old token after its call ended.
    let late = unsafe { set(ctx, &raw const value) };

    assert_eq!(late, UMBRA_ERR_STATE);
}

#[test]
fn given_malformed_leaf_calls_when_checked_then_invalid_argument_before_the_trait() {
    let condition = condition("read", "armed");
    let entity = view(5);
    let mut zero = view(5);
    zero.id = 0;
    let bb = blackboard();
    let mut no_get = blackboard();
    no_get.get = None;
    let mut cases = vec![
        ("entity id 0", leaf_call(&zero, &bb, 1)),
        ("blackboard without get", leaf_call(&entity, &no_get, 1)),
    ];
    let mut null_bb = leaf_call(&entity, &bb, 1);
    null_bb.blackboard = std::ptr::null();
    let mut null_entity = leaf_call(&entity, &bb, 1);
    null_entity.entity = std::ptr::null();
    let mut zero_rate = leaf_call(&entity, &bb, 1);
    zero_rate.rate_hz = 0;
    cases.extend([
        ("null blackboard", null_bb),
        ("null entity", null_entity),
        ("zero rate", zero_rate),
    ]);

    for (what, call) in &cases {
        clear();
        let (code, out, _) = check(condition_vt(), condition, call);
        assert_eq!(
            (code, out),
            (UMBRA_ERR_INVALID_ARGUMENT, UMBRA_CONDITION_UNSET),
            "{what}"
        );
        assert_eq!(seen(), Vec::<String>::new(), "{what}: the trait ran");
    }

    clear();
    let call = leaf_call(&entity, &bb, 1);
    let err = kinds_harness::err_sink();
    let f = condition_vt().check.expect("check");
    // SAFETY: a null `out`, which the SDK must refuse before the trait runs.
    let code = unsafe {
        f(
            condition,
            &raw const call,
            std::ptr::null_mut(),
            &raw const err,
        )
    };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(seen(), Vec::<String>::new());
}

// ---------------------------------------------------------------- action

fn action_instance(
    mode: &str,
    key: &str,
) -> (
    *mut umbra_sdk::abi::UmbraAction,
    *mut umbra_sdk::abi::UmbraActionInstance,
) {
    let (code, action, err) = action_configure(action_vt(), &params(mode, key));
    assert_eq!((code, err), (UMBRA_OK, None));
    let (code, instance, err) = instantiate(action_vt(), action);
    assert_eq!((code, err), (UMBRA_OK, None));
    (action, instance)
}

#[test]
fn given_the_count_action_when_ticked_then_statuses_and_minor_2_intents_at_item_0_and_halt_resets()
{
    clear();
    let (_, instance) = action_instance("count", "");
    let entity = view(5);
    let bb = blackboard();
    let mut rounds = Vec::new();
    for t in 1..=3 {
        let call = leaf_call(&entity, &bb, t);
        let (code, status, err) = tick(action_vt(), instance, &call);
        assert_eq!((code, err), (UMBRA_OK, None));
        rounds.push(status);
    }
    let pushed: Vec<_> = intents()
        .into_iter()
        .map(|i| (i.ctx, i.item, i.kind, i.value))
        .collect();
    assert_written_in_minor_2(&[("intent", size_of::<UmbraIntent>())]);

    assert_eq!(
        rounds,
        [
            UMBRA_STATUS_RUNNING,
            UMBRA_STATUS_RUNNING,
            UMBRA_STATUS_SUCCESS
        ]
    );
    assert_eq!(
        pushed,
        [
            (INTENT_CTX, 0, UMBRA_INTENT_SET_SPEED, 1.0),
            (INTENT_CTX, 0, UMBRA_INTENT_SET_SPEED, 2.0),
        ]
    );

    clear();
    let call = leaf_call(&entity, &bb, 4);
    let _ = tick(action_vt(), instance, &call);
    assert_eq!(halt(action_vt(), instance), UMBRA_OK);
    let _ = tick(action_vt(), instance, &call);
    let values: Vec<f64> = intents().iter().map(|i| i.value).collect();
    assert_eq!(values, [1.0, 1.0], "halt starts the count over");
    assert!(seen().contains(&"halt".to_owned()));
}

/// ADR 0050 (H2): events leave as `CUSTOM` intents; the blackboard write goes through the call's
/// service.
#[test]
fn given_the_custom_action_when_ticked_then_a_custom_intent_and_a_blackboard_write() {
    clear();
    let (_, instance) = action_instance("custom", "done");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 1);

    let (code, status, _) = tick(action_vt(), instance, &call);

    assert_eq!((code, status), (UMBRA_OK, UMBRA_STATUS_SUCCESS));
    let pushed: Vec<_> = intents()
        .into_iter()
        .map(|i| (i.kind, i.id, i.payload))
        .collect();
    assert_eq!(
        pushed,
        [(
            UMBRA_INTENT_CUSTOM,
            "test.event.ping".to_owned(),
            "{}".to_owned()
        )]
    );
    assert_eq!(board_value("done"), Some(HostValue::Bool(1)));
    assert_written_in_minor_2(&[
        ("intent", size_of::<UmbraIntent>()),
        ("value", size_of::<UmbraValue>()),
    ]);
}

#[test]
fn given_an_intent_refusal_the_action_ignores_when_ticked_then_the_hosts_code_and_no_status() {
    clear();
    let (_, instance) = action_instance("ignore_refusal", "");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 1);
    refuse("intent", UMBRA_ERR_STATE);

    let (code, status, err) = tick(action_vt(), instance, &call);

    assert_eq!((code, status), (UMBRA_ERR_STATE, UMBRA_STATUS_UNSET));
    assert!(err.expect("a message").contains("intent"));
}

#[test]
fn given_an_error_or_a_panic_in_tick_when_ticked_then_code_message_and_no_status() {
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 1);

    clear();
    let (_, instance) = action_instance("error", "");
    let (code, status, err) = tick(action_vt(), instance, &call);
    assert_eq!((code, status), (UMBRA_ERR_INTERNAL, UMBRA_STATUS_UNSET));
    assert_eq!(err.as_deref(), Some("tick failed on purpose"));

    clear();
    let (_, instance) = action_instance("panic_tick", "");
    let (code, status, err) = tick(action_vt(), instance, &call);
    assert_eq!((code, status), (UMBRA_ERR_PANIC, UMBRA_STATUS_UNSET));
    assert!(err.expect("a message").contains("boom tick"));
}

#[test]
fn given_a_panic_in_halt_when_halted_then_umbra_err_panic_and_an_error_log() {
    clear();
    let (_, instance) = action_instance("panic_halt", "");

    let code = halt(action_vt(), instance);

    assert_eq!(code, UMBRA_ERR_PANIC);
    assert!(
        logs()
            .iter()
            .any(|(level, text)| *level == UMBRA_LOG_ERROR && text.contains("boom halt")),
        "{:?}",
        logs()
    );
}

/// A tick rate of 0 is malformed, as in a call record; the trait never sees it.
#[test]
fn given_rate_0_when_instantiated_then_invalid_argument_and_no_instance() {
    clear();
    let (code, action, _) = action_configure(action_vt(), &params("count", ""));
    assert_eq!(code, UMBRA_OK);

    let (code, instance, err) = instantiate_at(action_vt(), action, 0);

    assert_eq!(
        (code, instance.is_null()),
        (UMBRA_ERR_INVALID_ARGUMENT, true)
    );
    assert!(err.expect("a message").contains("rate_hz"));
}

#[test]
fn given_a_failing_instantiate_when_called_then_code_message_and_no_instance() {
    clear();
    let (code, action, _) = action_configure(action_vt(), &params("bad_rate", ""));
    assert_eq!(code, UMBRA_OK);

    let (code, instance, err) = instantiate(action_vt(), action);

    assert_eq!((code, instance.is_null()), (UMBRA_ERR_PARAMS, true));
    assert_eq!(err.as_deref(), Some("rate 30 refused"));
}

/// Handles are tagged: an action passed as an instance is refused; destroy drops the instance, then
/// the action.
#[test]
fn given_action_handles_when_misused_then_refused_and_destroy_drops_each_once() {
    clear();
    let (action, instance) = action_instance("count", "");
    let entity = view(5);
    let bb = blackboard();
    let call = leaf_call(&entity, &bb, 1);

    let (code, _, _) = tick(action_vt(), action.cast(), &call);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(halt(action_vt(), action.cast()), UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(seen(), Vec::<String>::new());

    let vt = action_vt();
    // SAFETY: handles from `instantiate` and `configure`, destroyed once, instance first.
    unsafe {
        vt.destroy_instance.expect("destroy_instance")(instance);
        vt.destroy_action.expect("destroy_action")(action);
    }
    assert_eq!(seen(), ["drop instance", "drop action"]);
}
