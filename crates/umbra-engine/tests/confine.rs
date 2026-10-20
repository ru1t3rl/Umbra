//! AT-19 (engine rows, D-b): confinement of served loads to the scenario root. Requirements:
//! REQ-X-8, NFR-4, ADR 0026 (amendment), ASM-162.
//!
//! Every test builds its own directory under `CARGO_TARGET_TMPDIR`: `<base>/root` is the scenario
//! root and `<base>/outside` holds files that must stay out of reach.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod plugin_support;

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use plugin_support::{Tpl, write_scenario};
use umbra_core::PluginId;
use umbra_engine::builtins::standard_registry;
use umbra_engine::{
    ConfineError, Confinement, ManualWallClock, PluginLog, ScenarioInputs, Session, SessionError,
    SessionOptions, scenario_inputs,
};

struct Sandbox {
    base: PathBuf,
}

impl Sandbox {
    fn root(&self) -> PathBuf {
        self.base.join("root")
    }
    fn outside(&self) -> PathBuf {
        self.base.join("outside")
    }
    fn scenario(&self) -> PathBuf {
        self.root().join("scen/scenario.toml")
    }
    /// A scenario file in `root/scen` with these `[scenario]` lines.
    fn write_scenario(&self, lines: &str) {
        let text = format!("schema_version = 1\n\n[scenario]\nname = \"c\"\n{lines}\n");
        fs::write(self.scenario(), text).expect("scenario");
    }
    fn confinement(&self, plugin_roots: &[PathBuf]) -> Confinement {
        Confinement::new(&self.root(), plugin_roots).expect("confinement")
    }
}

fn sandbox(name: &str) -> Sandbox {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("confine")
        .join(name);
    let _ = fs::remove_dir_all(&base);
    for dir in [
        "root/scen/templates",
        "root/scen/behaviors",
        "root/plugins",
        "outside/templates",
        "outside/plugins",
    ] {
        fs::create_dir_all(base.join(dir)).expect("dirs");
    }
    fs::write(base.join("outside/secret.toml"), "x = 1\n").expect("secret");
    fs::write(base.join("outside/templates/t.toml"), "x = 1\n").expect("outside template");
    fs::write(base.join("root/scen/templates/ok.toml"), "x = 1\n").expect("inside template");
    let sb = Sandbox { base };
    sb.write_scenario("template_paths = [\"templates\", \"behaviors\"]");
    sb
}

fn not_allowed(result: Result<PathBuf, ConfineError>) -> String {
    match result {
        Err(ConfineError::PathNotAllowed { shown }) => shown,
        other => panic!("expected PathNotAllowed, got {other:?}"),
    }
}

/// D-b: a request that leaves the root lexically is refused before the disk is consulted, so an
/// existing and a missing target outside the root cannot be told apart.
#[test]
fn dotdot_outside_root_is_path_not_allowed_without_touching_the_disk() {
    let sb = sandbox("dotdot");
    let c = sb.confinement(&[]);
    let existing = not_allowed(c.resolve_request(Path::new("../outside/secret.toml")));
    let missing = not_allowed(c.resolve_request(Path::new("../no/such/dir/x.toml")));
    let deep = not_allowed(c.resolve_request(Path::new("scen/../../outside/secret.toml")));
    let absolute = not_allowed(c.resolve_request(&sb.outside().join("secret.toml")));
    // A relative request is echoed as written, an absolute one never: the same answer whether or
    // not the target exists.
    assert_eq!(existing, "../outside/secret.toml");
    assert_eq!(missing, "../no/such/dir/x.toml");
    assert_eq!(deep, "scen/../../outside/secret.toml");
    assert_eq!(absolute, "<absolute path outside the scenario root>");
    for shown in [&existing, &missing, &deep, &absolute] {
        assert!(!shown.starts_with('/'), "never an absolute path: {shown}");
        assert!(!shown.contains(sb.base.to_str().expect("utf-8")), "{shown}");
    }
    // Dots that stay inside the root are fine; the answer is the canonical path.
    let inside = c
        .resolve_request(Path::new("scen/../scen/./scenario.toml"))
        .expect("inside");
    assert_eq!(inside, fs::canonicalize(sb.scenario()).expect("canonical"));
}

