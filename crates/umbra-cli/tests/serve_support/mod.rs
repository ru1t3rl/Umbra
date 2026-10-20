//! Helpers of the `umbra serve` process tests (M5 step 4, ASM-184): start the built `umbra` binary
//! in a working directory under `CARGO_TARGET_TMPDIR`, read its `listening on http://<addr>` line,
//! and talk to it with a plain `std::net` HTTP/1.1 client and a minimal WebSocket client (text
//! frames only), so the CLI crate needs no new dev-dependency. Blocking waits are bounded by a hang
//! guard (a failing test, never an assertion about time).
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "helpers build known-valid values")]

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

const HANG_GUARD: Duration = Duration::from_secs(60);

/// The repository root.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh working directory `CARGO_TARGET_TMPDIR/cli-serve/<tag>`.
pub fn workdir(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cli-serve")
        .join(tag);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Copies `src` (a directory) to `dst`.
pub fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("mkdir");
    for entry in fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let to = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).expect("copy");
        }
    }
}

/// Copies `examples/scenarios/<name>` to `<dir>/<name>`.
pub fn copy_example(dir: &Path, name: &str) -> PathBuf {
    let to = dir.join(name);
    copy_dir(&repo_root().join("examples/scenarios").join(name), &to);
    to
}

/// Copies `walking_skeleton` to `<dir>/long` with a run length of 100000 s (3 million ticks).
pub fn write_long_scenario(dir: &Path) -> &'static str {
    let long = dir.join("long");
    copy_dir(
        &repo_root().join("examples/scenarios/walking_skeleton"),
        &long,
    );
    let file = long.join("scenario.toml");
    let text = fs::read_to_string(&file).expect("read");
    assert!(text.contains("duration_s = 240.0"));
    fs::write(
        &file,
        text.replace("duration_s = 240.0", "duration_s = 100000.0"),
    )
    .expect("write");
    "long/scenario.toml"
}

