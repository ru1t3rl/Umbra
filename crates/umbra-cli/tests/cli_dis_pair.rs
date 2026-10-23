//! AT-3 (M6 WI-16): two real `umbra run` processes on `examples/scenarios/dis_pair/{a,b}` exchange
//! entities over DIS on `127.0.0.1`, in real time, 10 s. Requirements: M6 acceptance 1, REQ-N-6,
//! REQ-X-2, ASM-246, ASM-250, ASM-438.
//!
//! The example fixes the ports 47001 (A) and 47002 (B); the test runs a scratch copy with two free
//! ports instead (ASM-246: tests never use a fixed port). The DIS plugin is built by the nested
//! cargo helper shared with the engine tests.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(
    clippy::disallowed_methods,
    reason = "a bounded wait on real processes needs the wall clock and short sleeps"
)]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;
#[path = "../../umbra-engine/tests/port_support/mod.rs"]
mod port_support;

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A scenario lasts 10 s; a run that takes six times as long is stuck.
const DEADLINE: Duration = Duration::from_secs(60);

/// The first stderr line `info: net dis: ...` as `(key, value)` pairs.
fn net_line(stderr: &str) -> Vec<(String, u64)> {
    let line = stderr
        .lines()
        .find(|l| l.starts_with("info: net dis: "))
        .unwrap_or_else(|| panic!("no network summary in: {stderr}"));
    line.trim_start_matches("info: net dis: ")
        .split_whitespace()
        .filter_map(|kv| kv.split_once('='))
        .filter_map(|(k, v)| Some((k.to_owned(), v.parse().ok()?)))
        .collect()
}

fn counter(line: &[(String, u64)], key: &str) -> u64 {
    line.iter()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("no counter {key} in {line:?}"))
        .1
}

/// A running `umbra run` process, killed when dropped so a failing test leaves nothing behind.
struct Run {
    child: Child,
    stderr: Option<std::thread::JoinHandle<String>>,
}

impl Drop for Run {
    fn drop(&mut self) {
        // The process may already have exited.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_all(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        // A killed process closes its pipe; whatever was read is kept.
        let _ = pipe.read_to_string(&mut text);
        text
    })
}

fn spawn_run(scenario: &Path, plugins: &Path) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_umbra"))
        .arg("run")
        .arg(scenario)
        .arg("--plugin-path")
        .arg(plugins)
        .args(["--log-level", "info"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn umbra run");
    // Both pipes are drained on their own threads, so a chatty process never blocks on a full pipe.
    drop(child.stdout.take().map(read_all));
    let stderr = child.stderr.take().map(read_all);
    Run { child, stderr }
}

/// Waits for the process (bounded) and returns its exit code and stderr.
fn finish(mut run: Run) -> (Option<i32>, String) {
    let begin = Instant::now();
    let status = loop {
        if let Some(status) = run.child.try_wait().expect("try_wait") {
            break status;
        }
        assert!(
            begin.elapsed() < DEADLINE,
            "umbra run did not finish in time"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let stderr = run
        .stderr
        .take()
        .map(|t| t.join().expect("stderr reader"))
        .unwrap_or_default();
    (status.code(), stderr)
}

/// Replaces `from` by `to` in `file`, which must contain it.
fn replace_in(file: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(file).expect("read scenario");
    assert!(text.contains(from), "{} lacks {from}", file.display());
    std::fs::write(file, text.replace(from, to)).expect("write scenario");
}

#[test]
fn two_umbra_processes_exchange_entities_over_dis() {
    port_support::retry_on_port_in_use(two_processes_exchange_entities);
}

/// One attempt of the test above (ASM-1200: the ports are released before the processes bind them,
/// so a port taken in between makes the attempt fail with "Address already in use" and it is rerun).
#[expect(
    clippy::print_stderr,
    reason = "the summary lines of a run are printed for the milestone log (run with --nocapture)"
)]
fn two_processes_exchange_entities() {
    let root = plugin_support::scratch_copy(&plugin_support::dis_pair_example_dir(), "cli_pair");
    let ports = port_support::ports(2);
    for side in ["a", "b"] {
        let file = root.join(side).join("scenario.toml");
        // Both fixed ports appear in both files; swap through a marker so they do not collide.
        replace_in(&file, "127.0.0.1:47001", "127.0.0.1:@A@");
        replace_in(&file, "127.0.0.1:47002", &format!("127.0.0.1:{}", ports[1]));
        replace_in(&file, "127.0.0.1:@A@", &format!("127.0.0.1:{}", ports[0]));
    }
    let plugins = plugin_support::dis();

    let a = spawn_run(&root.join("a/scenario.toml"), &plugins);
    let b = spawn_run(&root.join("b/scenario.toml"), &plugins);
    let (a, b) = (finish(a), finish(b));

    for (who, (code, err)) in [("a", &a), ("b", &b)] {
        assert_eq!(*code, Some(0), "{who}: {err}");
        let line = net_line(err);
        // Each side owns two entities and mirrors the two of its peer.
        assert_eq!(counter(&line, "ghosts"), 2, "{who}: {err}");
        assert!(counter(&line, "received") > 0, "{who}: {err}");
        assert!(counter(&line, "published") > 0, "{who}: {err}");
        for clean in [
            "unmapped",
            "tx_dropped",
            "rx_dropped",
            "decode_errors",
            "echo_dropped",
        ] {
            assert_eq!(counter(&line, clean), 0, "{who} {clean}: {err}");
        }
        eprintln!(
            "cli {who}: {}",
            err.lines()
                .find(|l| l.starts_with("info: net dis"))
                .unwrap_or("")
        );
    }
}