/// A missing path inside the root is not an error here: the loader reports it (422), not the
/// confinement (403). An absolute path inside the root is accepted too.
#[test]
fn missing_and_absolute_paths_inside_root_resolve() {
    let sb = sandbox("inside");
    let c = sb.confinement(&[]);
    let canonical_root = fs::canonicalize(sb.root()).expect("canonical");
    assert_eq!(
        c.resolve_request(Path::new("scen/missing.toml"))
            .expect("missing"),
        canonical_root.join("scen/missing.toml")
    );
    assert_eq!(
        c.resolve_request(&sb.scenario()).expect("absolute"),
        canonical_root.join("scen/scenario.toml")
    );
}

#[cfg(unix)]
#[test]
fn symlink_out_of_root_is_path_not_allowed() {
    let sb = sandbox("symlink");
    symlink(
        sb.outside().join("secret.toml"),
        sb.root().join("link.toml"),
    )
    .expect("file link");
    symlink(sb.outside(), sb.root().join("linkdir")).expect("dir link");
    let c = sb.confinement(&[]);
    for requested in ["link.toml", "linkdir/secret.toml", "linkdir/missing.toml"] {
        let shown = not_allowed(c.resolve_request(Path::new(requested)));
        assert!(!shown.starts_with('/'), "{shown}");
    }
    // A link that stays inside the root is fine.
    symlink(sb.scenario(), sb.root().join("inner.toml")).expect("inner link");
    assert!(c.resolve_request(Path::new("inner.toml")).is_ok());
}

#[test]
fn directory_is_not_a_file() {
    let sb = sandbox("directory");
    let c = sb.confinement(&[]);
    for requested in ["scen", "scen/templates/", "."] {
        match c.resolve_request(Path::new(requested)) {
            Err(ConfineError::NotAFile { shown }) => assert!(!shown.starts_with('/'), "{shown}"),
            other => panic!("{requested}: expected NotAFile, got {other:?}"),
        }
    }
}

fn refused(result: &Result<(), ConfineError>) {
    assert!(
        matches!(result, Err(ConfineError::PathNotAllowed { .. })),
        "expected PathNotAllowed, got {result:?}"
    );
}

#[test]
fn template_dir_outside_root_is_refused() {
    let sb = sandbox("template-dir");
    sb.write_scenario("template_paths = [\"templates\", \"../../outside/templates\"]");
    let inputs = scenario_inputs(&sb.scenario());
    assert_eq!(inputs.template_dirs.len(), 2);
    refused(&sb.confinement(&[]).check_inputs(&inputs));
    // The same inputs are fine for a root that holds both.
    let wide = Confinement::new(&sb.base, &[]).expect("wide root");
    wide.check_inputs(&inputs).expect("inside the wider root");
}

#[cfg(unix)]
#[test]
fn template_file_symlinked_out_is_refused() {
    let sb = sandbox("template-file");
    let c = sb.confinement(&[]);
    c.check_inputs(&scenario_inputs(&sb.scenario()))
        .expect("clean");
    symlink(
        sb.outside().join("secret.toml"),
        sb.root().join("scen/templates/leak.toml"),
    )
    .expect("link");
    let inputs = scenario_inputs(&sb.scenario());
    assert!(
        inputs
            .files
            .iter()
            .any(|f| f.ends_with("templates/leak.toml"))
    );
    refused(&c.check_inputs(&inputs));
}

#[cfg(unix)]
#[test]
fn graph_file_symlinked_out_is_refused() {
    let sb = sandbox("graph-file");
    let c = sb.confinement(&[]);
    symlink(
        sb.outside().join("secret.toml"),
        sb.root().join("scen/behaviors/leak.behavior.toml"),
    )
    .expect("link");
    let inputs = scenario_inputs(&sb.scenario());
    assert!(
        inputs
            .files
            .iter()
            .any(|f| f.ends_with("behaviors/leak.behavior.toml"))
    );
    refused(&c.check_inputs(&inputs));
}