/// Runs `umbra <args>` in `dir`.
pub fn command(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_umbra"));
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Kills a process by id (the hang guard).
fn kill(pid: u32, signal: &str) {
    let _ = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .status();
}

/// The outcome of a process that was expected to end by itself.
#[derive(Debug)]
pub struct Exited {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `umbra <args>` in `dir` until it exits (killed after the hang guard).
pub fn run_to_exit(dir: &Path, args: &[&str]) -> Exited {
    let child = command(dir, args).spawn().expect("spawn umbra");
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = rx.recv_timeout(HANG_GUARD).unwrap_or_else(|_| {
        kill(pid, "KILL");
        panic!("umbra {args:?} did not exit within the hang guard");
    });
    let output = output.expect("wait");
    Exited {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A running `umbra serve`.
pub struct Served {
    child: Child,
    pub addr: SocketAddr,
    /// The first stdout line.
    pub listening: String,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: Arc<Mutex<String>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl Served {
    /// Starts `umbra serve --bind 127.0.0.1:0 <args>` in `dir`.
    pub fn start(dir: &Path, args: &[&str]) -> Self {
        let mut all = vec!["serve", "--bind", "127.0.0.1:0"];
        all.extend_from_slice(args);
        Self::start_raw(dir, &all)
    }

    /// Starts `umbra <args>` (which must be a `serve` command that listens) in `dir`.
    pub fn start_raw(dir: &Path, args: &[&str]) -> Self {
        let mut child = command(dir, args).spawn().expect("spawn umbra");
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        let mut err = child.stderr.take().expect("stderr");
        let stderr_thread = std::thread::spawn(move || {
            let mut text = String::new();
            let mut chunk = [0_u8; 4096];
            while let Ok(n) = err.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                text.push_str(&String::from_utf8_lossy(&chunk[..n]));
                sink.lock().expect("lock").clone_from(&text);
            }
        });
        let pid = child.id();
        let (tx, rx) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            if rx.recv_timeout(HANG_GUARD).is_err() {
                kill(pid, "KILL");
            }
        });
        let mut line = String::new();
        let read = stdout.read_line(&mut line).expect("read the first line");
        let _ = tx.send(());
        assert!(
            read > 0,
            "umbra exited before listening; stderr: {}",
            stderr.lock().expect("lock")
        );
        let listening = line.trim_end().to_string();
        let addr: SocketAddr = listening
            .strip_prefix("listening on http://")
            .unwrap_or_else(|| panic!("unexpected first line {listening:?}"))
            .parse()
            .unwrap_or_else(|e| panic!("bad address in {listening:?}: {e}"));
        Self {
            child,
            addr,
            listening,
            stdout,
            stderr,
            stderr_thread: Some(stderr_thread),
        }
    }

    /// Sends SIGTERM and waits for the exit; returns the exit code. (On other platforms: kills.)
    pub fn terminate(&mut self) -> Option<i32> {
        #[cfg(unix)]
        kill(self.child.id(), "TERM");
        #[cfg(not(unix))]
        let _ = self.child.kill();
        self.wait()
    }

    /// Waits for the process to exit by itself (killed after the hang guard).
    pub fn wait(&mut self) -> Option<i32> {
        let pid = self.child.id();
        let (tx, rx) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            if rx.recv_timeout(HANG_GUARD).is_err() {
                kill(pid, "KILL");
            }
        });
        let status = self.child.wait().expect("wait");
        let _ = tx.send(());
        status.code()
    }

    /// The rest of stdout after the listening line (call after the process has exited).
    pub fn rest_of_stdout(&mut self) -> String {
        let mut rest = String::new();
        let _ = self.stdout.read_to_string(&mut rest);
        rest
    }

    /// Everything written to stderr so far (call after the process has exited to get it all).
    pub fn stderr_text(&mut self) -> String {
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
        self.stderr.lock().expect("lock").clone()
    }

    /// One HTTP request with an optional JSON body.
    pub fn request(&self, method: &str, path: &str, body: Option<&Value>) -> HttpReply {
        http_request(
            self.addr,
            method,
            path,
            &[],
            body.map(Value::to_string).as_deref(),
        )
    }

    /// One HTTP request with a raw body and extra headers.
    pub fn request_raw(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> HttpReply {
        http_request(self.addr, method, path, headers, body)
    }

    pub fn get(&self, path: &str) -> HttpReply {
        self.request("GET", path, None)
    }

    pub fn post(&self, path: &str, body: &Value) -> HttpReply {
        self.request("POST", path, Some(body))
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A parsed HTTP response.
#[derive(Debug, Clone)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpReply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("status {}: not JSON ({e}): {}", self.status, self.text()))
    }

    /// Asserts the status and returns the JSON body.
    pub fn assert_json(&self, status: u16) -> Value {
        assert_eq!(self.status, status, "{}", self.text());
        self.json()
    }

    /// Asserts a problem response with this status and `code`.
    pub fn assert_problem(&self, status: u16, code: &str) {
        assert_eq!(self.status, status, "{}", self.text());
        assert!(
            self.header("content-type")
                .is_some_and(|t| t.starts_with("application/problem+json")),
            "content type {:?}",
            self.header("content-type")
        );
        assert_eq!(self.json()["code"], code, "{}", self.text());
    }
}

fn set_guard(stream: &TcpStream) {
    stream.set_read_timeout(Some(HANG_GUARD)).expect("timeout");
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Decodes a `Transfer-Encoding: chunked` body.
fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(end) = find(raw, b"\r\n") else {
            return out;
        };
        let size = usize::from_str_radix(
            String::from_utf8_lossy(&raw[..end])
                .split(';')
                .next()
                .unwrap_or("0")
                .trim(),
            16,
        )
        .unwrap_or(0);
        raw = &raw[end + 2..];
        if size == 0 || raw.len() < size {
            return out;
        }
        out.extend_from_slice(&raw[..size]);
        raw = raw.get(size + 2..).unwrap_or(&[]);
    }
}

fn parse_head(head: &str) -> (u16, Vec<(String, String)>) {
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("bad status line in {head:?}"));
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    (status, headers)
}

/// One HTTP/1.1 request with `Connection: close`; a body gets `Content-Type: application/json`.
pub fn http_request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> HttpReply {
    let mut stream = TcpStream::connect(addr).expect("connect");
    set_guard(&stream);
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    for (key, value) in headers {
        let _ = write!(request, "{key}: {value}\r\n");
    }
    if let Some(text) = body {
        let _ = write!(
            request,
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            text.len()
        );
    }
    request.push_str("\r\n");
    if let Some(text) = body {
        request.push_str(text);
    }
    stream.write_all(request.as_bytes()).expect("write");
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let split = find(&raw, b"\r\n\r\n").unwrap_or_else(|| panic!("no response head: {raw:?}"));
    let (status, headers) = parse_head(&String::from_utf8_lossy(&raw[..split]));
    let rest = &raw[split + 4..];
    let chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    HttpReply {
        status,
        headers,
        body: if chunked {
            dechunk(rest)
        } else {
            rest.to_vec()
        },
    }
}

/// What a WebSocket client received.
#[derive(Debug)]
pub enum WsIn {
    Json(Value),
    Close(u16),
    Eof,
}

