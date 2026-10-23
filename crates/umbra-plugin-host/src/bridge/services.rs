//! The per-call host services of ABI 0.2 (ADR 0050, human decision H2): the terrain service of a
//! motion `step` and a sensor `sense` call (REQ-C-6, ASM-617), and the blackboard service of a
//! condition `check` and an action `tick` call (REQ-B-3, REQ-B-11, REQ-B-15, ASM-618, ASM-723).
//!
//! **Terrain.**
//! A service is a struct of a call-scoped token plus `extern "C"` entries (the sink pattern of ADR
//! 0019). The token is the image's sink token of kind [`Sink::Terrain`] in the call's generation, so
//! it names its image, its call and its kind (`tokens`). Every entry is a leaf: it catches panics,
//! resolves its token against the calling thread's top call record only and never calls into a
//! plugin. No record is `UMBRA_ERR_STATE` without any lookup (a call from another thread, or after
//! the call; ADR 0019 RR-M2). With a record, a token that is not this call's terrain token (stale,
//! of another capability's call, or of another service such as the contact sink), a null or
//! misaligned pointer, or a non-finite point records a breach on the record (the call then faults
//! the capability) and returns `UMBRA_ERR_STATE` (token) or `UMBRA_ERR_INVALID_ARGUMENT` (arguments)
//! without querying the terrain. Otherwise the answer comes from the `TerrainProvider` the core call
//! received, converted exactly as a built-in converts (`Ecef::to_geodetic`), and the out-value is
//! written only on `UMBRA_OK`. A panic of the provider (a host-side bug) answers
//! `UMBRA_ERR_INTERNAL` and is kept on the record; the bridge resumes it on the caller after the
//! plugin call ([`resume_provider_panic`]), so it surfaces as the same panic from a built-in would,
//! and the plugin is not faulted (ASM-722 b).
//!
//! **Blackboard** (ASM-618, ASM-723). The token is the image's sink token of kind
//! [`Sink::Blackboard`] in the call's generation; token rules as for terrain (no record:
//! `UMBRA_ERR_STATE` without a lookup; a token that is not this call's blackboard token: a breach and
//! `UMBRA_ERR_STATE`). The service grants exactly the keys the leaf's declared blackboard params name,
//! as resolved from its params at configure ([`Grant`]): `get` needs a `read` grant of the key, `set`
//! a `write` grant. A key that is not UTF-8, not granted or granted with the other access, a value the
//! host refuses (not finite, invalid, of another type than the grant), a null or malformed value sink,
//! or a write the node's blackboard refuses is a breach and `UMBRA_ERR_INVALID_ARGUMENT`; nothing is
//! read or written then. `get` reports through the plugin's value sink exactly once: the key's value,
//! or unset (a null value) when the key holds none or holds a value of another type than the grant
//! (which graph validation rules out); it returns the sink's own code. `set` writes the node's
//! blackboard at once, so later reads in the call see it, and logs the old value; when the call
//! faults the bridge undoes the call's writes, last first, so a faulting call leaves the blackboard
//! as it found it.

#![expect(
    unsafe_code,
    reason = "extern \"C\" terrain and blackboard service entries reading and writing plugin-passed values"
)]

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use umbra_core::{
    Access, BbType, BbValue, Blackboard, ComponentId, Ecef, PluginId, TerrainProvider, Vec3,
};
use umbra_plugin_api::{
    UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE, UMBRA_OK, UmbraBlackboard,
    UmbraCode, UmbraStr, UmbraTerrain, UmbraToken, UmbraValue, UmbraValueSink, UmbraVec3,
};

use super::marshal::{self, written_size, written_version};
use crate::host_api::{bytes, drop_payload, guarded};
use crate::log::{LogLevel, PluginLog};
pub(crate) use crate::tokens::Grant;
use crate::tokens::{self, CallRecord, Sink};

