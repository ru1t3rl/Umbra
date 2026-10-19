//! Forged plugin descriptors for `PluginHost::load_static` (AT-4 descriptor rows).
//!
//! A [`Parts`] is a valid one-capability plugin whose raw structs a test edits before publishing it.
//! `forged!` publishes the parts behind a fresh entry function: image identity is the entry address, so
//! every test needs its own function, and the macro's statics are per expansion site.
//!
//! **One call site per test.** Never call `forged!` from a helper function that several tests share: the
//! helper is one expansion, so all its callers would publish into the same statics and the same image
//! key, and parallel tests would overwrite each other's descriptors, reuse each other's live images and
//! share counters. Use a macro that expands `forged!` inside the test function (`load!` in
//! `descriptor.rs`, `host_with!` in `register.rs`, `raw_env!` in `support/bridge.rs`), and one `forged!`
//! per plugin when a test loads several.

use std::ffi::c_void;
use std::mem::size_of;

use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_CAPABILITY_BEHAVIOR, UMBRA_OK, UmbraBehavior, UmbraBehaviorBatch,
    UmbraBehaviorVTable, UmbraCapability, UmbraCode, UmbraErrorSink, UmbraInstance,
    UmbraPluginDescriptor, UmbraStr,
};

pub fn leak_str(s: &str) -> UmbraStr {
    let s: &'static str = Box::leak(s.to_owned().into_boxed_str());
    UmbraStr {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}

/// A string whose bytes are not UTF-8.
pub fn bad_utf8() -> UmbraStr {
    let b: &'static [u8] = Box::leak(vec![0xFF_u8, 0xFE, 0xFD].into_boxed_slice());
    UmbraStr {
        ptr: b.as_ptr(),
        len: b.len(),
    }
}

unsafe extern "C" fn configure(
    _: UmbraStr,
    out: *mut *mut UmbraBehavior,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the host passes a valid out pointer.
    unsafe { *out = 8 as *mut UmbraBehavior };
    UMBRA_OK
}
unsafe extern "C" fn destroy_behavior(_: *mut UmbraBehavior) {}
unsafe extern "C" fn instantiate(
    _: *mut UmbraBehavior,
    _: u32,
    out: *mut *mut UmbraInstance,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the host passes a valid out pointer.
    unsafe { *out = 16 as *mut UmbraInstance };
    UMBRA_OK
}
unsafe extern "C" fn destroy_instance(_: *mut UmbraInstance) {}
unsafe extern "C" fn tick_batch(
    _: *const UmbraBehaviorBatch,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_OK
}
unsafe extern "C" fn halt(_: *mut UmbraInstance) -> UmbraCode {
    UMBRA_OK
}

pub unsafe extern "C" fn dummy_set_input(
    _: *mut UmbraInstance,
    _: UmbraStr,
    _: *const umbra_plugin_api::UmbraValue,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_OK
}
pub unsafe extern "C" fn dummy_output(
    _: *mut UmbraInstance,
    _: UmbraStr,
    _: *const umbra_plugin_api::UmbraValueSink,
) -> UmbraCode {
    UMBRA_OK
}

pub fn valid_vtable() -> UmbraBehaviorVTable {
    UmbraBehaviorVTable {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraBehaviorVTable>() as u32,
        configure: Some(configure),
        destroy_behavior: Some(destroy_behavior),
        instantiate: Some(instantiate),
        destroy_instance: Some(destroy_instance),
        tick_batch: Some(tick_batch),
        halt: Some(halt),
        halt_running: None,
        set_input: None,
        output: None,
    }
}

pub fn capability(id: &str) -> UmbraCapability {
    UmbraCapability {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraCapability>() as u32,
        kind: UMBRA_CAPABILITY_BEHAVIOR,
        reserved_0: 0,
        id: leak_str(id),
        version: leak_str(""),
        param_schema: leak_str(r#"{"type":"object"}"#),
        metadata: leak_str(""),
        vtable: std::ptr::null(),
    }
}

/// The raw pieces of a forged plugin; edit fields, then publish with `forged!`.
pub struct Parts {
    pub desc: UmbraPluginDescriptor,
    pub caps: Vec<UmbraCapability>,
    pub vtable: Box<UmbraBehaviorVTable>,
}

/// A valid plugin `test.plugin.forge` 0.1.0 with the capability `test.behavior.forge`.
pub fn parts() -> Parts {
    let mut p = Parts {
        desc: UmbraPluginDescriptor {
            abi_version: UMBRA_ABI_VERSION,
            struct_size: size_of::<UmbraPluginDescriptor>() as u32,
            id: leak_str("test.plugin.forge"),
            name: leak_str("Forge"),
            version: leak_str("0.1.0"),
            capabilities: std::ptr::null(),
            capability_count: 0,
            init: None,
            shutdown: None,
        },
        caps: vec![capability("test.behavior.forge")],
        vtable: Box::new(valid_vtable()),
    };
    p.link();
    p
}

impl Parts {
    /// Points the capabilities at the vtable and the descriptor at the capabilities.
    pub fn link(&mut self) {
        let vt: *const c_void = (&raw const *self.vtable).cast();
        for c in &mut self.caps {
            c.vtable = vt;
        }
        self.desc.capabilities = self.caps.as_ptr();
        self.desc.capability_count = self.caps.len();
    }

    /// Replaces capability `index` by a fresh valid one with `id` and re-links vtable pointers.
    pub fn set_capability(&mut self, index: usize, id: &str) {
        self.caps[index] = capability(id);
        self.link();
    }

    /// Adds a capability with the given id.
    pub fn add_capability(&mut self, id: &str) {
        self.caps.push(capability(id));
        self.link();
    }

    /// Leaks everything (the host may keep pointers for the process lifetime) and returns the
    /// descriptor pointer.
    pub fn publish(self) -> *const UmbraPluginDescriptor {
        let Parts { desc, caps, vtable } = self;
        Box::leak(vtable);
        Box::leak(caps.into_boxed_slice());
        Box::leak(Box::new(desc))
    }
}

/// An array of `UmbraCapability` starting 4 bytes off 8-byte alignment (rows: misaligned array).
pub fn misaligned_caps(p: &Parts) -> *const UmbraCapability {
    let words = p.caps.len() * size_of::<UmbraCapability>() / 8 + 2;
    let buf: &'static mut [u64] = Box::leak(vec![0_u64; words].into_boxed_slice());
    // SAFETY: stays inside the allocation (one extra word); the host must reject it before reading.
    unsafe { buf.as_mut_ptr().cast::<u8>().add(4) }.cast()
}

/// Calls the host `log` entry (INFO) the way a plugin does.
///
/// Returns -1 when the host API has no `log` entry (the tests assert the code, so a missing entry is a
/// test failure outside the callback; a panic here would abort the process).
pub fn log_through(host: *const umbra_plugin_api::UmbraHostApi, message: &str) -> i32 {
    // SAFETY: the host API is a process-lifetime struct.
    let h = unsafe { &*host };
    let Some(log) = h.log else {
        return -1;
    };
    // SAFETY: the `log` entry is set by the host; the string is leaked and so valid.
    unsafe {
        log(
            h.host_ctx,
            umbra_plugin_api::UMBRA_LOG_INFO,
            leak_str(message),
        )
    }
}

/// A gate that makes `shutdown` block until the test opens it (the `load_waits_while_image_is_closing`
/// row). Not gated by default.
pub struct Gate {
    pub state: std::sync::Mutex<(bool, bool, bool)>, // (gated, in_shutdown, open)
    pub cv: std::sync::Condvar,
}

impl Gate {
    pub const fn new() -> Self {
        Self {
            state: std::sync::Mutex::new((false, false, false)),
            cv: std::sync::Condvar::new(),
        }
    }
    /// Makes the next `shutdown` block.
    pub fn arm(&self) {
        *self.state.lock().expect("gate") = (true, false, false);
    }
    /// Blocks until a `shutdown` is inside the gate.
    pub fn wait_in_shutdown(&self) {
        let mut g = self.state.lock().expect("gate");
        while !g.1 {
            g = self.cv.wait(g).expect("gate");
        }
    }
    /// Lets the blocked `shutdown` finish.
    pub fn open(&self) {
        self.state.lock().expect("gate").2 = true;
        self.cv.notify_all();
    }
    /// Called by the plugin's `shutdown`.
    pub fn pass(&self) {
        let relax = std::sync::PoisonError::into_inner;
        let mut g = self.state.lock().unwrap_or_else(relax);
        if g.0 {
            g.1 = true;
            self.cv.notify_all();
            while !g.2 {
                g = self.cv.wait(g).unwrap_or_else(relax);
            }
            g.0 = false;
        }
    }
}

/// What `forged!` returns.
pub struct Forged {
    pub entry: umbra_plugin_api::UmbraPluginEntry,
    pub entry_calls: &'static std::sync::atomic::AtomicUsize,
    pub init_calls: &'static std::sync::atomic::AtomicUsize,
    pub shutdown_calls: &'static std::sync::atomic::AtomicUsize,
    pub init_code: &'static std::sync::atomic::AtomicI32,
    /// Address of the last `UmbraHostApi*` the entry received (0 before the first call).
    pub host: &'static std::sync::atomic::AtomicUsize,
    /// `"init"` / `"shutdown"` in the order they ran.
    pub order: &'static std::sync::Mutex<Vec<&'static str>>,
    pub gate: &'static Gate,
    /// Code returned by the `log` host call made inside `init` / `shutdown` (-1 before they ran).
    pub init_log_code: &'static std::sync::atomic::AtomicI32,
    pub shutdown_log_code: &'static std::sync::atomic::AtomicI32,
}

/// Publishes `$parts` behind a fresh entry function with counting `init`/`shutdown`.
macro_rules! forged {
    ($parts:expr $(, alt = $alt:expr)?) => {{
        use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
        static SLOT: AtomicUsize = AtomicUsize::new(0);
        static SLOT2: AtomicUsize = AtomicUsize::new(0);
        static ENTRY: AtomicUsize = AtomicUsize::new(0);
        static INIT: AtomicUsize = AtomicUsize::new(0);
        static SHUT: AtomicUsize = AtomicUsize::new(0);
        static HOST: AtomicUsize = AtomicUsize::new(0);
        static INIT_CODE: AtomicI32 = AtomicI32::new(0);
        static INIT_LOG: AtomicI32 = AtomicI32::new(-1);
        static SHUT_LOG: AtomicI32 = AtomicI32::new(-1);
        static ORDER: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
        static GATE: $crate::support::forge::Gate = $crate::support::forge::Gate::new();
        unsafe extern "C" fn entry(
            host: *const umbra_plugin_api::UmbraHostApi,
        ) -> *const umbra_plugin_api::UmbraPluginDescriptor {
            HOST.store(host as usize, Ordering::SeqCst);
            let n = ENTRY.fetch_add(1, Ordering::SeqCst);
            let second = SLOT2.load(Ordering::SeqCst);
            if n >= 1 && second != 0 && n % 2 == 1 {
                return second as *const _;
            }
            SLOT.load(Ordering::SeqCst) as *const _
        }
        unsafe extern "C" fn init(
            host: *const umbra_plugin_api::UmbraHostApi,
            _: *const umbra_plugin_api::UmbraErrorSink,
        ) -> umbra_plugin_api::UmbraCode {
            INIT.fetch_add(1, Ordering::SeqCst);
            INIT_LOG.store($crate::support::forge::log_through(host, "init says hi"), Ordering::SeqCst);
            ORDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push("init");
            INIT_CODE.load(Ordering::SeqCst)
        }
        unsafe extern "C" fn shutdown() {
            SHUT.fetch_add(1, Ordering::SeqCst);
            SHUT_LOG.store(
                $crate::support::forge::log_through(HOST.load(Ordering::SeqCst) as *const _, "shutdown says bye"),
                Ordering::SeqCst,
            );
            ORDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push("shutdown");
            GATE.pass();
        }
        let mut parts: $crate::support::forge::Parts = $parts;
        parts.desc.init = Some(init);
        parts.desc.shutdown = Some(shutdown);
        let alt: Option<$crate::support::forge::Parts> = None $(.or(Some($alt)))?;
        if let Some(alt) = alt {
            SLOT2.store(alt.publish() as usize, Ordering::SeqCst);
        }
        SLOT.store(parts.publish() as usize, Ordering::SeqCst);
        $crate::support::forge::Forged {
            entry,
            entry_calls: &ENTRY,
            init_calls: &INIT,
            shutdown_calls: &SHUT,
            init_code: &INIT_CODE,
            host: &HOST,
            order: &ORDER,
            gate: &GATE,
            init_log_code: &INIT_LOG,
            shutdown_log_code: &SHUT_LOG,
        }
    }};
}

/// A fresh entry function that returns null.
macro_rules! forged_null {
    () => {{
        unsafe extern "C" fn entry(
            _: *const umbra_plugin_api::UmbraHostApi,
        ) -> *const umbra_plugin_api::UmbraPluginDescriptor {
            std::ptr::null()
        }
        entry as umbra_plugin_api::UmbraPluginEntry
    }};
}