/// A minimal WebSocket client: text frames only, no fragmentation of its own messages.
#[derive(Debug)]
pub struct Ws {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Ws {
    /// Upgrades `/api/v1/stream`; a refusal is returned as the HTTP reply.
    pub fn connect(addr: SocketAddr, headers: &[(&str, &str)]) -> Result<Self, HttpReply> {
        let mut stream = TcpStream::connect(addr).expect("connect");
        set_guard(&stream);
        let mut request = format!(
            "GET /api/v1/stream HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
        );
        for (key, value) in headers {
            let _ = write!(request, "{key}: {value}\r\n");
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).expect("write");
        let mut buf = Vec::new();
        let split = loop {
            if let Some(i) = find(&buf, b"\r\n\r\n") {
                break i;
            }
            let mut chunk = [0_u8; 4096];
            let n = stream.read(&mut chunk).expect("read the upgrade response");
            assert!(n > 0, "connection closed during the upgrade");
            buf.extend_from_slice(&chunk[..n]);
        };
        let (status, headers) = parse_head(&String::from_utf8_lossy(&buf[..split]));
        let mut rest = buf[split + 4..].to_vec();
        if status != 101 {
            let length: usize = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            while rest.len() < length {
                let mut chunk = [0_u8; 4096];
                let n = stream.read(&mut chunk).expect("read");
                if n == 0 {
                    break;
                }
                rest.extend_from_slice(&chunk[..n]);
            }
            return Err(HttpReply {
                status,
                headers,
                body: rest,
            });
        }
        Ok(Self { stream, buf: rest })
    }

    pub fn send_json(&mut self, message: &Value) {
        let payload = message.to_string().into_bytes();
        let mask = [0x11_u8, 0x22, 0x33, 0x44];
        let mut frame = vec![0x81_u8];
        let len = payload.len();
        if len < 126 {
            frame.push(0x80 | u8::try_from(len).expect("short"));
        } else if let Ok(short) = u16::try_from(len) {
            frame.push(0x80 | 0x7e);
            frame.extend_from_slice(&short.to_be_bytes());
        } else {
            frame.push(0x80 | 0x7f);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&frame).expect("write frame");
    }

    /// Sends `{"type":"subscribe", ..fields}`.
    pub fn subscribe(&mut self, mut fields: Value) {
        fields["type"] = Value::from("subscribe");
        self.send_json(&fields);
    }

    fn fill(&mut self, wanted: usize) -> bool {
        while self.buf.len() < wanted {
            let mut chunk = [0_u8; 16384];
            match self.stream.read(&mut chunk) {
                Ok(0) | Err(_) => return false,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
            }
        }
        true
    }

    /// The next JSON message, close or end of connection.
    pub fn recv(&mut self) -> WsIn {
        let mut text = Vec::new();
        loop {
            if !self.fill(2) {
                return WsIn::Eof;
            }
            let fin = self.buf[0] & 0x80 != 0;
            let opcode = self.buf[0] & 0x0f;
            let (len, head) = match self.buf[1] & 0x7f {
                126 => {
                    if !self.fill(4) {
                        return WsIn::Eof;
                    }
                    (
                        usize::from(u16::from_be_bytes([self.buf[2], self.buf[3]])),
                        4,
                    )
                }
                127 => {
                    if !self.fill(10) {
                        return WsIn::Eof;
                    }
                    let mut eight = [0_u8; 8];
                    eight.copy_from_slice(&self.buf[2..10]);
                    (
                        usize::try_from(u64::from_be_bytes(eight)).expect("length"),
                        10,
                    )
                }
                n => (usize::from(n), 2),
            };
            if !self.fill(head + len) {
                return WsIn::Eof;
            }
            let payload: Vec<u8> = self.buf[head..head + len].to_vec();
            self.buf.drain(..head + len);
            match opcode {
                0x1 | 0x0 => {
                    text.extend_from_slice(&payload);
                    if fin {
                        return WsIn::Json(
                            serde_json::from_slice(&text).expect("server text is JSON"),
                        );
                    }
                }
                0x8 => {
                    let code = if payload.len() >= 2 {
                        u16::from_be_bytes([payload[0], payload[1]])
                    } else {
                        1005
                    };
                    return WsIn::Close(code);
                }
                _ => {}
            }
        }
    }

    pub fn next_json(&mut self) -> Value {
        match self.recv() {
            WsIn::Json(value) => value,
            other => panic!("expected a JSON message, got {other:?}"),
        }
    }

    pub fn hello(&mut self) -> Value {
        let hello = self.next_json();
        assert_eq!(hello["type"], "hello", "{hello}");
        hello
    }

    /// Reads messages until a `frame` accepted by `accept`.
    pub fn frame_until(&mut self, accept: impl Fn(&Value) -> bool) -> Value {
        loop {
            let message = self.next_json();
            if message["type"] == "frame" && accept(&message) {
                return message;
            }
        }
    }
}