/// The terrain service of a call whose terrain token is `ctx`, written in the plugin's declared minor
/// `minor` (ABI 0.2).
pub(crate) fn terrain(ctx: UmbraToken, minor: u32) -> UmbraTerrain {
    UmbraTerrain {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraTerrain>(minor),
        ctx,
        height_at: Some(terrain_height_at),
        normal_at: Some(terrain_normal_at),
        line_of_sight: Some(terrain_line_of_sight),
    }
}

/// After a plugin call whose record held a provider panic (`panic`, taken from the record inside the
/// call): logs it at error level as the host's and resumes it on the caller, as the same panic from a
/// built-in would surface (ASM-722 b). The call lock is released and the record popped by then, so
/// nothing is poisoned and no plugin frame is crossed. `None` returns.
pub(crate) fn resume_provider_panic(
    panic: Option<Box<dyn Any + Send>>,
    log: &dyn PluginLog,
    plugin: &PluginId,
    capability: &ComponentId,
) {
    if let Some(payload) = panic {
        log.log(
            plugin,
            LogLevel::Error,
            &format!("capability {capability}: terrain provider panicked (host)"),
        );
        std::panic::resume_unwind(payload);
    }
}

/// Records `why` as the call's breach and returns `code`.
fn refuse(r: &mut CallRecord, code: UmbraCode, why: String) -> UmbraCode {
    r.breach(|| why);
    code
}

/// Whether `p` is non-null and aligned for `T` (every ABI value here is 8-byte aligned).
fn usable<T>(p: *const T) -> bool {
    !p.is_null() && p.is_aligned()
}

/// Reads the point `p` of entry `entry`, argument `what`.
///
/// # Safety
/// `p` is null or points to a `UmbraVec3` readable for the call (the plugin's promise).
unsafe fn point(p: *const UmbraVec3, entry: &str, what: &str) -> Result<Ecef, String> {
    if !usable(p) {
        return Err(format!(
            "terrain {entry}: {what} pointer is null or misaligned"
        ));
    }
    // SAFETY: non-null and aligned (checked); readable for the call (caller); every bit pattern is a
    // valid `UmbraVec3`.
    let v = unsafe { p.read() };
    if !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
        return Err(format!("terrain {entry}: {what} is not finite"));
    }
    Ok(Ecef(Vec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }))
}

/// One terrain query: checks the token against the top record (phase 1), then the arguments with
/// `read` (phase 2, a breach on failure), then answers with `answer` from the call's provider outside
/// any borrow of the record (phase 3). `out_ok` is the out-pointer's check.
fn query<A, R>(
    ctx: UmbraToken,
    entry: &str,
    out_ok: bool,
    read: impl FnOnce() -> Result<A, String>,
    answer: impl FnOnce(&dyn TerrainProvider, A) -> R,
) -> Result<R, UmbraCode> {
    let provider = tokens::with_top(|r| match r.terrain {
        Some(terrain) if r.sink(Sink::Terrain) == ctx => Ok(terrain),
        _ => Err(refuse(
            r,
            UMBRA_ERR_STATE,
            format!("terrain {entry}: stale or foreign token (not this call's terrain service)"),
        )),
    })
    .unwrap_or(Err(UMBRA_ERR_STATE))?;
    let args = read().and_then(|a| {
        if out_ok {
            Ok(a)
        } else {
            Err(format!(
                "terrain {entry}: out pointer is null or misaligned"
            ))
        }
    });
    let args = match args {
        Ok(args) => args,
        Err(why) => {
            return Err(
                tokens::with_top(|r| refuse(r, UMBRA_ERR_INVALID_ARGUMENT, why))
                    .unwrap_or(UMBRA_ERR_STATE),
            );
        }
    };
    // SAFETY: `provider` is the terrain state of the current call on this thread: the token matched
    // the top record, whose terrain state the bridge sets from a live `&&dyn TerrainProvider` on its
    // own frame (`TerrainCall::new`) for the duration of its call and clears before that frame ends.
    // `&dyn` is `Copy`, so this reads the reference; nothing is written through it.
    let provider = unsafe { *provider.provider.cast::<&dyn TerrainProvider>() };
    // A provider panic is the host's bug, not the plugin's (ASM-722 b): the plugin gets
    // `UMBRA_ERR_INTERNAL`, the payload goes on the record (the first one; later ones are dropped)
    // and the bridge resumes it on the caller after the plugin call, as from a built-in.
    match catch_unwind(AssertUnwindSafe(|| answer(provider, args))) {
        Ok(answer) => Ok(answer),
        Err(payload) => {
            // The first panic of the call is kept; a later one is dropped here.
            let left = tokens::with_top(move |r| {
                if r.provider_panic.is_none() {
                    r.provider_panic = Some(payload);
                    None
                } else {
                    Some(payload)
                }
            });
            if let Some(Some(payload)) = left {
                drop_payload(payload);
            }
            Err(UMBRA_ERR_INTERNAL)
        }
    }
}