#[cfg(unix)]
#[test]
fn scenario_plugin_paths_inside_root_or_plugin_roots_accepted_elsewhere_refused() {
    let sb = sandbox("plugin-paths");
    let plugin_root = sb.outside().join("plugins");
    fs::create_dir_all(plugin_root.join("sub")).expect("plugin sub");
    let canonical_plugin_root = fs::canonicalize(&plugin_root).expect("canonical");
    let c = sb.confinement(std::slice::from_ref(&canonical_plugin_root));

    sb.write_scenario("plugin_paths = [\"../plugins\"]");
    c.check_inputs(&scenario_inputs(&sb.scenario()))
        .expect("inside the root");

    sb.write_scenario("plugin_paths = [\"../../outside/plugins/sub\"]");
    let inputs = scenario_inputs(&sb.scenario());
    assert_eq!(inputs.plugin_dirs.len(), 1);
    c.check_inputs(&inputs)
        .expect("inside a configured plugin root");
    refused(&sb.confinement(&[]).check_inputs(&inputs));

    sb.write_scenario("plugin_paths = [\"../../outside/templates\"]");
    refused(&c.check_inputs(&scenario_inputs(&sb.scenario())));

    // A directory inside the root that links out is not inside the root.
    symlink(
        sb.outside().join("templates"),
        sb.root().join("plugins/sneaky"),
    )
    .expect("link");
    sb.write_scenario("plugin_paths = [\"../plugins/sneaky\"]");
    refused(&c.check_inputs(&scenario_inputs(&sb.scenario())));
}

#[cfg(unix)]
#[test]
fn display_paths_are_root_relative() {
    let sb = sandbox("display");
    let plugin_root = fs::canonicalize(sb.outside().join("plugins")).expect("canonical");
    let c = sb.confinement(std::slice::from_ref(&plugin_root));
    let root = fs::canonicalize(sb.root()).expect("canonical");

    assert_eq!(
        c.display(&root.join("scen/scenario.toml")),
        "scen/scenario.toml"
    );
    assert_eq!(c.display(&root), ".");
    let plugin = c.display(&plugin_root.join("wander/lib.so"));
    assert!(plugin.ends_with("wander/lib.so"), "{plugin}");
    assert!(!plugin.starts_with('/'), "{plugin}");
    let elsewhere = c.display(&sb.outside().join("secret.toml"));
    assert!(!elsewhere.starts_with('/'), "{elsewhere}");
    assert!(
        !elsewhere.contains(sb.base.to_str().expect("utf-8")),
        "{elsewhere}"
    );

    // The messages of refusals carry display forms only.
    symlink(
        sb.outside().join("secret.toml"),
        sb.root().join("scen/templates/leak.toml"),
    )
    .expect("link");
    let error = c
        .check_inputs(&scenario_inputs(&sb.scenario()))
        .expect_err("refused");
    let text = error.to_string();
    assert!(text.contains("scen/templates/leak.toml"), "{text}");
    assert!(!text.contains(sb.base.to_str().expect("utf-8")), "{text}");
}

/// `scenario_inputs` is a lenient pre-read: files that are not scenarios or do not exist have no
/// inputs, and nothing panics.
#[test]
fn scenario_inputs_is_lenient() {
    let sb = sandbox("lenient");
    let missing = scenario_inputs(&sb.root().join("nope.toml"));
    assert!(
        missing.template_dirs.is_empty()
            && missing.files.is_empty()
            && missing.plugin_dirs.is_empty()
    );
    let plain = sb.root().join("plain.toml");
    fs::write(&plain, "not = \"a scenario\"\n").expect("write");
    assert_eq!(scenario_inputs(&plain).template_dirs, Vec::<PathBuf>::new());
    fs::write(&plain, "this is [ not toml").expect("write");
    assert_eq!(scenario_inputs(&plain).template_dirs, Vec::<PathBuf>::new());

    sb.write_scenario("template_paths = [\"templates\"]\nplugin_paths = [\"../plugins\"]");
    let inputs: ScenarioInputs = scenario_inputs(&sb.scenario());
    assert_eq!(inputs.scenario, sb.scenario());
    assert_eq!(inputs.template_dirs, vec![sb.root().join("scen/templates")]);
    assert_eq!(inputs.files, vec![sb.root().join("scen/templates/ok.toml")]);
    assert_eq!(inputs.plugin_dirs, vec![sb.root().join("scen/../plugins")]);
}

