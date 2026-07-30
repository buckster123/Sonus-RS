//! The agentd-posture smoke: spawn the REAL sonus-mcp binary, speak real
//! newline-delimited JSON-RPC over its stdio, with upstream mocked at the
//! HTTP layer. initialize → tools/list → credits → generate → status →
//! download → honest not-yet. "The agent can compose."

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

struct Mock {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

fn json_mock(body: &str) -> Mock {
    Mock {
        status: 200,
        content_type: "application/json",
        body: body.as_bytes().to_vec(),
    }
}

fn audio_mock(body: &[u8]) -> Mock {
    Mock {
        status: 200,
        content_type: "audio/mpeg",
        body: body.to_vec(),
    }
}

/// Bind first so the builder can bake the host into response bodies
/// (record-info URLs must point back at the mock).
fn serve_dynamic(build: impl FnOnce(&str) -> Vec<Mock> + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let host = format!("http://{}", listener.local_addr().unwrap());
    let thread_host = host.clone();
    std::thread::spawn(move || {
        let responses = build(&thread_host);
        for mock in responses {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match sock.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..pos]);
                            let cl: usize = head
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    k.eq_ignore_ascii_case("content-length")
                                        .then(|| v.trim().parse().ok())?
                                })
                                .unwrap_or(0);
                            if buf.len() >= pos + 4 + cl {
                                break;
                            }
                        }
                    }
                }
            }
            let head = format!(
                "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                mock.status,
                mock.content_type,
                mock.body.len()
            );
            sock.write_all(head.as_bytes()).ok();
            sock.write_all(&mock.body).ok();
        }
    });
    host
}

struct Server {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Server {
    fn spawn(api_base: &str, download_dir: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_sonus-mcp"))
            .env("SUNO_API_BASE", api_base)
            .env("SUNO_API_KEY", "test-key-123")
            .env("SUNO_DOWNLOAD_DIR", download_dir)
            .env("RUST_LOG", "warn")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn sonus-mcp");
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut s = Self {
            child,
            reader,
            next_id: 1,
        };
        let init = s.request("initialize", json!({}));
        assert_eq!(init["result"]["serverInfo"]["name"], "sonus-mcp");
        s.notify("notifications/initialized");
        s
    }

    fn send(&mut self, v: &Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        let mut line = v.to_string();
        line.push('\n');
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc": "2.0", "method": method}));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }));
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read response");
        let resp: Value = serde_json::from_str(line.trim()).expect("valid JSON-RPC");
        assert_eq!(resp["id"], json!(id), "response id matches");
        resp
    }

    /// tools/call → the parsed text payload (hermes-shaped dict).
    fn call(&mut self, tool: &str, args: Value) -> Value {
        let resp = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("no text content in {resp}"));
        serde_json::from_str(text).expect("payload is JSON")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const TASK: &str = "ae2ad3f9fabcdee05de4deca2e521d9d";

#[test]
fn the_agent_can_compose_end_to_end() {
    let success_fixture = include_str!("../../sonus-core/tests/fixtures/record_info_success.json");
    let host = serve_dynamic(move |host| {
        let success = success_fixture.replace("https://tempfile.aiquickdraw.com", host);
        vec![
            json_mock(include_str!(
                "../../sonus-core/tests/fixtures/credits_number.json"
            )),
            json_mock(include_str!(
                "../../sonus-core/tests/fixtures/generate_ok.json"
            )),
            json_mock(&success), // check_status
            json_mock(&success), // download_track's own record-info
            audio_mock(b"ID3-smoke-audio-one"),
            audio_mock(b"ID3-smoke-audio-two"),
        ]
    });
    let dl_dir = std::env::temp_dir().join(format!("sonus-mcp-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dl_dir);

    let mut server = Server::spawn(&format!("{host}/api/v1"), &dl_dir.display().to_string());

    // the full hermes surface is visible
    let tools = server.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names.len(), 16);
    assert!(names.contains(&"generate_song") && names.contains(&"check_credits"));

    // the spend gate is free and first
    let credits = server.call("check_credits", json!({}));
    assert_eq!(credits["credits_remaining"], json!(437.2));

    // compose
    let song = server.call(
        "generate_song",
        json!({
            "styles": "orchestral cinematic mystical",
            "title": "Trismegistus Fanfare",
            "instrumental": true,
        }),
    );
    assert_eq!(song["task_id"], json!(TASK));
    assert_eq!(song["warnings"], json!([]));

    // poll
    let status = server.call("check_status", json!({"task_id": TASK}));
    assert_eq!(status["status"], json!("success"));
    assert_eq!(status["is_complete"], json!(true));
    assert_eq!(status["tracks"].as_array().unwrap().len(), 2);

    // land it in the library
    let dl = server.call("download_track", json!({"task_id": TASK}));
    let files = dl["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0]["path"]
        .as_str()
        .unwrap()
        .ends_with("ae2ad3f9fabc__1__Trismegistus_Fanfare.mp3"));
    for f in files {
        let path = f["path"].as_str().unwrap();
        assert!(std::path::Path::new(path).exists(), "{path} on disk");
    }

    // the extended surface answers honestly
    let stems = server.call("separate_stems", json!({"task_id": TASK}));
    assert!(stems["error"]
        .as_str()
        .unwrap()
        .contains("not implemented yet in Sonus-RS"));

    let _ = std::fs::remove_dir_all(&dl_dir);
}

#[test]
fn keyless_server_stays_up_and_answers_honestly() {
    // no HTTP mock needed — the honest no-key path never leaves the process
    let dl_dir = std::env::temp_dir().join(format!("sonus-mcp-nokey-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_sonus-mcp"))
        .env_remove("SUNO_API_KEY")
        .env("SUNO_DOWNLOAD_DIR", dl_dir.display().to_string())
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn sonus-mcp");
    let reader = BufReader::new(child.stdout.take().unwrap());
    let mut server = Server {
        child,
        reader,
        next_id: 1,
    };
    let init = server.request("initialize", json!({}));
    assert_eq!(init["result"]["serverInfo"]["name"], "sonus-mcp");

    let credits = server.call("check_credits", json!({}));
    assert!(credits["error"]
        .as_str()
        .unwrap()
        .contains("SUNO_API_KEY not configured"));
    let song = server.call("generate_song", json!({"styles": "lofi"}));
    assert!(song["error"]
        .as_str()
        .unwrap()
        .contains("SUNO_API_KEY not configured"));
}