/// `height_at` (ADR 0050): the terrain height (m above the ellipsoid) under the ECEF point `*ecef`.
unsafe extern "C" fn terrain_height_at(
    ctx: UmbraToken,
    ecef: *const UmbraVec3,
    out_m: *mut f64,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let answered = query(
            ctx,
            "height_at",
            usable(out_m),
            // SAFETY: the plugin passes the point readable for this call (ADR 0050).
            || unsafe { point(ecef, "height_at", "point") },
            |t, at| t.height_at(at.to_geodetic()).0,
        );
        match answered {
            Ok(height) => {
                // SAFETY: `query` checked `out_m` non-null and aligned; the plugin passes it
                // writable for this call.
                unsafe { out_m.write(height) };
                UMBRA_OK
            }
            Err(code) => code,
        }
    })
}

/// `normal_at` (ADR 0050): the unit terrain normal (east, north, up) under the ECEF point `*ecef`.
unsafe extern "C" fn terrain_normal_at(
    ctx: UmbraToken,
    ecef: *const UmbraVec3,
    out_enu: *mut UmbraVec3,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let answered = query(
            ctx,
            "normal_at",
            usable(out_enu),
            // SAFETY: the plugin passes the point readable for this call (ADR 0050).
            || unsafe { point(ecef, "normal_at", "point") },
            |t, at| t.surface_normal_at(at.to_geodetic()),
        );
        match answered {
            Ok(n) => {
                // SAFETY: `query` checked `out_enu` non-null and aligned; the plugin passes it
                // writable for this call.
                unsafe {
                    out_enu.write(UmbraVec3 {
                        x: n.east,
                        y: n.north,
                        z: n.up,
                    });
                }
                UMBRA_OK
            }
            Err(code) => code,
        }
    })
}

/// `line_of_sight` (ADR 0050): 1 if the terrain does not block the line from `*from` to `*to` (ECEF),
/// else 0.
unsafe extern "C" fn terrain_line_of_sight(
    ctx: UmbraToken,
    from: *const UmbraVec3,
    to: *const UmbraVec3,
    out: *mut u8,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let answered = query(
            ctx,
            "line_of_sight",
            !out.is_null(),
            || {
                // SAFETY: the plugin passes both points readable for this call (ADR 0050).
                let a = unsafe { point(from, "line_of_sight", "from") }?;
                // SAFETY: as above.
                let b = unsafe { point(to, "line_of_sight", "to") }?;
                Ok((a, b))
            },
            |t, (a, b)| t.line_of_sight(a, b),
        );
        match answered {
            Ok(visible) => {
                // SAFETY: `query` checked `out` non-null (a `u8` has no alignment); the plugin
                // passes it writable for this call.
                unsafe { out.write(u8::from(visible)) };
                UMBRA_OK
            }
            Err(code) => code,
        }
    })
}