/// `Session::load` runs the check before reading anything when a confinement is set.
#[test]
fn session_load_refuses_inputs_outside_the_root() {
    let sb = sandbox("session");
    sb.write_scenario("template_paths = [\"../../outside/templates\"]");
    let registry = standard_registry().expect("registry");
    let mut options = SessionOptions::new(Box::new(ManualWallClock::new()));
    options.load.confinement = Some(sb.confinement(&[]));
    match Session::load(&registry, &sb.scenario(), options) {
        Err(SessionError::Confine(ConfineError::PathNotAllowed { .. })) => {}
        Err(other) => panic!("expected a confinement error, got {other}"),
        Ok(_) => panic!("loaded despite the confinement"),
    }
    // Without a confinement the load gets as far as reading the file (and reports its problems).
    let options = SessionOptions::new(Box::new(ManualWallClock::new()));
    if let Err(SessionError::Confine(_)) = Session::load(&registry, &sb.scenario(), options) {
        panic!("no confinement was set");
    }
}

/// Lexical normalisation comes first: `deep/../../x.toml` leaves the root as written, although on
/// disk `deep` is a link to `a/b`, so `deep/../..` is the root and a canonical-only check would
/// accept the file.
#[cfg(unix)]
#[test]
fn lexical_check_comes_before_links_are_followed() {
    let sb = sandbox("lexical-first");
    fs::create_dir_all(sb.root().join("a/b")).expect("dirs");
    fs::write(sb.root().join("x.toml"), "x = 1\n").expect("x");
    fs::write(sb.base.join("x.toml"), "x = 1\n").expect("outside x");
    symlink(sb.root().join("a/b"), sb.root().join("deep")).expect("link");
    let c = sb.confinement(&[]);
    // The physical reading, for contrast: inside the root.
    assert!(
        fs::canonicalize(sb.root().join("deep/../../x.toml"))
            .expect("exists")
            .starts_with(fs::canonicalize(sb.root()).expect("root"))
    );
    not_allowed(c.resolve_request(Path::new("deep/../../x.toml")));
}

#[cfg(unix)]
#[test]
fn dangling_link_is_refused() {
    let sb = sandbox("dangling");
    symlink(
        sb.outside().join("not-there.toml"),
        sb.root().join("dangling.toml"),
    )
    .expect("link");
    symlink(
        sb.root().join("not-there.toml"),
        sb.root().join("inner-dangling.toml"),
    )
    .expect("link");
    let c = sb.confinement(&[]);
    for requested in ["dangling.toml", "inner-dangling.toml"] {
        not_allowed(c.resolve_request(Path::new(requested)));
    }
    sb.write_scenario("template_paths = [\"templates\", \"gone\"]");
    symlink(sb.outside().join("not-there"), sb.root().join("scen/gone")).expect("link");
    refused(&c.check_inputs(&scenario_inputs(&sb.scenario())));
}

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl PluginLog for Recorder {
    fn log(&self, _: &PluginId, _: umbra_plugin_host::LogLevel, message: &str) {
        self.0.lock().expect("log lock").push(message.to_owned());
    }
}

impl Recorder {
    fn messages(&self) -> Vec<String> {
        self.0.lock().expect("log lock").clone()
    }
}

fn lifecycle_scenario(name: &str, plugin_dir: &Path) -> PathBuf {
    write_scenario(
        name,
        1.0,
        &format!("plugin_paths = [{:?}]", plugin_dir.to_str().expect("utf-8")),
        &[Tpl {
            id: "test.entity.life",
            behavior: "test.behavior.lifecycle",
            params: "",
        }],
        &[],
        &[("test.entity.life", "a")],
    )
}