/// The blackboard service of a call whose blackboard token is `ctx`, written in the plugin's declared
/// minor `minor` (ABI 0.2).
pub(crate) fn blackboard(ctx: UmbraToken, minor: u32) -> UmbraBlackboard {
    UmbraBlackboard {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraBlackboard>(minor),
        ctx,
        get: Some(blackboard_get),
        set: Some(blackboard_set),
    }
}

/// The text of an access, as metadata writes it.
fn access_name(access: Access) -> &'static str {
    match access {
        Access::Read => "read",
        Access::Write => "write",
        // `Access` is non-exhaustive; metadata only declares the two above (ASM-709).
        _ => "other",
    }
}

/// The grant of `key` with `access` among `grants`, or why there is none (ASM-618).
fn granted<'g>(grants: &'g [Grant], key: &str, access: Access) -> Result<&'g Grant, String> {
    let mut named = grants.iter().filter(|g| &*g.key == key).peekable();
    if named.peek().is_none() {
        return Err(format!(
            "key {key:?} is not one of the leaf's declared blackboard keys"
        ));
    }
    let mut other = None;
    for grant in named {
        if grant.access == access {
            return Ok(grant);
        }
        other = Some(grant.access);
    }
    Err(format!(
        "key {key:?} is declared with access {}, not {}",
        other.map_or("other", access_name),
        access_name(access)
    ))
}

/// The state of the current blackboard call: the node's blackboard, the leaf's grants and the
/// plugin's declared minor.
#[derive(Clone, Copy)]
struct Granted {
    board: *mut Blackboard,
    grants: *const [Grant],
    minor: u32,
}

/// Phase 1 of both entries: the token against the top record. `Err` is the code to return (the
/// breach, if any, is recorded).
fn board_of(ctx: UmbraToken, entry: &str) -> Result<Granted, UmbraCode> {
    tokens::with_top(|r| {
        let ours = r.sink(Sink::Blackboard) == ctx;
        match r.leaf.as_ref().filter(|_| ours) {
            Some(leaf) => match (leaf.board, leaf.grants) {
                (Some(board), Some(grants)) => Ok(Granted {
                    board,
                    grants,
                    minor: r.minor,
                }),
                _ => Err(UMBRA_ERR_STATE),
            },
            None => Err(refuse(
                r,
                UMBRA_ERR_STATE,
                format!(
                    "blackboard {entry}: stale or foreign token (not this call's blackboard service)"
                ),
            )),
        }
    })
    .unwrap_or(Err(UMBRA_ERR_STATE))
}

/// Records `why` (prefixed with the entry) as a breach and returns `UMBRA_ERR_INVALID_ARGUMENT`.
fn invalid(entry: &str, why: &str) -> UmbraCode {
    tokens::with_top(|r| {
        refuse(
            r,
            UMBRA_ERR_INVALID_ARGUMENT,
            format!("blackboard {entry}: {why}"),
        )
    })
    .unwrap_or(UMBRA_ERR_STATE)
}

/// Borrows a key text for the call (no allocation; only an error message allocates, review m5). The
/// result borrows `key`, the entry's own argument, so the compiler keeps it inside the entry.
///
/// # Safety
/// As [`bytes`]: the bytes `key` points to are valid and unchanged while `key` is borrowed (the
/// plugin's promise for the duration of the service call that passed it).
unsafe fn key_text(key: &UmbraStr) -> Result<&str, String> {
    // SAFETY: forwarded caller contract; the returned slice's lifetime is that of the borrow of `key`.
    let b =
        unsafe { bytes(*key) }.ok_or_else(|| "key is a null pointer with a length".to_owned())?;
    std::str::from_utf8(b).map_err(|e| format!("key is not valid UTF-8 ({e})"))
}

/// What `get` reports for `key`: its value when it has the grant's type `ty`, else unset.
fn reported(board: &Blackboard, key: &str, ty: BbType) -> Option<BbValue> {
    board.get(key).filter(|v| v.ty() == ty).cloned()
}

/// `get` (ADR 0050): reports the value of the granted key `key` through the plugin's `sink`, exactly
/// once (see the module docs); returns the sink's code.
unsafe extern "C" fn blackboard_get(
    ctx: UmbraToken,
    key: UmbraStr,
    sink: *const UmbraValueSink,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let board = match board_of(ctx, "get") {
            Ok(board) => board,
            Err(code) => return code,
        };
        let minor = board.minor;
        // SAFETY: the plugin passes the key borrowed for this call (ADR 0050); the borrowed `&str`
        // does not outlive this entry.
        let key = match unsafe { key_text(&key) } {
            Ok(key) => key,
            Err(why) => return invalid("get", &why),
        };
        // SAFETY: `grants` points into the configured leaf, which outlives the call (the bridge set
        // it from a live `&[Grant]` and takes it off the record before the call returns); read only.
        let grants = unsafe { &*board.grants };
        let ty = match granted(grants, key, Access::Read) {
            Ok(grant) => grant.ty,
            Err(why) => return invalid("get", &why),
        };
        // SAFETY: the plugin passes the sink (null is refused) valid for this call.
        let (sink_ctx, set) = match unsafe { marshal::value_sink_within(sink, minor) } {
            Ok(sink) => sink,
            Err(why) => return invalid("get", &format!("key {key:?}: {why}")),
        };
        // SAFETY: `board` points at the node's blackboard: the leaf bridge derives it inside its call
        // closure from that closure's own capture of the `&mut Blackboard` (so no later reborrow of
        // the source invalidates it before the plugin call returns), does not touch the blackboard
        // while the plugin runs, and takes the pointer off the record (on this thread) before it uses
        // the blackboard again. The shared borrow ends here (the
        // value is cloned), before the plugin's sink runs and might call back into this service.
        let value = reported(unsafe { &*board.board }, key, ty);
        let raw = value.as_ref().map(|v| marshal::value_out(v, minor));
        let ptr = raw
            .as_ref()
            .map_or(std::ptr::null(), std::ptr::from_ref::<UmbraValue>);
        // SAFETY: `set` is the plugin's own sink entry, called on the thread of the plugin's call with
        // the plugin's token; `raw` (and the string it borrows from `value`) lives on this frame
        // across the call. No record borrow is held. A plugin cannot unwind.
        unsafe { set(sink_ctx, ptr) }
    })
}

/// `set` (ADR 0050): writes `*value` to the granted key `key`, or unsets it when `value` is null (see
/// the module docs).
unsafe extern "C" fn blackboard_set(
    ctx: UmbraToken,
    key: UmbraStr,
    value: *const UmbraValue,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let board = match board_of(ctx, "set") {
            Ok(board) => board,
            Err(code) => return code,
        };
        let minor = board.minor;
        // SAFETY: the plugin passes the key borrowed for this call (ADR 0050); the borrowed `&str`
        // does not outlive this entry.
        let key = match unsafe { key_text(&key) } {
            Ok(key) => key,
            Err(why) => return invalid("set", &why),
        };
        // SAFETY: as in `blackboard_get`.
        let grants = unsafe { &*board.grants };
        let grant = match granted(grants, key, Access::Write) {
            Ok(grant) => grant,
            Err(why) => return invalid("set", &why),
        };
        // SAFETY: the plugin passes the value (or null) and its string borrowed for this call.
        let value = match unsafe { marshal::value_within(value, minor) } {
            Ok(value) => value,
            Err(why) => return invalid("set", &format!("key {key:?}: {why}")),
        };
        if let Some(v) = &value
            && v.ty() != grant.ty
        {
            return invalid(
                "set",
                &format!(
                    "key {key:?} has the declared type {}, not {}",
                    grant.ty,
                    v.ty()
                ),
            );
        }
        // SAFETY: as in `blackboard_get`; the exclusive borrow lives only for this write, during
        // which nothing else reaches the blackboard (no plugin code runs).
        let bb = unsafe { &mut *board.board };
        let old = bb.get(key).cloned();
        let written = match value {
            Some(v) => bb.set(key, v),
            None => bb.unset(key),
        };
        match written {
            Ok(()) => tokens::with_top(|r| {
                if let Some(leaf) = r.leaf.as_mut() {
                    leaf.undo.push((std::sync::Arc::clone(&grant.key), old));
                }
                UMBRA_OK
            })
            .unwrap_or(UMBRA_ERR_STATE),
            Err(e) => invalid("set", &e.to_string()),
        }
    })
}