fn confined_options(c: Confinement, log: &Arc<Recorder>) -> SessionOptions {
    let mut options = SessionOptions::new(Box::new(ManualWallClock::new()));
    options.load.confinement = Some(c);
    let log: Arc<dyn PluginLog> = log.clone();
    options.plugin_log = Some(log);
    options
}

/// A directory path with a confinement is refused as a non-file, and the `plugin_paths` of the
/// scenarios inside it (outside the root) are never loaded: the plugin would log at least its
/// `shutdown`.
#[test]
fn session_load_of_a_directory_with_confinement_loads_no_plugin() {
    let plugins = plugin_support::fixture(&[]);
    let scenario = lifecycle_scenario("confined-dir", &plugins);
    let dir = scenario.parent().expect("scenario dir").to_path_buf();
    let root = dir.parent().expect("parent").join("empty-root-for-dir");
    fs::create_dir_all(&root).expect("root");
    // The directory is inside the root; the plugin directory it names is not.
    let inside = root.join("scen");
    plugin_support::copy_dir(&dir, &inside);
    let log = Arc::new(Recorder::default());
    let c = Confinement::new(&root, &[]).expect("confinement");
    let registry = standard_registry().expect("registry");
    match Session::load(&registry, &inside, confined_options(c, &log)) {
        Err(SessionError::Confine(ConfineError::NotAFile { shown })) => assert_eq!(shown, "scen"),
        Err(other) => panic!("expected NotAFile, got {other}"),
        Ok(_) => panic!("a directory loaded"),
    }
    assert_eq!(log.messages(), Vec::<String>::new(), "a plugin was loaded");
}

/// With a confinement the plugin directories are the checked file's: a plugin directory inside a
/// configured plugin root loads and its behavior is used; one outside every root is refused before
/// anything loads.
#[test]
fn confined_load_takes_plugin_dirs_from_the_checked_inputs() {
    let plugins = plugin_support::fixture(&[]);
    let scenario = lifecycle_scenario("confined-file", &plugins);
    let root = scenario.parent().expect("dir");
    let registry = standard_registry().expect("registry");

    let log = Arc::new(Recorder::default());
    let c = Confinement::new(root, std::slice::from_ref(&plugins)).expect("confinement");
    let session = Session::load(&registry, &scenario, confined_options(c, &log))
        .unwrap_or_else(|e| panic!("load: {e}"));
    assert!(log.messages().contains(&"create_behavior".to_owned()));
    drop(session);

    let log = Arc::new(Recorder::default());
    let c = Confinement::new(root, &[]).expect("confinement");
    match Session::load(&registry, &scenario, confined_options(c, &log)) {
        Err(SessionError::Confine(ConfineError::PathNotAllowed { .. })) => {}
        Err(other) => panic!("expected PathNotAllowed, got {other}"),
        Ok(_) => panic!("an unlisted plugin directory loaded"),
    }
    assert_eq!(log.messages(), Vec::<String>::new());
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// `Session::load` with a confinement resolves a relative path against the scenario root (not the
/// working directory) and reports the resolved path; a path outside the root is refused.
#[test]
fn session_load_resolves_a_relative_path_against_the_root() {
    let sb = sandbox("session-relative");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/walking_skeleton"),
        &sb.root().join("ws"),
    );
    let registry = standard_registry().expect("registry");
    let confinement = sb.confinement(&[]);
    let mut options = SessionOptions::new(Box::new(ManualWallClock::new()));
    options.load.confinement = Some(confinement.clone());
    let session = match Session::load(&registry, Path::new("./ws/../ws/scenario.toml"), options) {
        Ok(session) => session,
        Err(error) => panic!("a relative path inside the root must load: {error}"),
    };
    assert_eq!(session.path(), confinement.root.join("ws/scenario.toml"));

    let mut options = SessionOptions::new(Box::new(ManualWallClock::new()));
    options.load.confinement = Some(confinement);
    match Session::load(&registry, Path::new("../outside/secret.toml"), options) {
        Err(SessionError::Confine(ConfineError::PathNotAllowed { .. })) => {}
        Err(other) => panic!("expected path_not_allowed, got {other}"),
        Ok(_) => panic!("loaded outside the root"),
    }
}