/// Undoes the writes of a faulted call on `bb`, last first (ASM-723): each key gets back the value it
/// had before the call. Each restore is a write the blackboard accepted during the call (declared,
/// writable, of the key's type), so it cannot fail.
pub(crate) fn undo(bb: &mut Blackboard, log: &mut Vec<(std::sync::Arc<str>, Option<BbValue>)>) {
    for (key, old) in log.drain(..).rev() {
        let _restored = match old {
            Some(v) => bb.set(&key, v),
            None => bb.unset(&key),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::log::TracingLog;
    use crate::tokens::{ActiveCall, TerrainCall};
    use umbra_core::{Enu, Geodetic, Meters, PluginId};

    /// A terrain whose height is the longitude in radians times 1000 and whose normal is tilted.
    struct Probe;

    impl TerrainProvider for Probe {
        fn height_at(&self, at: Geodetic) -> Meters {
            Meters(1000.0 * at.lon().0)
        }
        fn surface_normal_at(&self, _: Geodetic) -> Enu {
            Enu {
                east: 0.6,
                north: 0.0,
                up: 0.8,
            }
        }
        fn line_of_sight(&self, _: Ecef, to: Ecef) -> bool {
            to.0.z > 0.0
        }
    }

    fn call(image: UmbraToken, generation: u32) -> ActiveCall {
        ActiveCall::push(CallRecord::new(
            image,
            generation,
            PluginId::new("test.plugin.terrain").unwrap_or_else(|e| panic!("{e}")),
            Arc::new(TracingLog),
        ))
    }

    fn at(lon_deg: f64) -> UmbraVec3 {
        let p = Geodetic::from_degrees(52.0, lon_deg, 10.0)
            .unwrap_or_else(|e| panic!("{e}"))
            .to_ecef()
            .0;
        UmbraVec3 {
            x: p.x,
            y: p.y,
            z: p.z,
        }
    }

    #[test]
    fn the_entries_answer_from_the_calls_provider_and_misuse_is_a_breach() {
        let image = tokens::make(300, tokens::HOST_CTX_INDEX);
        let active = call(image, 7);
        let provider: &dyn TerrainProvider = &Probe;
        tokens::with_top(|r| r.terrain = Some(TerrainCall::new(&provider)));
        let svc = terrain(tokens::sink_token(image, 7, Sink::Terrain), 2);
        let (height, normal, los) = (
            svc.height_at.unwrap_or_else(|| panic!("height_at")),
            svc.normal_at.unwrap_or_else(|| panic!("normal_at")),
            svc.line_of_sight.unwrap_or_else(|| panic!("line_of_sight")),
        );
        let p = at(4.0);
        let mut h = -1.0;
        let mut n = at(0.0);
        let mut v = 7_u8;
        let nan = UmbraVec3 {
            x: f64::NAN,
            y: 0.0,
            z: 0.0,
        };
        // SAFETY: valid locals for every call; the record is on this thread.
        unsafe {
            assert_eq!(height(svc.ctx, &raw const p, &raw mut h), UMBRA_OK);
            assert_eq!(normal(svc.ctx, &raw const p, &raw mut n), UMBRA_OK);
            assert_eq!(
                los(svc.ctx, &raw const p, &raw const p, &raw mut v),
                UMBRA_OK
            );
        }
        let want = Ecef(Vec3 {
            x: p.x,
            y: p.y,
            z: p.z,
        })
        .to_geodetic();
        assert_eq!(h, Probe.height_at(want).0, "the provider's answer, exactly");
        assert_eq!((n.x, n.y, n.z), (0.6, 0.0, 0.8));
        assert_eq!(v, 1);
        // SAFETY: as above; each must be refused before any read past its check.
        unsafe {
            let other = tokens::sink_token(image, 7, Sink::Contact);
            assert_eq!(height(other, &raw const p, &raw mut h), UMBRA_ERR_STATE);
            assert_eq!(
                height(svc.ctx, &raw const nan, &raw mut h),
                UMBRA_ERR_INVALID_ARGUMENT
            );
            assert_eq!(
                height(svc.ctx, std::ptr::null(), &raw mut h),
                UMBRA_ERR_INVALID_ARGUMENT
            );
            assert_eq!(
                normal(svc.ctx, &raw const p, std::ptr::null_mut()),
                UMBRA_ERR_INVALID_ARGUMENT
            );
            assert_eq!(
                los(svc.ctx, &raw const p, &raw const nan, &raw mut v),
                UMBRA_ERR_INVALID_ARGUMENT
            );
        }
        tokens::with_top(|r| r.terrain = None);
        // SAFETY: as above.
        let after = unsafe { height(svc.ctx, &raw const p, &raw mut h) };
        assert_eq!(after, UMBRA_ERR_STATE, "the state is gone: stale");
        let finished = active.finish();
        assert!(
            finished
                .breach
                .is_some_and(|b| b.contains("stale or foreign")),
            "the first misuse is the breach"
        );
        // SAFETY: without a record the entry refuses before reading anything.
        let off_call = unsafe { height(svc.ctx, &raw const p, &raw mut h) };
        assert_eq!(off_call, UMBRA_ERR_STATE);
        assert_eq!(
            h,
            Probe.height_at(want).0,
            "no refused query wrote its out-value"
        );
    }

    /// Runs `query` against a fresh call with the probe terrain; returns its code and the call's
    /// breach.
    fn misuse(query: impl FnOnce(&UmbraTerrain) -> UmbraCode) -> (UmbraCode, Option<String>) {
        let image = tokens::make(301, tokens::HOST_CTX_INDEX);
        let active = call(image, 9);
        let provider: &dyn TerrainProvider = &Probe;
        tokens::with_top(|r| r.terrain = Some(TerrainCall::new(&provider)));
        let svc = terrain(tokens::sink_token(image, 9, Sink::Terrain), 2);
        let code = query(&svc);
        tokens::with_top(|r| r.terrain = None);
        (code, active.finish().breach)
    }

    /// Review round 1 (Minor 1, 2): every coordinate of every point is checked, and a null out
    /// pointer is refused for each entry; each refusal is `UMBRA_ERR_INVALID_ARGUMENT` and a breach
    /// naming the problem.
    #[test]
    fn each_coordinate_and_each_out_pointer_is_checked_in_every_entry() {
        type Query = Box<dyn Fn(&UmbraTerrain) -> UmbraCode>;
        let p = at(4.0);
        let with = |f: fn(&mut UmbraVec3)| {
            let mut v = p;
            f(&mut v);
            v
        };
        let nan_y = with(|v| v.y = f64::NAN);
        let inf_z = with(|v| v.z = f64::INFINITY);
        let inf_y = with(|v| v.y = f64::NEG_INFINITY);
        let rows: [(&str, &str, Query); 5] = [
            (
                "height_at, NaN y",
                "point is not finite",
                Box::new(move |svc| {
                    let mut h = 0.0;
                    let f = svc.height_at.unwrap_or_else(|| panic!("height_at"));
                    // SAFETY: valid locals; the record is on this thread.
                    unsafe { f(svc.ctx, &raw const nan_y, &raw mut h) }
                }),
            ),
            (
                "normal_at, infinite z",
                "point is not finite",
                Box::new(move |svc| {
                    let mut n = p;
                    let f = svc.normal_at.unwrap_or_else(|| panic!("normal_at"));
                    // SAFETY: as above.
                    unsafe { f(svc.ctx, &raw const inf_z, &raw mut n) }
                }),
            ),
            (
                "line_of_sight, non-finite from",
                "from is not finite",
                Box::new(move |svc| {
                    let mut v = 0_u8;
                    let f = svc.line_of_sight.unwrap_or_else(|| panic!("line_of_sight"));
                    // SAFETY: as above.
                    unsafe { f(svc.ctx, &raw const inf_y, &raw const p, &raw mut v) }
                }),
            ),
            (
                "height_at, null out",
                "out pointer",
                Box::new(move |svc| {
                    let f = svc.height_at.unwrap_or_else(|| panic!("height_at"));
                    // SAFETY: as above; the null out pointer must be refused before any write.
                    unsafe { f(svc.ctx, &raw const p, std::ptr::null_mut()) }
                }),
            ),
            (
                "line_of_sight, null out",
                "out pointer",
                Box::new(move |svc| {
                    let f = svc.line_of_sight.unwrap_or_else(|| panic!("line_of_sight"));
                    // SAFETY: as above.
                    unsafe { f(svc.ctx, &raw const p, &raw const p, std::ptr::null_mut()) }
                }),
            ),
        ];
        for (label, needle, query) in rows {
            let (code, breach) = misuse(query);
            assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{label}");
            assert!(
                breach.as_deref().is_some_and(|b| b.contains(needle)),
                "{label}: {breach:?}"
            );
        }
    }

    /// Panics with "panic <n>", counting up from 1.
    struct Panics(std::sync::atomic::AtomicUsize);

    impl TerrainProvider for Panics {
        fn height_at(&self, _: Geodetic) -> Meters {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            panic!("panic {n}")
        }
        fn surface_normal_at(&self, _: Geodetic) -> Enu {
            Enu {
                east: 0.0,
                north: 0.0,
                up: 1.0,
            }
        }
    }

    /// ASM-722 (b): a provider panic answers `UMBRA_ERR_INTERNAL` without a breach and without
    /// writing the out-value; the record keeps the first payload, a later one is dropped.
    #[test]
    fn a_provider_panic_is_internal_and_the_record_keeps_the_first_payload() {
        let image = tokens::make(302, tokens::HOST_CTX_INDEX);
        let active = call(image, 3);
        let panics = Panics(std::sync::atomic::AtomicUsize::new(0));
        let provider: &dyn TerrainProvider = &panics;
        tokens::with_top(|r| r.terrain = Some(TerrainCall::new(&provider)));
        let svc = terrain(tokens::sink_token(image, 3, Sink::Terrain), 2);
        let height = svc.height_at.unwrap_or_else(|| panic!("height_at"));
        let p = at(4.0);
        let mut h = -1.0;

        // SAFETY: valid locals; the record is on this thread.
        let codes = unsafe {
            [
                height(svc.ctx, &raw const p, &raw mut h),
                height(svc.ctx, &raw const p, &raw mut h),
            ]
        };

        assert_eq!(codes, [UMBRA_ERR_INTERNAL; 2]);
        assert_eq!(h, -1.0, "nothing written");
        let kept = tokens::with_top(|r| {
            r.terrain = None;
            r.provider_panic.take()
        })
        .flatten()
        .unwrap_or_else(|| panic!("the payload is kept"));
        assert_eq!(
            kept.downcast_ref::<String>().map(String::as_str),
            Some("panic 1")
        );
        assert_eq!(active.finish().breach, None, "not the plugin's breach");
    }
}
